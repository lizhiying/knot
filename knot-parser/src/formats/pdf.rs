use crate::{DocumentParser, NodeMeta, PageIndexConfig, PageIndexError, PageNode};
use async_trait::async_trait;
use knot_pdf::{Config as PdfConfig, MarkdownRenderer, Pipeline};
use std::collections::HashMap;
use std::path::Path;

pub struct PdfParser;

impl PdfParser {
    pub fn new() -> Self {
        Self
    }

    /// 使用自定义配置创建 PdfParser（保留兼容性）
    pub fn with_config(_config: PdfConfig) -> Self {
        // Pipeline 现在在 parse 时按需创建，不再缓存
        Self
    }

    /// 从 PageIndexConfig 构建 knot-pdf 的 Config（用于需要自定义配置的场景）
    pub fn build_pdf_config(config: &PageIndexConfig) -> PdfConfig {
        let mut pdf_config = PdfConfig::default();

        // OCR 配置
        pdf_config.ocr_enabled = config.pdf_ocr_enabled;
        if config.pdf_ocr_enabled {
            pdf_config.ocr_mode = knot_pdf::config::OcrMode::Auto;
            pdf_config.ocr_render_width = 1024;
        }

        // OCR 模型路径
        if let Some(ref model_dir) = config.pdf_ocr_model_dir {
            pdf_config.ocr_model_dir = Some(std::path::PathBuf::from(model_dir));
        }

        // Vision LLM 配置（同时用于图片描述和扫描件 VLM 全页回退）
        // VLM 回退对扫描件（text_score=0）是唯一能获取内容的方式
        if let Some(ref api_url) = config.pdf_vision_api_url {
            // Vision API（图表/图片语义理解）
            pdf_config.vision_api_url = Some(api_url.clone());
            // VLM 全页回退（扫描件、低质量页面使用 Vision LLM 做 OCR）
            pdf_config.vlm_enabled = true;
            pdf_config.vlm_api_url = Some(api_url.clone());

            if let Some(ref model) = config.pdf_vision_model {
                pdf_config.vision_model = model.clone();
                pdf_config.vlm_model = Some(model.clone());
            }
        }

        // 页码过滤
        if let Some(ref pages) = config.pdf_page_indices {
            pdf_config.page_indices = Some(pages.clone());
        }

        // 资源保护
        pdf_config.page_timeout_secs = 30;
        pdf_config.max_memory_mb = 500;

        pdf_config.validate();
        pdf_config
    }

