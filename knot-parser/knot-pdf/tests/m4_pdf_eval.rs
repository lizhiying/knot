//! M4 真实 PDF 评测：使用 lopdf 生成带有线表格的 PDF，然后用 knot-pdf 解析验证
//!
//! 覆盖 20 种不同的 ruled 表格场景

use knot_pdf::ir::ExtractionMode;
use knot_pdf::{parse_pdf, Config};

// ─── PDF 生成辅助函数 ───

/// 生成一个包含带边框表格的 PDF 文件
///
/// 参数：
/// - `path`: 输出路径
/// - `headers`: 表头列表
/// - `data`: 数据行二维数组
/// - `col_widths`: 各列宽度
/// - `options`: 额外选项（粗分隔线、三线表等）
fn generate_ruled_table_pdf(
    path: &std::path::Path,
    headers: &[&str],
    data: &[Vec<&str>],
    col_widths: &[f32],
    options: &TablePdfOptions,
) {
    let page_width = 612.0_f32; // Letter
    let page_height = 792.0_f32;
    let margin_left = 50.0_f32;
    let margin_top = 700.0_f32; // PDF 坐标 y 从下到上

    let row_height = options.row_height;
    let total_rows = 1 + data.len(); // header + data
    let total_cols = headers.len();
    let total_width: f32 = col_widths.iter().sum();

    // 构建 content stream
    let mut cs = String::new();

    // 设置字体
    cs.push_str("BT\n/F1 10 Tf\nET\n");

    // 画线宽
    cs.push_str(&format!("{} w\n", options.line_width));

    if options.three_line_style {
        // 三线表：只画顶线、表头底线（粗）、底线
        // 顶线
        let y_top = margin_top;
        cs.push_str(&format!("{} w\n", options.thick_line_width));
        cs.push_str(&format!(
            "{} {} m {} {} l S\n",
            margin_left,
            y_top,
            margin_left + total_width,
            y_top
        ));

        // 表头底线（粗）
        let y_header_bottom = margin_top - row_height;
        cs.push_str(&format!(
            "{} {} m {} {} l S\n",
            margin_left,
            y_header_bottom,
            margin_left + total_width,
            y_header_bottom
        ));

        // 底线
        cs.push_str(&format!("{} w\n", options.thick_line_width));
        let y_bottom = margin_top - row_height * total_rows as f32;
        cs.push_str(&format!(
            "{} {} m {} {} l S\n",
            margin_left,
            y_bottom,
            margin_left + total_width,
            y_bottom
        ));
    } else {
        // 完整网格

        // 水平线
        for r in 0..=total_rows {
            let y = margin_top - r as f32 * row_height;
            let lw = if r == 1 && options.thick_header_line {
                options.thick_line_width
            } else {
                options.line_width
            };
            cs.push_str(&format!("{} w\n", lw));
            cs.push_str(&format!(
                "{} {} m {} {} l S\n",
                margin_left,
                y,
                margin_left + total_width,
                y
            ));
        }

        // 垂直线
        cs.push_str(&format!("{} w\n", options.line_width));
        let mut x = margin_left;
        for c in 0..=total_cols {
            let y_start = margin_top;
            let y_end = margin_top - total_rows as f32 * row_height;
            cs.push_str(&format!("{} {} m {} {} l S\n", x, y_start, x, y_end));
            if c < total_cols {
                x += col_widths[c];
            }
        }
    }

    // 写入文本
    let font_size = options.font_size;
    let text_offset_y = row_height / 2.0 - font_size / 3.0;

    // 表头
    let mut x = margin_left;
    for (c, header) in headers.iter().enumerate() {
        let tx = x + 5.0;
        let ty = margin_top - text_offset_y;
        let escaped = escape_pdf_text(header);
        cs.push_str(&format!(
            "BT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\n",
            if options.bold_header {
                font_size + 1.0
            } else {
                font_size
            },
            tx,
            ty,
            escaped
        ));
        x += col_widths[c];
    }

    // 数据行
    for (r, row) in data.iter().enumerate() {
        let mut x = margin_left;
        for (c, cell_text) in row.iter().enumerate() {
            let tx = x + 5.0;
            let ty = margin_top - (r + 1) as f32 * row_height - text_offset_y;
            let escaped = escape_pdf_text(cell_text);
            cs.push_str(&format!(
                "BT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\n",
                font_size, tx, ty, escaped
            ));
            if c < col_widths.len() {
                x += col_widths[c];
            }
        }
    }

    // 用 lopdf 构建 PDF 文档
    build_pdf_from_content_stream(path, &cs, page_width, page_height);
}

