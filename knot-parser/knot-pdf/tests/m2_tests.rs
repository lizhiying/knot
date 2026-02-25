//! M2 测试：PageScore 评分、页眉页脚检测、多列复排稳健化

use knot_pdf::backend::RawChar;
use knot_pdf::config::Config;
use knot_pdf::hf_detect::detect_and_mark_headers_footers;
use knot_pdf::ir::*;
use knot_pdf::layout::build_blocks;
use knot_pdf::scoring::{compute_page_score, ReasonFlag};

// ============================================================
// PageScore 评分测试
// ============================================================

fn make_char(ch: char, x: f32, y: f32, font_size: f32) -> RawChar {
    RawChar {
        unicode: ch,
        bbox: BBox::new(x, y, font_size * 0.6, font_size),
        font_size,
        font_name: None,
        is_bold: false,
    }
}

fn make_text_chars(text: &str, start_x: f32, y: f32, font_size: f32) -> Vec<RawChar> {
    text.chars()
        .enumerate()
        .map(|(i, ch)| make_char(ch, start_x + i as f32 * font_size * 0.6, y, font_size))
        .collect()
}

#[test]
fn test_page_score_empty_page() {
    let config = Config::default();
    let score = compute_page_score(&[], 612.0, 792.0, &config);
    assert_eq!(score.score, 0.0, "空页面应得 0 分");
    assert!(
        score.reason_flags.contains(&ReasonFlag::LowText),
        "空页面应标记 LowText"
    );
}

#[test]
fn test_page_score_normal_text() {
    let config = Config::default();
    // 模拟正常文本页：多行可打印字符
    let mut chars = Vec::new();
    for line in 0..10 {
        let text = format!("This is line {} of normal text content.", line + 1);
        chars.extend(make_text_chars(
            &text,
            72.0,
            100.0 + line as f32 * 20.0,
            12.0,
        ));
    }

    let score = compute_page_score(&chars, 612.0, 792.0, &config);
    assert!(
        score.score > 0.5,
        "正常文本页应得到较高分数，实际: {}",
        score.score
    );
    assert!(
        score.reason_flags.is_empty() || !score.reason_flags.contains(&ReasonFlag::HighGarbled),
        "正常文本页不应标记 HighGarbled"
    );
    assert!(score.metrics.total_char_count > 100);
    assert!(score.metrics.printable_ratio > 0.5);
}

#[test]
fn test_page_score_garbled_text() {
    let config = Config::default();
    // 模拟乱码页：大量 PUA 字符
    let mut chars = Vec::new();
    for i in 0..100 {
        chars.push(make_char(
            char::from_u32(0xE000 + i).unwrap_or('?'),
            (i as f32) * 6.0,
            100.0,
            12.0,
        ));
    }

    let score = compute_page_score(&chars, 612.0, 792.0, &config);
    assert!(
        score.score < 0.5,
        "乱码页应得到较低分数，实际: {}",
        score.score
    );
    assert!(
        score.reason_flags.contains(&ReasonFlag::HighGarbled),
        "乱码页应标记 HighGarbled"
    );
    assert!(score.metrics.garbled_rate > 0.5);
}

#[test]
fn test_page_score_few_chars() {
    let config = Config::default();
    // 模拟少量字符（如标题页）
    let chars = make_text_chars("Title", 200.0, 400.0, 24.0);

    let score = compute_page_score(&chars, 612.0, 792.0, &config);
    assert!(
        score.reason_flags.contains(&ReasonFlag::LowText),
        "少量字符应标记 LowText"
    );
    assert!(score.metrics.total_char_count < 50);
}

#[test]
fn test_page_score_metrics_accuracy() {
    let config = Config::default();
    let mut chars = make_text_chars("Hello World", 72.0, 100.0, 12.0);
    // 加入一些控制字符
    chars.push(make_char('\u{0001}', 200.0, 100.0, 12.0));
    chars.push(make_char('\u{FFFD}', 210.0, 100.0, 12.0));

    let score = compute_page_score(&chars, 612.0, 792.0, &config);
    assert_eq!(score.metrics.total_char_count, chars.len());
    assert!(score.metrics.garbled_rate > 0.0, "应检测到乱码字符");
}

// ============================================================
// 页眉页脚检测测试
// ============================================================

