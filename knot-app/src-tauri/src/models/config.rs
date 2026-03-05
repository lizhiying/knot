use serde::{Deserialize, Serialize};

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
        // 使用标准库计算 UTC 偏移（秒）
        // 原理：比较本地时间和 UTC 时间的差值
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let utc_secs = now.as_secs() as i64;

        // 获取本地时间的 tm 结构
        // libc::localtime_r 可以获取本地时间的 tm_gmtoff
        #[cfg(unix)]
        {
            let mut tm: libc::tm = unsafe { std::mem::zeroed() };
            let time_t = utc_secs as libc::time_t;
            unsafe {
                libc::localtime_r(&time_t, &mut tm);
            }
            Ok(tm.tm_gmtoff as i32)
        }

        #[cfg(not(unix))]
        {
            // Windows 简单回退：假设 Global
            Ok(0)
        }
    }

    pub fn get_url(&self, filename: &str) -> String {
        let base = match self.region {
            Region::Global => "https://huggingface.co",
            Region::China => "https://hf-mirror.com",
        };

        // Hardcoded mapping logic
        // GLM-OCR-Q8_0.gguf -> ggml-org/GLM-OCR-GGUF/resolve/main/GLM-OCR-Q8_0.gguf
        // mmproj-GLM-OCR-Q8_0.gguf -> ggml-org/GLM-OCR-GGUF/resolve/main/mmproj-GLM-OCR-Q8_0.gguf
        // Qwen3.5-4B-Q4_K_M.gguf -> unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf

        let path = match filename {
            "GLM-OCR-Q8_0.gguf" => "ggml-org/GLM-OCR-GGUF/resolve/main/GLM-OCR-Q8_0.gguf",
            "mmproj-GLM-OCR-Q8_0.gguf" => {
                "ggml-org/GLM-OCR-GGUF/resolve/main/mmproj-GLM-OCR-Q8_0.gguf"
            }
            "Qwen3.5-4B-Q4_K_M.gguf" => {
                "unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"
            }
            // PaddleOCR PP-OCRv5 模型（knot-pdf OCR 依赖）
            // HF 仓库: bukuroo/PPOCRv5-ONNX，使用 mobile 版本（体积小、速度快）
            "ppocrv5/det.onnx" => "bukuroo/PPOCRv5-ONNX/resolve/main/ppocrv5-mobile-det.onnx",
            "ppocrv5/rec.onnx" => "bukuroo/PPOCRv5-ONNX/resolve/main/ppocrv5-mobile-rec.onnx",
            "ppocrv5/ppocrv5_dict.txt" => "bukuroo/PPOCRv5-ONNX/resolve/main/ppocrv5_dict.txt",
            _ => return format!("{}/unknown/{}", base, filename),
        };

        format!("{}/{}", base, path)
    }
}
