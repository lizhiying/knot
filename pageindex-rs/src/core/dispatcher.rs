use crate::{
    formats::md::MarkdownParser, DocumentParser, PageIndexConfig, PageIndexError, PageNode,
};
use std::path::Path;

pub struct IndexDispatcher {
    parsers: Vec<Box<dyn DocumentParser>>,
}

impl IndexDispatcher {
    pub fn new() -> Self {
        let mut parsers: Vec<Box<dyn DocumentParser>> = vec![Box::new(MarkdownParser::new())];

        #[cfg(feature = "office")]
        parsers.push(Box::new(crate::formats::docx::DocxParser::new()));

        parsers.push(Box::new(crate::formats::pdf::PdfParser::new()));

        Self { parsers }
    }

    /// 外部调用的主入口
    pub async fn index_file(
        &self,
        path: &Path,
        config: &PageIndexConfig<'_>,
    ) -> Result<PageNode, PageIndexError> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase(); // Ensure case-insensitive matching

        // 1. 路由到具体的解析器
        let parser = self
            .parsers
            .iter()
            .find(|p| p.can_handle(&ext))
            .ok_or_else(|| PageIndexError::UnsupportedFormat(ext.to_string()))?;

        // 2. 执行解析得到原始树
        let mut root_node = parser.parse(path, config).await?;

        // 3. 树优化：执行 Thinning 逻辑，合并碎片节点
        self.apply_tree_thinning(&mut root_node, config.min_token_threshold);

        Ok(root_node)
    }

    /// 递归合并过小的节点，确保每个 RAG 分片有足够的语义信息
    ///
    /// 合并策略:
    /// 1. 后序遍历：先处理子节点
    /// 2. 如果当前节点 token 过少，且只有一个子节点，则合并该子节点
    /// 3. 如果当前节点 token 过少，且所有子节点都是小节点，则全部合并
    /// 4. 否则保持结构不变（避免把大型文档的所有内容都吸收到一个节点）
    fn apply_tree_thinning(&self, node: &mut PageNode, threshold: usize) {
        // 1. 后序遍历：先处理子节点
        for child in &mut node.children {
            self.apply_tree_thinning(child, threshold);
        }

        // 2. 跳过 root 节点（level=0）
        if node.level == 0 {
            return;
        }

        // 3. 如果当前节点内容足够大，不需要合并
        if node.metadata.token_count >= threshold {
            return;
        }

        // 4. 没有子节点，无需处理
        if node.children.is_empty() {
            return;
        }

        // 5. 判断是否应该合并
        let should_merge = if node.children.len() == 1 {
            // 只有一个子节点时，直接合并
            true
        } else {
            // 多个子节点时，只有当所有子节点都是小节点才合并
            node.children
                .iter()
                .all(|c| c.metadata.token_count < threshold && c.children.is_empty())
        };

        if !should_merge {
            return;
        }

        // 6. 执行合并
        let children = std::mem::take(&mut node.children);

        for child in children {
            if !node.content.is_empty() {
                node.content.push_str("\n\n");
            }

            // 重建子节点的 Markdown 格式
            let heading_prefix = "#".repeat(child.level as usize);
            if !child.title.is_empty() {
                node.content
                    .push_str(&format!("{} {}\n", heading_prefix, child.title));
            }
            node.content.push_str(&child.content);

            // 累加 Token
            node.metadata.token_count += child.metadata.token_count;
        }
    }
    /// 递归生成摘要
    pub async fn inject_summaries(&self, node: &mut PageNode, config: &PageIndexConfig<'_>) {
        if !config.enable_auto_summary {
            return;
        }
        self.do_summarize_recursive(node, config).await;
    }

    fn do_summarize_recursive<'a>(
        &'a self,
        node: &'a mut PageNode,
        config: &'a PageIndexConfig<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // 1. Process children first (or parallel? simpler is serial)
            for child in &mut node.children {
                self.do_summarize_recursive(child, config).await;
            }

            // 2. Process current node
            if node.metadata.token_count > config.summary_token_threshold {
                if let Some(provider) = config.llm_provider {
                    // Ignore errors for now (maybe log them)
                    if let Ok(summary) = provider.generate_summary(&node.content).await {
                        node.summary = Some(summary);
                    }
                }
            }
        })
    }

    /// 递归生成向量
    pub async fn inject_embeddings(&self, node: &mut PageNode, config: &PageIndexConfig<'_>) {
        if let Some(provider) = config.embedding_provider {
            self.do_embedding_recursive(node, provider).await;
        }
    }

    fn do_embedding_recursive<'a>(
        &'a self,
        node: &'a mut PageNode,
        provider: &'a dyn crate::EmbeddingProvider,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // 1. Process children
            for child in &mut node.children {
                self.do_embedding_recursive(child, provider).await;
            }

            // 2. Process current node
            // Embedding logic: use content (plus summary if available?)
            // For now, simple content.
            // Ignore errors for now.
            let text_to_embed = if let Some(summary) = &node.summary {
                format!("{}\nSummary: {}", node.content, summary)
            } else {
                node.content.clone()
            };

            if !text_to_embed.is_empty() {
                // println!("Generating embedding for node {} (len: {})...", node.title, text_to_embed.len());
                match provider.generate_embedding(&text_to_embed).await {
                    Ok(embedding) => {
                        node.embedding = Some(embedding);
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to generate embedding for node {}: {}",
                            node.title, e
                        );
                    }
                }
            } else {
                println!(
                    "Skipping embedding for node {}: content is empty",
                    node.title
                );
            }
        })
    }
}