/// 表格 PDF 生成选项
#[derive(Clone)]
struct TablePdfOptions {
    line_width: f32,
    thick_line_width: f32,
    thick_header_line: bool,
    bold_header: bool,
    three_line_style: bool,
    row_height: f32,
    font_size: f32,
}

impl Default for TablePdfOptions {
    fn default() -> Self {
        Self {
            line_width: 0.5,
            thick_line_width: 1.5,
            thick_header_line: true,
            bold_header: false,
            three_line_style: false,
            row_height: 20.0,
            font_size: 10.0,
        }
    }
}

fn escape_pdf_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// 从 content stream 字符串构建 PDF 文件
fn build_pdf_from_content_stream(
    path: &std::path::Path,
    content: &str,
    page_width: f32,
    page_height: f32,
) {
    use lopdf::dictionary;
    use lopdf::{Document, Object, Stream};

    let mut doc = Document::with_version("1.5");

    // 添加 Helvetica 字体
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });

    let content_bytes = content.as_bytes().to_vec();
    let content_stream = Stream::new(dictionary! {}, content_bytes);
    let content_id = doc.add_object(content_stream);

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(page_width),
            Object::Real(page_height),
        ],
        "Resources" => resources_id,
        "Contents" => content_id,
    });

    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => Object::Integer(1),
    });

    // 设置 Page 的 Parent
    if let Ok(page_obj) = doc.get_object_mut(page_id) {
        if let Ok(dict) = page_obj.as_dict_mut() {
            dict.set("Parent", pages_id);
        }
    }

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc.save(path).expect("Failed to save PDF");
}

// ─── 评测结构体 ───

struct EvalResult {
    name: String,
    found_table: bool,
    is_ruled: bool,
    expected_cols: usize,
    actual_cols: usize,
    expected_data_rows: usize,
    actual_data_rows: usize,
    header_match: bool,
    has_fallback: bool,
}

impl EvalResult {
    fn col_match(&self) -> bool {
        self.actual_cols == self.expected_cols
    }

    fn row_match(&self) -> bool {
        self.actual_data_rows == self.expected_data_rows
    }

    fn passed(&self) -> bool {
        self.found_table && self.col_match() && self.has_fallback
    }
}

// ─── 20 个评测样本 ───

