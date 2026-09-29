//! 健康检查 + 概览统计

use relay_common::{AppError, AppState};
use relay_entity::{device, pairing_code};
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

pub async fn overview(state: &AppState) -> Result<Overview, AppError> {
    use sea_orm::PaginatorTrait;
    let devices_total = device::Entity::find().count(&state.db).await?;
    let devices_revoked = device::Entity::find()
        .filter(device::Column::Revoked.eq(true))
        .count(&state.db)
        .await?;
    let online = device::Entity::find()
        .all(&state.db)
        .await?
        .into_iter()
        .filter(|d| state.hub.device_online(&d.id))
        .count() as u64;
    let now = relay_common::util::now_secs();
    let pairing_active = pairing_code::Entity::find()
        .filter(pairing_code::Column::UsedBy.is_null())
        .filter(pairing_code::Column::ExpiresAt.gt(now))
        .count(&state.db)
        .await?;
    let (rooms, hosts, clients) = state.hub.stats();
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
    })
}
