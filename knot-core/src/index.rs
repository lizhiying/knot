use crate::mock_embedding::MockEmbeddingProvider;
use crate::store::VectorRecord;
use anyhow::Result;
use pageindex_rs::{IndexDispatcher, PageIndexConfig, PageNode};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct KnotIndexer {
    dispatcher: IndexDispatcher,
    embedding_provider: MockEmbeddingProvider,
    registry: Option<crate::registry::FileRegistry>,
}

impl KnotIndexer {
    pub async fn new(db_url: Option<String>) -> Self {
        let registry = if let Some(url) = db_url {
            crate::registry::FileRegistry::new(&url).await.ok()
        } else {
            None
        };

        Self {
            dispatcher: IndexDispatcher::new(),
            embedding_provider: MockEmbeddingProvider,
            registry,
        }
    }

    pub async fn index_directory(&self, path: &Path) -> Result<(Vec<VectorRecord>, Vec<String>)> {
        let mut records = Vec::new();
        let mut seen_files = std::collections::HashSet::new();

        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let file_path = entry.path();
                // Simple filter: only md/txt
                if let Some(ext) = file_path.extension() {
                    if ext == "md" || ext == "txt" {
                        let path_str = file_path.to_string_lossy().to_string();
                        seen_files.insert(path_str.clone());

                        // Check hash if registry exists
                        let content = std::fs::read(file_path)?;
                        let hash = hex::encode(blake3::hash(&content).as_bytes());
                        let modified = entry.metadata()?.modified()?.elapsed()?.as_secs() as i64;

                        let should_index = if let Some(reg) = &self.registry {
                            if let Ok(Some(stored_hash)) = reg.get_file_hash(&path_str).await {
                                if stored_hash != hash {
                                    println!(
                                        "Hash mismatch: stored={} current={}",
                                        stored_hash, hash
                                    );
                                    true
                                } else {
                                    false
                                }
                            } else {
                                println!("Hash not found in registry for {}", path_str);
                                true
                            }
                        } else {
                            println!("Registry not initialized");
                            true
                        };

                        if should_index {
                            println!("Indexing: {:?}", file_path);
                            if let Ok(file_records) = self.index_file(file_path).await {
                                records.extend(file_records);
                                // Update registry
                                if let Some(reg) = &self.registry {
                                    let _ = reg.update_file(&path_str, &hash, modified).await;
                                }
                            } else {
                                eprintln!("Failed to index {:?}", file_path);
                            }
                        } else {
                            println!("Skipping unchanged: {:?}", file_path);
                        }
                    }
                }
            }
        }

        let mut deleted_files = Vec::new();
        // Prune logic
        if let Some(reg) = &self.registry {
            if let Ok(all_files) = reg.get_all_files().await {
                let path_prefix = path.to_string_lossy().to_string();
                for tracked_file in all_files {
                    // Check if tracked file belongs to indexed directory and is not seen
                    if tracked_file.starts_with(&path_prefix) && !seen_files.contains(&tracked_file)
                    {
                        println!("Detected deleted file: {}", tracked_file);
                        let _ = reg.remove_file(&tracked_file).await;
                        deleted_files.push(tracked_file);
                    }
                }
            }
        }

        Ok((records, deleted_files))
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
                records.push(VectorRecord {
                    id: current_id.clone(),
                    text: node.content, // move content
                    vector: embedding,
                    file_path: file_path.to_string(),
                    parent_id: parent_id.clone(),
                    breadcrumbs: bc_string,
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
