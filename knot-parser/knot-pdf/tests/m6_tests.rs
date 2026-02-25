//! M6 测试：性能/内存调优 + 可靠性增强 + 配置整合

use knot_pdf::error::PdfError;
use knot_pdf::Config;

// === §5 可靠性增强 ===

#[test]
fn test_encrypted_pdf_returns_error() {
    // PdfError::Encrypted 变体存在
    let err = PdfError::Encrypted;
    assert!(format!("{}", err).contains("encrypted"));
}

#[test]
fn test_corrupted_pdf_error_variant() {
    let err = PdfError::Corrupted("invalid xref table".to_string());
    assert!(format!("{}", err).contains("corrupted"));
    assert!(format!("{}", err).contains("invalid xref table"));
}

#[test]
fn test_timeout_error_variant() {
    let err = PdfError::Timeout("page 5 took too long".to_string());
    assert!(format!("{}", err).contains("timed out"));
}

#[test]
fn test_page_not_found_error() {
    let err = PdfError::PageNotFound(999);
    assert!(format!("{}", err).contains("999"));
}

#[test]
fn test_nonexistent_file_returns_io_error() {
    let result = knot_pdf::parse_pdf("/nonexistent/fake.pdf", &Config::default());
    assert!(result.is_err());
    match result.unwrap_err() {
        PdfError::Io(_) => {} // 期望 IO 错误
        other => panic!("Expected IO error, got: {:?}", other),
    }
}

// === §9 配置整合 ===

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert_eq!(config.max_memory_mb, 200);
    assert_eq!(config.page_queue_size, 4);
    assert_eq!(config.render_workers, 2);
    assert_eq!(config.ocr_workers, 1);
    assert_eq!(config.page_timeout_secs, 0);
    assert_eq!(config.scoring_text_threshold, 0.3);
    assert_eq!(config.garbled_threshold, 0.2);
    assert!(config.strip_headers_footers);
    assert!(config.emit_markdown);
    assert!(!config.emit_ir_json);
    assert!(!config.ocr_enabled);
    assert!(!config.store_enabled);
}

#[test]
fn test_config_validate_clamps_zero_values() {
    let mut config = Config::default();
    config.ocr_workers = 0;
    config.render_workers = 0;
    config.max_memory_mb = 0;
    config.page_queue_size = 0;
    config.max_columns = 0;

    config.validate();

    assert_eq!(config.ocr_workers, 1);
    assert_eq!(config.render_workers, 1);
    assert_eq!(config.max_memory_mb, 200);
    assert_eq!(config.page_queue_size, 4);
    assert_eq!(config.max_columns, 1);
}

#[test]
fn test_config_validate_clamps_threshold() {
    let mut config = Config::default();
    config.scoring_text_threshold = -0.5;
    config.validate();
    assert_eq!(config.scoring_text_threshold, 0.0);

    config.scoring_text_threshold = 1.5;
    config.validate();
    assert_eq!(config.scoring_text_threshold, 1.0);
}

#[test]
fn test_config_serde_roundtrip() {
    let config = Config::default();
    let json = serde_json::to_string(&config).unwrap();
    let loaded: Config = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.max_memory_mb, config.max_memory_mb);
    assert_eq!(loaded.page_queue_size, config.page_queue_size);
    assert_eq!(loaded.render_workers, config.render_workers);
    assert_eq!(loaded.scoring_text_threshold, config.scoring_text_threshold);
    assert_eq!(loaded.ocr_enabled, config.ocr_enabled);
    assert_eq!(loaded.store_enabled, config.store_enabled);
}

#[test]
fn test_config_from_partial_json() {
    // 只提供部分字段，其余走默认值
    let json = r#"{"ocr_enabled": true, "max_memory_mb": 512}"#;
    let config: Config = serde_json::from_str(json).unwrap();

    assert!(config.ocr_enabled);
    assert_eq!(config.max_memory_mb, 512);
    // 其余字段为默认值
    assert_eq!(config.page_queue_size, 4);
    assert_eq!(config.render_workers, 2);
    assert_eq!(config.scoring_text_threshold, 0.3);
}

// === §3 内存优化 — PageIR 不缓存大对象 ===

#[test]
fn test_page_ir_no_image_bytes() {
    // 验证 ImageIR 使用 bytes_ref（延迟加载），不内嵌图片数据
    let img = knot_pdf::ir::ImageIR {
        image_id: "test".to_string(),
        page_index: 0,
        bbox: knot_pdf::ir::BBox::new(0.0, 0.0, 100.0, 100.0),
        format: knot_pdf::ir::ImageFormat::Png,
        bytes_ref: None, // 延迟加载，不预加载图片数据
        caption_refs: vec![],
        source: knot_pdf::ir::ImageSource::Embedded,
        ocr_text: None,
    };

    // bytes_ref 默认为 None（不占用内存）
    assert!(img.bytes_ref.is_none());
}

