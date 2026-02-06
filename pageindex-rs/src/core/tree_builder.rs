use crate::{NodeMeta, PageNode};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::collections::HashMap;

pub struct SemanticTreeBuilder;

impl SemanticTreeBuilder {
    pub fn new() -> Self {
        Self
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

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// Transforms a list of flat physical page nodes into a hierarchical semantic tree.
    /// Preserves page_number from the source page nodes.
    pub fn build_from_pages(
        root_title: String,
        file_path: String,
        pages: Vec<PageNode>,
    ) -> PageNode {
        let root_node = PageNode {
            node_id: "root".to_string(),
            title: root_title,
            level: 0,
            content: String::new(),
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: file_path.clone(),
                page_number: Some(1), // Default to start
                line_number: None,
                token_count: 0,
                extra: HashMap::new(),
            },
            children: Vec::new(),
        };

        let mut stack: Vec<PageNode> = vec![root_node];
        let mut node_counter: usize = 0;
        let mut capturing_title = false;

        for page in pages {
            let page_num = page.metadata.page_number;
            let parser = Parser::new(&page.content);

            // Important: We append the raw page content to the root's full content view?
            // Or do we just rely on leaf nodes?
            // Usually, a PDF tree root represents the whole doc, so maybe we accumulate?
            // For now, let's focus on building the tree structure.
            // Note: If we just stream parse the content, we need to handle the stack continuity across pages.

            for event in parser {
                match event {
                    Event::Start(Tag::Heading { level, .. }) => {
                        let new_level = Self::heading_level_to_u32(level);

                        // Pop stack until we find a parent with level < new_level
                        while let Some(top) = stack.last() {
                            if top.level >= new_level {
                                if let Some(mut node) = stack.pop() {
                                    node.metadata.token_count = Self::count_tokens(&node.content);
                                    if let Some(parent) = stack.last_mut() {
                                        parent.children.push(node);
                                    } else {
                                        // Should not happen as root is level 0
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
                                format!("node-{}", node_counter)
                            },
                            title: String::new(),
                            level: new_level,
                            content: String::new(),
                            summary: None,
                            embedding: None,
                            metadata: NodeMeta {
                                file_path: file_path.clone(),
                                page_number: page_num, // Preserve Page Number!
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

                            // If the current node doesn't have a page number yet (e.g. root), set it.
                            // But for nodes created via Heading, we set it at creation.
                            // For Root, we set it at creation.
                            // We might update page_number bounds (start/end) in the future, but for now simple is ok.
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

            // Add a newline between pages to separate content cleanly
            if let Some(node) = stack.last_mut() {
                node.content.push_str("\n\n");
            }
        }

        // Unwind stack
        while stack.len() > 1 {
            if let Some(mut node) = stack.pop() {
                node.metadata.token_count = Self::count_tokens(&node.content);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
        }

        let mut final_root = stack.pop().unwrap(); // Safe because we started with root
        final_root.metadata.token_count = Self::count_tokens(&final_root.content);

        final_root
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
        assert_eq!(h1.metadata.page_number, Some(1));

        // Content should encompass text from Page 1 and Page 2 part
        assert!(h1.content.contains("Hello World"));
        assert!(h1.content.contains("Still intro"));

        // Verify H2 "Section 1.1"
        assert_eq!(h1.children.len(), 1);
        let h2 = &h1.children[0];
        assert_eq!(h2.title, "Section 1.1");
        assert_eq!(h2.metadata.page_number, Some(2)); // Should pick up Page 2
        assert!(h2.content.contains("Details here"));
    }
}
