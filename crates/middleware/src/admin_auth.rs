//! 管理端认证中间件：Bearer JWT（aud=admin）校验，身份注入请求/响应扩展。
//! handler 侧用 `axum::Extension<AdminIdentity>` 提取。

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use relay_common::jwt::extract_token;
use relay_common::{AppError, AppState};

pub async fn admin_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let identity = match extract_token(req.headers()) {
        Some(token) => match relay_service::auth::resolve_admin_token(&state, &token).await {
            Ok(identity) => identity,
            Err(err) => return err.into_response(),
        },
        None => return AppError::unauthorized("未登录或令牌缺失").into_response(),
    };

    req.extensions_mut().insert(identity.clone());
    let mut res = next.run(req).await;
    // 同时写入响应扩展，供外层操作日志中间件读取操作人
    res.extensions_mut().insert(identity);
    res
}
