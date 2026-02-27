//! PDFium 后端：高质量文本抽取
//!
//! 使用 libpdfium 的原生文本抽取 API，比 `pdf-extract` 在字体编码处理上更准确。
//! 特别适合学术论文、复杂排版等 born-digital PDF。

use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;

use crate::backend::traits::*;
use crate::error::PdfError;
use crate::ir::{BBox, DocumentMetadata, OutlineItem, PageSize};

/// 基于 PDFium 的 PDF 后端
pub struct PdfiumBackend {
    pdfium: Pdfium,
    pdf_path: Option<PathBuf>,
    page_count: usize,
}

impl PdfiumBackend {
    /// 创建 PDFium 后端
    pub fn new() -> Result<Self, PdfError> {
        let bindings = Self::find_pdfium()?;
        let pdfium = Pdfium::new(bindings);
        Ok(Self {
            pdfium,
            pdf_path: None,
            page_count: 0,
        })
    }

    /// 搜索 libpdfium 动态库
    fn find_pdfium() -> Result<Box<dyn PdfiumLibraryBindings>, PdfError> {
        let mut search_paths: Vec<PathBuf> = Vec::new();

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                search_paths.push(dir.to_path_buf());
            }
        }
        search_paths.push(PathBuf::from("."));

        for path in &search_paths {
            let lib_name =
                Pdfium::pdfium_platform_library_name_at_path(path.to_str().unwrap_or("."));
            if let Ok(bindings) = Pdfium::bind_to_library(lib_name) {
                log::info!("PdfiumBackend: library bound from: {}", path.display());
                return Ok(bindings);
            }
        }

        Pdfium::bind_to_system_library().map_err(|e| {
            PdfError::Backend(format!("PdfiumBackend: Failed to bind libpdfium: {}", e))
        })
    }

    /// 打开文档并执行操作
    fn with_document<F, R>(&self, f: F) -> Result<R, PdfError>
    where
        F: FnOnce(&PdfDocument) -> Result<R, PdfError>,
    {
        let path = self
            .pdf_path
            .as_ref()
            .ok_or_else(|| PdfError::Backend("PDF not opened".to_string()))?;

        let document = self
            .pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| PdfError::Backend(format!("Failed to load PDF: {}", e)))?;

        f(&document)
    }
}

