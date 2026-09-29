use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 一次性配对码：短时效（默认 600s）、单次使用、失败计数熔断
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "pairing_codes")]
pub struct Model {
    /// 配对码（XXXX-XXXX，业务主键）
    #[sea_orm(primary_key)]
    pub code: String,
    /// 允许配对的角色 host|client
    pub role: String,
    /// 预设设备名（可空）
    pub name: String,
    pub created_at: i64,
    pub expires_at: i64,
    /// 核销后记录设备 ID
    pub used_by: Option<String>,
    pub attempts: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
