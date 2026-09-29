//! 业务服务层（对齐 byte-service）：认证 / 设备 / 配对码 / 审计 / 健康 / 环境

pub mod audit;
pub mod auth;
pub mod device;
pub mod health;
pub mod pairing;
pub mod room;
pub mod users;
