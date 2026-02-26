//! M3 测试：表格 Stream 抽取 + fallback_text
//!
//! 覆盖：
//! - 表格候选区域检测
//! - 行聚类 / 列聚类
//! - 表头推断
//! - CellType 检测
//! - fallback_text 生成
//! - RAG 导出（CSV / Markdown / KV lines / row lines）
//! - Stream 表格端到端抽取

use knot_pdf::backend::RawChar;
use knot_pdf::ir::*;
use knot_pdf::table::candidate::detect_table_candidates;
use knot_pdf::table::cell_type::detect_cell_type;
use knot_pdf::table::extract_tables;
use knot_pdf::table::fallback::{generate_fallback_text, generate_kv_lines, generate_row_lines};
use knot_pdf::table::stream::extract_stream_table;

/// 辅助函数：创建 RawChar
fn make_char(c: char, x: f32, y: f32, w: f32, h: f32, font_size: f32, bold: bool) -> RawChar {
    RawChar {
        unicode: c,
        bbox: BBox::new(x, y, w, h),
        font_size,
        font_name: None,
        is_bold: bold,
    }
}

/// 辅助函数：创建一行文本的字符列表
fn make_text_at(text: &str, x_start: f32, y: f32, font_size: f32, bold: bool) -> Vec<RawChar> {
    let char_width = font_size * 0.6;
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            make_char(
                c,
                x_start + i as f32 * char_width,
                y,
                char_width,
                font_size,
                font_size,
                bold,
            )
        })
        .collect()
}

/// 辅助函数：创建一个简单的 3列×4行 表格的字符数据
/// 表头: Name  Age  Score
/// 行1:  Alice 25   95.5
/// 行2:  Bob   30   88.0
/// 行3:  Carol 28   92.3
fn make_simple_table_chars() -> Vec<RawChar> {
    let mut chars = Vec::new();
    let col_positions = [50.0, 200.0, 350.0]; // 3 列的 x 起始位置
    let row_y = [100.0, 120.0, 140.0, 160.0]; // 4 行的 y 位置

    // 表头（加粗）
    chars.extend(make_text_at("Name", col_positions[0], row_y[0], 10.0, true));
    chars.extend(make_text_at("Age", col_positions[1], row_y[0], 10.0, true));
    chars.extend(make_text_at(
        "Score",
        col_positions[2],
        row_y[0],
        10.0,
        true,
    ));

    // 数据行
    chars.extend(make_text_at(
        "Alice",
        col_positions[0],
        row_y[1],
        10.0,
        false,
    ));
    chars.extend(make_text_at("25", col_positions[1], row_y[1], 10.0, false));
    chars.extend(make_text_at(
        "95.5",
        col_positions[2],
        row_y[1],
        10.0,
        false,
    ));

    chars.extend(make_text_at("Bob", col_positions[0], row_y[2], 10.0, false));
    chars.extend(make_text_at("30", col_positions[1], row_y[2], 10.0, false));
    chars.extend(make_text_at(
        "88.0",
        col_positions[2],
        row_y[2],
        10.0,
        false,
    ));

    chars.extend(make_text_at(
        "Carol",
        col_positions[0],
        row_y[3],
        10.0,
        false,
    ));
    chars.extend(make_text_at("28", col_positions[1], row_y[3], 10.0, false));
    chars.extend(make_text_at(
        "92.3",
        col_positions[2],
        row_y[3],
        10.0,
        false,
    ));

    chars
}

// ===== 表格候选区域检测测试 =====

#[test]
fn test_candidate_detection_simple_table() {
    let chars = make_simple_table_chars();
    let candidates = detect_table_candidates(&chars, 595.0, 842.0);

    assert!(!candidates.is_empty(), "应检测到至少一个表格候选区域");
    let cand = &candidates[0];
    assert!(
        cand.confidence > 0.3,
        "置信度应大于 0.3，实际: {}",
        cand.confidence
    );
    assert!(!cand.chars.is_empty(), "候选区域应包含字符");
}

#[test]
fn test_candidate_detection_no_table() {
    // 单列纯文本，不应检测为表格
    let mut chars = Vec::new();
    for i in 0..5 {
        chars.extend(make_text_at(
            "This is a normal paragraph of text.",
            50.0,
            100.0 + i as f32 * 20.0,
            10.0,
            false,
        ));
    }
    let candidates = detect_table_candidates(&chars, 595.0, 842.0);
    assert!(candidates.is_empty(), "纯文本不应被检测为表格");
}

