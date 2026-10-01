#!/usr/bin/env python3
"""dsh-relay 部署工具（paramiko SSH）：本地打 Linux 包 → 上传 → systemd 重启。

流程：本地 cargo-zigbuild 交叉编译 Linux 二进制（dsh-relay-server）→ 打 tar 包
（deploy/dist/）→ SSH 上传解到远端 <REMOTE_DIR>/bin/ → 重启 systemd → 健康检查。
服务器只需要 glibc（Ubuntu 自带），不需要 Rust / 源码。**免反代**：WS :8787 与
管理台 :8788 直连暴露（config relay.toml 已置 trust_proxy=false，注意安全组放行）。

日常更新一条命令（二进制无变化自动跳过上传重启）：

    python deploy.py                # 一条龙：编译 + 打包 + 上传 + 重启 + 健康检查
    python deploy.py --skip-build   # 跳过编译，用现有产物打包上传
    python deploy.py --force        # 二进制没变也强制上传重启

分步命令：

    python deploy.py setup          # 仅首次初始化（目录 / 初始配置 / systemd）
    python deploy.py build          # 仅本地交叉编译
    python deploy.py pack           # 仅打 tar 包（→ deploy/dist/）
    python deploy.py upload         # 仅打包上传（不构建）
    python deploy.py restart        # 仅重启服务
    python deploy.py status         # 服务状态 + 健康检查
    python deploy.py logs [-n 100]  # 服务日志

配置在同目录 .env（见 .env.example），不入库。
"""
from __future__ import annotations

import argparse
import codecs
import hashlib
import io
import os
import shlex
import subprocess
import sys
import tarfile
import time
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
DIST = HERE / "dist"

# Windows GBK 控制台打不出 ✓ 等字符会直接崩；统一按 UTF-8 输出
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# 首次下发、之后永不覆盖/删除的远端内容（生产配置在服务器上改，如 swagger、限速）
KEEP_REMOTE_PREFIXES = ("config/", "data/")

# 服务标签 → 交叉编译产物二进制名
SVC_BIN = {"relay": "dsh-relay-server"}

# .env 键 → 注入 systemd unit 的环境变量名（relay-common config 的环境变量覆盖层，
# 优先级高于 config/relay.toml）
UNIT_ENV_MAP = {
    "JWT_SECRET": ["JWT_SECRET"],          # 留空则服务自管 data/jwt.secret
    "ADMIN_PASSWORD": ["ADMIN_PASSWORD"],  # 首启种子 admin 口令（未设置则随机生成并打印一次）
    "RELAY_AUTH_MODE": ["RELAY_AUTH_MODE"],
    "RELAY_DATABASE_URL": ["RELAY_DATABASE_URL"],
    "RELAY_HOST": ["RELAY_HOST"],
    "RELAY_PORT": ["RELAY_PORT"],
    "RELAY_WS_HOST": ["RELAY_WS_HOST"],
    "RELAY_WS_PORT": ["RELAY_WS_PORT"],
}


def env(key: str, default: str = "") -> str:
    return os.environ.get(key, default).strip()


def load_env() -> None:
    try:
        from dotenv import load_dotenv
    except ImportError:
        sys.exit("缺少依赖，请先执行：pip install -r requirements.txt")
    load_dotenv(HERE / ".env")


def parse_kv(raw: str) -> dict[str, str]:
    """'K=V K2=V2' → dict（构建期/运行期环境变量）"""
    result = {}
    for kv in raw.split():
        if "=" in kv:
            k, v = kv.split("=", 1)
            result[k] = v
    return result


def services() -> list[str]:
    raw = env("DEPLOY_SERVICES", "relay")
    svc = [s.strip() for s in raw.split(",") if s.strip()]
    bad = [s for s in svc if s not in SVC_BIN]
    if bad or not svc:
        sys.exit(f"DEPLOY_SERVICES 只支持 relay（当前：{raw}）")
    return svc


def unit_name(svc: str) -> str:
    return f"{env('SERVICE_PREFIX', 'dsh')}-{svc}.service"


def remote_dir() -> str:
    return env("REMOTE_DIR", "/opt/dsh-relay").rstrip("/")


def target_triple() -> str:
    return env("TARGET_TRIPLE", "x86_64-unknown-linux-gnu")


