//! M6 评测样本集验收脚本
//!
//! 功能：
//! 1. 批量解析 eval_samples/ 下所有 PDF → 输出 IR JSON
//! 2. 批量导出扁平化 RAG 索引文本
//! 3. 表格结构正确性自动检查
//! 4. RAG 命中率评测（30 个问答对）

use knot_pdf::render::RagExporter;
use knot_pdf::{parse_pdf, Config};
use std::path::{Path, PathBuf};

const EVAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/eval_samples");
const OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/eval_output");

/// 收集目录下所有 PDF 文件
fn collect_pdfs(dir: &Path) -> Vec<PathBuf> {
    let mut pdfs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "pdf").unwrap_or(false) {
                pdfs.push(path);
            }
        }
    }
    pdfs.sort();
    pdfs
}

/// 解析单个 PDF 并返回统计
struct ParseResult {
    filename: String,
    pages: usize,
    blocks: usize,
    tables: usize,
    images: usize,
    total_text_chars: usize,
    table_rows: usize,
    table_cells: usize,
    warnings: usize,
    parse_ok: bool,
    error: Option<String>,
}

fn parse_single(path: &Path, config: &Config) -> ParseResult {
    let filename = path.file_name().unwrap().to_string_lossy().to_string();

    match parse_pdf(path.to_str().unwrap(), config) {
        Ok(doc) => {
            let pages = doc.pages.len();
            let blocks: usize = doc.pages.iter().map(|p| p.blocks.len()).sum();
            let tables: usize = doc.pages.iter().map(|p| p.tables.len()).sum();
            let images: usize = doc.pages.iter().map(|p| p.images.len()).sum();
            let total_text_chars: usize = doc
                .pages
                .iter()
                .flat_map(|p| p.blocks.iter())
                .map(|b| b.normalized_text.len())
                .sum();
            let table_rows: usize = doc
                .pages
                .iter()
                .flat_map(|p| p.tables.iter())
                .map(|t| t.rows.len())
                .sum();
            let table_cells: usize = doc
                .pages
                .iter()
                .flat_map(|p| p.tables.iter())
                .flat_map(|t| t.rows.iter())
                .map(|r| r.cells.len())
                .sum();
            let warnings = doc.diagnostics.warnings.len();

            // 导出 IR JSON
            let json_dir = Path::new(OUTPUT_DIR).join("ir_json");
            std::fs::create_dir_all(&json_dir).ok();
            let json_path = json_dir.join(format!("{}.json", filename));
            if let Ok(json) = serde_json::to_string_pretty(&doc) {
                std::fs::write(&json_path, json).ok();
            }

            // 导出 RAG 扁平化文本
            let rag_dir = Path::new(OUTPUT_DIR).join("rag_text");
            std::fs::create_dir_all(&rag_dir).ok();
            let rag_lines = RagExporter::export_all(&doc);
            let rag_text: String = rag_lines.iter().map(|l| format!("{}\n", l.text)).collect();
            let rag_path = rag_dir.join(format!("{}.txt", filename));
            std::fs::write(&rag_path, rag_text).ok();

            ParseResult {
                filename,
                pages,
                blocks,
                tables,
                images,
                total_text_chars,
                table_rows,
                table_cells,
                warnings,
                parse_ok: true,
                error: None,
            }
        }
        Err(e) => ParseResult {
            filename,
            pages: 0,
            blocks: 0,
            tables: 0,
            images: 0,
            total_text_chars: 0,
            table_rows: 0,
            table_cells: 0,
            warnings: 0,
            parse_ok: false,
            error: Some(e.to_string()),
        },
    }
}

// ============================================================
// 1. 批量解析 + 输出 IR JSON + RAG 文本
// ============================================================

