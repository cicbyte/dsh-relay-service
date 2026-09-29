use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 设备注册（手机/桌面桥）。token 只存 SHA-256 哈希；revoked 可吊销
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    /// 设备 ID（dev_xxxx，业务主键）
    #[sea_orm(primary_key)]
    pub id: String,
    pub name: String,
    /// host（桌面桥）| client（手机）
    pub role: String,
    /// 绑定房间（hex8）；空串 = 任意房间
    pub room: String,
    pub token_hash: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
