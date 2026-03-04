//! VLM (Vision Language Model) 外部调用接口
//!
//! 为扫描件、复杂图表等 Fast Track 无法处理的页面，
//! 提供调用外部 VLM API 的能力。

use crate::error::PdfError;
use crate::ir::{BlockIR, TableIR};

/// VLM 解析结果
#[derive(Debug, Clone)]
pub struct VlmParseResult {
    /// VLM 返回的 Markdown 文本
    pub markdown: String,
    /// 解析出的文本块（从 Markdown 结构化得到）
    pub blocks: Vec<BlockIR>,
    /// 解析出的表格
    pub tables: Vec<TableIR>,
    /// 整体置信度
    pub confidence: f32,
}

/// VLM 后端 trait
pub trait VlmBackend: Send + Sync {
    /// 发送页面图片，获取结构化解析结果
    fn parse_page_image(&self, image_data: &[u8], prompt: &str)
        -> Result<VlmParseResult, PdfError>;

    /// 后端名称
    fn name(&self) -> &str;
}

/// Mock VLM 后端（测试用）
pub struct MockVlmBackend {
    /// 固定返回的 Markdown 内容
    pub mock_markdown: String,
    /// 固定返回的置信度
    pub mock_confidence: f32,
}

impl MockVlmBackend {
    pub fn new() -> Self {
        Self {
            mock_markdown: "# Mock VLM Output\n\nThis is a mock result.".to_string(),
            mock_confidence: 0.9,
        }
    }

    pub fn with_content(markdown: &str, confidence: f32) -> Self {
        Self {
            mock_markdown: markdown.to_string(),
            mock_confidence: confidence,
        }
    }
}

impl VlmBackend for MockVlmBackend {
    fn parse_page_image(
        &self,
        _image_data: &[u8],
        _prompt: &str,
    ) -> Result<VlmParseResult, PdfError> {
        // 将 Markdown 简单转化为 BlockIR
        let blocks = markdown_to_blocks(&self.mock_markdown);

        Ok(VlmParseResult {
            markdown: self.mock_markdown.clone(),
            blocks,
            tables: vec![],
            confidence: self.mock_confidence,
        })
    }

    fn name(&self) -> &str {
        "mock_vlm"
    }
}

/// 将简单 Markdown 文本转化为 BlockIR 列表
///
/// 注意：这是一个简化实现，仅处理段落和标题。
/// 生产环境中应使用完整的 Markdown 解析器。
fn markdown_to_blocks(markdown: &str) -> Vec<BlockIR> {
    use crate::ir::{BBox, BlockRole, TextLine, TextSpan};

    let mut blocks = Vec::new();
    let mut y_offset = 72.0f32;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (role, text) = if let Some(title) = trimmed.strip_prefix("# ") {
            (BlockRole::Title, title.to_string())
        } else if let Some(heading) = trimmed.strip_prefix("## ") {
            (BlockRole::Heading, heading.to_string())
        } else if let Some(heading) = trimmed.strip_prefix("### ") {
            (BlockRole::Heading, heading.to_string())
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            (BlockRole::List, trimmed.to_string())
        } else {
            (BlockRole::Body, trimmed.to_string())
        };

        let block_id = format!("vlm_b{}", blocks.len());
        blocks.push(BlockIR {
            block_id,
            bbox: BBox::new(72.0, y_offset, 468.0, 16.0),
            role,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.clone(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(BBox::new(72.0, y_offset, 468.0, 16.0)),
            }],
            normalized_text: text,
        });

        y_offset += 20.0;
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_vlm_backend() {
        let backend = MockVlmBackend::new();
        let result = backend.parse_page_image(&[], "parse this page").unwrap();
        assert!(result.markdown.contains("Mock VLM Output"));
        assert_eq!(result.confidence, 0.9);
        assert!(!result.blocks.is_empty());
    }

    #[test]
    fn test_mock_vlm_custom_content() {
        let backend = MockVlmBackend::with_content("# Title\n\nParagraph text.", 0.85);
        let result = backend.parse_page_image(&[], "").unwrap();
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].role, crate::ir::BlockRole::Title);
        assert_eq!(result.blocks[1].role, crate::ir::BlockRole::Body);
        assert_eq!(result.confidence, 0.85);
    }

    #[test]
    fn test_markdown_to_blocks() {
        let md = "# Title\n\n## Section\n\n- Item 1\n- Item 2\n\nBody text.";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0].role, crate::ir::BlockRole::Title);
        assert_eq!(blocks[1].role, crate::ir::BlockRole::Heading);
        assert_eq!(blocks[2].role, crate::ir::BlockRole::List);
        assert_eq!(blocks[3].role, crate::ir::BlockRole::List);
        assert_eq!(blocks[4].role, crate::ir::BlockRole::Body);
    }

    #[test]
    fn test_vlm_backend_name() {
        let backend = MockVlmBackend::new();
        assert_eq!(backend.name(), "mock_vlm");
    }
}
