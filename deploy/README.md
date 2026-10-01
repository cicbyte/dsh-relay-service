# dsh-relay 部署（paramiko）

**本地打 Linux 包 → 上传 → systemd 重启**：本机用 cargo-zigbuild 交叉编译出 Linux
二进制（`dsh-relay-server`，WS :8787 + 管理台 :8788 一体），打成 tar 包传到服务器
解到 `<REMOTE_DIR>/bin/`。服务器只需要 Ubuntu 自带的 glibc——**不用装 Rust、不用放源码**。

**免反代**：WS 数据面与管理台直连暴露（`config/relay.toml` 已置 `trust_proxy = false`，
伪造头不采信）。安全组/防火墙需放行 8787、8788 两端口。

## 准备（一次性）

```bash
pip install -r requirements.txt          # paramiko + python-dotenv
pip install cargo-zigbuild ziglang       # 本地交叉编译工具链（build 时缺也会自动装）
rustup target add x86_64-unknown-linux-gnu
# 编辑 deploy/.env：至少 SSH_HOST + 认证（SSH_PASSWORD / SSH_KEY_PATH）
```

`.env` 全部配置项见 `.env.example`。ARM 云主机把 `TARGET_TRIPLE` 改成
`aarch64-unknown-linux-gnu`（对应 `rustup target add aarch64-unknown-linux-gnu`）。

## 日常更新部署

```bash
python deploy.py                # 一条龙：编译 + 打包 + 上传 + 重启 + 健康检查
python deploy.py --skip-build   # 跳过编译，用现有产物打包上传（改配置后重发很快）
python deploy.py --force        # 二进制没变也强制上传重启
```

每次更新会产出一个部署包 `deploy/dist/dsh-relay-deploy-<时间戳>.tar.gz`（内含 `bin/`）。
上传前比对本地与远端二进制 sha256，**没变化自动跳过上传与重启**。

## 分步命令

```bash
python deploy.py setup          # 仅首次初始化（目录 / 初始配置 / systemd）
python deploy.py build          # 仅本地交叉编译（cargo zigbuild）
python deploy.py pack           # 仅打 tar 包（→ deploy/dist/）
python deploy.py upload         # 仅打包 + 上传 + 重启（不构建）
python deploy.py restart        # 仅重启服务
python deploy.py status         # 服务状态 + 健康检查
python deploy.py logs -n 200    # journalctl 日志
```

## 说明

- **免反代直连**：WS :8787 / 管理台 :8788 直接对外（安全组放行两端口；本机 ufw
  活跃时 `ufw allow 8787 8788`）。`trust_proxy` 保持 `false`——直连暴露不采信
  `X-Forwarded-For`，防伪造头绕过限速/污染审计。
- **密钥**：`.env` 的 `JWT_SECRET` / `ADMIN_PASSWORD` 注入 systemd unit 环境
  （优先级高于 `config/relay.toml`）。`JWT_SECRET` 留空则服务自管
  `<REMOTE_DIR>/data/jwt.secret`；`ADMIN_PASSWORD` 未设置则首启随机生成并
  一次性打印在日志（`python deploy.py logs` 查看）。
- **配置策略**：`config/*.toml` 仅首次下发到服务器，之后**永不覆盖/删除**——生产
  配置（swagger 开关、限速、auth_mode 等）直接改服务器上的 `config/relay.toml`；
  运行期环境变量用 `.env` 的 `SERVICE_ENV`（写进 systemd unit）。
- **数据**：SQLite 在远端 `<REMOTE_DIR>/data/relay.db`（配置里相对路径，以
  WorkingDirectory 解析）；迁移随服务启动自动执行；审计日志按 `audit.retain_days`
  启动清理。
- **端口**：WS :8787（`RELAY_WS_PORT` 可改）、管理台/API :8788（`RELAY_PORT` 可改）。
  生产建议在服务器配置里置 `swagger = false`（隐藏 openapi 文档页）。
- 健康检查默认打 `http://127.0.0.1:8788/api/health`（`RELAY_HEALTH_URL` 可改，留空跳过）。
- 首次部署会自动装 systemd 单元（`dsh-relay.service`）并 enable，之后随 `deploy`
  自动重启；崩溃自动拉起（Restart=on-failure）。