    /// 将 knot-pdf 的 DocumentIR 转换为 knot-parser 的 PageNode 列表
    fn convert_to_page_nodes(doc: &knot_pdf::DocumentIR, file_path: &str) -> Vec<PageNode> {
        let renderer = MarkdownRenderer::new();
        let mut pages = Vec::new();

        for page in &doc.pages {
            let markdown = renderer.render_page(page);

            // 推断使用的处理模块
            let mut modules: Vec<&str> = vec!["TextExtract"];
            let vlm_img = page.images.iter().filter(|i| i.ocr_text.is_some()).count();
            if vlm_img > 0 {
                modules.push("VLM-图片描述");
            }
            if page
                .blocks
                .iter()
                .any(|b| b.block_id.starts_with("img_desc_"))
            {
                modules.push("图片描述→提升");
            }
            if page.blocks.iter().any(|b| b.block_id.starts_with("vlm_")) {
                modules.push("VLM-全页回退");
            }
            if page
                .blocks
                .iter()
                .any(|b| b.block_id.starts_with("ocr_fallback_"))
            {
                modules.push("OCR-回退");
            }
            if page.blocks.iter().any(|b| {
                b.normalized_text.contains("<table") || b.normalized_text.contains("| ---")
            }) {
                modules.push("表格");
            }
            if page
                .images
                .iter()
                .any(|i| i.source == knot_pdf::ir::ImageSource::FigureRegion)
            {
                modules.push("图表区域");
            }
            if page.is_scanned_guess {
                modules.push("扫描件");
            }
            if !page.tables.is_empty() {
                modules.push(&"Table-IR");
            }

            let total_text: usize = page.blocks.iter().map(|b| b.normalized_text.len()).sum();
            println!(
                "[PdfParser] page {}: blocks={} text={}chars images={} tables={} source={:?} | [{}]",
                page.page_index,
                page.blocks.len(),
                total_text,
                page.images.len(),
                page.tables.len(),
                page.source,
                modules.join(" → ")
            );
            if markdown.trim().is_empty() {
                println!(
                    "[PdfParser] page {}: SKIPPED (empty markdown)",
                    page.page_index
                );
                continue;
            }

            let mut extra = HashMap::new();
            extra.insert("text_score".to_string(), page.text_score.to_string());
            extra.insert("is_scanned".to_string(), page.is_scanned_guess.to_string());
            extra.insert("source".to_string(), format!("{:?}", page.source));

            // 记录表格信息
            if !page.tables.is_empty() {
                let table_modes: Vec<String> = page
                    .tables
                    .iter()
                    .map(|t| format!("{:?}", t.extraction_mode))
                    .collect();
                extra.insert("table_modes".to_string(), table_modes.join(","));
                extra.insert("table_count".to_string(), page.tables.len().to_string());
            }

            // 记录图片信息
            if !page.images.is_empty() {
                extra.insert("image_count".to_string(), page.images.len().to_string());
            }

            let token_count = markdown.split_whitespace().count();

            pages.push(PageNode {
                node_id: format!("page-{}", page.page_index + 1),
                title: format!("Page {}", page.page_index + 1),
                level: 1,
                content: markdown,
                summary: None,
                embedding: None,
                metadata: NodeMeta {
                    file_path: file_path.to_string(),
                    page_number: Some((page.page_index + 1) as u32),
                    line_number: None,
                    token_count,
                    extra,
                },
                children: Vec::new(),
            });
        }

        pages
    }
}

#[async_trait]
impl DocumentParser for PdfParser {
    fn can_handle(&self, extension: &str) -> bool {
        matches!(extension, "pdf")
    }

