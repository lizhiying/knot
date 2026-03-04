//! 段落跨页合并标记
//!
//! 检测跨页断裂的段落，标记 `continues_from_previous_page`。
//! 实际合并在 DocumentIR 级别提供 merged_paragraphs() 方法。

use super::PostProcessor;
use crate::config::Config;
use crate::ir::PageIR;

/// 段落跨页合并检测（占位实现）
///
/// 注意：跨页合并需要在 DocumentIR 级别处理，
/// 单页 PostProcessor 只能标记首块是否"续接上一页"。
/// 完整实现在 Pipeline.parse() 的多页后处理阶段。
pub struct ParagraphMerger;

impl ParagraphMerger {
    pub fn new() -> Self {
        Self
    }
}

/// 判断文本是否以未完成的句子结尾（不以句末标点结尾）
pub fn is_incomplete_ending(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    let last_char = trimmed.chars().last().unwrap();

    // 常见句末标点
    let sentence_endings = ['.', '。', '!', '！', '?', '？', ':', '：', ';', '；'];
    if sentence_endings.contains(&last_char) {
        return false;
    }

    // 如果以右括号、引号结尾，看看它前面是否有句号
    if matches!(last_char, ')' | '）' | '"' | '\'' | '"') {
        if let Some(prev) = trimmed.chars().rev().nth(1) {
            if sentence_endings.contains(&prev) {
                return false;
            }
        }
    }

    true
}

/// 判断文本是否以段落开头的方式起始（大写字母、数字编号等）
pub fn is_paragraph_start(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    let first_char = trimmed.chars().next().unwrap();

    // 以大写字母开头 → 新段落
    if first_char.is_uppercase() {
        return true;
    }

    // 以数字编号开头（如 "1." "2)" "(a)"）→ 新段落
    if first_char.is_ascii_digit() {
        let rest: String = trimmed.chars().skip_while(|c| c.is_ascii_digit()).collect();
        if rest.starts_with('.') || rest.starts_with(')') {
            return true;
        }
    }

    // 以列表标记开头 → 新段落
    if matches!(first_char, '•' | '–' | '—' | '▪' | '◦' | '-') {
        return true;
    }

    // 以中文数字/序号开头 → 新段落
    let chinese_starts = [
        '一', '二', '三', '四', '五', '六', '七', '八', '九', '十', '第', '（',
    ];
    if chinese_starts.contains(&first_char) {
        return true;
    }

    false
}

impl PostProcessor for ParagraphMerger {
    fn name(&self) -> &str {
        "paragraph_merger"
    }

    fn process_page(&self, _page: &mut PageIR, _config: &Config) {
        // 单页级别暂不处理，跨页合并需要在 DocumentIR 级别实现
        // 这里只提供辅助函数 is_incomplete_ending / is_paragraph_start
        // 供 Pipeline.parse() 的多页后处理使用
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incomplete_ending() {
        assert!(is_incomplete_ending("and the result was"));
        assert!(is_incomplete_ending("with the following"));
        assert!(is_incomplete_ending("文本继续"));
    }

    #[test]
    fn test_complete_ending() {
        assert!(!is_incomplete_ending("This is the end."));
        assert!(!is_incomplete_ending("Done!"));
        assert!(!is_incomplete_ending("Really?"));
        assert!(!is_incomplete_ending("结束了。"));
    }

    #[test]
    fn test_paragraph_start_uppercase() {
        assert!(is_paragraph_start("The quick brown fox"));
        assert!(is_paragraph_start("Abstract"));
    }

    #[test]
    fn test_paragraph_start_numbered() {
        assert!(is_paragraph_start("1. Introduction"));
        assert!(is_paragraph_start("2) Method"));
    }

    #[test]
    fn test_paragraph_start_bullet() {
        assert!(is_paragraph_start("• First item"));
        assert!(is_paragraph_start("- Second item"));
    }

    #[test]
    fn test_not_paragraph_start() {
        assert!(!is_paragraph_start("the continuation of"));
        assert!(!is_paragraph_start("and more text"));
    }

    #[test]
    fn test_continuation_detection() {
        // 前一页末尾不完整 + 后一页非段首 → 应合并
        assert!(is_incomplete_ending("and the result was"));
        assert!(!is_paragraph_start("smaller than expected"));

        // 前一页末尾完整 + 后一页段首 → 不应合并
        assert!(!is_incomplete_ending("was found to be significant."));
        assert!(is_paragraph_start("The next experiment"));
    }
}
