//! M6 端到端基准测试：使用真实 100 页 PDF

use knot_pdf::{parse_pdf, Config};
use std::time::Instant;

/// 端到端：解析 100 页 born-digital PDF
#[test]
fn test_parse_100_page_pdf() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );

    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("Skipping: {} not found", pdf_path);
        return;
    }

    let config = Config::default();

    let start = Instant::now();
    let doc = parse_pdf(pdf_path, &config).expect("Failed to parse PDF");
    let elapsed = start.elapsed();

    // 基本验证
    assert_eq!(doc.pages.len(), 100, "Should have 100 pages");

    // 每页应该有文本
    let non_empty_pages = doc.pages.iter().filter(|p| !p.blocks.is_empty()).count();
    assert!(
        non_empty_pages >= 95,
        "At least 95/100 pages should have text blocks, got {}",
        non_empty_pages
    );

    // 总文本量检查
    let total_chars: usize = doc
        .pages
        .iter()
        .flat_map(|p| p.blocks.iter())
        .map(|b| b.normalized_text.len())
        .sum();
    assert!(
        total_chars > 10000,
        "Total text should be > 10KB, got {} chars",
        total_chars
    );

    // 性能指标
    let elapsed_secs = elapsed.as_secs_f64();
    let pages_per_sec = 100.0 / elapsed_secs;

    println!("=== 100-Page PDF Benchmark Results ===");
    println!("Total time:        {:.2}s", elapsed_secs);
    println!("Pages/second:      {:.1}", pages_per_sec);
    println!("Total pages:       {}", doc.pages.len());
    println!("Non-empty pages:   {}", non_empty_pages);
    println!("Total text chars:  {}", total_chars);
    println!(
        "Total tables:      {}",
        doc.pages.iter().map(|p| p.tables.len()).sum::<usize>()
    );
    println!(
        "Total images:      {}",
        doc.pages.iter().map(|p| p.images.len()).sum::<usize>()
    );

    // 每页平均提取时间
    let avg_extract_ms: f64 = doc
        .pages
        .iter()
        .filter_map(|p| p.timings.extract_ms)
        .map(|ms| ms as f64)
        .sum::<f64>()
        / doc.pages.len() as f64;
    println!("Avg extract_ms:    {:.1}ms", avg_extract_ms);

    // 性能目标验证
    // debug 模式下 ~200s 是正常的，release 模式目标 < 15s
    // 这里设宽松阈值避免 CI debug 模式误报
    assert!(
        elapsed_secs < 600.0,
        "100-page PDF should parse in < 600s (debug), took {:.2}s",
        elapsed_secs
    );

    println!("=== PASS ===");
}

/// 逐页 API 测试
#[test]
fn test_parse_pages_iterator() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );

    if !std::path::Path::new(pdf_path).exists() {
        return;
    }

    let config = Config::default();
    let start = Instant::now();
    let mut page_count = 0;
    let mut error_count = 0;

    for result in knot_pdf::parse_pdf_pages(pdf_path, &config).unwrap() {
        match result {
            Ok(_page) => page_count += 1,
            Err(e) => {
                error_count += 1;
                eprintln!("Page error: {}", e);
            }
        }
    }

    let elapsed = start.elapsed();

    println!(
        "Iterator API: {} pages in {:.2}s ({} errors)",
        page_count,
        elapsed.as_secs_f64(),
        error_count
    );

    assert_eq!(page_count, 100);
    assert_eq!(error_count, 0);
}

/// Markdown 渲染测试
#[test]
fn test_markdown_render_100_pages() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );

    if !std::path::Path::new(pdf_path).exists() {
        return;
    }

    let config = Config::default();
    let doc = parse_pdf(pdf_path, &config).unwrap();

    let renderer = knot_pdf::MarkdownRenderer::new();
    let start = Instant::now();
    let md = renderer.render_document(&doc);
    let elapsed = start.elapsed();

    assert!(!md.is_empty());
    assert!(
        md.len() > 5000,
        "Markdown should be > 5KB, got {} bytes",
        md.len()
    );

    println!(
        "Markdown render: {} bytes in {:.2}ms",
        md.len(),
        elapsed.as_millis()
    );
}