impl PdfBackend for PdfiumBackend {
    fn open(&mut self, path: &Path) -> Result<usize, PdfError> {
        let document = self
            .pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| match e {
                PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                    PdfError::Encrypted
                }
                _ => PdfError::Backend(format!("Failed to open PDF: {}", e)),
            })?;

        self.page_count = document.pages().len() as usize;
        self.pdf_path = Some(path.to_path_buf());

        log::info!(
            "PdfiumBackend: opened {} ({} pages)",
            path.display(),
            self.page_count
        );

        Ok(self.page_count)
    }

    fn page_info(&self, page_index: usize) -> Result<PageInfo, PdfError> {
        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let width = page.width().value;
            let height = page.height().value;

            Ok(PageInfo {
                page_index,
                size: PageSize { width, height },
                rotation: 0.0,
            })
        })
    }

    fn extract_chars(&self, page_index: usize) -> Result<Vec<RawChar>, PdfError> {
        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let page_height = page.height().value;
            let text_page = page.text().map_err(|e| {
                PdfError::Backend(format!("Failed to get text for page {}: {}", page_index, e))
            })?;

            let mut chars = Vec::new();

            for ch in text_page.chars().iter() {
                // 获取 unicode 字符
                let unicode = match ch.unicode_char() {
                    Some(c) => c,
                    None => continue,
                };

                // 跳过控制字符
                if unicode == '\0' || unicode == '\r' || unicode == '\n' {
                    continue;
                }

                // 获取字符边界框
                let rect = match ch.tight_bounds() {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let left = rect.left().value;
                let bottom = rect.bottom().value;
                let right = rect.right().value;
                let top = rect.top().value;

                // PDFium 坐标系：左下角为原点 → 转为左上角为原点
                let x = left;
                let y = page_height - top;
                let w = (right - left).abs();
                let h = (top - bottom).abs();

                if w > 0.0 && h > 0.0 {
                    let font_size = ch.scaled_font_size().value;
                    let font_name = {
                        let name = ch.font_name();
                        if name.is_empty() {
                            None
                        } else {
                            Some(name)
                        }
                    };
                    let is_bold = ch
                        .font_weight()
                        .map(|w| {
                            matches!(
                                w,
                                PdfFontWeight::Weight700Bold
                                    | PdfFontWeight::Weight800
                                    | PdfFontWeight::Weight900
                            )
                        })
                        .unwrap_or(false);

                    chars.push(RawChar {
                        unicode,
                        bbox: BBox {
                            x,
                            y,
                            width: w,
                            height: h,
                        },
                        font_size,
                        font_name,
                        is_bold,
                    });
                }
            }

            log::debug!(
                "PdfiumBackend: page {} extracted {} chars",
                page_index,
                chars.len()
            );

            Ok(chars)
        })
    }

    fn extract_images(&self, page_index: usize) -> Result<Vec<RawImage>, PdfError> {
        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let page_height = page.height().value;
            let mut images = Vec::new();

            for obj in page.objects().iter() {
                if let Some(img_obj) = obj.as_image_object() {
                    if let Ok(rect) = obj.bounds() {
                        let left = rect.left().value;
                        let bottom = rect.bottom().value;
                        let right = rect.right().value;
                        let top = rect.top().value;

                        // 尝试获取图片原始字节（用于 QR 检测等）
                        let data = match img_obj.get_raw_image() {
                            Ok(img) => {
                                let mut buf = Vec::new();
                                let mut cursor = std::io::Cursor::new(&mut buf);
                                if img.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                    Some(buf)
                                } else {
                                    None
                                }
                            }
                            Err(_) => None,
                        };
                        let format_hint = if data.is_some() {
                            Some("png".to_string())
                        } else {
                            None
                        };

                        images.push(RawImage {
                            bbox: BBox {
                                x: left,
                                y: page_height - top,
                                width: (right - left).abs(),
                                height: (top - bottom).abs(),
                            },
                            data,
                            format_hint,
                        });
                    }
                }
            }

            Ok(images)
        })
    }
    fn extract_lines(&self, page_index: usize) -> Result<Vec<RawLine>, PdfError> {
        use crate::backend::{LineOrientation, Point};

        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let page_height = page.height().value;
            let mut lines = Vec::new();

            for obj in page.objects().iter() {
                if let Some(path_obj) = obj.as_path_object() {
                    let segment_count = path_obj.segments().len();
                    // 只处理简单线段（1-2个 segment）
                    if segment_count > 2 {
                        continue;
                    }

                    if let Ok(rect) = obj.bounds() {
                        let left = rect.left().value;
                        let bottom = rect.bottom().value;
                        let right = rect.right().value;
                        let top = rect.top().value;

                        let w = (right - left).abs();
                        let h = (top - bottom).abs();

                        // 转成 top-left origin
                        let y1 = page_height - top;
                        let y2 = page_height - bottom;

                        let orientation = if h < 2.0 && w > 5.0 {
                            LineOrientation::Horizontal
                        } else if w < 2.0 && h > 5.0 {
                            LineOrientation::Vertical
                        } else {
                            continue; // 忽略对角线或太短的
                        };

                        let stroke_width =
                            path_obj.stroke_width().map(|sw| sw.value).unwrap_or(0.5);

                        lines.push(RawLine {
                            start: Point { x: left, y: y1 },
                            end: Point { x: right, y: y2 },
                            width: stroke_width.max(h.min(w)), // 使用 bbox 短边或 stroke_width
                            orientation,
                        });
                    }
                }
            }

            log::debug!(
                "PdfiumBackend: page {} extracted {} lines",
                page_index,
                lines.len()
            );

            Ok(lines)
        })
    }

    fn extract_rects(&self, page_index: usize) -> Result<Vec<RawRect>, PdfError> {
        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let page_height = page.height().value;
            let mut rects = Vec::new();

            for obj in page.objects().iter() {
                if let Some(path_obj) = obj.as_path_object() {
                    let segment_count = path_obj.segments().len();
                    // 矩形通常是 4-5 个 segment
                    if segment_count < 3 || segment_count > 6 {
                        continue;
                    }

                    if let Ok(rect) = obj.bounds() {
                        let left = rect.left().value;
                        let bottom = rect.bottom().value;
                        let right = rect.right().value;
                        let top = rect.top().value;

                        let w = (right - left).abs();
                        let h = (top - bottom).abs();

                        // 只保留「窄矩形」（看起来像线段的矩形）
                        if !(h < 3.0 && w > 5.0) && !(w < 3.0 && h > 5.0) {
                            continue;
                        }

                        let stroke_width =
                            path_obj.stroke_width().map(|sw| sw.value).unwrap_or(0.5);

                        rects.push(RawRect {
                            bbox: BBox {
                                x: left,
                                y: page_height - top,
                                width: w,
                                height: h,
                            },
                            width: stroke_width,
                        });
                    }
                }
            }

            log::debug!(
                "PdfiumBackend: page {} extracted {} rects",
                page_index,
                rects.len()
            );

            Ok(rects)
        })
    }

    fn extract_path_objects(&self, page_index: usize) -> Result<Vec<RawPathObject>, PdfError> {
        use crate::backend::traits::PathObjectKind;

        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let page_height = page.height().value;
            let mut path_objects = Vec::new();

            for obj in page.objects().iter() {
                if let Some(path_obj) = obj.as_path_object() {
                    if let Ok(rect) = obj.bounds() {
                        let left = rect.left().value;
                        let bottom = rect.bottom().value;
                        let right = rect.right().value;
                        let top = rect.top().value;

                        let w = (right - left).abs();
                        let h = (top - bottom).abs();

                        // 跳过极小的 path（噪点）
                        if w < 1.0 && h < 1.0 {
                            continue;
                        }

                        // 根据 segment 数量和形状推断类型
                        let segment_count = path_obj.segments().len();
                        let kind = if segment_count <= 2 {
                            PathObjectKind::Line
                        } else if segment_count <= 5 && (w - h).abs() / w.max(h).max(1.0) < 0.3 {
                            // 大致正方形且 segment <= 5 → 矩形
                            PathObjectKind::Rect
                        } else if segment_count <= 5 {
                            PathObjectKind::Rect
                        } else {
                            PathObjectKind::Curve
                        };

                        path_objects.push(RawPathObject {
                            bbox: BBox {
                                x: left,
                                y: page_height - top,
                                width: w,
                                height: h,
                            },
                            kind,
                        });
                    }
                }
            }

            log::debug!(
                "PdfiumBackend: page {} extracted {} path objects",
                page_index,
                path_objects.len()
            );

            Ok(path_objects)
        })
    }

    fn render_page_to_image(
        &self,
        page_index: usize,
        render_width: u32,
    ) -> Result<Vec<u8>, PdfError> {
        use std::io::Cursor;

        self.with_document(|doc| {
            let page = doc
                .pages()
                .get(page_index as u16)
                .map_err(|_| PdfError::PageNotFound(page_index))?;

            let render_config = PdfRenderConfig::new()
                .set_target_width(render_width as i32)
                .set_maximum_height(4000)
                .rotate_if_landscape(PdfPageRenderRotation::None, true);

            let bitmap = page.render_with_config(&render_config).map_err(|e| {
                PdfError::Backend(format!("Failed to render page {}: {}", page_index, e))
            })?;

            let img = bitmap.as_image();
            let rgb_img = img.to_rgb8();

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

            log::debug!(
                "PdfiumBackend: rendered page {} to PNG ({} bytes, width={})",
                page_index,
                bytes.len(),
                render_width
            );

            Ok(bytes)
        })
    }

    fn metadata(&self) -> Result<DocumentMetadata, PdfError> {
        self.with_document(|doc| {
            let meta = doc.metadata();
            Ok(DocumentMetadata {
                title: meta
                    .get(PdfDocumentMetadataTagType::Title)
                    .map(|t| t.value().to_string()),
                author: meta
                    .get(PdfDocumentMetadataTagType::Author)
                    .map(|t| t.value().to_string()),
                subject: meta
                    .get(PdfDocumentMetadataTagType::Subject)
                    .map(|t| t.value().to_string()),
                creator: meta
                    .get(PdfDocumentMetadataTagType::Creator)
                    .map(|t| t.value().to_string()),
                producer: meta
                    .get(PdfDocumentMetadataTagType::Producer)
                    .map(|t| t.value().to_string()),
                creation_date: None,
                modification_date: None,
            })
        })
    }

    fn outline(&self) -> Result<Vec<OutlineItem>, PdfError> {
        Ok(Vec::new())
    }
}
