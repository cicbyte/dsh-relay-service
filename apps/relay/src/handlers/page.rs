//! 前端未构建时的降级提示页（正常部署由 ServeDir 托管 apps/admin-web/dist）

/// SPA 静态目录缺失时的兜底
pub async fn fallback() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>dsh-relay 管理台</title>
<style>body{font:15px/1.8 system-ui;background:#0f1115;color:#e6e9f0;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
.card{background:#171a21;border:1px solid #262b36;border-radius:12px;padding:32px 40px;max-width:560px}
code{background:#10141c;padding:2px 8px;border-radius:6px;font-family:Consolas,monospace}</style></head>
<body><div class="card">
<h2>管理台前端未构建</h2>
<p>服务本身已就绪（API 全部可用），但未找到前端构建产物。构建后刷新即可：</p>
<p><code>cd apps/admin-web &amp;&amp; npm install &amp;&amp; npm run build</code></p>
<p>或用 <code>RELAY_STATIC_DIR</code> / <code>config/relay.toml</code> 的 <code>server.static_dir</code> 指向已有构建产物。</p>
<p class="muted">API 文档：<code>/api/openapi.json</code></p>
</div></body></html>"#,
    )
}
