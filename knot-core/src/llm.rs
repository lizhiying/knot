use anyhow::Result;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct LlamaSidecar {
    process: Option<Child>,
    _is_running: Arc<AtomicBool>,
}

impl LlamaSidecar {
    pub fn spawn_with_mmap(
        model_path: &str,
        bin_dir: &Path,
        mmproj_path: Option<&str>,
        port: u16,
    ) -> Result<Self> {
        // 根据平台选择正确的二进制文件
        #[cfg(target_os = "macos")]
        let server_name = "llama-server-mac-metal";
        #[cfg(target_os = "windows")]
        let server_name = "llama-server.exe";
        #[cfg(target_os = "linux")]
        let server_name = "llama-server";

        let bin_path = bin_dir.join("llama").join(server_name);

        println!("[LLM] Starting server from {:?} on port {}", bin_path, port);

        let mut cmd = Command::new(&bin_path);
        cmd.arg("--model")
            .arg(model_path)
            .arg("--mmap") // Critical for lazy loading
            .arg("--port")
            .arg(port.to_string())
            .arg("--n-gpu-layers")
            .arg("99") // Try to offload all
            .arg("-c") // Limit context to avoid OOM
            .arg("4096")
            .arg("--no-warmup");

        if let Some(mmproj) = mmproj_path {
            cmd.arg("--mmproj").arg(mmproj);
        }

        let child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        Ok(Self {
            process: Some(child),
            _is_running: Arc::new(AtomicBool::new(true)),
        })
    }
}

impl Drop for LlamaSidecar {
    fn drop(&mut self) {
        println!("[LlamaSidecar] Drop called. Checking process...");
        if let Some(mut child) = self.process.take() {
            println!(
                "[LlamaSidecar] Killing child process (PID: {:?})...",
                child.id()
            );
            if let Err(e) = child.kill() {
                println!("[LlamaSidecar] Failed to kill process: {}", e);
            } else {
                println!("[LlamaSidecar] Process kill signal sent.");
                // Clean up zombie
                let _ = child.wait();
            }
        } else {
            println!("[LlamaSidecar] No process to kill.");
        }
    }
}

// Client for interacting with the running llama-server
use async_trait::async_trait;
use pageindex_rs::{LlmProvider, PageIndexError};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::{sleep, Duration};

struct PriorityGate {
    hp_waiting: AtomicUsize,
    lock: Mutex<()>,
}

impl PriorityGate {
    fn new() -> Self {
        Self {
            hp_waiting: AtomicUsize::new(0),
            lock: Mutex::new(()),
        }
    }

    async fn lock_high(&self) -> MutexGuard<'_, ()> {
        // Increment waiting count to signal Low priority tasks to yield
        self.hp_waiting.fetch_add(1, Ordering::SeqCst);
        let guard = self.lock.lock().await;
        self.hp_waiting.fetch_sub(1, Ordering::SeqCst);
        guard
    }

    async fn lock_low(&self) -> MutexGuard<'_, ()> {
        loop {
            // 1. Yield if High Priority tasks are waiting
            if self.hp_waiting.load(Ordering::SeqCst) > 0 {
                sleep(Duration::from_millis(50)).await;
                continue;
            }

            // 2. Try to acquire lock
            let guard = self.lock.lock().await;

            // 3. Double check (Cooperative Yield)
            if self.hp_waiting.load(Ordering::SeqCst) > 0 {
                drop(guard);
                sleep(Duration::from_millis(10)).await;
                continue;
            }

            return guard;
        }
    }
}

#[derive(Clone)]
pub struct LlamaClient {
    base_url: String,
    client: reqwest::Client,
    gate: Arc<PriorityGate>,
}

impl LlamaClient {
    pub fn new(port: u16) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            client: reqwest::Client::new(),
            gate: Arc::new(PriorityGate::new()),
        }
    }
}

#[async_trait]
impl LlmProvider for LlamaClient {
    async fn generate_summary(&self, text: &str) -> Result<String, PageIndexError> {
        // Low Priority
        let _guard = self.gate.lock_low().await;

        println!(
            "[LlamaClient] Generating summary for text (len: {})...",
            text.len()
        );
        let prompt = format!(
            "<|im_start|>system\nYou are a helpful summary assistant. Summarize the content concisely in the same language as the content.<|im_end|>\n<|im_start|>user\n{}\n/no_think<|im_end|>\n<|im_start|>assistant\n",
            text
        );

        let body = json!({
            "prompt": prompt,
            "n_predict": 1024,
            "temperature": 0.3,
            "stop": ["<|im_end|>"]
        });

        let res = self
            .client
            .post(format!("{}/completion", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                println!("[LlamaClient] Request failed: {}", e);
                PageIndexError::ParseError(format!("LLM Request failed: {}", e))
            })?;

        if !res.status().is_success() {
            println!("[LlamaClient] Error status: {}", res.status());
            return Err(PageIndexError::ParseError(format!(
                "LLM Error status: {}",
                res.status()
            )));
        }

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| PageIndexError::ParseError(format!("Invalid response JSON: {}", e)))?;

