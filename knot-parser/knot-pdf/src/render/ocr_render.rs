//! OCR 渲染模块：将 PDF 页面渲染为图片供 OCR 识别

use crate::error::PdfError;
use crate::ir::BBox;

/// OCR 渲染器 Trait
pub trait OcrRenderer {
    /// 将指定页面渲染为图片字节数据 (可选格式如 PNG)
    fn render_page_to_image(
        &self,
        page_index: usize,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError>;

    /// 将指定页面的指定区域裁剪渲染为图片字节数据 (PNG)
    ///
    /// 默认实现：渲染整页后裁剪指定区域
    fn render_region_to_image(
        &self,
        page_index: usize,
        bbox: BBox,
        page_width: f32,
        page_height: f32,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        let _ = (page_index, bbox, page_width, page_height, render_width);
        Err(PdfError::Backend(
            "render_region_to_image not implemented".to_string(),
        ))
    }

    /// 设置当前处理的 PDF 文件路径（供需要打开 PDF 的渲染器使用）
    fn set_pdf_path(&self, _path: &std::path::Path) {
        // 默认空实现
    }
}

/// 简单的 Mock 渲染器，用于核心逻辑跑通
pub struct MockOcrRenderer;

impl OcrRenderer for MockOcrRenderer {
    fn render_page_to_image(
        &self,
        _page_index: usize,
        _render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        // 返回一个空的或伪造的图片数据
        Ok(vec![0; 100])
    }
}
