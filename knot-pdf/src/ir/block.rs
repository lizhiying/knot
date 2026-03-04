//! 文本块 IR

use serde::{Deserialize, Serialize};

use super::{BBox, BlockRole};

/// 文本 span（保留字体信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    pub font_size: Option<f32>,
    pub is_bold: bool,
    pub font_name: Option<String>,
}

/// 文本行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextLine {
    pub spans: Vec<TextSpan>,
    pub bbox: Option<BBox>,
}

impl TextLine {
    /// 获取整行文本
    pub fn text(&self) -> String {
        self.spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("")
    }
}

/// 文本块 IR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIR {
    /// 块 ID
    pub block_id: String,
    /// 边界框
    pub bbox: BBox,
    /// 角色
    #[serde(default)]
    pub role: BlockRole,
    /// 文本行
    pub lines: Vec<TextLine>,
    /// 归一化文本（用于索引）
    pub normalized_text: String,
}

impl BlockIR {
    /// 获取全部文本
    pub fn full_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
