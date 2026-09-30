//! 环境（房间）：显示名维护 + 分组视图（1 host : N client）+ 归属隔离
//!
//! 多用户隔离（权限矩阵写死）：admin 看/管全部环境；user 仅 owner_id=自己 的环境。
//! 越权访问一律 404「环境不存在」（不泄露存在性）。

use relay_common::identity::AdminIdentity;
use relay_common::util::now_secs;
use relay_common::{AppError, AppState};
use relay_entity::{admin_user, device, pairing_code, room};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, Insert, IntoActiveModel, QueryFilter, Set,
};
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomDevice {
    pub id: String,
    pub name: String,
    pub role: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
    pub online: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    pub room: String,
    pub display_name: String,
    pub created_at: i64,
    /// 归属用户（0=未归属）
    pub owner_id: i64,
    /// 归属用户名（管理台展示）
    pub owner_name: String,
    pub host_online: bool,
    pub clients_online: usize,
    pub host: Option<RoomDevice>,
    pub clients: Vec<RoomDevice>,
}

/// 登记环境（幂等；legacy 无绑定码核销时调用，owner=0 仅管理员可见）
pub async fn touch(state: &AppState, room_id: &str) -> Result<(), AppError> {
    if room_id.is_empty() {
        return Ok(());
    }
    if room::Entity::find_by_id(room_id).one(&state.db).await?.is_some() {
        return Ok(());
    }
    let row = room::ActiveModel {
        room: Set(room_id.to_string()),
        display_name: Set(None),
        owner_id: Set(0),
        created_at: Set(now_secs()),
    };
    // String 主键走 exec_without_returning
    Insert::one(row).exec_without_returning(&state.db).await?;
    Ok(())
}

/// 新建环境（归属当前用户；生成 room hex8，幂等重试防碰撞）
pub async fn create(
    state: &AppState,
    id: &AdminIdentity,
    display_name: &str,
) -> Result<RoomView, AppError> {
    use relay_common::util::rand_hex;
    let name = display_name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::bad_request("displayName 不能为空"));
    }
    for _ in 0..8 {
        let room_id = rand_hex(4); // 8 hex，与 room_of() 同形
        if room::Entity::find_by_id(&room_id)
            .one(&state.db)
            .await?
            .is_some()
        {
            continue;
        }
        let row = room::ActiveModel {
            room: Set(room_id.clone()),
            display_name: Set(Some(name.clone())),
            owner_id: Set(id.user_id),
            created_at: Set(now_secs()),
        };
        Insert::one(row).exec_without_returning(&state.db).await?;
        return Ok(RoomView {
            room: room_id,
            display_name: name,
            created_at: now_secs(),
            owner_id: id.user_id,
            owner_name: id.username.clone(),
            host_online: false,
            clients_online: 0,
            host: None,
            clients: Vec::new(),
        });
    }
    Err(AppError::internal("环境 ID 生成冲突"))
}

/// 环境归属访问校验：admin 全通；user 仅自己的；其余一律 404（不泄露存在性）
pub async fn ensure_room_access(
    state: &AppState,
    id: &AdminIdentity,
    room_id: &str,
) -> Result<room::Model, AppError> {
    let row = room::Entity::find_by_id(room_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("环境不存在"))?;
    if !id.is_admin() && row.owner_id != id.user_id {
        return Err(AppError::not_found("环境不存在"));
    }
    Ok(row)
}

/// 环境归属人（审计落账用；未知回 0）。走 owner 缓存（60s TTL + 删环境主动失效）
pub async fn owner_of(state: &AppState, room_id: &str) -> i64 {
    if let Some(o) = state.owner_cache.get(room_id) {
        return o;
    }
    let owner = room::Entity::find_by_id(room_id)
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .map(|r| r.owner_id)
        .unwrap_or(0);
    if owner != 0 {
        state.owner_cache.put(room_id, owner);
    }
    owner
}

/// 重命名环境（需可访问）
pub async fn rename(
    state: &AppState,
    id: &AdminIdentity,
    room_id: &str,
    display_name: &str,
) -> Result<(), AppError> {
    let row = ensure_room_access(state, id, room_id).await?;
    let mut am = row.into_active_model();
    am.display_name = Set(Some(display_name.trim().to_string()));
    am.update(&state.db).await?;
    Ok(())
}

/// 删除环境：踢线全部在线连接，连带删设备与未用配对码（审计由 oplog 中间件落）
pub async fn remove(state: &AppState, room_id: &str) -> Result<bool, AppError> {
    if room::Entity::find_by_id(room_id)
        .one(&state.db)
        .await?
        .is_none()
    {
        return Ok(false);
    }
    state.hub.kick_room(room_id);
    state.hub.buf_clear_room(room_id);
    state.owner_cache.invalidate(room_id);
    device::Entity::delete_many()
        .filter(device::Column::Room.eq(room_id))
        .exec(&state.db)
        .await?;
    pairing_code::Entity::delete_many()
        .filter(pairing_code::Column::Room.eq(room_id))
        .exec(&state.db)
        .await?;
    room::Entity::delete_by_id(room_id).exec(&state.db).await?;
    Ok(true)
}

/// 环境列表（含 host/client 分组与实时在线态；user 只见自己的）
pub async fn list(state: &AppState, id: &AdminIdentity) -> Result<Vec<RoomView>, AppError> {
    let mut query = room::Entity::find();
    if !id.is_admin() {
        query = query.filter(room::Column::OwnerId.eq(id.user_id));
    }
    let rooms = query.all(&state.db).await?;
    let devices = device::Entity::find().all(&state.db).await?;
    // 归属用户名（小表全量取，够用）
    let names: std::collections::HashMap<i64, String> = admin_user::Entity::find()
        .all(&state.db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect();
    let mut out = Vec::new();
    for r in rooms {
        let mut host: Option<RoomDevice> = None;
        let mut clients = Vec::new();
        for d in devices.iter().filter(|d| d.room == r.room) {
            let v = RoomDevice {
                id: d.id.clone(),
                name: d.name.clone(),
                role: d.role.clone(),
                created_at: d.created_at,
                last_seen_at: d.last_seen_at,
                revoked: d.revoked,
                online: state.hub.device_online(&d.id),
            };
            if d.role == "host" {
                host = Some(v);
            } else {
                clients.push(v);
            }
        }
        clients.sort_by_key(|c| std::cmp::Reverse(c.last_seen_at));
        let clients_online = clients.iter().filter(|c| c.online).count();
        out.push(RoomView {
            room: r.room.clone(),
            display_name: r
                .display_name
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("环境-{}", &r.room[..4.min(r.room.len())])),
            created_at: r.created_at,
            owner_id: r.owner_id,
            owner_name: names.get(&r.owner_id).cloned().unwrap_or_default(),
            host_online: host.as_ref().map(|h| h.online).unwrap_or(false),
            clients_online,
            host,
            clients,
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    Ok(out)
}
