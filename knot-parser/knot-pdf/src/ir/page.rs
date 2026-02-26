//! 页面 IR

use serde::{Deserialize, Serialize};

use super::{BlockIR, FormulaIR, ImageIR, PageDiagnostics, PageSize, PageSource, TableIR, Timings};

/// 页面 IR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageIR {
    /// 页码索引（从 0 开始）
    pub page_index: usize,
    /// 页面尺寸
    pub size: PageSize,
    /// 页面旋转角度
    #[serde(default)]
    pub rotation: f32,
    /// 文本块列表
    pub blocks: Vec<BlockIR>,
    /// 表格列表（可为空）
    #[serde(default)]
    pub tables: Vec<TableIR>,
    /// 图片列表
    #[serde(default)]
    pub images: Vec<ImageIR>,
    /// 公式列表（M12 新增）
    #[serde(default)]
    pub formulas: Vec<FormulaIR>,
    /// 页面诊断信息
    #[serde(default)]
    pub diagnostics: PageDiagnostics,
    /// 文本质量评分（0.0 ~ 1.0）
    #[serde(default)]
    pub text_score: f32,
    /// 是否疑似扫描页
    #[serde(default)]
    pub is_scanned_guess: bool,
    /// 页面来源
    #[serde(default)]
    pub source: PageSource,
    /// 耗时统计
    #[serde(default)]
    pub timings: Timings,
}