def bin_paths() -> dict[str, Path]:
    """svc → 本地交叉编译产物路径"""
    out = REPO / "target" / target_triple() / "release"
    return {s: out / SVC_BIN[s] for s in services()}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def run_local(cmd: str, *, check: bool = True, extra_env: dict[str, str] | None = None) -> int:
    """本机执行（透传输出）；BUILD_ENV 与额外环境变量并入子进程环境。"""
    e = {**os.environ, **parse_kv(env("BUILD_ENV"))}
    if extra_env:
        e.update(extra_env)
    print(f"  $ {cmd}")
    code = subprocess.call(cmd, shell=True, cwd=str(REPO), env=e)
    if check and code != 0:
        sys.exit(f"本机命令失败（exit {code}）：{cmd}")
    return code


class Ssh:
    """paramiko 封装：命令执行（流式输出 + 可选 sudo）、tar 管道上传、单文件上传。"""

    def __init__(self) -> None:
        try:
            import paramiko
        except ImportError:
            sys.exit("缺少依赖 paramiko，请先执行：pip install -r requirements.txt")

        host = env("SSH_HOST")
        if not host:
            sys.exit("deploy/.env 缺少 SSH_HOST，填好服务器地址后再部署（见 .env.example）")

        self.user = env("SSH_USER", "root")
        self.sudo_password = env("SUDO_PASSWORD")
        self.rdir = remote_dir()
        self.client = paramiko.SSHClient()
        self.client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

        kwargs: dict = dict(
            hostname=host,
            port=int(env("SSH_PORT", "22")),
            username=self.user,
            timeout=20,
            banner_timeout=20,
            auth_timeout=20,
        )
        key = env("SSH_KEY_PATH")
        if key:
            kwargs["key_filename"] = str(Path(key).expanduser())
            if env("SSH_KEY_PASSPHRASE"):
                kwargs["passphrase"] = env("SSH_KEY_PASSPHRASE")
        if env("SSH_PASSWORD"):
            kwargs["password"] = env("SSH_PASSWORD")
        if not key and not env("SSH_PASSWORD"):
            sys.exit("deploy/.env 需要 SSH_PASSWORD 或 SSH_KEY_PATH 至少配置一项")

        print(f"→ 连接 {self.user}@{host}:{kwargs['port']} …")
        self.client.connect(**kwargs)
        self.sftp = self.client.open_sftp()

    def close(self) -> None:
        self.client.close()

    def run(
        self,
        cmd: str,
        *,
        sudo: bool = False,
        input_text: str | None = None,
        check: bool = True,
        quiet: bool = False,
    ) -> tuple[int, str, str]:
        """执行远端命令并流式转发输出；sudo 时密码走 stdin 首行（sudo -S）。"""
        if sudo and self.user != "root":
            cmd = f"sudo -S -p '' {cmd}"
            input_text = (self.sudo_password or "") + "\n" + (input_text or "")
        stdin, stdout, stderr = self.client.exec_command(cmd, get_pty=False)
        ch = stdout.channel
        if input_text:
            stdin.write(input_text)
        stdin.channel.shutdown_write()

        dec_out = codecs.getincrementaldecoder("utf-8")(errors="replace")
        dec_err = codecs.getincrementaldecoder("utf-8")(errors="replace")
        out_parts: list[bytes] = []
        err_parts: list[bytes] = []
        while True:
            if ch.recv_ready():
                data = ch.recv(65536)
                out_parts.append(data)
                if not quiet:
                    sys.stdout.write(dec_out.decode(data))
            if ch.recv_stderr_ready():
                data = ch.recv(65536)
                err_parts.append(data)
                if not quiet:
                    sys.stderr.write(dec_err.decode(data))
            if ch.exit_status_ready() and not ch.recv_ready() and not ch.recv_stderr_ready():
                break
            if ch.closed:
                # 远端命令已结束/通道已死但 exit status 丢失时的逃生口，防死等
                break
            time.sleep(0.02)
        code = ch.recv_exit_status()
        out = b"".join(out_parts).decode("utf-8", errors="replace")
        err = b"".join(err_parts).decode("utf-8", errors="replace")
        if not quiet and out and not out.endswith("\n"):
            print()
        if check and code != 0:
            sys.exit(f"远端命令失败（exit {code}）：{cmd}")
        return code, out, err

    def upload(self, pairs: list[tuple[Path, str]]) -> None:
        """把 (本地文件, 远端相对路径) 列表打成 tar 包经 SSH 管道解到部署目录。"""
        if not pairs:
            return
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w:gz") as tar:
            for local, rel in pairs:
                tar.add(str(local), arcname=rel)
        buf.seek(0)
        stdin, stdout, stderr = self.client.exec_command(
            f"mkdir -p {shlex.quote(self.rdir)} && tar -xzf - -C {shlex.quote(self.rdir)}"
        )
        stdin.write(buf.read())
        stdin.channel.shutdown_write()
        code = stdout.channel.recv_exit_status()
        if code != 0:
            sys.exit(f"上传失败：{stderr.read().decode(errors='replace')}")

    def put_file(self, local: Path, remote_rel: str) -> None:
        """SFTP 上传单文件到部署目录下。"""
        self.sftp.put(str(local), f"{self.rdir}/{remote_rel}")

    def remote_hashes(self, rels: list[str]) -> dict[str, str]:
        """远端文件 sha256（缺失的不出现在结果里）；分批执行避免命令过长。"""
        result: dict[str, str] = {}
        for i in range(0, len(rels), 40):
            batch = " ".join(shlex.quote(r) for r in rels[i : i + 40])
            code, out, _ = self.run(
                f"cd {shlex.quote(self.rdir)} && sha256sum -- {batch} 2>/dev/null",
                check=False,
                quiet=True,
            )
            for line in out.splitlines():
                parts = line.split(None, 1)
                if len(parts) == 2:
                    result[parts[1].strip()] = parts[0]
        return result


