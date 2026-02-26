//! 噪声块过滤器
//!
//! 过滤掉只包含标点符号、引号、空白等无实质内容的文本块。
//! 这类块通常来自 PPT 装饰元素（如拉引号样式 `" "` `" " "`）。

use super::PostProcessor;
use crate::config::Config;
use crate::ir::{BlockRole, PageIR};

/// 噪声块过滤器
pub struct NoiseBlockFilter;

impl NoiseBlockFilter {
    pub fn new() -> Self {
        Self
    }

    /// 判断文本是否为纯噪声（只含标点、引号、空白）
    fn is_noise_text(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return true;
        }

        // 全部由标点/引号/空白组成
        trimmed.chars().all(|c| {
            c.is_whitespace()
                || c == '"' || c == '\u{201C}' || c == '\u{201D}'   // 引号 " "
                || c == '\'' || c == '\u{2018}' || c == '\u{2019}' // 单引号 ' '
                || c == '\u{300C}' || c == '\u{300D}'              // 日式引号 「」
                || c == '\u{300E}' || c == '\u{300F}'
                || c == '【' || c == '】'                // 中括号
                || c == '（' || c == '）'
                || c == '(' || c == ')'
                || c == '.' || c == '。'                 // 句号
                || c == ',' || c == '，'                 // 逗号
                || c == '、'                             // 顿号
                || c == ':' || c == '：'                 // 冒号
                || c == ';' || c == '；'                 // 分号
                || c == '-' || c == '—' || c == '–'     // 破折号
                || c == '…'                             // 省略号
                || c == '·'                             // 间隔号
                || c == '*' || c == '#'                  // 符号
                || c == '•' || c == '●' || c == '○'     // 项目符号
                || c == '→' || c == '←' || c == '↑' || c == '↓' // 箭头
        })
    }
}

impl PostProcessor for NoiseBlockFilter {
    fn name(&self) -> &str {
        "noise_block_filter"
    }

    fn process_page(&self, page: &mut PageIR, _config: &Config) {
        let before = page.blocks.len();
        page.blocks.retain(|b| {
            // 保留有角色标注的块（Header/Footer/Title 等不过滤）
            if !matches!(b.role, BlockRole::Body | BlockRole::Unknown) {
                return true;
            }
            let text = b.full_text();
            if Self::is_noise_text(&text) {
                log::debug!(
                    "Noise block removed: '{}' (block_id={})",
                    text.chars().take(20).collect::<String>(),
                    b.block_id,
                );
                return false;
            }
            true
        });
        let removed = before - page.blocks.len();
        if removed > 0 {
            log::debug!(
                "Noise block filter: removed {} blocks from page {}",
                removed,
                page.page_index
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_detection() {
        // 应该是噪声
        assert!(NoiseBlockFilter::is_noise_text("\" \""));
        // 中文引号组合
        let s1 = "\u{201C} \u{201D}"; // " "
        assert!(NoiseBlockFilter::is_noise_text(s1));
        let s2 = "\u{201C} \u{201D} \u{201C}"; // " " "
        assert!(NoiseBlockFilter::is_noise_text(s2));
        assert!(NoiseBlockFilter::is_noise_text("   "));
        assert!(NoiseBlockFilter::is_noise_text(""));
        assert!(NoiseBlockFilter::is_noise_text("..."));
        assert!(NoiseBlockFilter::is_noise_text("\u{2014}\u{2014}")); // ——
        assert!(NoiseBlockFilter::is_noise_text("\u{2022}")); // •
        assert!(NoiseBlockFilter::is_noise_text("\u{FF08}\u{FF09}")); // （）
        assert!(NoiseBlockFilter::is_noise_text(","));
        assert!(NoiseBlockFilter::is_noise_text("\u{3002}")); // 。

        // 不应该是噪声
        assert!(!NoiseBlockFilter::is_noise_text("AI"));
        assert!(!NoiseBlockFilter::is_noise_text("Hello"));
        assert!(!NoiseBlockFilter::is_noise_text("1"));
        assert!(!NoiseBlockFilter::is_noise_text("ROI"));
    }

    #[test]
    fn test_filter_in_page() {
        use crate::ir::*;

        let noise_text = "\u{201C} \u{201D}".to_string(); // " "

        let blocks = vec![
            BlockIR {
                block_id: "b1".into(),
                bbox: BBox::new(0.0, 0.0, 100.0, 20.0),
                role: BlockRole::Body,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: "normal".into(),
                        font_size: Some(12.0),
                        is_bold: false,
                        font_name: None,
                    }],
                    bbox: Some(BBox::new(0.0, 0.0, 100.0, 20.0)),
                }],
                normalized_text: "normal".into(),
            },
            BlockIR {
                block_id: "b2".into(),
                bbox: BBox::new(0.0, 50.0, 100.0, 20.0),
                role: BlockRole::Body,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: noise_text.clone(),
                        font_size: Some(12.0),
                        is_bold: false,
                        font_name: None,
                    }],
                    bbox: Some(BBox::new(0.0, 50.0, 100.0, 20.0)),
                }],
                normalized_text: noise_text,
            },
            BlockIR {
                block_id: "b3".into(),
                bbox: BBox::new(0.0, 100.0, 100.0, 20.0),
                role: BlockRole::Title,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: "".into(),
                        font_size: Some(12.0),
                        is_bold: false,
                        font_name: None,
                    }],
                    bbox: Some(BBox::new(0.0, 100.0, 100.0, 20.0)),
                }],
                normalized_text: "".into(),
            },
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

        let filter = NoiseBlockFilter::new();
        filter.process_page(&mut page, &Config::default());

        // 正常文本保留
        assert_eq!(page.blocks.len(), 2);
        assert_eq!(page.blocks[0].normalized_text, "normal");
        // Title 角色即使为空也保留
        assert_eq!(page.blocks[1].role, BlockRole::Title);
    }
}