#[test]
fn test_batch_parse_born_digital() {
    let dir = Path::new(EVAL_DIR).join("born_digital");
    if !dir.exists() {
        eprintln!("Skip: eval_samples/born_digital not found");
        return;
    }

    let pdfs = collect_pdfs(&dir);
    assert!(!pdfs.is_empty(), "No PDFs found in born_digital/");

    let config = Config::default();
    println!("=== Born-Digital Batch Parse ({} files) ===", pdfs.len());
    println!(
        "{:<35} {:>5} {:>6} {:>6} {:>8} {:>5}",
        "File", "Pages", "Blocks", "Tables", "Chars", "Warn"
    );

    let mut total_ok = 0;
    let mut total_fail = 0;

    for pdf in &pdfs {
        let r = parse_single(pdf, &config);
        if r.parse_ok {
            total_ok += 1;
            println!(
                "{:<35} {:>5} {:>6} {:>6} {:>8} {:>5}",
                r.filename, r.pages, r.blocks, r.tables, r.total_text_chars, r.warnings
            );
        } else {
            total_fail += 1;
            println!("{:<35} FAIL: {}", r.filename, r.error.unwrap_or_default());
        }
    }

    println!("---");
    println!("OK: {}, FAIL: {}", total_ok, total_fail);
    assert_eq!(total_fail, 0, "Some born-digital PDFs failed to parse");
    assert_eq!(total_ok, 10, "Expected 10 born-digital PDFs");
}

#[test]
fn test_batch_parse_stream_tables() {
    let dir = Path::new(EVAL_DIR).join("tables_stream");
    if !dir.exists() {
        return;
    }

    let pdfs = collect_pdfs(&dir);
    let config = Config::default();
    println!("=== Stream Tables Batch Parse ({} files) ===", pdfs.len());
    println!(
        "{:<25} {:>6} {:>6} {:>6} {:>8}",
        "File", "Tables", "Rows", "Cells", "Fallback"
    );

    let mut tables_found = 0;
    let mut tables_with_fallback = 0;
    let mut total_rows = 0;

    for pdf in &pdfs {
        let r = parse_single(pdf, &config);
        assert!(r.parse_ok, "Failed: {}", r.filename);
        tables_found += r.tables;
        total_rows += r.table_rows;

        // 检查 fallback_text
        let doc = parse_pdf(pdf.to_str().unwrap(), &config).unwrap();
        let has_fallback = doc
            .pages
            .iter()
            .flat_map(|p| p.tables.iter())
            .all(|t| !t.fallback_text.is_empty());
        if has_fallback {
            tables_with_fallback += r.tables;
        }

        println!(
            "{:<25} {:>6} {:>6} {:>6} {:>8}",
            r.filename,
            r.tables,
            r.table_rows,
            r.table_cells,
            if has_fallback { "yes" } else { "NO" }
        );
    }

    println!("---");
    println!(
        "Total tables: {}, with fallback: {}, total rows: {}",
        tables_found, tables_with_fallback, total_rows
    );
    assert_eq!(pdfs.len(), 20, "Expected 20 stream table PDFs");
}

#[test]
fn test_batch_parse_ruled_tables() {
    let dir = Path::new(EVAL_DIR).join("tables_ruled");
    if !dir.exists() {
        return;
    }

    let pdfs = collect_pdfs(&dir);
    let config = Config::default();
    println!("=== Ruled Tables Batch Parse ({} files) ===", pdfs.len());
    println!(
        "{:<25} {:>6} {:>6} {:>6}",
        "File", "Tables", "Rows", "Cells"
    );

    let mut tables_found = 0;

    for pdf in &pdfs {
        let r = parse_single(pdf, &config);
        assert!(r.parse_ok, "Failed: {}", r.filename);
        tables_found += r.tables;
        println!(
            "{:<25} {:>6} {:>6} {:>6}",
            r.filename, r.tables, r.table_rows, r.table_cells
        );
    }

    println!("---");
    println!("Total tables found: {}", tables_found);
    assert_eq!(pdfs.len(), 20, "Expected 20 ruled table PDFs");
}

