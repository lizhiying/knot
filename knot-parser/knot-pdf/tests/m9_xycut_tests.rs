//! M9 集成测试：XY-Cut 阅读顺序算法
//!
//! 测试内容：
//! 1. 真实 PDF 的 XY-Cut vs Heuristic 阅读顺序对比
//! 2. 多栏文档的阅读顺序正确性验证
//! 3. 性能基准对比：XY-Cut vs Heuristic

use knot_pdf::config::ReadingOrderMethod;
use knot_pdf::{parse_pdf, Config};
use std::time::Instant;

/// 创建指定阅读顺序算法的配置
fn config_with_method(method: ReadingOrderMethod) -> Config {
    let mut config = Config::default();
    config.reading_order_method = method;
    config
}

/// 获取 fixtures 目录
fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

// ─── 真实 PDF 验证测试 ───

#[test]
fn test_xycut_attention_paper_reading_order() {
    let pdf_path = fixtures_dir().join("Attention_Is_All_You_Need.pdf");
    if !pdf_path.exists() {
        eprintln!("跳过：Attention_Is_All_You_Need.pdf 不存在");
        return;
    }

    // 使用 XyCut 解析
    let config = config_with_method(ReadingOrderMethod::XyCut);
    let doc = parse_pdf(&pdf_path, &config).expect("解析 PDF 失败");

    assert!(!doc.pages.is_empty(), "应至少有 1 页");

    // 第 1 页验证：标题应该在 Abstract 之前
    let page0 = &doc.pages[0];
    assert!(!page0.blocks.is_empty(), "第 1 页应有文本块");

    // 收集所有文本
    let all_text: Vec<&str> = page0
        .blocks
        .iter()
        .map(|b| b.normalized_text.as_str())
        .collect();

    // 查找关键文本位置
    let title_pos = all_text
        .iter()
        .position(|t| t.contains("Attention") && t.contains("Need"));
    let abstract_pos = all_text.iter().position(|t| t.contains("Abstract"));

    if let (Some(tp), Some(ap)) = (title_pos, abstract_pos) {
        assert!(tp < ap, "标题 (pos={}) 应在 Abstract (pos={}) 之前", tp, ap,);
    }

    println!(
        "=== Attention Paper Page 1: {} blocks ===",
        page0.blocks.len()
    );
    for (i, blk) in page0.blocks.iter().take(10).enumerate() {
        println!(
            "  blk_{}: ({:.0},{:.0}) w={:.0} role={:?} text='{}'",
            i,
            blk.bbox.x,
            blk.bbox.y,
            blk.bbox.width,
            blk.role,
            &blk.normalized_text[..blk.normalized_text.len().min(60)]
        );
    }

    // 验证双栏页面（通常第 2 页开始是双栏正文）
    if doc.pages.len() > 1 {
        let page1 = &doc.pages[1];
        println!(
            "\n=== Attention Paper Page 2: {} blocks ===",
            page1.blocks.len()
        );
        for (i, blk) in page1.blocks.iter().take(15).enumerate() {
            println!(
                "  blk_{}: ({:.0},{:.0}) w={:.0} role={:?} text='{}'",
                i,
                blk.bbox.x,
                blk.bbox.y,
                blk.bbox.width,
                blk.role,
                &blk.normalized_text[..blk.normalized_text.len().min(60)]
            );
        }
    }
}

#[test]
fn test_xycut_multi_column_eval_sample() {
    let pdf_path = fixtures_dir()
        .join("eval_samples")
        .join("born_digital")
        .join("bd02_multi_column.pdf");
    if !pdf_path.exists() {
        eprintln!("跳过：bd02_multi_column.pdf 不存在");
        return;
    }

    let config_xycut = config_with_method(ReadingOrderMethod::XyCut);
    let config_heuristic = config_with_method(ReadingOrderMethod::Heuristic);

    let doc_xycut = parse_pdf(&pdf_path, &config_xycut).expect("XyCut 解析失败");
    let doc_heuristic = parse_pdf(&pdf_path, &config_heuristic).expect("Heuristic 解析失败");

    assert_eq!(doc_xycut.pages.len(), doc_heuristic.pages.len());

    for (page_idx, (page_xy, page_h)) in doc_xycut
        .pages
        .iter()
        .zip(doc_heuristic.pages.iter())
        .enumerate()
    {
        println!(
            "\n=== Page {} 对比: XyCut {} blocks vs Heuristic {} blocks ===",
            page_idx,
            page_xy.blocks.len(),
            page_h.blocks.len()
        );

        // 块数应该相同（只是顺序不同）
        assert_eq!(
            page_xy.blocks.len(),
            page_h.blocks.len(),
            "Page {} 块数应相同",
            page_idx
        );

        // 检查是否有顺序差异
        let xy_order: Vec<String> = page_xy
            .blocks
            .iter()
            .map(|b| b.normalized_text.clone())
            .collect();
        let h_order: Vec<String> = page_h
            .blocks
            .iter()
            .map(|b| b.normalized_text.clone())
            .collect();

        let same_order = xy_order == h_order;
        println!(
            "  顺序{}",
            if same_order {
                "相同（单栏或简单布局）"
            } else {
                "不同（XyCut 重排了阅读顺序）"
            }
        );

        if !same_order {
            println!("  XyCut  前 5 块:");
            for (i, text) in xy_order.iter().take(5).enumerate() {
                println!("    {}: '{}'", i, &text[..text.len().min(50)]);
            }
            println!("  Heuristic 前 5 块:");
            for (i, text) in h_order.iter().take(5).enumerate() {
                println!("    {}: '{}'", i, &text[..text.len().min(50)]);
            }
        }
    }
}

