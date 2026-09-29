//! WS 连接枢纽：房间（room=hex8，一房一桥一手机）+ 连接登记 + 踢线/在线统计。
//! 数据面转发逻辑在 apps/relay/ws.rs，这里只做连接拓扑管理。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::mpsc;

/// 出站消息（与 tungstenite 解耦：ws.rs 负责适配成 Message）
#[derive(Debug)]
pub enum WsOut {
    Text(String),
    Close,
}

pub type WsTx = mpsc::Sender<WsOut>;

pub struct JoinOutcome {
    pub id: u64,
    pub peer_online: bool,
    /// 同角色后到踢先到：被顶替连接的出站句柄（调用方负责通知 superseded）
    pub replaced: Option<WsTx>,
}

struct Slots {
    host: Option<u64>,
    client: Option<u64>,
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
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            conns: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
        }
    }

    /// 入住房间。room 为空串时按连接隔离（不进房间、不参与转发）。
    pub fn join(&self, room: &str, role: &str, device_id: &str, tx: WsTx) -> JoinOutcome {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
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
                peer_online: false,
                replaced: None,
            };
        }

        let mut rooms = self.rooms.lock().unwrap();
        let slots = rooms.entry(room.to_string()).or_insert(Slots {
            host: None,
            client: None,
        });
        let slot = if role == "host" {
            &mut slots.host
        } else {
            &mut slots.client
        };
        let replaced = slot
            .take()
            .and_then(|old_id| self.conns.lock().unwrap().get(&old_id).map(|c| c.tx.clone()));
        *slot = Some(id);

        let peer_id = if role == "host" { slots.client } else { slots.host };
        let peer_online = peer_id.is_some();
        JoinOutcome {
            id,
            peer_online,
            replaced,
        }
    }

    /// 离房。返回对端出站句柄（调用方通知 peer offline）。
    pub fn leave(&self, id: u64) -> Option<WsTx> {
        let conn = self.conns.lock().unwrap().remove(&id)?;
        if conn.room.is_empty() {
            return None;
        }
        let mut rooms = self.rooms.lock().unwrap();
        let mut peer = None;
        if let Some(slots) = rooms.get_mut(&conn.room) {
            let slot = if conn.role == "host" {
                &mut slots.host
            } else {
                &mut slots.client
            };
            if *slot == Some(id) {
                *slot = None;
            }
            let peer_id = if conn.role == "host" { slots.client } else { slots.host };
            peer = peer_id.and_then(|pid| self.conns.lock().unwrap().get(&pid).map(|c| c.tx.clone()));
            if slots.host.is_none() && slots.client.is_none() {
                rooms.remove(&conn.room);
            }
        }
        peer
    }

    /// 对端（另一角色）出站句柄
    pub fn peer_of(&self, room: &str, role: &str) -> Option<WsTx> {
        if room.is_empty() {
            return None;
        }
        let rooms = self.rooms.lock().unwrap();
        let slots = rooms.get(room)?;
        let peer_id = if role == "host" { slots.client } else { slots.host };
        peer_id.and_then(|pid| self.conns.lock().unwrap().get(&pid).map(|c| c.tx.clone()))
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
        let clients = rooms.values().filter(|s| s.client.is_some()).count();
        (rooms.len(), hosts, clients)
    }
}
