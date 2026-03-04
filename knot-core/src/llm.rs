use anyhow::Result;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Helper function to check if we should print debug output
fn is_quiet() -> bool {
    std::env::var("KNOT_QUIET").is_ok()
}

/// Macro for debug prints that respect KNOT_QUIET environment variable
macro_rules! debug_println {
    ($($arg:tt)*) => {
        if !is_quiet() {
            println!($($arg)*);
        }
    };
}

pub struct LlamaSidecar {
    process: Option<Child>,
    _is_running: Arc<AtomicBool>,
    quiet: bool,
}

impl LlamaSidecar {
    pub fn spawn_with_mmap(
        model_path: &str,
        bin_dir: &Path,
        mmproj_path: Option<&str>,
        port: u16,
    ) -> Result<Self> {
        Self::spawn_internal(model_path, bin_dir, mmproj_path, port, false, None)
    }

    /// Spawn with custom context size
    pub fn spawn_with_context(
        model_path: &str,
        bin_dir: &Path,
        mmproj_path: Option<&str>,
        port: u16,
        context_size: u32,
    ) -> Result<Self> {
        Self::spawn_internal(
            model_path,
            bin_dir,
            mmproj_path,
            port,
            false,
            Some(context_size),
        )
    }

    /// Spawn without any console output (for CLI use)
    pub fn spawn_quiet(
        model_path: &str,
        bin_dir: &Path,
        mmproj_path: Option<&str>,
        port: u16,
    ) -> Result<Self> {
        Self::spawn_internal(model_path, bin_dir, mmproj_path, port, true, None)
    }

    fn spawn_internal(
        model_path: &str,
        bin_dir: &Path,
        mmproj_path: Option<&str>,
        port: u16,
        quiet: bool,
        context_size: Option<u32>,
    ) -> Result<Self> {
        // 根据平台选择正确的二进制文件
        #[cfg(target_os = "macos")]
        let bin_path = {
            // macOS: 优先用 Homebrew 安装的版本（支持最新模型架构如 GLM-OCR）
            let homebrew_path = std::path::PathBuf::from("/opt/homebrew/bin/llama-server");
            if homebrew_path.exists() {
                if !quiet {
                    println!("[LLM] Using Homebrew llama-server: {:?}", homebrew_path);
                }
                homebrew_path
            } else {
                bin_dir.join("llama").join("llama-server-mac-metal")
            }
        };
        #[cfg(target_os = "windows")]
        let bin_path = bin_dir.join("llama").join("llama-server.exe");
        #[cfg(target_os = "linux")]
        let bin_path = bin_dir.join("llama").join("llama-server");

        // CLEANUP: Check for zombie processes on the target port
        cleanup_process_on_port(port);

        if !quiet {
            println!("[LLM] Starting server from {:?} on port {}", bin_path, port);
        }

        let mut cmd = Command::new(&bin_path);
        let ctx_size = context_size.unwrap_or(8192);
        cmd.arg("--model")
            .arg(model_path)
            .arg("--mmap")
            .arg("--port")
            .arg(port.to_string())
            .arg("--n-gpu-layers")
            .arg("99")
            .arg("-c")
            .arg(ctx_size.to_string())
            .arg("--parallel")
            .arg("2")
            .arg("-fa")
            .arg("on");

        if let Some(mmproj) = mmproj_path {
            cmd.arg("--mmproj").arg(mmproj);
        }

        // In quiet mode, suppress stderr output
        let child = if quiet {
            cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?
        } else {
            cmd.stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?
        };

        Ok(Self {
            process: Some(child),
            _is_running: Arc::new(AtomicBool::new(true)),
            quiet,
        })
    }

    /// Convenience method: spawn with default settings (no mmproj, with output)
    pub fn spawn(bin_dir: &Path, model_path: &str, port: u16) -> Result<Self> {
        Self::spawn_with_mmap(model_path, bin_dir, None, port)
    }

    /// Get the PID of the running process
    pub fn get_pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.id())
    }
}

impl Drop for LlamaSidecar {
    fn drop(&mut self) {
        if !self.quiet {
            println!("[LlamaSidecar] Drop called. Checking process...");
        }
        if let Some(mut child) = self.process.take() {
            if !self.quiet {
                println!(
                    "[LlamaSidecar] Killing child process (PID: {:?})...",
                    child.id()
                );
            }
            if let Err(e) = child.kill() {
                if !self.quiet {
                    println!("[LlamaSidecar] Failed to kill process: {}", e);
                }
            } else {
                if !self.quiet {
                    println!("[LlamaSidecar] Process kill signal sent.");
                }
                // Clean up zombie
                let _ = child.wait();
            }
        } else {
            if !self.quiet {
                println!("[LlamaSidecar] No process to kill.");
            }
        }
    }
}

