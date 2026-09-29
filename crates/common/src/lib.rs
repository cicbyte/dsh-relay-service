//! 公共层（对齐 byte-admin 的 byte-common）：配置、响应壳、错误、JWT、
//! 身份、分页、运行时状态与连接枢纽。

pub mod config;
pub mod error;
pub mod hub;
pub mod identity;
pub mod jwt;
pub mod pagination;
pub mod response;
pub mod state;
pub mod util;

pub use error::AppError;
pub use response::{Resp, CODE_SUCCESS};
pub use state::AppState;