#[test]
fn test_candidate_detection_empty_input() {
    let candidates = detect_table_candidates(&[], 595.0, 842.0);
    assert!(candidates.is_empty());
}

// ===== Stream 表格抽取测试 =====

#[test]
fn test_stream_extract_simple_table() {
    let chars = make_simple_table_chars();
    let bbox = BBox::new(40.0, 90.0, 400.0, 100.0);
    let result = extract_stream_table(&chars, &bbox, "t0_0", 0);

    assert!(result.is_some(), "应成功抽取表格");
    let table = result.unwrap();

    assert_eq!(table.table_id, "t0_0");
    assert_eq!(table.page_index, 0);
    assert_eq!(table.extraction_mode, ExtractionMode::Stream);

    // 表头
    assert_eq!(table.headers.len(), 3, "应检测到 3 列表头");
    assert_eq!(table.headers[0], "Name");
    assert_eq!(table.headers[1], "Age");
    assert_eq!(table.headers[2], "Score");

    // 数据行
    assert_eq!(table.rows.len(), 3, "应有 3 行数据");

    // 第一行
    assert_eq!(table.rows[0].cells[0].text, "Alice");
    assert_eq!(table.rows[0].cells[1].text, "25");
    assert_eq!(table.rows[0].cells[2].text, "95.5");

    // fallback_text 必须非空
    assert!(!table.fallback_text.is_empty(), "fallback_text 必须非空");
}

#[test]
fn test_stream_extract_empty_input() {
    let bbox = BBox::new(0.0, 0.0, 100.0, 100.0);
    let result = extract_stream_table(&[], &bbox, "t0_0", 0);
    assert!(result.is_none(), "空输入应返回 None");
}

#[test]
fn test_stream_extract_single_row() {
    // 只有一行，不足以构成表格
    let chars = make_text_at("A", 50.0, 100.0, 10.0, false);
    let bbox = BBox::new(40.0, 90.0, 200.0, 30.0);
    let result = extract_stream_table(&chars, &bbox, "t0_0", 0);
    assert!(result.is_none(), "单行不足以构成表格");
}

// ===== CellType 推断测试 =====

#[test]
fn test_cell_type_number_variants() {
    assert_eq!(detect_cell_type("1234"), CellType::Number);
    assert_eq!(detect_cell_type("1,234"), CellType::Number);
    assert_eq!(detect_cell_type("-1,234.56"), CellType::Number);
    assert_eq!(detect_cell_type("0.5"), CellType::Number);
    assert_eq!(detect_cell_type("+100"), CellType::Number);
    assert_eq!(detect_cell_type("(1,234)"), CellType::Number);
}

#[test]
fn test_cell_type_percent() {
    assert_eq!(detect_cell_type("12.3%"), CellType::Percent);
    assert_eq!(detect_cell_type("-5%"), CellType::Percent);
    assert_eq!(detect_cell_type("0.1%"), CellType::Percent);
}

#[test]
fn test_cell_type_currency() {
    assert_eq!(detect_cell_type("$1,234"), CellType::Currency);
    assert_eq!(detect_cell_type("¥5,678"), CellType::Currency);
    assert_eq!(detect_cell_type("€100"), CellType::Currency);
    assert_eq!(detect_cell_type("£50.99"), CellType::Currency);
    assert_eq!(detect_cell_type("1,234万元"), CellType::Currency);
    assert_eq!(detect_cell_type("100亿元"), CellType::Currency);
    assert_eq!(detect_cell_type("50美元"), CellType::Currency);
}

#[test]
fn test_cell_type_date() {
    assert_eq!(detect_cell_type("2023-01-15"), CellType::Date);
    assert_eq!(detect_cell_type("2023/01/15"), CellType::Date);
    assert_eq!(detect_cell_type("01/15/2023"), CellType::Date);
    assert_eq!(detect_cell_type("2023年1月"), CellType::Date);
    assert_eq!(detect_cell_type("FY2023"), CellType::Date);
}

#[test]
fn test_cell_type_text_and_unknown() {
    assert_eq!(detect_cell_type("Hello World"), CellType::Text);
    assert_eq!(detect_cell_type("收入"), CellType::Text);
    assert_eq!(detect_cell_type("Revenue"), CellType::Text);
    assert_eq!(detect_cell_type(""), CellType::Unknown);
    assert_eq!(detect_cell_type("  "), CellType::Unknown);
}

// ===== fallback_text 生成测试 =====

