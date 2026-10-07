use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::hub::WsHub;
use crate::identity::AdminIdentity;
use crate::jwt::JwtManager;

/// 全局运行时状态（对齐 byte-admin 的 AppState）
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: AppConfig,
    pub jwt: JwtManager,
    pub hub: Arc<WsHub>,
    pub login_guard: Arc<LoginGuard>,
    /// room→owner 元数据缓存（连接鉴权/审计高频读；安全敏感的令牌校验严禁走缓存）
    pub owner_cache: Arc<OwnerCache>,
    /// 进程内指标（/api/metrics 快照）
    pub metrics: Arc<Metrics>,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: AppConfig) -> Self {
        let jwt = JwtManager::new(
            &config.jwt_secret(),
            config.jwt.access_ttl_secs,
            config.jwt.refresh_ttl_secs,
        );
        Self {
            db,
            config,
            jwt,
            hub: Arc::new(WsHub::new()),
            login_guard: Arc::new(LoginGuard::new()),
            owner_cache: Arc::new(OwnerCache::new()),
            metrics: Arc::new(Metrics::new()),
        }
    }
}

/// 进程内指标：原子计数无锁；进程重启归零（快照型，不做持久化聚合）。
/// 之前只有日志没有数字——「通道最近忙不忙、转发量多大」只能翻日志数行数。
pub struct Metrics {
    /// 进程启动时刻（epoch 秒，uptime 基准）
    pub started_at: i64,
    /// ws 连接建立总数（鉴权成功后计数）
    pub ws_connects: AtomicU64,
    /// 收到的 ws 文本帧数（batch 信封按展开后计）
    pub frames_in: AtomicU64,
    /// 直发到对端的帧数（try_send 成功计；满队列丢弃不计——那是回放兜底的）
    pub frames_relayed: AtomicU64,
    /// http-req 转发次数
    pub http_proxied: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started_at: chrono::Utc::now().timestamp(),
            ws_connects: AtomicU64::new(0),
            frames_in: AtomicU64::new(0),
            frames_relayed: AtomicU64::new(0),
            http_proxied: AtomicU64::new(0),
        }
    }

    pub fn bump(&self, counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(&self, counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
}

/// room→owner_id 缓存：写侧（删环境/级联删用户）主动失效 + 60s TTL 兜底
pub struct OwnerCache {
    map: Mutex<std::collections::HashMap<String, (i64, i64)>>, // room → (owner_id, expires_at)
}

impl OwnerCache {
    const TTL_SECS: i64 = 60;

    pub fn new() -> Self {
        Self {
            map: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    pub fn get(&self, room: &str) -> Option<i64> {
        let m = self.map.lock().unwrap();
        m.get(room)
            .filter(|(_, exp)| *exp > Self::now())
            .map(|(o, _)| *o)
    }

    pub fn put(&self, room: &str, owner_id: i64) {
        self.map
            .lock()
            .unwrap()
            .insert(room.to_string(), (owner_id, Self::now() + Self::TTL_SECS));
    }

    pub fn invalidate(&self, room: &str) {
        self.map.lock().unwrap().remove(room);
    }
}

/// 登录失败限速（按 IP 滑动窗口；对齐 byte-admin 的紧窗口限速思路）
pub struct LoginGuard {
    max_fails: Mutex<std::collections::HashMap<String, (u32, i64)>>,
}

impl LoginGuard {
    pub fn new() -> Self {
        Self {
            max_fails: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    pub fn blocked(&self, ip: &str, max_fails: u32, window_secs: u64) -> bool {
        let mut m = self.max_fails.lock().unwrap();
        let now = Self::now();
        if let Some((_, start)) = m.get(ip) {
            if now - *start > window_secs as i64 {
                m.remove(ip);
            }
        }
        m.get(ip).map(|(n, _)| *n >= max_fails).unwrap_or(false)
    }

    pub fn fail(&self, ip: &str, window_secs: u64) {
        let mut m = self.max_fails.lock().unwrap();
        let now = Self::now();
        let e = m.entry(ip.to_string()).or_insert((0, now));
        if now - e.1 > window_secs as i64 {
            *e = (0, now);
        }
        e.0 += 1;
    }

    pub fn reset(&self, ip: &str) {
        self.max_fails.lock().unwrap().remove(ip);
    }
}

/// 操作日志（oplog）上下文：中间件把操作人写入响应扩展后由 oplog 中间件落库
pub type OpIdentity = AdminIdentity;
