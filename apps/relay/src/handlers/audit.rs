//! 审计日志查询

use axum::extract::{Query, State};
use axum::Json;
use relay_common::{AppError, AppState, Resp};
use relay_service::audit::AuditView;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<u64>,
}

pub async fn tail(
    State(state): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Resp<Vec<AuditView>>>, AppError> {
    let items = relay_service::audit::tail(&state, q.limit.unwrap_or(50)).await?;
    Ok(Json(Resp::ok(items)))
}
