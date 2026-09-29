use serde::Serialize;

/// 管理端身份（由认证中间件注入请求/响应扩展，对齐 byte-admin 的 AdminIdentity）
#[derive(Debug, Clone, Serialize)]
pub struct AdminIdentity {
    pub user_id: i64,
    pub username: String,
}
