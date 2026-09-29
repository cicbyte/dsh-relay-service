//! 配对码：签发一次性配对码；设备代领（桌面桥用自己的令牌为手机出码）

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use relay_common::{AppError, AppState, Resp};
use relay_service::pairing::PairingView;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PairingReq {
    /// host（桌面桥）| client（手机）
    pub role: String,
    #[serde(default)]
    pub name: String,
    /// 绑定环境（room hex8，来自管理台环境视图；空=不绑）
    #[serde(default)]
    pub room: String,
}

pub async fn issue(
    State(state): State<AppState>,
    Json(body): Json<PairingReq>,
) -> Result<Json<Resp<PairingView>>, AppError> {
    let out = relay_service::pairing::issue(&state, &body.role, &body.name, &body.room).await?;
    tracing::info!(role = %out.role, name = %out.name, room = %out.room, "签发配对码");
    Ok(Json(Resp::ok(out)))
}

#[derive(Debug, Deserialize)]
pub struct InviteReq {
    #[serde(default)]
    pub name: String,
}

/// 设备代领配对码：桌面桥（host 设备令牌）为手机签发本环境 role=client 的配对码。
/// 窄权限：仅 host 设备可用，只能出 client 码，房间锁定为设备所在环境。
pub async fn device_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InviteReq>,
) -> Result<Json<Resp<PairingView>>, AppError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if token.is_empty() {
        return Err(AppError::unauthorized("缺设备令牌"));
    }
    // 带令牌必带设备 ID：从令牌反查不可行（存哈希），走 deviceId 头
    let dev_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let dev = relay_service::device::check_token(&state, dev_id, token, "host", "")
        .await
        .map_err(|e| AppError::unauthorized(e.code()))?;
    let out = relay_service::pairing::issue(&state, "client", &body.name, &dev.room).await?;
    relay_service::audit::record(
        &state,
        "admin.op",
        "",
        Some(&dev.id),
        serde_json::json!({ "op": "device.invite", "name": out.name, "room": out.room }),
    )
    .await;
    tracing::info!(device = %dev.id, room = %out.room, "设备代领配对码");
    Ok(Json(Resp::ok(out)))
}