#[test]
fn test_m4_ruled_pdf_evaluation() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let mut results: Vec<EvalResult> = Vec::new();

    // ─── 样本 1: 简单 2列2行 ───
    {
        let path = tmp_dir.path().join("sample_01.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Name", "Age"],
            &[vec!["Alice", "30"], vec!["Bob", "25"]],
            &[150.0, 100.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(&path, "01_simple_2x2", 2, 2, &["Name", "Age"]));
    }

    // ─── 样本 2: 3列5行 财务表 ───
    {
        let path = tmp_dir.path().join("sample_02.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Year", "Revenue", "Profit"],
            &[
                vec!["2019", "1000", "200"],
                vec!["2020", "1200", "250"],
                vec!["2021", "1500", "300"],
                vec!["2022", "1800", "350"],
                vec!["2023", "2000", "400"],
            ],
            &[100.0, 120.0, 120.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "02_financial_3x5",
            3,
            5,
            &["Year", "Revenue", "Profit"],
        ));
    }

    // ─── 样本 3: 4列3行 ───
    {
        let path = tmp_dir.path().join("sample_03.pdf");
        generate_ruled_table_pdf(
            &path,
            &["ID", "Product", "Price", "Stock"],
            &[
                vec!["001", "Widget", "9.99", "100"],
                vec!["002", "Gadget", "19.99", "50"],
                vec!["003", "Doohickey", "4.99", "200"],
            ],
            &[60.0, 120.0, 80.0, 80.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "03_inventory_4x3",
            4,
            3,
            &["ID", "Product", "Price", "Stock"],
        ));
    }

    // ─── 样本 4: 5列2行 宽表 ───
    {
        let path = tmp_dir.path().join("sample_04.pdf");
        generate_ruled_table_pdf(
            &path,
            &["A", "B", "C", "D", "E"],
            &[
                vec!["1", "2", "3", "4", "5"],
                vec!["6", "7", "8", "9", "10"],
            ],
            &[80.0, 80.0, 80.0, 80.0, 80.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "04_wide_5x2",
            5,
            2,
            &["A", "B", "C", "D", "E"],
        ));
    }

    // ─── 样本 5: 2列10行 长表 ───
    {
        let path = tmp_dir.path().join("sample_05.pdf");
        let data: Vec<Vec<&str>> = (1..=10)
            .map(|i| {
                vec![
                    Box::leak(format!("Item{}", i).into_boxed_str()) as &str,
                    Box::leak(format!("{}", i * 100).into_boxed_str()) as &str,
                ]
            })
            .collect();
        generate_ruled_table_pdf(
            &path,
            &["Item", "Value"],
            &data,
            &[150.0, 100.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "05_long_2x10",
            2,
            10,
            &["Item", "Value"],
        ));
    }

    // ─── 样本 6: 三线表风格 ───
    {
        let path = tmp_dir.path().join("sample_06.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Category", "Count"],
            &[vec!["Alpha", "42"], vec!["Beta", "17"], vec!["Gamma", "99"]],
            &[150.0, 100.0],
            &TablePdfOptions {
                three_line_style: true,
                ..Default::default()
            },
        );
        // 三线表没有垂直分隔线，ruled 应该失败，降级到 stream
        results.push(evaluate_pdf_any_mode(&path, "06_three_line", 2, 3));
    }

    // ─── 样本 7: 粗表头分隔线 ───
    {
        let path = tmp_dir.path().join("sample_07.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Q", "Sales", "Growth"],
            &[
                vec!["Q1", "500", "10%"],
                vec!["Q2", "600", "20%"],
                vec!["Q3", "550", "-8%"],
                vec!["Q4", "700", "27%"],
            ],
            &[80.0, 100.0, 100.0],
            &TablePdfOptions {
                thick_header_line: true,
                thick_line_width: 2.0,
                ..Default::default()
            },
        );
        results.push(evaluate_pdf(
            &path,
            "07_thick_header",
            3,
            4,
            &["Q", "Sales", "Growth"],
        ));
    }

    // ─── 样本 8: 细线表格 ───
    {
        let path = tmp_dir.path().join("sample_08.pdf");
        generate_ruled_table_pdf(
            &path,
            &["X", "Y"],
            &[vec!["1.5", "2.5"], vec!["3.0", "4.0"]],
            &[120.0, 120.0],
            &TablePdfOptions {
                line_width: 0.25,
                thick_header_line: false,
                ..Default::default()
            },
        );
        results.push(evaluate_pdf(&path, "08_thin_lines", 2, 2, &["X", "Y"]));
    }

    // ─── 样本 9: 小字号表格 ───
    {
        let path = tmp_dir.path().join("sample_09.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Code", "Desc", "Qty"],
            &[
                vec!["A001", "Part-A", "10"],
                vec!["B002", "Part-B", "20"],
                vec!["C003", "Part-C", "30"],
            ],
            &[80.0, 120.0, 60.0],
            &TablePdfOptions {
                font_size: 8.0,
                row_height: 16.0,
                ..Default::default()
            },
        );
        results.push(evaluate_pdf(
            &path,
            "09_small_font",
            3,
            3,
            &["Code", "Desc", "Qty"],
        ));
    }

    // ─── 样本 10: 大字号表格 ───
    {
        let path = tmp_dir.path().join("sample_10.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Region", "Total"],
            &[vec!["East", "5000"], vec!["West", "4200"]],
            &[200.0, 150.0],
            &TablePdfOptions {
                font_size: 14.0,
                row_height: 30.0,
                ..Default::default()
            },
        );
        results.push(evaluate_pdf(
            &path,
            "10_large_font",
            2,
            2,
            &["Region", "Total"],
        ));
    }

    // ─── 样本 11: 6列2行 超宽 ───
    {
        let path = tmp_dir.path().join("sample_11.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
            &[
                vec!["10", "20", "30", "40", "50", "60"],
                vec!["15", "25", "35", "45", "55", "65"],
            ],
            &[70.0, 70.0, 70.0, 70.0, 70.0, 70.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "11_six_cols",
            6,
            2,
            &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
        ));
    }

    // ─── 样本 12: 2列1行 最小 ───
    {
        let path = tmp_dir.path().join("sample_12.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Key", "Val"],
            &[vec!["foo", "bar"]],
            &[120.0, 120.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(&path, "12_minimal_2x1", 2, 1, &["Key", "Val"]));
    }

    // ─── 样本 13: 百分比数据 ───
    {
        let path = tmp_dir.path().join("sample_13.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Metric", "2022", "2023"],
            &[
                vec!["Growth", "12.5%", "15.3%"],
                vec!["Margin", "8.2%", "9.1%"],
            ],
            &[120.0, 100.0, 100.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "13_percent_data",
            3,
            2,
            &["Metric", "2022", "2023"],
        ));
    }

    // ─── 样本 14: 货币数据 ───
    {
        let path = tmp_dir.path().join("sample_14.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Item", "Cost"],
            &[
                vec!["Laptop", "$1,299"],
                vec!["Mouse", "$29"],
                vec!["Monitor", "$499"],
            ],
            &[150.0, 100.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "14_currency_data",
            2,
            3,
            &["Item", "Cost"],
        ));
    }

    // ─── 样本 15: 日期数据 ───
    {
        let path = tmp_dir.path().join("sample_15.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Date", "Event"],
            &[
                vec!["2023-01-15", "Launch"],
                vec!["2023-06-01", "Update"],
                vec!["2023-12-31", "EOY"],
            ],
            &[120.0, 150.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "15_date_data",
            2,
            3,
            &["Date", "Event"],
        ));
    }

    // ─── 样本 16: 不等宽列 ───
    {
        let path = tmp_dir.path().join("sample_16.pdf");
        generate_ruled_table_pdf(
            &path,
            &["ID", "Description", "Notes"],
            &[
                vec!["1", "Short", "N/A"],
                vec!["2", "Medium length text", "OK"],
            ],
            &[40.0, 200.0, 100.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "16_uneven_cols",
            3,
            2,
            &["ID", "Description", "Notes"],
        ));
    }

    // ─── 样本 17: 窄列 ───
    {
        let path = tmp_dir.path().join("sample_17.pdf");
        generate_ruled_table_pdf(
            &path,
            &["A", "B", "C"],
            &[vec!["1", "2", "3"], vec!["4", "5", "6"]],
            &[40.0, 40.0, 40.0],
            &TablePdfOptions::default(),
        );
        results.push(evaluate_pdf(
            &path,
            "17_narrow_cols",
            3,
            2,
            &["A", "B", "C"],
        ));
    }

    // ─── 样本 18: 高行高 ───
    {
        let path = tmp_dir.path().join("sample_18.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Name", "Score"],
            &[vec!["Tom", "88"], vec!["Jerry", "95"]],
            &[150.0, 100.0],
            &TablePdfOptions {
                row_height: 40.0,
                ..Default::default()
            },
        );
        results.push(evaluate_pdf(
            &path,
            "18_tall_rows",
            2,
            2,
            &["Name", "Score"],
        ));
    }

    // ─── 样本 19: 矩形方式绘制表格（re 操作符代替 m/l） ───
    {
        let path = tmp_dir.path().join("sample_19.pdf");
        generate_rect_style_table_pdf(
            &path,
            &["Col1", "Col2"],
            &[vec!["A", "B"], vec!["C", "D"]],
            &[120.0, 120.0],
        );
        results.push(evaluate_pdf_any_mode(&path, "19_rect_style", 2, 2));
    }

    // ─── 样本 20: 外边框 + 内部单线 ───
    {
        let path = tmp_dir.path().join("sample_20.pdf");
        generate_ruled_table_pdf(
            &path,
            &["Country", "Capital", "Pop"],
            &[
                vec!["China", "Beijing", "1.4B"],
                vec!["Japan", "Tokyo", "126M"],
                vec!["Korea", "Seoul", "52M"],
            ],
            &[100.0, 120.0, 80.0],
            &TablePdfOptions {
                line_width: 1.0,
                thick_header_line: true,
                thick_line_width: 2.0,
                ..Default::default()
            },
        );
        results.push(evaluate_pdf(
            &path,
            "20_border_table",
            3,
            3,
            &["Country", "Capital", "Pop"],
        ));
    }

    // ─── 输出评测报告 ───
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║             M4 Ruled 表格 PDF 评测报告                     ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ {:<25} │ 表格 │ 列数 │ 行数 │ 结果 ║", "样本名称");
    println!("╠══════════════════════════════════════════════════════════════╣");

    let mut pass_count = 0;
    let total = results.len();

    for r in &results {
        let status = if r.passed() { "✅" } else { "❌" };
        if r.passed() {
            pass_count += 1;
        }
        println!(
            "║ {:<25} │  {}  │ {}/{} │ {}/{} │  {}  ║",
            r.name,
            if r.found_table { "✓" } else { "✗" },
            r.actual_cols,
            r.expected_cols,
            r.actual_data_rows,
            r.expected_data_rows,
            status,
        );
    }

    println!("╠══════════════════════════════════════════════════════════════╣");
    let rate = pass_count as f32 / total as f32 * 100.0;
    println!(
        "║ 通过率: {}/{} ({:.0}%)                                     ║",
        pass_count, total, rate
    );
    println!("╚══════════════════════════════════════════════════════════════╝");

    // 断言：通过率 > 80%
    assert!(
        rate >= 80.0,
        "Ruled 表格列映射正确率 {:.0}% 未达到 80% 的目标（通过 {}/{}）",
        rate,
        pass_count,
        total
    );
}

