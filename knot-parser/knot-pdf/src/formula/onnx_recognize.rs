//! ONNX 公式识别器
//!
//! M12 Phase B：基于 ort (ONNX Runtime) 的公式 OCR → LaTeX 推理
//!
//! 支持 TrOCR 架构（VisionEncoderDecoderModel）：
//! - encoder_model.onnx: DeiT encoder (image → hidden states)
//! - decoder_model.onnx: TrOCR decoder (hidden states + tokens → logits)
//! - tokenizer.json: BPE 词表

use crate::error::PdfError;
use crate::formula::recognize::{FormulaRecognition, FormulaRecognizer};

use ort::value::{DynTensorValueType, Tensor};
use std::sync::Mutex;

/// TrOCR Encoder-Decoder 公式识别器 (基于 ort/ONNX Runtime)
pub struct OnnxFormulaRecognizer {
    encoder: Mutex<ort::session::Session>,
    decoder: Mutex<ort::session::Session>,
    input_size: u32,
    id2token: Vec<String>,
    decoder_start_id: i64,
    eos_id: i64,
    max_length: usize,
    confidence_threshold: f32,
}

impl OnnxFormulaRecognizer {
    /// 从目录加载
    pub fn from_dir(
        model_dir: &std::path::Path,
        confidence_threshold: f32,
    ) -> Result<Self, PdfError> {
        let config_path = model_dir.join("config.json");
        let tokenizer_path = model_dir.join("tokenizer.json");
        let encoder_path = model_dir.join("encoder_model.onnx");
        let decoder_path = model_dir.join("decoder_model.onnx");

        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| PdfError::Backend(format!("Failed to read config.json: {}", e)))?;
        let config: serde_json::Value = serde_json::from_str(&config_str)
            .map_err(|e| PdfError::Backend(format!("Failed to parse config.json: {}", e)))?;

        let decoder_start_id = config["decoder_start_token_id"].as_i64().unwrap_or(2);
        let eos_id = config["eos_token_id"].as_i64().unwrap_or(2);
        let max_length = config["max_length"].as_u64().unwrap_or(256) as usize;

        // image size
        let preproc_path = model_dir.join("preprocessor_config.json");
        let input_size = if preproc_path.exists() {
            let s = std::fs::read_to_string(&preproc_path).unwrap_or_default();
            let p: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            p["size"]["height"].as_u64().unwrap_or(384) as u32
        } else {
            384
        };

        // tokenizer
        let tok_str = std::fs::read_to_string(&tokenizer_path)
            .map_err(|e| PdfError::Backend(format!("Failed to read tokenizer: {}", e)))?;
        let tok: serde_json::Value = serde_json::from_str(&tok_str)
            .map_err(|e| PdfError::Backend(format!("Failed to parse tokenizer: {}", e)))?;

        let vocab = tok["model"]["vocab"]
            .as_object()
            .ok_or_else(|| PdfError::Backend("Missing vocab".to_string()))?;

        let vocab_size = config["decoder"]["vocab_size"]
            .as_u64()
            .unwrap_or(vocab.len() as u64 + 10) as usize;

        let mut id2token = vec!["<unk>".to_string(); vocab_size];
        for (token, id_val) in vocab {
            if let Some(id) = id_val.as_u64() {
                if (id as usize) < id2token.len() {
                    id2token[id as usize] = token.clone();
                }
            }
        }
        if let Some(added) = tok["added_tokens"].as_array() {
            for t in added {
                if let (Some(c), Some(id)) = (t["content"].as_str(), t["id"].as_u64()) {
                    if (id as usize) < id2token.len() {
                        id2token[id as usize] = c.to_string();
                    }
                }
            }
        }

        log::info!(
            "Loading formula models from {:?} ({}x{}, vocab={})",
            model_dir,
            input_size,
            input_size,
            vocab_size
        );

        let encoder = ort::session::Session::builder()
            .map_err(|e| PdfError::Backend(format!("Encoder builder: {}", e)))?
            .with_intra_threads(1)
            .map_err(|e| PdfError::Backend(format!("Encoder threads: {}", e)))?
            .commit_from_file(&encoder_path)
            .map_err(|e| PdfError::Backend(format!("Load encoder: {}", e)))?;

