//! 管理台用户管理（仅 admin，权限矩阵写死）：无自助注册，账号由管理员增删。
//! 角色只有两种：admin=全局管理（用户管理+全部环境+全审计）| user=仅自己的环境。

use relay_common::identity::AdminIdentity;
use relay_common::util::{now_secs, rand_hex};
use relay_common::{AppError, AppState};
use relay_entity::admin_user;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
};
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: i64,
    pub last_login_at: i64,
}

/// 权限钩子：仅 admin（其余路由默认放行到归属过滤，见 ensure_room_access）
pub fn require_admin(id: &AdminIdentity) -> Result<(), AppError> {
    if id.is_admin() {
        Ok(())
    } else {
        Err(AppError::forbidden("仅管理员可操作"))
    }
}

pub async fn list(state: &AppState) -> Result<Vec<UserView>, AppError> {
    let rows = admin_user::Entity::find().all(&state.db).await?;
    Ok(rows.into_iter().map(view).collect())
}

pub async fn create(
    state: &AppState,
    username: &str,
    password: &str,
    role: &str,
) -> Result<UserView, AppError> {
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::bad_request("用户名不能为空"));
    }
    if password.len() < 8 {
        return Err(AppError::bad_request("密码至少 8 位"));
    }
    if role != "admin" && role != "user" {
        return Err(AppError::bad_request("role 只能是 admin 或 user"));
    }
    if admin_user::Entity::find()
        .filter(admin_user::Column::Username.eq(username))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(AppError::bad_request("用户名已存在"));
    }
    let hash = bcrypt::hash(password, 10)?;
    let row = admin_user::ActiveModel {
        username: Set(username.to_string()),
        role: Set(role.to_string()),
        password_hash: Set(hash),
        refresh_nonce: Set(String::new()),
        created_at: Set(now_secs()),
        last_login_at: Set(0),
        ..Default::default()
    };
    Ok(view(row.insert(&state.db).await?))
}

/// 重置密码（管理员代办）：轮转 nonce，该用户所有端全部下线
pub async fn reset_password(state: &AppState, id: i64, password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::bad_request("密码至少 8 位"));
    }
    let user = admin_user::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    let hash = bcrypt::hash(password, 10)?;
    let mut am = user.into_active_model();
    am.password_hash = Set(hash);
    am.refresh_nonce = Set(rand_hex(8));
    am.update(&state.db).await?;
    Ok(())
}

/// 删除用户：连带删其全部环境（踢线+设备+配对码）；不可删自己/最后一个管理员
pub async fn remove(state: &AppState, actor: &AdminIdentity, id: i64) -> Result<(), AppError> {
    if actor.user_id == id {
        return Err(AppError::bad_request("不能删除自己"));
    }
    let user = admin_user::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if user.role == "admin" {
        let admins = admin_user::Entity::find()
            .filter(admin_user::Column::Role.eq("admin"))
            .count(&state.db)
            .await?;
        if admins <= 1 {
            return Err(AppError::bad_request("至少保留一个管理员"));
        }
    }
    let rooms: Vec<String> = relay_entity::room::Entity::find()
        .filter(relay_entity::room::Column::OwnerId.eq(id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|r| r.room)
        .collect();
    for r in rooms {
        crate::room::remove(state, &r).await?;
    }
    admin_user::Entity::delete_by_id(id).exec(&state.db).await?;
    Ok(())
}

fn view(u: admin_user::Model) -> UserView {
    UserView {
        id: u.id,
        username: u.username,
        role: u.role,
        created_at: u.created_at,
        last_login_at: u.last_login_at,
    }
}
