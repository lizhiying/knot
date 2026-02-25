//! IR 类型单元测试 + serde 序列化/反序列化往返测试

use knot_pdf::ir::*;

/// 构造一个完整的 DocumentIR 用于测试
fn make_test_document() -> DocumentIR {
    DocumentIR {
        doc_id: "test_doc_hash_abc123".to_string(),
        metadata: DocumentMetadata {
            title: Some("测试文档".to_string()),
            author: Some("测试作者".to_string()),
            subject: Some("单元测试".to_string()),
            creator: Some("knot-pdf".to_string()),
            producer: Some("knot-pdf-test".to_string()),
            creation_date: Some("2025-01-01".to_string()),
            modification_date: None,
        },
        outline: vec![OutlineItem {
            title: "第一章".to_string(),
            page_index: Some(0),
            children: vec![OutlineItem {
                title: "1.1 节".to_string(),
                page_index: Some(1),
                children: vec![],
            }],
        }],
        pages: vec![make_test_page(0), make_test_page(1)],
        diagnostics: Diagnostics {
            warnings: vec!["test warning".to_string()],
            errors: vec![],
        },
    }
}

fn make_test_page(index: usize) -> PageIR {
    PageIR {
        page_index: index,
        size: PageSize {
            width: 612.0,
            height: 792.0,
        },
        rotation: 0.0,
        blocks: vec![
            BlockIR {
                block_id: format!("block_p{}_0", index),
                bbox: BBox::new(72.0, 72.0, 468.0, 20.0),
                role: BlockRole::Title,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: "测试标题".to_string(),
                        font_size: Some(18.0),
                        is_bold: true,
                        font_name: Some("SimSun".to_string()),
                    }],
                    bbox: Some(BBox::new(72.0, 72.0, 468.0, 20.0)),
                }],
                normalized_text: "测试标题".to_string(),
            },
            BlockIR {
                block_id: format!("block_p{}_1", index),
                bbox: BBox::new(72.0, 100.0, 468.0, 60.0),
                role: BlockRole::Body,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: "这是正文内容，用于测试 IR 的序列化和反序列化。".to_string(),
                        font_size: Some(12.0),
                        is_bold: false,
                        font_name: None,
                    }],
                    bbox: Some(BBox::new(72.0, 100.0, 468.0, 14.0)),
                }],
                normalized_text: "这是正文内容，用于测试 IR 的序列化和反序列化。".to_string(),
            },
        ],
        tables: vec![TableIR {
            table_id: format!("table_p{}_0", index),
            page_index: index,
            bbox: BBox::new(72.0, 200.0, 468.0, 100.0),
            extraction_mode: ExtractionMode::Unknown,
            headers: vec!["名称".to_string(), "数值".to_string(), "百分比".to_string()],
            rows: vec![TableRow {
                row_index: 0,
                cells: vec![
                    TableCell {
                        row: 0,
                        col: 0,
                        text: "项目A".to_string(),
                        cell_type: CellType::Text,
                        rowspan: 1,
                        colspan: 1,
                    },
                    TableCell {
                        row: 0,
                        col: 1,
                        text: "100".to_string(),
                        cell_type: CellType::Number,
                        rowspan: 1,
                        colspan: 1,
                    },
                    TableCell {
                        row: 0,
                        col: 2,
                        text: "50%".to_string(),
                        cell_type: CellType::Percent,
                        rowspan: 1,
                        colspan: 1,
                    },
                ],
            }],
            column_types: vec![CellType::Text, CellType::Number, CellType::Percent],
            fallback_text: "名称 数值 百分比\n项目A 100 50%".to_string(),
        }],
        images: vec![ImageIR {
            image_id: format!("img_p{}_0", index),
            page_index: index,
            bbox: BBox::new(72.0, 350.0, 200.0, 150.0),
            format: ImageFormat::Png,
            bytes_ref: None,
            caption_refs: vec![format!("block_p{}_1", index)],
            source: ImageSource::Embedded,
            ocr_text: None,
        }],
        diagnostics: PageDiagnostics {
            warnings: vec![],
            errors: vec![],
            block_count: 2,
            table_count: 1,
            image_count: 1,
            ocr_quality_score: None,
        },
        text_score: 0.85,
        is_scanned_guess: false,
        source: PageSource::BornDigital,
        timings: Timings {
            extract_ms: Some(42),
            render_ms: Some(5),
            ocr_ms: None,
            ..Default::default()
        },
    }
}

