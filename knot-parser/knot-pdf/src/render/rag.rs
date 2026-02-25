//! RAG 扁平化导出模块
//!
//! 将 IR 转换为检索友好的扁平文本格式

use crate::ir::{DocumentIR, ImageSource, PageIR};

/// RAG 扁平化导出器
pub struct RagExporter;

/// 导出的行
#[derive(Debug, Clone)]
pub struct RagLine {
    /// 行文本
    pub text: String,
    /// 所在页码
    pub page_index: usize,
    /// 行类型
    pub line_type: RagLineType,
}

/// RAG 行类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RagLineType {
    Block,
    TableRow,
    TableCell,
    Figure,
}

impl RagExporter {
    /// 导出整个文档的 block_lines
    pub fn export_block_lines(doc: &DocumentIR) -> Vec<RagLine> {
        let mut lines = Vec::new();
        for page in &doc.pages {
            lines.extend(Self::export_page_block_lines(page));
        }
        lines
    }

    /// 导出单个页面的 block_lines
    pub fn export_page_block_lines(page: &PageIR) -> Vec<RagLine> {
        let mut lines = Vec::new();
        for block in &page.blocks {
            let text = format!(
                "页={} bbox=({:.0},{:.0},{:.0},{:.0}) {}",
                page.page_index + 1,
                block.bbox.x,
                block.bbox.y,
                block.bbox.width,
                block.bbox.height,
                block.normalized_text
            );
            lines.push(RagLine {
                text,
                page_index: page.page_index,
                line_type: RagLineType::Block,
            });
        }
        lines
    }

    /// 导出整个文档的 table_row_lines
    pub fn export_table_row_lines(doc: &DocumentIR) -> Vec<RagLine> {
        let mut lines = Vec::new();
        for page in &doc.pages {
            for table in &page.tables {
                for row_line in table.to_row_lines() {
                    lines.push(RagLine {
                        text: row_line,
                        page_index: page.page_index,
                        line_type: RagLineType::TableRow,
                    });
                }
            }
        }
        lines
    }

    /// 导出整个文档的 table_cell_lines
    pub fn export_table_cell_lines(doc: &DocumentIR) -> Vec<RagLine> {
        let mut lines = Vec::new();
        for page in &doc.pages {
            for table in &page.tables {
                for cell_line in table.to_kv_lines() {
                    lines.push(RagLine {
                        text: cell_line,
                        page_index: page.page_index,
                        line_type: RagLineType::TableCell,
                    });
                }
            }
        }
        lines
    }

    /// 导出所有类型的行
    pub fn export_all(doc: &DocumentIR) -> Vec<RagLine> {
        let mut lines = Vec::new();
        lines.extend(Self::export_block_lines(doc));
        lines.extend(Self::export_table_row_lines(doc));
        lines.extend(Self::export_table_cell_lines(doc));
        lines.extend(Self::export_figure_lines(doc));
        lines
    }

    /// 导出整个文档的图表文字描述
    pub fn export_figure_lines(doc: &DocumentIR) -> Vec<RagLine> {
        let mut lines = Vec::new();
        for page in &doc.pages {
            for image in &page.images {
                if image.source == ImageSource::FigureRegion {
                    if let Some(ocr_text) = &image.ocr_text {
                        let text = format!(
                            "页={} type=figure id={} {}",
                            page.page_index + 1,
                            image.image_id,
                            ocr_text
                        );
                        lines.push(RagLine {
                            text,
                            page_index: page.page_index,
                            line_type: RagLineType::Figure,
                        });
                    }
                }
            }
        }
        lines
    }
}
