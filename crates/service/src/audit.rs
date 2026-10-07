//! 审计日志（oplog）：管理操作 + 连接事件，JSONL 已升级为 DB 表

use std::sync::OnceLock;

use relay_common::util::now_secs;
use relay_common::{AppError, AppState};
use relay_entity::audit_log;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, QuerySelect, Set};
use serde::Serialize;
use tokio::sync::mpsc;

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

/// 批量写通道（start_batch_writer 装填；None=刷盘协程未启动）
static AUDIT_TX: OnceLock<tokio::sync::mpsc::UnboundedSender<audit_log::ActiveModel>> =
    OnceLock::new();

/// 启动审计批量写刷盘协程（main 起服时调用一次）。
/// conn.open/close 这类高频事件此前逐条 INSERT：连接风暴时每秒几十条小事务，
/// SQLite 写锁被打满还会拖慢管理面查询；攒批 insert_many 一次落盘。
pub fn start_batch_writer(db: DatabaseConnection) {
    let (tx, mut rx) = mpsc::unbounded_channel::<audit_log::ActiveModel>();
    if AUDIT_TX.set(tx).is_err() {
        return; // 已启动（幂等）
    }
    tokio::spawn(async move {
        loop {
            // 阻塞等首条 → 尽量薅满一批（上限 100，防单批过大占写锁）
            let Some(first) = rx.recv().await else { break };
            let mut batch = vec![first];
            while batch.len() < 100 {
                match rx.try_recv() {
                    Ok(m) => batch.push(m),
                    Err(_) => break,
                }
            }
            if let Err(e) = audit_log::Entity::insert_many(batch).exec(&db).await {
                tracing::warn!(error = %e, "审计批量写入失败");
            }
        }
    });
}

fn row_of(
    event: &str,
    ip: &str,
    device: Option<&str>,
    user_id: i64,
    detail: serde_json::Value,
) -> audit_log::ActiveModel {
    audit_log::ActiveModel {
        ts: Set(now_secs()),
        event: Set(event.to_string()),
        ip: Set(ip.to_string()),
        device: Set(device.map(|s| s.to_string())),
        user_id: Set(user_id),
        detail: Set(detail.to_string()),
        ..Default::default()
    }
}

/// 写一条审计（失败只记日志，不影响主流程）。user_id=操作人/环境归属人（0=系统/未知）
pub async fn record(
    state: &AppState,
    event: &str,
    ip: &str,
    device: Option<&str>,
    user_id: i64,
    detail: serde_json::Value,
) {
    if let Err(e) = row_of(event, ip, device, user_id, detail)
        .insert(&state.db)
        .await
    {
        tracing::warn!(error = %e, "审计写入失败");
    }
}

/// 高频事件走批量通道（conn.open/close）：通道满/未启动时退回同步写，不丢事件。
/// 与 record 的差别只在落盘路径——调用方语义一致（fire-and-forget）。
pub async fn record_queued(
    state: &AppState,
    event: &str,
    ip: &str,
    device: Option<&str>,
    user_id: i64,
    detail: serde_json::Value,
) {
    match AUDIT_TX.get() {
        Some(tx) => {
            if let Err(e) = tx.send(row_of(event, ip, device, user_id, detail)) {
                tracing::warn!(error = %e, "审计入队失败（刷盘协程已终止）");
            }
        }
        None => record(state, event, ip, device, user_id, detail).await,
    }
}

/// 审计尾部（时间倒序）：admin 全量；user 仅本人（含其环境的连接事件）
pub async fn tail(
    state: &AppState,
    limit: u64,
    id: &relay_common::identity::AdminIdentity,
) -> Result<Vec<AuditView>, AppError> {
    use sea_orm::QueryFilter;
    use sea_orm::ColumnTrait;
    let mut query = audit_log::Entity::find();
    if !id.is_admin() {
        query = query.filter(audit_log::Column::UserId.eq(id.user_id));
    }
    let rows = query
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
