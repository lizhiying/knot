//! PDF 后端 trait 定义

use serde::{Deserialize, Serialize};

use crate::error::PdfError;
use crate::ir::{BBox, PageSize};

/// 从 PDF 中提取的原始字符信息
#[derive(Debug, Clone)]
pub struct RawChar {
    pub unicode: char,
    pub bbox: BBox,
    pub font_size: f32,
    pub font_name: Option<String>,
    pub is_bold: bool,
}

/// 从 PDF 中提取的原始图片信息
#[derive(Debug, Clone)]
pub struct RawImage {
    pub bbox: BBox,
    pub data: Option<Vec<u8>>,
    pub format_hint: Option<String>,
}

/// PDF 页面基本信息
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub page_index: usize,
    pub size: PageSize,
    pub rotation: f32,
}

/// 二维点
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// 线段方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineOrientation {
    Horizontal,
    Vertical,
    Diagonal,
}

/// 从 PDF 中提取的原始线段信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLine {
    /// 起点
    pub start: Point,
    /// 终点
    pub end: Point,
    /// 线宽
    pub width: f32,
    /// 方向
    pub orientation: LineOrientation,
}

/// 从 PDF 中提取的原始矩形信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawRect {
    /// 边界框
    pub bbox: BBox,
    /// 线宽
    pub width: f32,
}

/// Path object 类型（用于图表区域检测）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathObjectKind {
    Line,
    Rect,
    Curve,
    Fill,
}

/// 从 PDF 中提取的原始 Path object 信息（用于图表区域检测）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPathObject {
    /// 边界框
    pub bbox: BBox,
    /// Path 类型
    pub kind: PathObjectKind,
}

/// PDF 后端 trait —— 可插拔的 PDF 解析后端
pub trait PdfBackend {
    /// 打开 PDF 文件，返回页数
    fn open(&mut self, path: &std::path::Path) -> Result<usize, PdfError>;

    /// 获取页面基本信息
    fn page_info(&self, page_index: usize) -> Result<PageInfo, PdfError>;

    /// 提取页面中的字符（带 bbox / 字体信息）
    fn extract_chars(&self, page_index: usize) -> Result<Vec<RawChar>, PdfError>;

    /// 提取页面中的图片
    fn extract_images(&self, page_index: usize) -> Result<Vec<RawImage>, PdfError>;

    /// 获取文档元数据
    fn metadata(&self) -> Result<crate::ir::DocumentMetadata, PdfError>;

    /// 获取文档大纲
    fn outline(&self) -> Result<Vec<crate::ir::OutlineItem>, PdfError>;

    /// 提取页面中的线段（用于 ruled 表格检测）
    fn extract_lines(&self, page_index: usize) -> Result<Vec<RawLine>, PdfError> {
        let _ = page_index;
        Ok(Vec::new())
    }

    /// 提取页面中的矩形（用于 ruled 表格检测）
    fn extract_rects(&self, page_index: usize) -> Result<Vec<RawRect>, PdfError> {
        let _ = page_index;
        Ok(Vec::new())
    }

    /// 提取页面中的 Path objects（用于图表区域检测）
    fn extract_path_objects(&self, page_index: usize) -> Result<Vec<RawPathObject>, PdfError> {
        let _ = page_index;
        Ok(Vec::new())
    }

    /// 将指定页面渲染为 PNG 图片（用于图表 OCR）
    fn render_page_to_image(
        &self,
        page_index: usize,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        let _ = (page_index, render_width);
        Err(PdfError::Backend(
            "render_page_to_image not supported by this backend".to_string(),
        ))
    }
}
