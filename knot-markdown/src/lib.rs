//! # knot-markdown
//!
//! Markdown 解析器，将 Markdown 文本解析为结构化的 heading 树。
//!
//! ## 功能
//!
//! - `parse_text()`: 将 Markdown 文本解析为 `MarkdownNode` 树
//! - `build_from_pages()`: 将多个 page 内容合并构建为语义树（适用于 PDF 等多页文档）
//!
//! ## 示例
//!
//! ```rust
//! use knot_markdown::{parse_text, MarkdownNode};
//!
//! let md = "# Hello\nContent\n## Sub\nMore content";
//! let tree = parse_text(md, "doc", "doc.md").unwrap();
//! assert_eq!(tree.children.len(), 1);
//! assert_eq!(tree.children[0].title, "Hello");
//! ```

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

/// Markdown 解析树的节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownNode {
    /// 唯一标识符
    pub node_id: String,
    /// 标题文本
    pub title: String,
    /// 层级（0=root, 1=H1, 2=H2, ...）
    pub level: u32,
    /// 节点的原始文本内容
    pub content: String,
    /// 大约的 Token 数量
    pub token_count: usize,
    /// 子节点
    pub children: Vec<MarkdownNode>,
}

/// Markdown 解析错误
#[derive(Debug, thiserror::Error)]
pub enum MarkdownError {
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// 计算大约的 Token 数量（按空格分割估算）
pub fn count_tokens(text: &str) -> usize {
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

/// 将 Markdown 文本解析为 `MarkdownNode` 树。
///
/// # 参数
///
/// - `content`: Markdown 文本
/// - `title`: 文档标题（用于根节点）
/// - `file_path`: 文件路径（用于生成 node_id）
///
/// # 返回
///
/// 根 `MarkdownNode`，其 `children` 包含解析出的 heading 层级结构。
pub fn parse_text(
    content: &str,
    title: &str,
    file_path: impl Into<String>,
) -> Result<MarkdownNode, MarkdownError> {
    let parser = Parser::new(content);
    let file_path_str = file_path.into();

    let root_node = MarkdownNode {
        node_id: "root".to_string(),
        title: title.to_string(),
        level: 0,
        content: String::new(),
        token_count: 0,
        children: Vec::new(),
    };

    let mut stack: Vec<MarkdownNode> = vec![root_node];
    let mut node_counter: usize = 0;
    let mut capturing_title = false;
    let mut link_url_stack: Vec<String> = Vec::new();
    let mut list_stack: Vec<Option<u64>> = Vec::new(); // None=无序, Some(n)=有序

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let new_level = heading_level_to_u32(level);

                while let Some(top) = stack.last() {
                    if top.level >= new_level {
                        if let Some(mut node) = stack.pop() {
                            node.token_count = count_tokens(&node.content);
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

                let new_node = MarkdownNode {
                    node_id: {
                        node_counter += 1;
                        format!("{}-{}", file_path_str, node_counter)
                    },
                    title: String::new(),
                    level: new_level,
                    content: String::new(),
                    token_count: 0,
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

            // 段落边界
            Event::Start(Tag::Paragraph) => {
                if let Some(node) = stack.last_mut() {
                    if !node.content.is_empty() && !node.content.ends_with('\n') {
                        node.content.push_str("\n\n");
                    } else if node.content.ends_with('\n') && !node.content.ends_with("\n\n") {
                        node.content.push('\n');
                    }
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push('\n');
                }
            }

            // 列表
            Event::Start(Tag::List(ordered)) => {
                list_stack.push(ordered);
                if let Some(node) = stack.last_mut() {
                    if !node.content.is_empty() && !node.content.ends_with('\n') {
                        node.content.push('\n');
                    }
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                if let Some(node) = stack.last_mut() {
                    node.content.push('\n');
                }
            }

            // 列表项
            Event::Start(Tag::Item) => {
                if let Some(node) = stack.last_mut() {
                    // 嵌套缩进：每层 2 个空格
                    let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                    match list_stack.last_mut() {
                        Some(Some(n)) => {
                            node.content.push_str(&format!("{}{}. ", indent, n));
                            *n += 1;
                        }
                        _ => {
                            node.content.push_str(&format!("{}- ", indent));
                        }
                    }
                }
            }
            Event::End(TagEnd::Item) => {
                if let Some(node) = stack.last_mut() {
                    if !node.content.ends_with('\n') {
                        node.content.push('\n');
                    }
                }
            }

            // 表格
            Event::Start(Tag::Table(_)) => {
                if let Some(node) = stack.last_mut() {
                    if !node.content.is_empty() && !node.content.ends_with('\n') {
                        node.content.push('\n');
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push('\n');
                }
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push('\n');
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str("| ");
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push(' ');
                }
            }

            // 图片
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                if let Some(node) = stack.last_mut() {
                    node.content
                        .push_str(&format!("![{}]({})", title, dest_url));
                }
            }

            // 行内 HTML
            Event::Html(html) | Event::InlineHtml(html) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str(&html);
                }
            }

            // 链接
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_url_stack.push(dest_url.to_string());
                if let Some(node) = stack.last_mut() {
                    node.content.push('[');
                    if capturing_title {
                        node.title.push('[');
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                let url = link_url_stack.pop().unwrap_or_default();
                if let Some(node) = stack.last_mut() {
                    let suffix = format!("]({})", url);
                    node.content.push_str(&suffix);
                    if capturing_title {
                        node.title.push_str(&suffix);
                    }
                }
            }

            // 加粗
            Event::Start(Tag::Strong) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str("**");
                    if capturing_title {
                        node.title.push_str("**");
                    }
                }
            }
            Event::End(TagEnd::Strong) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str("**");
                    if capturing_title {
                        node.title.push_str("**");
                    }
                }
            }

            // 斜体
            Event::Start(Tag::Emphasis) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push('*');
                    if capturing_title {
                        node.title.push('*');
                    }
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push('*');
                    if capturing_title {
                        node.title.push('*');
                    }
                }
            }

            // 删除线
            Event::Start(Tag::Strikethrough) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str("~~");
                    if capturing_title {
                        node.title.push_str("~~");
                    }
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str("~~");
                    if capturing_title {
                        node.title.push_str("~~");
                    }
                }
            }

            // 行内代码
            Event::Code(code) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push('`');
                    node.content.push_str(&code);
                    node.content.push('`');
                    if capturing_title {
                        node.title.push('`');
                        node.title.push_str(&code);
                        node.title.push('`');
                    }
                }
            }

            _ => {}
        }
    }

    while stack.len() > 1 {
        if let Some(mut node) = stack.pop() {
            node.token_count = count_tokens(&node.content);
            if let Some(parent) = stack.last_mut() {
                parent.children.push(node);
            }
        }
    }

    let mut final_root = stack
        .pop()
        .ok_or_else(|| MarkdownError::ParseError("Empty stack".into()))?;
    final_root.token_count = count_tokens(&final_root.content);

    Ok(final_root)
}