// ─── serde 往返测试 ───

#[test]
fn test_document_ir_serde_roundtrip() {
    let doc = make_test_document();
    let json = serde_json::to_string_pretty(&doc).expect("序列化失败");
    let doc2: DocumentIR = serde_json::from_str(&json).expect("反序列化失败");

    assert_eq!(doc.doc_id, doc2.doc_id);
    assert_eq!(doc.pages.len(), doc2.pages.len());
    assert_eq!(doc.metadata.title, doc2.metadata.title);
    assert_eq!(doc.metadata.author, doc2.metadata.author);
    assert_eq!(doc.outline.len(), doc2.outline.len());
    assert_eq!(doc.outline[0].title, doc2.outline[0].title);
    assert_eq!(
        doc.outline[0].children.len(),
        doc2.outline[0].children.len()
    );
    assert_eq!(
        doc.diagnostics.warnings.len(),
        doc2.diagnostics.warnings.len()
    );
}

#[test]
fn test_page_ir_serde_roundtrip() {
    let page = make_test_page(0);
    let json = serde_json::to_string(&page).expect("序列化失败");
    let page2: PageIR = serde_json::from_str(&json).expect("反序列化失败");

    assert_eq!(page.page_index, page2.page_index);
    assert_eq!(page.size.width, page2.size.width);
    assert_eq!(page.size.height, page2.size.height);
    assert_eq!(page.rotation, page2.rotation);
    assert_eq!(page.blocks.len(), page2.blocks.len());
    assert_eq!(page.tables.len(), page2.tables.len());
    assert_eq!(page.images.len(), page2.images.len());
    assert_eq!(page.text_score, page2.text_score);
    assert_eq!(page.is_scanned_guess, page2.is_scanned_guess);
    assert_eq!(page.source, page2.source);
}

