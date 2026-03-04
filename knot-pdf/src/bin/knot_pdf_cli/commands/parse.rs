//! `parse` 子命令：解析 PDF 并输出 IR JSON

use clap::Args;
use knot_pdf::{parse_pdf, Config, PdfError};
use std::path::PathBuf;
use std::time::Instant;

use crate::utils::output::{check_input_exists, write_output};
use crate::utils::page_range;

/// 解析 PDF 并输出 IR JSON
#[derive(Args)]
pub struct ParseArgs {
    /// 输入 PDF 文件路径
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// 输出文件路径（默认输出到 stdout）
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// 页码范围，如 "1-5" 或 "1,3,5"（默认全部）
    #[arg(long, value_name = "RANGE")]
    pages: Option<String>,

    /// 美化 JSON 输出（默认紧凑格式）
    #[arg(long)]
    pretty: bool,

    /// 输出中包含耗时统计
    #[arg(long)]
    include_timings: bool,

    /// 输出中包含诊断信息
    #[arg(long)]
    include_diagnostics: bool,
}

/// 执行 parse 子命令
pub fn execute(args: ParseArgs, config: &Config, quiet: bool) -> Result<(), PdfError> {
    check_input_exists(&args.input)?;

    if !quiet {
        eprintln!(
            "正在解析: {} ...",
            args.input.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    let start = Instant::now();

    // 解析 PDF
    let mut doc = parse_pdf(&args.input, config)?;

    let elapsed = start.elapsed();

    if !quiet {
        eprintln!(
            "解析完成: {} 页, 耗时 {:.2}s",
            doc.pages.len(),
            elapsed.as_secs_f64()
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

    // 序列化为 JSON
    let json = if args.pretty {
        serde_json::to_string_pretty(&doc)
    } else {
        serde_json::to_string(&doc)
    }
    .map_err(|e| PdfError::Parse(format!("JSON 序列化失败: {}", e)))?;

    // 输出
    write_output(json.as_bytes(), args.output.as_deref())?;

    if let Some(ref out_path) = args.output {
        if !quiet {
            eprintln!("已写入: {}", out_path.display());
        }
    }

    Ok(())
}
