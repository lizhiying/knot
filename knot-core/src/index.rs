use crate::mock_embedding::MockEmbeddingProvider;
use crate::path_processor::PathProcessor;
use crate::store::VectorRecord;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use knot_parser::{IndexDispatcher, PageIndexConfig, PageNode};
use std::path::Path;
use walkdir::WalkDir;

use knot_parser::EmbeddingProvider;
use std::sync::Arc;

pub struct KnotIndexer {
    dispatcher: IndexDispatcher,
    embedding_provider: Arc<dyn EmbeddingProvider + Send + Sync>,
    registry: Option<crate::registry::FileRegistry>,
    // PDF parsing config
    pub pdf_ocr_enabled: bool,
    pub pdf_ocr_model_dir: Option<String>,
    pub pdf_vision_api_url: Option<String>,
    pub pdf_vision_model: Option<String>,
}

impl KnotIndexer {
    pub async fn new(
        db_path: &str,
        provider: Option<Arc<dyn EmbeddingProvider + Send + Sync>>,
    ) -> Self {
        let db_url = format!("sqlite://{}?mode=rwc", db_path);

        let registry = crate::registry::FileRegistry::new(&db_url).await.ok();

        let embedding_provider = provider.unwrap_or_else(|| {
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider + Send + Sync>
        });

        Self {
            dispatcher: IndexDispatcher::new(),
            embedding_provider,
            registry,
            pdf_ocr_enabled: false,
            pdf_ocr_model_dir: None,
            pdf_vision_api_url: None,
            pdf_vision_model: None,
        }
    }

