//! 路由装配（对齐 byte-admin 的 router::build）：
//! 公开组（登录前流程，紧窗口限速）+ 受保护组（JWT + 操作日志）

use axum::routing::{delete, get, post};
use axum::Router;
use relay_common::AppState;

use crate::handlers;

pub fn build(state: AppState) -> Router {
    // 认证公开组：登录/刷新整体限速（窗口内失败达上限 → 429）
    let auth_public = Router::new()
        .route("/api/auth/login", post(handlers::auth::login))
        .route("/api/auth/refresh", post(handlers::auth::refresh))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            relay_middleware::rate_limit::rate_limit_login,
        ));

    let mut public = Router::new()
        .route("/", get(handlers::page::console))
        .route("/api/health", get(handlers::health::simple))
        .route("/api/health/detail", get(handlers::health::detail))
        .merge(auth_public);
    if state.config.server.swagger {
        public = public.route("/api/openapi.json", get(handlers::openapi::spec));
    }

    // 受保护组：JWT 认证（内层）→ 操作日志（外层）→ 处理器
    let protected = Router::new()
        .route("/api/auth/logout", post(handlers::auth::logout))
        .route("/api/auth/profile", get(handlers::auth::profile))
        .route("/api/auth/password", post(handlers::auth::change_password))
        .route("/api/status", get(handlers::status::overview))
        .route("/api/devices", get(handlers::devices::list))
        .route("/api/devices/{id}/revoke", post(handlers::devices::revoke))
        .route("/api/devices/{id}/rotate", post(handlers::devices::rotate))
        .route("/api/devices/{id}", delete(handlers::devices::remove))
        .route("/api/pairing-codes", post(handlers::pairing::issue))
        .route("/api/audit", get(handlers::audit::tail))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            relay_middleware::oplog::oplog,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            relay_middleware::admin_auth::admin_auth,
        ));

    public
        .merge(protected)
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}
