//! 内置管理控制台（单文件 HTML）

/// GET / → 控制台
pub async fn console() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../admin.html"))
}
