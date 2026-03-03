pub mod core;
pub mod formats;
#[cfg(feature = "vision")]
pub mod vision;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// Re-export specific items for easier access
pub use core::dispatcher::IndexDispatcher;

/// PageNode 是 PageIndex 逻辑树的最小单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageNode {
    pub node_id: String,             // 唯一标识符（如 "0001"）
    pub title: String,               // 标题或节点名称
    pub level: u32,                  // 树层级（1 为根，2 为子章节...）
    pub content: String,             // 节点的原始文本或 Markdown
    pub summary: Option<String>,     // LLM 生成的摘要（可选）
    pub embedding: Option<Vec<f32>>, // 节点的向量表示
    pub metadata: NodeMeta,          // 节点的元数据
    pub children: Vec<PageNode>,     // 子节点
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub file_path: String,
    pub page_number: Option<u32>, // PDF 专用
    pub line_number: Option<u32>, // MD/Word 专用
    pub token_count: usize,
    pub extra: HashMap<String, String>, // 存储坐标、图片 OCR 等额外信息
}

#[derive(Debug, thiserror::Error)]
pub enum PageIndexError {
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Parsing error: {0}")]
    ParseError(String),
    #[error("Vision error: {0}")]
    VisionError(String),
    #[error("Embedding error: {0}")]
    EmbeddingError(String),
}

/// 视觉输出的枚举：兼容不同模型的输出风格
pub enum VisionOutput {
    /// 模式 A：模型直接给出了带结构的 Markdown 内容（如 OCRFlux-3B）
    StructuredMarkdown(String),

    /// 模式 B：模型只给出了坐标和标签，需要 pageindex-rs 进一步处理（如 Florence-2）
    LayoutElements(Vec<LayoutElement>),
}

pub struct LayoutElement {
    pub label: SemanticLabel, // 语义标签：Heading, Table, Image, Text
    pub bbox: [f32; 4],
    pub content: String, // 识别出的文本
}

pub enum SemanticLabel {
    Heading(u32), // 层级，如 1 代表 #
    Table,
    Image,
    Paragraph,
}

pub trait VisionProvider: Send + Sync {
    /// 核心接口：将图像直接解析为结构化数据
    /// 支持两种返回模式：
    /// 1. 结构化 Markdown（适合端到端模型如 Qwen-VL）
    /// 2. 原始布局元素（适合 Florence-2 这种需要后端重组的模型）
    fn process_page(&self, image_bytes: &[u8]) -> Result<VisionOutput, PageIndexError>;
}

use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 核心接口：输入文本，输出摘要
    /// 核心接口：输入文本，输出摘要
    async fn generate_summary(&self, text: &str) -> Result<String, PageIndexError>;

    /// 通用接口：输入 Prompt，输出内容
    async fn generate_content(&self, prompt: &str) -> Result<String, PageIndexError>;

    /// 多模态接口：输入 Prompt 和图片数据，输出内容
    async fn generate_content_with_image(
        &self,
        prompt: &str,
        image_data: &[u8],
    ) -> Result<String, PageIndexError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// 核心接口：输入文本，输出向量
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, PageIndexError>;
}

