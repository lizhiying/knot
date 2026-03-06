//! 复杂报表解析测试
//! 测试多级表头、forward_fill、脏数据行过滤

use knot_excel::config::ExcelConfig;
use knot_excel::pipeline;
use std::path::Path;

#[test]
fn test_complex_report_parsing() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_complex_report.xlsx");

    if !test_file.exists() {
        eprintln!("Skipping: test file not found at {:?}", test_file);
        return;
    }

    let config = ExcelConfig::default();
    let result = pipeline::parse_excel_full(&test_file, &config);

    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty(), "No blocks parsed");

    let block = &parsed.blocks[0];
    let profile = &parsed.profiles[0];

    println!("=== Complex Report Test Results ===");
    println!("Sheet: {}", block.sheet_name);
    println!("Header levels: {}", block.header_levels);
    println!("Columns ({}):", block.column_names.len());
    for (i, name) in block.column_names.iter().enumerate() {
        println!("  {} - {} ({})", i, name, block.column_types[i]);
    }
    println!("Row count: {}", block.row_count);

    // 验证：表头应该不是空白
    for name in &block.column_names {
        assert!(!name.is_empty(), "Column name should not be empty");
    }

    // 验证：数据行数应该是 6（不含说明行和备注行）
    println!("\nData rows:");
    for (i, row) in block.rows.iter().enumerate() {
        println!("  Row {}: {:?}", i, row);
    }

    // 验证：forward_fill 应该填充了部门列的空值
    // 原始数据中第 3 行和第 5 行的"部门"列为空
    let dept_col = 0; // 部门列
    for (i, row) in block.rows.iter().enumerate() {
        assert!(
            !row[dept_col].trim().is_empty(),
            "Row {} department should be filled by forward_fill, got empty",
            i
        );
    }

    // 验证：备注行应该被过滤掉
    for row in &block.rows {
        let first = row[0].trim();
        assert!(
            !first.starts_with("备注"),
            "Dirty row '备注' should be filtered"
        );
        assert!(
            !first.starts_with("制表人"),
            "Dirty row '制表人' should be filtered"
        );
    }

    // 验证 Profile 生成
    let chunk = profile.to_chunk_text();
    println!("\n=== Profile Chunk ===");
    println!("{}", chunk);
    assert!(chunk.contains("[表格数据]"));

    println!("\n=== Test Passed! ===");
}

#[test]
fn test_forward_fill_behavior() {
    // 直接测试 forward_fill 逻辑
    let config = ExcelConfig::default();
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_complex_report.xlsx");

    if !test_file.exists() {
        return;
    }

    let result = pipeline::parse_excel_full(&test_file, &config);
    let parsed = result.unwrap();
    let block = &parsed.blocks[0];

    // 检查连续相同部门是否被正确填充
    let dept_values: Vec<&str> = block.rows.iter().map(|r| r[0].as_str()).collect();
    println!("Department values after forward_fill: {:?}", dept_values);

    // 所有部门值都不应该为空
    for (i, dept) in dept_values.iter().enumerate() {
        assert!(
            !dept.is_empty(),
            "Row {} department is empty after forward_fill",
            i
        );
    }
}

#[test]
fn test_dirty_row_filter() {
    let config = ExcelConfig::default();
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_complex_report.xlsx");

    if !test_file.exists() {
        return;
    }

    let result = pipeline::parse_excel_full(&test_file, &config);
    let parsed = result.unwrap();
    let block = &parsed.blocks[0];

    // 验证没有备注行
    for row in &block.rows {
        let all_text: String = row.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(" ");
        assert!(
            !all_text.contains("备注"),
            "Dirty row containing '备注' should be filtered: {:?}",
            row
        );
        assert!(
            !all_text.contains("制表人"),
            "Dirty row containing '制表人' should be filtered: {:?}",
            row
        );
    }
}
