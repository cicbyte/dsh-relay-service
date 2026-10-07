use serde::{Deserialize, Serialize};

/// 应用配置。加载优先级：内置默认 < config/relay.toml（节深合并）< 环境变量
/// （对齐 byte-admin 的 AppConfig::load 模式）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub ws: WsConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtConfig,
    pub rate_limit: RateLimitConfig,
    pub audit: AuditConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// 是否信任反向代理头（X-Forwarded-For / X-Real-IP）。
    /// false（默认）：仅信连接地址——直连暴露时防伪造头绕过限速/污染审计；
    /// true：部署在可信反代（nginx/Caddy）后设置。
    pub trust_proxy: bool,
    /// 是否暴露 OpenAPI 文档（生产建议 false）
    pub swagger: bool,
    /// 管理台前端静态目录（apps/admin-web 的构建产物；缺省时降级为提示页）
    pub static_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WsConfig {
    pub host: String,
    pub port: u16,
    /// device=设备令牌/配对码（默认，fail-closed）| code=旧共享配对码（兼容）
    pub auth_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JwtConfig {
    /// 留空时：环境变量 JWT_SECRET > data/jwt.secret 自动生成
    pub secret: String,
    pub access_ttl_secs: i64,
    pub refresh_ttl_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    pub login_max_fails: u32,
    pub window_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditConfig {
    /// 审计保留天数（0=不清理）
    pub retain_days: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8788,
            trust_proxy: false,
            swagger: true,
            static_dir: "apps/admin-web/dist".to_string(),
        }
    }
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8787,
            auth_mode: "device".to_string(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://data/relay.db?mode=rwc".to_string(),
        }
    }
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            access_ttl_secs: 2 * 3600,
            refresh_ttl_secs: 7 * 24 * 3600,
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            login_max_fails: 10,
            window_secs: 60,
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self { retain_days: 90 }
    }
}

impl AppConfig {
    /// 按应用名加载配置：内置默认 < `{env_prefix}_CONFIG` 或 `config/{app_name}.toml`
    /// （可只写部分字段，按节深合并）< 环境变量。
    pub fn load(app_name: &str, env_prefix: &str, default_port: u16) -> Self {
        let mut cfg = AppConfig::default();
        cfg.server.port = default_port;

        let path = std::env::var(format!("{env_prefix}_CONFIG"))
            .unwrap_or_else(|_| format!("config/{app_name}.toml"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            match toml::from_str::<toml::Value>(&text) {
                Ok(overrides) => cfg = merge(cfg, &overrides),
                Err(err) => {
                    tracing::warn!(path = %path, error = %err, "配置文件解析失败，使用默认配置");
                }
            }
        }

        if let Ok(v) = std::env::var(format!("{env_prefix}_HOST")) {
            cfg.server.host = v;
        }
        if let Ok(v) = std::env::var(format!("{env_prefix}_PORT")) {
            if let Ok(port) = v.parse() {
                cfg.server.port = port;
            }
        }
        if let Ok(v) = std::env::var(format!("{env_prefix}_WS_HOST")) {
            cfg.ws.host = v;
        }
        if let Ok(v) = std::env::var(format!("{env_prefix}_WS_PORT")) {
            if let Ok(port) = v.parse() {
                cfg.ws.port = port;
            }
        }
        if let Ok(v) = std::env::var(format!("{env_prefix}_AUTH_MODE")) {
            cfg.ws.auth_mode = v;
        }
        // 归一化（大小写/空白），配合 main.rs 的 fail-closed 白名单校验
        cfg.ws.auth_mode = cfg.ws.auth_mode.trim().to_ascii_lowercase();
        if let Ok(v) = std::env::var(format!("{env_prefix}_DATABASE_URL")) {
            cfg.database.url = v;
        }
        if let Ok(v) = std::env::var(format!("{env_prefix}_STATIC_DIR")) {
            cfg.server.static_dir = v;
        }
        if let Ok(v) = std::env::var("JWT_SECRET") {
            cfg.jwt.secret = v;
        }
        cfg
    }

    /// `data/jwt.secret` 或配置里的 JWT secret（空则生成并落盘，重启稳定）
    pub fn jwt_secret(&self) -> String {
        if !self.jwt.secret.is_empty() {
            return self.jwt.secret.clone();
        }
        if let Ok(v) = std::env::var("JWT_SECRET") {
            if !v.is_empty() {
                return v;
            }
        }
        let path = std::path::Path::new("data").join("jwt.secret");
        if let Ok(s) = std::fs::read_to_string(&path) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
        let secret = crate::util::rand_hex(24);
        let _ = std::fs::create_dir_all("data");
        let _ = std::fs::write(&path, &secret);
        tracing::info!(path = %path.display(), "已生成 JWT secret");
        secret
    }
}

/// 将 toml 覆盖值深合并进当前配置（表递归合并，标量整体覆盖）。
/// 合并在 serde_json::Value 上进行（对齐 byte-admin 的 merge）。
fn merge(cfg: AppConfig, overrides: &toml::Value) -> AppConfig {
    let (Ok(mut base), Ok(over)) = (serde_json::to_value(&cfg), serde_json::to_value(overrides))
    else {
        return cfg;
    };
    deep_merge(&mut base, &over);
    serde_json::from_value(base).unwrap_or(cfg)
}

fn deep_merge(base: &mut serde_json::Value, over: &serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                if v.is_null() {
                    continue;
                }
                deep_merge(b.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
        (b, o) => *b = o.clone(),
    }
}
