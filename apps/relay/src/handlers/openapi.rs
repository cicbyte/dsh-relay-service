//! OpenAPI 文档（手写生成，避免宏依赖；swagger=false 时路由不注册）

use axum::Json;

pub async fn spec() -> Json<serde_json::Value> {
    Json(build_spec())
}

fn build_spec() -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "dsh-relay-service API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "DSH 中继服务管理 API：认证 / 设备 / 配对码 / 审计 / 概览"
        },
        "servers": [{ "url": "/" }],
        "paths": {
            "/api/health": { "get": { "tags": ["健康检查"], "summary": "简单健康检查", "responses": { "200": { "description": "ok" } } } },
            "/api/health/detail": { "get": { "tags": ["健康检查"], "summary": "详细健康检查（DB/WS 统计）", "responses": { "200": { "description": "ok" } } } },
            "/api/auth/login": { "post": { "tags": ["认证"], "summary": "管理端登录（限速）", "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/LoginReq" } } } }, "responses": { "200": { "description": "access/refresh 令牌" }, "401": { "description": "用户名或密码错误" }, "429": { "description": "尝试过于频繁" } } } },
            "/api/auth/refresh": { "post": { "tags": ["认证"], "summary": "刷新令牌（nonce 轮转）", "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RefreshReq" } } } }, "responses": { "200": { "description": "新令牌对" } } } },
            "/api/auth/logout": { "post": { "tags": ["认证"], "summary": "登出（refresh 失效）", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "ok" } } } },
            "/api/auth/profile": { "get": { "tags": ["认证"], "summary": "当前登录信息", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "ok" } } } },
            "/api/auth/password": { "post": { "tags": ["认证"], "summary": "修改管理密码", "security": [{ "bearerAuth": [] }], "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ChangePasswordReq" } } } }, "responses": { "200": { "description": "ok" } } } },
            "/api/status": { "get": { "tags": ["概览"], "summary": "运行概览（设备/在线/WS 统计）", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "ok" } } } },
            "/api/devices": { "get": { "tags": ["设备"], "summary": "设备列表（含在线状态）", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "ok" } } } },
            "/api/devices/{id}/revoke": { "post": { "tags": ["设备"], "summary": "吊销设备（即时踢线）", "security": [{ "bearerAuth": [] }], "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }], "responses": { "200": { "description": "ok" }, "404": { "description": "设备不存在" } } } },
            "/api/devices/{id}/rotate": { "post": { "tags": ["设备"], "summary": "轮换设备令牌（明文只返回一次）", "security": [{ "bearerAuth": [] }], "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }], "responses": { "200": { "description": "ok" } } } },
            "/api/devices/{id}": { "delete": { "tags": ["设备"], "summary": "删除设备", "security": [{ "bearerAuth": [] }], "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }], "responses": { "200": { "description": "ok" } } } },
            "/api/pairing-codes": { "post": { "tags": ["配对"], "summary": "签发一次性配对码（600s/单次/失败熔断）", "security": [{ "bearerAuth": [] }], "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PairingReq" } } } }, "responses": { "200": { "description": "ok" } } } },
            "/api/audit": { "get": { "tags": ["审计"], "summary": "审计日志（时间倒序）", "security": [{ "bearerAuth": [] }], "parameters": [{ "name": "limit", "in": "query", "schema": { "type": "integer", "default": 50, "maximum": 500 } }], "responses": { "200": { "description": "ok" } } } }
        },
        "components": {
            "securitySchemes": {
                "bearerAuth": { "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }
            },
            "schemas": {
                "Resp": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "integer" },
                        "result": {},
                        "message": { "type": "string" }
                    }
                },
                "LoginReq": { "type": "object", "required": ["username", "password"], "properties": { "username": { "type": "string" }, "password": { "type": "string" } } },
                "RefreshReq": { "type": "object", "required": ["refresh_token"], "properties": { "refresh_token": { "type": "string" } } },
                "ChangePasswordReq": { "type": "object", "required": ["old_password", "new_password"], "properties": { "old_password": { "type": "string" }, "new_password": { "type": "string" } } },
                "PairingReq": { "type": "object", "required": ["role"], "properties": { "role": { "type": "string", "enum": ["host", "client"] }, "name": { "type": "string" } } }
            }
        }
    })
}
