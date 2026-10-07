//! WS 转发数据面（子协议 dsh-relay-v1）：房间转发 / 设备令牌鉴权 / 保活。
//! 数据面协议与历史版本保持兼容：hello/welcome/reject/peer/bye 与
//! http-req/http-res/ws-open/ws-frame/ws-close/error 中继帧不变；
//! v2 增加 hello 的 deviceId+token / pairingCode 字段与 welcome.device。
//! v3 增量（可选能力，老端零感知）：hello.resumeFrom + resume 判定帧（断点回放续传）、
//! hello.batch + batch 信封（小帧合并）、隧道帧 seq 字段（每目的地递增）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Semaphore};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

use relay_common::hub::{buf_key_client, buf_key_host, Replay, WsOut};
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
const PING_INTERVAL: Duration = Duration::from_secs(5);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// hello 鉴权失败限速（每 IP）
const MAX_FAILS: u32 = 10;
const WINDOW_SECS: i64 = 60;
/// 连接资源上限：全局并发 + 每 IP 并发（未鉴权连接也占额，防握手风暴耗尽资源）
const MAX_CONNS_GLOBAL: usize = 256;
const MAX_CONNS_PER_IP: usize = 16;
/// 单条 WS 消息上限（协议帧 ≤4MiB；hello 是小 JSON，够了）
const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
/// 出站积压字节预算：超限发 bye{overflow} 踢线（README 承诺的慢端保护）
const OUT_BYTES_MAX: usize = 64 * 1024 * 1024;

