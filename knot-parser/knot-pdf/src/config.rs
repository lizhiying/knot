//! 配置结构体

use serde::{Deserialize, Serialize};

/// knot-pdf 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 文本质量评分阈值（低于此值视为扫描页）
    #[serde(default = "default_scoring_text_threshold")]
    pub scoring_text_threshold: f32,

    /// 乱码率阈值
    #[serde(default = "default_garbled_threshold")]
    pub garbled_threshold: f32,

    /// 是否剔除页眉页脚
    #[serde(default = "default_true")]
    pub strip_headers_footers: bool,

    /// 最大列数
    #[serde(default = "default_max_columns")]
    pub max_columns: usize,

    /// 是否输出 Markdown
    #[serde(default = "default_true")]
    pub emit_markdown: bool,

    /// 是否输出 IR JSON
    #[serde(default)]
    pub emit_ir_json: bool,

    /// OCR 是否启用
    #[serde(default)]
    pub ocr_enabled: bool,

    /// OCR 触发模式
    #[serde(default)]
    pub ocr_mode: OcrMode,

    /// OCR 识别语言
    #[serde(default = "default_ocr_languages")]
    pub ocr_languages: Vec<String>,

    /// OCR 渲染图片宽度
    #[serde(default = "default_ocr_render_width")]
    pub ocr_render_width: u32,

    /// OCR 并发数
    #[serde(default = "default_ocr_workers")]
    pub ocr_workers: usize,

    /// Store 是否启用
    #[serde(default)]
    pub store_enabled: bool,

    /// Store 存储路径
    #[serde(default)]
    pub store_path: Option<std::path::PathBuf>,

    /// OCR 模型目录路径（包含 det.onnx / rec.onnx / ppocrv5_dict.txt）
    #[serde(default)]
    pub ocr_model_dir: Option<std::path::PathBuf>,

    /// 内存峰值限制（MB）
    #[serde(default = "default_max_memory_mb")]
    pub max_memory_mb: usize,

    /// Pipeline 页面队列大小
    #[serde(default = "default_page_queue_size")]
    pub page_queue_size: usize,

    /// 渲染并发数
    #[serde(default = "default_render_workers")]
    pub render_workers: usize,

    /// 单页处理超时（秒，0 表示不超时）
    #[serde(default)]
    pub page_timeout_secs: u64,

    /// 是否启用图表区域检测
    #[serde(default = "default_true")]
    pub figure_detection_enabled: bool,

    /// 图表区域最小面积（占页面面积比例）
    #[serde(default = "default_figure_min_area_ratio")]
    pub figure_min_area_ratio: f32,

    /// 图表区域最小 Path objects 数量
    #[serde(default = "default_figure_min_path_count")]
    pub figure_min_path_count: usize,

    /// 图表渲染分辨率（宽度像素）
    #[serde(default = "default_figure_render_width")]
    pub figure_render_width: u32,

    /// Vision LLM API URL（OpenAI 兼容格式，如 https://api.openai.com/v1/chat/completions）
    #[serde(default)]
    pub vision_api_url: Option<String>,

    /// Vision LLM API Key（也可通过环境变量 KNOT_VISION_API_KEY 设置）
    #[serde(default)]
    pub vision_api_key: Option<String>,

    /// Vision LLM 模型名称（如 gpt-4o, claude-3-5-sonnet-20241022）
    #[serde(default = "default_vision_model")]
    pub vision_model: String,

    /// 阅读顺序算法
    #[serde(default)]
    pub reading_order_method: ReadingOrderMethod,

    /// XY-Cut 间隙比例（占页面宽度的比例，默认 0.02）
    #[serde(default = "default_xy_cut_gap_ratio")]
    pub xy_cut_gap_ratio: f32,

    // === 版面检测 (M10) ===
    /// 是否启用版面检测模型
    #[serde(default)]
    pub layout_model_enabled: bool,

    /// 版面检测模型文件路径（None 则自动搜索）
    #[serde(default)]
    pub layout_model_path: Option<std::path::PathBuf>,

    /// 版面检测置信度阈值（0.0~1.0，默认 0.5）
    #[serde(default = "default_layout_confidence")]
    pub layout_confidence_threshold: f32,

    /// 版面检测模型输入分辨率（默认 640）
    #[serde(default = "default_layout_input_size")]
    pub layout_input_size: u32,

    // === 表格结构模型 (M11) ===
    /// 是否启用表格结构识别模型
    #[serde(default)]
    pub table_model_enabled: bool,

    /// 表格结构模型文件路径（None 则自动搜索）
    #[serde(default)]
    pub table_model_path: Option<std::path::PathBuf>,

    /// 表格结构模型置信度阈值（0.0~1.0，默认 0.5）
    #[serde(default = "default_table_confidence")]
    pub table_confidence_threshold: f32,

    /// 表格结构模型输入分辨率（默认 640）
    #[serde(default = "default_table_input_size")]
    pub table_input_size: u32,

    // === 公式检测 (M12) ===
    /// 是否启用公式区域检测（默认 true）
    #[serde(default = "default_true")]
    pub formula_detection_enabled: bool,

    /// 是否启用公式 OCR 模型（需 formula_model feature）
    #[serde(default)]
    pub formula_model_enabled: bool,

    /// 公式 OCR 模型文件路径（None 则自动搜索）
    #[serde(default)]
    pub formula_model_path: Option<std::path::PathBuf>,

    /// 公式 OCR 词表文件路径
    #[serde(default)]
    pub formula_vocab_path: Option<std::path::PathBuf>,

    /// 公式 OCR 置信度阈值（0.0~1.0，默认 0.3）
    #[serde(default = "default_formula_confidence")]
    pub formula_confidence_threshold: f32,

    /// 公式 OCR 模型输入分辨率（默认 256）
    #[serde(default = "default_formula_input_size")]
    pub formula_input_size: u32,

    /// 公式渲染分辨率（宽度像素，默认 512）
    #[serde(default = "default_formula_render_width")]
    pub formula_render_width: u32,

    // === 后处理 (M13) ===
    /// 是否启用后处理 Pipeline（默认 true）
    #[serde(default = "default_true")]
    pub postprocess_enabled: bool,

    /// 是否检测并移除水印（默认 true）
    #[serde(default = "default_true")]
    pub remove_watermark: bool,

    /// 是否分离脚注（默认 false，标记但不移除）
    #[serde(default)]
    pub separate_footnotes: bool,

    /// 是否合并跨页段落（默认 true）
    #[serde(default = "default_true")]
    pub merge_cross_page_paragraphs: bool,

    /// 页码过滤（仅解析指定页面，0-indexed）
    /// 由 CLI --pages 参数设置，不通过配置文件设置
    #[serde(skip)]
    pub page_indices: Option<Vec<usize>>,
}

