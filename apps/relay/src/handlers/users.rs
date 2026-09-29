//! 管理台用户管理（仅 admin）：无自助注册，账号由管理员增删/重置密码

use axum::extract::{Path, State};
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::{AppError, AppState, Resp};
use relay_service::users::UserView;
use serde::Deserialize;

pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<Vec<UserView>>>, AppError> {
    relay_service::users::require_admin(&identity)?;
    let out = relay_service::users::list(&state).await?;
    Ok(Json(Resp::ok(out)))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserReq {
    pub username: String,
    pub password: String,
    /// admin | user（权限矩阵写死）
    pub role: String,
}

pub async fn create(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Json(body): Json<CreateUserReq>,
) -> Result<Json<Resp<UserView>>, AppError> {
    relay_service::users::require_admin(&identity)?;
    let out = relay_service::users::create(&state, &body.username, &body.password, &body.role).await?;
    tracing::info!(user = %out.username, role = %out.role, "新建用户");
    Ok(Json(Resp::ok(out)))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordReq {
    pub password: String,
}

pub async fn reset_password(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(id): Path<i64>,
    Json(body): Json<ResetPasswordReq>,
) -> Result<Json<Resp<()>>, AppError> {
    relay_service::users::require_admin(&identity)?;
    relay_service::users::reset_password(&state, id, &body.password).await?;
    tracing::info!(user_id = id, "已重置用户密码");
    Ok(Json(Resp::ok_empty()))
}

pub async fn remove(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<Resp<()>>, AppError> {
    relay_service::users::require_admin(&identity)?;
    relay_service::users::remove(&state, &identity, id).await?;
    tracing::info!(user_id = id, "已删除用户");
    Ok(Json(Resp::ok_empty()))
}
