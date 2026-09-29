use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 管理账号（当前单管理员；password 存 bcrypt，refresh_nonce 做刷新轮转）
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "admin_users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    #[sea_orm(unique)]
    pub username: String,
    pub password_hash: String,
    /// 刷新令牌轮转 nonce（刷新时更新，旧 refresh 立即失效）
    pub refresh_nonce: String,
    pub created_at: i64,
    pub last_login_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
