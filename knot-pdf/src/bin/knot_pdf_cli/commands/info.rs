//! `info` 子命令：显示 PDF 基础信息

use clap::Args;
use knot_pdf::{parse_pdf, Config, PdfError};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::utils::output::{check_input_exists, write_output};

/// 显示 PDF 基础信息（页数/元数据/大纲）
#[derive(Args)]
pub struct InfoArgs {
    /// 输入 PDF 文件路径
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// 以 JSON 格式输出
    #[arg(long)]
    json: bool,
}

/// PDF 信息摘要（用于 JSON 输出）
#[derive(Serialize)]
struct PdfInfo {
    file_name: String,
    file_size_bytes: u64,
    doc_id: String,
    total_pages: usize,
    metadata: MetadataInfo,
    outline_items: usize,
    page_summary: Vec<PageSummary>,
}

#[derive(Serialize)]
struct MetadataInfo {
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    creator: Option<String>,
    producer: Option<String>,
    creation_date: Option<String>,
}

#[derive(Serialize)]
struct PageSummary {
    page: usize,
    blocks: usize,
    tables: usize,
    images: usize,
    text_score: f32,
    is_scanned: bool,
}

/// 执行 info 子命令
pub fn execute(args: InfoArgs, config: &Config, quiet: bool) -> Result<(), PdfError> {
    check_input_exists(&args.input)?;

    if !quiet && !args.json {
        eprintln!(
            "正在解析: {} ...",
            args.input.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    let start = Instant::now();

    // 获取文件大小
    let file_size = std::fs::metadata(&args.input)?.len();

    // 解析 PDF
    let doc = parse_pdf(&args.input, config)?;

    let elapsed = start.elapsed();

    // 构建信息摘要
    let info = PdfInfo {
        file_name: args
            .input
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        file_size_bytes: file_size,
        doc_id: doc.doc_id.clone(),
        total_pages: doc.pages.len(),
        metadata: MetadataInfo {
            title: doc.metadata.title.clone(),
            author: doc.metadata.author.clone(),
            subject: doc.metadata.subject.clone(),
            creator: doc.metadata.creator.clone(),
            producer: doc.metadata.producer.clone(),
            creation_date: doc.metadata.creation_date.clone(),
        },
        outline_items: doc.outline.len(),
        page_summary: doc
            .pages
            .iter()
            .map(|p| PageSummary {
                page: p.page_index + 1,
                blocks: p.blocks.len(),
                tables: p.tables.len(),
                images: p.images.len(),
                text_score: p.text_score,
                is_scanned: p.is_scanned_guess,
            })
            .collect(),
    };

    if args.json {
        let json = serde_json::to_string_pretty(&info)
            .map_err(|e| PdfError::Parse(format!("JSON 序列化失败: {}", e)))?;
        write_output(json.as_bytes(), None)?;
    } else {
        let text = format_info_text(&info, elapsed.as_secs_f64());
        write_output(text.as_bytes(), None)?;
    }

    Ok(())
}

/// 格式化为人类可读的文本
fn format_info_text(info: &PdfInfo, elapsed_secs: f64) -> String {
    let mut out = String::new();

    out.push_str(&format!("📄 {}\n", info.file_name));
    out.push_str(&format!(
        "   文件大小: {}\n",
        format_file_size(info.file_size_bytes)
    ));
    out.push_str(&format!("   文档 ID:  {}\n", &info.doc_id[..16]));
    out.push_str(&format!("   页数:     {}\n", info.total_pages));
    out.push_str(&format!("   解析耗时: {:.2}s\n", elapsed_secs));

    // 元数据
    out.push_str("\n📋 元数据\n");
    if let Some(ref title) = info.metadata.title {
        if !title.is_empty() {
            out.push_str(&format!("   标题:     {}\n", title));
        }
    }
    if let Some(ref author) = info.metadata.author {
        if !author.is_empty() {
            out.push_str(&format!("   作者:     {}\n", author));
        }
    }
    if let Some(ref subject) = info.metadata.subject {
        if !subject.is_empty() {
            out.push_str(&format!("   主题:     {}\n", subject));
        }
    }
    if let Some(ref date) = info.metadata.creation_date {
        if !date.is_empty() {
            out.push_str(&format!("   创建日期: {}\n", date));
        }
    }

    // 大纲
    if info.outline_items > 0 {
        out.push_str(&format!("\n📑 大纲: {} 项\n", info.outline_items));
    }

    // 页面概览
    out.push_str("\n📊 页面概览\n");
    out.push_str("   页码  文本块  表格  图片  文本质量  扫描页\n");
    out.push_str("   ────  ─────  ────  ────  ────────  ──────\n");

    // 如果页数太多只显示前 10 页 + 最后 5 页
    let show_all = info.page_summary.len() <= 20;
    let pages = &info.page_summary;

    if show_all {
        for p in pages {
            out.push_str(&format_page_line(p));
        }
    } else {
        for p in pages.iter().take(10) {
            out.push_str(&format_page_line(p));
        }
        out.push_str(&format!("   ... 省略 {} 页 ...\n", pages.len() - 15));
        for p in pages.iter().skip(pages.len() - 5) {
            out.push_str(&format_page_line(p));
        }
    }

    // 汇总
    let total_blocks: usize = pages.iter().map(|p| p.blocks).sum();
    let total_tables: usize = pages.iter().map(|p| p.tables).sum();
    let total_images: usize = pages.iter().map(|p| p.images).sum();
    let scanned: usize = pages.iter().filter(|p| p.is_scanned).count();

    out.push_str(&format!(
        "\n   合计: {} 个文本块, {} 个表格, {} 个图片, {} 页疑似扫描\n",
        total_blocks, total_tables, total_images, scanned
    ));

    out
}

fn format_page_line(p: &PageSummary) -> String {
    format!(
        "   {:>4}  {:>5}  {:>4}  {:>4}  {:>7.1}%  {}\n",
        p.page,
        p.blocks,
        p.tables,
        p.images,
        p.text_score * 100.0,
        if p.is_scanned { "是" } else { "" }
    )
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
