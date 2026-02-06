use crate::{DocumentParser, LlmProvider, NodeMeta, PageIndexConfig, PageIndexError, PageNode};
use async_trait::async_trait;
use image::{DynamicImage, GenericImageView, ImageFormat};
use pdfium_render::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

pub struct PdfParser;

impl PdfParser {
    pub fn new() -> Self {
        Self
    }

    /// Build the OCRFlux prompt for page-to-markdown
    fn build_page_prompt(&self) -> String {
        // Based on OCRFlux prompts.py
        let prompt = "Below is the image of one page of a document. Just return the plain text representation of this document as if you were reading it naturally.\nALL tables should be presented in HTML format.\nIf there are images or figures in the page, present them as \"<Image>(left,top),(right,bottom)</Image>\", (left,top,right,bottom) are the coordinates of the top-left and bottom-right corners of the image or figure.\nPresent all titles and headings as H1 headings.\nDo not hallucinate.\n".to_string();
        prompt
    }

    /// Synchronous helper to load PDF and render pages to PNG bytes
    /// This isolates non-Send Pdfium types from async await points
    fn load_and_render_pages(&self, path: &Path) -> Result<Vec<Vec<u8>>, PageIndexError> {
        // 1. Init PDFium
        let pdfium = Pdfium::new(
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")) // try local first
                .or_else(|_| Pdfium::bind_to_system_library())
                .map_err(|e| {
                    PageIndexError::VisionError(format!("Failed to bind PDFium: {}", e))
                })?,
        );

        // Load document
        let document = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| PageIndexError::ParseError(format!("Failed to load PDF: {}", e)))?;

        // 2. Process all pages
        let max_pages = document.pages().len();
        println!("Processing all {} pages...", max_pages);

        let mut page_images = Vec::with_capacity(max_pages as usize);

        for i in 0..max_pages {
            println!("Rendering Page {}...", i + 1);
            let page = document.pages().get(i as u16).map_err(|e| {
                PageIndexError::ParseError(format!("Failed to get page {}: {}", i, e))
            })?;

            // Render page
            // Optimization: Use 768px width. This is the "sweet spot" for 3B models:
            // High enough for readable text, low enough for <5s inference on M1 Pro.
            let render_config = PdfRenderConfig::new()
                .set_target_width(768)
                .set_maximum_height(2000)
                .rotate_if_landscape(PdfPageRenderRotation::None, true);

            let bitmap = page.render_with_config(&render_config).map_err(|e| {
                PageIndexError::VisionError(format!("Failed to render PDF page: {}", e))
            })?;

            let image = bitmap
                .as_image() // Returns DynamicImage (from pdfium_render::image re-export if compatible, or just image crate if versions align)
                .to_rgba8(); // Convert to RGBA buffer

            // Convert to JPEG bytes (faster, smaller)
            let mut jpeg_data = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(image)
                .write_to(&mut jpeg_data, ImageFormat::Jpeg)
                .map_err(|e| {
                    PageIndexError::VisionError(format!("Failed to encode image to JPEG: {}", e))
                })?;

            page_images.push(jpeg_data.into_inner());
        }

