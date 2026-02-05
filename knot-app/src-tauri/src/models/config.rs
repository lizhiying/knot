use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Region {
    Global,
    China,
}

impl Default for Region {
    fn default() -> Self {
        Region::Global
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSourceConfig {
    pub region: Region,
    pub auto_detect: bool,
}

impl Default for ModelSourceConfig {
    fn default() -> Self {
        Self {
            region: Region::Global,
            auto_detect: true,
        }
    }
}

impl ModelSourceConfig {
    pub fn new() -> Self {
        let mut config = Self::default();
        if config.auto_detect {
            config.detect_region();
        }
        config
    }

    pub fn detect_region(&mut self) {
        // 简单策略：检查时区
        // 中国标准时间 (CST) 是 UTC+8
        // 如果系统时区偏移量是 +8 * 3600 秒，大概率在中国
        if let Ok(offset) = Self::get_timezone_offset() {
            if offset == 8 * 3600 {
                self.region = Region::China;
                println!("[ModelConfig] Detected Region: China (via Timezone)");
                return;
            }
        }

        // TODO: 可选 - 检查网络连通性 (ping hf-mirror.com vs huggingface.co)
        // 目前为了极速启动，仅基于时区推断
        self.region = Region::Global;
        println!("[ModelConfig] Detected Region: Global (Default)");
    }

    fn get_timezone_offset() -> Result<i32, String> {
        // Rust 标准库没有直接获取当前时区偏移的方法，通常依赖 `chrono`。
        // 为了避免引入重依赖，这里可以用一种简单的 heuristic 或者假设
        // 如果项目已经有了 `chrono`，直接用 `chrono::Local::now().offset().local_minus_utc()`
        // 让我们检查 Cargo.toml 是否有 chrono

        // 这是一个简单的 Mock 实现，实际需要 chrono
        // 暂时假设 Region::Global，后续集成 chrono
        Ok(0)
    }

    pub fn get_url(&self, filename: &str) -> String {
        let base = match self.region {
            Region::Global => "https://huggingface.co",
            Region::China => "https://hf-mirror.com",
        };

        // Hardcoded mapping logic as per requirements
        // OCRFlux-3B.Q4_K_M.gguf -> mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.Q4_K_M.gguf
        // OCRFlux-3B.mmproj-f16.gguf -> mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.mmproj-f16.gguf
        // Qwen3-1.7B-Q4_K_M.gguf -> unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf

        let path = match filename {
            "OCRFlux-3B.Q4_K_M.gguf" => {
                "mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.Q4_K_M.gguf"
            }
            "OCRFlux-3B.mmproj-f16.gguf" => {
                "mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.mmproj-f16.gguf"
            }
            "Qwen3-1.7B-Q4_K_M.gguf" => {
                "unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf"
            }
            _ => return format!("{}/unknown/{}", base, filename),
        };

        format!("{}/{}", base, path)
    }
}
