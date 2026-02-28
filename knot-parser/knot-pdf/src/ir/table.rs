//! 表格 IR

use serde::{Deserialize, Serialize};

use super::{BBox, CellType, ExtractionMode};

/// 表格单元格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCell {
    /// 行索引
    pub row: usize,
    /// 列索引
    pub col: usize,
    /// 文本内容
    pub text: String,
    /// 单元格类型
    #[serde(default)]
    pub cell_type: CellType,
    /// 跨行数
    #[serde(default = "default_span")]
    pub rowspan: usize,
    /// 跨列数
    #[serde(default = "default_span")]
    pub colspan: usize,
}

fn default_span() -> usize {
    1
}

/// 表格行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRow {
    /// 行索引
    pub row_index: usize,
    /// 该行的单元格
    pub cells: Vec<TableCell>,
}

/// 表格 IR（RAG 核心）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableIR {
    /// 表格 ID
    pub table_id: String,
    /// 所在页码
    pub page_index: usize,
    /// 边界框
    pub bbox: BBox,
    /// 抽取模式
    #[serde(default)]
    pub extraction_mode: ExtractionMode,
    /// 表头（列名列表）
    pub headers: Vec<String>,
    /// 行数据
    pub rows: Vec<TableRow>,
    /// 单元格类型映射（按列）
    pub column_types: Vec<CellType>,
    /// 回退文本（必须有）—— 即使结构化成功也要生成
    pub fallback_text: String,
    /// 表格置信度评估（M11 新增）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<crate::table::enhance::TableConfidence>,
}

impl TableIR {
    /// 导出为 CSV 字符串
    pub fn to_csv(&self) -> String {
        let mut lines = Vec::new();
        if !self.headers.is_empty() {
            lines.push(self.headers.join(","));
        }
        for row in &self.rows {
            let cells: Vec<&str> = row.cells.iter().map(|c| c.text.as_str()).collect();
            lines.push(cells.join(","));
        }
        lines.join("\n")
    }

    /// 导出为 Markdown 表格
    pub fn to_markdown(&self) -> String {
        let mut lines = Vec::new();
        if !self.headers.is_empty() {
            lines.push(format!("| {} |", self.headers.join(" | ")));
            lines.push(format!(
                "| {} |",
                self.headers
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        for row in &self.rows {
            let cells: Vec<&str> = row.cells.iter().map(|c| c.text.as_str()).collect();
            lines.push(format!("| {} |", cells.join(" | ")));
        }
        lines.join("\n")
    }

    /// 检测表格是否包含合并单元格（rowspan > 1 或 colspan > 1）
    pub fn has_merged_cells(&self) -> bool {
        // 检查表头行（headers 本身没有 span 信息，但第一行数据可能有）
        for row in &self.rows {
            for cell in &row.cells {
                if cell.rowspan > 1 || cell.colspan > 1 {
                    return true;
                }
            }
        }
        false
    }

    /// 智能导出：简单表格用 Markdown，复杂表格（有合并单元格）用 HTML
    pub fn to_markdown_or_html(&self) -> String {
        if self.has_merged_cells() {
            self.to_html()
        } else {
            self.to_markdown()
        }
    }

    /// 导出为 HTML 表格（支持 rowspan/colspan）
    pub fn to_html(&self) -> String {
        let mut html = String::new();
        html.push_str("<table>\n");

        // 表头行
        if !self.headers.is_empty() {
            html.push_str("  <thead>\n    <tr>");
            for header in &self.headers {
                html.push_str(&format!("<th>{}</th>", escape_html(header)));
            }
            html.push_str("</tr>\n  </thead>\n");
        }

        // 数据行
        if !self.rows.is_empty() {
            html.push_str("  <tbody>\n");
            for row in &self.rows {
                html.push_str("    <tr>");
                for cell in &row.cells {
                    let mut attrs = String::new();
                    if cell.rowspan > 1 {
                        attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                    }
                    if cell.colspan > 1 {
                        attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                    }
                    html.push_str(&format!("<td{attrs}>{}</td>", escape_html(&cell.text)));
                }
                html.push_str("</tr>\n");
            }
            html.push_str("  </tbody>\n");
        }

        html.push_str("</table>");
        html
    }

    /// 导出为 KV 行格式（用于 RAG 检索）
    pub fn to_kv_lines(&self) -> Vec<String> {
        let mut result = Vec::new();
        for row in &self.rows {
            for cell in &row.cells {
                let col_name = self
                    .headers
                    .get(cell.col)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{}", cell.col));
                result.push(format!(
                    "表={} 页={} 行={} 列={} 值={} 类型={:?}",
                    self.table_id, self.page_index, cell.row, col_name, cell.text, cell.cell_type
                ));
            }
        }
        result
    }

    /// 导出为行级 KV 格式
    pub fn to_row_lines(&self) -> Vec<String> {
        let mut result = Vec::new();
        for row in &self.rows {
            let mut parts = vec![
                format!("表={}", self.table_id),
                format!("页={}", self.page_index),
            ];
            for cell in &row.cells {
                let col_name = self
                    .headers
                    .get(cell.col)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{}", cell.col));
                parts.push(format!("{}={}", col_name, cell.text));
            }
            result.push(parts.join(" "));
        }
        result
    }

    /// 估算此表格在内存中的大小（字节数）
    ///
    /// 包含表头、行数据、单元格文本等的估算值。
    /// 仅为粗略估算，用于内存监控和大表格预警。
    pub fn estimated_memory_bytes(&self) -> usize {
        let mut size = 0usize;

        // table_id + fallback_text
        size += self.table_id.len();
        size += self.fallback_text.len();

        // headers
        for h in &self.headers {
            size += h.len() + std::mem::size_of::<String>();
        }

        // rows + cells
        for row in &self.rows {
            size += std::mem::size_of::<TableRow>();
            for cell in &row.cells {
                size += cell.text.len() + std::mem::size_of::<TableCell>();
            }
        }

        // column_types
        size += self.column_types.len() * std::mem::size_of::<CellType>();

        // struct 本身的固定开销
        size += std::mem::size_of::<TableIR>();

        size
    }

    /// 是否为大表格（超过指定行数或估算内存阈值）
    ///
    /// 默认阈值：超过 100 行或估算内存 > 100KB
    pub fn is_large(&self) -> bool {
        let total_rows: usize = self.rows.len();
        let memory = self.estimated_memory_bytes();
        total_rows > 100 || memory > 100 * 1024
    }

    /// 总单元格数
    pub fn cell_count(&self) -> usize {
        self.rows.iter().map(|r| r.cells.len()).sum()
    }
}

/// HTML 特殊字符转义
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