pub struct PageIndexConfig<'a> {
    /// 外部注入的视觉引擎（可选）
    /// 如果为 None，解析器将回退到纯文本提取模式
    pub vision_provider: Option<&'a dyn VisionProvider>,

    /// 外部注入的 LLM 引擎（可选）
    pub llm_provider: Option<&'a dyn LlmProvider>,

    /// 外部注入的 Embedding 引擎（可选）
    pub embedding_provider: Option<&'a dyn EmbeddingProvider>,

    /// 节点合并阈值（以 Token 数为单位）
    /// 核心逻辑：如果节点内容过少，其内容将合并至父节点以优化检索精度
    pub min_token_threshold: usize,

    /// 触发摘要生成的最小 Token 数
    pub summary_token_threshold: usize,

    /// 摘要生成开关
    /// 若开启，解析过程中会标记需要摘要的节点（实际摘要可由外部 LLM 异步完成）
    pub enable_auto_summary: bool,

    /// 文档语言偏好（辅助 OCR 和语义分割）
    pub default_language: String,

    /// 进度回调：(current_page, total_pages) — 使用 Arc 以便传递给 Pipeline
    pub progress_callback: Option<std::sync::Arc<dyn Fn(usize, usize) + Send + Sync>>,

    /// 逐页内容回调：(page_index, total_pages, markdown_content)
    /// 用于在 PDF 解析过程中实时推送每页的 markdown 内容
    pub page_content_callback: Option<std::sync::Arc<dyn Fn(usize, usize, String) + Send + Sync>>,

    // === PDF 专属配置（集成 knot-pdf） ===
    /// 是否启用 PDF OCR（扫描件回退）
    pub pdf_ocr_enabled: bool,

    /// PaddleOCR 模型目录路径（包含 det.onnx / rec.onnx / ppocrv5_dict.txt）
    pub pdf_ocr_model_dir: Option<String>,

    /// Vision LLM API URL（用于复杂排版页面的智能理解）
    /// 格式：OpenAI 兼容，如 "http://127.0.0.1:18080/v1/chat/completions"
    pub pdf_vision_api_url: Option<String>,

    /// Vision LLM 模型名称（如 "OCRFlux-3B", "gpt-4o"）
    pub pdf_vision_model: Option<String>,

    /// PDF 页码过滤（仅解析指定页面，0-indexed）
    pub pdf_page_indices: Option<Vec<usize>>,
}

impl<'a> PageIndexConfig<'a> {
    /// Create a new PageIndexConfig with sensible defaults.
    /// - `enable_auto_summary`: true (summary generation enabled by default)
    /// - `min_token_threshold`: 50
    /// - `summary_token_threshold`: 100
    /// - `default_language`: "zh"
    pub fn new() -> Self {
        Self {
            vision_provider: None,
            llm_provider: None,
            embedding_provider: None,
            min_token_threshold: 50,
            summary_token_threshold: 100,
            enable_auto_summary: true, // 默认开启摘要生成
            default_language: "zh".to_string(),
            progress_callback: None,
            page_content_callback: None,
            pdf_ocr_enabled: false,
            pdf_ocr_model_dir: None,
            pdf_vision_api_url: None,
            pdf_vision_model: None,
            pdf_page_indices: None,
        }
    }

    /// Builder method to set LLM provider.
    pub fn with_llm_provider(mut self, provider: &'a dyn LlmProvider) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// Builder method to set embedding provider.
    pub fn with_embedding_provider(mut self, provider: &'a dyn EmbeddingProvider) -> Self {
        self.embedding_provider = Some(provider);
        self
    }

    /// Builder method to set vision provider.
    pub fn with_vision_provider(mut self, provider: &'a dyn VisionProvider) -> Self {
        self.vision_provider = Some(provider);
        self
    }

    /// Builder method to disable auto summary.
    pub fn without_auto_summary(mut self) -> Self {
        self.enable_auto_summary = false;
        self
    }

    /// Builder method to enable PDF OCR.
    pub fn with_pdf_ocr(mut self, model_dir: Option<&str>) -> Self {
        self.pdf_ocr_enabled = true;
        self.pdf_ocr_model_dir = model_dir.map(|s| s.to_string());
        self
    }

    /// Builder method to set PDF Vision LLM.
    pub fn with_pdf_vision(mut self, api_url: &str, model: &str) -> Self {
        self.pdf_vision_api_url = Some(api_url.to_string());
        self.pdf_vision_model = Some(model.to_string());
        self
    }

    /// Builder method to set PDF page filter.
    pub fn with_pdf_pages(mut self, pages: Vec<usize>) -> Self {
        self.pdf_page_indices = Some(pages);
        self
    }
}

#[async_trait]
pub trait DocumentParser: Send + Sync {
    /// 检查该解析器是否能处理指定后缀的文件
    fn can_handle(&self, extension: &str) -> bool;

    /// 核心解析函数
    /// input: 文件路径或字节流
    /// config: 包含 VisionProvider 引用及其他转换参数
    async fn parse(
        &self,
        path: &Path,
        config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError>;
}
