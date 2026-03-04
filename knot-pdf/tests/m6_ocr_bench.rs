#![cfg(all(feature = "ocr_paddle", feature = "pdfium"))]
//! M6 OCR 性能基准测试
//!
//! 需要：
//! - `ocr_paddle` + `pdfium` features：`cargo test --features ocr_paddle,pdfium`
//! - models/ppocrv5/ 目录下的 ONNX 模型
//! - libpdfium.dylib（当前目录或系统路径）
//! - tests/fixtures/bench_100pages.pdf

use knot_pdf::config::OcrMode;
use knot_pdf::{parse_pdf, Config};
use std::time::Instant;

/// 检查 OCR 前置条件
fn check_ocr_prerequisites() -> bool {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );

    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("Skip: bench_100pages.pdf not found");
        return false;
    }

    let model_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocrv5");
    if !std::path::Path::new(model_dir).join("det.onnx").exists() {
        eprintln!("Skip: OCR model not found at {}", model_dir);
        return false;
    }

    true
}

/// 单页渲染 + OCR 耗时
///
/// 对 born-digital PDF 的单个页面强制 OCR：
/// - 测量 PdfiumOcrRenderer 渲染耗时
/// - 测量 PaddleOcrBackend 识别耗时
/// - 输出渲染分辨率和 OCR 识别的文本量
#[test]
fn test_single_page_render_and_ocr() {
    if !check_ocr_prerequisites() {
        return;
    }

    let model_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocrv5");

    // 单独测量渲染
    println!("=== Single Page Render + OCR Benchmark ===");

    // 初始化渲染器
    let render_start = Instant::now();
    let renderer =
        knot_pdf::render::PdfiumOcrRenderer::new(None).expect("Failed to init PdfiumOcrRenderer");
    let render_init_ms = render_start.elapsed().as_millis();
    println!("PdfiumOcrRenderer init: {}ms", render_init_ms);

    // 设置 PDF 路径
    let pdf_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    ));

    use knot_pdf::render::OcrRenderer;
    renderer.set_pdf_path(pdf_path);

    // 测量不同渲染宽度
    for render_width in [512, 768, 1024, 1536] {
        let t = Instant::now();
        let img_data = renderer
            .render_page_to_image(0, render_width)
            .expect("Failed to render page");
        let render_ms = t.elapsed().as_millis();
        println!(
            "  Render page 0 @ {}px: {}ms, {} bytes PNG",
            render_width,
            render_ms,
            img_data.len()
        );
    }

    // 初始化 OCR 后端
    let ocr_start = Instant::now();
    let ocr_backend = knot_pdf::ocr::PaddleOcrBackend::new(std::path::Path::new(model_dir))
        .expect("Failed to init PaddleOcrBackend");
    let ocr_init_ms = ocr_start.elapsed().as_millis();
    println!("PaddleOcrBackend init: {}ms", ocr_init_ms);

    // 渲染 + OCR 单页（默认 1024px）
    let render_width = 1024;
    let img_data = renderer
        .render_page_to_image(0, render_width)
        .expect("Failed to render");

    let ocr_t = Instant::now();
    use knot_pdf::ocr::OcrBackend;
    let blocks = ocr_backend.ocr_full_page(&img_data).expect("OCR failed");
    let ocr_ms = ocr_t.elapsed().as_millis();

    let total_text: usize = blocks.iter().map(|b| b.text.len()).sum();
    let avg_confidence: f32 = if blocks.is_empty() {
        0.0
    } else {
        blocks.iter().map(|b| b.confidence).sum::<f32>() / blocks.len() as f32
    };

    println!("  OCR page 0 @ {}px:", render_width);
    println!("    Time:       {}ms", ocr_ms);
    println!("    Blocks:     {}", blocks.len());
    println!("    Total text: {} chars", total_text);
    println!("    Avg conf:   {:.2}", avg_confidence);

    // 测量 5 页的平均耗时
    println!("\n--- 5-page average (render + OCR) ---");
    let mut total_render_ms = 0u128;
    let mut total_ocr_ms = 0u128;
    let mut total_blocks = 0usize;

    for page_idx in 0..5 {
        let t1 = Instant::now();
        let img = renderer
            .render_page_to_image(page_idx, render_width)
            .expect("render failed");
        total_render_ms += t1.elapsed().as_millis();

        let t2 = Instant::now();
        let result = ocr_backend.ocr_full_page(&img).expect("ocr failed");
        total_ocr_ms += t2.elapsed().as_millis();
        total_blocks += result.len();
    }

    println!("  Avg render:  {}ms/page", total_render_ms / 5);
    println!("  Avg OCR:     {}ms/page", total_ocr_ms / 5);
    println!(
        "  Avg total:   {}ms/page",
        (total_render_ms + total_ocr_ms) / 5
    );
    println!("  Avg blocks:  {}/page", total_blocks / 5);

    println!("=== PASS ===");
}

