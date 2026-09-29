use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 管理账号
        manager
            .create_table(
                Table::create()
                    .table(AdminUsers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AdminUsers::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AdminUsers::Username)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(AdminUsers::PasswordHash).string().not_null())
                    .col(
                        ColumnDef::new(AdminUsers::RefreshNonce)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(AdminUsers::CreatedAt).big_integer().not_null())
                    .col(
                        ColumnDef::new(AdminUsers::LastLoginAt)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        // 设备注册
        manager
            .create_table(
                Table::create()
                    .table(Devices::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Devices::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Devices::Name).string().not_null().default(""))
                    .col(ColumnDef::new(Devices::Role).string().not_null())
                    .col(ColumnDef::new(Devices::Room).string().not_null().default(""))
                    .col(ColumnDef::new(Devices::TokenHash).string().not_null())
                    .col(ColumnDef::new(Devices::CreatedAt).big_integer().not_null())
                    .col(
                        ColumnDef::new(Devices::LastSeenAt)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Devices::Revoked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_devices_token_hash")
                    .table(Devices::Table)
                    .col(Devices::TokenHash)
                    .to_owned(),
            )
            .await?;

        // 配对码
        manager
            .create_table(
                Table::create()
                    .table(PairingCodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PairingCodes::Code)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PairingCodes::Role).string().not_null())
                    .col(
                        ColumnDef::new(PairingCodes::Name)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(PairingCodes::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PairingCodes::ExpiresAt).big_integer().not_null(),
                    )
                    .col(ColumnDef::new(PairingCodes::UsedBy).string().null())
                    .col(
                        ColumnDef::new(PairingCodes::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        // 审计日志
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuditLogs::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuditLogs::Ts).big_integer().not_null())
                    .col(ColumnDef::new(AuditLogs::Event).string().not_null())
                    .col(
                        ColumnDef::new(AuditLogs::Ip)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(AuditLogs::Device).string().null())
                    .col(
                        ColumnDef::new(AuditLogs::Detail)
                            .string()
                            .not_null()
                            .default("{}"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_ts")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::Ts)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLogs::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(PairingCodes::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Devices::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AdminUsers::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum AdminUsers {
    Table,
    Id,
    Username,
    PasswordHash,
    RefreshNonce,
    CreatedAt,
    LastLoginAt,
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
    Name,
    Role,
    Room,
    TokenHash,
    CreatedAt,
    LastSeenAt,
    Revoked,
}

#[derive(DeriveIden)]
enum PairingCodes {
    Table,
    Code,
    Role,
    Name,
    CreatedAt,
    ExpiresAt,
    UsedBy,
    Attempts,
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    Id,
    Ts,
    Event,
    Ip,
    Device,
    Detail,
}