        let content = json["content"]
            .as_str()
            .ok_or_else(|| PageIndexError::ParseError("Missing content in response".to_string()))?;

        // 过滤掉 <think>...</think> 内容
        let cleaned_content = if let Some(end_idx) = content.find("</think>") {
            &content[end_idx + 8..]
        } else if content.starts_with("<think>") {
            &content[7..]
        } else {
            content
        };

        Ok(cleaned_content.trim().to_string())
    }

    async fn generate_content(&self, prompt: &str) -> Result<String, PageIndexError> {
        // High Priority
        let _guard = self.gate.lock_high().await;

        println!(
            "[LlamaClient] Generating content for prompt (len: {})...",
            prompt.len()
        );

        let formatted_prompt = if prompt.trim().starts_with("<|im_start|>") {
            println!("[LlamaClient] Using RAW PROMPT (ChatML detected)");
            prompt.to_string()
        } else {
            println!("[LlamaClient] Using WRAPPED PROMPT (Default)");
            format!(
                "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}\n<|im_end|>\n<|im_start|>assistant\n",
                prompt
            )
        };

        let body = json!({
            "prompt": formatted_prompt,
            "n_predict": 2048,
            "temperature": 0.7,
            "stop": ["<|im_end|>"]
        });

        let res = self
            .client
            .post(format!("{}/completion", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                println!("[LlamaClient] Request failed: {}", e);
                PageIndexError::ParseError(format!("LLM Request failed: {}", e))
            })?;

        if !res.status().is_success() {
            println!("[LlamaClient] Error status: {}", res.status());
            return Err(PageIndexError::ParseError(format!(
                "LLM Error status: {}",
                res.status()
            )));
        }

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| PageIndexError::ParseError(format!("Invalid response JSON: {}", e)))?;

        let content = json["content"]
            .as_str()
            .ok_or_else(|| PageIndexError::ParseError("Missing content in response".to_string()))?;

        // 过滤掉 <think>...</think> 内容
        let cleaned_content = if let Some(end_idx) = content.find("</think>") {
            &content[end_idx + 8..] // +8 for length of "</think>"
        } else if content.starts_with("<think>") {
            &content[7..]
        } else {
            content
        };

        Ok(cleaned_content.trim().to_string())
    }

    async fn generate_content_with_image(
        &self,
        prompt: &str,
        image_data: &[u8],
    ) -> Result<String, PageIndexError> {
        // High Priority (Image Analysis might be low? User prompt usually high?
        // Wait, VisionProvider for indexing is LOW priority?
        // Ah, `generate_content_with_image` is used by `provider.process_page` in `OfficeParser`.
        // THIS IS FOR INDEXING.
        // But user provided Chat Image?
        // Current app usage: `generate_content_with_image` is ONLY used by `VisionProvider` (indexing).
        // Chat UI does NOT support image upload yet in `LlmProvider` trait generic usage (trait has `generate_content`).
        // Wait, `LlmProvider` trait:
        // `generate_summary`, `generate_content`, `generate_content_with_image`.
        // `OfficeParser` calls `process_page`. `VisionProvider` calls `generate_content_with_image`.
        // So this is LOW PRIORITY.

        let _guard = self.gate.lock_low().await;
        // Wait, if I mark this Low, then indexing images yield. Correct.
        // User asked "RAG 生成阶段 (Chat) 最高优先级".
        // Chat uses `generate_content`.
        // Indexing uses `generate_summary` AND `generate_content_with_image`.
        // So this should be `lock_low`.

        use base64::{engine::general_purpose, Engine as _};
        let b64_image = general_purpose::STANDARD.encode(image_data);

        println!(
            "[LlamaClient] Generating content with image (len: {} bytes)...",
            image_data.len()
        );
        // ... rest of implementation identical ...
        // Re-implementing to ensure correct wrapper context.

        let body = json!({
            "messages": [
                {
                    "role": "system",
                    "content": "You are a helpful assistant."
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": prompt
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/png;base64,{}", b64_image)
                            }
                        }
                    ]
                }
            ],
            "max_tokens": 4096,
            "temperature": 0.1,
            "stream": false
        });

        let res = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                println!("[LlamaClient] Request failed: {}", e);
                PageIndexError::ParseError(format!("LLM Request failed: {}", e))
            })?;

        if !res.status().is_success() {
            let error_text = res.text().await.unwrap_or_default();
            println!("[LlamaClient] Error status: {} | Body: {}", 0, error_text);
            return Err(PageIndexError::ParseError(format!(
                "LLM Error: {}",
                error_text
            )));
        }

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| PageIndexError::ParseError(format!("Invalid response JSON: {}", e)))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| PageIndexError::ParseError("Missing content in response".to_string()))?;

        Ok(content.trim().to_string())
    }
}
