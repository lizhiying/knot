use async_trait::async_trait;
use pageindex_rs::{EmbeddingProvider, PageIndexError};

pub struct MockEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn generate_embedding(&self, _text: &str) -> Result<Vec<f32>, PageIndexError> {
        // Return a fixed size vector (e.g., 384 dimensions) with random or zero data
        Ok(vec![0.0; 384])
    }
}
