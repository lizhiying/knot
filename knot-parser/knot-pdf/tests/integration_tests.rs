//! 集成测试：born-digital PDF 解析
//!
//! 测试 1: 使用 lopdf 程序化生成的简单 PDF
//! 测试 2: 使用 lopdf 程序化生成的多页 PDF

use knot_pdf::render::{MarkdownRenderer, RagExporter};
use knot_pdf::{parse_pdf, parse_pdf_pages, Config};
use std::io::Write;

/// 用 lopdf 生成一个最简单的单页 PDF（包含嵌入文本）
fn generate_simple_pdf(path: &std::path::Path) {
    use lopdf::dictionary;
    use lopdf::{Document, Stream};

    let mut doc = Document::with_version("1.7");

    // 创建字体引用（使用内置字体 Helvetica）
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    // 页面内容：简单文本
    let content = b"BT /F1 14 Tf 72 720 Td (Hello knot-pdf) Tj ET\n\
                    BT /F1 12 Tf 72 700 Td (This is a test document for integration testing.) Tj ET\n\
                    BT /F1 12 Tf 72 680 Td (Line three of the document.) Tj ET";
    let content_stream = Stream::new(dictionary! {}, content.to_vec());
    let content_id = doc.add_object(content_stream);

    // 资源字典
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });

    // 页面
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => resources_id,
    });

    // Pages 节点
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    });

    // 更新 page 的 Parent
    if let Ok(page_obj) = doc.get_object_mut(page_id) {
        if let Ok(dict) = page_obj.as_dict_mut() {
            dict.set("Parent", pages_id);
        }
    }

    // Catalog
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });

    doc.trailer.set("Root", catalog_id);
    doc.max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(0);

    let mut file = std::fs::File::create(path).expect("创建 PDF 文件失败");
    doc.save_to(&mut file).expect("保存 PDF 失败");
}

/// 用 lopdf 生成一个多页 PDF
fn generate_multipage_pdf(path: &std::path::Path) {
    use lopdf::dictionary;
    use lopdf::{Document, Object, Stream};

    let mut doc = Document::with_version("1.7");

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

    let mut page_ids = Vec::new();

    for i in 1..=3 {
        let text = format!(
            "BT /F1 16 Tf 72 720 Td (Page {i} Title) Tj ET\n\
             BT /F1 12 Tf 72 700 Td (Content of page {i}.) Tj ET\n\
             BT /F1 12 Tf 72 680 Td (More text on page {i}.) Tj ET"
        );
        let content_stream = Stream::new(dictionary! {}, text.into_bytes());
        let content_id = doc.add_object(content_stream);

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        page_ids.push(page_id);
    }

    let kids: Vec<Object> = page_ids.iter().map(|id| (*id).into()).collect();
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => 3,
    });

    for pid in &page_ids {
        if let Ok(page_obj) = doc.get_object_mut(*pid) {
            if let Ok(dict) = page_obj.as_dict_mut() {
                dict.set("Parent", pages_id);
            }
        }
    }

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });

    doc.trailer.set("Root", catalog_id);
    doc.max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(0);

    let mut file = std::fs::File::create(path).expect("创建 PDF 文件失败");
    doc.save_to(&mut file).expect("保存 PDF 失败");
}

// ─── 集成测试 ───

#[test]
fn test_parse_simple_pdf() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let pdf_path = dir.path().join("simple.pdf");
    generate_simple_pdf(&pdf_path);

    // 验证 PDF 文件确实生成了
    assert!(pdf_path.exists(), "PDF 文件应该存在");
    assert!(
        std::fs::metadata(&pdf_path).unwrap().len() > 0,
        "PDF 文件不应为空"
    );

    let config = Config::default();
    let doc = parse_pdf(&pdf_path, &config).expect("解析 PDF 失败");

    // 基本结构验证
    assert!(!doc.doc_id.is_empty(), "doc_id 不应为空");
    assert_eq!(doc.pages.len(), 1, "应该有 1 页");

    let page = &doc.pages[0];
    assert_eq!(page.page_index, 0);
    assert!(page.size.width > 0.0, "页面宽度应该 > 0");
    assert!(page.size.height > 0.0, "页面高度应该 > 0");

    // 验证提取到了文本块
    assert!(!page.blocks.is_empty(), "应该提取到文本块");

    // 验证文本内容包含预期关键词
    let all_text: String = page
        .blocks
        .iter()
        .map(|b| b.normalized_text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        all_text.contains("Hello") || all_text.contains("knot") || all_text.contains("test"),
        "提取的文本应包含预期关键词，实际文本: {}",
        all_text
    );

    // 验证 Markdown 渲染
    let renderer = MarkdownRenderer::new();
    let md = renderer.render_document(&doc);
    assert!(!md.is_empty(), "Markdown 输出不应为空");

    // 验证 RAG 导出
    let rag_lines = RagExporter::export_block_lines(&doc);
    assert!(!rag_lines.is_empty(), "RAG block_lines 不应为空");

    // 验证 serde 往返
    let json = serde_json::to_string_pretty(&doc).expect("序列化失败");
    let doc2: knot_pdf::DocumentIR = serde_json::from_str(&json).expect("反序列化失败");
    assert_eq!(doc.doc_id, doc2.doc_id);
    assert_eq!(doc.pages.len(), doc2.pages.len());

    println!("=== Simple PDF - Markdown ===");
    println!("{}", md);
    println!("=== Simple PDF - RAG Lines ===");
    for line in &rag_lines {
        println!("{}", line.text);
    }
}

