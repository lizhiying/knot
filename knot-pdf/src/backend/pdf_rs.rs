//! 基于 pdf-extract 的默认 PDF 后端实现

use std::path::Path;

use crate::error::PdfError;
use crate::ir::{BBox, DocumentMetadata, OutlineItem, PageSize};

use super::{LineOrientation, PageInfo, PdfBackend, Point, RawChar, RawImage, RawLine, RawRect};

/// 基于 pdf-extract 的默认后端
pub struct PdfExtractBackend {
    /// 页面信息缓存
    pages: Vec<PageInfo>,
    /// PDF 文件路径
    file_path: Option<std::path::PathBuf>,
    /// 文档数据（内存中）
    doc_data: Option<Vec<u8>>,
}

impl PdfExtractBackend {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            file_path: None,
            doc_data: None,
        }
    }

    /// 从页面内容流中提取线段和矩形
    fn extract_graphics(
        &self,
        page_index: usize,
    ) -> Result<(Vec<RawLine>, Vec<RawRect>), PdfError> {
        let data = self
            .doc_data
            .as_ref()
            .ok_or_else(|| PdfError::Backend("PDF not opened".into()))?;

        let doc = lopdf::Document::load_mem(data)
            .map_err(|e| PdfError::Parse(format!("Failed to reload PDF: {}", e)))?;

        let page_info = self.page_info(page_index)?;
        let page_height = page_info.size.height;

        let pages_map = doc.get_pages();
        let mut page_indices: Vec<_> = pages_map.keys().collect();
        page_indices.sort();

        let page_num = page_indices
            .get(page_index)
            .ok_or(PdfError::PageNotFound(page_index))?;

        let page_id = pages_map[page_num];

        // 获取页面内容流
        let content_data = match doc.get_page_content(page_id) {
            Ok(data) => data,
            Err(_) => return Ok((Vec::new(), Vec::new())),
        };

        let content = match lopdf::content::Content::decode(&content_data) {
            Ok(c) => c,
            Err(_) => return Ok((Vec::new(), Vec::new())),
        };

        // 图形状态
        struct GraphicsState {
            line_width: f32,
            ctm: [f32; 6], // a, b, c, d, e, f
        }

        impl GraphicsState {
            fn new() -> Self {
                Self {
                    line_width: 1.0,
                    ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                }
            }

            fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
                let tx = self.ctm[0] * x + self.ctm[2] * y + self.ctm[4];
                let ty = self.ctm[1] * x + self.ctm[3] * y + self.ctm[5];
                (tx, ty)
            }
        }

        let mut state = GraphicsState::new();
        let mut state_stack: Vec<GraphicsState> = Vec::new();

        // 当前路径
        let mut current_subpath_start: Option<(f32, f32)> = None;
        let mut current_point: Option<(f32, f32)> = None;
        let mut line_segments: Vec<(f32, f32, f32, f32)> = Vec::new(); // (x1, y1, x2, y2) 未变换
        let mut rects_raw: Vec<(f32, f32, f32, f32)> = Vec::new(); // (x, y, w, h) 未变换

        let mut lines_out: Vec<RawLine> = Vec::new();
        let mut rects_out: Vec<RawRect> = Vec::new();

        let obj_to_f32 = |obj: &lopdf::Object| -> Option<f32> {
            match obj {
                lopdf::Object::Real(v) => Some(*v as f32),
                lopdf::Object::Integer(v) => Some(*v as f32),
                _ => None,
            }
        };

        let emit_path = |segments: &[(f32, f32, f32, f32)],
                         rects: &[(f32, f32, f32, f32)],
                         state: &GraphicsState,
                         page_h: f32,
                         lines_out: &mut Vec<RawLine>,
                         rects_out: &mut Vec<RawRect>| {
            let lw = state.line_width;

            // 处理矩形
            for &(x, y, w, h) in rects {
                let (tx, ty) = state.transform_point(x, y);
                let (tx2, ty2) = state.transform_point(x + w, y + h);
                let min_x = tx.min(tx2);
                let min_y = ty.min(ty2);
                let max_x = tx.max(tx2);
                let max_y = ty.max(ty2);
                // 转换 y 坐标（PDF y 从下到上，转为从上到下）
                let top = page_h - max_y;
                let rect_w = max_x - min_x;
                let rect_h = max_y - min_y;

                if rect_w > 0.5 && rect_h > 0.5 {
                    rects_out.push(RawRect {
                        bbox: BBox::new(min_x, top, rect_w, rect_h),
                        width: lw,
                    });
                }

                // 窄矩形可以视为线段
                if rect_h < 3.0 && rect_w > 5.0 {
                    // 水平线
                    let y_mid = top + rect_h / 2.0;
                    lines_out.push(RawLine {
                        start: Point { x: min_x, y: y_mid },
                        end: Point { x: max_x, y: y_mid },
                        width: rect_h.max(lw),
                        orientation: LineOrientation::Horizontal,
                    });
                } else if rect_w < 3.0 && rect_h > 5.0 {
                    // 垂直线
                    let x_mid = min_x + rect_w / 2.0;
                    lines_out.push(RawLine {
                        start: Point { x: x_mid, y: top },
                        end: Point {
                            x: x_mid,
                            y: top + rect_h,
                        },
                        width: rect_w.max(lw),
                        orientation: LineOrientation::Vertical,
                    });
                }
            }

            // 处理线段
            for &(x1, y1, x2, y2) in segments {
                let (tx1, ty1) = state.transform_point(x1, y1);
                let (tx2, ty2) = state.transform_point(x2, y2);
                // 转换 y 坐标
                let py1 = page_h - ty1;
                let py2 = page_h - ty2;

                let dx = (tx2 - tx1).abs();
                let dy = (py2 - py1).abs();

                let orientation = if dy < 2.0 && dx > 2.0 {
                    LineOrientation::Horizontal
                } else if dx < 2.0 && dy > 2.0 {
                    LineOrientation::Vertical
                } else {
                    LineOrientation::Diagonal
                };

                lines_out.push(RawLine {
                    start: Point { x: tx1, y: py1 },
                    end: Point { x: tx2, y: py2 },
                    width: lw,
                    orientation,
                });
            }
        };

        for op in &content.operations {
            match op.operator.as_str() {
                // 图形状态
                "q" => {
                    state_stack.push(GraphicsState {
                        line_width: state.line_width,
                        ctm: state.ctm,
                    });
                }
                "Q" => {
                    if let Some(s) = state_stack.pop() {
                        state = s;
                    }
                }
                "w" => {
                    // 设置线宽
                    if let Some(w) = op.operands.first().and_then(|o| obj_to_f32(o)) {
                        state.line_width = w;
                    }
                }
                "cm" => {
                    // 修改 CTM 变换矩阵
                    if op.operands.len() >= 6 {
                        let vals: Vec<f32> =
                            op.operands.iter().filter_map(|o| obj_to_f32(o)).collect();
                        if vals.len() == 6 {
                            // 矩阵乘法：new = vals * current
                            let old = state.ctm;
                            state.ctm = [
                                vals[0] * old[0] + vals[1] * old[2],
                                vals[0] * old[1] + vals[1] * old[3],
                                vals[2] * old[0] + vals[3] * old[2],
                                vals[2] * old[1] + vals[3] * old[3],
                                vals[4] * old[0] + vals[5] * old[2] + old[4],
                                vals[4] * old[1] + vals[5] * old[3] + old[5],
                            ];
                        }
                    }
                }
                // 路径构建
                "m" => {
                    // moveto
                    if op.operands.len() >= 2 {
                        if let (Some(x), Some(y)) =
                            (obj_to_f32(&op.operands[0]), obj_to_f32(&op.operands[1]))
                        {
                            current_point = Some((x, y));
                            current_subpath_start = Some((x, y));
                        }
                    }
                }
                "l" => {
                    // lineto
                    if op.operands.len() >= 2 {
                        if let (Some(x), Some(y)) =
                            (obj_to_f32(&op.operands[0]), obj_to_f32(&op.operands[1]))
                        {
                            if let Some((px, py)) = current_point {
                                line_segments.push((px, py, x, y));
                            }
                            current_point = Some((x, y));
                        }
                    }
                }
                "re" => {
                    // rectangle
                    if op.operands.len() >= 4 {
                        let vals: Vec<f32> =
                            op.operands.iter().filter_map(|o| obj_to_f32(o)).collect();
                        if vals.len() == 4 {
                            rects_raw.push((vals[0], vals[1], vals[2], vals[3]));
                            // re 之后 current point 设置为矩形左下角
                            current_point = Some((vals[0], vals[1]));
                            current_subpath_start = Some((vals[0], vals[1]));
                        }
                    }
                }
                "h" => {
                    // closepath - 从当前点回到子路径起点
                    if let (Some((px, py)), Some((sx, sy))) = (current_point, current_subpath_start)
                    {
                        if (px - sx).abs() > 0.1 || (py - sy).abs() > 0.1 {
                            line_segments.push((px, py, sx, sy));
                        }
                        current_point = Some((sx, sy));
                    }
                }
                // 路径绘制操作符（stroke / fill）
                "S" | "s" => {
                    // S = stroke, s = close+stroke
                    if op.operator.as_str() == "s" {
                        // close path first
                        if let (Some((px, py)), Some((sx, sy))) =
                            (current_point, current_subpath_start)
                        {
                            if (px - sx).abs() > 0.1 || (py - sy).abs() > 0.1 {
                                line_segments.push((px, py, sx, sy));
                            }
                        }
                    }
                    emit_path(
                        &line_segments,
                        &rects_raw,
                        &state,
                        page_height,
                        &mut lines_out,
                        &mut rects_out,
                    );
                    line_segments.clear();
                    rects_raw.clear();
                    current_point = None;
                    current_subpath_start = None;
                }
                "f" | "F" | "f*" => {
                    // fill — 矩形 fill 也可能是表格边框
                    emit_path(
                        &[],
                        &rects_raw,
                        &state,
                        page_height,
                        &mut lines_out,
                        &mut rects_out,
                    );
                    line_segments.clear();
                    rects_raw.clear();
                    current_point = None;
                    current_subpath_start = None;
                }
                "B" | "B*" | "b" | "b*" => {
                    // fill + stroke
                    if op.operator.as_str() == "b" || op.operator.as_str() == "b*" {
                        // close path first
                        if let (Some((px, py)), Some((sx, sy))) =
                            (current_point, current_subpath_start)
                        {
                            if (px - sx).abs() > 0.1 || (py - sy).abs() > 0.1 {
                                line_segments.push((px, py, sx, sy));
                            }
                        }
                    }
                    emit_path(
                        &line_segments,
                        &rects_raw,
                        &state,
                        page_height,
                        &mut lines_out,
                        &mut rects_out,
                    );
                    line_segments.clear();
                    rects_raw.clear();
                    current_point = None;
                    current_subpath_start = None;
                }
                "n" => {
                    // no-op path (clipping path, discard)
                    line_segments.clear();
                    rects_raw.clear();
                    current_point = None;
                    current_subpath_start = None;
                }
                _ => {}
            }
        }

        Ok((lines_out, rects_out))
    }
}

