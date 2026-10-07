//! 运行指标快照（/api/metrics；管理台/巡检脚本抓取）
//!
//! 进程内存计数：重启归零，不做持久化聚合——本服务的可观测性目标是
//! 「现在忙不忙、通道水位多高」，趋势图交给外部抓取方自行累积。
//!
//! 归属裁剪（对齐 /api/status，第二轮审查 #920）：admin 全量；user 只拿到
//! 自己环境的 ws/房间统计——进程级流量计数与全局环统计不向普通用户暴露。

use axum::extract::State;
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::util::now_secs;
use relay_common::{AppError, AppState, Resp};

pub async fn metrics(
    State(state): State<AppState>,
    Extension(identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<serde_json::Value>>, AppError> {
    let m = &state.metrics;
    let uptime = now_secs() - m.started_at;
    if identity.is_admin() {
        let (rooms, hosts, clients) = state.hub.stats();
        let (rings, ring_bytes) = state.hub.buf_stats();
        let out = serde_json::json!({
            "uptimeSecs": uptime,
            "wsConnects": m.get(&m.ws_connects),
            "framesIn": m.get(&m.frames_in),
            "framesRelayed": m.get(&m.frames_relayed),
            "httpProxied": m.get(&m.http_proxied),
            "rooms": rooms,
            "wsConns": hosts + clients,
            "wsHosts": hosts,
            "wsClients": clients,
            "resumeRings": rings,
            "resumeRingBytes": ring_bytes,
        });
        return Ok(Json(Resp::ok(out)));
    }
    // user 角色：只统计自己名下环境（原实现无裁剪，普通用户可见全局统计）
    let owned = relay_service::health::owned_rooms(&state, identity.user_id).await?;
    let (rooms, hosts, clients) = state.hub.stats_scoped(&owned);
    let out = serde_json::json!({
        "uptimeSecs": uptime,
        "rooms": rooms,
        "wsConns": hosts + clients,
        "wsHosts": hosts,
        "wsClients": clients,
    });
    Ok(Json(Resp::ok(out)))
}
