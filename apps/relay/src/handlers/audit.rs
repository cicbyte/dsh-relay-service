//! 审计日志查询（admin 全量；user 仅本人及其环境）

use axum::extract::{Query, State};
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::{AppError, AppState, Resp};
use relay_service::audit::AuditView;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<u64>,
}

pub async fn tail(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Resp<Vec<AuditView>>>, AppError> {
    let items = relay_service::audit::tail(&state, q.limit.unwrap_or(50), &identity).await?;
    Ok(Json(Resp::ok(items)))
}