#[test]
fn test_batch_parse_complex_tables() {
    let dir = Path::new(EVAL_DIR).join("tables_complex");
    if !dir.exists() {
        return;
    }

    let pdfs = collect_pdfs(&dir);
    let config = Config::default();
    println!("=== Complex Tables Batch Parse ({} files) ===", pdfs.len());
    println!(
        "{:<40} {:>5} {:>6} {:>6} {:>8} {:>5}",
        "File", "Pages", "Tables", "Rows", "Chars", "Warn"
    );

    for pdf in &pdfs {
        let r = parse_single(pdf, &config);
        assert!(r.parse_ok, "Failed: {}", r.filename);
        println!(
            "{:<40} {:>5} {:>6} {:>6} {:>8} {:>5}",
            r.filename, r.pages, r.tables, r.table_rows, r.total_text_chars, r.warnings
        );
    }

    assert_eq!(pdfs.len(), 10, "Expected 10 complex table PDFs");
}

#[test]
fn test_batch_parse_scanned() {
    let dir = Path::new(EVAL_DIR).join("scanned");
    if !dir.exists() {
        return;
    }

    let pdfs = collect_pdfs(&dir);
    let config = Config::default();
    println!("=== Scanned PDFs Batch Parse ({} files) ===", pdfs.len());

    for pdf in &pdfs {
        let r = parse_single(pdf, &config);
        assert!(r.parse_ok, "Failed: {}", r.filename);
        println!(
            "{:<30} pages={} blocks={} chars={}",
            r.filename, r.pages, r.blocks, r.total_text_chars
        );
    }

    assert_eq!(pdfs.len(), 5, "Expected 5 scanned PDFs");
}

// ============================================================
// 2. 表格结构正确性检查
// ============================================================