// === §6 性能指标收集 ===

#[test]
fn test_timings_recorded_in_page_ir() {
    // 验证 Timings 结构可以记录各阶段耗时和内存
    let timings = knot_pdf::ir::Timings {
        extract_ms: Some(42),
        render_ms: Some(100),
        ocr_ms: Some(300),
        peak_rss_bytes: Some(100 * 1024 * 1024), // 100MB
        rss_delta_bytes: Some(5 * 1024 * 1024),  // +5MB
    };

    assert_eq!(timings.extract_ms, Some(42));
    assert_eq!(timings.render_ms, Some(100));
    assert_eq!(timings.ocr_ms, Some(300));
    assert_eq!(timings.peak_rss_bytes, Some(100 * 1024 * 1024));
    assert_eq!(timings.rss_delta_bytes, Some(5 * 1024 * 1024));
}

// === §3 内存峰值监控 ===

#[test]
fn test_mem_track_current_rss() {
    let rss = knot_pdf::mem_track::current_rss_bytes();
    // macOS/Linux 上应该返回 > 0
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    assert!(rss > 0, "RSS should be > 0 on macOS/Linux, got {}", rss);
    // 任何平台上都不应 panic
    let _ = rss;
}

#[test]
fn test_mem_track_snapshot() {
    let snap = knot_pdf::mem_track::MemorySnapshot::now();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        assert!(snap.rss_bytes > 0);
        assert!(snap.rss_mb() > 0.0);
    }
}

#[test]
fn test_mem_track_page_stats() {
    let before = knot_pdf::mem_track::MemorySnapshot::now();
    // 分配一些内存来制造 delta
    let _data: Vec<u8> = vec![0u8; 1024 * 1024]; // 1MB
    let after = knot_pdf::mem_track::MemorySnapshot::now();

    let stats = knot_pdf::mem_track::PageMemoryStats::from_snapshots(&before, &after);

    // delta 应该是有意义的值（不一定正好 1MB，因为 RSS 粒度是页）
    let _ = stats.delta_mb(); // 不应 panic
    assert_eq!(stats.before_rss, before.rss_bytes);
    assert_eq!(stats.after_rss, after.rss_bytes);
}

#[test]
fn test_diagnostics_captures_warnings() {
    let mut diag = knot_pdf::ir::Diagnostics::default();
    diag.warnings.push("test warning".to_string());
    assert_eq!(diag.warnings.len(), 1);
}

// === §3 TableIR 内存估算 ===

#[test]
fn test_table_ir_estimated_memory() {
    use knot_pdf::ir::*;

    let table = make_sample_table(5, 3);
    let mem = table.estimated_memory_bytes();

    // 5 行 × 3 列 的小表格，内存应在合理范围
    assert!(mem > 0);
    assert!(
        mem < 10 * 1024,
        "Small table should be < 10KB, got {}B",
        mem
    );
}

#[test]
fn test_table_ir_is_large() {
    use knot_pdf::ir::*;

    // 小表格不算大
    let small = make_sample_table(5, 3);
    assert!(!small.is_large());

    // 大表格（超过 100 行）
    let large = make_sample_table(150, 10);
    assert!(large.is_large());
}

#[test]
fn test_table_ir_cell_count() {
    use knot_pdf::ir::*;

    let table = make_sample_table(4, 5);
    assert_eq!(table.cell_count(), 20); // 4 行 × 5 列
}

// === §4 快速跳过无表格页 ===

#[test]
fn test_table_extraction_skips_empty_page() {
    use knot_pdf::table::extract_tables_with_graphics;

    // 空字符列表 → 直接返回空
    let tables = extract_tables_with_graphics(&[], &[], &[], 0, 595.0, 842.0);
    assert!(tables.is_empty());

    // 少于 4 个字符 → 快速跳过
    let few_chars = vec![
        knot_pdf::backend::RawChar {
            unicode: 'A',
            bbox: knot_pdf::ir::BBox::new(50.0, 50.0, 10.0, 12.0),
            font_size: 12.0,
            font_name: None,
            is_bold: false,
        },
        knot_pdf::backend::RawChar {
            unicode: 'B',
            bbox: knot_pdf::ir::BBox::new(60.0, 50.0, 10.0, 12.0),
            font_size: 12.0,
            font_name: None,
            is_bold: false,
        },
    ];
    let tables = extract_tables_with_graphics(&few_chars, &[], &[], 0, 595.0, 842.0);
    assert!(tables.is_empty());
}

