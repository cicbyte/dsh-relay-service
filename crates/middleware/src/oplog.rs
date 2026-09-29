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

    let res = next.run(req).await;

    if method != "GET" {
        let user = res
            .extensions()
            .get::<AdminIdentity>()
            .map(|i| i.username.clone())
            .unwrap_or_default();
        relay_service::audit::record(
            &state,
            "admin.op",
            &ip,
            None,
            serde_json::json!({
                "method": method,
                "path": path,
                "status": res.status().as_u16(),
                "user": user,
            }),
        )
        .await;
    }
    res
}
