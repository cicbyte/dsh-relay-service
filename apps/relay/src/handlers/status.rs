//! 概览统计（控制台首页；多用户按归属裁剪）

use axum::extract::State;
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::{AppError, AppState, Resp};
use relay_service::health::Overview;

pub async fn overview(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<Overview>>, AppError> {
    let out = relay_service::health::overview(&state, &identity).await?;
    Ok(Json(Resp::ok(out)))
}