/// 20 页端到端 OCR 耗时
///
/// 使用 Pipeline 的完整流程（ForceAll 模式），
/// 对 20 页 PDF 进行完整的渲染 + OCR + 结果回填。
#[test]
fn test_20_page_ocr_end_to_end() {
    if !check_ocr_prerequisites() {
        return;
    }

    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );

    let model_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocrv5");

    let mut config = Config::default();
    config.ocr_enabled = true;
    config.ocr_mode = OcrMode::ForceAll;
    config.ocr_model_dir = Some(std::path::PathBuf::from(model_dir));
    config.ocr_render_width = 1024;

    println!("=== 20-Page OCR End-to-End Benchmark ===");
    println!(
        "Config: ocr_mode=ForceAll, render_width={}px",
        config.ocr_render_width
    );

    let start = Instant::now();
    let doc = parse_pdf(pdf_path, &config).expect("Failed to parse PDF");
    let total_ms = start.elapsed().as_millis();

    // 由于 parse_pdf 会处理所有 100 页，我们只关注前 20 页的统计
    let pages_to_analyze = doc.pages.len().min(20);

    let mut ocr_pages = 0usize;
    let mut total_ocr_ms = 0u64;
    let mut total_blocks = 0usize;
    let mut total_tables = 0usize;
    let mut total_text_chars = 0usize;

    for page in doc.pages.iter().take(pages_to_analyze) {
        total_blocks += page.blocks.len();
        total_tables += page.tables.len();
        total_text_chars += page
            .blocks
            .iter()
            .map(|b| b.normalized_text.len())
            .sum::<usize>();

        if let Some(ocr_ms) = page.timings.ocr_ms {
            if ocr_ms > 0 {
                ocr_pages += 1;
                total_ocr_ms += ocr_ms;
            }
        }
    }

    let total_secs = total_ms as f64 / 1000.0;
    let per_page_ms = total_ms / doc.pages.len() as u128;

    println!("Total pages:     {}", doc.pages.len());
    println!("Analyzed pages:  {}", pages_to_analyze);
    println!("OCR-ed pages:    {}", ocr_pages);
    println!("Total time:      {:.2}s", total_secs);
    println!("Per page:        {}ms", per_page_ms);
    println!(
        "Total blocks:    {} (in {} pages)",
        total_blocks, pages_to_analyze
    );
    println!("Total tables:    {}", total_tables);
    println!("Total text:      {} chars", total_text_chars);
    if ocr_pages > 0 {
        println!(
            "Avg OCR time:    {}ms/page",
            total_ocr_ms / ocr_pages as u64
        );
    }

    // 内存使用
    if let (Some(first_rss), Some(last_rss)) = (
        doc.pages.first().and_then(|p| p.timings.peak_rss_bytes),
        doc.pages.last().and_then(|p| p.timings.peak_rss_bytes),
    ) {
        println!(
            "Memory: first={:.1}MB, last={:.1}MB",
            first_rss as f64 / (1024.0 * 1024.0),
            last_rss as f64 / (1024.0 * 1024.0)
        );
    }

    // 基本验证
    assert!(doc.pages.len() >= 20, "Need at least 20 pages");
    assert!(total_blocks > 0, "Should have some text blocks");

    println!("=== PASS ===");
}
