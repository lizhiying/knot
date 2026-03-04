//! M4 测试：Ruled 表格抽取
//!
//! 测试内容：
//! - 线段归一化（合并/过滤/对齐）
//! - 网格生成（交叉点检测/cell 矩阵）
//! - 合并单元格检测
//! - 文本投影到 cell
//! - ruled vs stream 自动切换逻辑
//! - 降级链路（ruled → stream → fallback_text）

use knot_pdf::backend::{LineOrientation, Point, RawChar, RawLine, RawRect};
use knot_pdf::ir::*;
use knot_pdf::table;

// ─── 辅助函数 ───

fn make_h_line(x1: f32, x2: f32, y: f32, width: f32) -> RawLine {
    RawLine {
        start: Point { x: x1, y },
        end: Point { x: x2, y },
        width,
        orientation: LineOrientation::Horizontal,
    }
}

fn make_v_line(x: f32, y1: f32, y2: f32, width: f32) -> RawLine {
    RawLine {
        start: Point { x, y: y1 },
        end: Point { x, y: y2 },
        width,
        orientation: LineOrientation::Vertical,
    }
}

fn make_char(unicode: char, x: f32, y: f32, w: f32, h: f32) -> RawChar {
    RawChar {
        unicode,
        bbox: BBox::new(x, y, w, h),
        font_size: h,
        font_name: None,
        is_bold: false,
    }
}

fn make_bold_char(unicode: char, x: f32, y: f32, w: f32, h: f32) -> RawChar {
    RawChar {
        unicode,
        bbox: BBox::new(x, y, w, h),
        font_size: h,
        font_name: None,
        is_bold: true,
    }
}

fn make_text_chars(text: &str, x_start: f32, y: f32, char_w: f32, char_h: f32) -> Vec<RawChar> {
    text.chars()
        .enumerate()
        .map(|(i, c)| make_char(c, x_start + i as f32 * char_w, y, char_w, char_h))
        .collect()
}

fn make_bold_text_chars(
    text: &str,
    x_start: f32,
    y: f32,
    char_w: f32,
    char_h: f32,
) -> Vec<RawChar> {
    text.chars()
        .enumerate()
        .map(|(i, c)| make_bold_char(c, x_start + i as f32 * char_w, y, char_w, char_h))
        .collect()
}

// ─── 线段预处理测试 ───

#[test]
fn test_has_enough_lines_minimal() {
    // 2条水平 + 2条垂直 = 4 条，刚好达到阈值
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 50.0, 1.0),
        make_v_line(10.0, 10.0, 50.0, 1.0),
        make_v_line(310.0, 10.0, 50.0, 1.0),
    ];
    assert!(table::ruled::has_enough_lines(&lines, &[]));
}

#[test]
fn test_has_enough_lines_insufficient() {
    // 只有水平线，没有垂直线
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 50.0, 1.0),
    ];
    assert!(!table::ruled::has_enough_lines(&lines, &[]));
}

#[test]
fn test_has_enough_lines_with_rects() {
    // 用窄矩形代替线段
    let rects = vec![
        RawRect {
            bbox: BBox::new(10.0, 10.0, 300.0, 1.0),
            width: 1.0,
        }, // 水平
        RawRect {
            bbox: BBox::new(10.0, 50.0, 300.0, 1.0),
            width: 1.0,
        }, // 水平
        RawRect {
            bbox: BBox::new(10.0, 10.0, 1.0, 40.0),
            width: 1.0,
        }, // 垂直
        RawRect {
            bbox: BBox::new(310.0, 10.0, 1.0, 40.0),
            width: 1.0,
        }, // 垂直
    ];
    assert!(table::ruled::has_enough_lines(&[], &rects));
}

// ─── Ruled 表格抽取测试 ───

