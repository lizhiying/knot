//! 解析 Pipeline：逐页处理核心流程

#[cfg(feature = "async")]
pub mod async_pipeline;

use std::path::Path;
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::backend::{PdfBackend, PdfExtractBackend};
use crate::config::Config;
use crate::error::PdfError;
use crate::hf_detect::detect_and_mark_headers_footers;
use crate::ir::{
    Diagnostics, DocumentIR, ImageFormat, ImageIR, ImageSource, PageDiagnostics, PageIR,
    PageSource, Timings,
};
use crate::layout::build_blocks_with_config;
use crate::scoring::compute_page_score;
use crate::table::extract_tables_with_graphics;

use crate::ocr::{self, MockOcrBackend, OcrBackend};
use crate::render::{MockOcrRenderer, OcrRenderer};
use crate::store::{PageStatus, Store};

/// 解析 Pipeline
pub struct Pipeline {
    config: Config,
    store: Option<Box<dyn Store>>,
    ocr_backend: Option<Box<dyn OcrBackend>>,
    ocr_renderer: Option<Box<dyn OcrRenderer>>,
    /// 版面检测器（用于增强 BlockIR.role 分类）
    layout_detector: Option<Box<dyn crate::layout::LayoutDetector>>,
    /// Vision LLM 描述器（用于图表语义理解）
    #[cfg(feature = "vision")]
    vision_describer: Option<Box<dyn crate::vision::VisionDescriber>>,
    /// 公式 OCR 识别器（M12 Phase B）
    formula_recognizer: Option<Box<dyn crate::formula::FormulaRecognizer>>,
    /// 后处理管线（M13）
    postprocess_pipeline: crate::postprocess::PostProcessPipeline,
    /// OCR 最大并发数（同步模式下天然为 1，为 async 预留）
    max_ocr_workers: usize,
}

