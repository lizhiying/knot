use anyhow::Result;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use tokenizers::utils::truncation::TruncationParams;
use tokenizers::Tokenizer;

pub struct EmbeddingEngine {
    session: Session,
    tokenizer: Tokenizer,
}

impl EmbeddingEngine {
    pub fn init_onnx(model_path: &str) -> Result<Self> {
        // Load the session using ort 2.x API
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;

        // Tokenizer 文件命名规则: {model_basename}-tokenizer.json
        // 例如: bge-small-zh-v1.5.onnx -> bge-small-zh-v1.5-tokenizer.json
        let model_stem = Path::new(model_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model");
        let tokenizer_filename = format!("{}-tokenizer.json", model_stem);
        let tokenizer_path = Path::new(model_path).with_file_name(tokenizer_filename);
        let mut tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow::anyhow!(e))?;

        // 显式设置截断，防止超过模型最大长度 (通常 512)
        // 这样可以避免 "Attempting to broadcast an axis by a dimension other than 1" 错误
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 512,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(Self { session, tokenizer })
    }

    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        // 1. Tokenize
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!(e))?;
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();
        let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        // 2. Prepare inputs as ort Tensors
        let seq_len = input_ids.len();

        // Create tensors with shape [1, seq_len] using ort 2.x API
        let input_ids_tensor = Tensor::from_array(([1, seq_len], input_ids))?;
        let attention_mask_tensor = Tensor::from_array(([1, seq_len], attention_mask))?;
        let token_type_ids_tensor = Tensor::from_array(([1, seq_len], token_type_ids))?;

        // 3. Run inference
        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])?;

        // 4. Extract embeddings
        // Output usually "last_hidden_state" or "pooler_output" depending on model.
        // For BGE/BERT models, usually index 0 is last_hidden_state [1, seq_len, hidden_size]
        let output = &outputs[0];
        let (shape, data) = output.try_extract_tensor::<f32>()?;

        // shape is [1, seq_len, hidden_size]
        // Take CLS token (index 0) -> first hidden_size elements
        let hidden_size = shape[2] as usize;
        let cls_embedding: Vec<f32> = data[..hidden_size].to_vec();

        // Normalize
        let norm: f32 = cls_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized: Vec<f32> = cls_embedding.into_iter().map(|x| x / norm).collect();

        Ok(normalized)
    }
}

// 实现 knot_parser 的 EmbeddingProvider trait
use async_trait::async_trait;
use knot_parser::{EmbeddingProvider, PageIndexError};
use std::sync::Mutex;

/// 线程安全的 EmbeddingEngine 包装器
pub struct ThreadSafeEmbeddingEngine(Mutex<EmbeddingEngine>);

impl ThreadSafeEmbeddingEngine {
    pub fn new(engine: EmbeddingEngine) -> Self {
        Self(Mutex::new(engine))
    }
}

#[async_trait]
impl EmbeddingProvider for ThreadSafeEmbeddingEngine {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, PageIndexError> {
        let mut engine = self
            .0
            .lock()
            .map_err(|e| PageIndexError::EmbeddingError(format!("Lock error: {}", e)))?;

        engine
            .embed(text)
            .map_err(|e| PageIndexError::EmbeddingError(e.to_string()))
    }
}
