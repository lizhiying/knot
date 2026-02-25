//! knot-pdf 基准测试
//!
//! 使用 criterion 框架，测量核心操作的性能。

use criterion::{criterion_group, criterion_main, Criterion};

use knot_pdf::ir::*;
use knot_pdf::Config;

/// 构造一个模拟 PageIR
fn make_sample_page(index: usize, block_count: usize) -> PageIR {
    let mut blocks = Vec::with_capacity(block_count);
    for i in 0..block_count {
        blocks.push(BlockIR {
            block_id: format!("p{}_{}", index, i),
            bbox: BBox::new(50.0, 50.0 + i as f32 * 15.0, 400.0, 12.0),
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: format!(
                        "This is block {} on page {}. 这是第 {} 页的第 {} 块。",
                        i, index, index, i
                    ),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: Some("SimSun".to_string()),
                }],
                bbox: Some(BBox::new(50.0, 50.0 + i as f32 * 15.0, 400.0, 12.0)),
            }],
            normalized_text: format!(
                "This is block {} on page {}. 这是第 {} 页的第 {} 块。",
                i, index, index, i
            ),
        });
    }

    PageIR {
        page_index: index,
        size: PageSize {
            width: 595.0,
            height: 842.0,
        },
        rotation: 0.0,
        blocks,
        tables: vec![],
        images: vec![],
        diagnostics: PageDiagnostics::default(),
        text_score: 0.95,
        is_scanned_guess: false,
        source: PageSource::BornDigital,
        timings: Timings {
            extract_ms: Some(10),
            render_ms: None,
            ocr_ms: None,
            ..Default::default()
        },
    }
}

/// 基准：构建 DocumentIR（模拟 100 页）
fn bench_build_document_ir(c: &mut Criterion) {
    c.bench_function("build_100_page_document_ir", |b| {
        b.iter(|| {
            let pages: Vec<PageIR> = (0..100).map(|i| make_sample_page(i, 20)).collect();
            let doc = DocumentIR {
                doc_id: "bench_doc".to_string(),
                metadata: DocumentMetadata::default(),
                outline: vec![],
                pages,
                diagnostics: Diagnostics::default(),
            };
            criterion::black_box(doc);
        });
    });
}

/// 基准：PageIR 序列化/反序列化
fn bench_page_ir_serde(c: &mut Criterion) {
    let page = make_sample_page(0, 30);

    c.bench_function("page_ir_serialize", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&page).unwrap();
            criterion::black_box(json);
        });
    });

    let json = serde_json::to_string(&page).unwrap();
    c.bench_function("page_ir_deserialize", |b| {
        b.iter(|| {
            let p: PageIR = serde_json::from_str(&json).unwrap();
            criterion::black_box(p);
        });
    });
}

/// 基准：Config 序列化/反序列化
fn bench_config_serde(c: &mut Criterion) {
    let config = Config::default();

    c.bench_function("config_serialize", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&config).unwrap();
            criterion::black_box(json);
        });
    });
}

/// 基准：OCR 触发条件检查
fn bench_ocr_trigger_check(c: &mut Criterion) {
    let mut config = Config::default();
    config.ocr_enabled = true;
    config.ocr_mode = knot_pdf::config::OcrMode::Auto;

    let page = make_sample_page(0, 20);

    c.bench_function("ocr_trigger_check", |b| {
        b.iter(|| {
            let result = knot_pdf::ocr::should_trigger_ocr(&page, &config);
            criterion::black_box(result);
        });
    });
}

/// 基准：PageScore 计算（模拟字符集）
fn bench_page_score(c: &mut Criterion) {
    use knot_pdf::scoring::compute_page_score;

    // 创建模拟字符
    let chars: Vec<knot_pdf::backend::RawChar> = (0..500)
        .map(|i| knot_pdf::backend::RawChar {
            unicode: if i % 5 == 0 { '中' } else { 'A' },
            bbox: BBox::new(
                50.0 + (i % 40) as f32 * 12.0,
                50.0 + (i / 40) as f32 * 15.0,
                10.0,
                12.0,
            ),
            font_size: 12.0,
            font_name: None,
            is_bold: false,
        })
        .collect();

    let config = Config::default();

    c.bench_function("page_score_500_chars", |b| {
        b.iter(|| {
            let score = compute_page_score(&chars, 595.0, 842.0, &config);
            criterion::black_box(score);
        });
    });
}

/// 基准：Markdown 渲染
fn bench_markdown_render(c: &mut Criterion) {
    use knot_pdf::MarkdownRenderer;

    let pages: Vec<PageIR> = (0..10).map(|i| make_sample_page(i, 15)).collect();
    let doc = DocumentIR {
        doc_id: "bench_md".to_string(),
        metadata: DocumentMetadata::default(),
        outline: vec![],
        pages,
        diagnostics: Diagnostics::default(),
    };

    let renderer = MarkdownRenderer::new();

    c.bench_function("markdown_render_10_pages", |b| {
        b.iter(|| {
            let md = renderer.render_document(&doc);
            criterion::black_box(md);
        });
    });
}

criterion_group!(
    benches,
    bench_build_document_ir,
    bench_page_ir_serde,
    bench_config_serde,
    bench_ocr_trigger_check,
    bench_page_score,
    bench_markdown_render,
);
criterion_main!(benches);
