//! `markdown` 子命令：解析 PDF 并导出 Markdown

use clap::Args;
use knot_pdf::{parse_pdf, Config, MarkdownRenderer, PdfError};
use std::path::PathBuf;
use std::time::Instant;

use crate::utils::output::{check_input_exists, write_output};
use crate::utils::page_range;

/// 解析 PDF 并导出 Markdown
#[derive(Args)]
pub struct MarkdownArgs {
    /// 输入 PDF 文件路径
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// 输出文件路径（默认输出到 stdout）
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// 页码范围，如 "1-5" 或 "1,3,5"（默认全部）
    #[arg(long, value_name = "RANGE")]
    pages: Option<String>,

    /// 不包含表格
    #[arg(long)]
    no_tables: bool,

    /// 不包含图片引用
    #[arg(long)]
    no_images: bool,

    /// 不包含页码标记
    #[arg(long)]
    no_page_markers: bool,
}

/// 执行 markdown 子命令
pub fn execute(args: MarkdownArgs, config: &Config, quiet: bool) -> Result<(), PdfError> {
    check_input_exists(&args.input)?;

    if !quiet {
        eprintln!(
            "正在解析: {} ...",
            args.input.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    let start = Instant::now();

    // 解析 PDF（如果指定了页码范围，提前设置到 config 中避免解析无用页面）
    let mut config = config.clone();
    let page_range_str = args.pages.clone();

    // 先快速打开 PDF 获取总页数，解析页码范围后设置到 config
    if let Some(ref range_str) = page_range_str {
        // 用 lopdf 快速获取页数（不做全量解析）
        let total_pages = {
            let doc = lopdf::Document::load(&args.input)
                .map_err(|e| PdfError::Parse(format!("无法读取 PDF: {}", e)))?;
            doc.get_pages().len()
        };
        let indices = page_range::parse_page_range(range_str, total_pages)
            .map_err(|e| PdfError::Parse(format!("页码范围错误: {}", e)))?;
        if !quiet {
            eprintln!("已选择 {} 页 (共 {} 页)", indices.len(), total_pages);
        }
        config.page_indices = Some(indices);
    }

    let doc = parse_pdf(&args.input, &config)?;

    let parse_elapsed = start.elapsed();

    if !quiet {
        eprintln!(
            "解析完成: {} 页, 耗时 {:.2}s",
            doc.pages.len(),
            parse_elapsed.as_secs_f64()
        );
    }

    // 配置 Markdown 渲染器
    let renderer = MarkdownRenderer {
        include_page_markers: !args.no_page_markers,
        include_tables: !args.no_tables,
        include_images: !args.no_images,
    };

    // 渲染 Markdown
    let markdown = renderer.render_document(&doc);

    let total_elapsed = start.elapsed();

    if !quiet {
        eprintln!(
            "Markdown 渲染完成: {} 字节, 总耗时 {:.2}s",
            markdown.len(),
            total_elapsed.as_secs_f64()
        );
    }

    // 输出
    write_output(markdown.as_bytes(), args.output.as_deref())?;

    if let Some(ref out_path) = args.output {
        if !quiet {
            eprintln!("已写入: {}", out_path.display());
        }
    }

    Ok(())
}
