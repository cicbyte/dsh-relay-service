//! 运行指标快照（/api/metrics；管理台/巡检脚本抓取）
//!
//! 进程内存计数：重启归零，不做持久化聚合——本服务的可观测性目标是
//! 「现在忙不忙、通道水位多高」，趋势图交给外部抓取方自行累积。

use axum::extract::State;
use axum::{Extension, Json};
use relay_common::identity::AdminIdentity;
use relay_common::util::now_secs;
use relay_common::{AppError, AppState, Resp};

pub async fn metrics(
    State(state): State<AppState>,
    Extension(_identity): Extension<AdminIdentity>,
) -> Result<Json<Resp<serde_json::Value>>, AppError> {
    let (rooms, hosts, clients) = state.hub.stats();
    let (rings, ring_bytes) = state.hub.buf_stats();
    let m = &state.metrics;
    let out = serde_json::json!({
        "uptimeSecs": now_secs() - m.started_at,
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
    Ok(Json(Resp::ok(out)))
}