        let decoder = ort::session::Session::builder()
            .map_err(|e| PdfError::Backend(format!("Decoder builder: {}", e)))?
            .with_intra_threads(1)
            .map_err(|e| PdfError::Backend(format!("Decoder threads: {}", e)))?
            .commit_from_file(&decoder_path)
            .map_err(|e| PdfError::Backend(format!("Load decoder: {}", e)))?;

        log::info!(
            "Models loaded: start={}, eos={}, max_len={}",
            decoder_start_id,
            eos_id,
            max_length
        );

        Ok(Self {
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            input_size,
            id2token,
            decoder_start_id,
            eos_id,
            max_length,
            confidence_threshold,
        })
    }

    /// 图片预处理: PNG → RGB → resize → normalize → NCHW [1,3,H,W]
    fn preprocess_image(&self, image_bytes: &[u8]) -> Result<Vec<f32>, PdfError> {
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| PdfError::Backend(format!("Decode image: {}", e)))?;
        let rgb = img.to_rgb8();
        let resized = image::imageops::resize(
            &rgb,
            self.input_size,
            self.input_size,
            image::imageops::FilterType::Lanczos3,
        );
        let h = self.input_size as usize;
        let w = self.input_size as usize;
        let mut data = vec![0f32; 3 * h * w];
        for y in 0..h {
            for x in 0..w {
                let pixel = resized.get_pixel(x as u32, y as u32);
                for c in 0..3 {
                    data[c * h * w + y * w + x] = (pixel[c] as f32 / 255.0 - 0.5) / 0.5;
                }
            }
        }
        Ok(data)
    }

    fn decode_tokens(&self, tokens: &[i64]) -> String {
        let mut result = String::new();
        for &tid in tokens {
            let idx = tid as usize;
            if idx < self.id2token.len() {
                let token = &self.id2token[idx];
                if matches!(
                    token.as_str(),
                    "<s>" | "</s>" | "<pad>" | "<unk>" | "[CLS]" | "[SEP]" | "[PAD]"
                ) {
                    continue;
                }
                result.push_str(token);
            }
        }
        result.replace('Ġ', " ").trim().to_string()
    }
}

