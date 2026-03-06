//! 集成测试：用真实 xlsx 文件验证端到端流程

use knot_excel::config::ExcelConfig;
use knot_excel::pipeline::parse_excel_full;

#[test]
fn test_parse_real_excel_file() {
    let path = "/tmp/test_sales.xlsx";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping test: {} not found. Generate it first.", path);
        return;
    }

    let config = ExcelConfig::default();
    let result = parse_excel_full(path, &config).unwrap();

    // 应该有 2 个数据块（2 个 Sheet）
    assert_eq!(result.blocks.len(), 2, "Expected 2 data blocks (2 sheets)");

    // Sheet 1: 销售数据
    let sales = &result.blocks[0];
    assert_eq!(sales.sheet_name, "销售数据");
    assert_eq!(sales.row_count, 5); // 5 行数据
    assert_eq!(sales.column_names.len(), 5); // 5 列

    // 验证列名
    assert_eq!(sales.column_names[0], "日期");
    assert_eq!(sales.column_names[1], "产品");
    assert_eq!(sales.column_names[2], "销量");
    assert_eq!(sales.column_names[3], "金额");

    // Sheet 2: 库存
    let inventory = &result.blocks[1];
    assert_eq!(inventory.sheet_name, "库存");
    assert_eq!(inventory.row_count, 3); // 3 行数据
    assert_eq!(inventory.column_names.len(), 4);

    // 验证 Profile 生成
    assert_eq!(result.profiles.len(), 2);

    let sales_profile = &result.profiles[0];
    let chunk_text = sales_profile.to_chunk_text();
    println!("=== Sales Profile Chunk Text ===");
    println!("{}", chunk_text);
    println!("================================");

    assert!(chunk_text.contains("[表格数据]"));
    assert!(chunk_text.contains("销售数据"));
    assert!(chunk_text.contains("日期"));
    assert!(chunk_text.contains("产品"));
    assert!(chunk_text.contains("销量"));

    let inv_profile = &result.profiles[1];
    let inv_chunk = inv_profile.to_chunk_text();
    println!("=== Inventory Profile Chunk Text ===");
    println!("{}", inv_chunk);
    println!("====================================");

    assert!(inv_chunk.contains("库存"));
    assert!(inv_chunk.contains("SKU001"));
}
