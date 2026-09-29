//! 环境（房间）：分组视图 / 新建 / 重命名 / 删除（user 仅自己的，越权 404）

use axum::extract::{Path, State};
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::{AppError, AppState, Resp};
use relay_service::room::RoomView;
use serde::Deserialize;

/// 环境列表（host/client 分组 + 实时在线态；user 只见自己的）
pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<Vec<RoomView>>>, AppError> {
    let out = relay_service::room::list(&state, &identity).await?;
    Ok(Json(Resp::ok(out)))
}

#[derive(Debug, Deserialize)]
pub struct RenameReq {
    #[serde(default, rename = "displayName")]
    pub display_name: String,
}

/// 新建环境（归属当前用户）
pub async fn create(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Json(body): Json<RenameReq>,
) -> Result<Json<Resp<RoomView>>, AppError> {
    let out = relay_service::room::create(&state, &identity, &body.display_name).await?;
    tracing::info!(room = %out.room, name = %out.display_name, "新建环境");
    Ok(Json(Resp::ok(out)))
}

/// 重命名环境（需可访问）
pub async fn rename(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(room): Path<String>,
    Json(body): Json<RenameReq>,
) -> Result<Json<Resp<()>>, AppError> {
    if body.display_name.trim().is_empty() {
        return Err(AppError::bad_request("displayName 不能为空"));
    }
    relay_service::room::rename(&state, &identity, &room, &body.display_name).await?;
    Ok(Json(Resp::ok_empty()))
}

/// 删除环境（连带删设备与配对码，在线连接即时踢线）
pub async fn remove(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Path(room): Path<String>,
) -> Result<Json<Resp<()>>, AppError> {
    relay_service::room::ensure_room_access(&state, &identity, &room).await?;
    if !relay_service::room::remove(&state, &room).await? {
        return Err(AppError::not_found("环境不存在"));
    }
    tracing::info!(room = %room, "环境已删除");
    Ok(Json(Resp::ok_empty()))
}
