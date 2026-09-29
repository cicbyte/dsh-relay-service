use serde::{Deserialize, Serialize};

/// 分页查询参数（对齐 byte-admin 的 pagination）
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct PageQuery {
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

impl PageQuery {
    pub fn page(&self) -> u64 {
        self.page.unwrap_or(1).max(1)
    }

    /// 单页上限 200，防拖库式大页
    pub fn page_size(&self) -> u64 {
        self.page_size.unwrap_or(20).clamp(1, 200)
    }

    pub fn offset(&self) -> u64 {
        (self.page() - 1) * self.page_size()
    }
}

/// 分页结果
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

impl<T: Serialize> Page<T> {
    pub fn new(items: Vec<T>, total: u64, q: &PageQuery) -> Self {
        Self {
            items,
            total,
            page: q.page(),
            page_size: q.page_size(),
        }
    }
}
