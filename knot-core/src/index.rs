use crate::mock_embedding::MockEmbeddingProvider;
use crate::store::VectorRecord;
use anyhow::Result;
use pageindex_rs::{IndexDispatcher, PageIndexConfig, PageNode};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct KnotIndexer {
    dispatcher: IndexDispatcher,
    embedding_provider: MockEmbeddingProvider,
}

impl KnotIndexer {
    pub fn new() -> Self {
        Self {
            dispatcher: IndexDispatcher::new(), // You might need to check how to init this
            embedding_provider: MockEmbeddingProvider,
        }
    }

    pub async fn index_directory(&self, path: &Path) -> Result<Vec<VectorRecord>> {
        let mut records = Vec::new();

        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let file_path = entry.path();
                // Simple filter: only md/txt
                if let Some(ext) = file_path.extension() {
                    if ext == "md" || ext == "txt" {
                        println!("Indexing: {:?}", file_path);
                        if let Ok(file_records) = self.index_file(file_path).await {
                            records.extend(file_records);
                        } else {
                            eprintln!("Failed to index {:?}", file_path);
                        }
                    }
                }
            }
        }
        Ok(records)
    }

    pub async fn index_file(&self, path: &Path) -> Result<Vec<VectorRecord>> {
        // Setup config with mock embedding
        let mut config = PageIndexConfig::new().with_embedding_provider(&self.embedding_provider);
        config.min_token_threshold = 0;

        // Parse file
        let mut root_node = self.dispatcher.index_file(path, &config).await?;

        // Manually generate embeddings for nodes if missing
        self.enrich_node(&mut root_node).await?;

        // Flatten to records
        Ok(self.flatten_tree(root_node, path))
    }

    async fn enrich_node(&self, node: &mut PageNode) -> Result<()> {
        use pageindex_rs::EmbeddingProvider; // Trait must be in scope

        if node.embedding.is_none() && !node.content.is_empty() {
            let vec = self
                .embedding_provider
                .generate_embedding(&node.content)
                .await?;
            node.embedding = Some(vec);
        }

        for child in &mut node.children {
            Box::pin(self.enrich_node(child)).await?;
        }
        Ok(())
    }

    fn flatten_tree(&self, root: PageNode, file_path: &Path) -> Vec<VectorRecord> {
        let mut records = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(root);

        while let Some(node) = queue.pop_front() {
            // Processing logic:
            // For iteration 1, we just take nodes that have embeddings and content
            if let Some(embedding) = node.embedding {
                if !node.content.is_empty() {
                    records.push(VectorRecord {
                        id: node.node_id.clone(),
                        text: node.content.clone(),
                        vector: embedding,
                        file_path: file_path.to_string_lossy().to_string(),
                    });
                }
            }

            // Push children
            for child in node.children {
                queue.push_back(child);
            }
        }

        records
    }
}