# ---------------- 各步骤 ----------------


def do_setup(ssh: Ssh, *, quiet: bool = False) -> bool:
    """幂等初始化：目录 / 初始配置 / systemd 单元（服务器不需要 Rust）。
    返回 systemd 单元是否有变化（调用方据此重启）。"""
    say = (lambda m: None) if quiet else (lambda m: print(m))
    say("→ 初始化远端环境（幂等）")

    ssh.run(f"mkdir -p {shlex.quote(ssh.rdir)}/bin {shlex.quote(ssh.rdir)}/data {shlex.quote(ssh.rdir)}/config",
            quiet=True)

    # 初始配置：config/*.toml 只在远端缺失时下发（后续更新永不覆盖）
    local_cfg = sorted((REPO / "config").glob("*.toml")) if (REPO / "config").is_dir() else []
    missing = []
    for f in local_cfg:
        code, _, _ = ssh.run(
            f"test -f {shlex.quote(ssh.rdir + '/config/' + f.name)}", check=False, quiet=True
        )
        if code != 0:
            missing.append((f, f"config/{f.name}"))
    if missing:
        say(f"  · 下发初始配置：{', '.join(r for _, r in missing)}（之后以服务器上为准）")
        ssh.upload(missing)

    # systemd 单元：内容有变化才重写
    user = env("SERVICE_USER") or ssh.user
    units_changed = False
    for svc in services():
        unit = unit_name(svc)
        text = unit_text(svc, user)
        code, out, _ = ssh.run(f"cat /etc/systemd/system/{unit} 2>/dev/null || true", check=False, quiet=True)
        if out == text:
            continue
        say(f"  · 安装 systemd 单元：{unit}")
        ssh.run(f"tee /etc/systemd/system/{unit} > /dev/null", sudo=True, input_text=text, quiet=True)
        ssh.run(f"systemctl daemon-reload && systemctl enable {unit}", sudo=True, quiet=True)
        units_changed = True
    return units_changed


def unit_env_lines() -> str:
    """把 .env 的密钥/端口配置渲染成 systemd Environment= 行（SERVICE_ENV 显式值优先）。"""
    pairs: dict[str, str] = {}
    for key, names in UNIT_ENV_MAP.items():
        v = env(key)
        if not v:
            continue
        for n in names:
            pairs[n] = v
    pairs.update(parse_kv(env("SERVICE_ENV")))
    return "".join(f'Environment="{k}={v}"\n' for k, v in pairs.items())


def unit_text(svc: str, user: str) -> str:
    env_lines = unit_env_lines()
    user_lines = "" if user == "root" else f"User={user}\nGroup={user}\n"
    return f"""[Unit]
Description=dsh-relay {svc} service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory={remote_dir()}
ExecStart={remote_dir()}/bin/{SVC_BIN[svc]}
Restart=on-failure
RestartSec=3
{env_lines}{user_lines}
[Install]
WantedBy=multi-user.target
"""


