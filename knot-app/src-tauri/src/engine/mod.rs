use std::sync::Arc;
use tokio::sync::RwLock;

pub mod embedding;
pub mod llm;

pub struct EngineManager {
    pub embedding: Arc<RwLock<Option<embedding::EmbeddingEngine>>>,
    pub llm: Arc<RwLock<Option<llm::LlamaSidecar>>>,
}
