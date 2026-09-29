//! 登录限速中间件：按 IP 滑动窗口计失败次数（对齐 byte-admin 的紧窗口限速）

use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use relay_common::{AppError, AppState};

/// 公开组（登录/刷新）整体限速：窗口内失败达上限 → 429
pub async fn rate_limit_login(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let ip = client_ip(&req, state.config.server.trust_proxy);
    let max = state.config.rate_limit.login_max_fails;
    let window = state.config.rate_limit.window_secs;

    if state.login_guard.blocked(&ip, max, window) {
        return AppError::too_many_requests("尝试过于频繁，请稍后再试").into_response();
    }
    let res = next.run(req).await;
    if res.status() == StatusCode::UNAUTHORIZED {
        state.login_guard.fail(&ip, window);
    } else {
        state.login_guard.reset(&ip);
    }
    res
}

/// 客户端 IP：trust_proxy 时取 X-Forwarded-For 首段 / X-Real-IP，否则取连接地址
pub fn client_ip(req: &Request, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(v) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = v.split(',').next() {
                let s = first.trim();
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        if let Some(v) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
            if !v.trim().is_empty() {
                return v.trim().to_string();
            }
        }
    }
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<std::net::SocketAddr>>() {
        return addr.ip().to_string();
    }
    "unknown".to_string()
}
