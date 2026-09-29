//! WS 转发数据面（子协议 dsh-relay-v1）：房间转发 / 设备令牌鉴权 / 保活。
//! 数据面协议与历史版本保持兼容：hello/welcome/reject/peer/bye 与
//! http-req/http-res/ws-open/ws-frame/ws-close/error 中继帧不变；
//! v2 增加 hello 的 deviceId+token / pairingCode 字段与 welcome.device。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::Message;

use relay_common::hub::WsOut;
use relay_common::util::now_secs;
use relay_common::AppState;

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Host,
    Client,
}

impl Role {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "host" => Some(Role::Host),
            "client" => Some(Role::Client),
            _ => None,
        }
    }

    fn str(&self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Client => "client",
        }
    }
}

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// hello 鉴权失败限速（每 IP）
const MAX_FAILS: u32 = 10;
const WINDOW_SECS: i64 = 60;

struct RateLimiter {
    fails: Mutex<HashMap<String, (u32, i64)>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            fails: Mutex::new(HashMap::new()),
        }
    }

    fn blocked(&self, ip: &str) -> bool {
        let mut m = self.fails.lock().unwrap();
        let now = now_secs();
        if let Some((_, start)) = m.get(ip) {
            if now - *start > WINDOW_SECS {
                m.remove(ip);
            }
        }
        m.get(ip).map(|(n, _)| *n >= MAX_FAILS).unwrap_or(false)
    }

    fn fail(&self, ip: &str) {
        let mut m = self.fails.lock().unwrap();
        let now = now_secs();
        let e = m.entry(ip.to_string()).or_insert((0, now));
        if now - e.1 > WINDOW_SECS {
            *e = (0, now);
        }
        e.0 += 1;
    }

    fn reset(&self, ip: &str) {
        self.fails.lock().unwrap().remove(ip);
    }
}

/// 房间 key：共享配对码 SHA-256 的前 4 字节 hex（hex8）
fn room_of(code: &str) -> String {
    let mut h = Sha256::new();
    h.update(code.as_bytes());
    h.finalize().iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn json_text(v: &Value) -> String {
    v.to_string()
}

fn reject_msg(code: &str) -> String {
    tracing::warn!(reject = %code, "拒绝连接");
    json_text(&json!({ "type": "reject", "code": code }))
}

fn bye_msg(code: &str) -> String {
    json_text(&json!({ "type": "bye", "code": code }))
}

/// 启动 WS 数据面（常驻 accept 循环）
pub async fn serve(addr: &str, state: AppState) {
    let listener = TcpListener::bind(addr).await.expect("ws bind failed");
    tracing::info!(
        %addr,
        auth = %state.config.ws.auth_mode,
        "ws 数据面已启动（dsh-relay-v1）"
    );
    let rate = std::sync::Arc::new(RateLimiter::new());
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let st = state.clone();
                let rt = rate.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle(stream, peer, st, rt).await {
                        tracing::debug!(peer = %peer, error = %e, "连接结束");
                    }
                });
            }
            Err(e) => tracing::warn!(error = %e, "accept error"),
        }
    }
}

type WsStream = tokio_tungstenite::WebSocketStream<TcpStream>;