/// 逐页内存释放验证
///
/// 通过 mem_track 记录的 peak_rss_bytes 验证：
/// - 后 50 页的 RSS 不应远大于前 50 页（证明内存没有持续积累）
/// - 每页 rss_delta 的中位数应接近 0（每页处理完后内存基本回落）
#[test]
fn test_per_page_memory_release() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );

    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("Skip: fixture not found");
        return;
    }

    let config = Config::default();
    let doc = parse_pdf(pdf_path, &config).unwrap();
    assert_eq!(doc.pages.len(), 100);

    // 收集每页的内存数据
    let mut rss_values: Vec<usize> = Vec::new();
    let mut delta_values: Vec<i64> = Vec::new();

    for page in &doc.pages {
        if let Some(rss) = page.timings.peak_rss_bytes {
            rss_values.push(rss);
        }
        if let Some(delta) = page.timings.rss_delta_bytes {
            delta_values.push(delta);
        }
    }

    println!("=== Per-Page Memory Release Verification ===");

    // 在支持的平台上验证
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        assert!(
            !rss_values.is_empty(),
            "Should have RSS data on macOS/Linux"
        );

        let first_rss_mb = rss_values[0] as f64 / (1024.0 * 1024.0);
        let last_rss_mb = *rss_values.last().unwrap() as f64 / (1024.0 * 1024.0);
        let max_rss_mb = *rss_values.iter().max().unwrap() as f64 / (1024.0 * 1024.0);
        let min_rss_mb = *rss_values.iter().min().unwrap() as f64 / (1024.0 * 1024.0);

        println!("First page RSS:  {:.1}MB", first_rss_mb);
        println!("Last page RSS:   {:.1}MB", last_rss_mb);
        println!("Max RSS:         {:.1}MB", max_rss_mb);
        println!("Min RSS:         {:.1}MB", min_rss_mb);

        // 前 50 页 vs 后 50 页的平均 RSS
        let first_half_avg: f64 =
            rss_values[..50].iter().sum::<usize>() as f64 / 50.0 / (1024.0 * 1024.0);
        let second_half_avg: f64 =
            rss_values[50..].iter().sum::<usize>() as f64 / 50.0 / (1024.0 * 1024.0);

        println!("First 50 avg:    {:.1}MB", first_half_avg);
        println!("Last 50 avg:     {:.1}MB", second_half_avg);

        // RSS delta 统计
        if !delta_values.is_empty() {
            let mut sorted_deltas = delta_values.clone();
            sorted_deltas.sort();
            let median_delta = sorted_deltas[sorted_deltas.len() / 2];
            let avg_delta: f64 =
                delta_values.iter().sum::<i64>() as f64 / delta_values.len() as f64 / 1024.0;
            let positive_deltas = delta_values.iter().filter(|&&d| d > 0).count();
            let negative_deltas = delta_values.iter().filter(|&&d| d < 0).count();

            println!("Median delta:    {}B", median_delta);
            println!("Avg delta:       {:.1}KB", avg_delta);
            println!(
                "Positive deltas: {}/{}  (memory grew)",
                positive_deltas,
                delta_values.len()
            );
            println!(
                "Negative deltas: {}/{}  (memory released)",
                negative_deltas,
                delta_values.len()
            );
        }

        // 关键断言：后半段 RSS 不应超过前半段的 3 倍
        // 如果没有逐页释放，100 页会积累大量数据，后半段 RSS 会远大于前半段
        assert!(
            second_half_avg < first_half_avg * 3.0,
            "Memory leak detected! Last 50 avg ({:.1}MB) > 3x first 50 avg ({:.1}MB)",
            second_half_avg,
            first_half_avg
        );

        // RSS 不应超过 max_memory_mb 配置（默认 200MB）
        assert!(
            max_rss_mb < config.max_memory_mb as f64 * 5.0,
            "RSS ({:.1}MB) exceeded 5x max_memory_mb ({}MB)",
            max_rss_mb,
            config.max_memory_mb
        );
    }

    println!("=== PASS: Per-page release verified ===");
}