#[test]
fn test_table_structure_validation() {
    let config = Config::default();

    println!("=== Table Structure Validation ===");

    let mut total_tables = 0;
    let mut tables_with_headers = 0;
    let mut tables_with_fallback = 0;
    let mut tables_with_rows = 0;

    // 检查所有表格目录
    for subdir in &["tables_stream", "tables_ruled", "tables_complex"] {
        let dir = Path::new(EVAL_DIR).join(subdir);
        if !dir.exists() {
            continue;
        }

        let pdfs = collect_pdfs(&dir);
        for pdf in &pdfs {
            let doc = match parse_pdf(pdf.to_str().unwrap(), &config) {
                Ok(d) => d,
                Err(_) => continue,
            };

            for page in &doc.pages {
                for table in &page.tables {
                    total_tables += 1;

                    // 检查 1: fallback_text 非空
                    if !table.fallback_text.is_empty() {
                        tables_with_fallback += 1;
                    }

                    // 检查 2: 有数据行
                    if !table.rows.is_empty() {
                        tables_with_rows += 1;
                    }

                    // 检查 3: headers 非空
                    if !table.headers.is_empty() {
                        tables_with_headers += 1;
                    }
                }
            }
        }
    }

    println!("Total tables:          {}", total_tables);
    println!(
        "With fallback_text:    {} ({:.0}%)",
        tables_with_fallback,
        if total_tables > 0 {
            tables_with_fallback as f64 / total_tables as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "With rows:             {} ({:.0}%)",
        tables_with_rows,
        if total_tables > 0 {
            tables_with_rows as f64 / total_tables as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "With headers:          {} ({:.0}%)",
        tables_with_headers,
        if total_tables > 0 {
            tables_with_headers as f64 / total_tables as f64 * 100.0
        } else {
            0.0
        }
    );

    // 断言：所有表格必须有 fallback_text
    assert_eq!(
        tables_with_fallback, total_tables,
        "All tables must have fallback_text (info: {}/{})",
        tables_with_fallback, total_tables
    );
}

// ============================================================
// 2b. 表格列映射正确率评测
// ============================================================

/// 列映射正确率：每行的 cells 数量与该表 headers 列数一致的行数 / 总行数
fn column_mapping_accuracy(
    dir: &Path,
    config: &Config,
) -> (usize, usize, Vec<(String, usize, usize)>) {
    let pdfs = collect_pdfs(dir);
    let mut total_rows = 0usize;
    let mut correct_rows = 0usize;
    let mut per_file: Vec<(String, usize, usize)> = Vec::new();

    for pdf in &pdfs {
        let filename = pdf.file_name().unwrap().to_string_lossy().to_string();
        let doc = match parse_pdf(pdf.to_str().unwrap(), config) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let mut file_total = 0usize;
        let mut file_correct = 0usize;

        for page in &doc.pages {
            for table in &page.tables {
                let expected_cols = if !table.headers.is_empty() {
                    table.headers.len()
                } else if let Some(first_row) = table.rows.first() {
                    first_row.cells.len()
                } else {
                    continue;
                };

                for row in &table.rows {
                    file_total += 1;
                    if row.cells.len() == expected_cols {
                        file_correct += 1;
                    }
                }
            }
        }

        per_file.push((filename, file_correct, file_total));
        total_rows += file_total;
        correct_rows += file_correct;
    }

    (correct_rows, total_rows, per_file)
}

#[test]
fn test_stream_column_mapping_accuracy() {
    let dir = Path::new(EVAL_DIR).join("tables_stream");
    if !dir.exists() {
        eprintln!("Skip: eval_samples/tables_stream not found");
        return;
    }

    let config = Config::default();
    let (correct, total, per_file) = column_mapping_accuracy(&dir, &config);

    println!("=== Stream Table Column Mapping Accuracy ===");
    println!(
        "{:<25} {:>8} {:>8} {:>8}",
        "File", "Correct", "Total", "Rate"
    );
    for (name, c, t) in &per_file {
        let rate = if *t > 0 {
            *c as f64 / *t as f64 * 100.0
        } else {
            100.0
        };
        println!("{:<25} {:>8} {:>8} {:>7.1}%", name, c, t, rate);
    }

    let rate = if total > 0 {
        correct as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    println!("---");
    println!("Overall: {}/{} ({:.1}%)", correct, total, rate);

    assert!(
        rate >= 70.0,
        "Stream column mapping accuracy {:.1}% is below 70% threshold ({}/{})",
        rate,
        correct,
        total
    );
    println!("=== PASS (stream column mapping: {:.1}%) ===", rate);
}

#[test]
fn test_ruled_column_mapping_accuracy() {
    let dir = Path::new(EVAL_DIR).join("tables_ruled");
    if !dir.exists() {
        eprintln!("Skip: eval_samples/tables_ruled not found");
        return;
    }

    let config = Config::default();
    let (correct, total, per_file) = column_mapping_accuracy(&dir, &config);

    println!("=== Ruled Table Column Mapping Accuracy ===");
    println!(
        "{:<25} {:>8} {:>8} {:>8}",
        "File", "Correct", "Total", "Rate"
    );
    for (name, c, t) in &per_file {
        let rate = if *t > 0 {
            *c as f64 / *t as f64 * 100.0
        } else {
            100.0
        };
        println!("{:<25} {:>8} {:>8} {:>7.1}%", name, c, t, rate);
    }

    let rate = if total > 0 {
        correct as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    println!("---");
    println!("Overall: {}/{} ({:.1}%)", correct, total, rate);

    assert!(
        rate >= 80.0,
        "Ruled column mapping accuracy {:.1}% is below 80% threshold ({}/{})",
        rate,
        correct,
        total
    );
    println!("=== PASS (ruled column mapping: {:.1}%) ===", rate);
}

// ============================================================
// 3. RAG 命中率评测
// ============================================================

/// QA 对：(PDF 文件路径模式, 查询关键词, 期望在 RAG 输出中匹配的关键词)
struct QaPair {
    /// PDF 来源子目录
    source_dir: &'static str,
    /// PDF 文件名 pattern（前缀匹配）
    file_pattern: &'static str,
    /// 查询描述
    query: &'static str,
    /// 期望在 RAG 扁平化文本中找到的关键词（至少一个命中即算通过）
    expected_keywords: &'static [&'static str],
}

#[test]
fn test_rag_hit_rate() {
    let config = Config::default();

    // 30 个 QA 对
    let qa_pairs = vec![
        // --- born-digital 文本查找 ---
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd01",
            query: "Document Processing 章节",
            expected_keywords: &["Chapter", "Document Processing"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd03",
            query: "财报中的 Revenue 数据",
            expected_keywords: &["Revenue"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd03",
            query: "COGS 数据",
            expected_keywords: &["COGS", "Cost"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd03",
            query: "Net Income 数据",
            expected_keywords: &["Net Income"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd04",
            query: "学术论文 Abstract",
            expected_keywords: &["table extraction", "PDF"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd05",
            query: "合同条款中的终止条件",
            expected_keywords: &["terminate", "notice"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd06",
            query: "发票总金额",
            expected_keywords: &["TOTAL", "Subtotal", "Tax"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd06",
            query: "发票中的 Software License",
            expected_keywords: &["Software License"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd09",
            query: "Feature Checklist",
            expected_keywords: &["Text Extraction", "Table Extraction"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd10",
            query: "目录大纲章节列表",
            expected_keywords: &["Introduction", "Architecture"],
        },
        // --- Stream 表格数据查找 ---
        QaPair {
            source_dir: "tables_stream",
            file_pattern: "stream_01",
            query: "stream 表格数据",
            expected_keywords: &["Item_", "Col_"],
        },
        QaPair {
            source_dir: "tables_stream",
            file_pattern: "stream_05",
            query: "stream_05 表格数据",
            expected_keywords: &["Item_", "$"],
        },
        QaPair {
            source_dir: "tables_stream",
            file_pattern: "stream_10",
            query: "stream_10 产品数据",
            expected_keywords: &["R1", "$"],
        },
        QaPair {
            source_dir: "tables_stream",
            file_pattern: "stream_15",
            query: "stream_15 区域数据",
            expected_keywords: &["R1", "$"],
        },
        QaPair {
            source_dir: "tables_stream",
            file_pattern: "stream_20",
            query: "stream_20 ID 数据",
            expected_keywords: &["R1", "$"],
        },
        // --- Ruled 表格数据查找 ---
        QaPair {
            source_dir: "tables_ruled",
            file_pattern: "ruled_01",
            query: "ruled_01 月度数据",
            expected_keywords: &["Row", "$"],
        },
        QaPair {
            source_dir: "tables_ruled",
            file_pattern: "ruled_05",
            query: "ruled_05 指标数据",
            expected_keywords: &["Row", "$"],
        },
        QaPair {
            source_dir: "tables_ruled",
            file_pattern: "ruled_10",
            query: "ruled_10 部门数据",
            expected_keywords: &["Row"],
        },
        QaPair {
            source_dir: "tables_ruled",
            file_pattern: "ruled_15",
            query: "ruled_15 商品数据",
            expected_keywords: &["Row"],
        },
        QaPair {
            source_dir: "tables_ruled",
            file_pattern: "ruled_20",
            query: "ruled_20 表格汇总",
            expected_keywords: &["Row", "$"],
        },
        // --- Complex 表格 ---
        QaPair {
            source_dir: "tables_complex",
            file_pattern: "complex_01",
            query: "超宽表格数据",
            expected_keywords: &["C1", "Wide"],
        },
        QaPair {
            source_dir: "tables_complex",
            file_pattern: "complex_02",
            query: "超长表格数据",
            expected_keywords: &["Item description", "Active"],
        },
        QaPair {
            source_dir: "tables_complex",
            file_pattern: "complex_07",
            query: "微型表格 Revenue",
            expected_keywords: &["Revenue", "1,234,567"],
        },
        QaPair {
            source_dir: "tables_complex",
            file_pattern: "complex_08",
            query: "相邻表格 Table 1",
            expected_keywords: &["T1_R1", "$"],
        },
        QaPair {
            source_dir: "tables_complex",
            file_pattern: "complex_09",
            query: "Q1 和 Q2 数据",
            expected_keywords: &["Q1", "Q2"],
        },
        QaPair {
            source_dir: "tables_complex",
            file_pattern: "complex_10",
            query: "纯数字表格数据",
            expected_keywords: &["V1"],
        },
        // --- 跨文件查询 ---
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd06",
            query: "Training 培训费用",
            expected_keywords: &["Training"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd03",
            query: "EBITDA 数据",
            expected_keywords: &["EBITDA"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd07",
            query: "申请人信息",
            expected_keywords: &["John Smith", "Senior Software"],
        },
        QaPair {
            source_dir: "born_digital",
            file_pattern: "bd05",
            query: "付款金额",
            expected_keywords: &["$", "month"],
        },
    ];

    assert_eq!(qa_pairs.len(), 30, "Must have 30 QA pairs");

    println!("=== RAG Hit Rate Evaluation (30 QA pairs) ===");
    println!("{:<5} {:<20} {:<40} {:>6}", "#", "File", "Query", "Hit?");

    let mut hits = 0;
    let mut misses = 0;

    for (i, qa) in qa_pairs.iter().enumerate() {
        let dir = Path::new(EVAL_DIR).join(qa.source_dir);
        let pdfs = collect_pdfs(&dir);
        let pdf = pdfs.iter().find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(qa.file_pattern)
        });

        let pdf = match pdf {
            Some(p) => p,
            None => {
                println!(
                    "{:<5} {:<20} {:<40} {:>6}",
                    i + 1,
                    qa.file_pattern,
                    qa.query,
                    "SKIP"
                );
                continue;
            }
        };

        let doc = match parse_pdf(pdf.to_str().unwrap(), &config) {
            Ok(d) => d,
            Err(_) => {
                println!(
                    "{:<5} {:<20} {:<40} {:>6}",
                    i + 1,
                    qa.file_pattern,
                    qa.query,
                    "ERR"
                );
                misses += 1;
                continue;
            }
        };

        // 导出 RAG 文本
        let rag_lines = RagExporter::export_all(&doc);
        let all_text: String = rag_lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // 检查关键词命中
        let hit = qa.expected_keywords.iter().any(|kw| all_text.contains(kw));

        if hit {
            hits += 1;
            println!(
                "{:<5} {:<20} {:<40} {:>6}",
                i + 1,
                qa.file_pattern,
                qa.query,
                "HIT"
            );
        } else {
            misses += 1;
            println!(
                "{:<5} {:<20} {:<40} {:>6}",
                i + 1,
                qa.file_pattern,
                qa.query,
                "MISS"
            );
            // 调试：显示 RAG 文本前 200 字符
            let preview: String = all_text.chars().take(200).collect();
            println!("      Preview: {}...", preview);
        }
    }

    let total = hits + misses;
    let rate = if total > 0 {
        hits as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    println!("\n=== RAG Hit Rate Report ===");
    println!("Total:  {}", total);
    println!("Hits:   {} ({:.1}%)", hits, rate);
    println!("Misses: {} ({:.1}%)", misses, 100.0 - rate);

    // 目标：至少 80% 命中率
    assert!(
        rate >= 80.0,
        "RAG hit rate {:.1}% is below minimum 80% threshold ({}/{})",
        rate,
        hits,
        total
    );

    println!("=== PASS (hit rate: {:.1}%) ===", rate);
}