def do_build() -> None:
    """本地交叉编译 Linux 二进制（cargo-zigbuild；缺工具链自动装）。"""
    triple = target_triple()
    print(f"→ 本地交叉编译（target {triple}）")

    installed = subprocess.run(
        "rustup target list --installed", shell=True, capture_output=True, text=True
    ).stdout
    if triple not in installed:
        print("  · 安装 rustup 目标三元组 …")
        run_local(f"rustup target add {triple}")

    probe = subprocess.run(
        "cargo zigbuild -V", shell=True, capture_output=True, text=True
    )
    if probe.returncode != 0:
        print("  · 安装 cargo-zigbuild + ziglang（zig 交叉工具链）…")
        run_local(f'"{sys.executable}" -m pip install -U cargo-zigbuild ziglang')

    # 二进制名 dsh-relay-server 来自包 dsh-relay-server（apps/relay）
    pkgs = " ".join(f"-p {pkg_name(s)}" for s in services())
    run_local(f"cargo zigbuild --release --target {triple} {pkgs}")
    for svc, path in bin_paths().items():
        if not path.is_file():
            sys.exit(f"构建产物缺失：{path}")
        with path.open("rb") as f:
            if f.read(4) != b"\x7fELF":
                sys.exit(f"产物不是 Linux ELF：{path}（检查 TARGET_TRIPLE）")
        print(f"  · {svc}: {path}（{path.stat().st_size / 1024 / 1024:.1f} MB，ELF ✓）")


def pkg_name(svc: str) -> str:
    """cargo 包名（apps/relay 的 package.name 即二进制名）。"""
    return SVC_BIN[svc]


def do_pack() -> Path:
    """打 tar 包：bin/<bin> … → deploy/dist/dsh-relay-deploy-<时间戳>.tar.gz"""
    DIST.mkdir(exist_ok=True)
    missing = [f"{s} → {p}" for s, p in bin_paths().items() if not p.is_file()]
    if missing:
        sys.exit("缺少构建产物，请先 python deploy.py build：\n  " + "\n  ".join(missing))

    pkg = DIST / f"dsh-relay-deploy-{datetime.now():%Y%m%d-%H%M%S}.tar.gz"
    with tarfile.open(pkg, "w:gz") as tar:
        for svc, path in bin_paths().items():
            tar.add(str(path), arcname=f"bin/{SVC_BIN[svc]}")
    print(f"→ 打包 {pkg.name}（{pkg.stat().st_size / 1024 / 1024:.1f} MB，sha256 {sha256_file(pkg)[:12]}…）")
    return pkg


def do_upload(ssh: Ssh, pkg: Path) -> None:
    """上传部署包并解到远端 bin/。"""
    print(f"→ 上传 {pkg.name} …")
    ssh.put_file(pkg, ".deploy.tar.gz")
    ssh.run(f"mkdir -p {shlex.quote(ssh.rdir)}/bin", quiet=True)
    ssh.run(f"tar -xzf {shlex.quote(ssh.rdir)}/.deploy.tar.gz -C {shlex.quote(ssh.rdir)}", quiet=True)
    ssh.run(f"rm -f {shlex.quote(ssh.rdir)}/.deploy.tar.gz", quiet=True)
    for s in services():
        ssh.run(f"chmod +x {shlex.quote(ssh.rdir)}/bin/{SVC_BIN[s]}", quiet=True)
    user = env("SERVICE_USER") or ssh.user
    if user != ssh.user:
        ssh.run(f"chown -R {shlex.quote(user)} {shlex.quote(ssh.rdir)}/bin", sudo=True, quiet=True)


def remote_changed(ssh: Ssh) -> bool:
    """比对本地二进制与远端 bin/ 是否一致（一致则可跳过上传重启）。"""
    local = {f"bin/{SVC_BIN[s]}": sha256_file(p) for s, p in bin_paths().items() if p.is_file()}
    if len(local) != len(bin_paths()):
        return True
    remote = ssh.remote_hashes(list(local.keys()))
    return any(remote.get(k) != v for k, v in local.items())


def do_restart(ssh: Ssh) -> None:
    units = " ".join(unit_name(s) for s in services())
    print(f"→ 重启服务：{units}")
    ssh.run(f"systemctl restart {units}", sudo=True, quiet=True)
    for s in services():
        code, out, _ = ssh.run(f"systemctl is-active {unit_name(s)}", check=False, quiet=True)
        print(f"  · {unit_name(s)} → {out.strip() or 'inactive'}")


def do_health(ssh: Ssh) -> None:
    for svc in services():
        url = env(f"{svc.upper()}_HEALTH_URL")
        if not url:
            continue
        # 刚重启的服务可能尚未绑端口（部署时实测竞态），短重试 3 次
        ok = False
        for _ in range(3):
            code, out, _ = ssh.run(
                f"curl -sf --max-time 5 {shlex.quote(url)} || echo FAIL", check=False, quiet=True
            )
            if "FAIL" not in out:
                ok = True
                break
            time.sleep(1)
        print(f"  · 健康检查 {svc}: {url} → {'ok' if ok else 'FAIL'}")


def do_status(ssh: Ssh) -> None:
    for s in services():
        code, out, _ = ssh.run(f"systemctl is-active {unit_name(s)}", check=False, quiet=True)
        print(f"· {unit_name(s)} → {out.strip() or 'inactive'}")
    do_health(ssh)


