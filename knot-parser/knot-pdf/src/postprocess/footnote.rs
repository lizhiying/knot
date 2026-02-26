//! 脚注/尾注检测与分离
//!
//! 脚注特征：
//! - 页面底部 15% 区域内的小字号文本
//! - 以数字上标或方括号编号开头（如 "¹"、"[1]"）
//! - 脚注区域上方可能有短水平分隔线
//! - 字号通常比正文小

use super::PostProcessor;
use crate::config::Config;
use crate::ir::{BlockRole, PageIR};

/// 脚注检测器
pub struct FootnoteDetector;

impl FootnoteDetector {
    pub fn new() -> Self {
        Self
    }

    /// 检测文本是否以脚注编号开头
    fn starts_with_footnote_marker(text: &str) -> bool {
        let trimmed = text.trim_start();
        if trimmed.is_empty() {
            return false;
        }

        // 上标数字：¹ ² ³ ⁴ ⁵ ⁶ ⁷ ⁸ ⁹ ⁰
        let superscript_digits = ['¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹', '⁰'];
        if trimmed.starts_with(|c: char| superscript_digits.contains(&c)) {
            return true;
        }

        // 方括号编号：[1], [2], [10]
        if let Some(rest) = trimmed.strip_prefix('[') {
            if let Some(idx) = rest.find(']') {
                let inside = &rest[..idx];
                if inside.chars().all(|c| c.is_ascii_digit()) && !inside.is_empty() {
                    return true;
                }
            }
        }

        // 数字点号前缀（在底部区域的小文本）：1. 2. 等
        // 注意：这个规则比较宽松，需要结合位置信息使用
        if let Some(first_char) = trimmed.chars().next() {
            if first_char.is_ascii_digit() {
                // 检查是否形如 "1 ", "1.", "1)"
                let rest: String = trimmed.chars().skip_while(|c| c.is_ascii_digit()).collect();
                if rest.starts_with(". ") || rest.starts_with(") ") || rest.starts_with(" ") {
                    return true;
                }
            }
        }

        // 星号脚注标记：*, **, ***, †, ‡
        if trimmed.starts_with('*') || trimmed.starts_with('†') || trimmed.starts_with('‡') {
            return true;
        }

        false
    }

    /// 估算块的平均字号
    fn avg_font_size(block: &crate::ir::BlockIR) -> f32 {
        let mut total = 0.0f32;
        let mut count = 0;
        for line in &block.lines {
            for span in &line.spans {
                if let Some(fs) = span.font_size {
                    total += fs;
                    count += 1;
                }
            }
        }
        if count > 0 {
            total / count as f32
        } else {
            12.0
        }
    }
}

impl PostProcessor for FootnoteDetector {
    fn name(&self) -> &str {
        "footnote_detector"
    }