// Client for interacting with the running llama-server
use async_trait::async_trait;
use knot_parser::{LlmProvider, PageIndexError};
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

    /// 流式生成内容
    /// 返回一个 Receiver，接收生成的 token
    pub async fn generate_content_stream(
        &self,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, PageIndexError> {
        let _guard = self.gate.lock_high().await;

        let formatted_prompt = if prompt.contains("<|im_start|>") {
            prompt.to_string()
        } else {
            format!(
                "<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                prompt
            )
        };

        let body = json!({
            "prompt": formatted_prompt,
            "stream": true,
            "n_predict": max_tokens,
            "temperature": 0.1,
            "stop": ["<|im_end|>"]
        });

        let client = self.client.clone();
        let url = format!("{}/completion", self.base_url);
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            debug_println!("[LlamaClient] Stream task spawned. URL: {}", url);
            let mut attempts = 0;
            let max_attempts = 60; // Wait up to 60s for model load

            loop {
                debug_println!(
                    "[LlamaClient] Stream request attempt {}/{}",
                    attempts + 1,
                    max_attempts
                );
                let res = client.post(&url).json(&body).send().await;

                match res {
                    Ok(response) => {
                        if response.status().is_success() {
                            use futures_util::StreamExt;
                            let mut stream = response.bytes_stream();
                            debug_println!("[LlamaClient] Response stream acquired. Starting to read chunks...");

                            while let Some(item) = stream.next().await {
                                match item {
                                    Ok(chunk) => {
                                        let _chunk_len = chunk.len();
                                        // debug_println!("[LlamaClient] Received chunk of size: {}", chunk_len);

                                        let chunk_str = String::from_utf8_lossy(&chunk);
                                        // Server-Sent Events format: "data: {...}\n\n"
                                        for line in chunk_str.lines() {
                                            if line.starts_with("data: ") {
                                                // debug_println!("[LlamaClient] Parsing data line: {:.50}...", line);
                                                let json_str = &line[6..];
                                                if let Ok(json) =
                                                    serde_json::from_str::<serde_json::Value>(
                                                        json_str,
                                                    )
                                                {
                                                    if let Some(content) = json["content"].as_str()
                                                    {
                                                        if !content.is_empty() {
                                                            // debug_println!("[LlamaClient] Sending content: {:?}", content);
                                                            if tx
                                                                .send(content.to_string())
                                                                .await
                                                                .is_err()
                                                            {
                                                                debug_println!("[LlamaClient] Receiver dropped.");
                                                                break; // Receiver dropped
                                                            }
                                                        }
                                                    }
                                                    // Check for stop
                                                    if let Some(stop) = json["stop"].as_bool() {
                                                        if stop {
                                                            debug_println!("[LlamaClient] Stop token received.");
                                                            break;
                                                        }
                                                    }
                                                } else {
                                                    debug_println!(
                                                        "[LlamaClient] Non-content JSON frame (stop signal or metadata)"
                                                    );
                                                }
                                            } else if !line.trim().is_empty() {
                                                // debug_println!("[LlamaClient] Ignored non-data line: {}", line);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        debug_println!("[LlamaClient] Stream Error: {}", e);
                                        let _ = tx.send(format!("Stream Error: {}", e)).await;
                                        break;
                                    }
                                }
                            }
                            debug_println!("[LlamaClient] Stream ended successfully.");
                            return; // Success, exit task
                        } else if response.status().as_u16() == 503 {
                            attempts += 1;
                            if attempts >= max_attempts {
                                debug_println!("[LlamaClient] Error 503 Timeout.");
                                let _ = tx
                                    .send(format!("Error: {} (Timeout)", response.status()))
                                    .await;
                                return;
                            }
                            println!(
                                "[LlamaClient] 503 Service Unavailable, retrying... ({}/{})",
                                attempts, max_attempts
                            );
                            sleep(Duration::from_secs(1)).await;
                            continue;
                        } else {
                            // 读取错误响应体以便调试
                            let status = response.status();
                            let error_body = response.text().await.unwrap_or_default();
                            println!(
                                "[LlamaClient] Error status: {} | Body: {}",
                                status, error_body
                            );
                            let _ = tx.send(format!("Error: {} - {}", status, error_body)).await;
                            return;
                        }
                    }
                    Err(e) => {
                        // Network error (e.g. Connection Refused), retry
                        attempts += 1;
                        if attempts >= max_attempts {
                            debug_println!(
                                "[LlamaClient] Max attempts reached. Network error: {}",
                                e
                            );
                            let _ = tx.send(format!("Request Error: {}", e)).await;
                            return;
                        }
                        println!(
                            "[LlamaClient] Network error: {}. Retrying... ({}/{})",
                            e, attempts, max_attempts
                        );
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }
        });

        Ok(rx)
    }
}

fn cleanup_process_on_port(port: u16) {
    #[cfg(unix)]
    {
        use std::process::Command;
        // lsof -t -i:PORT
        // -t: terse output (PID only)
        // -i:PORT: select internet files on PORT
        let output = Command::new("lsof")
            .arg("-t")
            .arg(format!("-i:{}", port))
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<i32>() {
                        println!(
                            "[LLM] Found process occupying port {}: PID {}. Killing...",
                            port, pid
                        );
                        // kill -9 PID
                        let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
                    }
                }
            }
            Err(e) => {
                // lsof might not be installed or permission error.
                // We just log warning, don't crash.
                println!("[LLM] Warning: Failed to check for zombie processes: {}", e);
            }
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

        let mut attempts = 0;
        let max_attempts = 60;

        loop {
            let res = self
                .client
                .post(format!("{}/completion", self.base_url))
                .json(&body)
                .send()
                .await;

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        let json: serde_json::Value = response.json().await.map_err(|e| {
                            PageIndexError::ParseError(format!("Invalid response JSON: {}", e))
                        })?;

                        let content = json["content"].as_str().ok_or_else(|| {
                            PageIndexError::ParseError("Missing content in response".to_string())
                        })?;

                        // 过滤掉 <think>...</think> 内容
                        let cleaned_content = if let Some(end_idx) = content.find("</think>") {
                            &content[end_idx + 8..]
                        } else if content.starts_with("<think>") {
                            &content[7..]
                        } else {
                            content
                        };
                        return Ok(cleaned_content.trim().to_string());
                    } else if response.status().as_u16() == 503 {
                        attempts += 1;
                        if attempts >= max_attempts {
                            debug_println!("[LlamaClient] Error status: {}", response.status());
                            return Err(PageIndexError::ParseError(format!(
                                "LLM Error status: {} (Timeout)",
                                response.status()
                            )));
                        }
                        println!(
                            "[LlamaClient] Model loading (503), retrying {}/{}...",
                            attempts, max_attempts
                        );
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    } else {
                        debug_println!("[LlamaClient] Error status: {}", response.status());
                        return Err(PageIndexError::ParseError(format!(
                            "LLM Error status: {}",
                            response.status()
                        )));
                    }
                }
                Err(e) => {
                    debug_println!("[LlamaClient] Request failed: {}", e);
                    return Err(PageIndexError::ParseError(format!(
                        "LLM Request failed: {}",
                        e
                    )));
                }
            }
        }
    }

    /// 核心接口：输入 Prompt，输出内容 (Synchronous wrapper, or keep independent)
    /// High Priority
    async fn generate_content(&self, prompt: &str) -> Result<String, PageIndexError> {
        // High Priority
        let _guard = self.gate.lock_high().await;

        println!(
            "[LlamaClient] Generating content for prompt (len: {})...",
            prompt.len()
        );

        let formatted_prompt = if prompt.trim().starts_with("<|im_start|>") {
            debug_println!("[LlamaClient] Using RAW PROMPT (ChatML detected)");
            prompt.to_string()
        } else {
            debug_println!("[LlamaClient] Using WRAPPED PROMPT (Default)");
            format!(
                "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}\n<|im_end|>\n<|im_start|>assistant\n",
                prompt
            )
        };

        let body = json!({
            "prompt": formatted_prompt,
            "n_predict": 1024,
            "temperature": 0.1,
            "stop": ["<|im_end|>"]
        });

        let mut attempts = 0;
        let max_attempts = 60;

        loop {
            let res = self
                .client
                .post(format!("{}/completion", self.base_url))
                .json(&body)
                .send()
                .await;

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        let json: serde_json::Value = response.json().await.map_err(|e| {
                            PageIndexError::ParseError(format!("Invalid response JSON: {}", e))
                        })?;

                        let content = json["content"].as_str().ok_or_else(|| {
                            PageIndexError::ParseError("Missing content in response".to_string())
                        })?;

                        // 过滤掉 <think>...</think> 内容
                        let cleaned_content = if let Some(end_idx) = content.find("</think>") {
                            &content[end_idx + 8..] // +8 for length of "</think>"
                        } else if content.starts_with("<think>") {
                            &content[7..]
                        } else {
                            content
                        };

                        return Ok(cleaned_content.trim().to_string());
                    } else if response.status().as_u16() == 503 {
                        attempts += 1;
                        if attempts >= max_attempts {
                            debug_println!("[LlamaClient] Error status: {}", response.status());
                            return Err(PageIndexError::ParseError(format!(
                                "LLM Error status: {} (Timeout)",
                                response.status()
                            )));
                        }
                        println!(
                            "[LlamaClient] Model loading (503), retrying {}/{}...",
                            attempts, max_attempts
                        );
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    } else {
                        debug_println!("[LlamaClient] Error status: {}", response.status());
                        return Err(PageIndexError::ParseError(format!(
                            "LLM Error status: {}",
                            response.status()
                        )));
                    }
                }
                Err(e) => {
                    debug_println!("[LlamaClient] Request failed: {}", e);
                    return Err(PageIndexError::ParseError(format!(
                        "LLM Request failed: {}",
                        e
                    )));
                }
            }
        }
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

        let mut attempts = 0;
        let max_attempts = 60; // Wait up to 60s for model load

        loop {
            let res = self
                .client
                .post(format!("{}/v1/chat/completions", self.base_url))
                .json(&body)
                .send()
                .await;

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        let json: serde_json::Value = response.json().await.map_err(|e| {
                            PageIndexError::ParseError(format!("Invalid response JSON: {}", e))
                        })?;

                        let content = json["choices"][0]["message"]["content"]
                            .as_str()
                            .ok_or_else(|| {
                                PageIndexError::ParseError(
                                    "Missing content in response".to_string(),
                                )
                            })?;

                        return Ok(content.trim().to_string());
                    } else if response.status().as_u16() == 503 {
                        // Model loading, retry
                        attempts += 1;
                        if attempts >= max_attempts {
                            let error_text = response.text().await.unwrap_or_default();
                            debug_println!(
                                "[LlamaClient] Error status: 503 | Body: {}",
                                error_text
                            );
                            return Err(PageIndexError::ParseError(format!(
                                "LLM Error: Service Unavailable (Model Loading Timeout): {}",
                                error_text
                            )));
                        }
                        println!(
                            "[LlamaClient] Model loading (503), retrying {}/{}...",
                            attempts, max_attempts
                        );
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    } else {
                        let status = response.status();
                        let error_text = response.text().await.unwrap_or_default();
                        println!(
                            "[LlamaClient] Error status: {} | Body: {}",
                            status, error_text
                        );
                        return Err(PageIndexError::ParseError(format!(
                            "LLM Error: {}",
                            error_text
                        )));
                    }
                }
                Err(e) => {
                    debug_println!("[LlamaClient] Request failed: {}", e);
                    return Err(PageIndexError::ParseError(format!(
                        "LLM Request failed: {}",
                        e
                    )));
                }
            }
        }
    }
}