impl Default for PdfExtractBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfBackend for PdfExtractBackend {
    fn open(&mut self, path: &Path) -> Result<usize, PdfError> {
        let data = std::fs::read(path)?;
        self.file_path = Some(path.to_path_buf());

        // 使用 lopdf 打开文档获取页面信息
        let doc = lopdf::Document::load_mem(&data)
            .map_err(|e| PdfError::Parse(format!("Failed to load PDF: {}", e)))?;

        // 检查是否加密
        if doc.is_encrypted() {
            return Err(PdfError::Encrypted);
        }

        let page_count = doc.get_pages().len();

        // 收集页面信息
        self.pages.clear();
        let pages_map = doc.get_pages();
        let mut page_indices: Vec<_> = pages_map.keys().collect();
        page_indices.sort();

        for (idx, &page_num) in page_indices.iter().enumerate() {
            let page_id = pages_map[page_num];
            let page_dict = doc.get_dictionary(page_id).ok();

            let (width, height) = if let Some(dict) = page_dict {
                extract_page_size(dict)
            } else {
                (612.0, 792.0) // 默认 Letter 尺寸
            };

            let rotation = if let Some(dict) = page_dict {
                extract_rotation(dict)
            } else {
                0.0
            };

            self.pages.push(PageInfo {
                page_index: idx,
                size: PageSize { width, height },
                rotation,
            });
        }

        self.doc_data = Some(data);
        Ok(page_count)
    }

