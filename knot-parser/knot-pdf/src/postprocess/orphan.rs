//! 孤儿块合并器
//!
//! 检测并合并被错误拆分的文本块。
//!
//! PPT 导出 PDF 中，长句换行后，行首的 1-3 个字符可能被提取为独立块，
//! 甚至被错误分类为 Title/Heading（因为字号与正文不同或行间距较大）。
//!
//! 典型案例：
//! - "...从 技术探索 走向 价值驱" (blk A, role=Body)
//! - "动 的商业化深水区"           (blk B, role=Title) ← 应该合并到 A
//!
//! 检测策略：
//! 1. 当前块很短（≤15 字符）
//! 2. 前一块以非句末字符结尾（不是。！？.!?等）
//! 3. 当前块在前一块的下方且 x 方向有重叠（视觉上连续）

use super::PostProcessor;
use crate::config::Config;
use crate::ir::{BlockRole, PageIR};

/// 孤儿块合并器
pub struct OrphanBlockMerger;

impl OrphanBlockMerger {
    pub fn new() -> Self {
        Self
    }

    /// 判断前一块是否以"未完成"的方式结尾（不是句末标点）
    fn ends_incomplete(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }
        let last_char = trimmed.chars().last().unwrap();
        // 如果以句末标点结尾，认为是完整的
        !matches!(
            last_char,
            '。' | '！'
                | '？'
                | '.'
                | '!'
                | '?'
                | '；'
                | ';'
                | ')'
                | '）'
                | ']'
                | '】'
                | '」'
                | '』'
                | ':'
                | '：' // 冒号通常也是结束
        )
    }

    /// 判断当前块文本是否像一个孤儿续写
    fn is_orphan_continuation(text: &str) -> bool {
        let trimmed = text.trim();
        // 非常短（≤15 字符）
        trimmed.chars().count() <= 15
    }

    /// 判断两个块在空间上是否连续（B 在 A 的正下方）
    fn is_spatially_continuous(prev_bbox: &crate::ir::BBox, curr_bbox: &crate::ir::BBox) -> bool {
        // B 的顶部在 A 的底部附近（允许一定间距）
        let y_gap = curr_bbox.y - prev_bbox.bottom();
        if y_gap < -5.0 || y_gap > 30.0 {
            return false;
        }

        // x 方向有一定重叠或起点接近
        let x_overlap = prev_bbox.x < curr_bbox.right() && curr_bbox.x < prev_bbox.right();
        let x_start_close = (curr_bbox.x - prev_bbox.x).abs() < 100.0;

        x_overlap || x_start_close
    }
}

impl PostProcessor for OrphanBlockMerger {
    fn name(&self) -> &str {
        "orphan_block_merger"
    }