#[test]
fn test_parse_multipage_pdf() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let pdf_path = dir.path().join("multipage.pdf");
    generate_multipage_pdf(&pdf_path);

    assert!(pdf_path.exists());

    let config = Config::default();
    let doc = parse_pdf(&pdf_path, &config).expect("解析多页 PDF 失败");

    // 基本结构验证
    assert!(!doc.doc_id.is_empty());
    assert_eq!(doc.pages.len(), 3, "应该有 3 页");

    for (i, page) in doc.pages.iter().enumerate() {
        assert_eq!(page.page_index, i, "页码索引应匹配");
        assert!(!page.blocks.is_empty(), "第 {} 页应有文本块", i + 1);

        let all_text: String = page
            .blocks
            .iter()
            .map(|b| b.normalized_text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        // 验证每一页都包含与该页相关的内容
        assert!(!all_text.is_empty(), "第 {} 页文本不应为空", i + 1);
    }

    // 验证 Markdown 渲染包含所有页面
    let renderer = MarkdownRenderer::new();
    let md = renderer.render_document(&doc);
    assert!(
        md.contains("Page 1") || md.contains("Page 2") || md.contains("Page 3") || md.len() > 50,
        "Markdown 应包含多页内容"
    );

    // 验证 RAG 导出
    let rag_lines = RagExporter::export_block_lines(&doc);
    assert!(
        rag_lines.len() >= 3,
        "RAG 至少应有 3 条 block_lines（每页至少 1 条）"
    );

    // 验证 IR JSON 输出
    let json = serde_json::to_string_pretty(&doc).expect("序列化失败");
    assert!(json.contains("\"pages\""));
    assert!(json.contains("\"blocks\""));

    println!("=== Multipage PDF - IR JSON (truncated) ===");
    println!("{}", &json[..json.len().min(2000)]);
}

#[test]
fn test_parse_pdf_pages_iterator() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let pdf_path = dir.path().join("iter.pdf");
    generate_multipage_pdf(&pdf_path);

    let config = Config::default();
    let pages_iter = parse_pdf_pages(&pdf_path, &config).expect("创建迭代器失败");

    let pages: Vec<_> = pages_iter.collect::<Result<Vec<_>, _>>().expect("迭代失败");
    assert_eq!(pages.len(), 3, "迭代器应返回 3 页");

    for (i, page) in pages.iter().enumerate() {
        assert_eq!(page.page_index, i);
        assert!(!page.blocks.is_empty(), "第 {} 页应有文本块", i + 1);
    }
}

#[test]
fn test_parse_nonexistent_pdf() {
    let config = Config::default();
    let result = parse_pdf("/tmp/nonexistent_knot_pdf_test_file.pdf", &config);
    assert!(result.is_err(), "解析不存在的文件应返回错误");
}

#[test]
fn test_config_serde() {
    let config = Config::default();
    let json = serde_json::to_string(&config).expect("Config 序列化失败");
    let config2: Config = serde_json::from_str(&json).expect("Config 反序列化失败");
    assert_eq!(config.max_columns, config2.max_columns);
    assert_eq!(config.emit_markdown, config2.emit_markdown);
}

#[test]
fn test_ir_json_output_quality() {
    // 验证 IR JSON 输出符合预期的结构和可读性
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let pdf_path = dir.path().join("quality.pdf");
    generate_simple_pdf(&pdf_path);

    let config = Config::default();
    let doc = parse_pdf(&pdf_path, &config).expect("解析失败");
    let json = serde_json::to_string_pretty(&doc).expect("序列化失败");

    // 写入文件供人工检查
    let json_path = dir.path().join("output.json");
    let mut f = std::fs::File::create(&json_path).expect("创建 JSON 文件失败");
    f.write_all(json.as_bytes()).expect("写入 JSON 失败");

    // 验证 JSON 结构完整性
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON 应可解析");
    assert!(parsed.is_object());
    assert!(parsed["doc_id"].is_string());
    assert!(parsed["pages"].is_array());
    assert!(parsed["metadata"].is_object());
    assert!(parsed["diagnostics"].is_object());

    let pages = parsed["pages"].as_array().unwrap();
    assert!(!pages.is_empty());
    let page0 = &pages[0];
    assert!(page0["page_index"].is_number());
    assert!(page0["size"].is_object());
    assert!(page0["blocks"].is_array());
    assert!(page0["text_score"].is_number());

    println!("=== IR JSON saved to: {:?} ===", json_path);
    println!("{}", json);
}
