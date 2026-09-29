//! 配对码：签发一次性配对码

use axum::extract::State;
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
}

pub async fn issue(
    State(state): State<AppState>,
    Json(body): Json<PairingReq>,
) -> Result<Json<Resp<PairingView>>, AppError> {
    let out = relay_service::pairing::issue(&state, &body.role, &body.name).await?;
    tracing::info!(role = %out.role, name = %out.name, "签发配对码");
    Ok(Json(Resp::ok(out)))
}