        Ok(page_images)
    }

    /// Process a page image with LLM
    async fn process_page_image(
        &self,
        png_bytes: Vec<u8>,
        provider: &dyn LlmProvider,
    ) -> Result<(String, HashMap<String, String>), PageIndexError> {
        let prompt = self.build_page_prompt();
        let response = provider
            .generate_content_with_image(&prompt, &png_bytes)
            .await?;

        // OCRFlux often returns a JSON object with "natural_text"
        #[derive(serde::Deserialize)]
        struct FluxResponse {
            natural_text: Option<String>,
        }

        // Clean up markdown code blocks if present
        let clean_json = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Helper to safe-parse JSON
        let mut final_text = response.clone();

        // Try to extract natural_text
        if let Ok(flux) = serde_json::from_str::<FluxResponse>(clean_json) {
            if let Some(text) = flux.natural_text {
                final_text = text;
            }
        } else if let Some(start) = clean_json.find('{') {
            if let Some(end) = clean_json.rfind('}') {
                if start < end {
                    let potential_json = &clean_json[start..=end];
                    if let Ok(flux) = serde_json::from_str::<FluxResponse>(potential_json) {
                        if let Some(text) = flux.natural_text {
                            final_text = text;
                        }
                    }
                }
            }
        }

        // Post-process images: <Image>(x1,y1),(x2,y2)</Image>
        let img_tag_regex = Regex::new(r"<Image>\((\d+),(\d+)\),\((\d+),(\d+)\)</Image>").unwrap();

        // We only decode the image if we find tags (optimization)
        let mut cached_image: Option<DynamicImage> = None;
        let mut extra_metadata = HashMap::new();

        // Collect replacements to insert analysis text into content
        // Note: We are NOT replacing the <Image> tag with Base64 here anymore.
        // We are appending analysis text after it.
        let mut replacements = Vec::new();

        for cap in img_tag_regex.captures_iter(&final_text) {
            let full_match = cap.get(0).unwrap().as_str().to_string();
            let x1: u32 = cap[1].parse().unwrap_or(0);
            let y1: u32 = cap[2].parse().unwrap_or(0);
            let x2: u32 = cap[3].parse().unwrap_or(0);
            let y2: u32 = cap[4].parse().unwrap_or(0);

            // Decode image if needed
            if cached_image.is_none() {
                if let Ok(img) = image::load_from_memory(&png_bytes) {
                    cached_image = Some(img);
                }
            }

            if let Some(ref img) = cached_image {
                let width = x2.saturating_sub(x1).max(1);
                let height = y2.saturating_sub(y1).max(1);

                // Crop
                let cropped = img.view(x1, y1, width, height).to_image();

                // Encode crop to JPEG
                let mut crop_bytes = Cursor::new(Vec::new());
                if let Ok(_) =
                    DynamicImage::ImageRgba8(cropped).write_to(&mut crop_bytes, ImageFormat::Jpeg)
                {
                    let crop_vec = crop_bytes.into_inner();

                    // 1. Analyze image
                    println!("Analyzing cropped image ({}x{})...", width, height);
                    let analysis_prompt = "Describe this image detail concisely. Focus on the key data or visual elements.";
                    let analysis = match provider
                        .generate_content_with_image(analysis_prompt, &crop_vec)
                        .await
                    {
                        Ok(s) => s,
                        Err(_) => "No analysis available.".to_string(),
                    };

                    // 2. Store Base64 in Metadata (Key: "image:(x1,y1),(x2,y2)")
                    use base64::{engine::general_purpose, Engine as _};
                    let b64 = general_purpose::STANDARD.encode(&crop_vec);
                    let coords_key = format!("image:({},{})-({},{})", x1, y1, x2, y2);
                    extra_metadata.insert(coords_key, b64);

                    // 3. Prepare Content Modification (Append Analysis)
                    // Original: <Image>...</Image>
                    // New: <Image>...</Image>\n\n> **图表分析**: ...
                    let content_modification =
                        format!("{}\n\n> **图表分析**: {}\n\n", full_match, analysis);

                    replacements.push((full_match, content_modification));
                }
            }
        }

        // Apply content modifications
        for (target, replacement) in replacements {
            final_text = final_text.replace(&target, &replacement);
        }

        Ok((final_text, extra_metadata))
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
        let start_time = std::time::Instant::now(); // Start timer

        let llm_provider = config.llm_provider.ok_or_else(|| {
            PageIndexError::ParseError(
                "LLM Provider is required for OCRFlux PDF parsing".to_string(),
            )
        })?;

        // 1. Load and Render (Synchronous, no Send issues)
        // This moves all Pdfium logic into a sync function, so no Pdfium types live across awaits here.
        let page_images = self.load_and_render_pages(path)?;

        let mut pages = Vec::with_capacity(page_images.len());

        // 2. Process images with LLM (Async)
        let total_pages = page_images.len();
        for (i, png_bytes) in page_images.into_iter().enumerate() {
            // Callback: (current, total)
            if let Some(cb) = config.progress_callback {
                cb(i + 1, total_pages);
            }

            println!("Processing LLM for Page {}...", i + 1);

            match self.process_page_image(png_bytes, llm_provider).await {
                Ok((markdown, extra)) => {
                    println!("Page {} processed successfully.", i + 1);
                    // Add as a child node
                    let node = PageNode {
                        node_id: format!("page-{}", i + 1),
                        title: format!("Page {}", i + 1),
                        level: 1,
                        content: markdown.clone(),
                        summary: None,
                        embedding: None,
                        metadata: NodeMeta {
                            file_path: path.to_string_lossy().to_string(),
                            page_number: Some((i + 1) as u32),
                            line_number: None,
                            token_count: markdown.len() / 4, // Approx
                            extra: extra.clone(),            // Page-level metadata
                        },
                        children: Vec::new(),
                    };
                    pages.push(node);
                }
                Err(e) => {
                    eprintln!("Failed to process page {}: {}", i + 1, e);
                }
            }
        }

        // 3. Post-process: Build Semantic Tree using H1/H2
        let semantic_root = crate::core::tree_builder::SemanticTreeBuilder::build_from_pages(
            path.file_stem().unwrap().to_string_lossy().to_string(),
            path.to_string_lossy().to_string(),
            pages,
        );

        // Save duration
        let duration = start_time.elapsed();
        let mut final_root = semantic_root;
        final_root.metadata.extra.insert(
            "processing_time_ms".to_string(),
            duration.as_millis().to_string(),
        );
        final_root.metadata.extra.insert(
            "processing_time_display".to_string(),
            format!("{:.2} s", duration.as_secs_f64()),
        );

        Ok(final_root)
    }
}
