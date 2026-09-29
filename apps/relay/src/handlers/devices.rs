//! 设备管理：列表 / 吊销 / 轮换令牌 / 删除（经环境归属隔离）

use axum::extract::{Path, State};
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::{AppError, AppState, Resp};
use relay_service::device::DeviceView;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RotateOut {
    pub token: String,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<Vec<DeviceView>>>, AppError> {
    let items = relay_service::device::list(&state, &identity).await?;
    Ok(Json(Resp::ok(items)))
}

/// 吊销：立即踢线，令牌作废
pub async fn revoke(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Resp<()>>, AppError> {
    relay_service::device::ensure_device_access(&state, &identity, &id).await?;
    if !relay_service::device::revoke(&state, &id).await? {
        return Err(AppError::not_found("设备不存在"));
    }
    tracing::info!(device = %id, "设备已吊销");
    Ok(Json(Resp::ok_empty()))
}

/// 轮换令牌：新明文只返回一次
pub async fn rotate(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Resp<RotateOut>>, AppError> {
    relay_service::device::ensure_device_access(&state, &identity, &id).await?;
    match relay_service::device::rotate(&state, &id).await? {
        Some(token) => {
            tracing::info!(device = %id, "设备令牌已轮换");
            Ok(Json(Resp::ok(RotateOut { token })))
        }
        None => Err(AppError::not_found("设备不存在")),
    }
}

pub async fn remove(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Resp<()>>, AppError> {
    relay_service::device::ensure_device_access(&state, &identity, &id).await?;
    if !relay_service::device::remove(&state, &id).await? {
        return Err(AppError::not_found("设备不存在"));
    }
    tracing::info!(device = %id, "设备已删除");
    Ok(Json(Resp::ok_empty()))
}