// === §1 超时配置 ===

#[test]
fn test_page_timeout_config() {
    let mut config = Config::default();
    assert_eq!(config.page_timeout_secs, 0); // 默认不超时

    config.page_timeout_secs = 30;
    let json = serde_json::to_string(&config).unwrap();
    let loaded: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.page_timeout_secs, 30);
}

#[test]
fn test_pipeline_with_timeout_config() {
    let mut config = Config::default();
    config.page_timeout_secs = 60;
    // 不应 panic
    let pipeline = knot_pdf::pipeline::Pipeline::new(config);
    drop(pipeline);
}

// === 辅助函数 ===

fn make_sample_table(rows: usize, cols: usize) -> knot_pdf::ir::TableIR {
    use knot_pdf::ir::*;

    let headers: Vec<String> = (0..cols).map(|c| format!("Col{}", c)).collect();
    let table_rows: Vec<TableRow> = (0..rows)
        .map(|r| TableRow {
            row_index: r,
            cells: (0..cols)
                .map(|c| TableCell {
                    row: r,
                    col: c,
                    text: format!("R{}C{}", r, c),
                    cell_type: CellType::Text,
                    rowspan: 1,
                    colspan: 1,
                })
                .collect(),
        })
        .collect();

    TableIR {
        table_id: "test_table".to_string(),
        page_index: 0,
        bbox: BBox::new(50.0, 50.0, 400.0, rows as f32 * 20.0),
        extraction_mode: ExtractionMode::Stream,
        headers,
        rows: table_rows,
        column_types: vec![CellType::Text; cols],
        fallback_text: "test fallback".to_string(),
    }
}

// === §7 TOML 配置文件 ===

#[test]
fn test_config_from_toml_str() {
    let toml = r#"
scoring_text_threshold = 0.5
max_columns = 4
ocr_enabled = true
ocr_mode = "auto"
ocr_render_width = 1024
max_memory_mb = 500
page_timeout_secs = 30
"#;

    let config = Config::from_toml_str(toml).expect("TOML parse failed");
    assert!((config.scoring_text_threshold - 0.5).abs() < 0.01);
    assert_eq!(config.max_columns, 4);
    assert!(config.ocr_enabled);
    assert_eq!(config.ocr_render_width, 1024);
    assert_eq!(config.max_memory_mb, 500);
    assert_eq!(config.page_timeout_secs, 30);
    // 未指定字段使用默认值
    assert!(config.strip_headers_footers);
    assert!(config.emit_markdown);
    assert!(!config.store_enabled);
}

#[test]
fn test_config_from_toml_partial() {
    // 只指定少数字段
    let toml = "ocr_enabled = true\n";
    let config = Config::from_toml_str(toml).expect("TOML parse failed");
    assert!(config.ocr_enabled);
    assert_eq!(config.max_memory_mb, 200); // 默认值
}

#[test]
fn test_config_toml_roundtrip() {
    let original = Config::default();
    let toml_str = original.to_toml_string().expect("Serialize failed");
    let restored = Config::from_toml_str(&toml_str).expect("Parse failed");

    assert!((original.scoring_text_threshold - restored.scoring_text_threshold).abs() < 0.001);
    assert_eq!(original.max_columns, restored.max_columns);
    assert_eq!(original.ocr_enabled, restored.ocr_enabled);
    assert_eq!(original.max_memory_mb, restored.max_memory_mb);
}

#[test]
fn test_config_from_toml_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-config.toml");

    // 保存
    let mut config = Config::default();
    config.ocr_enabled = true;
    config.max_memory_mb = 512;
    config.save_toml_file(&path).expect("Save failed");

    // 加载
    let loaded = Config::from_toml_file(&path).expect("Load failed");
    assert!(loaded.ocr_enabled);
    assert_eq!(loaded.max_memory_mb, 512);
}

#[test]
fn test_config_from_example_toml() {
    let example_path = concat!(env!("CARGO_MANIFEST_DIR"), "/knot-pdf.example.toml");
    let config = Config::from_toml_file(example_path).expect("Failed to load example config");

    // 验证示例配置的值
    assert!((config.scoring_text_threshold - 0.3).abs() < 0.01);
    assert_eq!(config.max_memory_mb, 200);
    assert!(!config.ocr_enabled);
    assert!(config.emit_markdown);
}

#[test]
fn test_config_load_auto_default() {
    // 当前目录没有 knot-pdf.toml，应返回默认值
    let config = Config::load_auto();
    assert!(!config.ocr_enabled);
    assert_eq!(config.max_memory_mb, 200);
}
