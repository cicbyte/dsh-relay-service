# DSH 云端转发（relay）

不在局域网时的接入方案：手机 → relay（公网 VPS）→ 桌面桥 → 本机 dsh web。
协议 `dsh-relay-v1`（设计对照 ZCode `apps/zcode-relay/SPEC.md` 的简化版）。

## 组件

| 组件 | 位置 | 语言 | 运行位置 |
|---|---|---|---|
| relay 服务端 | 本仓库 `src/` | **Rust**（tokio + tokio-tungstenite） | 公网 VPS |
| 桌面桥（**推荐：dsh 插件**） | [`../dsh-relay-plugin/`](../dsh-relay-plugin) | Node.js（cordis bundle） | dsh 进程内，随 dsh 启停 |
| 桌面桥（独立脚本，兼容保留） | `test/bridge.mjs` | Node.js | 跑 dsh web 的桌面机 |
| 测试客户端 | `test/test-client.mjs` / `test/test-mux.mjs` | Node.js | 任意 |
| 手机接入 | [`../dsh-relay-mobile/`](../dsh-relay-mobile) `lib/dsh/transport.dart::RelayTransport` | Dart | App 内「云端转发」模式 |

## 帧协议（文本帧，UTF-8 JSON，一帧一对象，单帧 ≤ 4 MiB）

```
C→S  hello {role:'host'|'client', code}          首帧，10s 内必须到达
S→C  welcome {role, peerOnline} | reject {code} | peer {online}
双向 http-req {rid, method, path, body}          手机→桥：HTTP 请求
双向 http-res {rid, status, body, setCookie?}    桥→手机：HTTP 响应
双向 ws-open {rid, path, headers?}               手机→桥：WS 隧道（桥以 ws-frame{text:'__open__'} 应答）
双向 ws-frame {rid, text}                        隧道数据帧
双向 ws-close {rid}                              隧道关闭
双向 error {rid?, code, message}
S→C  ping {t} / C→S  pong {t}                    心跳 15s，3 次未应答踢线（bye{timeout}）
```

- 共享配对码（code）划分房间：1 host + 1 client；同角色后到踢先到（`bye{superseded}`）。
- 背压：出站队列 64 条，溢出踢线（`bye{overflow}`）。
- relay 不解析业务载荷，只做鉴权/路由/心跳/背压。

## 部署

```powershell
# VPS：relay 服务端（Rust）
cd dsh-relay-service
cargo build --release            # 产出 target/release/dsh-relay-server.exe（Linux 同理）
$env:PORT='8787'                 # 或 HOST/PORT 环境变量
./target/release/dsh-relay-server
# 生产务必置于 TLS 后：Caddy/Nginx 反代 WebSocket（wss://），勿裸奔公网明文

# 桌面机：桥（与 dsh web 同机）
# 方式 A（推荐）：装进 dsh profile 当插件，随 dsh 启停自动挂载
#   dsh plugin --profile web add link:<同盘 junction 或插件目录>   # 详见 ../dsh-relay-plugin/README.md
#   配置走设置页「手机通道」，或兜底 $DSH_HOME/mobile-bridge.json（relayUrl/code/dshUrl）
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
node test/test-client.mjs ws://127.0.0.1:8787 <code>   # HTTP 通道：session/list
node test/test-mux.mjs     ws://127.0.0.1:8787 <code> <sessionId>  # WS 隧道：session/follow 快照
```

## v2 方向（对齐 ZCode SPEC）

Ed25519 身份（deviceId=公钥）+ challenge-auth、二维码 PSK 配对（k 不上网）、
E2E AES-256-GCM（X25519+HKDF，方向计数器 nonce 重放免疫）、auth/claim 限速、
`devices.json` 持久化与吊销、`/healthz` 与管理口。
