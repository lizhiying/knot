//! 文档 IR

use serde::{Deserialize, Serialize};

use super::{Diagnostics, PageIR};

/// 文档大纲项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    pub page_index: Option<usize>,
    pub children: Vec<OutlineItem>,
}

/// 文档元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
}

/// 文档 IR（顶层结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentIR {
    /// 文档 ID（基于文件内容 hash）
    pub doc_id: String,
    /// 文档元数据
    #[serde(default)]
    pub metadata: DocumentMetadata,
    /// 文档大纲
    #[serde(default)]
    pub outline: Vec<OutlineItem>,
    /// 页面列表
    pub pages: Vec<PageIR>,
    /// 全局诊断信息
    #[serde(default)]
    pub diagnostics: Diagnostics,
}
