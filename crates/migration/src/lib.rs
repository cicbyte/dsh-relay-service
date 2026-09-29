//! 数据库迁移 + 种子（对齐 byte-migration 的 init_db_connect）。
//! 首次启动自动建表并生成管理账号（ADMIN_PASSWORD 环境变量或随机密码，
//! 随机密码只在日志打印一次）。

use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter, Set};
use sea_orm_migration::MigratorTrait;

mod m20260601_000001_init;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
        vec![Box::new(m20260601_000001_init::Migration)]
    }
}

/// 连接数据库 + 跑迁移 + 种子
pub async fn init_db_connect(url: &str) -> Result<DatabaseConnection, DbErr> {
    // sqlite：mode=rwc 只建库文件不建目录，先确保父目录存在
    if let Some(rest) = url.strip_prefix("sqlite://") {
        let path = rest.split('?').next().unwrap_or(rest);
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DbErr::Custom(format!("创建数据目录失败: {e}")))?;
            }
        }
    }
    let db = sea_orm::Database::connect(url).await?;
    Migrator::up(&db, None).await?;
    seed_admin(&db).await?;
    Ok(db)
}

/// 首启种子：无管理员时创建 admin（密码取 ADMIN_PASSWORD 或随机生成并打印一次）
async fn seed_admin(db: &DatabaseConnection) -> Result<(), DbErr> {
    use relay_entity::admin_user;

    let count = admin_user::Entity::find().count(db).await?;
    if count > 0 {
        return Ok(());
    }

    let password = std::env::var("ADMIN_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // 形如 xxxx-xxxx-xxxx 的随机密码
            let hex = rand_hex(6);
            format!("{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
        });
    let hash = bcrypt::hash(&password, 10)
        .map_err(|e| DbErr::Custom(format!("bcrypt hash failed: {e}")))?;

    let now = chrono::Utc::now().timestamp();
    let row = admin_user::ActiveModel {
        username: Set("admin".to_string()),
        password_hash: Set(hash),
        refresh_nonce: Set(String::new()),
        created_at: Set(now),
        last_login_at: Set(0),
        ..Default::default()
    };
    row.insert(db).await?;

    tracing::warn!("=====================================================");
    tracing::warn!("初始管理员 admin 的密码（只此一次显示，请立即保存）: {password}");
    tracing::warn!("=====================================================");
    Ok(())
}

/// 保留审计表清理（启动时调用；retain_days=0 不清理）
pub async fn cleanup_audit(db: &DatabaseConnection, retain_days: u64) -> Result<(), DbErr> {
    if retain_days == 0 {
        return Ok(());
    }
    use relay_entity::audit_log;
    let cutoff = chrono::Utc::now().timestamp() - (retain_days as i64) * 86400;
    audit_log::Entity::delete_many()
        .filter(audit_log::Column::Ts.lt(cutoff))
        .exec(db)
        .await?;
    Ok(())
}

/// 随机十六进制串（getrandom；失败兜底时间熵）
fn rand_hex(nbytes: usize) -> String {
    let mut buf = vec![0u8; nbytes];
    if getrandom::getrandom(&mut buf).is_err() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(7);
        for (i, b) in buf.iter_mut().enumerate() {
            *b = t.wrapping_add(i as u32) as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}
