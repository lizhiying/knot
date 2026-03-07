//! 多数据块切割测试
//!
//! 覆盖三种场景：
//! 1. 空白楚河汉界（≥4 行空行分隔）→ 应该切割
//! 2. 数据类型跳变（1 行空行 + 全文本表头紧跟数值数据）→ 应该切割
//! 3. 数据中间有少量空行（1-2 行空行但类型一致）→ 不应该切割

use knot_excel::config::ExcelConfig;
use knot_excel::pipeline;
use std::path::Path;

/// 场景1：空白楚河汉界（4行空行分隔两个数据块）
#[test]
fn test_split_by_empty_rows() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_multi_block.xlsx");
    if !test_file.exists() {
        eprintln!("Skipping: {:?} not found", test_file);
        return;
    }

    let config = ExcelConfig::default();
    let result = pipeline::parse_excel_full(&test_file, &config);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let parsed = result.unwrap();

    println!("=== Empty Row Split Test ===");
    println!("Total blocks: {}", parsed.blocks.len());
    for (i, block) in parsed.blocks.iter().enumerate() {
        println!(
            "  Block {}: {} rows, cols={:?}",
            i, block.row_count, block.column_names
        );
    }

    // 4行空行分隔 → 应该切割为 2 个块
    assert_eq!(
        parsed.blocks.len(),
        2,
        "Should detect 2 blocks (separated by 4+ empty rows)"
    );

    // 第一个块：员工信息
    assert_eq!(parsed.blocks[0].column_names[0], "姓名");
    assert_eq!(parsed.blocks[0].row_count, 3);

    // 第二个块：销售数据
    assert_eq!(parsed.blocks[1].column_names[0], "月份");
    assert_eq!(parsed.blocks[1].row_count, 4);
}

/// 场景2：数据类型跳变（只有1行空行，但列结构从数值区变成全文本表头 → 新数据块）
#[test]
fn test_split_by_type_transition() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_type_transition.xlsx");
    if !test_file.exists() {
        eprintln!("Skipping: {:?} not found", test_file);
        return;
    }

    let config = ExcelConfig::default();
    let result = pipeline::parse_excel_full(&test_file, &config);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let parsed = result.unwrap();

    println!("=== Type Transition Split Test ===");
    println!("Total blocks: {}", parsed.blocks.len());
    for (i, block) in parsed.blocks.iter().enumerate() {
        println!(
            "  Block {}: {} rows, cols={:?}",
            i, block.row_count, block.column_names
        );
        for (j, row) in block.rows.iter().enumerate() {
            println!("    Row {}: {:?}", j, row);
        }
    }

    // 类型跳变 → 应该切割为 2 个块
    assert_eq!(
        parsed.blocks.len(),
        2,
        "Should detect 2 blocks (type transition: numeric→text header→numeric)"
    );

    // 第一个块：产品信息
    assert!(
        parsed.blocks[0].column_names.contains(&"产品".to_string())
            || parsed.blocks[0].column_names.contains(&"价格".to_string())
    );

    // 第二个块：供应商信息
    assert!(
        parsed.blocks[1]
            .column_names
            .contains(&"供应商".to_string())
            || parsed.blocks[1]
                .column_names
                .contains(&"联系人".to_string())
    );
}

/// 场景3：数据中间的空行不应该触发切割
#[test]
fn test_no_split_for_data_gaps() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_with_gaps.xlsx");
    if !test_file.exists() {
        eprintln!("Skipping: {:?} not found", test_file);
        return;
    }

    let config = ExcelConfig::default();
    let result = pipeline::parse_excel_full(&test_file, &config);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let parsed = result.unwrap();

    println!("=== No-Split Gap Test ===");
    println!("Total blocks: {}", parsed.blocks.len());
    for (i, block) in parsed.blocks.iter().enumerate() {
        println!(
            "  Block {}: {} rows, cols={:?}",
            i, block.row_count, block.column_names
        );
        for (j, row) in block.rows.iter().enumerate() {
            println!("    Row {}: {:?}", j, row);
        }
    }

    // 数据中间 1 行空行 + 类型一致（都有数值列）→ 不应该切割
    assert_eq!(
        parsed.blocks.len(),
        1,
        "Should remain 1 block (gaps within same data type should not split)"
    );

    // 应该包含所有 5 个项目
    assert_eq!(parsed.blocks[0].row_count, 5, "Should have all 5 data rows");
}
