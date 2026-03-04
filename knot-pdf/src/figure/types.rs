//! 图表区域类型定义

use serde::{Deserialize, Serialize};

use crate::ir::BBox;

/// 图表区域
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureRegion {
    /// 区域 ID
    pub figure_id: String,
    /// 边界框
    pub bbox: BBox,
    /// 区域内 Path objects 数量
    pub path_count: usize,
    /// 区域内文字块 ID 列表（需要从正文 blocks 中剔除）
    pub contained_block_ids: Vec<String>,
    /// 置信度 (0.0 ~ 1.0)
    pub confidence: f32,
    /// 关联的 Caption 文本
    pub caption: Option<String>,
}