    fn page_info(&self, page_index: usize) -> Result<PageInfo, PdfError> {
        self.pages
            .get(page_index)
            .cloned()
            .ok_or(PdfError::PageNotFound(page_index))
    }

    fn extract_chars(&self, page_index: usize) -> Result<Vec<RawChar>, PdfError> {
        let data = self
            .doc_data
            .as_ref()
            .ok_or_else(|| PdfError::Backend("PDF not opened".into()))?;

        let page_info = self.page_info(page_index)?;
        let mut collector = CharCollector::new(page_info.size, page_index);

        // 使用 pdf_extract::output_doc 来获取字符信息
        let doc = lopdf::Document::load_mem(data)
            .map_err(|e| PdfError::Parse(format!("Failed to reload PDF: {}", e)))?;

        output_single_page(&doc, page_index, &mut collector)?;

        Ok(collector.chars)
    }

    fn extract_images(&self, page_index: usize) -> Result<Vec<RawImage>, PdfError> {
        let data = self
            .doc_data
            .as_ref()
            .ok_or_else(|| PdfError::Backend("PDF not opened".into()))?;

        let doc = lopdf::Document::load_mem(data)
            .map_err(|e| PdfError::Parse(format!("Failed to reload PDF: {}", e)))?;

        let pages_map = doc.get_pages();
        let mut page_indices: Vec<_> = pages_map.keys().collect();
        page_indices.sort();

        let page_num = page_indices
            .get(page_index)
            .ok_or(PdfError::PageNotFound(page_index))?;

        let page_id = pages_map[page_num];
        let mut images = Vec::new();

        // 尝试从页面资源中提取 XObject 图片
        if let Ok(dict) = doc.get_dictionary(page_id) {
            if let Ok(resources) = dict.get(b"Resources") {
                if let Ok(res_dict) = resources.as_dict() {
                    if let Ok(xobjects_obj) = res_dict.get(b"XObject") {
                        if let Ok(xobjects) = xobjects_obj.as_dict() {
                            for (name, obj_ref) in xobjects.iter() {
                                if let Ok(obj_id) = obj_ref.as_reference() {
                                    if let Ok(obj) = doc.get_object(obj_id) {
                                        if let Ok(stream) = obj.as_stream() {
                                            let is_image = stream
                                                .dict
                                                .get(b"Subtype")
                                                .ok()
                                                .and_then(|s| s.as_name().ok())
                                                == Some(b"Image".as_slice());
                                            if is_image {
                                                let width = stream
                                                    .dict
                                                    .get(b"Width")
                                                    .ok()
                                                    .and_then(|w| w.as_i64().ok())
                                                    .unwrap_or(0)
                                                    as f32;
                                                let height = stream
                                                    .dict
                                                    .get(b"Height")
                                                    .ok()
                                                    .and_then(|h| h.as_i64().ok())
                                                    .unwrap_or(0)
                                                    as f32;

                                                images.push(RawImage {
                                                    bbox: BBox::new(0.0, 0.0, width, height),
                                                    data: None,
                                                    format_hint: Some(
                                                        String::from_utf8_lossy(name).to_string(),
                                                    ),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(images)
    }

    fn metadata(&self) -> Result<DocumentMetadata, PdfError> {
        let data = self
            .doc_data
            .as_ref()
            .ok_or_else(|| PdfError::Backend("PDF not opened".into()))?;

        let doc = lopdf::Document::load_mem(data)
            .map_err(|e| PdfError::Parse(format!("Failed to reload PDF: {}", e)))?;

        let mut meta = DocumentMetadata::default();

        // 尝试从 Trailer -> Info 中读取元数据
        if let Ok(info_ref) = doc.trailer.get(b"Info") {
            if let Ok(obj_id) = info_ref.as_reference() {
                if let Ok(info_dict) = doc.get_dictionary(obj_id) {
                    meta.title = get_string_from_dict(info_dict, b"Title");
                    meta.author = get_string_from_dict(info_dict, b"Author");
                    meta.subject = get_string_from_dict(info_dict, b"Subject");
                    meta.creator = get_string_from_dict(info_dict, b"Creator");
                    meta.producer = get_string_from_dict(info_dict, b"Producer");
                    meta.creation_date = get_string_from_dict(info_dict, b"CreationDate");
                    meta.modification_date = get_string_from_dict(info_dict, b"ModDate");
                }
            }
        }

        Ok(meta)
    }

    fn outline(&self) -> Result<Vec<OutlineItem>, PdfError> {
        // 基础实现：暂不解析大纲
        Ok(Vec::new())
    }

    fn extract_lines(&self, page_index: usize) -> Result<Vec<RawLine>, PdfError> {
        let (lines, _rects) = self.extract_graphics(page_index)?;
        Ok(lines)
    }

    fn extract_rects(&self, page_index: usize) -> Result<Vec<RawRect>, PdfError> {
        let (_lines, rects) = self.extract_graphics(page_index)?;
        Ok(rects)
    }
}

// ─── 辅助结构体与函数 ───

/// 字符收集器：实现 pdf_extract::OutputDev 收集字符信息
struct CharCollector {
    chars: Vec<RawChar>,
    page_size: PageSize,
    current_page: Option<u32>,
    target_page_index: usize,
    current_page_index: usize,
    target_found: bool,
    target_done: bool,
}

impl CharCollector {
    fn new(page_size: PageSize, target_page_index: usize) -> Self {
        Self {
            chars: Vec::new(),
            page_size,
            current_page: None,
            target_page_index,
            current_page_index: 0,
            target_found: false,
            target_done: false,
        }
    }
}

impl pdf_extract::OutputDev for CharCollector {
    fn begin_page(
        &mut self,
        page_num: u32,
        media_box: &pdf_extract::MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        self.current_page = Some(page_num);
        if self.current_page_index == self.target_page_index {
            // 更新页面尺寸
            self.page_size = PageSize {
                width: (media_box.urx - media_box.llx) as f32,
                height: (media_box.ury - media_box.lly) as f32,
            };
            self.target_found = true;
        }
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        if self.current_page_index == self.target_page_index && self.target_found {
            self.target_done = true;
        }
        self.current_page_index += 1;
        self.current_page = None;
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &pdf_extract::Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        char: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        if !self.target_found || self.target_done {
            return Ok(());
        }

        let x = trm.m31 as f32;
        let y = trm.m32 as f32;
        let fs = font_size as f32;
        let char_width = (width * font_size) as f32;

        // PDF 坐标系 y 从下往上，转换为从上往下
        let page_height = self.page_size.height;
        let y_top = page_height - y;

        for c in char.chars() {
            self.chars.push(RawChar {
                unicode: c,
                bbox: BBox::new(x, y_top - fs, char_width.max(fs * 0.1), fs),
                font_size: fs,
                font_name: None,
                is_bold: false,
            });
        }

        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
}

/// 对单个页面执行 pdf_extract 输出
///
/// 注意：pdf_extract::output_doc 会遍历所有页面，
/// 但 CharCollector 内部只收集 target_page_index 对应页面的字符。
fn output_single_page(
    doc: &lopdf::Document,
    _page_index: usize,
    collector: &mut CharCollector,
) -> Result<(), PdfError> {
    pdf_extract::output_doc(doc, collector)
        .map_err(|e| PdfError::Backend(format!("pdf_extract error: {:?}", e)))?;

    Ok(())
}

/// 从 PDF Object 中提取数值（兼容 int 和 float）
fn obj_to_f64(obj: &lopdf::Object) -> Option<f64> {
    obj.as_float()
        .ok()
        .map(|v| v as f64)
        .or_else(|| obj.as_i64().ok().map(|v| v as f64))
}

/// 从页面字典中提取页面尺寸
fn extract_page_size(dict: &lopdf::Dictionary) -> (f32, f32) {
    if let Ok(media_box) = dict.get(b"MediaBox") {
        if let Ok(arr) = media_box.as_array() {
            if arr.len() >= 4 {
                let x1 = obj_to_f64(&arr[0]).unwrap_or(0.0);
                let y1 = obj_to_f64(&arr[1]).unwrap_or(0.0);
                let x2 = obj_to_f64(&arr[2]).unwrap_or(612.0);
                let y2 = obj_to_f64(&arr[3]).unwrap_or(792.0);
                return ((x2 - x1) as f32, (y2 - y1) as f32);
            }
        }
    }
    (612.0, 792.0)
}

/// 从页面字典中提取旋转角度
fn extract_rotation(dict: &lopdf::Dictionary) -> f32 {
    dict.get(b"Rotate")
        .ok()
        .and_then(|r| r.as_i64().ok())
        .unwrap_or(0) as f32
}

/// 从字典中提取字符串
fn get_string_from_dict(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|v| {
        match v {
            lopdf::Object::String(bytes, _) => {
                // 尝试 UTF-8 解码
                String::from_utf8(bytes.clone()).ok().or_else(|| {
                    // 尝试 Latin-1 解码
                    Some(bytes.iter().map(|&b| b as char).collect())
                })
            }
            _ => v
                .as_name()
                .ok()
                .map(|s| String::from_utf8_lossy(s).to_string()),
        }
    })
}