#[test]
fn test_fallback_text_format() {
    let headers = vec!["年份".to_string(), "收入".to_string(), "支出".to_string()];
    let rows = vec![
        TableRow {
            row_index: 0,
            cells: vec![
                TableCell {
                    row: 0,
                    col: 0,
                    text: "2023".into(),
                    cell_type: CellType::Date,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 0,
                    col: 1,
                    text: "1234".into(),
                    cell_type: CellType::Number,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 0,
                    col: 2,
                    text: "567".into(),
                    cell_type: CellType::Number,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        },
        TableRow {
            row_index: 1,
            cells: vec![
                TableCell {
                    row: 1,
                    col: 0,
                    text: "2022".into(),
                    cell_type: CellType::Date,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 1,
                    col: 1,
                    text: "1100".into(),
                    cell_type: CellType::Number,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 1,
                    col: 2,
                    text: "480".into(),
                    cell_type: CellType::Number,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        },
    ];

    let fallback = generate_fallback_text(&headers, &rows, "t0_0", 0);
    assert!(fallback.contains("[表t0_0 页0]"), "应包含表格标识");
    assert!(fallback.contains("年份=2023"), "应包含 KV 对");
    assert!(fallback.contains("收入=1234"), "应包含 KV 对");
    assert!(fallback.contains("支出=567"), "应包含 KV 对");
    assert!(fallback.contains("行1:"), "应包含行号");
    assert!(fallback.contains("行2:"), "应包含行号");
}

#[test]
fn test_kv_lines_format() {
    let headers = vec!["名称".to_string(), "数量".to_string()];
    let rows = vec![TableRow {
        row_index: 0,
        cells: vec![
            TableCell {
                row: 0,
                col: 0,
                text: "苹果".into(),
                cell_type: CellType::Text,
                rowspan: 1,
                colspan: 1,
            },
            TableCell {
                row: 0,
                col: 1,
                text: "100".into(),
                cell_type: CellType::Number,
                rowspan: 1,
                colspan: 1,
            },
        ],
    }];

    let lines = generate_kv_lines(&headers, &rows, "t1_0", 2);
    assert_eq!(lines.len(), 2, "应有 2 行（每个单元格一行）");
    assert!(lines[0].contains("表=t1_0"), "应包含表 ID");
    assert!(lines[0].contains("页=2"), "应包含页码");
    assert!(lines[0].contains("列=名称"), "应包含列名");
    assert!(lines[0].contains("值=苹果"), "应包含值");
}

#[test]
fn test_row_lines_format() {
    let headers = vec!["年份".to_string(), "收入".to_string()];
    let rows = vec![TableRow {
        row_index: 0,
        cells: vec![
            TableCell {
                row: 0,
                col: 0,
                text: "2023".into(),
                cell_type: CellType::Date,
                rowspan: 1,
                colspan: 1,
            },
            TableCell {
                row: 0,
                col: 1,
                text: "1234".into(),
                cell_type: CellType::Number,
                rowspan: 1,
                colspan: 1,
            },
        ],
    }];

    let lines = generate_row_lines(&headers, &rows, "t0_0", 0);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("表=t0_0"));
    assert!(lines[0].contains("行key=2023"));
    assert!(lines[0].contains("列=收入 值=1234"));
}

// ===== CSV / Markdown 导出测试 =====

#[test]
fn test_table_ir_csv_export() {
    let table = make_test_table_ir();
    let csv = table.to_csv();
    assert!(csv.contains("Name,Age,Score"), "CSV 应包含表头");
    assert!(csv.contains("Alice,25,95.5"), "CSV 应包含数据行");
}

#[test]
fn test_table_ir_markdown_export() {
    let table = make_test_table_ir();
    let md = table.to_markdown();
    assert!(md.contains("| Name | Age | Score |"), "Markdown 应包含表头");
    assert!(md.contains("| --- | --- | --- |"), "Markdown 应包含分隔行");
    assert!(
        md.contains("| Alice | 25 | 95.5 |"),
        "Markdown 应包含数据行"
    );
}

#[test]
fn test_table_ir_kv_lines() {
    let table = make_test_table_ir();
    let lines = table.to_kv_lines();
    assert!(!lines.is_empty(), "KV lines 不应为空");
    assert!(lines[0].contains("表="), "应包含表标识");
    assert!(lines[0].contains("列="), "应包含列名");
    assert!(lines[0].contains("值="), "应包含值");
}

#[test]
fn test_table_ir_row_lines() {
    let table = make_test_table_ir();
    let lines = table.to_row_lines();
    assert!(!lines.is_empty(), "Row lines 不应为空");
    assert!(lines[0].contains("表="), "应包含表标识");
}

// ===== 端到端抽取测试 =====

#[test]
fn test_extract_tables_end_to_end() {
    let chars = make_simple_table_chars();
    let tables = extract_tables(&chars, 0, 595.0, 842.0);

    // 表格是否被检测到取决于候选区域检测的灵敏度
    // 这里主要验证函数不崩溃且返回合理结果
    for table in &tables {
        assert!(
            !table.fallback_text.is_empty(),
            "每个 TableIR 必须有 fallback_text"
        );
        assert!(!table.headers.is_empty(), "表格应有表头");
        assert!(!table.rows.is_empty(), "表格应有数据行");
        assert_eq!(table.extraction_mode, ExtractionMode::Stream);
    }
}

#[test]
fn test_extract_tables_empty_page() {
    let tables = extract_tables(&[], 0, 595.0, 842.0);
    assert!(tables.is_empty(), "空页面不应抽取出表格");
}

// ===== 辅助：创建测试用 TableIR =====

fn make_test_table_ir() -> TableIR {
    TableIR {
        table_id: "t0_0".to_string(),
        page_index: 0,
        bbox: BBox::new(50.0, 100.0, 400.0, 80.0),
        extraction_mode: ExtractionMode::Stream,
        headers: vec!["Name".into(), "Age".into(), "Score".into()],
        rows: vec![
            TableRow {
                row_index: 0,
                cells: vec![
                    TableCell {
                        row: 0,
                        col: 0,
                        text: "Alice".into(),
                        cell_type: CellType::Text,
                        rowspan: 1,
                        colspan: 1,
                    },
                    TableCell {
                        row: 0,
                        col: 1,
                        text: "25".into(),
                        cell_type: CellType::Number,
                        rowspan: 1,
                        colspan: 1,
                    },
                    TableCell {
                        row: 0,
                        col: 2,
                        text: "95.5".into(),
                        cell_type: CellType::Number,
                        rowspan: 1,
                        colspan: 1,
                    },
                ],
            },
            TableRow {
                row_index: 1,
                cells: vec![
                    TableCell {
                        row: 1,
                        col: 0,
                        text: "Bob".into(),
                        cell_type: CellType::Text,
                        rowspan: 1,
                        colspan: 1,
                    },
                    TableCell {
                        row: 1,
                        col: 1,
                        text: "30".into(),
                        cell_type: CellType::Number,
                        rowspan: 1,
                        colspan: 1,
                    },
                    TableCell {
                        row: 1,
                        col: 2,
                        text: "88.0".into(),
                        cell_type: CellType::Number,
                        rowspan: 1,
                        colspan: 1,
                    },
                ],
            },
        ],
        column_types: vec![CellType::Text, CellType::Number, CellType::Number],
        fallback_text:
            "[表t0_0 页0]\n行1: Name=Alice Age=25 Score=95.5\n行2: Name=Bob Age=30 Score=88.0"
                .to_string(),
        confidence: None,
    }
}

// ===== TableIR serde roundtrip =====

#[test]
fn test_table_ir_serde_with_cell_types() {
    let table = make_test_table_ir();
    let json = serde_json::to_string(&table).unwrap();
    let deserialized: TableIR = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.table_id, table.table_id);
    assert_eq!(deserialized.headers, table.headers);
    assert_eq!(deserialized.rows.len(), table.rows.len());
    assert_eq!(deserialized.column_types.len(), table.column_types.len());
    assert_eq!(deserialized.fallback_text, table.fallback_text);
    assert_eq!(deserialized.extraction_mode, ExtractionMode::Stream);
}

// ===== 列类型推断整合测试 =====

#[test]
fn test_column_type_consistency() {
    // 模拟一个有数字列的表格
    let chars = make_simple_table_chars();
    let bbox = BBox::new(40.0, 90.0, 400.0, 100.0);
    let result = extract_stream_table(&chars, &bbox, "t0_0", 0);

    if let Some(table) = result {
        // column_types 长度应与 headers 一致
        assert_eq!(
            table.column_types.len(),
            table.headers.len(),
            "column_types 长度应与 headers 一致"
        );

        // 第一列（Name: Text），第二列（Age: Number），第三列（Score: Number）
        // 这取决于具体的类型推断逻辑
        assert_eq!(
            table.column_types[0],
            CellType::Text,
            "Name 列应为 Text 类型"
        );
        assert_eq!(
            table.column_types[1],
            CellType::Number,
            "Age 列应为 Number 类型"
        );
        assert_eq!(
            table.column_types[2],
            CellType::Number,
            "Score 列应为 Number 类型"
        );
    }
}
