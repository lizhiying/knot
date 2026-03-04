use crate::{
    formats::md::MarkdownParser, DocumentParser, PageIndexConfig, PageIndexError, PageNode,
};
use async_trait::async_trait;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use undoc::docx::DocxParser as UndocDocxParser;
use undoc::model::Block;

pub struct DocxParser;

impl DocxParser {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DocumentParser for DocxParser {
    fn can_handle(&self, extension: &str) -> bool {
        extension.eq_ignore_ascii_case("docx")
    }

    async fn parse(
        &self,
        path: &Path,
        _config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError> {
        // 1. Parse with undoc specific DocxParser
        // We use the specific parser to ensure we only handle DOCX and get the full model
        let mut parser =
            UndocDocxParser::open(path).map_err(|e| PageIndexError::ParseError(e.to_string()))?;
        let doc = parser
            .parse()
            .map_err(|e| PageIndexError::ParseError(e.to_string()))?;

        // 2. Process Resources (images)
        // undoc stores resources in doc.resources map
        let image_data_map: HashMap<String, String> = HashMap::new();

        // 3. Custom Render to Markdown
        let mut markdown = String::new();

        // Buffer for TOC table generation
        let mut toc_buffer: Vec<(String, String)> = Vec::new();

        let flush_toc = |md: &mut String, buffer: &mut Vec<(String, String)>| {
            // Decision Logic:
            // 1. If buffer has >= 2 items, it's definitely a table.
            // 2. If buffer has 1 item:
            //    - If it was explicitly a TOC style, we accept it (e.g. 1-item TOC).
            //    - If it was just a Regex match on "Normal" text, we reject it (likely false positive).

            // Note: capturing `was_style` requires storing it in the buffer.
            // For now, let's stick to the >= 2 rule for safety,
            // OR checks if we can infer confidence.

            if buffer.len() < 2 {
                // For now, Strict Mode: Require at least 2 items to form a table.
                // This might miss 1-item TOCs, but prevents "Sentence 2023" bugs.
                for (title, page) in buffer.drain(..) {
                    md.push_str(&format!("{} {}\n\n", title.trim(), page));
                }
                return;
            }

            md.push_str("| 标题 | 页码 |\n");
            md.push_str("| --- | --- |\n");
            for (title, page) in buffer.drain(..) {
                md.push_str(&format!("| {} | {} |\n", title.trim(), page));
            }
            md.push('\n');
        };

        for section in &doc.sections {
            for block in &section.content {
                match block {
                    Block::Paragraph(p) => {
                        // Check for TOC style (e.g., "TOC 1", "TOC 2", "toc 1")
                        let style_id = p.style_id.as_deref().unwrap_or("").to_lowercase();
                        let is_toc = style_id.contains("toc");

                        if is_toc && !p.runs.is_empty() {
                            // Extract TOC entries from paragraph
                            // Word TOC can be formatted in various ways:
                            // 1. "Title\tPage" (tab separator)
                            // 2. "Title......Page" (dot leader)
                            // 3. Multiple entries concatenated: "Title1\t1Title2\t2Title3\t3"

                            let full_text = p.plain_text();

                            // Use find_iter to extract ALL entries from this paragraph
                            // Pattern: Non-greedy title followed by optional separators and digits
                            // This handles both single and concatenated entries
                            let entry_regex =
                                Regex::new(r"(?P<title>[^\t\d]+?)[\s.．。…—\-\t]*(?P<page>\d+)")
                                    .unwrap();

                            let mut found_any = false;
                            for caps in entry_regex.captures_iter(&full_text) {
                                let title = caps
                                    .name("title")
                                    .map_or("", |m| m.as_str())
                                    .trim()
                                    .to_string();
                                let page = caps.name("page").map_or("", |m| m.as_str()).to_string();
                                if !title.is_empty() && !page.is_empty() {
                                    toc_buffer.push((title, page));
                                    found_any = true;
                                }
                            }

                            if found_any {
                                continue;
                            }
                        }

                        // Not a TOC line, flush buffer first
                        flush_toc(&mut markdown, &mut toc_buffer);

                        // Check list info
                        let prefix = if let Some(_info) = &p.list_info {
                            "- ".to_string()
                        } else {
                            match &p.heading {
                                undoc::HeadingLevel::H1 => "# ".to_string(),
                                undoc::HeadingLevel::H2 => "## ".to_string(),
                                undoc::HeadingLevel::H3 => "### ".to_string(),
                                undoc::HeadingLevel::H4 => "#### ".to_string(),
                                undoc::HeadingLevel::H5 => "##### ".to_string(),
                                undoc::HeadingLevel::H6 => "###### ".to_string(),
                                _ => String::new(),
                            }
                        };

                        let text = p.plain_text();
                        if text.trim().is_empty() {
                            continue;
                        }

                        markdown.push_str(&format!("{}{}\n\n", prefix, text));
                    }
                    Block::Table(t) => {
                        flush_toc(&mut markdown, &mut toc_buffer);
                        // Simple table rendering
                        for row in &t.rows {
                            let mut row_text = Vec::new();
                            for cell in &row.cells {
                                let mut cell_text = String::new();
                                for p in &cell.content {
                                    cell_text.push_str(&p.plain_text());
                                    cell_text.push(' ');
                                }
                                row_text.push(cell_text.trim().to_string());
                            }
                            markdown.push_str(&format!("| {} |\n", row_text.join(" | ")));
                        }
                        markdown.push('\n');
                    }
                    _ => {
                        flush_toc(&mut markdown, &mut toc_buffer);
                    }
                }
            }
        }
        flush_toc(&mut markdown, &mut toc_buffer);

        // 4. Post-process Markdown (same as before)
        let img_regex = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();
        let processed_markdown = img_regex
            .replace_all(&markdown, |caps: &regex::Captures| {
                let _alt = &caps[1];
                let id = &caps[2];
                if let Some(analysis) = image_data_map.get(id) {
                    let mut replacement = format!("\n<Image rId=\"{}\"></Image>\n", id);
                    if !analysis.is_empty() {
                        replacement.push_str(&format!("> **图表分析**: {}\n", analysis));
                    }
                    replacement
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 5. Parse Markdown Structure
        let title = path.file_stem().unwrap_or_default().to_string_lossy();
        let root_node =
            MarkdownParser::parse_text(&processed_markdown, &title, path.to_string_lossy())?;

        Ok(root_node)
    }
}
