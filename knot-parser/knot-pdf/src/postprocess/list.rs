//! 列表识别增强
//!
//! 检测有序列表（1. 2. (a) (i) ① 等）和无序列表（• - – ▪ ◦ 等），
//! 标记 role = ListItem 并识别嵌套级别。

use super::PostProcessor;
use crate::config::Config;
use crate::ir::{BlockRole, PageIR};

/// 列表检测器
pub struct ListDetector;

impl ListDetector {
    pub fn new() -> Self {
        Self
    }

    /// 检测文本是否以有序列表标记开头，返回 (是否匹配, 缩进级别推断)
    fn detect_ordered_list(text: &str) -> Option<u32> {
        let trimmed = text.trim_start();
        let indent_chars = text.len() - trimmed.len();
        let level = (indent_chars / 2).min(4) as u32; // 每 2 个空格一级

        // "1." "2." "10." 模式
        if let Some(first) = trimmed.chars().next() {
            if first.is_ascii_digit() {
                let rest: String = trimmed.chars().skip_while(|c| c.is_ascii_digit()).collect();
                if rest.starts_with(". ") || rest.starts_with(".\t") {
                    return Some(level);
                }
                // "1)" "2)" 模式
                if rest.starts_with(") ") || rest.starts_with(")\t") {
                    return Some(level);
                }
            }
        }

        // "(a)" "(b)" "(i)" "(ii)" 模式
        if let Some(rest) = trimmed.strip_prefix('(') {
            if let Some(end_idx) = rest.find(')') {
                let inside = &rest[..end_idx];
                // 字母编号
                if inside.len() <= 3 && inside.chars().all(|c| c.is_ascii_lowercase()) {
                    return Some(level + 1);
                }
                // 罗马数字（简单检测）
                if inside.chars().all(|c| matches!(c, 'i' | 'v' | 'x')) && !inside.is_empty() {
                    return Some(level + 1);
                }
            }
        }

        // 圆圈编号 ① ② ③ ④ ⑤ ⑥ ⑦ ⑧ ⑨ ⑩
        let circled = ['①', '②', '③', '④', '⑤', '⑥', '⑦', '⑧', '⑨', '⑩'];
        if trimmed.starts_with(|c: char| circled.contains(&c)) {
            return Some(level);
        }

        None
    }

    /// 检测文本是否以无序列表标记开头
    fn detect_unordered_list(text: &str) -> Option<u32> {
        let trimmed = text.trim_start();
        let indent_chars = text.len() - trimmed.len();
        let level = (indent_chars / 2).min(4) as u32;

        // 常见无序列表标记
        let bullets = ['•', '◦', '▪', '▸', '▹', '►', '○', '●', '■', '□'];
        if trimmed.starts_with(|c: char| bullets.contains(&c)) {
            return Some(level);
        }

        // 短横线标记（确保后面有空格，避免误匹配连字符）
        if (trimmed.starts_with("- ") || trimmed.starts_with("– ") || trimmed.starts_with("— "))
            && trimmed.len() > 2
        {
            return Some(level);
        }

        None
    }
}

impl PostProcessor for ListDetector {
    fn name(&self) -> &str {
        "list_detector"
    }

    fn process_page(&self, page: &mut PageIR, _config: &Config) {
        // 收集所有 Body 块的 x 坐标，用于判断缩进
        let body_x_positions: Vec<f32> = page
            .blocks
            .iter()
            .filter(|b| matches!(b.role, BlockRole::Body))
            .map(|b| b.bbox.x)
            .collect();

        let min_x = body_x_positions
            .iter()
            .copied()
            .reduce(f32::min)
            .unwrap_or(72.0);

        for block in &mut page.blocks {
            if block.role != BlockRole::Body {
                continue;
            }

            let text = block.full_text();

            // 检测有序列表
            if let Some(level) = Self::detect_ordered_list(&text) {
                // 额外验证：根据 x 偏移调整级别
                let x_offset = ((block.bbox.x - min_x) / 20.0).max(0.0) as u32;
                block.role = BlockRole::List;
                let _ = level + x_offset; // 级别信息暂存（后续可在 BlockIR 中增加 list_level 字段）
                continue;
            }

            // 检测无序列表
            if let Some(level) = Self::detect_unordered_list(&text) {
                let x_offset = ((block.bbox.x - min_x) / 20.0).max(0.0) as u32;
                block.role = BlockRole::List;
                let _ = level + x_offset;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_page(blocks: Vec<BlockIR>) -> PageIR {
        PageIR {
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
            text_score: 1.0,
            is_scanned_guess: false,
            source: PageSource::BornDigital,
            timings: Timings::default(),
        }
    }

    fn make_block(id: &str, text: &str, x: f32) -> BlockIR {
        BlockIR {
            block_id: id.to_string(),
            bbox: BBox::new(x, 100.0, 400.0, 16.0),
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(BBox::new(x, 100.0, 400.0, 16.0)),
            }],
            normalized_text: text.to_string(),
        }
    }

    #[test]
    fn test_detect_ordered_dot() {
        assert!(ListDetector::detect_ordered_list("1. Introduction").is_some());
        assert!(ListDetector::detect_ordered_list("10. Chapter ten").is_some());
    }

    #[test]
    fn test_detect_ordered_paren() {
        assert!(ListDetector::detect_ordered_list("1) First").is_some());
        assert!(ListDetector::detect_ordered_list("(a) Sub item").is_some());
        assert!(ListDetector::detect_ordered_list("(ii) Roman").is_some());
    }

    #[test]
    fn test_detect_ordered_circled() {
        assert!(ListDetector::detect_ordered_list("① 第一项").is_some());
        assert!(ListDetector::detect_ordered_list("③ 第三项").is_some());
    }

    #[test]
    fn test_detect_unordered() {
        assert!(ListDetector::detect_unordered_list("• Bullet item").is_some());
        assert!(ListDetector::detect_unordered_list("- Dash item").is_some());
        assert!(ListDetector::detect_unordered_list("– En dash item").is_some());
    }

    #[test]
    fn test_not_list() {
        assert!(ListDetector::detect_ordered_list("Normal text").is_none());
        assert!(ListDetector::detect_unordered_list("Normal text").is_none());
    }

    #[test]
    fn test_list_detection_in_page() {
        let blocks = vec![
            make_block("b1", "Introduction", 72.0),
            make_block("b2", "1. First item", 72.0),
            make_block("b3", "2. Second item", 72.0),
            make_block("b4", "• Sub bullet", 92.0),
            make_block("b5", "Conclusion paragraph", 72.0),
        ];
        let mut page = make_page(blocks);
        let config = Config::default();
        let detector = ListDetector::new();
        detector.process_page(&mut page, &config);

        assert_eq!(page.blocks[0].role, BlockRole::Body); // "Introduction"
        assert_eq!(page.blocks[1].role, BlockRole::List); // "1. First item"
        assert_eq!(page.blocks[2].role, BlockRole::List); // "2. Second item"
        assert_eq!(page.blocks[3].role, BlockRole::List); // "• Sub bullet"
        assert_eq!(page.blocks[4].role, BlockRole::Body); // "Conclusion"
    }
}