impl Default for IndexDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeMeta;
    use std::collections::HashMap;

    fn create_mock_node(id: &str, level: u32, token_count: usize) -> PageNode {
        PageNode {
            node_id: id.to_string(),
            title: format!("Title {}", id),
            level,
            content: format!("Content {}", id),
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: "test".to_string(),
                page_number: None,
                line_number: None,
                token_count,
                extra: HashMap::new(),
            },
            children: Vec::new(),
        }
    }

    #[test]
    fn test_tree_thinning() {
        let dispatcher = IndexDispatcher::new();

        // Construct a tree:
        // Root (Token: 10) < Threshold 50
        //   - Child1 (Token: 30)
        //   - Child2 (Token: 30)
        // Expected: Root absorbs Child1 and Child2. Root Token becomes ~70. Children empty.

        let mut root = create_mock_node("root", 1, 10);
        root.children.push(create_mock_node("c1", 2, 30));
        root.children.push(create_mock_node("c2", 2, 30));

        dispatcher.apply_tree_thinning(&mut root, 50);

        assert!(root.children.is_empty());
        assert!(root.metadata.token_count >= 70);
        assert!(root.content.contains("## Title c1"));
        assert!(root.content.contains("Content c1"));
        assert!(root.content.contains("## Title c2"));
    }

    #[test]
    fn test_tree_thinning_nested() {
        let dispatcher = IndexDispatcher::new();

        // Root (100) > Threshold 50 -> Keep
        //   - Child1 (10) < Threshold 50 -> Absorb SubChild
        //      - SubChild (20)
        // Expected: Root keeps Child1. Child1 absorbs SubChild.

        let mut root = create_mock_node("root", 1, 100);
        let mut child1 = create_mock_node("c1", 2, 10);
        let subchild = create_mock_node("sc1", 3, 20);

        child1.children.push(subchild);
        root.children.push(child1);

        dispatcher.apply_tree_thinning(&mut root, 50);

        assert_eq!(root.children.len(), 1);
        let new_child1 = &root.children[0];
        assert!(new_child1.children.is_empty());
        assert!(new_child1.metadata.token_count >= 30);
        assert!(new_child1.content.contains("### Title sc1"));
    }

    struct MockLlm;
    #[async_trait::async_trait]
    impl crate::LlmProvider for MockLlm {
        async fn generate_summary(&self, text: &str) -> Result<String, PageIndexError> {
            Ok(format!("Summary of: {}", text))
        }

        async fn generate_content(&self, prompt: &str) -> Result<String, PageIndexError> {
            Ok(format!("Content for: {}", prompt))
        }

        async fn generate_content_with_image(
            &self,
            prompt: &str,
            _image: &[u8],
        ) -> Result<String, PageIndexError> {
            Ok(format!("Content with image for: {}", prompt))
        }
    }

    #[tokio::test]
    async fn test_inject_summaries() {
        let dispatcher = IndexDispatcher::new();
        let mock_llm = MockLlm;

        // Node 1: Token 100 > Threshold 50 -> Generate Summary
        // Node 2: Token 10 < Threshold 50 -> Skip

        let mut root = create_mock_node("high", 1, 100);
        root.content = "High content".to_string();
        let mut child = create_mock_node("low", 2, 10);
        child.content = "Low content".to_string();

        root.children.push(child);

        let config = PageIndexConfig {
            vision_provider: None,
            llm_provider: Some(&mock_llm),
            embedding_provider: None,
            min_token_threshold: 0,
            summary_token_threshold: 50,
            enable_auto_summary: true,
            default_language: "en".into(),
            progress_callback: None,
        };

        dispatcher.inject_summaries(&mut root, &config).await;

        assert_eq!(root.summary, Some("Summary of: High content".to_string()));
        assert_eq!(root.children[0].summary, None);
    }

    struct MockEmbedding;
    #[async_trait::async_trait]
    impl crate::EmbeddingProvider for MockEmbedding {
        async fn generate_embedding(&self, _text: &str) -> Result<Vec<f32>, PageIndexError> {
            Ok(vec![0.1, 0.2, 0.3])
        }
    }

    #[tokio::test]
    async fn test_inject_embeddings() {
        let dispatcher = IndexDispatcher::new();
        let mock_emb = MockEmbedding;

        let mut root = create_mock_node("root", 1, 10);
        root.content = "Content".to_string();

        let config = PageIndexConfig {
            vision_provider: None,
            llm_provider: None,
            embedding_provider: Some(&mock_emb),
            min_token_threshold: 0,
            summary_token_threshold: 0,
            enable_auto_summary: false,
            default_language: "en".into(),
            progress_callback: None,
        };

        dispatcher.inject_embeddings(&mut root, &config).await;

        assert_eq!(root.embedding, Some(vec![0.1, 0.2, 0.3]));
    }
}
