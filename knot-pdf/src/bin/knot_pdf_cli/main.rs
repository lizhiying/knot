//! knot-pdf-cli: Rust 原生离线 PDF 解析器命令行工具

use clap::{Parser, Subcommand};
use std::process;

mod commands;
mod utils;

/// knot-pdf CLI — Rust 原生离线 PDF 解析器
///
/// 无需外部服务，将 PDF 转换为结构化 IR / Markdown / RAG 文本。
#[derive(Parser)]
#[command(name = "knot-pdf-cli")]
#[command(version, about, long_about = None)]
struct Cli {
    /// 指定配置文件路径（默认自动搜索 knot-pdf.toml）
    #[arg(short, long, global = true, value_name = "FILE")]
    config: Option<String>,

    /// 输出详细日志（可叠加 -vv -vvv）
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// 静默模式（仅输出结果，不输出进度和日志）
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 解析 PDF 并输出 IR JSON
    Parse(commands::parse::ParseArgs),

    /// 解析 PDF 并导出 Markdown
    Markdown(commands::markdown::MarkdownArgs),

    /// 解析 PDF 并导出 RAG 扁平化文本
    Rag(commands::rag::RagArgs),

    /// 显示 PDF 基础信息（页数/元数据/大纲）
    Info(commands::info::InfoArgs),

    /// 配置管理（查看/生成/搜索路径）
    Config(commands::config::ConfigArgs),
}

fn main() {
    let cli = Cli::parse();

    // 初始化日志
    init_logger(cli.verbose, cli.quiet);

    // 加载配置
    let config = load_config(cli.config.as_deref());

    // 分发子命令
    let result = match cli.command {
        Commands::Parse(args) => commands::parse::execute(args, &config, cli.quiet),
        Commands::Markdown(args) => commands::markdown::execute(args, &config, cli.quiet),
        Commands::Rag(args) => commands::rag::execute(args, &config, cli.quiet),
        Commands::Info(args) => commands::info::execute(args, &config, cli.quiet),
        Commands::Config(args) => commands::config::execute(args, &config, cli.quiet),
    };

    if let Err(e) = result {
        let exit_code = error_to_exit_code(&e);
        if !cli.quiet {
            eprintln!("错误: {}", e);
        }
        process::exit(exit_code);
    }
}

/// 初始化日志级别
fn init_logger(verbose: u8, quiet: bool) {
    let level = if quiet {
        log::LevelFilter::Off
    } else {
        match verbose {
            0 => log::LevelFilter::Warn,
            1 => log::LevelFilter::Info,
            2 => log::LevelFilter::Debug,
            _ => log::LevelFilter::Trace,
        }
    };

    env_logger::Builder::new()
        .filter_level(level)
        .format_timestamp_millis()
        .init();
}

/// 加载配置文件
fn load_config(config_path: Option<&str>) -> knot_pdf::Config {
    if let Some(path) = config_path {
        match knot_pdf::Config::from_toml_file(path) {
            Ok(config) => {
                log::info!("配置文件已加载: {}", path);
                config
            }
            Err(e) => {
                eprintln!("错误: 加载配置文件失败: {}", e);
                process::exit(1);
            }
        }
    } else {
        knot_pdf::Config::load_auto()
    }
}

/// 将 PdfError 映射为退出码
fn error_to_exit_code(err: &knot_pdf::PdfError) -> i32 {
    match err {
        knot_pdf::PdfError::Encrypted => 2,
        knot_pdf::PdfError::Corrupted(_) => 3,
        _ => 1,
    }
}