impl FormulaRecognizer for OnnxFormulaRecognizer {
    fn recognize(&self, image_bytes: &[u8]) -> Result<FormulaRecognition, PdfError> {
        let h = self.input_size as usize;
        let w = self.input_size as usize;

        // 1. 预处理
        let pixel_data = self.preprocess_image(image_bytes)?;
        let pixel_values = Tensor::from_array(([1usize, 3, h, w], pixel_data.into_boxed_slice()))
            .map_err(|e| PdfError::Backend(format!("Create pixel tensor: {}", e)))?;

        // 2. Encoder — 在锁内完成 run 和数据提取
        let (enc_shape, enc_data) = {
            let mut guard = self
                .encoder
                .lock()
                .map_err(|_| PdfError::Backend("Encoder lock poisoned".to_string()))?;
            let mut outputs = guard
                .run([pixel_values.into()])
                .map_err(|e| PdfError::Backend(format!("Encoder run: {}", e)))?;
            let val = outputs
                .remove("last_hidden_state")
                .or_else(|| outputs.into_iter().next().map(|(_, v)| v))
                .ok_or_else(|| PdfError::Backend("No encoder output".to_string()))?;
            let tensor = val
                .downcast::<DynTensorValueType>()
                .map_err(|_| PdfError::Backend("Encoder output not a tensor".to_string()))?;
            let (shape_ref, data_ref) = tensor
                .try_extract_tensor::<f32>()
                .map_err(|e| PdfError::Backend(format!("Extract encoder: {}", e)))?;
            let shape: Vec<usize> = shape_ref.iter().map(|&d| d as usize).collect();
            let data: Vec<f32> = data_ref.to_vec();
            (shape, data)
        };

        // 3. Decoder 自回归
        let mut generated: Vec<i64> = vec![self.decoder_start_id];
        let mut total_conf = 0.0f32;

        for step in 0..self.max_length.min(128) {
            let seq_len = generated.len();

            let input_ids =
                Tensor::from_array(([1usize, seq_len], generated.clone().into_boxed_slice()))
                    .map_err(|e| PdfError::Backend(format!("Create input_ids: {}", e)))?;

            let enc_hs =
                Tensor::from_array((enc_shape.clone(), enc_data.clone().into_boxed_slice()))
                    .map_err(|e| PdfError::Backend(format!("Re-create enc hidden: {}", e)))?;

            // Decoder — 在锁内完成 run 和 logits 提取
            let (max_idx, max_val) = {
                let mut guard = self
                    .decoder
                    .lock()
                    .map_err(|_| PdfError::Backend("Decoder lock poisoned".to_string()))?;
                let mut outputs = guard
                    .run([input_ids.into(), enc_hs.into()])
                    .map_err(|e| PdfError::Backend(format!("Decoder step {}: {}", step, e)))?;
                let val = outputs
                    .remove("logits")
                    .or_else(|| outputs.into_iter().next().map(|(_, v)| v))
                    .ok_or_else(|| PdfError::Backend("No decoder output".to_string()))?;
                let tensor = val
                    .downcast::<DynTensorValueType>()
                    .map_err(|_| PdfError::Backend("Logits not a tensor".to_string()))?;
                let (shape, data) = tensor
                    .try_extract_tensor::<f32>()
                    .map_err(|e| PdfError::Backend(format!("Read logits: {}", e)))?;

                let vocab_size = shape[2] as usize;
                let last_pos = shape[1] as usize - 1;
                let offset = last_pos * vocab_size;
                let last_logits = &data[offset..offset + vocab_size];

                last_logits
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                    .map(|(i, &v)| (i as i64, v))
                    .unwrap_or((0, 0.0))
            };

            total_conf += 1.0 / (1.0 + (-max_val).exp());

            if max_idx == self.eos_id {
                break;
            }
            generated.push(max_idx);
        }

        let num_gen = generated.len() - 1;
        let avg_conf = if num_gen > 0 {
            total_conf / num_gen as f32
        } else {
            0.0
        };
        let latex = self.decode_tokens(&generated[1..]);

        log::debug!(
            "Formula OCR: {} tokens, conf={:.3}, latex='{}'",
            num_gen,
            avg_conf,
            latex
        );

        Ok(FormulaRecognition {
            latex,
            confidence: avg_conf,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_logic() {
        let id2token = vec![
            "<pad>".into(),
            "<s>".into(),
            "</s>".into(),
            "\\".into(),
            "int".into(),
            "Ġ(".into(),
            "x".into(),
            ")".into(),
        ];
        let mut result = String::new();
        for &tid in &[3i64, 4, 5, 6, 7] {
            let idx = tid as usize;
            if idx < id2token.len() {
                let t: &String = &id2token[idx];
                if matches!(t.as_str(), "<s>" | "</s>" | "<pad>" | "<unk>") {
                    continue;
                }
                result.push_str(t);
            }
        }
        assert_eq!(result.replace('Ġ', " ").trim(), "\\int (x)");
    }

    #[test]
    fn test_model_dir_not_found() {
        assert!(
            OnnxFormulaRecognizer::from_dir(std::path::Path::new("/nonexistent"), 0.3).is_err()
        );
    }

    #[test]
    fn test_load_real_model() {
        let dir = std::path::Path::new("models/formula");
        if !dir.join("encoder_model.onnx").exists() {
            eprintln!("Skip: no model");
            return;
        }
        OnnxFormulaRecognizer::from_dir(dir, 0.3).expect("Load failed");
        println!("✓ Model loaded via ort");
    }

    #[test]
    fn test_inference_real_model() {
        let dir = std::path::Path::new("models/formula");
        if !dir.join("encoder_model.onnx").exists() {
            eprintln!("Skip: no model");
            return;
        }
        let rec = OnnxFormulaRecognizer::from_dir(dir, 0.3).unwrap();

        let mut png = Vec::new();
        {
            use image::ImageEncoder;
            let enc = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png));
            enc.write_image(
                &vec![255u8; 32 * 32 * 3],
                32,
                32,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        }

        let t = std::time::Instant::now();
        let r = rec.recognize(&png).expect("Inference failed");
        println!(
            "✓ {}ms, '{}', conf={:.3}",
            t.elapsed().as_millis(),
            r.latex,
            r.confidence
        );
    }
}