    async fn parse(
        &self,
        path: &Path,
        config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError> {
        let start_time = std::time::Instant::now();

        // 1. 根据 PageIndexConfig 构建 knot-pdf Config 并创建 Pipeline
        let pdf_config = Self::build_pdf_config(config);
        println!(
            "[PdfParser] ocr_enabled={}, ocr_model_dir={:?}, vision_api={:?}",
            pdf_config.ocr_enabled, pdf_config.ocr_model_dir, pdf_config.vision_api_url,
        );
        let mut pipeline = Pipeline::new(pdf_config);

        // 将进度回调注入 Pipeline（在 Pipeline 内部逐页处理时实时通知）
        if let Some(cb) = config.progress_callback.clone() {
            pipeline.set_page_progress_callback(move |current, total| {
                cb(current, total);
            });
        }

        // 2. 使用 Pipeline 解析 PDF → DocumentIR
        let doc = pipeline
            .parse(path)
            .map_err(|e| PageIndexError::ParseError(format!("knot-pdf error: {}", e)))?;

        println!("[PdfParser] Pipeline returned {} pages", doc.pages.len());

        // 2. 转换为 PageNode 列表，逐页通知前端
        let file_path = path.to_string_lossy().to_string();
        let total_pages = doc.pages.len();
        let renderer = MarkdownRenderer::new();
        let mut pages = Vec::new();

        for (i, page) in doc.pages.iter().enumerate() {
            let markdown = renderer.render_page(page);

            // 推断使用的处理模块
            let mut modules: Vec<&str> = vec!["TextExtract"];
            let vlm_img = page
                .images
                .iter()
                .filter(|im| im.ocr_text.is_some())
                .count();
            if vlm_img > 0 {
                modules.push("VLM-图片描述");
            }
            if page
                .blocks
                .iter()
                .any(|b| b.block_id.starts_with("img_desc_"))
            {
                modules.push("图片描述→提升");
            }
            if page.blocks.iter().any(|b| b.block_id.starts_with("vlm_")) {
                modules.push("VLM-全页回退");
            }
            if page
                .blocks
                .iter()
                .any(|b| b.block_id.starts_with("ocr_fallback_"))
            {
                modules.push("OCR-回退");
            }
            if !page.tables.is_empty() {
                modules.push("Table-IR");
            }
            if page.is_scanned_guess {
                modules.push("扫描件");
            }

            let total_text: usize = page.blocks.iter().map(|b| b.normalized_text.len()).sum();
            println!(
                "[PdfParser] page {}/{}: blocks={} text={}chars source={:?} | [{}]",
                i + 1,
                total_pages,
                page.blocks.len(),
                total_text,
                page.source,
                modules.join(" → ")
            );

            // 通知前端：进度更新
            if let Some(cb) = &config.progress_callback {
                cb(i + 1, total_pages);
            }

            // 通知前端：逐页 markdown 内容
            if let Some(cb) = &config.page_content_callback {
                cb(i, total_pages, markdown.clone());
            }

            if markdown.trim().is_empty() {
                continue;
            }

            let mut extra = HashMap::new();
            extra.insert("text_score".to_string(), page.text_score.to_string());
            extra.insert("is_scanned".to_string(), page.is_scanned_guess.to_string());
            extra.insert("source".to_string(), format!("{:?}", page.source));

            // 记录表格信息
            if !page.tables.is_empty() {
                let table_modes: Vec<String> = page
                    .tables
                    .iter()
                    .map(|t| format!("{:?}", t.extraction_mode))
                    .collect();
                extra.insert("table_modes".to_string(), table_modes.join(","));
                extra.insert("table_count".to_string(), page.tables.len().to_string());
            }

            // 记录图片信息
            if !page.images.is_empty() {
                extra.insert("image_count".to_string(), page.images.len().to_string());
            }

            let token_count = markdown.split_whitespace().count();

            pages.push(PageNode {
                node_id: format!("page-{}", page.page_index + 1),
                title: format!("Page {}", page.page_index + 1),
                level: 1,
                content: markdown,
                summary: None,
                embedding: None,
                metadata: NodeMeta {
                    file_path: file_path.to_string(),
                    page_number: Some((page.page_index + 1) as u32),
                    line_number: None,
                    token_count,
                    extra,
                },
                children: Vec::new(),
            });
        }

        println!(
            "[PdfParser] Converted to {} page nodes (elapsed: {:.1}s)",
            pages.len(),
            start_time.elapsed().as_secs_f64()
        );

        if pages.is_empty() {
            return Err(PageIndexError::ParseError(
                "knot-pdf: no content extracted from PDF".to_string(),
            ));
        }

        // 3. 构建语义树
        let file_path = path.to_string_lossy().to_string();
        let title = doc.metadata.title.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        let semantic_root = crate::core::tree_builder::SemanticTreeBuilder::build_from_pages(
            title, file_path, pages,
        );

        // 4. 记录处理时间和解析元数据
        let duration = start_time.elapsed();
        let mut final_root = semantic_root;
        final_root.metadata.extra.insert(
            "processing_time_ms".to_string(),
            duration.as_millis().to_string(),
        );
        final_root.metadata.extra.insert(
            "processing_time_display".to_string(),
            format!("{:.2}s", duration.as_secs_f64()),
        );
        final_root
            .metadata
            .extra
            .insert("parser".to_string(), "knot-pdf".to_string());
        final_root
            .metadata
            .extra
            .insert("total_pages".to_string(), doc.pages.len().to_string());
        final_root.metadata.extra.insert(
            "ocr_enabled".to_string(),
            config.pdf_ocr_enabled.to_string(),
        );

        // 汇总诊断信息
        if !doc.diagnostics.warnings.is_empty() {
            final_root
                .metadata
                .extra
                .insert("warnings".to_string(), doc.diagnostics.warnings.join("; "));
        }

        Ok(final_root)
    }
}
