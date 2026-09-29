# DSH 云端转发（relay）

不在局域网时的接入方案：手机 → relay（公网 VPS）→ 桌面桥 → 本机 dsh web。
协议 `dsh-relay-v1`（设计对照 ZCode `apps/zcode-relay/SPEC.md` 的简化版）。

v2 起服务端架构对齐 **byte-admin**（Rust 后台管理模板）：Cargo workspace +
axum 管理 API/内置控制台 + sea-orm(sqlite) + JWT + tracing。

## 组件

| 组件 | 位置 | 语言 | 运行位置 |
|---|---|---|---|
| relay 服务端 | 本仓库 `apps/relay` + `crates/*` | **Rust**（axum + tokio-tungstenite + sea-orm） | 公网 VPS |
| 桌面桥（**推荐：dsh 插件**） | [`../dsh-relay-plugin/`](../dsh-relay-plugin) | Node.js（cordis bundle） | dsh 进程内，随 dsh 启停 |
| 桌面桥（独立脚本，兼容保留） | `test/bridge.mjs` | Node.js | 跑 dsh web 的桌面机 |
| 测试客户端 | `test/test-client.mjs` / `test/test-mux.mjs` / `test/test-auth.mjs` | Node.js | 任意 |
| 手机接入 | [`../dsh-relay-mobile/`](../dsh-relay-mobile) `lib/dsh/transport.dart::RelayTransport` | Dart | App 内「云端转发」模式 |

## 服务端架构（对齐 byte-admin）

```
dsh-relay-service/
├── config/relay.toml        # 分层配置：内置默认 < 本文件 < 环境变量（RELAY_* / JWT_SECRET）
├── apps/relay/              # 可执行 dsh-relay-server
│   ├── src/ws.rs            # WS 转发数据面（dsh-relay-v1）
│   ├── src/router.rs        # 管理面路由（公开组限速 / 受保护组 JWT+oplog）
│   ├── src/handlers/        # auth / devices / pairing / audit / status / health / openapi
│   └── src/admin.html       # 内置管理台（单文件控制台）
└── crates/
    ├── common               # 配置/响应壳{code,result,message}/AppError/JWT/分页/ws-hub
    ├── entity               # sea-orm 实体：admin_users / devices / pairing_codes / audit_logs
    ├── migration            # 建表迁移 + 首启种子（生成管理密码，只打印一次）
    ├── middleware           # admin_auth / rate_limit / oplog
    └── service              # 业务层：认证 / 设备 / 配对码 / 审计 / 健康
```

- **管理台**：`http://<host>:8788/`（登录 → 概览 / 设备吊销·轮换·删除 / 配对码 / 审计）。
  初始账号 `admin`，密码由 `ADMIN_PASSWORD` 环境变量指定或首启随机生成（日志只打印一次）。
- **管理 API**：`/api/openapi.json` 可查全量接口；JWT（access 2h + refresh 7d，nonce 轮转）。
- **持久化**：sqlite（`data/relay.db`，迁移自动建表）；审计按 `audit.retain_days` 启动清理。
- **安全**：bcrypt 管理密码、登录失败限速（默认 10 次/60s）、设备令牌只存 SHA-256、
  吊销即时踢线、写操作 oplog 落审计、WS hello 失败限速、默认 `auth_mode=device` fail-closed。
- **部署建议**：公网务必置于 TLS 反代后（Caddy/Nginx，WS 走 wss）；`server.trust_proxy=true`
  仅在可信反代后开启。明文 HTTP 时控制台会显示「未加密」警告。

## 帧协议（文本帧，UTF-8 JSON，一帧一对象，单帧 ≤ 4 MiB）

```
C→S  hello {role:'host'|'client', code, deviceId?, token?, pairingCode?, name?}
                              首帧，10s 内必须到达；auth_mode=device 时必须带 token 或 pairingCode
S→C  welcome {role, peerOnline, room, auth:'token'|'pairing'|'code', device?:{id,token}}
S→C  reject {code} | peer {online} | bye {code}
双向 http-req {rid, method, path, body}          手机→桥：HTTP 请求
双向 http-res {rid, status, body, setCookie?}    桥→手机：HTTP 响应
双向 ws-open {rid, path, headers?}               手机→桥：WS 隧道（桥以 ws-frame{text:'__open__'} 应答）
双向 ws-frame {rid, text}                        隧道数据帧
双向 ws-close {rid}                              隧道关闭
双向 error {rid?, code, message}
```

- 配对码（code）划分房间：1 host + 1 client；同角色后到踢先到（`bye{superseded}`）。
- **设备鉴权（默认 fail-closed）**：管理台生成一次性配对码（`XXXX-XXXX`，600s/单次/
  失败 5 次熔断）→ 客户端 `hello{pairingCode}` 配对 → `welcome.device` 下发设备 ID +
  令牌（明文只此一次）→ 之后 `hello{deviceId, token}` 重连。令牌可吊销/轮换，吊销即时踢线。
- reject 码：`auth-required` / `bad-token` / `revoked` / `unknown-device` / `role-mismatch` /
  `room-mismatch` / `pairing-invalid|expired|used|burned` / `rate-limited` / `bad-hello|role|code`。
- 背压：出站队列 64 条，溢出踢线（`bye{overflow}`）。relay 不解析业务载荷。

## 部署

```powershell
# VPS：relay 服务端（Rust）
cd dsh-relay-service
cargo build --release            # 产出 target/release/dsh-relay-server（.exe）
$env:ADMIN_PASSWORD='<强口令>'    # 可选；不设则首启随机生成并打印
./target/release/dsh-relay-server
# 管理台 http://<vps>:8788/   WS ws://<vps>:8787（协议 dsh-relay-v1）
# 生产务必置于 TLS 后：Caddy/Nginx 反代（wss://），勿裸奔公网明文

# 桌面机：桥（与 dsh web 同机）
# 方式 A（推荐）：装进 dsh profile 当插件，随 dsh 启停自动挂载
#   dsh plugin --profile web add link:<同盘 junction 或插件目录>   # 详见 ../dsh-relay-plugin/README.md
# 方式 B（独立脚本，调试/应急）：
cd test
npm install                      # 只有 ws 一个依赖
$env:RELAY_URL='wss://your-vps:8787'
$env:RELAY_CODE='<长随机配对码>'   # 与手机端输入一致
node bridge.mjs

# 手机 App：连接设置 → 云端转发 → relay 地址 + 配对码 → 连接
```

## 自测

```powershell
node test/test-auth.mjs                                    # P0 安全底座冒烟（登录/配对/令牌/吊销/审计）
node test/test-client.mjs ws://127.0.0.1:8787 <code>        # HTTP 通道：session/list
node test/test-mux.mjs     ws://127.0.0.1:8787 <code> <sid> # WS 隧道：session/follow 快照
```

> 注：WS hello 失败限速为 10 次/60s（每 IP），连续重跑 `test-auth.mjs` 前请重启服务或间隔 60s；
> 同机若有 App 自动重连也会消耗窗口。

## v3 方向

Ed25519 身份（deviceId=公钥）+ challenge-auth、二维码 PSK 配对（k 不上网）、
E2E AES-256-GCM（X25519+HKDF，方向计数器 nonce 重放免疫）、管理台 Vue 化。