async fn handle(
    stream: TcpStream,
    peer: SocketAddr,
    state: AppState,
    rate: std::sync::Arc<RateLimiter>,
) -> Result<(), String> {
    let ip = peer.ip().to_string();

    // 子协议协商：客户端带 dsh-relay-v1 时回选
    let ws: WsStream = tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
            let proto = req
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if proto.split(',').map(|s| s.trim()).any(|p| p == "dsh-relay-v1") {
                if let Ok(v) = "dsh-relay-v1".parse() {
                    response.headers_mut().insert("sec-websocket-protocol", v);
                }
            }
            Ok(response)
        },
    )
    .await
    .map_err(|e| format!("ws upgrade failed: {e}"))?;

    let (mut sink, mut stream) = ws.split();

    // fail-closed：10s 内必须交出 hello
    let first = tokio::time::timeout(HELLO_TIMEOUT, stream.next())
        .await
        .map_err(|_| "hello timeout")?
        .ok_or("closed before hello")?
        .map_err(|e| format!("read error: {e}"))?;
    let hello_text = match first {
        Message::Text(t) => t,
        _ => return Err("first frame must be text".into()),
    };
    let hv: Value = serde_json::from_str(&hello_text).map_err(|_| "bad hello json".to_string())?;
    if hv.get("type").and_then(|v| v.as_str()) != Some("hello") {
        let _ = sink.send(Message::Text(reject_msg("bad-hello"))).await;
        return Err("bad-hello".into());
    }

    let role_s = hv.get("role").and_then(|v| v.as_str()).unwrap_or("");
    let Some(role) = Role::parse(role_s) else {
        let _ = sink.send(Message::Text(reject_msg("bad-role"))).await;
        return Err("bad-role".into());
    };
    let code = hv.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if code.len() < 6 {
        let _ = sink.send(Message::Text(reject_msg("bad-code"))).await;
        return Err("bad-code".into());
    }
    if rate.blocked(&ip) {
        let _ = sink
            .send(Message::Text(reject_msg("rate-limited")))
            .await;
        return Err("rate-limited".into());
    }
    let room = room_of(code);

    // ---- 鉴权（device 模式 fail-closed；code 模式为旧共享码兼容） ----
    let device_id;
    let mut auth_tag = "code";
    let mut pair_issued: Option<(String, String)> = None;

    if state.config.ws.auth_mode == "device" {
        let token = hv.get("token").and_then(|v| v.as_str()).unwrap_or("");
        let pair_code = hv.get("pairingCode").and_then(|v| v.as_str()).unwrap_or("");
        let name = hv.get("name").and_then(|v| v.as_str()).unwrap_or("");

        if !pair_code.is_empty() && !token.is_empty() {
            rate.fail(&ip);
            let _ = sink
                .send(Message::Text(reject_msg("auth-required")))
                .await;
            return Err("auth-required".into());
        }

        if !pair_code.is_empty() {
            match relay_service::pairing::redeem(&state, pair_code, role_s, &room, name).await {
                Ok((dev, token)) => {
                    device_id = dev.id.clone();
                    pair_issued = Some((dev.id, token));
                    auth_tag = "pairing";
                }
                Err(e) => {
                    rate.fail(&ip);
                    let _ = sink.send(Message::Text(reject_msg(e.code()))).await;
                    return Err(e.code().into());
                }
            }
        } else if !token.is_empty() {
            let dev_id = hv.get("deviceId").and_then(|v| v.as_str()).unwrap_or("");
            match relay_service::device::check_token(&state, dev_id, token, role_s, &room).await {
                Ok(dev) => {
                    device_id = dev.id.clone();
                    auth_tag = "token";
                    relay_service::device::touch(&state, &device_id).await;
                }
                Err(e) => {
                    rate.fail(&ip);
                    let _ = sink.send(Message::Text(reject_msg(e.code()))).await;
                    return Err(e.code().into());
                }
            }
        } else {
            rate.fail(&ip);
            let _ = sink
                .send(Message::Text(reject_msg("auth-required")))
                .await;
            return Err("auth-required".into());
        }
    } else {
        device_id = format!("u{}", room);
    }
    rate.reset(&ip);

    // ---- 入房 ----
    let (tx, mut rx) = mpsc::channel::<WsOut>(256);
    let join = state.hub.join(&room, role.str(), &device_id, tx);

    let mut welcome = json!({
        "type": "welcome",
        "role": role_s,
        "peerOnline": join.peer_online,
        "room": room,
        "auth": auth_tag,
    });
    if let Some((id, token)) = pair_issued.as_ref() {
        welcome["device"] = json!({ "id": id, "token": token });
    }
    if sink.send(Message::Text(json_text(&welcome))).await.is_err() {
        state.hub.leave(join.id);
        return Err("send welcome failed".into());
    }

    // 同角色后到踢先到
    if let Some(old_tx) = join.replaced {
        let _ = old_tx.try_send(WsOut::Text(bye_msg("superseded")));
        let _ = old_tx.try_send(WsOut::Close);
    }
    // 通知对端上线
    if join.peer_online {
        if let Some(peer_tx) = state.hub.peer_of(&room, role.str()) {
            let _ = peer_tx
                .send(WsOut::Text(json_text(&json!({ "type": "peer", "online": true }))))
                .await;
        }
    }

    relay_service::audit::record(
        &state,
        "conn.open",
        &ip,
        Some(&device_id),
        json!({ "role": role_s, "room": room, "auth": auth_tag }),
    )
    .await;

    // ---- 转发循环 ----
    let mut last_activity = now_secs();
    let mut tick = tokio::time::interval(PING_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut done = false;

    while !done {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(m)) => {
                        last_activity = now_secs();
                        match m {
                            Message::Text(text) => {
                                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                                    match v.get("type").and_then(|t| t.as_str()) {
                                        Some("hello") | Some("welcome") | Some("reject") | Some("bye") => {}
                                        Some("ping") => {
                                            let _ = sink.send(Message::Text(json_text(&json!({ "type": "pong" })))).await;
                                        }
                                        Some(t) if matches!(t, "http-req" | "http-res" | "ws-open" | "ws-frame" | "ws-close" | "error") => {
                                            if let Some(peer_tx) = state.hub.peer_of(&room, role.str()) {
                                                if peer_tx.send(WsOut::Text(text)).await.is_err() {
                                                    let _ = sink.send(Message::Text(json_text(&json!({ "type": "peer", "online": false })))).await;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                            Message::Close(_) => done = true,
                        }
                    }
                    Some(Err(_)) | None => done = true,
                }
            }
            out = rx.recv() => {
                match out {
                    Some(WsOut::Text(t)) => {
                        if sink.send(Message::Text(t)).await.is_err() {
                            done = true;
                        }
                    }
                    Some(WsOut::Close) => {
                        let _ = sink.send(Message::Close(None)).await;
                        done = true;
                    }
                    None => done = true,
                }
            }
            _ = tick.tick() => {
                if now_secs() - last_activity > IDLE_TIMEOUT.as_secs() as i64 {
                    tracing::warn!(device = %device_id, "空闲超时踢线");
                    done = true;
                } else if sink.send(Message::Ping(Vec::new())).await.is_err() {
                    done = true;
                }
            }
        }
    }

    // ---- 离房 ----
    if let Some(peer_tx) = state.hub.leave(join.id) {
        let _ = peer_tx
            .send(WsOut::Text(json_text(&json!({ "type": "peer", "online": false }))))
            .await;
    }
    relay_service::audit::record(
        &state,
        "conn.close",
        &ip,
        Some(&device_id),
        json!({ "role": role.str(), "room": room }),
    )
    .await;
    Ok(())
}
