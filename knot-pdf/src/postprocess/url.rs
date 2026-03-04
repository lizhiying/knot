//! URL / 超链接碎片修复
//!
//! PDF 中的长 URL 常被 PDF 生成器拆成多个 span 甚至多行。
//! 本模块将碎片化的 URL span 合并回完整的 URL。

use super::PostProcessor;
use crate::config::Config;
use crate::ir::PageIR;

/// URL 修复器
pub struct UrlFixer;

impl UrlFixer {
    pub fn new() -> Self {
        Self
    }

    /// 判断文本是否像 URL 的开头
    fn is_url_start(text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("ftp://")
            || lower.starts_with("mailto:")
            || lower.starts_with("www.")
    }

    /// 判断文本是否像 URL 的后续部分（不含空格，含 URL 常见字符）
    fn is_url_continuation(text: &str) -> bool {
        if text.is_empty() || text.contains(' ') {
            return false;
        }
        // URL 中常见的字符
        text.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '/' | '.'
                        | '-'
                        | '_'
                        | '~'
                        | ':'
                        | '?'
                        | '#'
                        | '['
                        | ']'
                        | '@'
                        | '!'
                        | '$'
                        | '&'
                        | '\''
                        | '('
                        | ')'
                        | '*'
                        | '+'
                        | ','
                        | ';'
                        | '='
                        | '%'
                )
        })
    }

    /// 判断文本是否包含邮箱地址模式
    fn contains_email(text: &str) -> bool {
        // 简单检测：包含 @ 且 @ 前后都有非空文本
        if let Some(at_pos) = text.find('@') {
            let before = &text[..at_pos];
            let after = &text[at_pos + 1..];
            !before.is_empty() && !after.is_empty() && after.contains('.')
        } else {
            false
        }
    }
}

impl PostProcessor for UrlFixer {
    fn name(&self) -> &str {
        "url_fixer"
    }

    fn process_page(&self, page: &mut PageIR, _config: &Config) {
        // 对每个块的每一行，合并相邻的 URL span
        for block in &mut page.blocks {
            for line in &mut block.lines {
                if line.spans.len() <= 1 {
                    continue;
                }

                let mut merged_spans = Vec::with_capacity(line.spans.len());
                let mut i = 0;

                while i < line.spans.len() {
                    let span = &line.spans[i];

                    // 检测 URL 开头
                    if Self::is_url_start(&span.text) {
                        let mut url_text = span.text.clone();

                        // 合并后续的 URL 续接 span
                        let mut j = i + 1;
                        while j < line.spans.len() {
                            let next = &line.spans[j];
                            if Self::is_url_continuation(&next.text) {
                                url_text.push_str(&next.text);
                                j += 1;
                            } else {
                                break;
                            }
                        }

                        merged_spans.push(crate::ir::TextSpan {
                            text: url_text,
                            font_size: span.font_size,
                            is_bold: span.is_bold,
                            font_name: span.font_name.clone(),
                        });
                        i = j;
                    } else {
                        merged_spans.push(span.clone());
                        i += 1;
                    }
                }

                if merged_spans.len() != line.spans.len() {
                    line.spans = merged_spans;
                }
            }

            // 重新计算 normalized_text
            let new_text: String = block
                .lines
                .iter()
                .map(|l| l.text())
                .collect::<Vec<_>>()
                .join("\n");
            block.normalized_text = new_text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_span(text: &str) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            font_size: Some(12.0),
            is_bold: false,
            font_name: None,
        }
    }

    fn make_page_with_spans(spans: Vec<TextSpan>) -> PageIR {
        PageIR {
            page_index: 0,
            size: PageSize {
                width: 612.0,
                height: 792.0,
            },
            rotation: 0.0,
            blocks: vec![BlockIR {
                block_id: "b1".to_string(),
                bbox: BBox::new(72.0, 100.0, 468.0, 16.0),
                role: BlockRole::Body,
                lines: vec![TextLine {
                    spans,
                    bbox: Some(BBox::new(72.0, 100.0, 468.0, 16.0)),
                }],
                normalized_text: String::new(),
            }],
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

    #[test]
    fn test_url_start_detection() {
        assert!(UrlFixer::is_url_start("https://example.com"));
        assert!(UrlFixer::is_url_start("http://test.org"));
        assert!(UrlFixer::is_url_start("mailto:user@test.com"));
        assert!(UrlFixer::is_url_start("www.example.com"));
        assert!(!UrlFixer::is_url_start("normal text"));
    }

    #[test]
    fn test_url_continuation() {
        assert!(UrlFixer::is_url_continuation("/path/to/page"));
        assert!(UrlFixer::is_url_continuation("?key=value"));
        assert!(!UrlFixer::is_url_continuation("has space"));
        assert!(!UrlFixer::is_url_continuation(""));
    }

    #[test]
    fn test_email_detection() {
        assert!(UrlFixer::contains_email("user@example.com"));
        assert!(UrlFixer::contains_email("abc@test.org"));
        assert!(!UrlFixer::contains_email("no at sign"));
        assert!(!UrlFixer::contains_email("@no-before"));
    }

    #[test]
    fn test_url_merge() {
        let spans = vec![
            make_span("Visit "),
            make_span("https://"),
            make_span("example.com"),
            make_span("/path"),
            make_span(" for details"),
        ];
        let mut page = make_page_with_spans(spans);
        let config = Config::default();
        let fixer = UrlFixer::new();
        fixer.process_page(&mut page, &config);

        let line = &page.blocks[0].lines[0];
        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].text, "Visit ");
        assert_eq!(line.spans[1].text, "https://example.com/path");
        assert_eq!(line.spans[2].text, " for details");
    }

    #[test]
    fn test_no_url_no_change() {
        let spans = vec![make_span("Normal "), make_span("text "), make_span("here")];
        let mut page = make_page_with_spans(spans);
        let config = Config::default();
        let fixer = UrlFixer::new();
        fixer.process_page(&mut page, &config);

        let line = &page.blocks[0].lines[0];
        assert_eq!(line.spans.len(), 3); // 未改变
    }
}
