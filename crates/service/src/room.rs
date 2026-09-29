//! 环境（房间）：显示名维护 + 分组视图（1 host : N client）

use relay_common::util::now_secs;
use relay_common::{AppError, AppState};
use relay_entity::{device, room};
use sea_orm::{ActiveModelTrait, EntityTrait, Insert, IntoActiveModel, Set};
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
    pub host_online: bool,
    pub clients_online: usize,
    pub host: Option<RoomDevice>,
    pub clients: Vec<RoomDevice>,
}

/// 登记环境（幂等；首个设备入网/核销时调用）
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
        created_at: Set(now_secs()),
    };
    // String 主键走 exec_without_returning
    Insert::one(row).exec_without_returning(&state.db).await?;
    Ok(())
}

/// 新建环境（生成 room hex8，幂等重试防碰撞）
pub async fn create(state: &AppState, display_name: &str) -> Result<RoomView, AppError> {
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
            created_at: Set(now_secs()),
        };
        Insert::one(row).exec_without_returning(&state.db).await?;
        return Ok(RoomView {
            room: room_id,
            display_name: name,
            created_at: now_secs(),
            host_online: false,
            clients_online: 0,
            host: None,
            clients: Vec::new(),
        });
    }
    Err(AppError::internal("环境 ID 生成冲突"))
}

/// 重命名环境
pub async fn rename(
    state: &AppState,
    room_id: &str,
    display_name: &str,
) -> Result<(), AppError> {
    let Some(row) = room::Entity::find_by_id(room_id).one(&state.db).await? else {
        return Err(AppError::not_found("环境不存在"));
    };
    let mut am = row.into_active_model();
    am.display_name = Set(Some(display_name.trim().to_string()));
    am.update(&state.db).await?;
    Ok(())
}

/// 环境列表（含 host/client 分组与实时在线态）
pub async fn list(state: &AppState) -> Result<Vec<RoomView>, AppError> {
    let rooms = room::Entity::find().all(&state.db).await?;
    let devices = device::Entity::find().all(&state.db).await?;
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
            host_online: host.as_ref().map(|h| h.online).unwrap_or(false),
            clients_online,
            host,
            clients,
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    Ok(out)
}
