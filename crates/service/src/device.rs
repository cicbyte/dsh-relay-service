//! 设备注册：列表 / 吊销（即时踢线）/ 轮换令牌 / 删除 / WS 侧令牌校验

use relay_common::AppError;
use relay_common::AppState;
use relay_common::util::{now_secs, rand_hex};
use relay_entity::device;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, QueryOrder, Set};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// 设备视图（带实时在线状态）
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub role: String,
    pub room: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
    pub online: bool,
}

/// WS hello 令牌校验错误码（对应协议 reject code）
#[derive(Debug)]
pub enum TokenErr {
    UnknownDevice,
    BadToken,
    Revoked,
    RoleMismatch,
    RoomMismatch,
}

impl TokenErr {
    pub fn code(&self) -> &'static str {
        match self {
            TokenErr::UnknownDevice => "unknown-device",
            TokenErr::BadToken => "bad-token",
            TokenErr::Revoked => "revoked",
            TokenErr::RoleMismatch => "role-mismatch",
            TokenErr::RoomMismatch => "room-mismatch",
        }
    }
}

fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// 新设备令牌：`tok_` + 32 hex（只在发放/轮换时明文返回一次）
pub fn new_token() -> String {
    format!("tok_{}", rand_hex(16))
}

/// WS 侧校验设备令牌（常数时间比较哈希）
pub async fn check_token(
    state: &AppState,
    device_id: &str,
    token: &str,
    role: &str,
    room: &str,
) -> Result<device::Model, TokenErr> {
    if device_id.is_empty() {
        return Err(TokenErr::UnknownDevice);
    }
    let Some(d) = device::Entity::find_by_id(device_id)
        .one(&state.db)
        .await
        .map_err(|_| TokenErr::UnknownDevice)?
    else {
        return Err(TokenErr::UnknownDevice);
    };
    if d.revoked {
        return Err(TokenErr::Revoked);
    }
    if !d.role.is_empty() && d.role != role {
        return Err(TokenErr::RoleMismatch);
    }
    if !d.room.is_empty() && d.room != room {
        return Err(TokenErr::RoomMismatch);
    }
    let want = hash_token(token);
    let a = want.as_bytes();
    let b = d.token_hash.as_bytes();
    if a.len() != b.len() || !bool::from(a.ct_eq(b)) {
        return Err(TokenErr::BadToken);
    }
    Ok(d)
}

/// 更新最后活跃时间（WS hello 时触发）
pub async fn touch(state: &AppState, device_id: &str) {
    if let Ok(Some(d)) = device::Entity::find_by_id(device_id).one(&state.db).await {
        let mut am = d.into_active_model();
        am.last_seen_at = Set(now_secs());
        let _ = am.update(&state.db).await;
    }
}

pub async fn list(state: &AppState) -> Result<Vec<DeviceView>, AppError> {
    let rows = device::Entity::find()
        .order_by_desc(device::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|d| {
            let online = state.hub.device_online(&d.id);
            DeviceView {
                id: d.id,
                name: d.name,
                role: d.role,
                room: d.room,
                created_at: d.created_at,
                last_seen_at: d.last_seen_at,
                revoked: d.revoked,
                online,
            }
        })
        .collect())
}

/// 吊销：置 revoked 并即时踢线
pub async fn revoke(state: &AppState, id: &str) -> Result<bool, AppError> {
    let Some(d) = device::Entity::find_by_id(id).one(&state.db).await? else {
        return Ok(false);
    };
    let mut am = d.into_active_model();
    am.revoked = Set(true);
    am.update(&state.db).await?;
    state.hub.kick_device(id);
    Ok(true)
}

/// 轮换令牌：返回新明文 token（只此一次）
pub async fn rotate(state: &AppState, id: &str) -> Result<Option<String>, AppError> {
    let Some(d) = device::Entity::find_by_id(id).one(&state.db).await? else {
        return Ok(None);
    };
    let token = new_token();
    let mut am = d.into_active_model();
    am.token_hash = Set(hash_token(&token));
    am.revoked = Set(false);
    am.update(&state.db).await?;
    state.hub.kick_device(id);
    Ok(Some(token))
}

/// 删除设备（连带清配对核销引用）
pub async fn remove(state: &AppState, id: &str) -> Result<bool, AppError> {
    let res = device::Entity::delete_by_id(id).exec(&state.db).await?;
    state.hub.kick_device(id);
    Ok(res.rows_affected > 0)
}

/// 配对创建设备（pairing::redeem 用）。String 主键走 exec_without_returning，
/// 返回构造好的 Model（不走 insert() 的回查路径）。
pub async fn create(
    state: &AppState,
    id: &str,
    name: &str,
    role: &str,
    room: &str,
    token: &str,
) -> Result<device::Model, AppError> {
    let now = now_secs();
    let hash = hash_token(token);
    let row = device::ActiveModel {
        id: Set(id.to_string()),
        name: Set(name.to_string()),
        role: Set(role.to_string()),
        room: Set(room.to_string()),
        token_hash: Set(hash.clone()),
        created_at: Set(now),
        last_seen_at: Set(now),
        revoked: Set(false),
    };
    sea_orm::Insert::one(row)
        .exec_without_returning(&state.db)
        .await?;
    Ok(device::Model {
        id: id.to_string(),
        name: name.to_string(),
        role: role.to_string(),
        room: room.to_string(),
        token_hash: hash,
        created_at: now,
        last_seen_at: now,
        revoked: false,
    })
}
