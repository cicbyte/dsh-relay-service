//! 审计日志（oplog）：管理操作 + 连接事件，JSONL 已升级为 DB 表

use relay_common::util::now_secs;
use relay_common::{AppError, AppState};
use relay_entity::audit_log;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, QuerySelect, Set};
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditView {
    pub id: i64,
    pub ts: i64,
    pub event: String,
    pub ip: String,
    pub device: Option<String>,
    /// JSON 文本
    pub detail: String,
}

/// 写一条审计（失败只记日志，不影响主流程）
pub async fn record(
    state: &AppState,
    event: &str,
    ip: &str,
    device: Option<&str>,
    detail: serde_json::Value,
) {
    let row = audit_log::ActiveModel {
        ts: Set(now_secs()),
        event: Set(event.to_string()),
        ip: Set(ip.to_string()),
        device: Set(device.map(|s| s.to_string())),
        detail: Set(detail.to_string()),
        ..Default::default()
    };
    if let Err(e) = row.insert(&state.db).await {
        tracing::warn!(error = %e, "审计写入失败");
    }
}

/// 审计尾部（时间倒序）
pub async fn tail(state: &AppState, limit: u64) -> Result<Vec<AuditView>, AppError> {
    let rows = audit_log::Entity::find()
        .order_by_desc(audit_log::Column::Ts)
        .order_by_desc(audit_log::Column::Id)
        .limit(limit.clamp(1, 500))
        .all(&state.db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| AuditView {
            id: r.id,
            ts: r.ts,
            event: r.event,
            ip: r.ip,
            device: r.device,
            detail: r.detail,
        })
        .collect())
}
