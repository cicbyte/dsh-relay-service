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

/// 批量写消息：Row=待落盘行；Flush=停机冲刷（把排队行写完回 ack）
enum Msg {
    Row(audit_log::ActiveModel),
    Flush(tokio::sync::oneshot::Sender<()>),
}

/// 批量写通道（start_batch_writer 装填；None=刷盘协程未启动）。
/// 有界 10k：DB 持续失败时无界通道会吃光内存（第二轮审查 #920）——
/// 满则丢弃入计数（审计尽力而为，不能反噬数据面）。
static AUDIT_TX: OnceLock<tokio::sync::mpsc::Sender<Msg>> = OnceLock::new();
static AUDIT_DROPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 启动审计批量写刷盘协程（main 起服时调用一次）。
/// conn.open/close 这类高频事件此前逐条 INSERT：连接风暴时每秒几十条小事务，
/// SQLite 写锁被打满还会拖慢管理面查询；攒批 insert_many 一次落盘。
pub fn start_batch_writer(db: DatabaseConnection) {
    let (tx, mut rx) = mpsc::channel::<Msg>(10_000);
    if AUDIT_TX.set(tx).is_err() {
        return; // 已启动（幂等）
    }
    tokio::spawn(async move {
        loop {
            let Some(first) = rx.recv().await else { break };
            let mut batch = Vec::new();
            let mut flush_ack: Option<tokio::sync::oneshot::Sender<()>> = None;
            match first {
                Msg::Row(m) => batch.push(m),
                Msg::Flush(a) => flush_ack = Some(a),
            }
            // 尽量薅满一批（上限 100，防单批过大占写锁）；遇 Flush 停手先落盘
            while batch.len() < 100 {
                match rx.try_recv() {
                    Ok(Msg::Row(m)) => batch.push(m),
                    Ok(Msg::Flush(a)) => {
                        if flush_ack.is_none() {
                            flush_ack = Some(a);
                        }
                        break;
                    }
                    Err(_) => break,
                }
            }
            if !batch.is_empty() {
                if let Err(e) = insert_batch(&db, batch).await {
                    tracing::warn!(error = %e, "审计批量写入失败");
                }
            }
            // 停机冲刷：把通道剩余行清完才回 ack（尾巴不丢，5s 兜底在调用侧）
            if let Some(a) = flush_ack {
                loop {
                    let mut rest = Vec::new();
                    while rest.len() < 100 {
                        match rx.try_recv() {
                            Ok(Msg::Row(m)) => rest.push(m),
                            _ => break,
                        }
                    }
                    if rest.is_empty() {
                        break;
                    }
                    if let Err(e) = insert_batch(&db, rest).await {
                        tracing::warn!(error = %e, "审计停机冲刷写入失败");
                    }
                }
                let _ = a.send(());
            }
        }
    });
}

/// 批量落盘：insert_many 原子性失败会整批全丢——拆半重试，坏行只牺牲
/// 自己，不拖累整批（第二轮审查 #920）。迭代实现（async fn 递归需装箱）。
async fn insert_batch(
    db: &DatabaseConnection,
    batch: Vec<audit_log::ActiveModel>,
) -> Result<(), sea_orm::DbErr> {
    let mut stack = vec![batch];
    while let Some(batch) = stack.pop() {
        if batch.is_empty() {
            continue;
        }
        match audit_log::Entity::insert_many(batch.clone()).exec(db).await {
            Ok(_) => {}
            Err(e) if batch.len() > 1 => {
                let mid = batch.len() / 2;
                let mut a = batch;
                let b = a.split_off(mid);
                stack.push(b);
                stack.push(a);
            }
            Err(e) => return Err(e), // 单行：无可拆，落给调用方 warn
        }
    }
    Ok(())
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

/// 高频事件走批量通道（conn.open/close）：有界通道满/未启动时退回同步写。
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
        Some(tx) => match tx.try_send(Msg::Row(row_of(event, ip, device, user_id, detail))) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                // 通道满 = DB 已落后 10k 行（极端洪峰/持续故障）：丢弃+计数，
                // 不给数据面加同步写压力
                let n = AUDIT_DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 100 == 1 {
                    tracing::warn!(total = n, "审计通道满，事件丢弃中");
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("审计入队失败（刷盘协程已终止）");
            }
        },
        None => record(state, event, ip, device, user_id, detail).await,
    }
}

/// 停机冲刷：优雅关闭时调用，把批量通道剩余审计行写完再退进程
/// （5s 兜底超时，DB 卡死不拖住停机；第二轮审查 #920）。
pub async fn shutdown_flush(db: &DatabaseConnection) {
    let Some(tx) = AUDIT_TX.get() else { return };
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel::<()>();
    if tx.send(Msg::Flush(ack_tx)).await.is_err() {
        return; // 刷盘协程已终止
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), ack_rx).await;
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
