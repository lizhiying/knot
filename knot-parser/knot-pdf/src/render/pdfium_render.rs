//! Pdfium OCR 渲染器：使用 libpdfium 将 PDF 页面渲染为图片
//!
//! 基于 `pdfium-render` crate，动态加载 libpdfium.dylib。
//! 参考 pageindex-rs 的实现方式。

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pdfium_render::prelude::*;

use crate::error::PdfError;
use crate::render::OcrRenderer;

/// 基于 Pdfium 的 OCR 渲染器
///
/// 将 PDF 页面渲染为 PNG 图片字节数据，供 OCR 后端识别。
///
/// 注意：`Pdfium` 和 `PdfDocument` 内部使用了 `Rc`（非 Send），
/// 因此使用 `Mutex` 包装以满足 `OcrRenderer: Send + Sync` 的需求。
pub struct PdfiumOcrRenderer {
    /// 底层 Pdfium 实例（Mutex 包装以满足 Send+Sync）
    inner: Mutex<PdfiumRendererInner>,
}

struct PdfiumRendererInner {
    pdfium: Pdfium,
    /// 当前打开的 PDF 文件路径
    current_path: Option<PathBuf>,
}

// SAFETY: Pdfium 内部有 Rc（不是 Send），
// 但 Mutex 保证同一时刻只有一个线程访问
unsafe impl Send for PdfiumOcrRenderer {}
unsafe impl Sync for PdfiumOcrRenderer {}

impl PdfiumOcrRenderer {
    /// 创建 Pdfium 渲染器
    ///
    /// 自动在以下位置搜索 libpdfium：
    /// 1. `lib_path` 参数指定的目录
    /// 2. 可执行文件同级目录
    /// 3. 当前工作目录 `./`
    /// 4. 系统库路径
    pub fn new(lib_path: Option<&Path>) -> Result<Self, PdfError> {
        let bindings = Self::find_and_bind_pdfium(lib_path)?;
        let pdfium = Pdfium::new(bindings);

        Ok(Self {
            inner: Mutex::new(PdfiumRendererInner {
                pdfium,
                current_path: None,
            }),
        })
    }

    /// 搜索并绑定 libpdfium 动态库
    fn find_and_bind_pdfium(
        lib_path: Option<&Path>,
    ) -> Result<Box<dyn PdfiumLibraryBindings>, PdfError> {
        // 候选搜索路径
        let mut search_paths: Vec<PathBuf> = Vec::new();

        if let Some(p) = lib_path {
            search_paths.push(p.to_path_buf());
        }

        // 可执行文件同级目录
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                search_paths.push(dir.to_path_buf());
                // 也尝试 ../bin/
                if let Some(parent) = dir.parent() {
                    search_paths.push(parent.join("bin"));
                }
            }
        }

        // 当前工作目录
        search_paths.push(PathBuf::from("."));

        // 尝试每个路径
        for path in &search_paths {
            let lib_name =
                Pdfium::pdfium_platform_library_name_at_path(path.to_str().unwrap_or("."));
            if let Ok(bindings) = Pdfium::bind_to_library(lib_name) {
                log::info!("Pdfium library bound from: {}", path.display());
                return Ok(bindings);
            }
        }

        // 最后尝试系统库
        Pdfium::bind_to_system_library().map_err(|e| {
            PdfError::Backend(format!(
                "Failed to bind libpdfium (searched {:?}): {}",
                search_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>(),
                e
            ))
        })
    }

    /// 打开 PDF 文件并渲染指定页面
    fn render_page_inner(
        &self,
        pdf_path: &Path,
        page_index: usize,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| PdfError::Backend(format!("Pdfium lock failed: {}", e)))?;

        let document = inner
            .pdfium
            .load_pdf_from_file(pdf_path, None)
            .map_err(|e| PdfError::Backend(format!("Failed to load PDF: {}", e)))?;

        let page = document
            .pages()
            .get(page_index as u16)
            .map_err(|e| PdfError::Backend(format!("Failed to get page {}: {}", page_index, e)))?;

        // 渲染配置（参考 pageindex-rs）
        let render_config = PdfRenderConfig::new()
            .set_target_width(render_width as i32)
            .set_maximum_height(4000)
            .rotate_if_landscape(PdfPageRenderRotation::None, true);

        let bitmap = page.render_with_config(&render_config).map_err(|e| {
            PdfError::Backend(format!("Failed to render page {}: {}", page_index, e))
        })?;

        let img = bitmap.as_image();

        // 转为 RGB8（去掉 alpha 通道，JPEG 不支持 RGBA）
        let rgb_img = img.to_rgb8();

        // 编码为 PNG
        let mut bytes: Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);

        rgb_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| {
                PdfError::Backend(format!(
                    "Failed to encode page {} to PNG: {}",
                    page_index, e
                ))
            })?;

        Ok(bytes)
    }
}

impl OcrRenderer for PdfiumOcrRenderer {
    fn render_page_to_image(
        &self,
        page_index: usize,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        let path = {
            let inner = self
                .inner
                .lock()
                .map_err(|e| PdfError::Backend(format!("Pdfium lock failed: {}", e)))?;
            inner.current_path.clone().ok_or_else(|| {
                PdfError::Backend("PDF path not set on PdfiumOcrRenderer".to_string())
            })?
        };

        self.render_page_inner(&path, page_index, render_width)
    }

    fn render_region_to_image(
        &self,
        page_index: usize,
        bbox: crate::ir::BBox,
        page_width: f32,
        page_height: f32,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        // 先渲染整页
        let full_page_png = self.render_page_to_image(page_index, render_width)?;

        // 解码整页图片
        let full_img = image::load_from_memory(&full_page_png).map_err(|e| {
            PdfError::Backend(format!("Failed to decode rendered page image: {}", e))
        })?;

        let img_width = full_img.width() as f32;
        let img_height = full_img.height() as f32;

        // PDF 坐标 → 像素坐标
        let scale_x = img_width / page_width;
        let scale_y = img_height / page_height;

        let crop_x = (bbox.x * scale_x).max(0.0) as u32;
        let crop_y = (bbox.y * scale_y).max(0.0) as u32;
        let crop_w = (bbox.width * scale_x)
            .min(img_width - crop_x as f32)
            .max(1.0) as u32;
        let crop_h = (bbox.height * scale_y)
            .min(img_height - crop_y as f32)
            .max(1.0) as u32;

        // 裁剪
        let cropped = full_img.crop_imm(crop_x, crop_y, crop_w, crop_h);

        // 编码为 PNG
        let rgb_img = cropped.to_rgb8();
        let mut bytes: Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);
        rgb_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| {
                PdfError::Backend(format!("Failed to encode cropped region to PNG: {}", e))
            })?;

        Ok(bytes)
    }

    fn set_pdf_path(&self, path: &std::path::Path) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.current_path = Some(path.to_path_buf());
        }
    }
}
