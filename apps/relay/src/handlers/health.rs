//! 健康检查（公开，供探针使用）

use axum::extract::State;
use axum::Json;
use relay_common::{AppError, AppState, Resp};
use relay_service::health::HealthDetail;

pub async fn simple() -> Json<Resp<()>> {
    Json(Resp::ok_empty())
}

pub async fn detail(State(state): State<AppState>) -> Result<Json<Resp<HealthDetail>>, AppError> {
    let out = relay_service::health::detail(&state).await?;
    Ok(Json(Resp::ok(out)))
}