def do_logs(ssh: Ssh, lines: int) -> None:
    units = " ".join(f"-u {unit_name(s)}" for s in services())
    ssh.run(f"journalctl {units} -n {lines} --no-pager", sudo=True, check=False)


def do_admin_web(ssh: Ssh) -> None:
    """构建并上传管理台前端：本地 npm build → tar → <REMOTE_DIR>/apps/admin-web/dist。
    服务按 WorkingDirectory 下的 apps/admin-web/dist 找控制台，缺失时降级为提示页。"""
    dist = REPO / "apps" / "admin-web" / "dist"
    if not (dist / "index.html").is_file():
        print("→ 构建管理台前端（npm run build）")
        if not (REPO / "apps" / "admin-web" / "node_modules").is_dir():
            run_local("cd apps/admin-web && npm install")
        run_local("cd apps/admin-web && npm run build")
    if not (dist / "index.html").is_file():
        sys.exit("admin-web 构建产物缺失：apps/admin-web/dist/index.html")

    DIST.mkdir(exist_ok=True)
    pkg = DIST / f"dsh-relay-admin-{datetime.now():%Y%m%d-%H%M%S}.tar.gz"
    with tarfile.open(pkg, "w:gz") as tar:
        tar.add(str(dist), arcname="dist")
    print(f"→ 打包 {pkg.name}（{pkg.stat().st_size / 1024 / 1024:.1f} MB）")

    print("→ 上传管理台前端（解到 apps/admin-web/）")
    ssh.put_file(pkg, ".admin-web.tar.gz")
    ssh.run(f"mkdir -p {shlex.quote(ssh.rdir)}/apps/admin-web", quiet=True)
    ssh.run(
        f"rm -rf {shlex.quote(ssh.rdir)}/apps/admin-web/dist"
        f" && tar -xzf {shlex.quote(ssh.rdir)}/.admin-web.tar.gz -C {shlex.quote(ssh.rdir)}/apps/admin-web"
        f" && rm -f {shlex.quote(ssh.rdir)}/.admin-web.tar.gz",
        quiet=True,
    )


def do_deploy(ssh: Ssh, *, skip_build: bool, force: bool) -> None:
    units_changed = do_setup(ssh, quiet=True)
    if not skip_build:
        do_build()
    pkg = do_pack()
    bin_changed = force or remote_changed(ssh)
    if bin_changed:
        do_upload(ssh, pkg)
    if bin_changed or units_changed:
        if units_changed and not bin_changed:
            print("→ 二进制无变化，但服务配置有变化，仅重启")
        do_restart(ssh)
    else:
        print("→ 二进制与配置均无变化，跳过上传与重启（--force 可强制）")
    do_health(ssh)
    do_admin_web(ssh)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="dsh-relay 部署（paramiko）：本地打 Linux 包 + 上传 + systemd",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "cmd",
        nargs="?",
        default="deploy",
        choices=["deploy", "setup", "build", "pack", "upload", "admin", "restart", "status", "logs"],
        help="deploy(默认)=编译+打包+上传+重启一条龙；admin=仅构建上传管理台前端；其余为分步命令",
    )
    parser.add_argument("--skip-build", action="store_true", help="deploy 时跳过本地编译（用现有产物）")
    parser.add_argument("--force", action="store_true", help="二进制无变化也强制上传重启")
    parser.add_argument("-n", "--lines", type=int, default=100, help="logs 显示行数（默认 100）")
    args = parser.parse_args()

    load_env()
    if args.cmd == "build":
        do_build()
        return
    if args.cmd == "pack":
        do_pack()
        return

    ssh = Ssh()
    try:
        if args.cmd == "deploy":
            do_deploy(ssh, skip_build=args.skip_build, force=args.force)
        elif args.cmd == "setup":
            if do_setup(ssh):
                print("· systemd 单元有变化，执行 restart 生效：python deploy.py restart")
            else:
                print("· 已是最新配置")
        elif args.cmd == "upload":
            pkg = do_pack()
            if args.force or remote_changed(ssh):
                do_upload(ssh, pkg)
                do_restart(ssh)
            else:
                print("→ 二进制无变化，跳过上传与重启（--force 可强制）")
            do_health(ssh)
        elif args.cmd == "admin":
            do_admin_web(ssh)
        elif args.cmd == "restart":
            do_restart(ssh)
        elif args.cmd == "status":
            do_status(ssh)
        elif args.cmd == "logs":
            do_logs(ssh, args.lines)
    finally:
        ssh.close()
    print("✓ 完成")


if __name__ == "__main__":
    main()
