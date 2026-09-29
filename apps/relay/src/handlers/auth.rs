//! 认证：登录 / 刷新 / 登出 / 个人信息 / 修改密码

use axum::extract::State;
use axum::http::HeaderMap;
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::jwt::extract_token;
use relay_common::{AppError, AppState, Resp};
use relay_service::auth::{LoginOutput, ProfileOutput};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshReq {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordReq {
    pub old_password: String,
    pub new_password: String,
}

/// 登录（受登录限速中间件保护）
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginReq>,
) -> Result<Json<Resp<LoginOutput>>, AppError> {
    let out = relay_service::auth::login(&state, &body.username, &body.password).await?;
    tracing::info!(user = %out.username, "管理端登录");
    Ok(Json(Resp::ok(out)))
}

/// 刷新令牌（nonce 轮转）
pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshReq>,
) -> Result<Json<Resp<LoginOutput>>, AppError> {
    let out = relay_service::auth::refresh(&state, &body.refresh_token).await?;
    Ok(Json(Resp::ok(out)))
}

/// 登出：refresh 立即失效
pub async fn logout(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<()>>, AppError> {
    relay_service::auth::logout(&state, identity.user_id).await?;
    Ok(Json(Resp::ok_empty()))
}

/// 当前登录信息
pub async fn profile(
    Extension(identity): Extension<AdminIdentity>,
) -> Json<Resp<ProfileOutput>> {
    Json(Resp::ok(ProfileOutput {
        user_id: identity.user_id,
        username: identity.username,
        role: identity.role,
    }))
}

/// 修改密码（成功后其他端全部下线）
pub async fn change_password(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordReq>,
) -> Result<Json<Resp<()>>, AppError> {
    let _ = extract_token(&headers);
    relay_service::auth::change_password(
        &state,
        identity.user_id,
        &body.old_password,
        &body.new_password,
    )
    .await?;
    tracing::info!(user = %identity.username, "管理密码已修改");
    Ok(Json(Resp::ok_empty()))
}
