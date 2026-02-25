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
        let client = ureq::Agent::new();
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
}

impl VisionDescriber for OpenAiVisionDescriber {
    fn describe_image(
        &self,
        image_png: &[u8],
        context_hint: Option<&str>,
    ) -> Result<String, PdfError> {
        // 将图片编码为 base64
        let b64 = base64::engine::general_purpose::STANDARD.encode(image_png);
        let image_url = format!("data:image/png;base64,{}", b64);

        // 构建用户消息
        let user_text = if let Some(hint) = context_hint {
            format!(
                "Please describe this figure. Context: the caption reads \"{}\".",
                hint
            )
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
                                "url": image_url,
                                "detail": "low"
                            }
                        }
                    ]
                }
            ],
            "max_tokens": 500,
            "temperature": 0.2
        });

        log::debug!(
            "VisionDescriber: calling {} with model {}",
            self.api_url,
            self.model
        );

        // 发送请求
        let response = self
            .client
            .post(&self.api_url)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&request_body)
            .map_err(|e| PdfError::Backend(format!("Vision API request failed: {}", e)))?;

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

        log::info!(
            "VisionDescriber: got {} chars description",
            description.len()
        );

        Ok(description)
    }
}
