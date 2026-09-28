//! DSH 云端转发 relay 服务端（Rust，dsh-relay-v1）
//!
//! 架构（对照 ZCode zcode-relay SPEC 的简化版）：
//!   手机 App ←─ WS(dsh-relay-v1) ─→ relay（本服务，部署在公网 VPS）←─ WS ─→ 桌面桥(bridge.mjs) ─→ 本机 dsh web
//!
//! v1 定位「个人自用 1 桌面 + 1 手机」：
//!   - 共享配对码（code）划分房间，一房间 = 1 host（桌面桥）+ 1 client（手机）；
//!     同角色后到踢先到（`bye{superseded}`）。
//!   - relay 只做鉴权/路由/心跳/背压，不解析业务载荷（http-req/ws-frame 对 relay 不透明）。
//!   - 生产部署务必放在 TLS 后（wss://，Caddy/Nginx 反代 WebSocket）。
//! v2 方向（对齐 ZCode SPEC）：Ed25519 身份 + challenge-auth + 二维码 PSK 配对 + E2E AES-256-GCM。
//!
//! 帧协议（文本帧，UTF-8 JSON，一帧一对象，单帧 ≤ 4 MiB）：
//!   C→S  hello {role:'host'|'client', code}          首帧，10s 内必须到达
//!   S→C  welcome {} | reject {code} | peer {online}  鉴权结果 / 对端在线状态
//!   双向 http-req {rid, method, path, body}          手机→桥：HTTP 请求
//!   双向 http-res {rid, status, body, setCookie?}    桥→手机：HTTP 响应
//!   双向 ws-open {rid, path, headers?}               手机→桥：打开到本机 dsh 的 WS 隧道
//!   双向 ws-frame {rid, text}                        隧道数据帧
//!   双向 ws-close {rid}                              隧道关闭
//!   双向 error {rid?, code, message}
//!   S→C  ping {t} / C→S  pong {t}                    心跳：15s 一次，3 次未应答踢线

use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

const HEARTBEAT_MS: u64 = 15_000;
/// WS 子协议（客户端请求了就必须回显，否则 ws 客户端握手失败）
const SUBPROTOCOL: &str = "dsh-relay-v1";
const MAX_MISSED_PONGS: u32 = 3;
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
/// 出站队列深度（背压双限之条数限；字节限由单帧 ≤4MiB + 队列深度共同兜住）
const OUT_QUEUE_FRAMES: usize = 64;
const AUTH_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Host,
    Client,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Client => "client",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "host" => Some(Role::Host),
            "client" => Some(Role::Client),
            _ => None,
        }
    }
    fn other(self) -> Role {
        match self {
            Role::Host => Role::Client,
            Role::Client => Role::Host,
        }
    }
}

type Tx = mpsc::Sender<Message>;
type CodeKey = [u8; 32];

#[derive(Clone)]
struct ClientHandle {
    id: u64,
    role: Role,
    tx: Tx,
}

struct Room {
    host: Option<ClientHandle>,
    client: Option<ClientHandle>,
}

impl Room {
    fn get(&self, role: Role) -> Option<&ClientHandle> {
        match role {
            Role::Host => self.host.as_ref(),
            Role::Client => self.client.as_ref(),
        }
    }
    fn set(&mut self, role: Role, h: ClientHandle) {
        match role {
            Role::Host => self.host = Some(h),
            Role::Client => self.client = Some(h),
        }
    }
    fn clear(&mut self, role: Role) {
        match role {
            Role::Host => self.host = None,
            Role::Client => self.client = None,
        }
    }
    fn empty(&self) -> bool {
        self.host.is_none() && self.client.is_none()
    }
}

struct AppState {
    rooms: Mutex<HashMap<CodeKey, Room>>,
    next_id: AtomicU64,
}

impl AppState {
    fn new() -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    fn peer_tx(&self, key: &CodeKey, me: Role) -> Option<Tx> {
        self.rooms
            .lock()
            .unwrap()
            .get(key)
            .and_then(|r| r.get(me.other()))
            .map(|h| h.tx.clone())
    }

    fn join(&self, key: CodeKey, role: Role, handle: ClientHandle) -> Option<ClientHandle> {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.entry(key).or_insert_with(|| Room { host: None, client: None });
        let old = room.get(role).cloned();
        room.set(role, handle);
        old
    }

