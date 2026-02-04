use crate::mock_embedding::MockEmbeddingProvider;
use crate::store::VectorRecord;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use pageindex_rs::{IndexDispatcher, PageIndexConfig, PageNode};
use std::path::Path;
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
                            if ext == "md" || ext == "txt" {
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
                                                println!("Skipping unchanged: {:?}", file_path);
                                            } else {
                                                println!("Start Indexing: {:?}", file_path);
                                            }
                                        } else {
                                            println!("Start Indexing (New): {:?}", file_path);
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