// ─── 评测函数 ───

fn evaluate_pdf(
    path: &std::path::Path,
    name: &str,
    expected_cols: usize,
    expected_data_rows: usize,
    expected_headers: &[&str],
) -> EvalResult {
    let config = Config::default();
    match parse_pdf(path, &config) {
        Ok(doc) => {
            // 查找第一个表格
            let table = doc.pages.iter().flat_map(|p| p.tables.iter()).next();
            if let Some(t) = table {
                EvalResult {
                    name: name.to_string(),
                    found_table: true,
                    is_ruled: t.extraction_mode == ExtractionMode::Ruled,
                    expected_cols,
                    actual_cols: t.headers.len(),
                    expected_data_rows,
                    actual_data_rows: t.rows.len(),
                    header_match: t
                        .headers
                        .iter()
                        .zip(expected_headers.iter())
                        .all(|(a, b)| a.trim() == *b),
                    has_fallback: !t.fallback_text.is_empty(),
                }
            } else {
                EvalResult {
                    name: name.to_string(),
                    found_table: false,
                    is_ruled: false,
                    expected_cols,
                    actual_cols: 0,
                    expected_data_rows,
                    actual_data_rows: 0,
                    header_match: false,
                    has_fallback: false,
                }
            }
        }
        Err(e) => {
            eprintln!("  [{}] 解析失败: {}", name, e);
            EvalResult {
                name: name.to_string(),
                found_table: false,
                is_ruled: false,
                expected_cols,
                actual_cols: 0,
                expected_data_rows,
                actual_data_rows: 0,
                header_match: false,
                has_fallback: false,
            }
        }
    }
}

