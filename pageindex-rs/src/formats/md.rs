use crate::{DocumentParser, NodeMeta, PageIndexConfig, PageIndexError, PageNode};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct MarkdownParser;

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }

    /// 计算大约的 Token 数量 (简单按空格分割估算)
    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    fn heading_level_to_u32(level: HeadingLevel) -> u32 {
        match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }
    }

    /// Public helper to parse markdown text into PageNode tree.
    /// Used by OfficeParser to process converted markdown.
    pub fn parse_text(
        content: &str,
        title: &str,
        file_path: impl Into<String>,
    ) -> Result<PageNode, PageIndexError> {
        let parser = Parser::new(content);
        let file_path_str = file_path.into();

        // Stack to maintain hierarchy.
        let root_node = PageNode {
            node_id: "root".to_string(),
            title: title.to_string(),
            level: 0,
            content: String::new(),
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: file_path_str.clone(),
                page_number: None,
                line_number: Some(0),
                token_count: 0,
                extra: HashMap::new(),
            },
            children: Vec::new(),
        };

        let mut stack: Vec<PageNode> = vec![root_node];
        let mut node_counter: usize = 0;
        let mut capturing_title = false;

        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    let new_level = Self::heading_level_to_u32(level);

                    while let Some(top) = stack.last() {
                        if top.level >= new_level {
                            if let Some(mut node) = stack.pop() {
                                node.metadata.token_count = Self::count_tokens(&node.content);
                                if let Some(parent) = stack.last_mut() {
                                    parent.children.push(node);
                                } else {
                                    stack.push(node);
                                    break;
                                }
                            }
                        } else {
                            break;
                        }
                    }

                    let new_node = PageNode {
                        node_id: {
                            node_counter += 1;
                            format!("{}-{}", file_path_str, node_counter)
                        },
                        title: String::new(),
                        level: new_level,
                        content: String::new(),
                        summary: None,
                        embedding: None,
                        metadata: NodeMeta {
                            file_path: file_path_str.clone(),
                            page_number: None,
                            line_number: None,
                            token_count: 0,
                            extra: HashMap::new(),
                        },
                        children: Vec::new(),
                    };
                    stack.push(new_node);
                    capturing_title = true;
                }

                Event::End(TagEnd::Heading(_)) => {
                    capturing_title = false;
                    if let Some(node) = stack.last_mut() {
                        node.content.push('\n');
                    }
                }

                Event::Text(text) => {
                    if let Some(node) = stack.last_mut() {
                        if capturing_title {
                            node.title.push_str(&text);
                        }
                        node.content.push_str(&text);
                    }
                }

                Event::Start(Tag::CodeBlock(kind)) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("\n```");
                        if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind {
                            node.content.push_str(&lang);
                        }
                        node.content.push('\n');
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("```\n");
                    }
                }

                Event::SoftBreak | Event::HardBreak => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('\n');
                    }
                }

                _ => {}
            }
        }

        while stack.len() > 1 {
            if let Some(mut node) = stack.pop() {
                node.metadata.token_count = Self::count_tokens(&node.content);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
        }

        let mut final_root = stack
            .pop()
            .ok_or_else(|| PageIndexError::ParseError("Empty stack".into()))?;
        final_root.metadata.token_count = Self::count_tokens(&final_root.content);

        Ok(final_root)
    }
}

use async_trait::async_trait;

#[async_trait]
impl DocumentParser for MarkdownParser {
    fn can_handle(&self, extension: &str) -> bool {
        matches!(extension, "md" | "markdown")
    }