impl LlamaClient {
    /// 发送一个预热请求，触发模型加载
    pub async fn warmup(&self) -> Result<(), PageIndexError> {
        let _guard = self.gate.lock_low().await;
        debug_println!("[LlamaClient] Sending warmup request...");

        // Empty prompt or very short prompt
        let body = json!({
            "prompt": "<|im_start|>system\nWarmup.<|im_end|>\n",
            "n_predict": 1,
            "temperature": 0.0
        });

        let mut attempts = 0;
        let max_attempts = 5; // Warmup doesn't need to try too hard if server is dead

        loop {
            let res = self
                .client
                .post(format!("{}/completion", self.base_url))
                .json(&body)
                .send()
                .await;

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        debug_println!("[LlamaClient] Warmup successful.");
                        return Ok(());
                    } else if response.status().as_u16() == 503 {
                        attempts += 1;
                        if attempts >= max_attempts {
                            debug_println!("[LlamaClient] Warmup failed: 503 Timeout.");
                            return Err(PageIndexError::ParseError("Warmup 503".into()));
                        }
                        println!(
                            "[LlamaClient] Warmup 503, retrying ({}/{})",
                            attempts, max_attempts
                        );
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    } else {
                        debug_println!("[LlamaClient] Warmup failed status: {}", response.status());
                        return Err(PageIndexError::ParseError(format!(
                            "Warmup Error: {}",
                            response.status()
                        )));
                    }
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        debug_println!("[LlamaClient] Warmup network error: {}", e);
                        return Err(PageIndexError::ParseError(format!("Warmup Error: {}", e)));
                    }
                    debug_println!("[LlamaClient] Warmup network error: {}, retrying...", e);
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }
    }
}