fn evaluate_pdf_any_mode(
    path: &std::path::Path,
    name: &str,
    expected_cols: usize,
    expected_data_rows: usize,
) -> EvalResult {
    let config = Config::default();
    match parse_pdf(path, &config) {
        Ok(doc) => {
            let table = doc.pages.iter().flat_map(|p| p.tables.iter()).next();
            if let Some(t) = table {
                EvalResult {
                    name: name.to_string(),
                    found_table: true,
                    is_ruled: t.extraction_mode == ExtractionMode::Ruled,
                    expected_cols,
                    actual_cols: t.headers.len(),
                    expected_data_rows,
                    actual_data_rows: t.rows.len(),
                    header_match: true, // 不检查具体表头
                    has_fallback: !t.fallback_text.is_empty(),
                }
            } else {
                EvalResult {
                    name: name.to_string(),
                    found_table: false,
                    is_ruled: false,
                    expected_cols,
                    actual_cols: 0,
                    expected_data_rows,
                    actual_data_rows: 0,
                    header_match: false,
                    has_fallback: false,
                }
            }
        }
        Err(e) => {
            eprintln!("  [{}] 解析失败: {}", name, e);
            EvalResult {
                name: name.to_string(),
                found_table: false,
                is_ruled: false,
                expected_cols,
                actual_cols: 0,
                expected_data_rows,
                actual_data_rows: 0,
                header_match: false,
                has_fallback: false,
            }
        }
    }
}