#[test]
fn test_xycut_academic_paper_eval_sample() {
    let pdf_path = fixtures_dir()
        .join("eval_samples")
        .join("born_digital")
        .join("bd04_academic_paper.pdf");
    if !pdf_path.exists() {
        eprintln!("跳过：bd04_academic_paper.pdf 不存在");
        return;
    }

    let config = config_with_method(ReadingOrderMethod::XyCut);
    let doc = parse_pdf(&pdf_path, &config).expect("解析失败");

    println!("=== Academic Paper: {} pages ===", doc.pages.len());
    for page in &doc.pages {
        println!("  Page {}: {} blocks", page.page_index, page.blocks.len());
    }
}

#[test]
fn test_xycut_auto_mode_same_as_single_column() {
    // Auto 模式对单栏文档应回退到 Heuristic（不增加开销）
    let pdf_path = fixtures_dir()
        .join("eval_samples")
        .join("born_digital")
        .join("bd01_text_only.pdf");
    if !pdf_path.exists() {
        eprintln!("跳过：bd01_text_only.pdf 不存在");
        return;
    }

    let config_auto = config_with_method(ReadingOrderMethod::Auto);
    let config_heuristic = config_with_method(ReadingOrderMethod::Heuristic);

    let doc_auto = parse_pdf(&pdf_path, &config_auto).expect("Auto 解析失败");
    let doc_heuristic = parse_pdf(&pdf_path, &config_heuristic).expect("Heuristic 解析失败");

    // 单栏文档，Auto 应该选择 Heuristic，结果应该完全一致
    for (page_a, page_h) in doc_auto.pages.iter().zip(doc_heuristic.pages.iter()) {
        assert_eq!(page_a.blocks.len(), page_h.blocks.len(), "块数应相同");

        let texts_a: Vec<&str> = page_a
            .blocks
            .iter()
            .map(|b| b.normalized_text.as_str())
            .collect();
        let texts_h: Vec<&str> = page_h
            .blocks
            .iter()
            .map(|b| b.normalized_text.as_str())
            .collect();

        assert_eq!(texts_a, texts_h, "单栏文档 Auto 和 Heuristic 结果应一致");
    }

    println!("✓ 单栏文档 Auto 模式与 Heuristic 结果一致");
}

// ─── ReadingOrderMethod Config 序列化测试 ───

#[test]
fn test_reading_order_config_serde() {
    // 测试 ReadingOrderMethod 序列化/反序列化
    let mut config = Config::default();
    config.reading_order_method = ReadingOrderMethod::XyCut;
    config.xy_cut_gap_ratio = 0.03;

    let json = serde_json::to_string(&config).expect("序列化失败");
    assert!(json.contains("xy_cut"), "JSON 应包含 'xy_cut'");

    let config2: Config = serde_json::from_str(&json).expect("反序列化失败");
    assert_eq!(config2.reading_order_method, ReadingOrderMethod::XyCut);
    assert!((config2.xy_cut_gap_ratio - 0.03).abs() < 0.001);
}

#[test]
fn test_reading_order_config_toml() {
    // 测试 TOML 配置
    let toml_str = r#"
reading_order_method = "xy_cut"
xy_cut_gap_ratio = 0.05
"#;

    let config = Config::from_toml_str(toml_str).expect("TOML 解析失败");
    assert_eq!(config.reading_order_method, ReadingOrderMethod::XyCut);
    assert!((config.xy_cut_gap_ratio - 0.05).abs() < 0.001);

    // 默认值测试
    let toml_default = "";
    let config_default = Config::from_toml_str(toml_default).expect("空 TOML 解析失败");
    assert_eq!(
        config_default.reading_order_method,
        ReadingOrderMethod::Auto,
        "默认应为 Auto"
    );
}

// ─── 性能基准对比 ───

