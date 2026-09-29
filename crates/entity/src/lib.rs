//! 数据实体（对齐 byte-entity）：管理账号 / 设备 / 配对码 / 审计日志 / 环境

pub mod admin_user;
pub mod audit_log;
pub mod device;
pub mod pairing_code;
pub mod room;

pub use sea_orm;
