//! 多数据块切割测试

use knot_excel::config::ExcelConfig;
use knot_excel::pipeline;
use std::path::Path;

#[test]
fn test_multi_block_detection() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_multi_block.xlsx");

    if !test_file.exists() {
        eprintln!("Skipping: test file not found at {:?}", test_file);
        return;
    }

    let config = ExcelConfig::default();
    let result = pipeline::parse_excel_full(&test_file, &config);

    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let parsed = result.unwrap();

    println!("=== Multi-Block Test Results ===");
    println!("Total blocks: {}", parsed.blocks.len());

    for (i, block) in parsed.blocks.iter().enumerate() {
        println!(
            "\nBlock {} - Sheet '{}', {} rows x {} cols",
            i,
            block.sheet_name,
            block.row_count,
            block.column_names.len()
        );
        println!("  Columns: {:?}", block.column_names);
        for (j, row) in block.rows.iter().enumerate() {
            println!("  Row {}: {:?}", j, row);
        }
    }

    // 验证：应该有 2 个数据块
    assert_eq!(
        parsed.blocks.len(),
        2,
        "Should detect 2 data blocks, got {}",
        parsed.blocks.len()
    );

    // 验证第一个数据块
    let block1 = &parsed.blocks[0];
    assert_eq!(block1.column_names[0], "姓名");
    assert_eq!(block1.row_count, 3);

    // 验证第二个数据块
    let block2 = &parsed.blocks[1];
    assert_eq!(block2.column_names[0], "月份");
    assert_eq!(block2.row_count, 4);

    // 验证每个块有独立的 Profile
    assert_eq!(parsed.profiles.len(), 2);

    for (i, profile) in parsed.profiles.iter().enumerate() {
        let chunk = profile.to_chunk_text();
        println!("\n=== Profile {} ===", i);
        println!("{}", chunk);
        assert!(chunk.contains("[表格数据]"));
    }

    println!("\n=== Multi-Block Test Passed! ===");
}