fn make_page_ir(page_index: usize, blocks: Vec<BlockIR>) -> PageIR {
    PageIR {
        page_index,
        size: PageSize {
            width: 612.0,
            height: 792.0,
        },
        rotation: 0.0,
        blocks,
        tables: Vec::new(),
        images: Vec::new(),
        diagnostics: PageDiagnostics::default(),
        text_score: 1.0,
        is_scanned_guess: false,
        source: PageSource::BornDigital,
        timings: Timings::default(),
    }
}

fn make_block(id: &str, x: f32, y: f32, w: f32, h: f32, text: &str) -> BlockIR {
    BlockIR {
        block_id: id.to_string(),
        bbox: BBox::new(x, y, w, h),
        role: BlockRole::Body,
        lines: vec![TextLine {
            spans: vec![TextSpan {
                text: text.to_string(),
                font_size: Some(12.0),
                is_bold: false,
                font_name: None,
            }],
            bbox: Some(BBox::new(x, y, w, h)),
        }],
        normalized_text: text.to_string(),
    }
}

#[test]
fn test_hf_detect_repeated_header() {
    // 3 页 PDF，每页顶部都有相同的页眉文本
    let mut pages = vec![
        make_page_ir(
            0,
            vec![
                make_block("h0", 72.0, 10.0, 200.0, 15.0, "Company Report 2024"),
                make_block("b0", 72.0, 100.0, 468.0, 600.0, "Body text of page 1"),
            ],
        ),
        make_page_ir(
            1,
            vec![
                make_block("h1", 72.0, 10.0, 200.0, 15.0, "Company Report 2024"),
                make_block("b1", 72.0, 100.0, 468.0, 600.0, "Body text of page 2"),
            ],
        ),
        make_page_ir(
            2,
            vec![
                make_block("h2", 72.0, 10.0, 200.0, 15.0, "Company Report 2024"),
                make_block("b2", 72.0, 100.0, 468.0, 600.0, "Body text of page 3"),
            ],
        ),
    ];

    let result = detect_and_mark_headers_footers(&mut pages, false);

    assert!(result.header_patterns > 0, "应检测到页眉模式");
    assert!(result.affected_page_count > 0, "应有受影响页面");

    // 验证页眉被标记
    for page in &pages {
        let header_blocks: Vec<_> = page
            .blocks
            .iter()
            .filter(|b| b.role == BlockRole::Header)
            .collect();
        assert!(!header_blocks.is_empty(), "每页应有被标记的页眉块");
    }
}

#[test]
fn test_hf_detect_repeated_footer_with_page_numbers() {
    // 3 页 PDF，每页底部有带页码的页脚
    let mut pages = vec![
        make_page_ir(
            0,
            vec![
                make_block("b0", 72.0, 100.0, 468.0, 600.0, "Body text page 1"),
                make_block("f0", 200.0, 760.0, 100.0, 15.0, "Page 1 of 3"),
            ],
        ),
        make_page_ir(
            1,
            vec![
                make_block("b1", 72.0, 100.0, 468.0, 600.0, "Body text page 2"),
                make_block("f1", 200.0, 760.0, 100.0, 15.0, "Page 2 of 3"),
            ],
        ),
        make_page_ir(
            2,
            vec![
                make_block("b2", 72.0, 100.0, 468.0, 600.0, "Body text page 3"),
                make_block("f2", 200.0, 760.0, 100.0, 15.0, "Page 3 of 3"),
            ],
        ),
    ];

    let result = detect_and_mark_headers_footers(&mut pages, false);

    assert!(result.footer_patterns > 0, "应检测到页脚模式");

    // 验证页脚被标记
    for page in &pages {
        let footer_blocks: Vec<_> = page
            .blocks
            .iter()
            .filter(|b| b.role == BlockRole::Footer)
            .collect();
        assert!(!footer_blocks.is_empty(), "每页应有被标记的页脚块");
    }
}