#[test]
fn test_block_ir_serde_roundtrip() {
    let block = BlockIR {
        block_id: "b1".to_string(),
        bbox: BBox::new(10.0, 20.0, 300.0, 50.0),
        role: BlockRole::List,
        lines: vec![
            TextLine {
                spans: vec![TextSpan {
                    text: "列表项 1".to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(BBox::new(10.0, 20.0, 300.0, 14.0)),
            },
            TextLine {
                spans: vec![TextSpan {
                    text: "列表项 2".to_string(),
                    font_size: Some(12.0),
                    is_bold: true,
                    font_name: Some("Arial".to_string()),
                }],
                bbox: None,
            },
        ],
        normalized_text: "列表项 1\n列表项 2".to_string(),
    };
    let json = serde_json::to_string(&block).expect("序列化失败");
    let block2: BlockIR = serde_json::from_str(&json).expect("反序列化失败");

    assert_eq!(block.block_id, block2.block_id);
    assert_eq!(block.role, block2.role);
    assert_eq!(block.lines.len(), block2.lines.len());
    assert_eq!(block.lines[0].text(), block2.lines[0].text());
    assert_eq!(
        block.lines[1].spans[0].is_bold,
        block2.lines[1].spans[0].is_bold
    );
    assert_eq!(block.normalized_text, block2.normalized_text);
}

#[test]
fn test_table_ir_serde_roundtrip() {
    let table = TableIR {
        table_id: "t1".to_string(),
        page_index: 0,
        bbox: BBox::new(0.0, 0.0, 500.0, 200.0),
        extraction_mode: ExtractionMode::Ruled,
        headers: vec!["A".to_string(), "B".to_string()],
        rows: vec![TableRow {
            row_index: 0,
            cells: vec![
                TableCell {
                    row: 0,
                    col: 0,
                    text: "a1".to_string(),
                    cell_type: CellType::Text,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 0,
                    col: 1,
                    text: "99.5".to_string(),
                    cell_type: CellType::Number,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        }],
        column_types: vec![CellType::Text, CellType::Number],
        fallback_text: "A B\na1 99.5".to_string(),
    };
    let json = serde_json::to_string(&table).expect("序列化失败");
    let table2: TableIR = serde_json::from_str(&json).expect("反序列化失败");

    assert_eq!(table.table_id, table2.table_id);
    assert_eq!(table.extraction_mode, table2.extraction_mode);
    assert_eq!(table.headers, table2.headers);
    assert_eq!(table.rows.len(), table2.rows.len());
    assert_eq!(table.rows[0].cells[0].text, table2.rows[0].cells[0].text);
    assert_eq!(
        table.rows[0].cells[1].cell_type,
        table2.rows[0].cells[1].cell_type
    );
    assert_eq!(table.fallback_text, table2.fallback_text);
}

#[test]
fn test_image_ir_serde_roundtrip() {
    let img = ImageIR {
        image_id: "i1".to_string(),
        page_index: 2,
        bbox: BBox::new(50.0, 50.0, 400.0, 300.0),
        format: ImageFormat::Jpg,
        bytes_ref: Some(vec![0xFF, 0xD8, 0xFF]), // bytes_ref 被 serde skip，反序列化后应为 None
        caption_refs: vec!["block_3".to_string()],
        source: ImageSource::Embedded,
        ocr_text: None,
    };
    let json = serde_json::to_string(&img).expect("序列化失败");
    let img2: ImageIR = serde_json::from_str(&json).expect("反序列化失败");

    assert_eq!(img.image_id, img2.image_id);
    assert_eq!(img.page_index, img2.page_index);
    assert_eq!(img.format, img2.format);
    assert!(
        img2.bytes_ref.is_none(),
        "bytes_ref should be None after serde roundtrip (skip)"
    );
    assert_eq!(img.caption_refs, img2.caption_refs);
}

// ─── BBox 基础逻辑测试 ───

#[test]
fn test_bbox_overlap() {
    let a = BBox::new(0.0, 0.0, 100.0, 100.0);
    let b = BBox::new(50.0, 50.0, 100.0, 100.0);
    let c = BBox::new(200.0, 200.0, 50.0, 50.0);

    assert!(a.overlaps(&b));
    assert!(b.overlaps(&a));
    assert!(!a.overlaps(&c));
    assert!(!c.overlaps(&a));
}

#[test]
fn test_bbox_overlap_area() {
    let a = BBox::new(0.0, 0.0, 100.0, 100.0);
    let b = BBox::new(50.0, 50.0, 100.0, 100.0);

    let area = a.overlap_area(&b);
    assert!(
        (area - 2500.0).abs() < 0.01,
        "overlap area should be 50*50=2500, got {}",
        area
    );

    let c = BBox::new(200.0, 200.0, 50.0, 50.0);
    assert_eq!(a.overlap_area(&c), 0.0);
}

#[test]
fn test_bbox_geometry() {
    let b = BBox::new(10.0, 20.0, 100.0, 50.0);
    assert_eq!(b.right(), 110.0);
    assert_eq!(b.bottom(), 70.0);
    assert_eq!(b.center_x(), 60.0);
    assert_eq!(b.center_y(), 45.0);
    assert_eq!(b.area(), 5000.0);
}

// ─── 枚举默认值测试 ───

#[test]
fn test_enum_defaults() {
    assert_eq!(BlockRole::default(), BlockRole::Body);
    assert_eq!(ImageFormat::default(), ImageFormat::Unknown);
    assert_eq!(ExtractionMode::default(), ExtractionMode::Unknown);
    assert_eq!(CellType::default(), CellType::Unknown);
    assert_eq!(PageSource::default(), PageSource::BornDigital);
}

// ─── TableIR 导出方法测试 ───

#[test]
fn test_table_to_csv() {
    let table = TableIR {
        table_id: "t1".to_string(),
        page_index: 0,
        bbox: BBox::new(0.0, 0.0, 100.0, 100.0),
        extraction_mode: ExtractionMode::Unknown,
        headers: vec!["Name".to_string(), "Value".to_string()],
        rows: vec![TableRow {
            row_index: 0,
            cells: vec![
                TableCell {
                    row: 0,
                    col: 0,
                    text: "X".to_string(),
                    cell_type: CellType::Text,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 0,
                    col: 1,
                    text: "42".to_string(),
                    cell_type: CellType::Number,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        }],
        column_types: vec![CellType::Text, CellType::Number],
        fallback_text: "Name Value\nX 42".to_string(),
    };
    let csv = table.to_csv();
    assert!(csv.contains("Name,Value"));
    assert!(csv.contains("X,42"));
}

#[test]
fn test_table_to_markdown() {
    let table = TableIR {
        table_id: "t1".to_string(),
        page_index: 0,
        bbox: BBox::new(0.0, 0.0, 100.0, 100.0),
        extraction_mode: ExtractionMode::Unknown,
        headers: vec!["A".to_string(), "B".to_string()],
        rows: vec![TableRow {
            row_index: 0,
            cells: vec![
                TableCell {
                    row: 0,
                    col: 0,
                    text: "hello".to_string(),
                    cell_type: CellType::Text,
                    rowspan: 1,
                    colspan: 1,
                },
                TableCell {
                    row: 0,
                    col: 1,
                    text: "world".to_string(),
                    cell_type: CellType::Text,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        }],
        column_types: vec![CellType::Text, CellType::Text],
        fallback_text: "".to_string(),
    };
    let md = table.to_markdown();
    assert!(md.contains("| A | B |"));
    assert!(md.contains("| --- | --- |"));
    assert!(md.contains("| hello | world |"));
}

// ─── BlockIR / TextLine 辅助方法测试 ───

#[test]
fn test_block_full_text() {
    let block = BlockIR {
        block_id: "b1".to_string(),
        bbox: BBox::new(0.0, 0.0, 100.0, 50.0),
        role: BlockRole::Body,
        lines: vec![
            TextLine {
                spans: vec![
                    TextSpan {
                        text: "Hello ".to_string(),
                        font_size: None,
                        is_bold: false,
                        font_name: None,
                    },
                    TextSpan {
                        text: "World".to_string(),
                        font_size: None,
                        is_bold: true,
                        font_name: None,
                    },
                ],
                bbox: None,
            },
            TextLine {
                spans: vec![TextSpan {
                    text: "Line 2".to_string(),
                    font_size: None,
                    is_bold: false,
                    font_name: None,
                }],
                bbox: None,
            },
        ],
        normalized_text: "Hello World\nLine 2".to_string(),
    };
    assert_eq!(block.full_text(), "Hello World\nLine 2");
    assert_eq!(block.lines[0].text(), "Hello World");
}

// ─── JSON 输出格式验证 ───

#[test]
fn test_document_ir_json_output() {
    let doc = make_test_document();
    let json = serde_json::to_string_pretty(&doc).expect("序列化失败");

    // 验证 JSON 包含关键字段
    assert!(json.contains("\"doc_id\""));
    assert!(json.contains("\"metadata\""));
    assert!(json.contains("\"pages\""));
    assert!(json.contains("\"blocks\""));
    assert!(json.contains("\"tables\""));
    assert!(json.contains("\"images\""));
    assert!(json.contains("\"text_score\""));
    assert!(json.contains("\"diagnostics\""));
    assert!(json.contains("\"outline\""));

    // 验证中文内容没有被转义（serde_json 默认保留 UTF-8）
    assert!(json.contains("测试文档"));
    assert!(json.contains("测试标题"));

    // 打印出来供人工检查（cargo test -- --nocapture 可查看）
    println!("=== DocumentIR JSON ===");
    println!("{}", json);
    println!("=== END ===");
}
