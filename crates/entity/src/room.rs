use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环境（房间）：room=hex8 为物理键，display_name 供人认
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "rooms")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub room: String,
    /// 环境显示名（如「家里」「公司」）
    pub display_name: Option<String>,
    /// 归属用户（admin_users.id；0=未归属，仅管理员可见）
    pub owner_id: i64,
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
