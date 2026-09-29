use serde::Serialize;

/// 业务成功码（与 HTTP 状态码保持一致语义）
pub const CODE_SUCCESS: i32 = 200;

/// 统一响应包裹：`{code, result, message}`（对齐 byte-admin）
#[derive(Serialize, Debug, utoipa::ToSchema)]
pub struct Resp<T> {
    pub code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    pub message: String,
}

impl<T> Resp<T> {
    pub fn ok(result: T) -> Self {
        Self {
            code: CODE_SUCCESS,
            result: Some(result),
            message: "ok".to_string(),
        }
    }
}

impl Resp<()> {
    /// 无数据体的成功响应（result 序列化时省略）
    pub fn ok_empty() -> Self {
        Self {
            code: CODE_SUCCESS,
            result: None,
            message: "ok".to_string(),
        }
    }
}

/// OpenAPI 文档中的空结果占位（注解用，避免 unit 类型导致宏展开失败）
#[derive(Serialize, Debug, Default, utoipa::ToSchema)]
pub struct Empty {}