    pub async fn index_directory(&self, path: &Path) -> Result<(Vec<VectorRecord>, Vec<String>)> {
        let entries: Vec<_> = WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .collect();

        let mut results = stream::iter(entries)
            .map(|entry| {
                let registry = self.registry.clone();
                let indexer_ref = self; // Since &self is Copy? No. &self is shared ref.
                                        // We need to pass references or clone needed data.
                                        // self.index_file is async and takes &self.
                                        // But self needs to be static or Arc?
                                        // self is &KnotIndexer. If index_directory takes &self, the lifetime of &self outlives the future?
                                        // Yes, because we await the stream.
                async move {
                    let mut records = Vec::new();
                    let mut files_seen = Vec::new();

                    if entry.file_type().is_file() {
                        let file_path = entry.path();
                        if let Some(ext) = file_path.extension() {
                            if ext == "md"
                                || ext == "txt"
                                || ext == "pdf"
                                || ext == "xlsx"
                                || ext == "xls"
                            {
                                let path_str = file_path.to_string_lossy().to_string();
                                files_seen.push(path_str.clone());

                                let mut should_index = true;
                                let mut hash = String::new();
                                let mut modified = 0;

                                // content read might block, but it's file io.
                                if let Ok(content) = std::fs::read(file_path) {
                                    hash = hex::encode(blake3::hash(&content).as_bytes());
                                    if let Ok(meta) = entry.metadata() {
                                        if let Ok(m) = meta.modified() {
                                            if let Ok(el) = m.elapsed() {
                                                modified = el.as_secs() as i64;
                                            }
                                        }
                                    }

                                    if let Some(reg) = &registry {
                                        if let Ok(Some(stored_hash)) =
                                            reg.get_file_hash(&path_str).await
                                        {
                                            if stored_hash == hash {
                                                should_index = false;
                                                // println!("Skipping unchanged: {:?}", file_path);
                                            } else {
                                                if std::env::var("KNOT_QUIET").is_err() {
                                                    println!("Start Indexing: {:?}", file_path);
                                                }
                                            }
                                        } else {
                                            if std::env::var("KNOT_QUIET").is_err() {
                                                println!("Start Indexing (New): {:?}", file_path);
                                            }
                                        }
                                    }
                                }

                                if should_index {
                                    // index_file is on self.
                                    // rust async closure capture issue.
                                    // self must be valid.
                                    // Since we are in &self method, and we await stream, it should be fine?
                                    // Compiler might complain self not 'static.
                                    // But indexer_ref has lifetime 'a.
                                    // We need to verify if this compiles.
                                    // If not, KnotIndexer usually wrapped in Arc in main.
                                    // But here it's &self.

                                    // Let's try.
                                    if let Ok(file_records) =
                                        indexer_ref.index_file(file_path).await
                                    {
                                        records.extend(file_records);
                                        if let Some(reg) = &registry {
                                            let _ =
                                                reg.update_file(&path_str, &hash, modified).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    (records, files_seen)
                }
            })
            .buffer_unordered(8); // Concurrency

        let mut final_records = Vec::new();
        let mut seen_files = std::collections::HashSet::new();

        while let Some((recs, files)) = results.next().await {
            final_records.extend(recs);
            for f in files {
                seen_files.insert(f);
            }
        }

        let mut deleted_files = Vec::new();
        // Prune logic
        if let Some(reg) = &self.registry {
            if let Ok(all_files) = reg.get_all_files().await {
                let path_prefix = path.to_string_lossy().to_string();
                for tracked_file in all_files {
                    if tracked_file.starts_with(&path_prefix) && !seen_files.contains(&tracked_file)
                    {
                        println!("Detected deleted file: {}", tracked_file);
                        let _ = reg.remove_file(&tracked_file).await;
                        deleted_files.push(tracked_file);
                    }
                }
            }
        }

        Ok((final_records, deleted_files))
    }

    pub async fn index_file(&self, path: &Path) -> Result<Vec<VectorRecord>> {
        // Setup config with embedding provider and PDF parsing options
        let mut config =
            PageIndexConfig::new().with_embedding_provider(self.embedding_provider.as_ref());
        config.min_token_threshold = 0;
        config.pdf_ocr_enabled = self.pdf_ocr_enabled;
        config.pdf_ocr_model_dir = self.pdf_ocr_model_dir.clone();
        config.pdf_vision_api_url = self.pdf_vision_api_url.clone();
        config.pdf_vision_model = self.pdf_vision_model.clone();

        // Ensure absolute path
        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        // Parse file
        let mut root_node = self.dispatcher.index_file(&abs_path, &config).await?;

        // Manually generate embeddings for nodes if missing
        let file_name = PathProcessor::extract_file_name(&path.to_string_lossy());
        let directory_tags = PathProcessor::extract_directory_tags(&path.to_string_lossy());

        self.enrich_node(&mut root_node, &file_name, &directory_tags, &[])
            .await?;

        // Build document-level summary BEFORE flatten_tree consumes the tree
        let doc_summary = Self::build_doc_summary(&root_node);
        let path_str = abs_path.to_string_lossy().to_string();

        // Flatten to records
        let mut records = self.flatten_tree(root_node, &abs_path);

        // Generate summary record if summary is non-empty
        if !doc_summary.is_empty() {
            let summary_enriched = format!(
                "File: {} | Path: {}\n[Document Overview]\n{}",
                file_name, directory_tags, doc_summary
            );

            if let Ok(summary_vec) = self
                .embedding_provider
                .generate_embedding(&summary_enriched)
                .await
            {
                let extension = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let doc_type = if matches!(extension, "xlsx" | "xls" | "xlsm" | "xlsb") {
                    "tabular".to_string()
                } else {
                    "text".to_string()
                };
                records.push(VectorRecord {
                    id: format!("{}-doc-summary", path_str),
                    text: format!("[文档概览] {}\n\n{}", file_name, doc_summary),
                    vector: summary_vec,
                    file_path: path_str,
                    parent_id: None,
                    breadcrumbs: None,
                    doc_type,
                });
            }
        }

        Ok(records)
    }

    async fn enrich_node(
        &self,
        node: &mut PageNode,
        file_name: &str,
        directory_tags: &str,
        breadcrumbs: &[String],
    ) -> Result<()> {
        if node.embedding.is_none() && !node.content.is_empty() {
            // 构建层级上下文：File + Path + Breadcrumbs + Content
            let breadcrumb_str = if breadcrumbs.is_empty() {
                String::new()
            } else {
                format!("Section: {}\n", breadcrumbs.join(" > "))
            };

            let enriched_text = format!(
                "File: {} | Path: {}\n{}{}",
                file_name, directory_tags, breadcrumb_str, node.content
            );

            let vec = self
                .embedding_provider
                .generate_embedding(&enriched_text)
                .await?;
            node.embedding = Some(vec);
        }

        // 子节点的 breadcrumbs = 当前 breadcrumbs + 当前 title
        let mut child_breadcrumbs = breadcrumbs.to_vec();
        if !node.title.is_empty() {
            child_breadcrumbs.push(node.title.clone());
        }

        for child in &mut node.children {
            Box::pin(self.enrich_node(child, file_name, directory_tags, &child_breadcrumbs))
                .await?;
        }
        Ok(())
    }

    /// 构建文档级摘要：包含文档标题、章节目录和各章节首句
    fn build_doc_summary(root: &PageNode) -> String {
        // 跳过空文档
        if root.children.is_empty() && root.content.is_empty() {
            return String::new();
        }

        let mut summary = String::new();

        // 文档标题
        if !root.title.is_empty() {
            summary.push_str(&format!("# {}\n\n", root.title));
        }

        // 构建目录 + 各节摘要
        summary.push_str("目录与摘要:\n");
        for child in &root.children {
            Self::collect_outline(child, &mut summary, 0);
        }

        // 如果 root 自身有内容（无 heading 的文档），取前 200 字符
        if !root.content.is_empty() && root.children.is_empty() {
            let snippet: String = root.content.chars().take(200).collect();
            summary.push_str(&format!("\n{}", snippet));
            if root.content.chars().count() > 200 {
                summary.push_str("...");
            }
        }

        summary
    }

    /// 递归收集章节标题和首句摘要
    fn collect_outline(node: &PageNode, output: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);

        // 标题
        if !node.title.is_empty() {
            output.push_str(&format!("{}- {}", indent, node.title));

            // 提取首句摘要（去掉标题行后取第一段非空内容）
            let content_after_title = node
                .content
                .strip_prefix(&node.title)
                .unwrap_or(&node.content)
                .trim_start_matches('\n');

            if !content_after_title.is_empty() {
                // 取第一个非空行，最多 100 字符
                let first_line = content_after_title
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("");

                if !first_line.is_empty() {
                    let snippet: String = first_line.chars().take(100).collect();
                    output.push_str(&format!(": {}", snippet));
                    if first_line.chars().count() > 100 {
                        output.push_str("...");
                    }
                }
            }

            output.push('\n');
        }

        // 递归子节点
        for child in &node.children {
            Self::collect_outline(child, output, depth + 1);
        }
    }

    fn flatten_tree(&self, root: PageNode, file_path: &Path) -> Vec<VectorRecord> {
        let mut records = Vec::new();
        let path_str = file_path.to_string_lossy().to_string();

        // Start recursion with no parent and empty breadcrumbs
        self.flatten_recursive(root, &path_str, None, Vec::new(), &mut records);

        records
    }

    fn flatten_recursive(
        &self,
        node: PageNode,
        file_path: &str,
        parent_id: Option<String>,
        mut breadcrumbs: Vec<String>,
        records: &mut Vec<VectorRecord>,
    ) {
        // Current breadcrumbs string
        let bc_string = if breadcrumbs.is_empty() {
            None
        } else {
            Some(breadcrumbs.join(" > "))
        };

        // Add current record if it has embedding
        // Note: Even if it doesn't have embedding (container node), needed for breadcrumbs?
        // Yes, index logic: Only leaf nodes or nodes with content usually.
        // But for Knot, maybe we index everything that has content.

        // Clone for children before moving or borrowing
        let current_id = node.node_id.clone();
        let current_title = node.title.clone();

        if let Some(embedding) = node.embedding {
            if !node.content.is_empty() {
                // 从 PageNode metadata 中读取 doc_type（Excel parser 写入 "tabular"）
                let doc_type = node
                    .metadata
                    .extra
                    .get("doc_type")
                    .cloned()
                    .unwrap_or_else(|| "text".to_string());
                records.push(VectorRecord {
                    id: current_id.clone(),
                    text: node.content, // move content
                    vector: embedding,
                    file_path: file_path.to_string(),
                    parent_id: parent_id.clone(),
                    breadcrumbs: bc_string,
                    doc_type,
                });
            }
        }

        // Prepare breadcrumbs for children
        breadcrumbs.push(current_title);

        for child in node.children {
            self.flatten_recursive(
                child,
                file_path,
                Some(current_id.clone()),
                breadcrumbs.clone(),
                records,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_parser::{EmbeddingProvider, PageIndexError};
    use std::sync::Mutex;

    struct SpyEmbeddingProvider {
        last_text: Arc<Mutex<String>>,
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for SpyEmbeddingProvider {
        async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, PageIndexError> {
            let mut last = self.last_text.lock().unwrap();
            *last = text.to_string();
            Ok(vec![0.0; 512])
        }
    }

    #[tokio::test]
    async fn test_enrich_node_with_metadata() {
        let last_text = Arc::new(Mutex::new(String::new()));
        let provider = Arc::new(SpyEmbeddingProvider {
            last_text: last_text.clone(),
        });

        let indexer = KnotIndexer::new(":memory:", Some(provider)).await;

        // Setup a mock node
        let mut node = PageNode {
            node_id: "1".to_string(),
            title: "Test Node".to_string(),
            level: 1,
            content: "Original Content".to_string(),
            summary: None,
            embedding: None,
            metadata: knot_parser::NodeMeta {
                file_path: "test.md".to_string(),
                page_number: None,
                line_number: None,
                token_count: 0,
                extra: std::collections::HashMap::new(),
            },
            children: vec![],
        };

        let file_name = "test.md";
        let tags = "project src";

        // Call the private method via test access (tests is child module so it can access private items)
        indexer
            .enrich_node(&mut node, file_name, tags, &[])
            .await
            .unwrap();

        // Check if embedding was generated using enriched text
        let captured_text = last_text.lock().unwrap().clone();
        assert!(captured_text.contains("File: test.md"));
        assert!(captured_text.contains("Path: project src"));
        assert!(captured_text.contains("Original Content"));

        // Check if original content is preserved
        assert_eq!(node.content, "Original Content");
        assert!(node.embedding.is_some());
    }

    #[tokio::test]
    async fn test_enrich_node_with_breadcrumbs() {
        let last_text = Arc::new(Mutex::new(String::new()));
        let provider = Arc::new(SpyEmbeddingProvider {
            last_text: last_text.clone(),
        });

        let indexer = KnotIndexer::new(":memory:", Some(provider)).await;

        // 构建两层嵌套：root → 第一章 → 1.1 节
        let child_node = PageNode {
            node_id: "child-1".to_string(),
            title: "1.1 节".to_string(),
            level: 2,
            content: "监督学习的内容".to_string(),
            summary: None,
            embedding: None,
            metadata: knot_parser::NodeMeta {
                file_path: "ml.md".to_string(),
                page_number: None,
                line_number: None,
                token_count: 0,
                extra: std::collections::HashMap::new(),
            },
            children: vec![],
        };

        let mut root = PageNode {
            node_id: "root".to_string(),
            title: "第一章".to_string(),
            level: 1,
            content: "机器学习概述".to_string(),
            summary: None,
            embedding: None,
            metadata: knot_parser::NodeMeta {
                file_path: "ml.md".to_string(),
                page_number: None,
                line_number: None,
                token_count: 0,
                extra: std::collections::HashMap::new(),
            },
            children: vec![child_node],
        };

        indexer
            .enrich_node(&mut root, "ml.md", "docs", &[])
            .await
            .unwrap();

        // 子节点的 embedding 输入应包含 breadcrumbs
        let captured = last_text.lock().unwrap().clone();
        assert!(
            captured.contains("Section: 第一章"),
            "Child embedding should contain parent breadcrumb. Got: {}",
            captured
        );
        assert!(
            captured.contains("监督学习的内容"),
            "Child embedding should contain its own content"
        );
    }

    #[test]
    fn test_build_doc_summary() {
        let child1 = PageNode {
            node_id: "1".to_string(),
            title: "机器学习概述".to_string(),
            level: 1,
            content: "机器学习概述\n本章介绍机器学习的基本概念和发展历史。\n".to_string(),
            summary: None,
            embedding: None,
            metadata: knot_parser::NodeMeta {
                file_path: "ml.md".to_string(),
                page_number: None,
                line_number: None,
                token_count: 20,
                extra: std::collections::HashMap::new(),
            },
            children: vec![
                PageNode {
                    node_id: "1-1".to_string(),
                    title: "监督学习".to_string(),
                    level: 2,
                    content: "监督学习\nSVM 和决策树是常见方法。\n".to_string(),
                    summary: None,
                    embedding: None,
                    metadata: knot_parser::NodeMeta {
                        file_path: "ml.md".to_string(),
                        page_number: None,
                        line_number: None,
                        token_count: 10,
                        extra: std::collections::HashMap::new(),
                    },
                    children: vec![],
                },
                PageNode {
                    node_id: "1-2".to_string(),
                    title: "无监督学习".to_string(),
                    level: 2,
                    content: "无监督学习\n聚类和降维技术。\n".to_string(),
                    summary: None,
                    embedding: None,
                    metadata: knot_parser::NodeMeta {
                        file_path: "ml.md".to_string(),
                        page_number: None,
                        line_number: None,
                        token_count: 10,
                        extra: std::collections::HashMap::new(),
                    },
                    children: vec![],
                },
            ],
        };

        let root = PageNode {
            node_id: "root".to_string(),
            title: "机器学习教程".to_string(),
            level: 0,
            content: String::new(),
            summary: None,
            embedding: None,
            metadata: knot_parser::NodeMeta {
                file_path: "ml.md".to_string(),
                page_number: None,
                line_number: None,
                token_count: 0,
                extra: std::collections::HashMap::new(),
            },
            children: vec![child1],
        };

        let summary = KnotIndexer::build_doc_summary(&root);

        // 包含文档标题
        assert!(
            summary.contains("# 机器学习教程"),
            "Summary should contain doc title. Got:\n{}",
            summary
        );

        // 包含章节标题
        assert!(
            summary.contains("机器学习概述"),
            "Summary should contain H1 title"
        );
        assert!(
            summary.contains("监督学习"),
            "Summary should contain H2 title"
        );
        assert!(
            summary.contains("无监督学习"),
            "Summary should contain H2 title"
        );

        // 包含章节首句摘要
        assert!(
            summary.contains("本章介绍机器学习的基本概念"),
            "Summary should contain first line of H1. Got:\n{}",
            summary
        );
        assert!(
            summary.contains("SVM"),
            "Summary should contain first line snippet of H2"
        );

        // 子节点应该有缩进
        assert!(
            summary.contains("  - 监督学习"),
            "H2 should be indented under H1. Got:\n{}",
            summary
        );
    }
}
