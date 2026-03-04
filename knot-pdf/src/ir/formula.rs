//! 公式 IR 数据结构

use serde::{Deserialize, Serialize};

use super::BBox;

/// 公式类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormulaType {
    /// 行内公式 $...$
    Inline,
    /// 行间公式（独占一行）$$...$$
    Display,
}

impl Default for FormulaType {
    fn default() -> Self {
        Self::Inline
    }
}

/// 公式 IR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaIR {
    /// 公式 ID
    pub formula_id: String,
    /// 页码索引
    pub page_index: usize,
    /// 边界框
    pub bbox: BBox,
    /// 公式类型
    pub formula_type: FormulaType,
    /// 检测置信度 (0.0 ~ 1.0)
    pub confidence: f32,
    /// 原始文本（从 PDF 中提取的字符）
    pub raw_text: String,
    /// 识别后的 LaTeX（Phase B 模型识别后填入）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latex: Option<String>,
    /// 公式编号（如 "(1)"、"(2.3)"）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equation_number: Option<String>,
    /// 公式区域内的文本块 ID（需从正文 blocks 中剔除）
    #[serde(default)]
    pub contained_block_ids: Vec<String>,
}

impl FormulaIR {
    /// 获取公式的显示文本
    ///
    /// 优先返回 LaTeX，回退到原始文本
    pub fn display_text(&self) -> &str {
        if let Some(ref latex) = self.latex {
            latex.as_str()
        } else {
            &self.raw_text
        }
    }

    /// 渲染为 Markdown 格式
    pub fn to_markdown(&self) -> String {
        let text = self.display_text();
        match self.formula_type {
            FormulaType::Inline => format!("${}$", text),
            FormulaType::Display => {
                let mut s = String::from("$$\n");
                s.push_str(text);
                s.push_str("\n$$");
                if let Some(ref num) = self.equation_number {
                    s.push_str(&format!("  {}", num));
                }
                s
            }
        }
    }
}
