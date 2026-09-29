//! 多用户：账号角色 + 环境归属 + 审计到人
//! - admin_users.role：admin=全局管理 | user=仅自己的环境（权限矩阵写死代码）；
//!   存量行默认 admin（迁移时唯一的种子账号）
//! - rooms.owner_id：环境归属用户；存量环境回填到首个账号
//! - audit_logs.user_id：操作人/环境归属人；conn 事件按房间归属回填，user=0 归管理员可见
//! - username 唯一索引兜底（登录按用户名查一行）

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(
                        ColumnDef::new(AdminUsers::Role)
                            .string()
                            .not_null()
                            .default("admin"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Rooms::Table)
                    .add_column(
                        ColumnDef::new(Rooms::OwnerId)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(AuditLogs::Table)
                    .add_column(
                        ColumnDef::new(AuditLogs::UserId)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;
        // 存量环境归属到首个账号（迁移时唯一的 admin）
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE rooms SET owner_id = (SELECT MIN(id) FROM admin_users) WHERE owner_id = 0",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_admin_users_username ON admin_users (username)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // 加列类迁移不回滚（sqlite ALTER 兼容性差；多用户列保留无害）
        Ok(())
    }
}

#[derive(DeriveIden)]
enum AdminUsers {
    Table,
    Role,
}

#[derive(DeriveIden)]
enum Rooms {
    Table,
    OwnerId,
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    UserId,
}