/// 每 IP 活跃连接计数守卫：Drop 时递减，连接异常退出也不漏计
struct IpGuard {
    per_ip: Arc<Mutex<HashMap<String, usize>>>,
    ip: String,
}
impl Drop for IpGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = self.per_ip.lock() {
            if let Some(c) = m.get_mut(&self.ip) {
                *c -= 1;
                if *c == 0 {
                    m.remove(&self.ip);
                }
            }
        }
    }
}

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

    /// 距窗口解除的剩余秒数（供 reject.retryAfterSecs 提示客户端退避）
    fn retry_after(&self, ip: &str) -> i64 {
        let m = self.fails.lock().unwrap();
        m.get(ip)
            .map(|(_, start)| (start + WINDOW_SECS - now_secs()).max(1))
            .unwrap_or(1)
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

/// 编码二进制载荷帧：[seq:8 BE][rid_len:1][rid][payload]。
/// seq 放头部便于续传回放排序；rid 自包含路由（回程 cid 前缀）。
fn encode_bin_frame(rid: &str, _meta: &[u8], payload: &[u8], seq: u64) -> Vec<u8> {
    let rid_bytes = rid.as_bytes();
    let mut out = Vec::with_capacity(8 + 1 + rid_bytes.len() + payload.len());
    out.extend_from_slice(&seq.to_be_bytes());
    out.push(rid_bytes.len() as u8);
    out.extend_from_slice(rid_bytes);
    out.extend_from_slice(payload);
    out
}

/// 解析二进制载荷帧：返回 (seq, rid, payload)
fn decode_bin_frame(bin: &[u8]) -> Option<(u64, &str, &[u8])> {
    if bin.len() < 9 {
        return None;
    }
    let seq = u64::from_be_bytes(bin[0..8].try_into().ok()?);
    let rid_len = bin[8] as usize;
    if bin.len() < 9 + rid_len {
        return None;
    }
    let rid = std::str::from_utf8(&bin[9..9 + rid_len]).ok()?;
    Some((seq, rid, &bin[9 + rid_len..]))
}

fn reject_msg(code: &str) -> String {
    tracing::warn!(reject = %code, "拒绝连接");
    json_text(&json!({ "type": "reject", "code": code }))
}

/// 限流拒绝：附 retryAfterSecs 提示（客户端据此退避，防 1/s 重连风暴把窗口喂成活锁）
fn reject_rate_limited(retry_after_secs: i64) -> String {
    tracing::warn!(reject = "rate-limited", retryAfterSecs = retry_after_secs, "拒绝连接");
    json_text(&json!({ "type": "reject", "code": "rate-limited", "retryAfterSecs": retry_after_secs }))
}

fn bye_msg(code: &str) -> String {
    json_text(&json!({ "type": "bye", "code": code }))
}

/// 启动 WS 数据面（常驻 accept 循环）。listener 由调用方先 bind——bind 失败
/// 属启动错误，必须让进程退出来（在 spawn 里 expect 会静默半死）。
pub async fn serve(addr: String, listener: TcpListener, state: AppState) {
    tracing::info!(
        %addr,
        auth = %state.config.ws.auth_mode,
        "ws 数据面已启动（dsh-relay-v1）"
    );
    let rate = std::sync::Arc::new(RateLimiter::new());
    let conn_sem = Arc::new(Semaphore::new(MAX_CONNS_GLOBAL));
    let per_ip: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                // 全局并发上限：满员直接丢弃（不 spawn 任务，accept 循环继续）
                let Ok(permit) = conn_sem.clone().try_acquire_owned() else {
                    tracing::warn!(peer = %peer, "全局连接数已满，拒绝连接");
                    drop(stream);
                    continue;
                };
                let ip = peer.ip().to_string();
                // 每 IP 并发上限
                {
                    let mut m = match per_ip.lock() {
                        Ok(m) => m,
                        Err(_) => {
                            drop(stream);
                            continue;
                        }
                    };
                    let c = m.entry(ip.clone()).or_insert(0);
                    if *c >= MAX_CONNS_PER_IP {
                        tracing::warn!(peer = %peer, "单 IP 连接数已满，拒绝连接");
                        drop(stream);
                        continue;
                    }
                    *c += 1;
                }
                let st = state.clone();
                let rt = rate.clone();
                let pi = per_ip.clone();
                tokio::spawn(async move {
                    let _guard = IpGuard { per_ip: pi, ip };
                    let _permit = permit;
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

    // 握手即启用 TCP_NODELAY：WS 帧化协议、帧都小（隧道帧/下载块 ≤64KiB），
    // Nagle 攒包只加延迟不省带宽
    let _ = stream.set_nodelay(true);

    // 子协议协商：客户端带 dsh-relay-v1 时回选；消息上限 4MiB（默认 64MiB 太宽，
    // 未鉴权阶段一条巨帧即可吃内存）
    let ws: WsStream = tokio_tungstenite::accept_hdr_async_with_config(
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
        Some(WebSocketConfig {
            max_message_size: Some(MAX_MESSAGE_SIZE),
            max_frame_size: Some(MAX_MESSAGE_SIZE),
            ..Default::default()
        }),
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
    // v3 可选能力：断点续传（resumeFrom=已处理最大 seq）与批量帧接收（batch）
    let resume_from = hv.get("resumeFrom").and_then(|v| v.as_u64());
    let want_batch = hv.get("batch").and_then(|v| v.as_bool()).unwrap_or(false);
    let continuity = resume_from.is_some();
    // 限速只拦「猜凭据」路径（配对码/无凭据/旧共享码）；已注册设备的令牌重连放行——
    // 否则同 IP 的失败风暴（负向探测/多套件连跑/同 NAT）会把有效设备一起饿死成活锁。
    // 令牌 128-bit 熵不可暴力枚举，其失败照样计数（fail）。
    // blocked 对带令牌连接同样生效：成功连接会 reset（rate.reset），正常设备不会被
    // 历史失败误伤；而假 token 风暴若绕开检查则每连接都打 DB，构成廉价资源攻击面
    if rate.blocked(&ip) {
        let _ = sink
            .send(Message::Text(reject_rate_limited(rate.retry_after(&ip))))
            .await;
        return Err("rate-limited".into());
    }
    // code 兼作寻址/共享码：带令牌或绑房间的配对码时可省略（room 以设备记录为准）
    let mut room = if code.is_empty() {
        String::new()
    } else {
        room_of(code)
    };

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
                    room = dev.room.clone();
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
                    room = dev.room.clone();
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
        // code 模式：共享码即密钥（旧客户端兼容），必须带 code
        if code.len() < 6 {
            let _ = sink.send(Message::Text(reject_msg("bad-code"))).await;
            return Err("bad-code".into());
        }
        device_id = format!("u{}", room);
    }
    rate.reset(&ip);
    // 环境登记（幂等；管理台环境视图展示）
    let _ = relay_service::room::touch(&state, &room).await;

    // ---- 入房 ----
    let (tx, mut rx) = mpsc::channel::<WsOut>(256);
    let join = state.hub.join(&room, role.str(), &device_id, tx);

    let mut welcome = json!({
        "type": "welcome",
        "role": role_s,
        "peerOnline": join.peer_online,
        "clients": join.clients,
        "room": room,
        "auth": auth_tag,
        "batch": true,
    });
    if let Some((id, token)) = pair_issued.as_ref() {
        welcome["device"] = json!({ "id": id, "token": token });
    }
    if sink.send(Message::Text(json_text(&welcome))).await.is_err() {
        state.hub.leave(join.id);
        return Err("send welcome failed".into());
    }

    // ---- 续传判定 + 断点回放（同任务直发；活帧在 rx 排队，先后序有保证） ----
    if let Some(since) = resume_from {
        let dest = if role_s == "client" {
            buf_key_client(&room, &join.cid)
        } else {
            buf_key_host(&room)
        };
        let (verdict, replay_frames) = match state.hub.buf_replay(&dest, since) {
            Replay::Frames(frames) => (
                json!({ "type": "resume", "ok": true, "count": frames.len() }),
                frames,
            ),
            Replay::Reset => (
                json!({ "type": "resume", "ok": false, "reason": "reset" }),
                Vec::new(),
            ),
        };
        if sink.send(Message::Text(json_text(&verdict))).await.is_err() {
            state.hub.leave(join.id);
            return Err("send resume failed".into());
        }
        for (bytes, is_binary) in replay_frames {
            let msg = if is_binary {
                Message::Binary(bytes)
            } else {
                match String::from_utf8(bytes) {
                    Ok(t) => Message::Text(t.into()),
                    Err(_) => continue,
                }
            };
            if sink.send(msg).await.is_err() {
                state.hub.leave(join.id);
                return Err("send replay failed".into());
            }
        }
    }

    // 被顶替连接踢线（host 一房一岗 / client 同设备重连）
    if let Some(old_tx) = join.replaced {
        let _ = old_tx.try_send(WsOut::Text(bye_msg("superseded")));
        let _ = old_tx.try_send(WsOut::Close);
    }
    // 对端上线通知（peer 帧）由 hub.join 内部下发

    relay_service::audit::record_queued(
        &state,
        "conn.open",
        &ip,
        Some(&device_id),
        relay_service::room::owner_of(&state, &room).await,
        json!({ "role": role_s, "room": room, "auth": auth_tag }),
    )
    .await;
    state.metrics.bump(&state.metrics.ws_connects);

    // ---- 转发循环 ----
    let mut last_activity = now_secs();
    let mut tick = tokio::time::interval(PING_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut done = false;
    // 出站积压字节预算：慢端收不动时队列水位上涨，超预算主动踢线（防一个慢端拖死通道）
    let mut out_bytes: usize = 0;

    while !done {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(m)) => {
                        last_activity = now_secs();
                        match m {
                            Message::Text(text) => {
                                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                                    // batch 信封展开（发送端合并的小帧风暴；envelope 内为 JSON 字符串）
                                    let frames: Vec<Value> = if v.get("type").and_then(|t| t.as_str()) == Some("batch") {
                                        v.get("frames").and_then(|f| f.as_array())
                                            .map(|a| a.iter()
                                                .filter_map(|x| x.as_str())
                                                .filter_map(|s| serde_json::from_str::<Value>(s).ok())
                                                .collect())
                                            .unwrap_or_default()
                                    } else {
                                        vec![v]
                                    };
                                    for mut v in frames {
                                    state.metrics.bump(&state.metrics.frames_in);
                                    match v.get("type").and_then(|t| t.as_str()) {
                                        Some("hello") | Some("welcome") | Some("reject") | Some("bye") | Some("resume") => {}
                                        Some("ping") => {
                                            let _ = sink.send(Message::Text(json_text(&json!({ "type": "pong" })))).await;
                                        }
                                        Some(t) if matches!(t, "http-req" | "http-res" | "ws-open" | "ws-frame" | "ws-close" | "error") => {
                                            if t == "http-req" {
                                                state.metrics.bump(&state.metrics.http_proxied);
                                            }
                                            // rid 回程路由：client→host 加 cid 前缀；host→client 拆前缀定向
                                            // v3：seq 注入 + 每目的地落环（断点回放续传）
                                            let rid = v.get("rid").and_then(|r| r.as_str()).unwrap_or("").to_string();
                                            if role_s == "client" {
                                                let dest = buf_key_host(&room);
                                                v["rid"] = json!(format!("{}.{}", join.cid, rid));
                                                // seq 分配+注入+落环单锁原子（防并发交错判洞误报）
                                                let (_seq, bytes) = state.hub.buf_alloc_push_with(&dest, false, |seq| {
                                                    v["seq"] = json!(seq);
                                                    v.to_string().into_bytes()
                                                });
                                                let out = String::from_utf8(bytes).unwrap_or_default();
                                                // 背压：try_send 不等慢端——队列满即放弃本次直发
                                                //（续传模式下帧已在环里，重连回放补；老端报 peer-offline）
                                                match state.hub.host_tx(&room) {
                                                    Some(host_tx) if host_tx.try_send(WsOut::Text(out)).is_ok() => {
                                                        state.metrics.bump(&state.metrics.frames_relayed);
                                                    }
                                                    _ => {
                                                        // 续传模式下静默缓冲等 host 回放；老端保留快速失败
                                                        if !continuity {
                                                            let _ = sink.send(Message::Text(json_text(&json!({
                                                                "type": "error", "rid": rid,
                                                                "code": "peer-offline", "message": "host offline"
                                                            })))).await;
                                                        }
                                                    }
                                                }
                                            } else {
                                                // host→client：rid 形如 "cid.orig"；无前缀时单 client 兼容直达
                                                let (cid, orig) = match rid.split_once('.') {
                                                    Some((c, o)) => (Some(c.to_string()), o.to_string()),
                                                    None => state.hub.sole_client(&room)
                                                        .map(|(c, _)| (Some(c), rid.clone()))
                                                        .unwrap_or((None, rid.clone())),
                                                };
                                                match cid {
                                                    Some(cid) => {
                                                        let dest = buf_key_client(&room, &cid);
                                                        v["rid"] = json!(orig);
                                                        let (_seq, bytes) = state.hub.buf_alloc_push_with(&dest, false, |seq| {
                                                            v["seq"] = json!(seq);
                                                            v.to_string().into_bytes()
                                                        });
                                                        let out = String::from_utf8(bytes).unwrap_or_default();
                                                        if let Some(ctx) = state.hub.client_tx(&room, &cid) {
                                                            // 队列满 = 慢端：丢直发（环里已有，重连回放补）
                                                            if ctx.try_send(WsOut::Text(out)).is_ok() {
                                                                state.metrics.bump(&state.metrics.frames_relayed);
                                                            } else {
                                                                tracing::warn!(room = %room, rid = %rid, "client 出站满，丢弃直发（回放兜底）");
                                                            }
                                                        }
                                                    }
                                                    None => {
                                                        tracing::warn!(room = %room, rid = %rid, "host 帧 rid 无前缀且多 client，丢弃");
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                    }
                                }
                            }
                            Message::Binary(bin) => {
                                // 二进制载荷帧（下载块零膨胀）：[seq:8][rid_len:1][rid][payload]，自包含路由
                                // seq 注入 + 落环续传；rid 复用元数据帧回程路由（cid 前缀）。
                                let Some((_old_seq, rid, payload)) = decode_bin_frame(&bin) else { continue };
                                if role_s == "client" {
                                    let dest = buf_key_host(&room);
                                    // seq 分配+构帧+落环单锁原子；返回字节与入环同一份（免 clone）
                                    let (_seq, full) = state.hub.buf_alloc_push_with(&dest, true, |seq| {
                                        encode_bin_frame(&format!("{}.{}", join.cid, rid), &[], payload, seq)
                                    });
                                    if let Some(host_tx) = state.hub.host_tx(&room) {
                                        if host_tx.try_send(WsOut::Binary(full)).is_ok() {
                                            state.metrics.bump(&state.metrics.frames_relayed);
                                        } else {
                                            tracing::warn!(room = %room, rid = %rid, "host 出站满，丢弃直发（回放兜底）");
                                        }
                                    }
                                } else {
                                    let (cid, orig) = match rid.split_once('.') {
                                        Some((c, o)) => (Some(c.to_string()), o.to_string()),
                                        None => state.hub.sole_client(&room).map(|(c, _)| (Some(c), rid.to_string())).unwrap_or((None, rid.to_string())),
                                    };
                                    if let Some(cid) = cid {
                                        let dest = buf_key_client(&room, &cid);
                                        let (_seq, full) = state.hub.buf_alloc_push_with(&dest, true, |seq| {
                                            encode_bin_frame(&orig, &[], payload, seq)
                                        });
                                        if let Some(ctx) = state.hub.client_tx(&room, &cid) {
                                            if ctx.try_send(WsOut::Binary(full)).is_ok() {
                                                state.metrics.bump(&state.metrics.frames_relayed);
                                            } else {
                                                tracing::warn!(room = %room, rid = %rid, "client 出站满，丢弃直发（回放兜底）");
                                            }
                                        }
                                    }
                                }
                            }
                            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                            Message::Close(_) => done = true,
                        }
                    }
                    Some(Err(_)) | None => done = true,
                }
            }
            out = rx.recv() => {
                match out {
                    Some(WsOut::Text(t)) => {
                        out_bytes += t.len();
                        // 同刻积压合并成 batch 信封（仅对声明 batch 的对端；不加延迟）
                        let mut texts = vec![t];
                        let mut pending_bin: Vec<Vec<u8>> = Vec::new();
                        let mut close_after = false;
                        if want_batch {
                            while texts.len() < 32 {
                                match rx.try_recv() {
                                    Ok(WsOut::Text(t)) => { out_bytes += t.len(); texts.push(t); }
                                    Ok(WsOut::Binary(b)) => { out_bytes += b.len(); pending_bin.push(b); break; }
                                    Ok(WsOut::Close) => { close_after = true; break; }
                                    Err(_) => break,
                                }
                            }
                        }
                        // 慢端保护：出站积压超预算 → bye{overflow} 踢线（与 README 协议对齐）
                        if out_bytes > OUT_BYTES_MAX {
                            tracing::warn!(device = %device_id, bytes = out_bytes, "出站积压超限，overflow 踢线");
                            let _ = sink.send(Message::Text(bye_msg("overflow"))).await;
                            let _ = sink.send(Message::Close(None)).await;
                            done = true;
                            continue;
                        }
                        let msg = if texts.len() > 1 {
                            Message::Text(json_text(&json!({ "type": "batch", "frames": texts })))
                        } else {
                            Message::Text(texts.remove(0))
                        };
                        if sink.send(msg).await.is_err() {
                            done = true;
                        } else {
                            out_bytes = out_bytes.saturating_sub(texts.iter().map(|t| t.len()).sum());
                            for b in pending_bin {
                                let n = b.len();
                                if sink.send(Message::Binary(b)).await.is_err() {
                                    done = true;
                                    break;
                                } else {
                                    out_bytes = out_bytes.saturating_sub(n);
                                }
                            }
                            if !done && close_after {
                                let _ = sink.send(Message::Close(None)).await;
                                done = true;
                            }
                        }
                    }
                    Some(WsOut::Binary(b)) => {
                        // 二进制载荷帧逐帧发（batch 只合并文本元数据）
                        let n = b.len();
                        out_bytes += n;
                        if sink.send(Message::Binary(b)).await.is_err() {
                            done = true;
                        } else {
                            out_bytes = out_bytes.saturating_sub(n);
                            if out_bytes > OUT_BYTES_MAX {
                                tracing::warn!(device = %device_id, bytes = out_bytes, "出站积压超限，overflow 踢线");
                                let _ = sink.send(Message::Text(bye_msg("overflow"))).await;
                                let _ = sink.send(Message::Close(None)).await;
                                done = true;
                            }
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
    // 对端离线/在线数通知由 hub.leave 内部下发
    state.hub.leave(join.id);
    relay_service::audit::record_queued(
        &state,
        "conn.close",
        &ip,
        Some(&device_id),
        relay_service::room::owner_of(&state, &room).await,
        json!({ "role": role.str(), "room": room }),
    )
    .await;
    Ok(())
}
// ---- 单测：二进制帧编解码 / 失败限流（续传环测试在 common::hub） ----
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_frame_roundtrip() {
        let frame = encode_bin_frame("ab", &[], b"payload", 7);
        let (seq, rid, payload) = decode_bin_frame(&frame).unwrap();
        assert_eq!(seq, 7);
        assert_eq!(rid, "ab");
        assert_eq!(payload, b"payload");
    }

    #[test]
    fn bin_frame_decode_rejects_truncated() {
        let frame = encode_bin_frame("abc", &[], b"xy", 42);
        // 头部不足 9 字节
        assert!(decode_bin_frame(&frame[..8]).is_none());
        // rid 声明 3 字节但只剩 2
        assert!(decode_bin_frame(&frame[..9 + 2]).is_none());
        // 空 rid 合法（载荷为空）
        let empty = encode_bin_frame("", &[], &[], 0);
        let (seq, rid, payload) = decode_bin_frame(&empty).unwrap();
        assert_eq!((seq, rid, payload), (0, "", &[] as &[u8]));
    }

    #[test]
    fn rate_limiter_blocks_after_max_fails_then_resets() {
        let rl = RateLimiter::new();
        assert!(!rl.blocked("1.2.3.4"));
        for _ in 0..MAX_FAILS {
            rl.fail("1.2.3.4");
        }
        assert!(rl.blocked("1.2.3.4"));
        // 其它 IP 不受牵连
        assert!(!rl.blocked("5.6.7.8"));
        // 认证成功即清零（token 连接重试恢复）
        rl.reset("1.2.3.4");
        assert!(!rl.blocked("1.2.3.4"));
    }
}
