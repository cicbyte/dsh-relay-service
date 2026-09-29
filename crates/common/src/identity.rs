use serde::Serialize;

/// 管理端身份（由认证中间件注入请求/响应扩展，对齐 byte-admin 的 AdminIdentity）
#[derive(Debug, Clone, Serialize)]
pub struct AdminIdentity {
    pub user_id: i64,
    pub username: String,
    /// 角色：admin=全局管理 | user=仅自己的环境（权限矩阵写死，见 service 层钩子）
    pub role: String,
}

impl AdminIdentity {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}
