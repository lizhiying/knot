//! Store Trait 定义

use crate::error::PdfError;
use crate::ir::{PageDiagnostics, PageIR};
use serde::{Deserialize, Serialize};

/// 页面处理状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PageStatus {
    NotStarted,
    InProgress,
    Done,
    Failed(String),
}

/// 存储后端 Trait
pub trait Store: Send + Sync {
    /// 保存页面 IR
    fn save_page(&self, doc_id: &str, page_index: usize, page_ir: &PageIR) -> Result<(), PdfError>;

    /// 加载页面 IR
    fn load_page(&self, doc_id: &str, page_index: usize) -> Result<Option<PageIR>, PdfError>;

    /// 获取页面处理状态
    fn get_status(&self, doc_id: &str, page_index: usize) -> Result<PageStatus, PdfError>;

    /// 更新页面处理状态
    fn update_status(
        &self,
        doc_id: &str,
        page_index: usize,
        status: PageStatus,
    ) -> Result<(), PdfError>;

    /// 保存页面诊断信息
    fn save_diagnostics(
        &self,
        doc_id: &str,
        page_index: usize,
        diagnostics: &PageDiagnostics,
    ) -> Result<(), PdfError>;

    /// 获取最后一个完成的页码
    fn get_last_completed_page(&self, doc_id: &str) -> Result<Option<usize>, PdfError>;
}