#[test]
fn test_ruled_simple_2x2() {
    // 简单 2行2列 有线表格
    //  ┌────────┬────────┐
    //  │ Name   │ Value  │
    //  ├────────┼────────┤
    //  │ A      │ 100    │
    //  └────────┴────────┘
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 2.0), // 粗线
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_bold_text_chars("Name", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_bold_text_chars("Value", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("A", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("100", 170.0, 50.0, 8.0, 12.0));

    let result = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0");
    assert!(result.is_some(), "Should extract a 2x2 ruled table");

    let table = result.unwrap();
    assert_eq!(table.extraction_mode, ExtractionMode::Ruled);
    assert_eq!(table.headers.len(), 2);
    assert!(table.headers.contains(&"Name".to_string()));
    assert!(table.headers.contains(&"Value".to_string()));
    assert_eq!(table.rows.len(), 1);
    assert!(!table.fallback_text.is_empty());
}

#[test]
fn test_ruled_3x3() {
    // 3行3列 有线表格
    let lines = vec![
        // 水平线
        make_h_line(10.0, 460.0, 10.0, 1.0),
        make_h_line(10.0, 460.0, 40.0, 1.0),
        make_h_line(10.0, 460.0, 70.0, 1.0),
        make_h_line(10.0, 460.0, 100.0, 1.0),
        // 垂直线
        make_v_line(10.0, 10.0, 100.0, 1.0),
        make_v_line(160.0, 10.0, 100.0, 1.0),
        make_v_line(310.0, 10.0, 100.0, 1.0),
        make_v_line(460.0, 10.0, 100.0, 1.0),
    ];

    let mut chars = Vec::new();
    // 表头
    chars.extend(make_text_chars("Year", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Revenue", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Profit", 320.0, 20.0, 8.0, 12.0));
    // 数据行1
    chars.extend(make_text_chars("2022", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("1000", 170.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("200", 320.0, 50.0, 8.0, 12.0));
    // 数据行2
    chars.extend(make_text_chars("2023", 20.0, 80.0, 8.0, 12.0));
    chars.extend(make_text_chars("1500", 170.0, 80.0, 8.0, 12.0));
    chars.extend(make_text_chars("350", 320.0, 80.0, 8.0, 12.0));

    let result = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0");
    assert!(result.is_some(), "Should extract a 3x3 ruled table");

    let table = result.unwrap();
    assert_eq!(table.headers.len(), 3);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.extraction_mode, ExtractionMode::Ruled);
}

#[test]
fn test_ruled_extraction_mode_marker() {
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 1.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("Col1", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Col2", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("A", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("B", 170.0, 50.0, 8.0, 12.0));

    let result = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0");
    assert!(result.is_some());
    assert_eq!(result.unwrap().extraction_mode, ExtractionMode::Ruled);
}

// ─── 降级链路测试 ───

#[test]
fn test_fallback_to_stream_no_lines() {
    // 没有线段时，应该降级到 stream 抽取
    let mut chars = Vec::new();
    // 构造一个对齐的文本数据（stream 表格候选）
    for row in 0..5 {
        let y = 50.0 + row as f32 * 20.0;
        chars.extend(make_text_chars("Item", 20.0, y, 8.0, 12.0));
        chars.extend(make_text_chars("100", 200.0, y, 8.0, 12.0));
        chars.extend(make_text_chars("200", 350.0, y, 8.0, 12.0));
    }

    let tables = table::extract_tables_with_graphics(&chars, &[], &[], 0, 600.0, 800.0);
    // 应该以 stream 模式抽取
    for t in &tables {
        assert_ne!(t.extraction_mode, ExtractionMode::Ruled);
    }
}

#[test]
fn test_extract_tables_with_graphics_ruled_priority() {
    // 当有足够的线段时，应优先使用 ruled 抽取
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 1.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("Name", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Score", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Alice", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("95", 170.0, 50.0, 8.0, 12.0));

    let tables = table::extract_tables_with_graphics(&chars, &lines, &[], 0, 600.0, 800.0);
    assert!(!tables.is_empty(), "Should extract at least one table");
    assert_eq!(tables[0].extraction_mode, ExtractionMode::Ruled);
}

#[test]
fn test_fallback_text_always_present() {
    // 无论 ruled 还是 stream，fallback_text 必须存在
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 1.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("X", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Y", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("1", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("2", 170.0, 50.0, 8.0, 12.0));

    let tables = table::extract_tables_with_graphics(&chars, &lines, &[], 0, 600.0, 800.0);
    for table in &tables {
        assert!(
            !table.fallback_text.is_empty(),
            "fallback_text must not be empty"
        );
    }
}

// ─── CSV / KV / Markdown 导出测试 ───

#[test]
fn test_ruled_table_csv_export() {
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 2.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("Name", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Age", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Bob", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("30", 170.0, 50.0, 8.0, 12.0));

    let table = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0").unwrap();
    let csv = table.to_csv();
    assert!(csv.contains("Name"));
    assert!(csv.contains("Age"));
}

#[test]
fn test_ruled_table_kv_lines() {
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 2.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("Col1", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Col2", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("A", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("B", 170.0, 50.0, 8.0, 12.0));

    let table = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0").unwrap();
    let kv = table.to_kv_lines();
    assert!(!kv.is_empty(), "KV lines should not be empty");
}

#[test]
fn test_ruled_table_markdown() {
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 2.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("X", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Y", 170.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("1", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("2", 170.0, 50.0, 8.0, 12.0));

    let table = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0").unwrap();
    let md = table.to_markdown();
    assert!(md.contains("|"), "Markdown should contain table separators");
    assert!(
        md.contains("---"),
        "Markdown should contain header separator"
    );
}

// ─── 合并单元格测试 ───

#[test]
fn test_ruled_missing_vertical_separator() {
    // 表格中某些垂直分隔线缺失 → 应检测到 colspan
    // ┌────────────────┬────────┐
    // │ Merged Header  │ Col3   │
    // ├────────┬───────┼────────┤
    // │ A      │ B     │ C      │
    // └────────┴───────┴────────┘
    let lines = vec![
        make_h_line(10.0, 460.0, 10.0, 1.0), // 顶
        make_h_line(10.0, 460.0, 40.0, 1.0), // 表头底
        make_h_line(10.0, 460.0, 70.0, 1.0), // 底
        make_v_line(10.0, 10.0, 70.0, 1.0),  // 左
        // 160.0 处的垂直线只存在于第二行（即 y=40 到 y=70）
        make_v_line(160.0, 40.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0), // 中间（全高）
        make_v_line(460.0, 10.0, 70.0, 1.0), // 右
    ];

    let mut chars = Vec::new();
    chars.extend(make_text_chars("Merged", 20.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("Col3", 320.0, 20.0, 8.0, 12.0));
    chars.extend(make_text_chars("A", 20.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("B", 170.0, 50.0, 8.0, 12.0));
    chars.extend(make_text_chars("C", 320.0, 50.0, 8.0, 12.0));

    let result = table::ruled::extract_ruled_table(&lines, &[], &chars, 0, "t0_0");
    assert!(
        result.is_some(),
        "Should extract a ruled table with merged cells"
    );

    let table = result.unwrap();
    // 查看表头行：应该有合并的单元格
    assert!(!table.fallback_text.is_empty());
}

// ─── 边界情况测试 ───

#[test]
fn test_ruled_empty_lines() {
    let result = table::ruled::extract_ruled_table(&[], &[], &[], 0, "t0_0");
    assert!(result.is_none(), "Empty lines should return None");
}

#[test]
fn test_ruled_insufficient_grid() {
    // 只有一条水平线和一条垂直线 — 无法构成网格
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_v_line(10.0, 10.0, 100.0, 1.0),
    ];
    let result = table::ruled::extract_ruled_table(&lines, &[], &[], 0, "t0_0");
    assert!(result.is_none(), "Insufficient lines should return None");
}

#[test]
fn test_ruled_no_chars() {
    // 有网格但没有字符
    let lines = vec![
        make_h_line(10.0, 310.0, 10.0, 1.0),
        make_h_line(10.0, 310.0, 40.0, 1.0),
        make_h_line(10.0, 310.0, 70.0, 1.0),
        make_v_line(10.0, 10.0, 70.0, 1.0),
        make_v_line(160.0, 10.0, 70.0, 1.0),
        make_v_line(310.0, 10.0, 70.0, 1.0),
    ];

    let result = table::ruled::extract_ruled_table(&lines, &[], &[], 0, "t0_0");
    // 即使没有字符，有网格也应该能构建（空表格）
    if let Some(table) = result {
        assert_eq!(table.extraction_mode, ExtractionMode::Ruled);
    }
}