/// OCR 模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    Auto,
    ForceAll,
    Disabled,
}

impl Default for OcrMode {
    fn default() -> Self {
        Self::Disabled
    }
}

/// 阅读顺序算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingOrderMethod {
    /// 现有的启发式排序
    Heuristic,
    /// XY-Cut 递归分割（推荐用于多栏文档）
    XyCut,
    /// 自动选择（默认：对多栏使用 XyCut，单栏使用 Heuristic）
    Auto,
}

impl Default for ReadingOrderMethod {
    fn default() -> Self {
        Self::Auto
    }
}

fn default_scoring_text_threshold() -> f32 {
    0.3
}

fn default_garbled_threshold() -> f32 {
    0.2
}

fn default_true() -> bool {
    true
}

fn default_max_columns() -> usize {
    3
}

fn default_ocr_languages() -> Vec<String> {
    vec!["eng".to_string()]
}

fn default_ocr_render_width() -> u32 {
    512
}

fn default_ocr_workers() -> usize {
    1
}

fn default_max_memory_mb() -> usize {
    200
}

fn default_page_queue_size() -> usize {
    4
}

fn default_render_workers() -> usize {
    2
}

fn default_figure_min_area_ratio() -> f32 {
    0.05
}

fn default_figure_min_path_count() -> usize {
    10
}

fn default_figure_render_width() -> u32 {
    800
}

fn default_xy_cut_gap_ratio() -> f32 {
    0.02
}

fn default_layout_confidence() -> f32 {
    0.5
}

fn default_layout_input_size() -> u32 {
    640
}

fn default_table_confidence() -> f32 {
    0.5
}

fn default_table_input_size() -> u32 {
    640
}

fn default_vision_model() -> String {
    "gpt-4o".to_string()
}

fn default_formula_confidence() -> f32 {
    0.3
}

fn default_formula_input_size() -> u32 {
    256
}