    fn leave(&self, key: &CodeKey, role: Role) -> Option<Tx> {
        let mut rooms = self.rooms.lock().unwrap();
        let peer = if let Some(room) = rooms.get_mut(key) {
            room.clear(role);
            let peer = room.get(role.other()).map(|h| h.tx.clone());
            if room.empty() {
                rooms.remove(key);
            }
            peer
        } else {
            None
        };
        peer
    }
}

fn json_text(v: Value) -> Message {
    Message::Text(v.to_string())
}

fn bye_msg(code: &str) -> Message {
    json_text(serde_json::json!({ "type": "bye", "code": code }))
}

fn reject_msg(code: &str) -> Message {
    eprintln!("[relay] reject: {code}");
    json_text(serde_json::json!({ "type": "reject", "code": code }))
}

fn hash_code(code: &str) -> CodeKey {
    let mut h = Sha256::new();
    h.update(code.as_bytes());
    h.finalize().into()
}

fn code_matches(a: &str, b: &CodeKey) -> bool {
    let ha = hash_code(a);
    bool::from(ha.as_slice().ct_eq(b.as_slice()))
}

async fn handle_connection(stream: TcpStream, state: Arc<AppState>) {
    let addr = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "?".into());

    let cfg = WebSocketConfig {
        max_message_size: Some(MAX_FRAME_BYTES),
        ..Default::default()
    };
    // 协商子协议 dsh-relay-v1：客户端带 Sec-WebSocket-Protocol 时必须回显
    let callback = |req: &Request, mut res: Response| -> Result<Response, ErrorResponse> {
        let requested = req
            .headers()
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').any(|p| p.trim() == SUBPROTOCOL))
            .unwrap_or(false);
        if requested {
            if let Ok(val) = SUBPROTOCOL.parse() {
                res.headers_mut().insert(SEC_WEBSOCKET_PROTOCOL, val);
            }
        }
        Ok(res)
    };
    let ws = match tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(cfg)).await
    {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[relay] {addr} upgrade failed: {e}");
            return;
        }
    };
    let (mut wtx, mut wrx) = ws.split();

    // ---- 鉴权阶段：首帧必须是 hello，10s 超时 ----
    let first = timeout(Duration::from_secs(AUTH_TIMEOUT_SECS), wrx.next()).await;
    let first_text = match first {
        Ok(Some(Ok(Message::Text(t)))) => t,
        Ok(Some(Ok(_))) => {
            let _ = wtx.send(reject_msg("hello-required")).await;
            return;
        }
        _ => {
            let _ = wtx.send(reject_msg("auth-timeout")).await;
            return;
        }
    };

    let hello: Value = match serde_json::from_str(&first_text) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("[relay] {addr} bad hello json: {}", first_text.chars().take(120).collect::<String>());
            let _ = wtx.send(reject_msg("bad-hello")).await;
            return;
        }
    };
    if hello.get("type").and_then(Value::as_str) != Some("hello") {
        let _ = wtx.send(reject_msg("hello-required")).await;
        return;
    }
    let role = match hello.get("role").and_then(Value::as_str).and_then(Role::parse) {
        Some(r) => r,
        None => {
            let _ = wtx.send(reject_msg("bad-hello")).await;
            return;
        }
    };
    let code = match hello.get("code").and_then(Value::as_str) {
        Some(c) if c.len() >= 6 => c.to_string(),
        _ => {
            let _ = wtx.send(reject_msg("bad-hello")).await;
            return;
        }
    };
    let key = hash_code(&code);

    // ---- 入住房间 ----
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::channel::<Message>(OUT_QUEUE_FRAMES);
    let handle = ClientHandle { id, role, tx: tx.clone() };
    if let Some(old) = state.join(key, role, handle) {
        // 同角色后到踢先到（superseded）
        let _ = old.tx.try_send(bye_msg("superseded"));
        let _ = old.tx.try_send(Message::Close(None));
    }

    let peer_online = state.peer_tx(&key, role).is_some();
    if tx
        .send(json_text(
            serde_json::json!({ "type": "welcome", "role": role.as_str(), "peerOnline": peer_online }),
        ))
        .await
        .is_err()
    {
        state.leave(&key, role);
        return;
    }
    if let Some(peer) = state.peer_tx(&key, role) {
        let _ = peer.try_send(json_text(serde_json::json!({ "type": "peer", "online": true })));
    }
    eprintln!(
        "[relay] {} {role:?} #{id} joined room {} (peer={peer_online})",
        addr,
        hex8(&key)
    );

    // ---- 会话阶段：读帧 / 发帧 / 心跳 ----
    let mut hb = interval(Duration::from_millis(HEARTBEAT_MS));
    hb.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut missed_pongs: u32 = 0;
    let mut authenticated = true;

    loop {
        tokio::select! {
            incoming = wrx.next() => {
                match incoming {
                    Some(Ok(Message::Text(t))) => {
                        if !dispatch_frame(&state, &key, role, &tx, &t, &mut missed_pongs) {
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        let _ = wtx.send(Message::Pong(p)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => { /* 二进制帧拒绝但不断线（协议为文本帧） */ }
                    Some(Err(_)) => break,
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(msg) => {
                        let is_close = matches!(msg, Message::Close(_));
                        if wtx.send(msg).await.is_err() { break; }
                        if is_close { break; }
                    }
                    None => break,
                }
            }
            _ = hb.tick() => {
                if !authenticated { continue; }
                missed_pongs += 1;
                if missed_pongs > MAX_MISSED_PONGS {
                    let _ = wtx.send(bye_msg("timeout")).await;
                    break;
                }
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let _ = wtx.send(json_text(serde_json::json!({ "type": "ping", "t": now }))).await;
            }
        }
    }

    // ---- 离房 ----
    let _ = authenticated;
    if let Some(peer) = state.leave(&key, role) {
        let _ = peer.try_send(json_text(serde_json::json!({ "type": "peer", "online": false })));
    }
    eprintln!("[relay] {role:?} #{id} disconnected");
}

/// 数据面分发。返回 false 表示应断开本连接。
fn dispatch_frame(
    state: &Arc<AppState>,
    key: &CodeKey,
    role: Role,
    me: &Tx,
    text: &str,
    missed_pongs: &mut u32,
) -> bool {
    let frame: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => {
            let _ = me.try_send(reject_msg("bad-frame"));
            return true;
        }
    };
    let ftype = frame.get("type").and_then(Value::as_str).unwrap_or("");
    match ftype {
        "pong" => {
            *missed_pongs = 0;
            true
        }
        "ping" => {
            let _ = me.try_send(json_text(serde_json::json!({ "type": "pong", "t": frame.get("t").cloned().unwrap_or(Value::Null) })));
            true
        }
        "http-req" | "http-res" | "ws-open" | "ws-frame" | "ws-close" | "error" => {
            // relay 不解析业务内容：形状校验后原样转发给对端
            if frame.get("rid").and_then(Value::as_str).is_none() {
                let _ = me.try_send(json_text(serde_json::json!({ "type": "error", "code": "bad-frame", "message": "missing rid" })));
                return true;
            }
            match state.peer_tx(key, role) {
                Some(peer) => {
                    if peer.try_send(Message::Text(text.to_string())).is_err() {
                        // 对端出站队列满：踢对端（背压双限）并回 unreachable
                        let _ = me.try_send(json_text(serde_json::json!({
                            "type": "error",
                            "rid": frame.get("rid").cloned().unwrap_or(Value::Null),
                            "code": "peer-offline",
                            "message": "对端不可用（背压溢出）"
                        })));
                    }
                    true
                }
                None => {
                    let _ = me.try_send(json_text(serde_json::json!({
                        "type": "error",
                        "rid": frame.get("rid").cloned().unwrap_or(Value::Null),
                        "code": "peer-offline",
                        "message": "对端不在线"
                    })));
                    true
                }
            }
        }
        _ => {
            let _ = me.try_send(json_text(serde_json::json!({ "type": "error", "code": "bad-frame", "message": format!("unknown type {ftype}") })));
            true
        }
    }
}

fn hex8(key: &CodeKey) -> String {
    key.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() {
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = env::var("PORT").unwrap_or_else(|_| "8787".into());
    let bind = format!("{host}:{port}");

    let listener = TcpListener::bind(&bind).await.expect("bind failed");
    eprintln!("[relay] (rust) listening on {bind} (subprotocol dsh-relay-v1)");
    let state = Arc::new(AppState::new());

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let st = state.clone();
                tokio::spawn(async move { handle_connection(stream, st).await });
            }
            Err(e) => eprintln!("[relay] accept error: {e}"),
        }
    }
}
