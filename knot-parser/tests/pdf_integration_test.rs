//! 端到端集成测试：验证 PDF → PageNode 的完整链路

use knot_parser::{IndexDispatcher, PageIndexConfig};
use std::path::Path;

/// 测试：使用 Attention_Is_All_You_Need.pdf 验证 PDF 解析
#[tokio::test]
async fn test_pdf_parse_attention_paper() {
    let pdf_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../knot-pdf/tests/fixtures/Attention_Is_All_You_Need.pdf");

    if !pdf_path.exists() {
        eprintln!("Skipping test: PDF fixture not found at {:?}", pdf_path);
        return;
    }

    let dispatcher = IndexDispatcher::new();
    let config = PageIndexConfig::new();

    let root = dispatcher
        .index_file(&pdf_path, &config)
        .await
        .expect("PDF parsing should succeed");

    // 基本验证
    assert!(
        !root.content.is_empty() || !root.children.is_empty(),
        "Root should have content or children"
    );

    // 验证元数据
    assert_eq!(
        root.metadata.extra.get("parser").map(|s| s.as_str()),
        Some("knot-pdf"),
        "Parser should be knot-pdf"
    );

    // 验证总页数（Attention paper 有 15 页）
    let total_pages = root
        .metadata
        .extra
        .get("total_pages")
        .and_then(|s| s.parse::<usize>().ok());
    assert_eq!(total_pages, Some(15), "Should have 15 pages");

    // 验证有处理时间记录
    assert!(
        root.metadata.extra.contains_key("processing_time_ms"),
        "Should record processing time"
    );

    // 验证关键内容（标题或摘要中应包含 "Attention" 相关文本）
    let all_content = collect_all_content(&root);
    assert!(
        all_content.contains("Attention") || all_content.contains("attention"),
        "Content should contain 'Attention'"
    );
    assert!(
        all_content.contains("Transformer") || all_content.contains("transformer"),
        "Content should contain 'Transformer'"
    );

    // 验证树结构
    let node_count = count_nodes(&root);
    assert!(
        node_count >= 2,
        "Should have at least 2 nodes (root + at least 1 child), got {}",
        node_count
    );

    // 打印摘要信息
    let time_display = root
        .metadata
        .extra
        .get("processing_time_display")
        .cloned()
        .unwrap_or_default();
    println!("=== PDF Parse Result ===");
    println!("Title: {}", root.title);
    println!("Total pages: {:?}", total_pages);
    println!("Node count: {}", node_count);
    println!("Processing time: {}", time_display);
    println!("Content length: {} chars", all_content.len());
    println!("Children: {}", root.children.len());
    for (i, child) in root.children.iter().enumerate().take(10) {
        println!(
            "  [{}] {} (L{}, {} tokens)",
            i, child.title, child.level, child.metadata.token_count
        );
    }
}

/// 测试：PdfParser 可以正确处理不存在的文件
#[tokio::test]
async fn test_pdf_parse_nonexistent() {
    let dispatcher = IndexDispatcher::new();
    let config = PageIndexConfig::new();

    let result = dispatcher
        .index_file(Path::new("/tmp/nonexistent_file.pdf"), &config)
        .await;

    assert!(result.is_err(), "Should fail for nonexistent file");
}

/// 递归收集所有节点的内容
fn collect_all_content(node: &knot_parser::PageNode) -> String {
    let mut content = node.content.clone();
    for child in &node.children {
        content.push('\n');
        content.push_str(&collect_all_content(child));
    }
    content
}

/// 递归计算节点总数
fn count_nodes(node: &knot_parser::PageNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}
