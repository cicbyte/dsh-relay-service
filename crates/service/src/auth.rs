//! 管理端认证：登录 / 刷新（nonce 轮转）/ 登出 / 改密 / 令牌解析

use relay_common::identity::AdminIdentity;
use relay_common::jwt::{AUD_ADMIN, TYP_REFRESH};
use relay_common::util::{now_secs, rand_hex};
use relay_common::{AppError, AppState};
use relay_entity::admin_user;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginOutput {
    pub access_token: String,
    pub refresh_token: String,
    pub username: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOutput {
    pub user_id: i64,
    pub username: String,
}

/// 登录：bcrypt 校验 → 签发 access/refresh（refresh 绑定 nonce）
pub async fn login(
    state: &AppState,
    username: &str,
    password: &str,
) -> Result<LoginOutput, AppError> {
    let user = admin_user::Entity::find()
        .filter(admin_user::Column::Username.eq(username))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::unauthorized("用户名或密码错误"))?;
    if !bcrypt::verify(password, &user.password_hash)? {
        return Err(AppError::unauthorized("用户名或密码错误"));
    }

    let sid = rand_hex(8);
    let nonce = rand_hex(8);
    let mut am = user.clone().into_active_model();
    am.refresh_nonce = Set(nonce.clone());
    am.last_login_at = Set(now_secs());
    am.update(&state.db).await?;

    let access = state.jwt.sign_access(user.id, &sid, AUD_ADMIN)?;
    let refresh = state.jwt.sign_refresh(user.id, &sid, AUD_ADMIN, &nonce)?;
    Ok(LoginOutput {
        access_token: access,
        refresh_token: refresh,
        username: user.username,
    })
}

/// 刷新令牌轮转：nonce 必须与库里一致（旧 refresh 立即失效，防盗用）
pub async fn refresh(state: &AppState, refresh_token: &str) -> Result<LoginOutput, AppError> {
    let claims = state.jwt.verify(refresh_token, AUD_ADMIN, TYP_REFRESH)?;
    let user = admin_user::Entity::find_by_id(claims.sub)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::unauthorized("令牌已失效，请重新登录"))?;
    if claims.nonce.is_empty() || claims.nonce != user.refresh_nonce {
        return Err(AppError::unauthorized("令牌已失效，请重新登录"));
    }

    let sid = rand_hex(8);
    let nonce = rand_hex(8);
    let mut am = user.clone().into_active_model();
    am.refresh_nonce = Set(nonce.clone());
    am.update(&state.db).await?;

    let access = state.jwt.sign_access(user.id, &sid, AUD_ADMIN)?;
    let refresh = state.jwt.sign_refresh(user.id, &sid, AUD_ADMIN, &nonce)?;
    Ok(LoginOutput {
        access_token: access,
        refresh_token: refresh,
        username: user.username,
    })
}

/// 登出：轮转 nonce，refresh 立即失效（access 到 exp 自然过期）
pub async fn logout(state: &AppState, user_id: i64) -> Result<(), AppError> {
    let Some(user) = admin_user::Entity::find_by_id(user_id).one(&state.db).await? else {
        return Ok(());
    };
    let mut am = user.into_active_model();
    am.refresh_nonce = Set(rand_hex(8));
    am.update(&state.db).await?;
    Ok(())
}

/// 修改密码：校验旧密码 → bcrypt 新密码 → 轮转 nonce（其他端全部下线）
pub async fn change_password(
    state: &AppState,
    user_id: i64,
    old_password: &str,
    new_password: &str,
) -> Result<(), AppError> {
    if new_password.len() < 8 {
        return Err(AppError::bad_request("新密码至少 8 位"));
    }
    let user = admin_user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::unauthorized("账号不存在"))?;
    if !bcrypt::verify(old_password, &user.password_hash)? {
        return Err(AppError::unauthorized("原密码错误"));
    }
    let hash = bcrypt::hash(new_password, 10)?;
    let mut am = user.into_active_model();
    am.password_hash = Set(hash);
    am.refresh_nonce = Set(rand_hex(8));
    am.update(&state.db).await?;
    Ok(())
}

/// access token → 管理端身份（认证中间件用）
pub async fn resolve_admin_token(
    state: &AppState,
    token: &str,
) -> Result<AdminIdentity, AppError> {
    let claims = state.jwt.verify(token, AUD_ADMIN, relay_common::jwt::TYP_ACCESS)?;
    let user = admin_user::Entity::find_by_id(claims.sub)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::unauthorized("账号不存在"))?;
    Ok(AdminIdentity {
        user_id: user.id,
        username: user.username,
    })
}