    fn process_page(&self, page: &mut PageIR, _config: &Config) {
        if page.blocks.len() < 2 {
            return;
        }

        // 检测需要合并的块对 (prev_idx, curr_idx)
        let mut merge_pairs: Vec<(usize, usize)> = Vec::new();

        for i in 1..page.blocks.len() {
            let prev = &page.blocks[i - 1];
            let curr = &page.blocks[i];

            let prev_text = prev.full_text();
            let curr_text = curr.full_text();

            // 条件 1：前一块以未完成的方式结尾
            if !Self::ends_incomplete(&prev_text) {
                continue;
            }

            // 条件 2：当前块是短块（孤儿）
            if !Self::is_orphan_continuation(&curr_text) {
                continue;
            }

            // 条件 3：空间上连续
            if !Self::is_spatially_continuous(&prev.bbox, &curr.bbox) {
                continue;
            }

            // 跳过已标记为 List 的块（列表项本身就是短的）
            if matches!(curr.role, BlockRole::List | BlockRole::PageNumber) {
                continue;
            }

            merge_pairs.push((i - 1, i));
        }

        // 从后往前合并，避免索引位移
        for &(prev_idx, curr_idx) in merge_pairs.iter().rev() {
            let curr_text = page.blocks[curr_idx].full_text();
            let curr_role = page.blocks[curr_idx].role;
            let curr_bbox = page.blocks[curr_idx].bbox;
            let curr_lines = page.blocks[curr_idx].lines.clone();

            log::debug!(
                "Orphan merge: '{}' (role={:?}) -> appended to '{}'",
                curr_text.chars().take(20).collect::<String>(),
                curr_role,
                page.blocks[prev_idx]
                    .full_text()
                    .chars()
                    .rev()
                    .take(20)
                    .collect::<String>(),
            );

            // 合并文本
            let merged = format!("{}{}", page.blocks[prev_idx].normalized_text, curr_text);
            page.blocks[prev_idx].normalized_text = merged;

            // 合并 lines
            page.blocks[prev_idx].lines.extend(curr_lines);

            // 扩展 bbox
            let prev_bbox = &mut page.blocks[prev_idx].bbox;
            let new_right = prev_bbox.right().max(curr_bbox.right());
            let new_bottom = prev_bbox.bottom().max(curr_bbox.bottom());
            prev_bbox.width = new_right - prev_bbox.x;
            prev_bbox.height = new_bottom - prev_bbox.y;

            // 移除当前块
            page.blocks.remove(curr_idx);
        }

        if !merge_pairs.is_empty() {
            log::debug!(
                "Orphan block merger: merged {} pairs on page {}",
                merge_pairs.len(),
                page.page_index,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_block(id: &str, text: &str, role: BlockRole, bbox: BBox) -> BlockIR {
        BlockIR {
            block_id: id.to_string(),
            bbox,
            role,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(bbox),
            }],
            normalized_text: text.to_string(),
        }
    }

    #[test]
    fn test_ends_incomplete() {
        assert!(OrphanBlockMerger::ends_incomplete("value driven"));
        assert!(!OrphanBlockMerger::ends_incomplete("end of sentence."));
        assert!(!OrphanBlockMerger::ends_incomplete("end of sentence;"));
    }

    #[test]
    fn test_merge_orphan_block() {
        let blocks = vec![
            make_block(
                "b1",
                "title",
                BlockRole::Title,
                BBox::new(50.0, 20.0, 400.0, 20.0),
            ),
            make_block(
                "b2",
                "first para ending with partial word",
                BlockRole::Body,
                BBox::new(50.0, 50.0, 400.0, 20.0),
            ),
            make_block(
                "b3",
                "continuation",
                BlockRole::Title,
                BBox::new(50.0, 72.0, 100.0, 20.0),
            ),
            make_block(
                "b4",
                "next paragraph starts here.",
                BlockRole::Body,
                BBox::new(50.0, 110.0, 400.0, 20.0),
            ),
        ];

        let mut page = PageIR {
            page_index: 0,
            size: PageSize {
                width: 612.0,
                height: 792.0,
            },
            rotation: 0.0,
            blocks,
            tables: vec![],
            images: vec![],
            formulas: vec![],
            diagnostics: PageDiagnostics::default(),
            text_score: 0.9,
            is_scanned_guess: false,
            source: PageSource::BornDigital,
            timings: Timings::default(),
        };

        let merger = OrphanBlockMerger::new();
        merger.process_page(&mut page, &Config::default());

        // b2 and b3 should be merged
        assert_eq!(page.blocks.len(), 3);
        assert!(page.blocks[1].normalized_text.contains("continuation"));
        assert!(page.blocks[1].normalized_text.contains("partial word"));
    }

    #[test]
    fn test_no_merge_after_complete_sentence() {
        let blocks = vec![
            make_block(
                "b1",
                "Complete sentence.",
                BlockRole::Body,
                BBox::new(50.0, 50.0, 400.0, 20.0),
            ),
            make_block(
                "b2",
                "New block",
                BlockRole::Body,
                BBox::new(50.0, 72.0, 100.0, 20.0),
            ),
        ];

        let mut page = PageIR {
            page_index: 0,
            size: PageSize {
                width: 612.0,
                height: 792.0,
            },
            rotation: 0.0,
            blocks,
            tables: vec![],
            images: vec![],
            formulas: vec![],
            diagnostics: PageDiagnostics::default(),
            text_score: 0.9,
            is_scanned_guess: false,
            source: PageSource::BornDigital,
            timings: Timings::default(),
        };

        let merger = OrphanBlockMerger::new();
        merger.process_page(&mut page, &Config::default());

        // Should NOT merge (prev ends with '.')
        assert_eq!(page.blocks.len(), 2);
    }
}
