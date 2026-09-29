//! 操作日志（oplog）中间件：受保护组的写操作落审计表（操作人/方法/路径/状态码）

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use relay_common::identity::AdminIdentity;
use relay_common::AppState;

pub async fn oplog(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let ip = crate::rate_limit::client_ip(&req, state.config.server.trust_proxy);
    // 身份在请求扩展（外层 admin_auth 注入）；不要读响应扩展——那个在本中间件返回后才写
    let identity = req.extensions().get::<AdminIdentity>().cloned();

    let res = next.run(req).await;

    if method != "GET" {
        relay_service::audit::record(
            &state,
            "admin.op",
            &ip,
            None,
            identity.as_ref().map(|i| i.user_id).unwrap_or(0),
            serde_json::json!({
                "method": method,
                "path": path,
                "status": res.status().as_u16(),
                "user": identity.as_ref().map(|i| i.username.clone()).unwrap_or_default(),
            }),
        )
        .await;
    }
    res
}
