//! 健康检查 + 概览统计

use relay_common::{AppError, AppState};
use relay_entity::{device, pairing_code, room};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WsStats {
    pub rooms: usize,
    pub hosts: usize,
    pub clients: usize,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthDetail {
    pub status: String,
    pub version: String,
    pub db: String,
    pub ws: WsStats,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub devices_total: u64,
    pub devices_online: u64,
    pub devices_revoked: u64,
    pub pairing_active: u64,
    pub ws: WsStats,
    pub auth_mode: String,
    pub listen_ws: String,
    pub listen_admin: String,
    /// 中继地址提示：WS 端口 + 服务端出网网卡地址（出码弹窗自动推导用）
    pub ws_port: u16,
    pub lan_addr: String,
}

/// 出网网卡地址（UDP connect 不发包，仅选路由）；失败回退 127.0.0.1
fn lan_addr() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            Ok(s.local_addr()?.ip().to_string())
        })
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

pub async fn detail(state: &AppState) -> Result<HealthDetail, AppError> {
    let (rooms, hosts, clients) = state.hub.stats();
    Ok(HealthDetail {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        db: "ok".to_string(),
        ws: WsStats {
            rooms,
            hosts,
            clients,
        },
    })
}

/// 用户名下环境集合（归属裁剪共用：/api/status、/api/metrics，#920）
pub async fn owned_rooms(
    state: &AppState,
    user_id: i64,
) -> Result<std::collections::HashSet<String>, AppError> {
    Ok(room::Entity::find()
        .filter(room::Column::OwnerId.eq(user_id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|r| r.room)
        .collect())
}

/// 运行概览（多用户按归属裁剪：admin 全量，user 只统计自己的环境）
pub async fn overview(
    state: &AppState,
    id: &relay_common::identity::AdminIdentity,
) -> Result<Overview, AppError> {
    let owned: std::collections::HashSet<String> = if id.is_admin() {
        Default::default()
    } else {
        room::Entity::find()
            .filter(room::Column::OwnerId.eq(id.user_id))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|r| r.room)
            .collect()
    };
    let scoped = |r: &str| id.is_admin() || owned.contains(r);
    let devices = device::Entity::find().all(&state.db).await?;
    let devices_total = devices.iter().filter(|d| scoped(&d.room)).count() as u64;
    let devices_revoked = devices
        .iter()
        .filter(|d| scoped(&d.room) && d.revoked)
        .count() as u64;
    let online = devices
        .iter()
        .filter(|d| scoped(&d.room) && state.hub.device_online(&d.id))
        .count() as u64;
    let now = relay_common::util::now_secs();
    let pairing_active = pairing_code::Entity::find()
        .filter(pairing_code::Column::UsedBy.is_null())
        .filter(pairing_code::Column::ExpiresAt.gt(now))
        .all(&state.db)
        .await?
        .into_iter()
        .filter(|p| scoped(p.room.as_deref().unwrap_or_default()))
        .count() as u64;
    let (rooms, hosts, clients) = if id.is_admin() {
        state.hub.stats()
    } else {
        state.hub.stats_scoped(&owned)
    };
    Ok(Overview {
        devices_total,
        devices_online: online,
        devices_revoked,
        pairing_active,
        ws: WsStats {
            rooms,
            hosts,
            clients,
        },
        auth_mode: state.config.ws.auth_mode.clone(),
        listen_ws: format!("{}:{}", state.config.ws.host, state.config.ws.port),
        listen_admin: format!("{}:{}", state.config.server.host, state.config.server.port),
        ws_port: state.config.ws.port,
        lan_addr: lan_addr(),
    })
}
