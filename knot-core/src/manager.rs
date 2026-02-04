use crate::embedding::EmbeddingEngine;
use crate::llm::LlamaSidecar;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct EngineManager {
    pub embedding: Arc<RwLock<Option<EmbeddingEngine>>>,
    pub llm: Arc<RwLock<Option<LlamaSidecar>>>,
}
