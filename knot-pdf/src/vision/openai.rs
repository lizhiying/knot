//! OpenAI 兼容的 Vision API 实现
//!
//! 支持任何兼容 OpenAI Chat Completions API 的服务：
//! - OpenAI GPT-4o / GPT-4-vision
//! - Claude (通过 OpenAI 兼容代理)
//! - Google Gemini (通过 OpenAI 兼容代理)
//! - 本地模型 (Ollama, vLLM 等)

use super::VisionDescriber;
use crate::error::PdfError;
use base64::Engine;

/// OpenAI 兼容的 Vision API 描述器
pub struct OpenAiVisionDescriber {
    /// API endpoint URL (如 "https://api.openai.com/v1/chat/completions")
    api_url: String,
    /// API Key
    api_key: String,
    /// 模型名称 (如 "gpt-4o", "claude-3-5-sonnet-20241022")
    model: String,
    /// 系统提示词
    system_prompt: String,
    /// HTTP 客户端
    client: ureq::Agent,
}

impl OpenAiVisionDescriber {
    pub fn new(api_url: &str, api_key: &str, model: &str) -> Self {
        let client = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(60))
            .build();
        Self {
            api_url: api_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: "You are a document analysis assistant. \
                Describe the content of this image concisely in the same language as the document. \
                Focus on the key information: what type of figure it is, what it shows, \
                and any important labels, values, or relationships. \
                Keep the description under 200 words."
                .to_string(),
            client,
        }
    }

    /// 自定义系统提示词
    pub fn with_system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = prompt.to_string();
        self
    }

    /// 压缩图片：缩放到指定最大宽度并编码为 JPEG（质量 85%）
    fn compress_image(png_data: &[u8], max_width: u32) -> Result<Vec<u8>, String> {
        use image::ImageReader;
        use std::io::Cursor;

        let img = ImageReader::new(Cursor::new(png_data))
            .with_guessed_format()
            .map_err(|e| format!("format guess failed: {}", e))?
            .decode()
            .map_err(|e| format!("decode failed: {}", e))?;

        let (w, h) = (img.width(), img.height());
        if w <= max_width {
            return Err("image already small enough".to_string());
        }

        let new_w = max_width;
        let new_h = (h as f64 * max_width as f64 / w as f64) as u32;
        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);

        // 编码为 JPEG（质量 85%），比 PNG 小 5-10 倍
        let mut buf = Cursor::new(Vec::new());
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
        resized
            .write_with_encoder(encoder)
            .map_err(|e| format!("jpeg encode failed: {}", e))?;

        Ok(buf.into_inner())
    }
}

impl VisionDescriber for OpenAiVisionDescriber {
    fn describe_image(
        &self,
        image_png: &[u8],
        context_hint: Option<&str>,
    ) -> Result<String, PdfError> {
        // 记录图片大小
        let img_size_kb = image_png.len() / 1024;
        log::debug!(
            "[VisionAPI] describe_image called: image={} KB, model={}",
            img_size_kb, self.model
        );

        // 如果图片过大（> 200KB），缩放压缩为 JPEG 以避免 VLM 模型 OOM
        // 阈值较低是因为本地 Ollama 和 Qwen3 共享 GPU 内存
        let (image_bytes, mime_type): (std::borrow::Cow<[u8]>, &str) =
            if image_png.len() > 200 * 1024 {
                match Self::compress_image(image_png, 800) {
                    Ok(compressed) => {
                        log::info!(
                            "VisionDescriber: compressed image from {} KB to {} KB (JPEG)",
                            img_size_kb,
                            compressed.len() / 1024
                        );
                        (std::borrow::Cow::Owned(compressed), "image/jpeg")
                    }
                    Err(e) => {
                        log::debug!(
                            "VisionDescriber: compression failed ({}), using original",
                            e
                        );
                        (std::borrow::Cow::Borrowed(image_png), "image/png")
                    }
                }
            } else {
                // 自动检测 MIME type：JPEG 以 FF D8 开头，PNG 以 89 50 开头
                let mime = if image_png.starts_with(&[0xFF, 0xD8]) {
                    "image/jpeg"
                } else {
                    "image/png"
                };
                (std::borrow::Cow::Borrowed(image_png), mime)
            };

        // 将图片编码为 base64
        let b64 = base64::engine::general_purpose::STANDARD.encode(&*image_bytes);
        let image_url = format!("data:{};base64,{}", mime_type, b64);

        // 构建用户消息
        // 如果有自定义 hint（如中文 prompt），直接使用；否则用默认英文提示
        let user_text = if let Some(hint) = context_hint {
            hint.to_string()
        } else {
            "Please describe this figure from a PDF document.".to_string()
        };

        // 构建 OpenAI Chat Completions 请求体
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": self.system_prompt
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": user_text
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": image_url
                            }
                        }
                    ]
                }
            ],
            "max_tokens": 4096,
            "temperature": 0.2,
            // Ollama 特有参数：增大上下文窗口和输出长度
            "options": {
                "num_ctx": 8192,
                "num_predict": 4096
            }
        });

        log::debug!(
            "VisionDescriber: calling {} with model {} (image={} KB, request≈{} KB)",
            self.api_url,
            self.model,
            img_size_kb,
            b64.len() / 1024,
        );

        // 发送请求
        log::debug!(
            "[VisionAPI] Sending request to {} (model={}, image={} KB)",
            self.api_url, self.model, img_size_kb
        );
        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post(&self.api_url)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&request_body)
            .map_err(|e| {
                // 读取错误响应体获取详细信息
                let detail = match e {
                    ureq::Error::Status(code, resp) => {
                        let body = resp.into_string().unwrap_or_default();
                        log::debug!(
                            "[VisionAPI] ERROR: status {} - {}",
                            code,
                            body.chars().take(150).collect::<String>()
                        );
                        format!(
                            "{}: status code {} - {}",
                            self.api_url,
                            code,
                            body.chars().take(250).collect::<String>()
                        )
                    }
                    other => {
                        log::debug!("[VisionAPI] ERROR: {}", other);
                        format!("{}: {}", self.api_url, other)
                    }
                };
                PdfError::Backend(format!("Vision API request failed: {}", detail))
            })?;
        log::debug!(
            "[VisionAPI] Response received in {:.1}s",
            request_start.elapsed().as_secs_f64()
        );

        // 解析响应
        let resp_body: serde_json::Value = response
            .into_json()
            .map_err(|e| PdfError::Backend(format!("Vision API response parse failed: {}", e)))?;

        // 提取文字描述
        let description = resp_body["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                PdfError::Backend(format!(
                    "Vision API response missing content: {}",
                    serde_json::to_string_pretty(&resp_body).unwrap_or_default()
                ))
            })?
            .to_string();

        log::debug!("[VisionAPI] Got {} chars description", description.len());

        Ok(description)
    }
}