/// 用矩形（re 操作符）方式绘制表格  
fn generate_rect_style_table_pdf(
    path: &std::path::Path,
    headers: &[&str],
    data: &[Vec<&str>],
    col_widths: &[f32],
) {
    let page_width = 612.0_f32;
    let page_height = 792.0_f32;
    let margin_left = 50.0_f32;
    let margin_top = 700.0_f32;
    let row_height = 20.0_f32;
    let total_rows = 1 + data.len();

    let mut cs = String::new();
    cs.push_str("BT\n/F1 10 Tf\nET\n");

    // 用 re 操作符画每个 cell 的边框（fill + stroke style）
    cs.push_str("0.5 w\n");
    for r in 0..total_rows {
        let mut x = margin_left;
        for (_c, &w) in col_widths.iter().enumerate() {
            let y = margin_top - (r + 1) as f32 * row_height;
            cs.push_str(&format!("{} {} {} {} re S\n", x, y, w, row_height));
            x += w;
        }
    }

    // 写入文本
    let text_offset_y = row_height / 2.0 - 3.3;

    let mut x = margin_left;
    for (c, header) in headers.iter().enumerate() {
        let tx = x + 5.0;
        let ty = margin_top - text_offset_y;
        let escaped = escape_pdf_text(header);
        cs.push_str(&format!(
            "BT\n/F1 10 Tf\n{} {} Td\n({}) Tj\nET\n",
            tx, ty, escaped
        ));
        x += col_widths[c];
    }

    for (r, row) in data.iter().enumerate() {
        let mut x = margin_left;
        for (c, cell_text) in row.iter().enumerate() {
            let tx = x + 5.0;
            let ty = margin_top - (r + 1) as f32 * row_height - text_offset_y;
            let escaped = escape_pdf_text(cell_text);
            cs.push_str(&format!(
                "BT\n/F1 10 Tf\n{} {} Td\n({}) Tj\nET\n",
                tx, ty, escaped
            ));
            if c < col_widths.len() {
                x += col_widths[c];
            }
        }
    }

    build_pdf_from_content_stream(path, &cs, page_width, page_height);
}