fn default_formula_render_width() -> u32 {
    512
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scoring_text_threshold: default_scoring_text_threshold(),
            garbled_threshold: default_garbled_threshold(),
            strip_headers_footers: true,
            max_columns: default_max_columns(),
            emit_markdown: true,
            emit_ir_json: false,
            ocr_enabled: false,
            ocr_mode: OcrMode::default(),
            ocr_languages: default_ocr_languages(),
            ocr_render_width: default_ocr_render_width(),
            ocr_workers: default_ocr_workers(),
            store_enabled: false,
            store_path: None,
            ocr_model_dir: None,
            max_memory_mb: default_max_memory_mb(),
            page_queue_size: default_page_queue_size(),
            render_workers: default_render_workers(),
            page_timeout_secs: 0,
            figure_detection_enabled: true,
            figure_min_area_ratio: default_figure_min_area_ratio(),
            figure_min_path_count: default_figure_min_path_count(),
            figure_render_width: default_figure_render_width(),
            vision_api_url: None,
            vision_api_key: None,
            vision_model: default_vision_model(),
            reading_order_method: ReadingOrderMethod::default(),
            xy_cut_gap_ratio: default_xy_cut_gap_ratio(),
            layout_model_enabled: false,
            layout_model_path: None,
            layout_confidence_threshold: default_layout_confidence(),
            layout_input_size: default_layout_input_size(),
            table_model_enabled: false,
            table_model_path: None,
            table_confidence_threshold: default_table_confidence(),
            table_input_size: default_table_input_size(),
            formula_detection_enabled: true,
            formula_model_enabled: false,
            formula_model_path: None,
            formula_vocab_path: None,
            formula_confidence_threshold: default_formula_confidence(),
            formula_input_size: default_formula_input_size(),
            formula_render_width: default_formula_render_width(),
            postprocess_enabled: true,
            remove_watermark: true,
            separate_footnotes: false,
            merge_cross_page_paragraphs: true,
            page_indices: None,
        }
    }
}

impl Config {
    /// 校验配置并应用合理默认值
    pub fn validate(&mut self) {
        if self.scoring_text_threshold < 0.0 {
            self.scoring_text_threshold = 0.0;
        }
        if self.scoring_text_threshold > 1.0 {
            self.scoring_text_threshold = 1.0;
        }
        if self.max_columns == 0 {
            self.max_columns = 1;
        }
        if self.ocr_workers == 0 {
            self.ocr_workers = 1;
        }
        if self.render_workers == 0 {
            self.render_workers = 1;
        }
        if self.max_memory_mb == 0 {
            self.max_memory_mb = 200;
        }
        if self.page_queue_size == 0 {
            self.page_queue_size = 4;
        }
    }

    /// 从 TOML 文件加载配置
    ///
    /// TOML 文件中未指定的字段将使用默认值。
    ///
    /// ```no_run
    /// use knot_pdf::Config;
    /// let config = Config::from_toml_file("knot-pdf.toml").unwrap();
    /// ```
    pub fn from_toml_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            format!(
                "Failed to read config file {}: {}",
                path.as_ref().display(),
                e
            )
        })?;
        Self::from_toml_str(&content)
    }

    /// 从 TOML 字符串解析配置
    pub fn from_toml_str(toml_str: &str) -> Result<Self, String> {
        let mut config: Config =
            toml::from_str(toml_str).map_err(|e| format!("Failed to parse TOML config: {}", e))?;
        config.validate();
        Ok(config)
    }

    /// 序列化为 TOML 字符串
    pub fn to_toml_string(&self) -> Result<String, String> {
        toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config to TOML: {}", e))
    }

    /// 保存配置到 TOML 文件
    pub fn save_toml_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let content = self.to_toml_string()?;
        std::fs::write(path.as_ref(), content).map_err(|e| {
            format!(
                "Failed to write config file {}: {}",
                path.as_ref().display(),
                e
            )
        })
    }

    /// 自动搜索并加载配置文件
    ///
    /// 按以下优先级搜索 `knot-pdf.toml`：
    /// 1. 当前工作目录
    /// 2. 可执行文件同级目录
    /// 3. `~/.config/knot-pdf/`
    ///
    /// 如果未找到任何配置文件，返回默认配置。
    pub fn load_auto() -> Self {
        let candidates = [
            // 当前目录
            Some(std::path::PathBuf::from("knot-pdf.toml")),
            // 可执行文件同级
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("knot-pdf.toml"))),
            // ~/.config/knot-pdf/
            dirs_or_home().map(|d| d.join("knot-pdf").join("knot-pdf.toml")),
        ];

        for candidate in candidates.iter().flatten() {
            if candidate.exists() {
                match Self::from_toml_file(candidate) {
                    Ok(config) => {
                        log::info!("Loaded config from: {}", candidate.display());
                        return config;
                    }
                    Err(e) => {
                        log::warn!("Failed to load config from {}: {}", candidate.display(), e);
                    }
                }
            }
        }

        log::debug!("No config file found, using defaults");
        Self::default()
    }
}

/// 获取用户配置目录（跨平台）
fn dirs_or_home() -> Option<std::path::PathBuf> {
    // macOS: ~/Library/Application Support 或 ~/.config
    // Linux: ~/.config
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".config"))
        })
}
