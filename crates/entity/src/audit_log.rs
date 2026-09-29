use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 审计日志（oplog）：登录/设备管理/连接事件，detail 存 JSON 文本
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub ts: i64,
    pub event: String,
    pub ip: String,
    pub device: Option<String>,
    /// JSON 文本
    pub detail: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