/// 页面内容描述（用于 `build_from_pages`）
pub struct PageContent {
    /// 页面的 Markdown 内容
    pub content: String,
    /// 页码（可选，1-indexed）
    pub page_number: Option<u32>,
}

/// 将多个页面的 Markdown 内容合并构建为语义树。
///
/// 适用于 PDF 等多页文档场景：每页的 Markdown 内容作为输入，
/// 函数会解析所有 heading 并构建跨页的层级结构。
///
/// # 参数
///
/// - `root_title`: 文档标题
/// - `file_path`: 文件路径
/// - `pages`: 页面内容列表
pub fn build_from_pages(
    root_title: String,
    _file_path: String,
    pages: Vec<PageContent>,
) -> MarkdownNode {
    let root_node = MarkdownNode {
        node_id: "root".to_string(),
        title: root_title,
        level: 0,
        content: String::new(),
        token_count: 0,
        children: Vec::new(),
    };

    let mut stack: Vec<MarkdownNode> = vec![root_node];
    let mut node_counter: usize = 0;
    let mut capturing_title = false;
    let mut link_url_stack: Vec<String> = Vec::new();
    let mut list_stack: Vec<Option<u64>> = Vec::new();

    for page in pages {
        let parser = Parser::new(&page.content);

        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    let new_level = heading_level_to_u32(level);

                    // Pop stack until we find a parent with level < new_level
                    while let Some(top) = stack.last() {
                        if top.level >= new_level {
                            if let Some(mut node) = stack.pop() {
                                node.token_count = count_tokens(&node.content);
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

                    let new_node = MarkdownNode {
                        node_id: {
                            node_counter += 1;
                            format!("node-{}", node_counter)
                        },
                        title: String::new(),
                        level: new_level,
                        content: String::new(),
                        token_count: 0,
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

                // 段落边界
                Event::Start(Tag::Paragraph) => {
                    if let Some(node) = stack.last_mut() {
                        if !node.content.is_empty() && !node.content.ends_with('\n') {
                            node.content.push_str("\n\n");
                        } else if node.content.ends_with('\n') && !node.content.ends_with("\n\n") {
                            node.content.push('\n');
                        }
                    }
                }
                Event::End(TagEnd::Paragraph) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('\n');
                    }
                }

                // 列表
                Event::Start(Tag::List(ordered)) => {
                    list_stack.push(ordered);
                    if let Some(node) = stack.last_mut() {
                        if !node.content.is_empty() && !node.content.ends_with('\n') {
                            node.content.push('\n');
                        }
                    }
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                    if let Some(node) = stack.last_mut() {
                        node.content.push('\n');
                    }
                }

                // 列表项
                Event::Start(Tag::Item) => {
                    if let Some(node) = stack.last_mut() {
                        let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                        match list_stack.last_mut() {
                            Some(Some(n)) => {
                                node.content.push_str(&format!("{}{}. ", indent, n));
                                *n += 1;
                            }
                            _ => {
                                node.content.push_str(&format!("{}- ", indent));
                            }
                        }
                    }
                }
                Event::End(TagEnd::Item) => {
                    if let Some(node) = stack.last_mut() {
                        if !node.content.ends_with('\n') {
                            node.content.push('\n');
                        }
                    }
                }

                // 表格
                Event::Start(Tag::Table(_)) => {
                    if let Some(node) = stack.last_mut() {
                        if !node.content.is_empty() && !node.content.ends_with('\n') {
                            node.content.push('\n');
                        }
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('\n');
                    }
                }
                Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {}
                Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('\n');
                    }
                }
                Event::Start(Tag::TableCell) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("| ");
                    }
                }
                Event::End(TagEnd::TableCell) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push(' ');
                    }
                }

                // 图片
                Event::Start(Tag::Image {
                    dest_url, title, ..
                }) => {
                    if let Some(node) = stack.last_mut() {
                        node.content
                            .push_str(&format!("![{}]({})", title, dest_url));
                    }
                }

                // 行内 HTML
                Event::Html(html) | Event::InlineHtml(html) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str(&html);
                    }
                }

                // 链接
                Event::Start(Tag::Link { dest_url, .. }) => {
                    link_url_stack.push(dest_url.to_string());
                    if let Some(node) = stack.last_mut() {
                        node.content.push('[');
                        if capturing_title {
                            node.title.push('[');
                        }
                    }
                }
                Event::End(TagEnd::Link) => {
                    let url = link_url_stack.pop().unwrap_or_default();
                    if let Some(node) = stack.last_mut() {
                        let suffix = format!("]({})", url);
                        node.content.push_str(&suffix);
                        if capturing_title {
                            node.title.push_str(&suffix);
                        }
                    }
                }

                // 加粗
                Event::Start(Tag::Strong) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("**");
                        if capturing_title {
                            node.title.push_str("**");
                        }
                    }
                }
                Event::End(TagEnd::Strong) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("**");
                        if capturing_title {
                            node.title.push_str("**");
                        }
                    }
                }

                // 斜体
                Event::Start(Tag::Emphasis) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('*');
                        if capturing_title {
                            node.title.push('*');
                        }
                    }
                }
                Event::End(TagEnd::Emphasis) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('*');
                        if capturing_title {
                            node.title.push('*');
                        }
                    }
                }

                // 删除线
                Event::Start(Tag::Strikethrough) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("~~");
                        if capturing_title {
                            node.title.push_str("~~");
                        }
                    }
                }
                Event::End(TagEnd::Strikethrough) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push_str("~~");
                        if capturing_title {
                            node.title.push_str("~~");
                        }
                    }
                }

                // 行内代码
                Event::Code(code) => {
                    if let Some(node) = stack.last_mut() {
                        node.content.push('`');
                        node.content.push_str(&code);
                        node.content.push('`');
                        if capturing_title {
                            node.title.push('`');
                            node.title.push_str(&code);
                            node.title.push('`');
                        }
                    }
                }

                _ => {}
            }
        }

        // 页面之间添加换行分隔
        if let Some(node) = stack.last_mut() {
            node.content.push_str("\n\n");
        }
    }

    // Unwind stack
    while stack.len() > 1 {
        if let Some(mut node) = stack.pop() {
            node.token_count = count_tokens(&node.content);
            if let Some(parent) = stack.last_mut() {
                parent.children.push(node);
            }
        }
    }

    let mut final_root = stack.pop().unwrap();
    final_root.token_count = count_tokens(&final_root.content);

    final_root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_hierarchy() {
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

        let root = parse_text(md, "test", "test.md").unwrap();

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

    #[test]
    fn test_h3_only_headings() {
        let md = r#"这份行程围绕你已订的三段住宿。

---

### 2.15（周日）：抵达大理
- 06:50 内容1

### 2.16（周一）：大理慢游
- 09:00 内容2

### 2.17（周二）：大理到丽江
- 08:00 内容3
"#;

        let root = parse_text(md, "test", "test.md").unwrap();

        assert_eq!(root.children.len(), 3, "Root should have 3 H3 children");
        assert_eq!(root.children[0].title, "2.15（周日）：抵达大理");
        assert_eq!(root.children[1].title, "2.16（周一）：大理慢游");
        assert_eq!(root.children[2].title, "2.17（周二）：大理到丽江");
    }

    #[test]
    fn test_build_from_pages() {
        let pages = vec![
            PageContent {
                content: "# Introduction\nHello World.\n".to_string(),
                page_number: Some(1),
            },
            PageContent {
                content: "Still intro.\n## Section 1.1\nDetails here.".to_string(),
                page_number: Some(2),
            },
        ];

        let root = build_from_pages("Doc".to_string(), "doc.pdf".to_string(), pages);

        assert_eq!(root.title, "Doc");
        assert_eq!(root.children.len(), 1);

        let h1 = &root.children[0];
        assert_eq!(h1.title, "Introduction");
        assert!(h1.content.contains("Hello World"));
        assert!(h1.content.contains("Still intro"));

        assert_eq!(h1.children.len(), 1);
        let h2 = &h1.children[0];
        assert_eq!(h2.title, "Section 1.1");
        assert!(h2.content.contains("Details here"));
    }

    #[test]
    fn test_inline_formatting_preserved() {
        let md = r#"# Test
This is **bold** and *italic* text.
Here is a [link](https://example.com) and `inline code`.
Also ~~deleted~~ text.
"#;

        let root = parse_text(md, "test", "test.md").unwrap();
        let h1 = &root.children[0];

        // 链接还原
        assert!(
            h1.content.contains("[link](https://example.com)"),
            "Link should be preserved. Got: {}",
            h1.content
        );

        // 加粗还原
        assert!(
            h1.content.contains("**bold**"),
            "Bold should be preserved. Got: {}",
            h1.content
        );

        // 斜体还原
        assert!(
            h1.content.contains("*italic*"),
            "Italic should be preserved. Got: {}",
            h1.content
        );

        // 行内代码还原
        assert!(
            h1.content.contains("`inline code`"),
            "Inline code should be preserved. Got: {}",
            h1.content
        );

        // 删除线还原
        assert!(
            h1.content.contains("~~deleted~~"),
            "Strikethrough should be preserved. Got: {}",
            h1.content
        );
    }

    #[test]
    fn test_ordered_list() {
        let md = r#"# Lists
Unordered:
- apple
- banana

Ordered:
1. first
2. second
3. third
"#;

        let root = parse_text(md, "test", "test.md").unwrap();
        let h1 = &root.children[0];

        // 无序列表保持 "- "
        assert!(
            h1.content.contains("- apple"),
            "Unordered list should use '- '. Got: {}",
            h1.content
        );

        // 有序列表使用 "1. ", "2. ", "3. "
        assert!(
            h1.content.contains("1. first"),
            "Ordered list should use '1. '. Got: {}",
            h1.content
        );
        assert!(
            h1.content.contains("2. second"),
            "Ordered list should use '2. '. Got: {}",
            h1.content
        );
        assert!(
            h1.content.contains("3. third"),
            "Ordered list should use '3. '. Got: {}",
            h1.content
        );
    }

    #[test]
    fn test_nested_list() {
        let md = r#"# Nested
- top
  - nested
    - deep
- back to top
"#;

        let root = parse_text(md, "test", "test.md").unwrap();
        let h1 = &root.children[0];

        // 顶层无缩进
        assert!(
            h1.content.contains("- top"),
            "Top-level should have no indent. Got: {}",
            h1.content
        );

        // 第二层 2 空格缩进
        assert!(
            h1.content.contains("  - nested"),
            "Nested should have 2-space indent. Got: {}",
            h1.content
        );

        // 第三层 4 空格缩进
        assert!(
            h1.content.contains("    - deep"),
            "Deeply nested should have 4-space indent. Got: {}",
            h1.content
        );
    }

    #[test]
    fn test_heading_with_formatting() {
        let md = "# **Bold** and `code` Title\nBody text.\n";

        let root = parse_text(md, "test", "test.md").unwrap();
        let h1 = &root.children[0];

        // title 保留格式标记
        assert_eq!(
            h1.title, "**Bold** and `code` Title",
            "Title should preserve inline formatting"
        );

        // content 也保留
        assert!(
            h1.content.contains("**Bold**"),
            "Content should preserve bold. Got: {}",
            h1.content
        );
        assert!(
            h1.content.contains("`code`"),
            "Content should preserve inline code. Got: {}",
            h1.content
        );
    }
}