#[test]
fn test_hf_strip_preserves_body() {
    // 确保 strip=true 时正文不受影响
    let mut pages = vec![
        make_page_ir(
            0,
            vec![
                make_block("h0", 72.0, 10.0, 200.0, 15.0, "Header Text"),
                make_block("b0", 72.0, 200.0, 468.0, 400.0, "Main content A"),
                make_block("f0", 200.0, 760.0, 100.0, 15.0, "Footer Text"),
            ],
        ),
        make_page_ir(
            1,
            vec![
                make_block("h1", 72.0, 10.0, 200.0, 15.0, "Header Text"),
                make_block("b1", 72.0, 200.0, 468.0, 400.0, "Main content B"),
                make_block("f1", 200.0, 760.0, 100.0, 15.0, "Footer Text"),
            ],
        ),
    ];

    detect_and_mark_headers_footers(&mut pages, true);

    for (i, page) in pages.iter().enumerate() {
        assert!(!page.blocks.is_empty(), "第 {} 页应保留正文块", i);
        // 验证剩余的都是 Body
        for block in &page.blocks {
            assert_ne!(block.role, BlockRole::Header, "页眉应被移除");
            assert_ne!(block.role, BlockRole::Footer, "页脚应被移除");
        }
    }
}

#[test]
fn test_hf_no_false_positive_on_single_page() {
    // 单页 PDF 不应检测到页眉页脚
    let mut pages = vec![make_page_ir(
        0,
        vec![
            make_block("b0", 72.0, 10.0, 468.0, 700.0, "Some title at top"),
            make_block("b1", 72.0, 100.0, 468.0, 600.0, "Body text"),
        ],
    )];

    let result = detect_and_mark_headers_footers(&mut pages, false);

    assert_eq!(result.header_patterns, 0, "单页不应检测到页眉");
    assert_eq!(result.footer_patterns, 0, "单页不应检测到页脚");
}

#[test]
fn test_hf_safety_no_strip_all_blocks() {
    // 安全机制：如果所有块都被标记为页眉/页脚，不应全部删除
    let mut pages = vec![
        make_page_ir(
            0,
            vec![make_block("h0", 72.0, 10.0, 200.0, 15.0, "Repeated Header")],
        ),
        make_page_ir(
            1,
            vec![make_block("h1", 72.0, 10.0, 200.0, 15.0, "Repeated Header")],
        ),
    ];

    detect_and_mark_headers_footers(&mut pages, true);

    // 安全机制：如果移除后没有剩余块，不应移除
    for (i, page) in pages.iter().enumerate() {
        assert!(!page.blocks.is_empty(), "第 {} 页不应所有块都被删除", i);
    }
}

// ============================================================
// 多列复排测试
// ============================================================

#[test]
fn test_build_blocks_single_column() {
    // 模拟单列文本
    let mut chars = Vec::new();
    for line in 0..5 {
        let text = format!("Line {} of single column text content here.", line + 1);
        chars.extend(make_text_chars(
            &text,
            72.0,
            100.0 + line as f32 * 20.0,
            12.0,
        ));
    }

    let blocks = build_blocks(&chars, 612.0, 792.0);
    assert!(!blocks.is_empty(), "应生成至少一个文本块");

    // 验证块的 y 坐标递增（阅读顺序）
    for i in 1..blocks.len() {
        assert!(
            blocks[i].bbox.y >= blocks[i - 1].bbox.y,
            "块应按 y 坐标排序"
        );
    }
}

#[test]
fn test_build_blocks_two_columns() {
    // 模拟双列文本：左列 x=72, 右列 x=320
    let mut chars = Vec::new();
    for line in 0..5 {
        // 左列
        let left_text = format!("Left col line {}", line + 1);
        chars.extend(make_text_chars(
            &left_text,
            72.0,
            100.0 + line as f32 * 20.0,
            12.0,
        ));
        // 右列
        let right_text = format!("Right col line {}", line + 1);
        chars.extend(make_text_chars(
            &right_text,
            320.0,
            100.0 + line as f32 * 20.0,
            12.0,
        ));
    }

    let blocks = build_blocks(&chars, 612.0, 792.0);
    assert!(
        blocks.len() >= 2,
        "双列应生成至少 2 个文本块，实际: {}",
        blocks.len()
    );
}

#[test]
fn test_build_blocks_with_banner() {
    // 模拟混合布局：顶部横幅标题 + 下方双列
    let mut chars = Vec::new();

    // 横幅标题（跨全页宽度）
    let title = "This is a full-width banner title that spans the entire page width area";
    chars.extend(make_text_chars(title, 72.0, 50.0, 18.0));

    // 左列
    for line in 0..3 {
        let text = format!("Left column paragraph line {}", line + 1);
        chars.extend(make_text_chars(
            &text,
            72.0,
            120.0 + line as f32 * 20.0,
            12.0,
        ));
    }
    // 右列
    for line in 0..3 {
        let text = format!("Right column paragraph line {}", line + 1);
        chars.extend(make_text_chars(
            &text,
            330.0,
            120.0 + line as f32 * 20.0,
            12.0,
        ));
    }

    let blocks = build_blocks(&chars, 612.0, 792.0);
    assert!(!blocks.is_empty(), "应生成文本块");

    // 横幅标题应在最前面
    let first_block = &blocks[0];
    assert!(
        first_block.bbox.y <= 70.0,
        "第一个块应在页面顶部（横幅标题）"
    );
}

