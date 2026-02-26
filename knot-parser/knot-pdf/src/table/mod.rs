//! 表格抽取模块
//!
//! 包含：
//! - 表格候选区域检测（candidate detection）
//! - Stream 表格抽取引擎（无框表格）
//! - Ruled 表格抽取引擎（有线表格）
//! - CellType 推断
//! - fallback_text 生成

pub mod candidate;
pub mod cell_type;
pub mod enhance;
pub mod fallback;
#[cfg(feature = "table_model")]
pub mod onnx_structure;
pub mod ruled;
pub mod stream;
pub mod structure_detect;

use crate::backend::{RawChar, RawLine, RawRect};
use crate::ir::{BBox, TableIR};

/// 表格候选区域
#[derive(Debug, Clone)]
pub struct TableCandidate {
    /// 候选区域边界
    pub bbox: BBox,
    /// 置信度 (0.0 ~ 1.0)
    pub confidence: f32,
    /// 候选类型
    pub candidate_type: CandidateType,
    /// 区域内的原始字符
    pub chars: Vec<RawChar>,
}

/// 候选区域类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateType {
    Stream,
    Ruled,
    Unknown,
}

/// 从原始字符中检测并抽取表格（核心入口）
///
/// 支持 ruled（有线表格）和 stream（无框表格）两种抽取模式，
/// 自动切换并包含降级链路：ruled → stream → fallback_text
pub fn extract_tables(
    chars: &[RawChar],
    page_index: usize,
    page_width: f32,
    page_height: f32,
) -> Vec<TableIR> {
    extract_tables_with_graphics(chars, &[], &[], page_index, page_width, page_height)
}

/// 带线段/矩形信息的表格抽取入口
///
/// 当提供了线段/矩形数据时，会优先尝试 ruled 抽取
pub fn extract_tables_with_graphics(
    chars: &[RawChar],
    lines: &[RawLine],
    rects: &[RawRect],
    page_index: usize,
    page_width: f32,
    page_height: f32,
) -> Vec<TableIR> {
    // 快速跳过：字符数过少不可能构成表格（至少需要 2 行 × 2 列 = 4 个单元格的文本）
    // 典型表格至少 8 个字符，这里用保守阈值 4
    if chars.len() < 4 && lines.is_empty() && rects.is_empty() {
        return Vec::new();
    }

    let mut tables = Vec::new();

    // 策略1：如果有足够的线段，尝试 ruled 抽取（全页范围）
    if ruled::has_enough_lines(lines, rects) {
        let table_id = format!("t{}_{}", page_index, tables.len());
        if let Some(table) = ruled::extract_ruled_table(lines, rects, chars, page_index, &table_id)
        {
            tables.push(table);
            // ruled 成功后，不再尝试 stream（避免重复抽取同一表格）
            return tables;
        }
        // ruled 失败，降级到 stream
    }

    // 策略2：基于文本对齐特征检测候选区域，使用 stream 抽取
    let candidates = candidate::detect_table_candidates(chars, page_width, page_height);

    for (idx, cand) in candidates.iter().enumerate() {
        let table_id = format!("t{}_{}", page_index, tables.len() + idx);
        if let Some(table) =
            stream::extract_stream_table(&cand.chars, &cand.bbox, &table_id, page_index)
        {
            tables.push(table);
        }
    }

    // M11: 增强处理（置信度评估 + 合并单元格 + IoU 消歧 + 低置信度告警）
    enhance::enhance_tables(&mut tables, chars, 0.5);

    tables
}
