//! WS 连接枢纽：房间（room=hex8，一 host : N client）+ 连接登记 + 路由/踢线/在线统计。
//! - host 一房一个：后到踢先到（superseded）
//! - client 多台共存：仅「同一设备」重连挤占自己的旧连接，不同设备互不影响
//! - cid 为设备稳定短标识（stable_cid）：host→client 的帧以 `cid.` 前缀重写 rid 做回程路由
//!   （rid 对桥/手机不透明，wire 协议零改动）；跨重连不变是隧道续传的前提
//! - 每目的地环形缓冲（buf_*）：隧道帧带 seq 落环，hello.resumeFrom 断点回放（续传）
//! 数据面转发逻辑在 apps/relay/ws.rs，这里只做连接拓扑、路由与续传缓冲。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::mpsc;

/// 出站消息（与 tungstenite 解耦：ws.rs 负责适配成 Message）
#[derive(Debug)]
pub enum WsOut {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

pub type WsTx = mpsc::Sender<WsOut>;

/// 帧载荷（统一字节模型）：文本/二进制都存字节，发时按 is_binary 适配 Message::Text/Binary
pub type FramePayload = (Vec<u8>, bool);

/// 续传回放判定
pub enum Replay {
    /// 断点之后的帧（按 seq 升序）；每项 (bytes, is_binary)
    Frames(Vec<FramePayload>),
    /// 断点不可满足（环已绕回/服务重启）：端点应放弃旧流重建
    Reset,
}

/// 稳定 cid：设备维度跨重连不变（FNV-1a 64 → 16 hex）
pub fn stable_cid(device_id: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in device_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// 续传缓冲键：host 目的地
pub fn buf_key_host(room: &str) -> String {
    format!("{room}\u{0}h")
}

/// 续传缓冲键：client 目的地（cid 稳定）
pub fn buf_key_client(room: &str, cid: &str) -> String {
    format!("{room}\u{0}c\u{0}{cid}")
}

const BUF_MAX_FRAMES: usize = 256;
const BUF_MAX_BYTES: usize = 128 * 1024;
const BUF_TTL_SECS: i64 = 30;

struct SeqRing {
    /// (seq, ts, bytes, is_binary)；seq 从 1 起连续分配
    items: VecDeque<(u64, i64, Vec<u8>, bool)>,
    bytes: usize,
    next_seq: u64,
}

impl SeqRing {
    fn new() -> Self {
        Self {
            items: VecDeque::new(),
            bytes: 0,
            next_seq: 1,
        }
    }

    fn evict(&mut self, now: i64) {
        while let Some(&(seq, ts, _, _)) = self.items.front() {
            let over = self.items.len() > BUF_MAX_FRAMES
                || self.bytes > BUF_MAX_BYTES
                || now - ts > BUF_TTL_SECS;
            if !over {
                break;
            }
            if let Some((_, _, f, _)) = self.items.pop_front() {
                self.bytes -= f.len();
            }
            let _ = seq;
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct JoinOutcome {
    pub id: u64,
    /// 连接短标识（rid 回程路由前缀）：设备稳定，跨重连不变
    pub cid: String,
    /// 对端在线：client 看 host；host 看是否有 client
    pub peer_online: bool,
    /// 入房后房内 client 数
    pub clients: usize,
    /// 被顶替连接的出站句柄（host 恒一房一岗；client 仅同设备重连时出现）
    pub replaced: Option<WsTx>,
}

struct Slots {
    host: Option<u64>,
    /// cid → conn id
    clients: HashMap<String, u64>,
}

struct Conn {
    room: String,
    role: String,
    device_id: String,
    tx: WsTx,
}

#[derive(Default)]
pub struct WsHub {
    next_id: AtomicU64,
    conns: Mutex<HashMap<u64, Conn>>,
    rooms: Mutex<HashMap<String, Slots>>,
    /// 续传缓冲：dest key → 环形帧队列
    bufs: Mutex<HashMap<String, SeqRing>>,
}

fn peer_msg(online: bool, clients: usize) -> WsOut {
    WsOut::Text(
        serde_json::json!({ "type": "peer", "online": online, "clients": clients }).to_string(),
    )
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            conns: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
            bufs: Mutex::new(HashMap::new()),
        }
    }

    /// 入住房间。room 为空串时按连接隔离（不进房间、不参与转发）。
    /// 对端上线通知（peer 帧）由本方法直接下发。
    pub fn join(&self, room: &str, role: &str, device_id: &str, tx: WsTx) -> JoinOutcome {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // 设备维度稳定 cid（无设备标识的连接退回每连接随机）
        let cid = if device_id.is_empty() {
            format!("{:04x}{:02x}", id & 0xffff, (id >> 16) & 0xff)
        } else {
            stable_cid(device_id)
        };
        self.conns.lock().unwrap().insert(
            id,
            Conn {
                room: room.to_string(),
                role: role.to_string(),
                device_id: device_id.to_string(),
                tx,
            },
        );
        if room.is_empty() {
            return JoinOutcome {
                id,
                cid,
                peer_online: false,
                clients: 0,
                replaced: None,
            };
        }

        let mut replaced_tx = None;
        let (peer_online, clients, host_tx, client_txs) = {
            let mut rooms = self.rooms.lock().unwrap();
            let slots = rooms.entry(room.to_string()).or_insert(Slots {
                host: None,
                clients: HashMap::new(),
            });
            let conns = self.conns.lock().unwrap();
            if role == "host" {
                // host 一房一岗：后到踢先到
                if let Some(old_id) = slots.host.replace(id) {
                    if let Some(c) = conns.get(&old_id) {
                        replaced_tx = Some(c.tx.clone());
                    }
                }
            } else {
                // client 多台共存：仅同设备重连挤占自己
                if !device_id.is_empty() {
                    let old = slots
                        .clients
                        .iter()
                        .find(|&(_, conn_id)| {
                            conns
                                .get(conn_id)
                                .map(|c| c.device_id == device_id)
                                .unwrap_or(false)
                        })
                        .map(|(k, _)| k.clone());
                    if let Some(old_key) = old {
                        if let Some(old_id) = slots.clients.remove(&old_key) {
                            if let Some(c) = conns.get(&old_id) {
                                replaced_tx = Some(c.tx.clone());
                            }
                        }
                    }
                }
                slots.clients.insert(cid.clone(), id);
            }
            let host_tx = slots
                .host
                .and_then(|hid| conns.get(&hid).map(|c| c.tx.clone()));
            let client_txs: Vec<WsTx> = slots
                .clients
                .values()
                .filter_map(|cid_id| conns.get(cid_id).map(|c| c.tx.clone()))
                .collect();
            let clients = slots.clients.len();
            (
                if role == "host" {
                    clients > 0
                } else {
                    host_tx.is_some()
                },
                clients,
                host_tx,
                if role == "host" { client_txs } else { Vec::new() },
            )
        };

        // 对端上线通知：client 入房 → 告知 host 在线数；host 入房 → 告知各 client
        if role == "client" {
            if let Some(htx) = host_tx {
                let _ = htx.try_send(peer_msg(true, clients));
            }
        } else {
            for ctx in client_txs {
                let _ = ctx.try_send(peer_msg(true, clients));
            }
        }

        JoinOutcome {
            id,
            cid,
            peer_online,
            clients,
            replaced: replaced_tx,
        }
    }

    /// 离房；对端离线/在线数更新通知由本方法直接下发。
    pub fn leave(&self, id: u64) {
        let conn = self.conns.lock().unwrap().remove(&id);
        let Some(conn) = conn else { return };
        if conn.room.is_empty() {
            return;
        }
        let mut rooms = self.rooms.lock().unwrap();
        let Some(slots) = rooms.get_mut(&conn.room) else { return };
        let conns = self.conns.lock().unwrap();
        if conn.role == "host" {
            if slots.host == Some(id) {
                slots.host = None;
            }
            // host 下线 → 全部 client 标记对端离线
            for ctx in slots.clients.values().filter_map(|cid_id| conns.get(cid_id).map(|c| c.tx.clone())) {
                let _ = ctx.try_send(peer_msg(false, 0));
            }
        } else {
            slots.clients.retain(|_, conn_id| *conn_id != id);
            let clients = slots.clients.len();
            if let Some(htx) = slots.host.and_then(|hid| conns.get(&hid).map(|c| c.tx.clone())) {
                let _ = htx.try_send(peer_msg(clients > 0, clients));
            }
        }
        drop(conns);
        // 空房清理（host 或最后一个 client 离房）
        let empty = rooms
            .get(&conn.room)
            .map(|s| s.host.is_none() && s.clients.is_empty())
            .unwrap_or(true);
        if empty {
            rooms.remove(&conn.room);
        }
    }

    /// client→host 路由目标
    pub fn host_tx(&self, room: &str) -> Option<WsTx> {
        if room.is_empty() {
            return None;
        }
        let rooms = self.rooms.lock().unwrap();
        let conns = self.conns.lock().unwrap();
        rooms
            .get(room)?
            .host
            .and_then(|hid| conns.get(&hid).map(|c| c.tx.clone()))
    }

    /// host→client 路由目标（cid 来自 rid 前缀）
    pub fn client_tx(&self, room: &str, cid: &str) -> Option<WsTx> {
        if room.is_empty() {
            return None;
        }
        let rooms = self.rooms.lock().unwrap();
        let conns = self.conns.lock().unwrap();
        rooms
            .get(room)?
            .clients
            .get(cid)
            .and_then(|cid_id| conns.get(cid_id).map(|c| c.tx.clone()))
    }

    /// 房内全部 client 的 cid（rid 无前缀时的兼容兜底：单 client 直达）
    pub fn sole_client(&self, room: &str) -> Option<(String, WsTx)> {
        let rooms = self.rooms.lock().unwrap();
        let conns = self.conns.lock().unwrap();
        let slots = rooms.get(room)?;
        if slots.clients.len() != 1 {
            return None;
        }
        let (cid, conn_id) = slots.clients.iter().next()?;
        conns.get(conn_id).map(|c| (cid.clone(), c.tx.clone()))
    }

    /// 踢掉某设备全部在线连接，返回是否踢到
    pub fn kick_device(&self, device_id: &str) -> usize {
        let targets: Vec<WsTx> = self
            .conns
            .lock()
            .unwrap()
            .values()
            .filter(|c| !c.device_id.is_empty() && c.device_id == device_id)
            .map(|c| c.tx.clone())
            .collect();
        let n = targets.len();
        for tx in targets {
            let _ = tx.try_send(WsOut::Text(
                serde_json::json!({ "type": "bye", "code": "revoked" }).to_string(),
            ));
            let _ = tx.try_send(WsOut::Close);
        }
        n
    }

    /// 踢掉某房间全部在线连接（删环境用），返回踢到的连接数
    pub fn kick_room(&self, room: &str) -> usize {
        let targets: Vec<WsTx> = self
            .conns
            .lock()
            .unwrap()
            .values()
            .filter(|c| c.room == room)
            .map(|c| c.tx.clone())
            .collect();
        let n = targets.len();
        for tx in targets {
            let _ = tx.try_send(WsOut::Text(
                serde_json::json!({ "type": "bye", "code": "revoked" }).to_string(),
            ));
            let _ = tx.try_send(WsOut::Close);
        }
        n
    }

    pub fn device_online(&self, device_id: &str) -> bool {
        self.conns
            .lock()
            .unwrap()
            .values()
            .any(|c| c.device_id == device_id)
    }

    /// (rooms, 在线 host 数, 在线 client 数)
    pub fn stats(&self) -> (usize, usize, usize) {
        let rooms = self.rooms.lock().unwrap();
        let hosts = rooms.values().filter(|s| s.host.is_some()).count();
        let clients = rooms.values().map(|s| s.clients.len()).sum();
        (rooms.len(), hosts, clients)
    }

    /// 限定房间集合的 (rooms, hosts, clients)（多用户概览按归属裁剪）
    pub fn stats_scoped(
        &self,
        owned: &std::collections::HashSet<String>,
    ) -> (usize, usize, usize) {
        let conns = self.conns.lock().unwrap();
        let mut rooms = std::collections::HashSet::new();
        let mut hosts = 0usize;
        let mut clients = 0usize;
        for c in conns.values() {
            if !owned.contains(&c.room) {
                continue;
            }
            rooms.insert(c.room.clone());
            if c.role == "host" {
                hosts += 1;
            } else {
                clients += 1;
            }
        }
        (rooms.len(), hosts, clients)
    }

    // ---- 续传缓冲（隧道帧 seq 落环，断点回放） ----

    /// 分配下一 seq（帧内注入后调 buf_push 落环）
    pub fn buf_alloc(&self, dest: &str) -> u64 {
        let mut bufs = self.bufs.lock().unwrap();
        let ring = bufs.entry(dest.to_string()).or_insert_with(SeqRing::new);
        let seq = ring.next_seq;
        ring.next_seq += 1;
        seq
    }

    /// 帧落环（调用方负责 seq 与帧内容一致）
    pub fn buf_push(&self, dest: &str, seq: u64, frame: Vec<u8>, is_binary: bool) {
        let mut bufs = self.bufs.lock().unwrap();
        let ring = bufs.entry(dest.to_string()).or_insert_with(SeqRing::new);
        let now = now_secs();
        ring.bytes += frame.len();
        ring.items.push_back((seq, now, frame, is_binary));
        ring.evict(now);
    }

    /// 断点回放：since=端点已处理的最大 seq（0=从未收到）
    pub fn buf_replay(&self, dest: &str, since: u64) -> Replay {
        let mut bufs = self.bufs.lock().unwrap();
        let now = now_secs();
        let Some(ring) = bufs.get_mut(dest) else {
            // 无环：从未发过帧（干净起点）或服务已重启（有历史断点）
            return if since == 0 {
                Replay::Frames(Vec::new())
            } else {
                Replay::Reset
            };
        };
        ring.evict(now);
        let last = ring.next_seq - 1;
        if since > last {
            // 端点比服务还超前：seq 已被重启重置
            return Replay::Reset;
        }
        let frames: Vec<FramePayload> = ring
            .items
            .iter()
            .filter(|(s, _, _, _)| *s > since)
            .map(|(_, _, f, b)| (f.clone(), *b))
            .collect();
        let first = ring.items.iter().find(|(s, _, _, _)| *s > since).map(|(s, _, _, _)| *s);
        match first {
            // 断点之后第一帧必须正好衔接，否则中间有洞（环已绕回）
            Some(s) if s == since + 1 => Replay::Frames(frames),
            None if since == last => Replay::Frames(Vec::new()),
            _ => Replay::Reset,
        }
    }

    /// 房间删除时清掉相关续传缓冲（级联删用户环境用）
    pub fn buf_clear_room(&self, room: &str) {
        let prefix = format!("{room}\u{0}");
        self.bufs
            .lock()
            .unwrap()
            .retain(|k, _| !k.starts_with(&prefix));
    }
}