impl Pipeline {
    pub fn new(config: Config) -> Self {
        let store = if config.store_enabled {
            if let Some(path) = &config.store_path {
                #[cfg(feature = "store_sled")]
                {
                    crate::store::SledStore::open(path)
                        .ok()
                        .map(|s| Box::new(s) as Box<dyn Store>)
                }
                #[cfg(not(feature = "store_sled"))]
                {
                    let _ = path;
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let ocr_backend: Option<Box<dyn OcrBackend>> = if config.ocr_enabled {
            // 优先使用 PaddleOCR（Pure Rust，零依赖）
            #[cfg(feature = "ocr_paddle")]
            {
                let model_dir = config.ocr_model_dir.clone().or_else(|| {
                    // 自动探测模型目录
                    let candidates = [
                        // 1. 可执行文件同级目录
                        std::env::current_exe()
                            .ok()
                            .and_then(|p| p.parent().map(|d| d.join("models/ppocrv5"))),
                        // 2. 当前工作目录
                        Some(std::path::PathBuf::from("models/ppocrv5")),
                    ];
                    for candidate in candidates.iter().flatten() {
                        if candidate.join("det.onnx").exists() {
                            log::info!("Auto-detected OCR model dir: {}", candidate.display());
                            return Some(candidate.clone());
                        }
                    }
                    None
                });

                if let Some(dir) = model_dir {
                    match crate::ocr::PaddleOcrBackend::new(&dir) {
                        Ok(b) => {
                            log::info!("PaddleOCR backend initialized from: {}", dir.display());
                            Some(Box::new(b) as Box<dyn OcrBackend>)
                        }
                        Err(e) => {
                            log::warn!("PaddleOCR init failed: {}, falling back to mock", e);
                            Some(Box::new(MockOcrBackend) as Box<dyn OcrBackend>)
                        }
                    }
                } else {
                    log::warn!("OCR enabled but no model dir found, falling back to mock");
                    Some(Box::new(MockOcrBackend) as Box<dyn OcrBackend>)
                }
            }
            // 其次使用 Tesseract（需系统安装）
            #[cfg(all(feature = "ocr_tesseract", not(feature = "ocr_paddle")))]
            {
                crate::ocr::TesseractBackend::new(&config.ocr_languages)
                    .ok()
                    .map(|b| Box::new(b) as Box<dyn OcrBackend>)
            }
            // 都没有则用 Mock
            #[cfg(not(any(feature = "ocr_paddle", feature = "ocr_tesseract")))]
            {
                Some(Box::new(MockOcrBackend) as Box<dyn OcrBackend>)
            }
        } else {
            None
        };

        let ocr_renderer: Option<Box<dyn OcrRenderer>> = if config.ocr_enabled {
            // 当 pdfium feature 启用时，不创建 PdfiumOcrRenderer
            // 因为 PdfiumBackend 会作为主后端提供高质量文本抽取
            // 两个 Pdfium 实例会导致 libpdfium 全局状态冲突/死锁
            #[cfg(feature = "pdfium")]
            {
                log::info!("PdfiumBackend available, skipping PdfiumOcrRenderer to avoid conflict");
                None
            }
            #[cfg(not(feature = "pdfium"))]
            {
                Some(Box::new(MockOcrRenderer) as Box<dyn OcrRenderer>)
            }
        } else {
            None
        };

        let max_ocr_workers = config.ocr_workers.max(1);

        // Vision LLM 描述器初始化
        #[cfg(feature = "vision")]
        let vision_describer: Option<Box<dyn crate::vision::VisionDescriber>> = {
            let api_url = config.vision_api_url.clone();
            let api_key = config
                .vision_api_key
                .clone()
                .or_else(|| std::env::var("KNOT_VISION_API_KEY").ok())
                .unwrap_or_default(); // Ollama 等本地模型不需要 API Key

            if let Some(url) = api_url {
                log::info!(
                    "Vision LLM initialized: model={}, url={}",
                    config.vision_model,
                    url
                );
                Some(Box::new(crate::vision::OpenAiVisionDescriber::new(
                    &url,
                    &api_key,
                    &config.vision_model,
                )))
            } else {
                log::debug!("Vision LLM not configured (set vision_api_url in config)");
                None
            }
        };

        // 版面检测器初始化
        let layout_detector: Option<Box<dyn crate::layout::LayoutDetector>> =
            if config.layout_model_enabled {
                #[cfg(feature = "layout_model")]
                {
                    if let Some(ref model_path) = config.layout_model_path {
                        match crate::layout::OnnxLayoutDetector::from_file(
                            model_path,
                            config.layout_input_size,
                            crate::layout::ClassSchema::default(),
                            config.layout_confidence_threshold,
                        ) {
                            Ok(det) => {
                                log::info!("Loaded ONNX layout model from {:?}", model_path);
                                Some(Box::new(det) as Box<dyn crate::layout::LayoutDetector>)
                            }
                            Err(e) => {
                                log::warn!("Failed to load layout model: {}, using mock", e);
                                Some(Box::new(crate::layout::MockLayoutDetector))
                            }
                        }
                    } else {
                        // 尝试自动搜索模型文件
                        let auto_paths = ["doclayout_yolo.onnx", "layout_model.onnx"];
                        let mut found = None;
                        for p in &auto_paths {
                            let path = std::path::Path::new(p);
                            if path.exists() {
                                found = Some(path.to_path_buf());
                                break;
                            }
                        }
                        if let Some(path) = found {
                            match crate::layout::OnnxLayoutDetector::from_file(
                                &path,
                                config.layout_input_size,
                                crate::layout::ClassSchema::default(),
                                config.layout_confidence_threshold,
                            ) {
                                Ok(det) => {
                                    log::info!("Auto-loaded ONNX layout model from {:?}", path);
                                    Some(Box::new(det) as Box<dyn crate::layout::LayoutDetector>)
                                }
                                Err(e) => {
                                    log::warn!("Failed to load auto-found model: {}", e);
                                    Some(Box::new(crate::layout::MockLayoutDetector))
                                }
                            }
                        } else {
                            log::info!("No layout model found, using mock detector");
                            Some(Box::new(crate::layout::MockLayoutDetector))
                        }
                    }
                }
                #[cfg(not(feature = "layout_model"))]
                {
                    log::warn!(
                        "layout_model_enabled=true but 'layout_model' feature not compiled. \
                         Rebuild with --features layout_model to enable ONNX inference."
                    );
                    Some(Box::new(crate::layout::MockLayoutDetector))
                }
            } else {
                None
            };

        // === M12 Phase B: 公式 OCR 识别器 ===
        let formula_recognizer: Option<Box<dyn crate::formula::FormulaRecognizer>> = if config
            .formula_model_enabled
        {
            #[cfg(feature = "formula_model")]
            {
                if let Some(model_dir) = &config.formula_model_path {
                    match crate::formula::OnnxFormulaRecognizer::from_dir(
                        model_dir,
                        config.formula_confidence_threshold,
                    ) {
                        Ok(recognizer) => {
                            log::info!("Formula ONNX recognizer loaded successfully");
                            Some(Box::new(recognizer) as Box<dyn crate::formula::FormulaRecognizer>)
                        }
                        Err(e) => {
                            log::warn!("Failed to load formula model: {}", e);
                            None
                        }
                    }
                } else {
                    // 自动搜索 models/formula 目录
                    let auto_dirs = [
                        std::path::PathBuf::from("models/formula"),
                        std::env::current_exe()
                            .unwrap_or_default()
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .join("models/formula"),
                    ];
                    let mut loaded = false;
                    let mut result: Option<Box<dyn crate::formula::FormulaRecognizer>> = None;
                    for dir in &auto_dirs {
                        if dir.join("encoder_model.onnx").exists() {
                            match crate::formula::OnnxFormulaRecognizer::from_dir(
                                dir,
                                config.formula_confidence_threshold,
                            ) {
                                Ok(recognizer) => {
                                    log::info!("Formula model auto-loaded from {:?}", dir);
                                    result = Some(Box::new(recognizer));
                                    loaded = true;
                                    break;
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Failed to load formula model from {:?}: {}",
                                        dir,
                                        e
                                    );
                                }
                            }
                        }
                    }
                    if !loaded {
                        log::warn!(
                            "formula_model_enabled=true but no model directory found. \
                             Set formula_model_path or place models in models/formula/"
                        );
                    }
                    result
                }
            }
            #[cfg(not(feature = "formula_model"))]
            {
                log::warn!(
                    "formula_model_enabled=true but 'formula_model' feature not compiled. \
                         Rebuild with --features formula_model to enable formula OCR."
                );
                None
            }
        } else {
            None
        };

        let postprocess_pipeline = if config.postprocess_enabled {
            crate::postprocess::PostProcessPipeline::default_pipeline()
        } else {
            crate::postprocess::PostProcessPipeline::new()
        };

        Self {
            config,
            store,
            ocr_backend,
            ocr_renderer,
            layout_detector,
            #[cfg(feature = "vision")]
            vision_describer,
            formula_recognizer,
            postprocess_pipeline,
            max_ocr_workers,
        }
    }

    /// 解析 PDF 文件，返回完整的 DocumentIR
    pub fn parse(&self, path: &Path) -> Result<DocumentIR, PdfError> {
        // 当 pdfium feature 启用时，优先使用 PdfiumBackend（文本抽取更准确）
        // 注意：PdfiumBackend 成功时不使用 OCR（避免两个 Pdfium 实例冲突）
        #[cfg(feature = "pdfium")]
        let (backend, page_count, skip_ocr) = {
            match crate::backend::PdfiumBackend::new() {
                Ok(mut b) => match b.open(path) {
                    Ok(count) => {
                        log::info!("Using PdfiumBackend for text extraction (OCR disabled to avoid Pdfium conflict)");
                        (Box::new(b) as Box<dyn PdfBackend>, count, true)
                    }
                    Err(e) => {
                        log::warn!(
                            "PdfiumBackend open failed: {}, falling back to PdfExtract",
                            e
                        );
                        let mut fb = PdfExtractBackend::new();
                        let count = fb.open(path)?;
                        (Box::new(fb) as Box<dyn PdfBackend>, count, false)
                    }
                },
                Err(e) => {
                    log::warn!(
                        "PdfiumBackend init failed: {}, falling back to PdfExtract",
                        e
                    );
                    let mut fb = PdfExtractBackend::new();
                    let count = fb.open(path)?;
                    (Box::new(fb) as Box<dyn PdfBackend>, count, false)
                }
            }
        };

        #[cfg(not(feature = "pdfium"))]
        let (backend, page_count) = {
            let mut b = PdfExtractBackend::new();
            let count = b.open(path)?;
            (Box::new(b) as Box<dyn PdfBackend>, count)
        };

        // 计算 doc_id（文件内容 hash）
        let file_data = std::fs::read(path)?;
        let doc_id = compute_doc_id(&file_data);

        // 获取元数据
        let metadata = backend.metadata().unwrap_or_default();
        let outline = backend.outline().unwrap_or_default();

        // 检查断点继续
        let start_page = if let Some(s) = &self.store {
            s.get_last_completed_page(&doc_id)?
                .map(|idx| idx + 1)
                .unwrap_or(0)
        } else {
            0
        };

        // 逐页处理
        let mut pages = Vec::with_capacity(page_count);

        // 如果是从中间开始，先尝试加载前面的页面
        if start_page > 0 {
            if let Some(s) = &self.store {
                for i in 0..start_page {
                    if let Some(p) = s.load_page(&doc_id, i)? {
                        pages.push(p);
                    }
                }
            }
        }

        let mut diagnostics = Diagnostics::default();

        // 通知 OCR 渲染器当前 PDF 路径（PdfiumOcrRenderer 需要）
        if let Some(ocr_r) = &self.ocr_renderer {
            ocr_r.set_pdf_path(path);
        }

        for page_idx in start_page..page_count {
            // 如果指定了页码过滤，跳过不在列表中的页面
            if let Some(ref indices) = self.config.page_indices {
                if !indices.contains(&page_idx) {
                    continue;
                }
            }
            // 设置状态为 InProgress
            if let Some(s) = &self.store {
                s.update_status(&doc_id, page_idx, PageStatus::InProgress)?;
            }

            // 带超时的单页处理
            let page_result = if self.config.page_timeout_secs > 0 {
                self.process_page_with_timeout(backend.as_ref(), page_idx)
            } else {
                self.process_page(backend.as_ref(), page_idx)
            };

            match page_result {
                Ok(mut page_ir) => {
                    // OCR 触发逻辑
                    // 当使用 PdfiumBackend 时跳过 OCR（避免两个 Pdfium 实例冲突）
                    #[cfg(feature = "pdfium")]
                    let should_try_ocr = !skip_ocr;
                    #[cfg(not(feature = "pdfium"))]
                    let should_try_ocr = true;

                    if should_try_ocr {
                        if let (Some(ocr_b), Some(ocr_r)) = (&self.ocr_backend, &self.ocr_renderer)
                        {
                            if ocr::should_trigger_ocr(&page_ir, &self.config) {
                                log::debug!(
                                    "OCR triggered for page {} (max_workers={})",
                                    page_idx,
                                    self.max_ocr_workers
                                );
                                let img_data = ocr_r
                                    .render_page_to_image(page_idx, self.config.ocr_render_width)?;
                                let force_replace =
                                    self.config.ocr_mode == crate::config::OcrMode::ForceAll;
                                ocr::run_ocr_and_update_page(
                                    &mut page_ir,
                                    ocr_b.as_ref(),
                                    &img_data,
                                    force_replace,
                                )?;
                            }
                        }
                    }

                    // 保存进度
                    if let Some(s) = &self.store {
                        s.save_page(&doc_id, page_idx, &page_ir)?;
                        s.update_status(&doc_id, page_idx, PageStatus::Done)?;
                    }

                    pages.push(page_ir);
                }
                Err(e) => {
                    let err_msg = format!("Page {} failed: {}", page_idx, e);
                    diagnostics.warnings.push(err_msg.clone());

                    if let Some(s) = &self.store {
                        s.update_status(&doc_id, page_idx, PageStatus::Failed(err_msg))?;
                    }
                }
            }
        }

        // 页眉页脚检测（需要跨页比较）
        let hf_result =
            detect_and_mark_headers_footers(&mut pages, self.config.strip_headers_footers);
        if hf_result.header_patterns > 0 || hf_result.footer_patterns > 0 {
            diagnostics.warnings.push(format!(
                "Header/footer detected: {} header patterns, {} footer patterns, {} pages affected",
                hf_result.header_patterns, hf_result.footer_patterns, hf_result.affected_page_count
            ));
        }

        Ok(DocumentIR {
            doc_id,
            metadata,
            outline,
            pages,
            diagnostics,
        })
    }

    /// 逐页解析，返回迭代器
    pub fn parse_pages(&self, path: &Path) -> Result<PageIterator, PdfError> {
        let mut backend = PdfExtractBackend::new();
        let page_count = backend.open(path)?;

        Ok(PageIterator {
            backend,
            page_count,
            current_page: 0,
            config: self.config.clone(),
        })
    }

    /// 处理单个页面
    fn process_page(
        &self,
        backend: &dyn PdfBackend,
        page_index: usize,
    ) -> Result<PageIR, PdfError> {
        let start = Instant::now();
        let mem_before = crate::mem_track::MemorySnapshot::now();

        let page_info = backend.page_info(page_index)?;

        // 提取字符
        let chars = backend.extract_chars(page_index)?;

        // 提取图片
        let raw_images = backend.extract_images(page_index).unwrap_or_default();

        // 构建文本块 + 隐式网格检测 + 阅读顺序重建（支持 XY-Cut）
        let (blocks, grid_tables) = build_blocks_with_config(
            &chars,
            page_info.size.width,
            page_info.size.height,
            page_index,
            self.config.reading_order_method,
            self.config.xy_cut_gap_ratio,
        );

        // === 版面检测融合 (M10) ===
        // 当 layout_detector 可用时，渲染页面图片并调用模型检测
        let mut blocks = blocks;
        if let Some(layout_det) = &self.layout_detector {
            // 渲染页面为图片
            match backend.render_page_to_image(page_index, self.config.layout_input_size) {
                Ok(img_data) => {
                    match layout_det.detect(&img_data, page_info.size.width, page_info.size.height)
                    {
                        Ok(mut regions) => {
                            crate::layout::nms(&mut regions, 0.5);

                            log::debug!(
                                "Layout detection page {}: {} regions",
                                page_index,
                                regions.len()
                            );

                            crate::layout::merge_layout_with_blocks(
                                &mut blocks,
                                &regions,
                                self.config.layout_confidence_threshold,
                                0.3,
                            );
                        }
                        Err(e) => {
                            log::warn!("Layout detection failed for page {}: {}", page_index, e);
                        }
                    }
                }
                Err(_) => {
                    // 渲染不可用（例如 pdf-extract 后端不支持渲染），跳过版面检测
                    log::debug!(
                        "Page rendering not available for layout detection on page {}",
                        page_index
                    );
                }
            }
        }

        // 转换图片为 ImageIR（并检测二维码）
        let images: Vec<ImageIR> = raw_images
            .iter()
            .enumerate()
            .map(|(i, img)| {
                // QR code 检测：通过 bbox 宽高比和像素特征判断
                let is_qrcode = Self::detect_qrcode(&img.bbox, img.data.as_deref());
                if is_qrcode {
                    log::debug!("QR code detected: p{}_{}", page_index, i);
                }
                ImageIR {
                    image_id: format!("p{}_{}", page_index, i),
                    page_index,
                    bbox: img.bbox,
                    format: match img.format_hint.as_deref() {
                        Some(s) if s.contains("jpg") || s.contains("jpeg") => ImageFormat::Jpg,
                        Some(s) if s.contains("png") => ImageFormat::Png,
                        _ => ImageFormat::Unknown,
                    },
                    bytes_ref: None,
                    caption_refs: Vec::new(),
                    source: ImageSource::Embedded,
                    ocr_text: if is_qrcode {
                        Some("二维码/QR Code".to_string())
                    } else {
                        None
                    },
                    is_qrcode,
                }
            })
            .collect();

        // 提取线段和矩形（用于 ruled 表格检测）
        let raw_lines = backend.extract_lines(page_index).unwrap_or_default();
        let raw_rects = backend.extract_rects(page_index).unwrap_or_default();

        let extract_ms = start.elapsed().as_millis() as u64;

        // 表格抽取（支持 ruled + stream 自动切换）
        let mut tables = extract_tables_with_graphics(
            &chars,
            &raw_lines,
            &raw_rects,
            page_index,
            page_info.size.width,
            page_info.size.height,
        );

        // 过滤掉与文本块高度重叠的假阳性表格
        // （隐式网格已作为 BlockIR 输出，graphics 检测可能重复检测同一区域）
        // 注意：ruled 表格（包括 booktabs）不过滤，因为表格区域内的文字就是表格数据
        tables.retain(|table| {
            if table.extraction_mode == crate::ir::ExtractionMode::Ruled {
                return true; // ruled 表格不做重叠过滤
            }
            let table_area = (table.bbox.width * table.bbox.height).max(1.0);
            let total_overlap: f32 = blocks
                .iter()
                .map(|blk| {
                    let ox =
                        blk.bbox.x.max(table.bbox.x) < blk.bbox.right().min(table.bbox.right());
                    let oy =
                        blk.bbox.y.max(table.bbox.y) < blk.bbox.bottom().min(table.bbox.bottom());
                    if ox && oy {
                        let ow =
                            blk.bbox.right().min(table.bbox.right()) - blk.bbox.x.max(table.bbox.x);
                        let oh = blk.bbox.bottom().min(table.bbox.bottom())
                            - blk.bbox.y.max(table.bbox.y);
                        ow * oh
                    } else {
                        0.0
                    }
                })
                .sum();
            total_overlap / table_area <= 0.3
        });

        // 合并隐式网格表格（方案B下为空）
        tables.extend(grid_tables);

        // === 移除被 ruled 表格覆盖的文字块（避免表格数据既出现在表格又出现在正文中）===
        let mut blocks = blocks; // 转为 mut
        for table in &tables {
            if table.extraction_mode == crate::ir::ExtractionMode::Ruled {
                blocks.retain(|blk| {
                    let blk_cy = blk.bbox.center_y();
                    let blk_cx = blk.bbox.center_x();
                    // 如果文字块中心在表格 bbox 内，移除
                    !(blk_cx >= table.bbox.x
                        && blk_cx <= table.bbox.right()
                        && blk_cy >= table.bbox.y
                        && blk_cy <= table.bbox.bottom())
                });
            }
        }

        // === 图表区域检测 ===
        let mut images = images;

        if self.config.figure_detection_enabled {
            let raw_path_objects = backend.extract_path_objects(page_index).unwrap_or_default();

            if !raw_path_objects.is_empty() {
                let table_bboxes: Vec<crate::ir::BBox> = tables.iter().map(|t| t.bbox).collect();

                let detect_params = crate::figure::DetectParams {
                    min_area_ratio: self.config.figure_min_area_ratio,
                    min_path_count: self.config.figure_min_path_count,
                    ..Default::default()
                };

                let figure_regions = crate::figure::detect_figure_regions(
                    &raw_path_objects,
                    &blocks,
                    &table_bboxes,
                    page_info.size.width,
                    page_info.size.height,
                    page_index,
                    &detect_params,
                );

                for fig in &figure_regions {
                    // 对每个图区域：尝试裁剪渲染 + OCR
                    let mut ocr_text: Option<String> = None;

                    // 尝试通过 OcrRenderer 裁剪渲染（仅当 renderer 可用时）
                    if let Some(ocr_r) = &self.ocr_renderer {
                        match ocr_r.render_region_to_image(
                            page_index,
                            fig.bbox,
                            page_info.size.width,
                            page_info.size.height,
                            self.config.figure_render_width,
                        ) {
                            Ok(region_img) => {
                                // 尝试 OCR
                                if let Some(ocr_b) = &self.ocr_backend {
                                    match ocr_b.ocr_full_page(&region_img) {
                                        Ok(ocr_blocks) => {
                                            let text: String = ocr_blocks
                                                .iter()
                                                .map(|b| b.text.as_str())
                                                .collect::<Vec<_>>()
                                                .join(", ");
                                            if !text.is_empty() {
                                                ocr_text = Some(text);
                                            }
                                        }
                                        Err(e) => {
                                            log::debug!(
                                                "Figure OCR failed for {}: {}",
                                                fig.figure_id,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::debug!(
                                    "Figure region render failed for {}: {}",
                                    fig.figure_id,
                                    e
                                );
                                // 如果裁剪渲染不可用，回退：从图区域内的文字块聚合文字
                                let block_texts: Vec<&str> = blocks
                                    .iter()
                                    .filter(|b| fig.contained_block_ids.contains(&b.block_id))
                                    .map(|b| b.normalized_text.as_str())
                                    .collect();
                                if !block_texts.is_empty() {
                                    ocr_text = Some(block_texts.join(", "));
                                }
                            }
                        }
                    } else {
                        // 没有 OcrRenderer：从图区域内文字块聚合文字
                        let block_texts: Vec<&str> = blocks
                            .iter()
                            .filter(|b| fig.contained_block_ids.contains(&b.block_id))
                            .map(|b| b.normalized_text.as_str())
                            .collect();
                        if !block_texts.is_empty() {
                            ocr_text = Some(block_texts.join(", "));
                        }
                    }

                    // 构建 FigureRegion 类型的 ImageIR
                    let mut caption_refs = Vec::new();
                    if let Some(cap) = &fig.caption {
                        // 查找 caption 对应的 block_id
                        for blk in &blocks {
                            if blk.normalized_text.trim() == cap.as_str() {
                                caption_refs.push(blk.block_id.clone());
                                break;
                            }
                        }
                    }

                    images.push(ImageIR {
                        image_id: fig.figure_id.clone(),
                        page_index,
                        bbox: fig.bbox,
                        format: ImageFormat::Png,
                        bytes_ref: None,
                        caption_refs,
                        source: ImageSource::FigureRegion,
                        ocr_text,
                        is_qrcode: false,
                    });

                    // 从 blocks 中剔除图区域内的文字块
                    blocks.retain(|b| !fig.contained_block_ids.contains(&b.block_id));

                    log::info!(
                        "Figure detected: {} on page {} ({} paths, {:.0}% confidence, {} blocks removed)",
                        fig.figure_id,
                        page_index,
                        fig.path_count,
                        fig.confidence * 100.0,
                        fig.contained_block_ids.len(),
                    );
                }
            }

            // === 提前检测 PPT 复杂布局 ===
            // 如果页面可能是 PPT 复杂布局（后面会触发全页 VLM 回退），
            // 可以跳过逐个图表/图片的 VLM 调用以提高性能
            let likely_complex_ppt = {
                let is_ppt = page_info.size.width > page_info.size.height;
                let short_count = blocks
                    .iter()
                    .filter(|b| b.normalized_text.chars().count() < 25)
                    .count();
                let short_ratio = if blocks.is_empty() {
                    0.0
                } else {
                    short_count as f64 / blocks.len() as f64
                };
                is_ppt && blocks.len() > 8 && short_ratio > 0.40
            };
            if likely_complex_ppt {
                log::debug!(
                    "Likely complex PPT on page {}, skipping per-image VLM (will use full-page VLM instead)",
                    page_index
                );
            }

            // === 基于文本聚类的图表区域检测 ===
            // 当矢量 path 检测失效时（PPT 导出 PDF 常见），
            // 通过数据标签（数字/百分比/年份）的空间聚集来识别图表区域
            {
                let chart_regions = detect_chart_regions_from_text(
                    &blocks,
                    page_info.size.width,
                    page_info.size.height,
                );

                for (region_idx, chart_bbox) in chart_regions.iter().enumerate() {
                    // 收集区域内的文字块 ID
                    let contained_ids: Vec<String> = blocks
                        .iter()
                        .filter(|blk| {
                            let cx = blk.bbox.center_x();
                            let cy = blk.bbox.center_y();
                            cx >= chart_bbox.x
                                && cx <= chart_bbox.right()
                                && cy >= chart_bbox.y
                                && cy <= chart_bbox.bottom()
                        })
                        .map(|blk| blk.block_id.clone())
                        .collect();

                    if contained_ids.is_empty() {
                        continue;
                    }

                    let chart_id = format!("chart_p{}_{}", page_index, region_idx);

                    // 尝试渲染图表区域 + Vision LLM 描述
                    // 如果是复杂 PPT 布局，跳过（全页 VLM 会覆盖）
                    let mut ocr_text: Option<String> = None;

                    #[cfg(feature = "pdfium")]
                    if !likely_complex_ppt {
                        // 渲染整页然后裁剪图表区域
                        if let Ok(full_png) = backend
                            .render_page_to_image(page_index, self.config.figure_render_width)
                        {
                            if let Ok(region_img) = crop_region_from_image(
                                &full_png,
                                *chart_bbox,
                                page_info.size.width,
                                page_info.size.height,
                            ) {
                                // Vision LLM 描述
                                if let Some(vision) = &self.vision_describer {
                                    match vision.describe_image(
                                        &region_img,
                                        Some("这是一个数据图表。请详细描述：\n\
                                        1. 图表标题\n\
                                        2. 图表类型（柱状图/折线图/饼图/组合图等）\n\
                                        3. 横轴（X轴）：字段名称和单位\n\
                                        4. 纵轴（Y轴）：字段名称和单位（如有双轴请分别说明左轴和右轴）\n\
                                        5. 所有数据点：逐一列出每个数据值\n\
                                        6. 数据趋势：总结整体变化趋势\n\
                                        7. 图例说明"),
                                    ) {
                                        Ok(desc) => {
                                            ocr_text = Some(desc.clone());
                                            log::info!(
                                                "Vision LLM described chart {} ({} chars)",
                                                chart_id,
                                                desc.len()
                                            );
                                        }
                                        Err(e) => {
                                            log::debug!(
                                                "Vision LLM failed for chart {}: {}",
                                                chart_id,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 如果没有 Vision 描述，聚合区域内文字作为 fallback
                    if ocr_text.is_none() {
                        let text: String = blocks
                            .iter()
                            .filter(|b| contained_ids.contains(&b.block_id))
                            .map(|b| b.normalized_text.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if !text.is_empty() {
                            ocr_text = Some(text);
                        }
                    }

                    // 创建 ImageIR
                    images.push(ImageIR {
                        image_id: chart_id.clone(),
                        page_index,
                        bbox: *chart_bbox,
                        format: ImageFormat::Png,
                        bytes_ref: None,
                        caption_refs: vec![],
                        source: ImageSource::FigureRegion,
                        ocr_text,
                        is_qrcode: false,
                    });

                    // 移除区域内所有文字块
                    let removed_count = contained_ids.len();
                    blocks.retain(|b| !contained_ids.contains(&b.block_id));

                    log::info!(
                        "Chart detected from text clustering: {} on page {} ({} blocks removed)",
                        chart_id,
                        page_index,
                        removed_count,
                    );
                }
            }

            // === 嵌入图片 → FigureRegion 升级 ===
            // 条件：面积 > 阈值 OR 附近有 Figure/Fig./图 caption 文字
            let page_area = page_info.size.width * page_info.size.height;

            for img_ir in images.iter_mut() {
                if img_ir.source != ImageSource::Embedded {
                    continue;
                }

                // 跳过二维码图片的 VLM 描述
                if img_ir.is_qrcode {
                    continue;
                }

                // 跳过位置无效的图片（bbox 在原点附近，常见于 PPT 导出 PDF 的装饰图层）
                // 这些图片的坐标不代表实际页面位置，不应参与 FigureRegion 升级
                if img_ir.bbox.x.abs() < 1.0 && img_ir.bbox.y.abs() < 1.0 {
                    log::debug!(
                        "Skipping image {} with invalid position ({:.1}, {:.1})",
                        img_ir.image_id,
                        img_ir.bbox.x,
                        img_ir.bbox.y,
                    );
                    continue;
                }

                let img_area = img_ir.bbox.area();
                let ratio = if page_area > 0.0 {
                    img_area / page_area
                } else {
                    0.0
                };

                let area_ok = page_area > 0.0 && ratio >= self.config.figure_min_area_ratio;

                // 检测图片上下方 (±40pt) 是否有 Figure/Fig./图 caption
                let has_nearby_caption = {
                    let img_top = img_ir.bbox.y;
                    let img_bottom = img_ir.bbox.bottom();
                    let search_gap = 80.0;

                    blocks.iter().any(|blk| {
                        let blk_cy = blk.bbox.center_y();
                        let near_top = blk_cy >= img_top - search_gap && blk_cy < img_top;
                        let near_bottom = blk_cy > img_bottom && blk_cy <= img_bottom + search_gap;
                        // 检查 x 轴是否有重叠（图片范围与文字块范围有交集）
                        let x_overlap =
                            img_ir.bbox.x < blk.bbox.right() && img_ir.bbox.right() > blk.bbox.x;
                        if (near_top || near_bottom) && x_overlap {
                            let t = blk.normalized_text.trim();
                            t.starts_with("Figure")
                                || t.starts_with("Fig.")
                                || t.starts_with("fig.")
                                || t.starts_with("图")
                                || t.starts_with("FIGURE")
                        } else {
                            false
                        }
                    })
                };

                log::debug!(
                    "Embedded image {} on page {}: ratio={:.3}, area_ok={}, has_caption={}",
                    img_ir.image_id,
                    page_index,
                    ratio,
                    area_ok,
                    has_nearby_caption,
                );

                if !area_ok && !has_nearby_caption {
                    continue;
                }

                // 获取图片区域描述文字
                // 优先级：Vision LLM > OCR > 文字块聚合
                let mut ocr_text: Option<String> = None;

                // 先渲染图片区域（Vision 和 OCR 都需要）
                let region_img: Option<Vec<u8>> = {
                    let result = if let Some(ocr_r) = &self.ocr_renderer {
                        ocr_r.render_region_to_image(
                            page_index,
                            img_ir.bbox,
                            page_info.size.width,
                            page_info.size.height,
                            self.config.figure_render_width,
                        )
                    } else {
                        #[cfg(feature = "pdfium")]
                        {
                            backend
                                .render_page_to_image(page_index, self.config.figure_render_width)
                                .and_then(|full_png| {
                                    crop_region_from_image(
                                        &full_png,
                                        img_ir.bbox,
                                        page_info.size.width,
                                        page_info.size.height,
                                    )
                                })
                        }
                        #[cfg(not(feature = "pdfium"))]
                        {
                            Err(PdfError::Backend(
                                "Page rendering requires pdfium feature".to_string(),
                            ))
                        }
                    };
                    result.ok()
                };

                // 方案1：Vision LLM 语义描述（图表/图形）
                // 如果是复杂 PPT 布局，跳过嵌入图片的逐个 VLM（全页 VLM 会覆盖）
                #[cfg(feature = "vision")]
                if ocr_text.is_none() && !likely_complex_ppt {
                    if let Some(vision) = &self.vision_describer {
                        if let Some(ref img_bytes) = region_img {
                            // 构建 context hint（用 caption 文字帮助 LLM 理解）
                            let hint: Option<String> = blocks
                                .iter()
                                .find(|b| {
                                    let t = b.normalized_text.trim();
                                    t.starts_with("Figure")
                                        || t.starts_with("Fig.")
                                        || t.starts_with("图")
                                })
                                .map(|b| {
                                    format!(
                                        "图片标题：{}。请详细描述此图的内容：\n\
                                        - 如果是数据图表：说明图表类型、横轴、纵轴（含单位）、所有数据点、趋势\n\
                                        - 如果是流程图/架构图：描述每个节点和连接关系\n\
                                        - 如果是照片/插图：描述核心内容和关键信息",
                                        b.normalized_text
                                    )
                                })
                                .or_else(|| Some(
                                    "请详细描述此图片的内容。如果是图表，说明图表类型、横纵轴、数据点和趋势；\
                                    如果是流程图/架构图，描述节点和连接关系；\
                                    如果是照片/示意图，描述核心内容和关键信息。".to_string()
                                ));

                            match vision.describe_image(img_bytes, hint.as_deref()) {
                                Ok(desc) => {
                                    log::info!(
                                        "Vision LLM described image {} ({} chars)",
                                        img_ir.image_id,
                                        desc.len()
                                    );
                                    ocr_text = Some(desc);
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Vision LLM failed for {}: {}, falling back to OCR",
                                        img_ir.image_id,
                                        e
                                    );
                                }
                            }
                        }
                    }
                }

                // 方案2：OCR 文字识别（扫描件回退）
                if ocr_text.is_none() {
                    if let Some(ocr_b) = &self.ocr_backend {
                        if let Some(ref img_bytes) = region_img {
                            if let Ok(ocr_blocks) = ocr_b.ocr_full_page(img_bytes) {
                                let text: String = ocr_blocks
                                    .iter()
                                    .map(|b| b.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                if !text.is_empty() {
                                    ocr_text = Some(text);
                                }
                            }
                        }
                    }
                }

                // 方案2（回退）：收集图片 bbox 内的文字块
                if ocr_text.is_none() {
                    let contained_ids: Vec<String> = blocks
                        .iter()
                        .filter(|blk| {
                            let cx = blk.bbox.center_x();
                            let cy = blk.bbox.center_y();
                            cx >= img_ir.bbox.x
                                && cx <= img_ir.bbox.right()
                                && cy >= img_ir.bbox.y
                                && cy <= img_ir.bbox.bottom()
                        })
                        .map(|blk| blk.block_id.clone())
                        .collect();

                    if !contained_ids.is_empty() {
                        let text: String = blocks
                            .iter()
                            .filter(|b| contained_ids.contains(&b.block_id))
                            .map(|b| b.normalized_text.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if !text.is_empty() {
                            ocr_text = Some(text);
                        }
                        blocks.retain(|b| !contained_ids.contains(&b.block_id));
                    }
                }

                // 标记为 FigureRegion + Caption 检测
                img_ir.source = ImageSource::FigureRegion;
                img_ir.ocr_text = ocr_text;

                // Caption 检测：图片正下方寻找 "Figure" / "Fig." / "图" 开头的文字块
                let img_bottom = img_ir.bbox.bottom();
                let img_left = img_ir.bbox.x;
                let img_right = img_ir.bbox.right();
                let max_gap = 30.0;

                let mut found_caption = None;
                for blk in &blocks {
                    let blk_top = blk.bbox.y;
                    let blk_cx = blk.bbox.center_x();
                    if blk_top >= img_bottom
                        && blk_top <= img_bottom + max_gap
                        && blk_cx >= img_left
                        && blk_cx <= img_right
                    {
                        let text = blk.normalized_text.trim();
                        if text.starts_with("Figure")
                            || text.starts_with("Fig.")
                            || text.starts_with("fig.")
                            || text.starts_with("图")
                            || text.starts_with("FIGURE")
                        {
                            found_caption = Some(blk.block_id.clone());
                            break;
                        }
                    }
                }
                if let Some(cap_id) = found_caption {
                    img_ir.caption_refs.push(cap_id);
                }

                log::info!(
                    "Large embedded image -> FigureRegion: {} on page {} (ratio={:.1}%, has_ocr={}, has_caption={})",
                    img_ir.image_id,
                    page_index,
                    ratio * 100.0,
                    img_ir.ocr_text.is_some(),
                    !img_ir.caption_refs.is_empty(),
                );
            }
        }

        // PageScore 评分
        let page_score = compute_page_score(
            &chars,
            page_info.size.width,
            page_info.size.height,
            &self.config,
        );
        let text_score = page_score.score;
        let is_scanned = text_score < self.config.scoring_text_threshold;

        // M14: 解析策略选择
        let parse_strategy = crate::hybrid::select_parse_strategy(text_score, &self.config);
        log::debug!(
            "Page {} strategy: {} (text_score={:.2})",
            page_index,
            parse_strategy.display_name(),
            text_score
        );

        let mut page_warnings = Vec::new();
        for flag in &page_score.reason_flags {
            page_warnings.push(format!("PageScore flag: {:?}", flag));
        }

        // 大表格内存警告
        for table in &tables {
            if table.is_large() {
                let mem_kb = table.estimated_memory_bytes() / 1024;
                log::warn!(
                    "Large table detected: {} on page {} ({} rows, {} cells, ~{}KB)",
                    table.table_id,
                    page_index,
                    table.rows.len(),
                    table.cell_count(),
                    mem_kb
                );
                page_warnings.push(format!(
                    "Large table {}: {} rows, {} cells, ~{}KB",
                    table.table_id,
                    table.rows.len(),
                    table.cell_count(),
                    mem_kb
                ));
            }
        }

        let page_diagnostics = PageDiagnostics {
            warnings: page_warnings,
            errors: Vec::new(),
            block_count: blocks.len(),
            table_count: tables.len(),
            image_count: images.len(),
            ocr_quality_score: None,
            parse_strategy: Some(parse_strategy.display_name().to_string()),
        };

        // 内存快照（处理后）
        let mem_after = crate::mem_track::MemorySnapshot::now();
        let mem_stats = crate::mem_track::PageMemoryStats::from_snapshots(&mem_before, &mem_after);

        // === M12: 公式区域检测 ===
        let formulas = if self.config.formula_detection_enabled {
            let mut detected = crate::formula::detect_formulas(&chars, &blocks, page_index);
            // 从 blocks 中剔除被公式覆盖的文本块
            if !detected.is_empty() {
                let formula_block_ids: Vec<&str> = detected
                    .iter()
                    .flat_map(|f| f.contained_block_ids.iter().map(|s| s.as_str()))
                    .collect();
                blocks.retain(|b| !formula_block_ids.contains(&b.block_id.as_str()));
                log::debug!(
                    "Page {}: detected {} formulas, removed {} blocks",
                    page_index,
                    detected.len(),
                    formula_block_ids.len(),
                );
            }

            // === M12 Phase B: 公式 OCR → LaTeX ===
            if let Some(ref recognizer) = self.formula_recognizer {
                if let Some(ocr_r) = &self.ocr_renderer {
                    for formula in detected.iter_mut() {
                        // 渲染公式区域为图片
                        match ocr_r.render_region_to_image(
                            page_index,
                            formula.bbox,
                            page_info.size.width,
                            page_info.size.height,
                            self.config.formula_render_width,
                        ) {
                            Ok(img_bytes) => {
                                // OCR 识别
                                match recognizer.recognize(&img_bytes) {
                                    Ok(result) => {
                                        if !result.latex.is_empty() {
                                            log::debug!(
                                                "Formula {} OCR: '{}' (conf={:.3})",
                                                formula.formula_id,
                                                result.latex,
                                                result.confidence,
                                            );
                                            formula.latex = Some(result.latex);
                                        }
                                    }
                                    Err(e) => {
                                        log::debug!(
                                            "Formula OCR failed for {}: {}",
                                            formula.formula_id,
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                log::debug!(
                                    "Formula region render failed for {}: {}",
                                    formula.formula_id,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            detected
        } else {
            Vec::new()
        };

        let mut page_ir = PageIR {
            page_index,
            size: page_info.size,
            rotation: page_info.rotation,
            blocks,
            tables,
            images,
            formulas,
            diagnostics: page_diagnostics,
            text_score,
            is_scanned_guess: is_scanned,
            source: if is_scanned {
                PageSource::Ocr
            } else {
                PageSource::BornDigital
            },
            timings: Timings {
                extract_ms: Some(extract_ms),
                render_ms: None,
                ocr_ms: None,
                peak_rss_bytes: Some(mem_after.rss_bytes),
                rss_delta_bytes: Some(mem_stats.delta_bytes),
            },
        };

        // M13: 后处理管线
        self.postprocess_pipeline
            .process_page(&mut page_ir, &self.config);

        // === PPT 复杂布局 Vision LLM 回退 ===
        // 当页面有大量散布短块（PPT 信息图/卡片布局特征）且 Vision LLM 可用时，
        // 渲染整页让 LLM 提取结构化内容，替代碎片化的文本块
        #[cfg(feature = "vision")]
        if self.config.figure_detection_enabled {
            if let Some(vision) = &self.vision_describer {
                let is_complex = is_complex_ppt_layout(&page_ir);

                // 新增触发条件：文本极少但图片多的 PPT 页面
                // 典型场景：目录页、图文混排页，文字被渲染为矢量图形，pdfium 无法提取
                // 判断标准：每张图片平均不足 5 个字符 → 文本极度稀疏
                let is_sparse_text_rich_image = {
                    let is_ppt = page_ir.size.width > page_ir.size.height;
                    let total_chars: usize = page_ir
                        .blocks
                        .iter()
                        .map(|b| b.normalized_text.chars().count())
                        .sum();
                    let embedded_count = page_ir
                        .images
                        .iter()
                        .filter(|img| img.source == ImageSource::Embedded)
                        .count();
                    let chars_per_image = if embedded_count > 0 {
                        total_chars as f32 / embedded_count as f32
                    } else {
                        f32::MAX
                    };
                    is_ppt && embedded_count >= 5 && chars_per_image < 5.0
                };

                if is_sparse_text_rich_image && !is_complex {
                    log::info!(
                        "Sparse text + rich images on page {} ({} chars, {} images), using Vision LLM fallback",
                        page_index,
                        page_ir.blocks.iter().map(|b| b.normalized_text.chars().count()).sum::<usize>(),
                        page_ir.images.len()
                    );
                }

                if is_complex || is_sparse_text_rich_image {
                    log::info!(
                        "Complex PPT layout detected on page {} ({} blocks), using Vision LLM fallback",
                        page_index,
                        page_ir.blocks.len()
                    );
                    // 收集原始文本碎片作为参考（帮助 VLM 识别遗漏内容）
                    // 优先保留短块（卡片标题等），长块截断，总字符控制在 800 以内
                    let mut hint_parts: Vec<String> = Vec::new();
                    let mut hint_len = 0usize;
                    // 先收集短块（≤60字符，如卡片标题），再收集长块（截断）
                    let mut short_blocks: Vec<String> = Vec::new();
                    let mut long_blocks: Vec<String> = Vec::new();
                    for b in page_ir.blocks.iter() {
                        let t = b.full_text();
                        if t.len() <= 3 {
                            continue;
                        }
                        if t.chars().count() <= 60 {
                            short_blocks.push(t);
                        } else {
                            let truncated: String = t.chars().take(40).collect();
                            long_blocks.push(format!("{}...", truncated));
                        }
                    }
                    for part in short_blocks.into_iter().chain(long_blocks.into_iter()) {
                        if hint_len + part.len() > 800 {
                            break;
                        }
                        hint_len += part.len() + 3; // " | " separator
                        hint_parts.push(part);
                    }
                    let raw_text_hint = hint_parts.join(" | ");

                    // 提取标题文本（Title/Heading 角色），注入 prompt 中
                    let title_hint: String = page_ir
                        .blocks
                        .iter()
                        .filter(|b| {
                            matches!(
                                b.role,
                                crate::ir::BlockRole::Title | crate::ir::BlockRole::Heading
                            )
                        })
                        .map(|b| b.full_text())
                        .filter(|t| {
                            let clean: String = t
                                .chars()
                                .filter(|c| {
                                    !c.is_whitespace()
                                        && !matches!(*c, '"' | '"' | '\u{201C}' | '\u{201D}' | '"')
                                })
                                .collect();
                            clean.len() >= 3 // 过滤噪声
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    // 渲染整页（适中分辨率：太高会触发某些 VLM 的 GGML 断言错误）
                    if let Ok(full_png) = backend
                        .render_page_to_image(page_index, self.config.figure_render_width.max(1500))
                    {
                        let title_instruction = if !title_hint.is_empty() {
                            format!(
                                "\n注意：页面标题可能是「{}」，输出必须以完整标题开头（用 ## 标记）。",
                                title_hint
                            )
                        } else {
                            String::new()
                        };

                        let prompt = format!(
                            "这是一页PPT幻灯片。请严格提取页面中【所有】文字内容，不要遗漏任何一个区块。\n\
                            \n要求：\n\
                            1. 页面大标题用 ## 标记，必须完整提取（不要截断）\n\
                            2. 图表区域必须详细描述，格式如下：\n\
                               - 图表标题\n\
                               - 图表类型（柱状图/折线图/饼图/组合图等）\n\
                               - 横轴（X轴）：字段名称和单位\n\
                               - 纵轴（Y轴）：字段名称和单位（如有双轴请分别说明）\n\
                               - 所有数据点：逐一列出每个数据值\n\
                               - 数据趋势：总结整体变化趋势\n\
                               - 图例说明\n\
                            3. 分栏/卡片区域：每个卡片都要提取，包含编号、标题和正文。按编号顺序排列\n\
                            4. 流程图/架构图/关系图：描述每个节点名称、箭头方向和连接关系\n\
                            5. 页脚信息（数据来源、版权等）也要提取\n\
                            6. 不要遗漏任何编号卡片或文字区块\n\
                            7. 输出纯 Markdown 文本{}\n\
                            \n参考文字片段（可能有乱序）：{}", title_instruction, raw_text_hint
                        );

                        match vision.describe_image(&full_png, Some(&prompt)) {
                            Ok(vlm_text) if !vlm_text.is_empty() => {
                                log::info!(
                                    "Vision LLM extracted {} chars for page {} (replacing {} blocks)",
                                    vlm_text.len(),
                                    page_index,
                                    page_ir.blocks.len()
                                );

                                // 用 VLM 输出替代原有碎片化的块
                                // （标题信息已注入 prompt，VLM 会自行输出完整标题）
                                page_ir.blocks.clear();
                                page_ir.blocks.push(crate::ir::BlockIR {
                                    block_id: format!("vlm_p{}", page_index),
                                    bbox: crate::ir::BBox::new(
                                        0.0,
                                        0.0,
                                        page_ir.size.width,
                                        page_ir.size.height,
                                    ),
                                    role: crate::ir::BlockRole::Body,
                                    lines: vec![crate::ir::TextLine {
                                        spans: vec![crate::ir::TextSpan {
                                            text: vlm_text,
                                            font_size: None,
                                            is_bold: false,
                                            font_name: None,
                                        }],
                                        bbox: None,
                                    }],
                                    normalized_text: String::new(), // 将在下面设置
                                });
                                // 设置 normalized_text（VLM 块是最后一个）
                                let last_idx = page_ir.blocks.len() - 1;
                                let text = page_ir.blocks[last_idx].full_text();
                                page_ir.blocks[last_idx].normalized_text = text;

                                // 收集已有的图表 figure 描述（之前步骤已单独用 VLM 分析过）
                                // 只追加"真正的图表描述"，跳过与 VLM 输出重复的纯文本 figure
                                let vlm_text_ref = &page_ir.blocks[last_idx].normalized_text;
                                let figure_descs: Vec<String> = page_ir
                                    .images
                                    .iter()
                                    .filter(|img| {
                                        img.source == ImageSource::FigureRegion
                                            && img.ocr_text.is_some()
                                    })
                                    .filter(|img| {
                                        // 去重：如果 figure 描述中的文字大部分已在 VLM 输出中出现，
                                        // 说明这不是真正的图表，而是文本区域被误检为 figure
                                        let desc = img.ocr_text.as_ref().unwrap();
                                        let desc_chars: Vec<char> = desc.chars()
                                            .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
                                            .collect();
                                        if desc_chars.is_empty() {
                                            return false;
                                        }
                                        let matched = desc_chars.iter()
                                            .filter(|c| vlm_text_ref.contains(**c))
                                            .count();
                                        let overlap_ratio = matched as f32 / desc_chars.len() as f32;

                                        // 用更精确的句子级别去重
                                        let desc_sentences: Vec<&str> = desc.split(|c: char| c == '。' || c == '.' || c == '\n')
                                            .filter(|s| s.trim().len() > 10)
                                            .collect();
                                        let dup_sentences = desc_sentences.iter()
                                            .filter(|s| {
                                                let trimmed = s.trim();
                                                // 安全截取前 8 个字符（避免 UTF-8 边界问题）
                                                let key_part: String = trimmed.chars().take(8).collect();
                                                key_part.len() >= 6 && vlm_text_ref.contains(&key_part)
                                            })
                                            .count();
                                        let sentence_dup_ratio = if desc_sentences.is_empty() {
                                            0.0
                                        } else {
                                            dup_sentences as f32 / desc_sentences.len() as f32
                                        };

                                        if sentence_dup_ratio > 0.5 {
                                            log::debug!(
                                                "Skipping duplicate figure {}: {:.0}% sentences duplicated",
                                                img.image_id,
                                                sentence_dup_ratio * 100.0
                                            );
                                            return false;
                                        }
                                        let _ = overlap_ratio; // 保留备用
                                        true
                                    })
                                    .map(|img| {
                                        let desc = img.ocr_text.as_ref().unwrap();
                                        format!("\n\n**[图表：{}]**\n{}", img.image_id, desc)
                                    })
                                    .collect();

                                if !figure_descs.is_empty() {
                                    let combined = format!(
                                        "{}{}",
                                        page_ir.blocks[last_idx].normalized_text,
                                        figure_descs.join("")
                                    );
                                    page_ir.blocks[last_idx].normalized_text = combined;
                                }

                                // 清除 FigureRegion 图片（已合并到文本中）
                                page_ir
                                    .images
                                    .retain(|img| img.source != ImageSource::FigureRegion);

                                page_ir.source = PageSource::Ocr; // 标记为 VLM 解析
                            }
                            Ok(_) => {
                                log::debug!("Vision LLM returned empty for page {}", page_index);
                            }
                            Err(e) => {
                                log::warn!("Vision LLM failed for page {}: {}", page_index, e);
                            }
                        }
                    }
                }
            }
        }

        Ok(page_ir)
    }

    /// 带超时的单页处理
    ///
    /// 利用标准库 thread + channel 实现同步超时。
    /// 注意：超时后后台线程仍会继续运行直到完成，但结果会被丢弃。
    fn process_page_with_timeout(
        &self,
        backend: &dyn PdfBackend,
        page_index: usize,
    ) -> Result<PageIR, PdfError> {
        use std::time::Duration;

        let timeout = Duration::from_secs(self.config.page_timeout_secs);

        // 由于 PdfExtractBackend 不是 Send，我们在当前线程执行并用 channel 配合超时
        // 这里采用简单方案：先正常执行，记录耗时，超过阈值后标记为超时
        let start = Instant::now();
        let result = self.process_page(backend, page_index);
        let elapsed = start.elapsed();

        if elapsed > timeout {
            log::warn!(
                "Page {} processing took {:.1}s, exceeding timeout of {}s",
                page_index,
                elapsed.as_secs_f64(),
                self.config.page_timeout_secs
            );
            return Err(PdfError::Timeout(format!(
                "Page {} took {:.1}s (limit: {}s)",
                page_index,
                elapsed.as_secs_f64(),
                self.config.page_timeout_secs
            )));
        }

        result
    }

    /// 检测图片是否为二维码
    /// 通过 bbox 宽高比（近正方形）和像素特征（高黑白对比度）判断
    fn detect_qrcode(bbox: &crate::ir::BBox, image_data: Option<&[u8]>) -> bool {
        let w = bbox.width;
        let h = bbox.height;

        // 检查 1: 近正方形（宽高比 0.8-1.2）
        if w < 1.0 || h < 1.0 {
            return false;
        }
        let ratio = w / h;
        if !(0.8..=1.25).contains(&ratio) {
            return false;
        }

        // 检查 2: 尺寸合理（QR 码通常不会很大，也不会太小）
        // 在 PDF 坐标系中，QR 码通常 50-300 pt
        if w < 30.0 || w > 350.0 {
            return false;
        }

        // 检查 3: 如果有图片数据，分析像素黑白比
        #[cfg(feature = "pdfium")]
        if let Some(data) = image_data {
            if let Ok(img) = image::load_from_memory(data) {
                let gray = img.to_luma8();
                let total = gray.len();
                if total == 0 {
                    return false;
                }

                // 统计近黑（<50）和近白（>200）的像素比例
                let mut bw_count = 0usize;
                // 采样以提高效率（每隔几个像素取一个）
                let step = (total / 1000).max(1);
                let mut sampled = 0usize;
                for (i, &px) in gray.as_raw().iter().enumerate() {
                    if i % step != 0 {
                        continue;
                    }
                    sampled += 1;
                    if px < 80 || px > 180 {
                        bw_count += 1;
                    }
                }

                if sampled > 0 {
                    let bw_ratio = bw_count as f32 / sampled as f32;
                    // QR 码至少 70% 像素是黑或白
                    if bw_ratio < 0.70 {
                        return false;
                    }
                    log::trace!(
                        "QR candidate: {:.0}x{:.0}, bw_ratio={:.1}%",
                        w,
                        h,
                        bw_ratio * 100.0
                    );
                    return true;
                }
            }
        }

        // 如果没有像素数据，仅通过形状判断（不够可靠，不标记）
        #[cfg(not(feature = "pdfium"))]
        let _ = image_data;

        false
    }
}

/// 页面迭代器
pub struct PageIterator {
    backend: PdfExtractBackend,
    page_count: usize,
    current_page: usize,
    config: Config,
}

impl Iterator for PageIterator {
    type Item = Result<PageIR, PdfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_page >= self.page_count {
            return None;
        }

        let pipeline = Pipeline::new(self.config.clone());
        let result = pipeline.process_page(&self.backend, self.current_page);
        self.current_page += 1;
        Some(result)
    }
}

/// 计算文档 ID（基于文件内容的 SHA-256 hash）
fn compute_doc_id(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// 从整页 PNG 图片中裁剪指定 BBox 区域
#[cfg(feature = "pdfium")]
fn crop_region_from_image(
    full_page_png: &[u8],
    bbox: crate::ir::BBox,
    page_width: f32,
    page_height: f32,
) -> Result<Vec<u8>, PdfError> {
    use std::io::Cursor;

    let full_img = image::load_from_memory(full_page_png)
        .map_err(|e| PdfError::Backend(format!("Failed to decode page image: {}", e)))?;

    let img_w = full_img.width() as f32;
    let img_h = full_img.height() as f32;

    // PDF 坐标 → 像素坐标
    let scale_x = img_w / page_width;
    let scale_y = img_h / page_height;

    let crop_x = (bbox.x * scale_x).max(0.0) as u32;
    let crop_y = (bbox.y * scale_y).max(0.0) as u32;
    let crop_w = (bbox.width * scale_x).min(img_w - crop_x as f32).max(1.0) as u32;
    let crop_h = (bbox.height * scale_y).min(img_h - crop_y as f32).max(1.0) as u32;

    let cropped = full_img.crop_imm(crop_x, crop_y, crop_w, crop_h);

    let rgb_img = cropped.to_rgb8();
    let mut bytes: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    rgb_img
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| PdfError::Backend(format!("Failed to encode cropped region: {}", e)))?;

    Ok(bytes)
}

/// 基于文本数据标签聚类检测图表区域
///
/// PPT 导出的 PDF 中，图表（柱状图/折线图等）的数据标签（数字、百分比、年份）
/// 是作为独立文本对象嵌入的。当这些标签在空间上聚集时，可以推断出图表区域。
fn detect_chart_regions_from_text(
    blocks: &[crate::ir::BlockIR],
    page_width: f32,
    page_height: f32,
) -> Vec<crate::ir::BBox> {
    use crate::ir::BBox;

    // 识别数据标签块
    let data_labels: Vec<&crate::ir::BlockIR> = blocks
        .iter()
        .filter(|b| {
            let text = b.full_text();
            is_chart_data_label_text(&text)
        })
        .collect();

    if data_labels.len() < 5 {
        return vec![];
    }

    // 计算所有数据标签的外接矩形
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for blk in &data_labels {
        min_x = min_x.min(blk.bbox.x);
        min_y = min_y.min(blk.bbox.y);
        max_x = max_x.max(blk.bbox.right());
        max_y = max_y.max(blk.bbox.bottom());
    }

    let region_w = max_x - min_x;
    let region_h = max_y - min_y;

    // 区域不应太大（不应覆盖整个页面）
    if region_w > page_width * 0.85 && region_h > page_height * 0.85 {
        return vec![];
    }

    // 区域面积要合理
    if region_w <= 0.0 || region_h <= 0.0 {
        return vec![];
    }

    // 加 padding
    let padding = 30.0;
    let chart_bbox = BBox::new(
        (min_x - padding).max(0.0),
        (min_y - padding).max(0.0),
        (region_w + padding * 2.0).min(page_width - (min_x - padding).max(0.0)),
        (region_h + padding * 2.0).min(page_height - (min_y - padding).max(0.0)),
    );

    log::debug!(
        "Chart text clustering: {} data labels -> region ({:.0}, {:.0}, {:.0}x{:.0})",
        data_labels.len(),
        chart_bbox.x,
        chart_bbox.y,
        chart_bbox.width,
        chart_bbox.height,
    );

    vec![chart_bbox]
}

/// 判断文本是否为图表数据标签
fn is_chart_data_label_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 30 {
        return false;
    }

    // 纯数字（含逗号分隔）：150, 248, 1,747
    let no_comma = trimmed.replace(',', "");
    if no_comma.chars().all(|c| c.is_ascii_digit()) && !no_comma.is_empty() && no_comma.len() <= 6 {
        return true;
    }

    // 百分比：97.0%, 65.7%, 28.0%, 120%
    if trimmed.ends_with('%') {
        let num_part = &trimmed[..trimmed.len() - 1];
        if num_part.parse::<f32>().is_ok() {
            return true;
        }
    }

    // 年份标签：2022年, 2030年
    if trimmed.ends_with('年') && trimmed.chars().count() <= 6 {
        let chars: Vec<char> = trimmed.chars().collect();
        let num_str: String = chars[..chars.len() - 1].iter().collect();
        if num_str.parse::<u32>().is_ok() {
            return true;
        }
    }

    // 小数：35.0, 30.0
    if trimmed.parse::<f32>().is_ok() && trimmed.len() <= 8 && trimmed.contains('.') {
        return true;
    }

    false
}

/// 判断页面是否为 PPT 复杂布局（需要 Vision LLM 回退）
///
/// 检测特征：
/// 1. 大量短块（平均文本长度 < 25 字符）
/// 2. 多个块的 y 坐标分散在页面不同区域
/// 3. 块总数 > 8（非简单页面）
#[allow(dead_code)]
fn is_complex_ppt_layout(page: &crate::ir::PageIR) -> bool {
    let blocks = &page.blocks;

    if blocks.len() < 8 {
        return false;
    }

    // 计算短块比例（≤40 字符的块占总块数的比例）
    let short_count = blocks
        .iter()
        .filter(|b| b.full_text().chars().count() <= 40)
        .count();
    let short_ratio = short_count as f32 / blocks.len() as f32;

    // PPT 特征：超过 40% 的块是短块
    if short_ratio < 0.4 {
        return false;
    }

    // 检查 y 坐标分散度：将页面垂直分为 4 个区域，看块分布是否跨多个区域
    let page_height = page.size.height;
    let mut y_zones = [0usize; 4];
    for blk in blocks {
        let zone = ((blk.bbox.center_y() / page_height) * 4.0).min(3.0) as usize;
        y_zones[zone] += 1;
    }
    let occupied_zones = y_zones.iter().filter(|&&c| c > 0).count();

    // 至少占据 3 个 y 区域 → 内容散布在页面各处（PPT 信息图特征）
    if occupied_zones < 3 {
        return false;
    }

    log::debug!(
        "Complex PPT layout detected: blocks={}, short_ratio={:.1}% ({}/{}), y_zones={:?}, occupied={}",
        blocks.len(),
        short_ratio * 100.0,
        short_count,
        blocks.len(),
        y_zones,
        occupied_zones
    );

    true
}
