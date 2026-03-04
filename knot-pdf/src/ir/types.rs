//! 通用类型定义

use serde::{Deserialize, Serialize};

/// 边界框
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// 判断两个 BBox 是否有重叠
    pub fn overlaps(&self, other: &BBox) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// 计算重叠面积
    pub fn overlap_area(&self, other: &BBox) -> f32 {
        let x_overlap = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let y_overlap = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        if x_overlap > 0.0 && y_overlap > 0.0 {
            x_overlap * y_overlap
        } else {
            0.0
        }
    }

    /// 右边界
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// 下边界
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// 中心 x
    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    /// 中心 y
    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }
}

/// 页面尺寸
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// 页面来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSource {
    BornDigital,
    Ocr,
    Mixed,
}

impl Default for PageSource {
    fn default() -> Self {
        Self::BornDigital
    }
}

/// 文本块角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockRole {
    Body,
    Header,
    Footer,
    Title,
    /// 标题（H2/H3 级别，区分于 Title 即 H1）
    Heading,
    List,
    Caption,
    /// 页码
    PageNumber,
    /// 侧边栏/旁注
    Sidebar,
    /// 水印（M13 新增）
    Watermark,
    /// 脚注（M13 新增）
    Footnote,
    Unknown,
}

impl Default for BlockRole {
    fn default() -> Self {
        Self::Body
    }
}

/// 图片格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpg,
    Unknown,
}

impl Default for ImageFormat {
    fn default() -> Self {
        Self::Unknown
    }
}

/// 图片来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSource {
    /// 嵌入的位图（Image XObject）
    Embedded,
    /// 矢量图表区域渲染
    FigureRegion,
}

impl Default for ImageSource {
    fn default() -> Self {
        Self::Embedded
    }
}

/// 表格抽取模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtractionMode {
    Ruled,
    Stream,
    Unknown,
}

impl Default for ExtractionMode {
    fn default() -> Self {
        Self::Unknown
    }
}

/// 单元格类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CellType {
    Number,
    Text,
    Percent,
    Currency,
    Date,
    Unknown,
}

impl Default for CellType {
    fn default() -> Self {
        Self::Unknown
    }
}

/// 耗时统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    pub extract_ms: Option<u64>,
    pub render_ms: Option<u64>,
    pub ocr_ms: Option<u64>,
    /// 页面处理后的进程 RSS（字节）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_rss_bytes: Option<usize>,
    /// 页面处理期间的 RSS 增量（字节，正数=增长，负数=回落）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_delta_bytes: Option<i64>,
}

/// 诊断信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// 页面诊断信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageDiagnostics {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub block_count: usize,
    pub table_count: usize,
    pub image_count: usize,
    pub ocr_quality_score: Option<f32>,
    /// 使用的解析策略（M14: FastTrack / Enhanced / Full+VLM）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse_strategy: Option<String>,
}