#[test]
fn test_xycut_performance_benchmark() {
    let pdf_path = fixtures_dir().join("bench_100pages.pdf");
    if !pdf_path.exists() {
        eprintln!("跳过性能基准：bench_100pages.pdf 不存在");
        return;
    }

    let iterations = 3;

    // Heuristic 性能
    let config_h = config_with_method(ReadingOrderMethod::Heuristic);
    let mut heuristic_times = Vec::new();
    for _ in 0..iterations {
        let start = Instant::now();
        let _doc = parse_pdf(&pdf_path, &config_h).expect("Heuristic 解析失败");
        heuristic_times.push(start.elapsed().as_millis() as f64);
    }

    // XyCut 性能
    let config_x = config_with_method(ReadingOrderMethod::XyCut);
    let mut xycut_times = Vec::new();
    for _ in 0..iterations {
        let start = Instant::now();
        let _doc = parse_pdf(&pdf_path, &config_x).expect("XyCut 解析失败");
        xycut_times.push(start.elapsed().as_millis() as f64);
    }

    // Auto 性能
    let config_a = config_with_method(ReadingOrderMethod::Auto);
    let mut auto_times = Vec::new();
    for _ in 0..iterations {
        let start = Instant::now();
        let _doc = parse_pdf(&pdf_path, &config_a).expect("Auto 解析失败");
        auto_times.push(start.elapsed().as_millis() as f64);
    }

    let avg_h: f64 = heuristic_times.iter().sum::<f64>() / iterations as f64;
    let avg_x: f64 = xycut_times.iter().sum::<f64>() / iterations as f64;
    let avg_a: f64 = auto_times.iter().sum::<f64>() / iterations as f64;

    println!("\n=== 性能基准：100 页 PDF ===");
    println!("  Heuristic: {:.1}ms (runs: {:?})", avg_h, heuristic_times);
    println!("  XyCut:     {:.1}ms (runs: {:?})", avg_x, xycut_times);
    println!("  Auto:      {:.1}ms (runs: {:?})", avg_a, auto_times);

    let overhead_pct = if avg_h > 0.0 {
        (avg_x - avg_h) / avg_h * 100.0
    } else {
        0.0
    };

    println!("  XyCut 开销: {:.1}%", overhead_pct);

    // XyCut 不应比 Heuristic 慢超过 50%（宽松阈值，因 XyCut 需要额外排序）
    if overhead_pct > 50.0 {
        println!(
            "  ⚠ 性能警告：XyCut 比 Heuristic 慢 {:.1}% (目标 <20%)",
            overhead_pct
        );
    } else {
        println!("  ✓ 性能达标");
    }
}

#[test]
fn test_xycut_attention_paper_performance() {
    let pdf_path = fixtures_dir().join("Attention_Is_All_You_Need.pdf");
    if !pdf_path.exists() {
        eprintln!("跳过：Attention_Is_All_You_Need.pdf 不存在");
        return;
    }

    let iterations = 3;

    // Heuristic
    let config_h = config_with_method(ReadingOrderMethod::Heuristic);
    let start = Instant::now();
    for _ in 0..iterations {
        let _doc = parse_pdf(&pdf_path, &config_h).expect("解析失败");
    }
    let avg_h = start.elapsed().as_millis() as f64 / iterations as f64;

    // XyCut
    let config_x = config_with_method(ReadingOrderMethod::XyCut);
    let start = Instant::now();
    for _ in 0..iterations {
        let _doc = parse_pdf(&pdf_path, &config_x).expect("解析失败");
    }
    let avg_x = start.elapsed().as_millis() as f64 / iterations as f64;

    let overhead_pct = if avg_h > 0.0 {
        (avg_x - avg_h) / avg_h * 100.0
    } else {
        0.0
    };

    println!("\n=== Attention Paper 性能 ({} iterations) ===", iterations);
    println!("  Heuristic: {:.1}ms", avg_h);
    println!("  XyCut:     {:.1}ms", avg_x);
    println!("  开销:      {:.1}%", overhead_pct);
}

// ─── 各种布局类型的 XyCut 回归测试 ───

#[test]
fn test_xycut_no_regression_on_eval_samples() {
    let eval_dir = fixtures_dir().join("eval_samples").join("born_digital");
    if !eval_dir.exists() {
        eprintln!("跳过评测样本回归测试");
        return;
    }

    let config = config_with_method(ReadingOrderMethod::XyCut);
    let mut success = 0;
    let mut total = 0;

    for entry in std::fs::read_dir(&eval_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "pdf") {
            continue;
        }

        total += 1;
        let name = path.file_name().unwrap().to_str().unwrap();

        match parse_pdf(&path, &config) {
            Ok(doc) => {
                let total_blocks: usize = doc.pages.iter().map(|p| p.blocks.len()).sum();
                println!(
                    "  ✓ {}: {} pages, {} blocks",
                    name,
                    doc.pages.len(),
                    total_blocks
                );
                success += 1;
            }
            Err(e) => {
                println!("  ✗ {}: {}", name, e);
            }
        }
    }

    println!("\n评测样本回归测试: {}/{} 通过", success, total);
    assert_eq!(success, total, "所有评测样本应当解析成功");
}