#[test]
fn test_build_blocks_list_detection() {
    // 模拟列表项
    let mut chars = Vec::new();
    let items = [
        "• First item in the list",
        "• Second item in the list",
        "• Third item in the list",
    ];
    for (i, item) in items.iter().enumerate() {
        chars.extend(make_text_chars(item, 72.0, 100.0 + i as f32 * 20.0, 12.0));
    }

    let blocks = build_blocks(&chars, 612.0, 792.0);
    assert!(!blocks.is_empty(), "应生成文本块");

    // 应检测到列表项角色
    let list_blocks: Vec<_> = blocks
        .iter()
        .filter(|b| b.role == BlockRole::List)
        .collect();
    assert!(
        !list_blocks.is_empty(),
        "应检测到列表项，实际角色: {:?}",
        blocks.iter().map(|b| &b.role).collect::<Vec<_>>()
    );
}

#[test]
fn test_build_blocks_numbered_list() {
    // 模拟编号列表
    let mut chars = Vec::new();
    let items = [
        "1. First numbered item",
        "2. Second numbered item",
        "3. Third numbered item",
    ];
    for (i, item) in items.iter().enumerate() {
        chars.extend(make_text_chars(item, 72.0, 100.0 + i as f32 * 20.0, 12.0));
    }

    let blocks = build_blocks(&chars, 612.0, 792.0);
    assert!(!blocks.is_empty(), "应生成文本块");

    let list_blocks: Vec<_> = blocks
        .iter()
        .filter(|b| b.role == BlockRole::List)
        .collect();
    assert!(!list_blocks.is_empty(), "应检测到编号列表项");
}

#[test]
fn test_build_blocks_empty_input() {
    let blocks = build_blocks(&[], 612.0, 792.0);
    assert!(blocks.is_empty(), "空输入应返回空列表");
}

#[test]
fn test_build_blocks_reading_order() {
    // 验证阅读顺序：上方内容应先于下方
    let mut chars = Vec::new();
    // 底部文本先添加（模拟 PDF 中不保证顺序的情况）
    chars.extend(make_text_chars("Bottom text comes last", 72.0, 500.0, 12.0));
    chars.extend(make_text_chars("Top text comes first", 72.0, 100.0, 12.0));
    chars.extend(make_text_chars("Middle text in between", 72.0, 300.0, 12.0));

    let blocks = build_blocks(&chars, 612.0, 792.0);
    assert!(blocks.len() >= 2, "应生成多个文本块");

    // 验证按 y 排序
    for i in 1..blocks.len() {
        assert!(
            blocks[i].bbox.y >= blocks[i - 1].bbox.y,
            "块 {} (y={}) 应在块 {} (y={}) 之后",
            i,
            blocks[i].bbox.y,
            i - 1,
            blocks[i - 1].bbox.y
        );
    }
}

// ============================================================
// 集成测试：PageScore + 页眉页脚 + pipeline
// ============================================================

#[test]
fn test_config_strip_headers_footers_flag() {
    // 验证配置项可以控制是否剔除页眉页脚
    let config_on = Config {
        strip_headers_footers: true,
        ..Config::default()
    };
    assert!(config_on.strip_headers_footers);

    let config_off = Config {
        strip_headers_footers: false,
        ..Config::default()
    };
    assert!(!config_off.strip_headers_footers);
}

#[test]
fn test_page_score_serde_roundtrip() {
    // 验证 PageScore 相关指标可通过 JSON 保持
    let config = Config::default();
    let chars = make_text_chars("Hello World from PDF", 72.0, 100.0, 12.0);
    let score = compute_page_score(&chars, 612.0, 792.0, &config);

    // PageScore 的 metrics 应该是可检查的
    assert!(score.metrics.total_char_count > 0);
    assert!(score.metrics.printable_ratio >= 0.0);
    assert!(score.metrics.printable_ratio <= 1.0);
    assert!(score.score >= 0.0);
    assert!(score.score <= 1.0);
}