    async fn parse(
        &self,
        path: &Path,
        _config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError> {
        let content = fs::read_to_string(path)?;
        let title = path.file_stem().unwrap_or_default().to_string_lossy();

        Self::parse_text(&content, &title, path.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn test_markdown_hierarchy() {
        let md = r#"
# Heading 1
Content 1
## Heading 1.1
Content 1.1
### Heading 1.1.1
Content 1.1.1
## Heading 1.2
Content 1.2
# Heading 2
Content 2
"#;

        // Write to temp file
        let path = Path::new("test_hierarchy.md");
        let mut file = fs::File::create(path).unwrap();
        file.write_all(md.as_bytes()).unwrap();

        let parser = MarkdownParser::new();
        let config = PageIndexConfig {
            vision_provider: None,
            llm_provider: None,
            embedding_provider: None,
            min_token_threshold: 0,
            summary_token_threshold: 100,
            enable_auto_summary: false,
            default_language: "en".into(),
            progress_callback: None,
            page_content_callback: None,
            pdf_ocr_enabled: false,
            pdf_ocr_model_dir: None,
            pdf_vision_api_url: None,
            pdf_vision_model: None,
            pdf_page_indices: None,
        };

        let root = parser.parse(path, &config).await.unwrap();

        // Clean up
        fs::remove_file(path).unwrap();

        // Print tree for debugging
        fn print_tree(node: &PageNode, indent: usize) {
            println!(
                "{:indent$}- {} (L{})",
                "",
                node.title,
                node.level,
                indent = indent * 2
            );
            for child in &node.children {
                print_tree(child, indent + 1);
            }
        }
        print_tree(&root, 0);

        // Verification
        assert_eq!(root.children.len(), 2); // H1, H2
        assert_eq!(root.children[0].title, "Heading 1");
        assert_eq!(root.children[1].title, "Heading 2");

        // H1 children -> H1.1, H1.2
        assert_eq!(root.children[0].children.len(), 2);
        assert_eq!(root.children[0].children[0].title, "Heading 1.1");

        // H1.1 children -> H1.1.1
        assert_eq!(root.children[0].children[0].children.len(), 1);
        assert_eq!(
            root.children[0].children[0].children[0].title,
            "Heading 1.1.1"
        );
    }

    #[tokio::test]
    async fn test_h3_only_headings() {
        // 模拟用户的云南旅游文件格式：开头是段落，然后是 H3 标题
        let md = r#"这份行程围绕你已订的三段住宿。

---

### 2.15（周日）：抵达大理
- 06:50 内容1

### 2.16（周一）：大理慢游
- 09:00 内容2

### 2.17（周二）：大理到丽江
- 08:00 内容3
"#;

        let path = Path::new("test_h3_only.md");
        let mut file = fs::File::create(path).unwrap();
        file.write_all(md.as_bytes()).unwrap();

        let parser = MarkdownParser::new();
        let config = PageIndexConfig {
            vision_provider: None,
            llm_provider: None,
            embedding_provider: None,
            min_token_threshold: 0,
            summary_token_threshold: 100,
            enable_auto_summary: false,
            default_language: "zh".into(),
            progress_callback: None,
            page_content_callback: None,
            pdf_ocr_enabled: false,
            pdf_ocr_model_dir: None,
            pdf_vision_api_url: None,
            pdf_vision_model: None,
            pdf_page_indices: None,
        };

        let root = parser.parse(path, &config).await.unwrap();
        fs::remove_file(path).unwrap();

        // 打印调试信息
        fn print_tree(node: &PageNode, indent: usize) {
            println!(
                "{:indent$}- {} (L{}) [children: {}]",
                "",
                node.title,
                node.level,
                node.children.len(),
                indent = indent * 2
            );
            for child in &node.children {
                print_tree(child, indent + 1);
            }
        }
        print_tree(&root, 0);

        // 验证：应该有 3 个 H3 子节点
        assert_eq!(root.children.len(), 3, "Root should have 3 H3 children");
        assert_eq!(root.children[0].title, "2.15（周日）：抵达大理");
        assert_eq!(root.children[1].title, "2.16（周一）：大理慢游");
        assert_eq!(root.children[2].title, "2.17（周二）：大理到丽江");
    }
}
