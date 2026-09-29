//! 环境（房间）显示名 + 配对码绑房间
//! - rooms：room(hex8) → display_name，终结裸哈希展示
//! - pairing_codes.room：出码即绑环境，核销按码内房间（hello.code 只做寻址交叉校验）

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Rooms::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Rooms::Room).string().not_null().primary_key())
                    .col(ColumnDef::new(Rooms::DisplayName).string().null())
                    .col(ColumnDef::new(Rooms::CreatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(PairingCodes::Table)
                    .add_column(ColumnDef::new(PairingCodes::Room).string().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Rooms::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Rooms {
    Table,
    Room,
    DisplayName,
    CreatedAt,
}

#[derive(DeriveIden)]
enum PairingCodes {
    Table,
    Room,
}
