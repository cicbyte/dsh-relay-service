//! 概览统计（控制台首页）

use axum::extract::State;
use axum::Json;
use relay_common::{AppError, AppState, Resp};
use relay_service::health::Overview;

pub async fn overview(State(state): State<AppState>) -> Result<Json<Resp<Overview>>, AppError> {
    let out = relay_service::health::overview(&state).await?;
    Ok(Json(Resp::ok(out)))
}
