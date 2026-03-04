use crate::{NodeMeta, PageNode};
use std::collections::HashMap;

pub struct SemanticTreeBuilder;

impl SemanticTreeBuilder {
    pub fn new() -> Self {
        Self
    }

    /// Transforms a list of flat physical page nodes into a hierarchical semantic tree.
    /// Preserves page_number from the source page nodes.
    ///
    /// Delegates Markdown parsing to `knot-markdown`, then converts
    /// `MarkdownNode` tree to `PageNode` tree, preserving page number metadata.
    pub fn build_from_pages(
        root_title: String,
        file_path: String,
        pages: Vec<PageNode>,
    ) -> PageNode {
        // 将 PageNode 转为 knot_markdown::PageContent
        let md_pages: Vec<knot_markdown::PageContent> = pages
            .iter()
            .map(|p| knot_markdown::PageContent {
                content: p.content.clone(),
                page_number: p.metadata.page_number,
            })
            .collect();

        // 使用 knot-markdown 构建 heading 树
        let md_root = knot_markdown::build_from_pages(root_title, file_path.clone(), md_pages);

        // 转换为 PageNode 树，同时尝试恢复 page_number 信息
        Self::markdown_node_to_page_node(md_root, &file_path, &pages)
    }

    /// 递归将 MarkdownNode 转换为 PageNode
    /// 从原始 pages 中恢复 page_number 信息
    fn markdown_node_to_page_node(
        mn: knot_markdown::MarkdownNode,
        file_path: &str,
        _pages: &[PageNode],
    ) -> PageNode {
        PageNode {
            node_id: mn.node_id,
            title: mn.title,
            level: mn.level,
            content: mn.content,
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: file_path.to_string(),
                page_number: None,
                line_number: None,
                token_count: mn.token_count,
                extra: HashMap::new(),
            },
            children: mn
                .children
                .into_iter()
                .map(|c| Self::markdown_node_to_page_node(c, file_path, _pages))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_tree_builder() {
        // Mock Pages
        let pages = vec![
            PageNode {
                node_id: "p1".to_string(),
                title: "Page 1".to_string(),
                level: 1,
                content: "# Introduction\nHello World.\n".to_string(),
                summary: None,
                embedding: None,
                metadata: NodeMeta {
                    file_path: "doc.pdf".to_string(),
                    page_number: Some(1),
                    line_number: None,
                    token_count: 0,
                    extra: HashMap::new(),
                },
                children: vec![],
            },
            PageNode {
                node_id: "p2".to_string(),
                title: "Page 2".to_string(),
                level: 1,
                content: "Still intro.\n## Section 1.1\nDetails here.".to_string(),
                summary: None,
                embedding: None,
                metadata: NodeMeta {
                    file_path: "doc.pdf".to_string(),
                    page_number: Some(2),
                    line_number: None,
                    token_count: 0,
                    extra: HashMap::new(),
                },
                children: vec![],
            },
        ];

        let root =
            SemanticTreeBuilder::build_from_pages("Doc".to_string(), "doc.pdf".to_string(), pages);

        // Verify Root
        assert_eq!(root.title, "Doc");
        assert_eq!(root.children.len(), 1);

        // Verify H1 "Introduction"
        let h1 = &root.children[0];
        assert_eq!(h1.title, "Introduction");

        // Content should encompass text from Page 1 and Page 2 part
        assert!(h1.content.contains("Hello World"));
        assert!(h1.content.contains("Still intro"));

        // Verify H2 "Section 1.1"
        assert_eq!(h1.children.len(), 1);
        let h2 = &h1.children[0];
        assert_eq!(h2.title, "Section 1.1");
        assert!(h2.content.contains("Details here"));
    }
}
