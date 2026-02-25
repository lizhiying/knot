//! `rag` 子命令：解析 PDF 并导出 RAG 扁平化文本

use clap::Args;
use knot_pdf::{parse_pdf, Config, PdfError, RagExporter};
use std::path::PathBuf;
use std::time::Instant;

use crate::utils::output::{check_input_exists, write_output};
use crate::utils::page_range;

/// RAG 输出格式
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum RagFormat {
    /// 纯文本（每行一条）
    Text,
    /// JSON Lines（每行一个 JSON 对象）
    Jsonl,
    /// CSV 格式
    Csv,
}

/// RAG 行类型过滤
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum RagTypeFilter {
    /// 所有类型
    All,
    /// 仅文本块
    Blocks,
    /// 仅表格行
    TableRows,
    /// 仅表格单元格
    TableCells,
}

/// 解析 PDF 并导出 RAG 扁平化文本
#[derive(Args)]
pub struct RagArgs {
    /// 输入 PDF 文件路径
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// 输出文件路径（默认输出到 stdout）
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// 页码范围，如 "1-5" 或 "1,3,5"（默认全部）
    #[arg(long, value_name = "RANGE")]
    pages: Option<String>,

    /// 输出格式
    #[arg(long, value_enum, default_value = "text")]
    format: RagFormat,

    /// 行类型过滤
    #[arg(long, value_enum, default_value = "all", value_name = "TYPE")]
    r#type: RagTypeFilter,
}

/// 执行 rag 子命令
pub fn execute(args: RagArgs, config: &Config, quiet: bool) -> Result<(), PdfError> {
    check_input_exists(&args.input)?;

    if !quiet {
        eprintln!(
            "正在解析: {} ...",
            args.input.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    let start = Instant::now();

    let mut doc = parse_pdf(&args.input, config)?;

    if !quiet {
        eprintln!(
            "解析完成: {} 页, 耗时 {:.2}s",
            doc.pages.len(),
            start.elapsed().as_secs_f64()
        );
    }

    // 页码过滤
    if let Some(ref range_str) = args.pages {
        let total = doc.pages.len();
        let indices = page_range::parse_page_range(range_str, total)
            .map_err(|e| PdfError::Parse(format!("页码范围错误: {}", e)))?;
        page_range::filter_pages(&mut doc, &indices);
        if !quiet {
            eprintln!("已过滤: 保留 {} 页", doc.pages.len());
        }
    }

    // 根据类型过滤导出
    let lines = match args.r#type {
        RagTypeFilter::All => RagExporter::export_all(&doc),
        RagTypeFilter::Blocks => RagExporter::export_block_lines(&doc),
        RagTypeFilter::TableRows => RagExporter::export_table_row_lines(&doc),
        RagTypeFilter::TableCells => RagExporter::export_table_cell_lines(&doc),
    };

    // 格式化输出
    let output_text = match args.format {
        RagFormat::Text => lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        RagFormat::Jsonl => lines
            .iter()
            .map(|l| {
                format!(
                    "{{\"page\":{},\"type\":\"{:?}\",\"text\":{}}}",
                    l.page_index + 1,
                    l.line_type,
                    serde_json::to_string(&l.text).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        RagFormat::Csv => {
            let mut csv_output = String::from("page,type,text\n");
            for l in &lines {
                csv_output.push_str(&format!(
                    "{},{:?},{}\n",
                    l.page_index + 1,
                    l.line_type,
                    csv_escape(&l.text)
                ));
            }
            csv_output
        }
    };

    write_output(output_text.as_bytes(), args.output.as_deref())?;

    if !quiet {
        eprintln!("导出完成: {} 行", lines.len());
        if let Some(ref out_path) = args.output {
            eprintln!("已写入: {}", out_path.display());
        }
    }

    Ok(())
}

/// CSV 转义（双引号包裹，内部双引号转义）
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
