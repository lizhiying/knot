use crate::{DocumentParser, NodeMeta, PageIndexConfig, PageIndexError, PageNode};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct MarkdownParser;

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }

    /// Public helper to parse markdown text into PageNode tree.
    /// Used by OfficeParser to process converted markdown.
    pub fn parse_text(
        content: &str,
        title: &str,
        file_path: impl Into<String>,
    ) -> Result<PageNode, PageIndexError> {
        let md_node = knot_markdown::parse_text(content, title, file_path)
            .map_err(|e| PageIndexError::ParseError(e.to_string()))?;
        Ok(Self::markdown_node_to_page_node(md_node))
    }

    /// 将 knot_markdown::MarkdownNode 递归转换为 PageNode
    fn markdown_node_to_page_node(mn: knot_markdown::MarkdownNode) -> PageNode {
        PageNode {
            node_id: mn.node_id,
            title: mn.title,
            level: mn.level,
            content: mn.content,
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: String::new(),
                page_number: None,
                line_number: None,
                token_count: mn.token_count,
                extra: HashMap::new(),
            },
            children: mn
                .children
                .into_iter()
                .map(Self::markdown_node_to_page_node)
                .collect(),
        }
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
        let config = PageIndexConfig::new();

        let root = parser.parse(path, &config).await.unwrap();

        // Clean up
        fs::remove_file(path).unwrap();

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
        let config = PageIndexConfig::new();

        let root = parser.parse(path, &config).await.unwrap();
        fs::remove_file(path).unwrap();

        // 验证：应该有 3 个 H3 子节点
        assert_eq!(root.children.len(), 3, "Root should have 3 H3 children");
        assert_eq!(root.children[0].title, "2.15（周日）：抵达大理");
        assert_eq!(root.children[1].title, "2.16（周一）：大理慢游");
        assert_eq!(root.children[2].title, "2.17（周二）：大理到丽江");
    }
}
