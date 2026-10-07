//! dsh-relay-service：WS 转发数据面 + axum 管理 API / 内置控制台。
//!
//! 架构对齐 byte-admin（Rust 后台管理模板）：
//! - Cargo workspace：apps/relay + crates/{common,entity,migration,middleware,service}
//! - axum 0.8 + tower-http（trace/cors/timeout）、sea-orm(sqlite) + 迁移种子
//! - 管理端 bcrypt 密码 + JWT（access/refresh + nonce 轮转）、登录限速、oplog 审计
//! - TOML 分层配置（config/relay.toml < 环境变量）、tracing 日志、优雅停机

use relay_common::config::AppConfig;
use relay_common::AppState;

mod handlers;
mod router;
mod ws;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // 跨平台做法（对齐 byte-admin）：大栈线程跑 tokio 运行时
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio 运行时创建失败")
                .block_on(run())
        })
        .expect("启动线程创建失败")
        .join()
        .expect("服务线程异常退出");
}

async fn run() {
    let config = AppConfig::load("relay", "RELAY", 8788);
    // auth_mode fail-closed：只认 device | code，其他值（拼写错误等）直接拒绝启动——
    // 静默降级到共享码模式等于把数据面准入整个放开，宁可不服务
    match config.ws.auth_mode.as_str() {
        "device" | "code" => {}
        other => {
            eprintln!("auth_mode 非法：{other:?}（只支持 \"device\" | \"code\"），拒绝启动");
            return;
        }
    }
    let db = relay_migration::init_db_connect(&config.database.url)
        .await
        .expect("数据库初始化（迁移+种子）失败");
    if let Err(e) = relay_migration::cleanup_audit(&db, config.audit.retain_days).await {
        tracing::warn!(error = %e, "审计清理失败");
    }

    if config.ws.auth_mode == "code" {
        tracing::warn!("auth_mode=code 是兼容模式（共享配对码即准入），建议迁移到设备令牌");
    }
    if config.server.host == "0.0.0.0" || config.server.host == "::" {
        tracing::warn!("管理台对外监听中：公网部署建议置于 TLS 反代（Caddy/Nginx）之后");
    }

    let state = AppState::new(db, config.clone());

    // 审计批量写刷盘（conn.open/close 高频事件攒批落库）+ 保留期周期清理
    //（启动时已清过一次，此后每 6h 一轮）
    relay_service::audit::start_batch_writer(state.db.clone());
    {
        let db = state.db.clone();
        let retain_days = config.audit.retain_days;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await; // interval 首个 tick 立即到期，跳过（启动时刚清过）
            loop {
                tick.tick().await;
                if let Err(e) = relay_migration::cleanup_audit(&db, retain_days).await {
                    tracing::warn!(error = %e, "审计周期清理失败");
                }
            }
        });
    }

    // WS 数据面（与历史 dsh-relay-v1 协议兼容）：bind 前置到主流程——此前 bind
    // 在 spawn 内部 expect，端口被占/权限不足时任务静默消失（管理面活着、数据面
    // 没了），排障只能靠猜；现在 bind 失败直接 panic 退出
    let ws_addr = format!("{}:{}", config.ws.host, config.ws.port);
    let ws_listener = tokio::net::TcpListener::bind(&ws_addr)
        .await
        .unwrap_or_else(|e| panic!("ws 监听 {ws_addr} 失败: {e}"));
    tracing::info!(addr = %ws_addr, "ws 数据面监听就绪");
    {
        let st = state.clone();
        tokio::spawn(async move { ws::serve(ws_addr, ws_listener, st).await });
    }

    // 管理面：内置控制台 + 管理 API
    let app = router::build(state.clone());
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("监听 {addr} 失败: {e}"));
    tracing::info!(%addr, "管理面已启动（控制台 + API）");
    if config.server.swagger {
        tracing::info!("OpenAPI: http://{addr}/api/openapi.json");
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("HTTP 服务异常退出");
}

/// 优雅停机：Ctrl+C / SIGTERM 后停止接收新连接
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("安装 Ctrl+C 信号处理器失败");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("安装 SIGTERM 信号处理器失败")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