    fn process_page(&self, page: &mut PageIR, config: &Config) {
        if !config.separate_footnotes {
            return;
        }

        let page_height = page.size.height;
        // 脚注区域：页面底部 15%
        let footnote_y_threshold = page_height * 0.85;

        // 计算正文平均字号作为参考
        let body_blocks: Vec<_> = page
            .blocks
            .iter()
            .filter(|b| matches!(b.role, BlockRole::Body) && b.bbox.y < footnote_y_threshold)
            .collect();
        let avg_body_font_size = if body_blocks.is_empty() {
            12.0
        } else {
            body_blocks
                .iter()
                .map(|b| Self::avg_font_size(b))
                .sum::<f32>()
                / body_blocks.len() as f32
        };

        // 标记脚注块
        for block in &mut page.blocks {
            // 跳过已有角色的块
            if block.role != BlockRole::Body {
                continue;
            }

            // 必须在底部区域
            if block.bbox.y < footnote_y_threshold {
                continue;
            }

            let text = block.full_text();
            let font_size = Self::avg_font_size(block);

            // 条件：在底部区域 + (脚注标记开头 或 字号明显小于正文)
            let has_marker = Self::starts_with_footnote_marker(&text);
            let is_small_font = font_size < avg_body_font_size * 0.85;

            if has_marker || is_small_font {
                block.role = BlockRole::Footnote;
                log::debug!(
                    "Footnote detected: block_id={}, marker={}, small_font={}",
                    block.block_id,
                    has_marker,
                    is_small_font
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_page_with_height(blocks: Vec<BlockIR>, height: f32) -> PageIR {
        PageIR {
            page_index: 0,
            size: PageSize {
                width: 612.0,
                height,
            },
            rotation: 0.0,
            blocks,
            tables: vec![],
            images: vec![],
            formulas: vec![],
            diagnostics: PageDiagnostics::default(),
            text_score: 1.0,
            is_scanned_guess: false,
            source: PageSource::BornDigital,
            timings: Timings::default(),
        }
    }

    fn make_block(id: &str, text: &str, bbox: BBox, font_size: f32) -> BlockIR {
        BlockIR {
            block_id: id.to_string(),
            bbox,
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.to_string(),
                    font_size: Some(font_size),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(bbox),
            }],
            normalized_text: text.to_string(),
        }
    }

    #[test]
    fn test_footnote_marker_superscript() {
        assert!(FootnoteDetector::starts_with_footnote_marker(
            "¹ This is a footnote"
        ));
        assert!(FootnoteDetector::starts_with_footnote_marker("² Reference"));
    }

    #[test]
    fn test_footnote_marker_bracket() {
        assert!(FootnoteDetector::starts_with_footnote_marker(
            "[1] Smith et al."
        ));
        assert!(FootnoteDetector::starts_with_footnote_marker(
            "[12] Reference"
        ));
    }

    #[test]
    fn test_footnote_marker_star() {
        assert!(FootnoteDetector::starts_with_footnote_marker(
            "* Corresponding author"
        ));
        assert!(FootnoteDetector::starts_with_footnote_marker("† Deceased"));
    }

    #[test]
    fn test_not_footnote_marker() {
        assert!(!FootnoteDetector::starts_with_footnote_marker(
            "Normal text"
        ));
        assert!(!FootnoteDetector::starts_with_footnote_marker(
            "The quick brown fox"
        ));
    }

    #[test]
    fn test_footnote_detection_in_bottom() {
        let page_h = 792.0;
        let blocks = vec![
            make_block(
                "b1",
                "Body text paragraph",
                BBox::new(72.0, 100.0, 468.0, 20.0),
                12.0,
            ),
            make_block(
                "b2",
                "More body text",
                BBox::new(72.0, 130.0, 468.0, 20.0),
                12.0,
            ),
            make_block(
                "b3",
                "¹ This is a footnote reference.",
                BBox::new(72.0, 700.0, 468.0, 14.0),
                9.0,
            ),
        ];
        let mut page = make_page_with_height(blocks, page_h);
        let mut config = Config::default();
        config.separate_footnotes = true;

        let detector = FootnoteDetector::new();
        detector.process_page(&mut page, &config);

        assert_eq!(page.blocks[0].role, BlockRole::Body);
        assert_eq!(page.blocks[1].role, BlockRole::Body);
        assert_eq!(page.blocks[2].role, BlockRole::Footnote);
    }

    #[test]
    fn test_no_footnote_in_body_area() {
        let page_h = 792.0;
        let blocks = vec![
            // [1] 在正文区域不应被标为脚注
            make_block(
                "b1",
                "[1] Introduction",
                BBox::new(72.0, 100.0, 468.0, 20.0),
                12.0,
            ),
        ];
        let mut page = make_page_with_height(blocks, page_h);
        let mut config = Config::default();
        config.separate_footnotes = true;

        let detector = FootnoteDetector::new();
        detector.process_page(&mut page, &config);

        assert_eq!(page.blocks[0].role, BlockRole::Body);
    }
}
