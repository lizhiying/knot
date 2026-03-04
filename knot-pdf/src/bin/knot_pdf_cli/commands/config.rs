//! `config` 子命令：配置管理

use clap::{Args, Subcommand};
use knot_pdf::{Config, PdfError};

use crate::utils::output::write_output;

/// 配置管理（查看/生成/搜索路径）
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// 显示当前生效的配置（TOML 格式）
    Show,

    /// 在当前目录生成 knot-pdf.toml 示例文件
    Init,

    /// 显示配置文件搜索路径
    Path,
}

/// 执行 config 子命令
pub fn execute(args: ConfigArgs, config: &Config, _quiet: bool) -> Result<(), PdfError> {
    match args.action {
        ConfigAction::Show => execute_show(config),
        ConfigAction::Init => execute_init(),
        ConfigAction::Path => execute_path(),
    }
}

/// 显示当前生效的配置
fn execute_show(config: &Config) -> Result<(), PdfError> {
    let toml = config
        .to_toml_string()
        .map_err(|e| PdfError::Parse(format!("配置序列化失败: {}", e)))?;

    write_output(toml.as_bytes(), None)?;
    Ok(())
}

/// 生成配置文件
fn execute_init() -> Result<(), PdfError> {
    let target = std::path::PathBuf::from("knot-pdf.toml");

    if target.exists() {
        eprintln!("⚠️  knot-pdf.toml 已存在，跳过生成。");
        eprintln!("   如需覆盖，请先删除现有文件。");
        return Ok(());
    }

    // 使用默认配置生成
    let config = Config::default();
    let toml = config
        .to_toml_string()
        .map_err(|e| PdfError::Parse(format!("配置序列化失败: {}", e)))?;

    // 添加注释头
    let content = format!(
        "# knot-pdf 配置文件\n\
         # 由 knot-pdf-cli config init 生成\n\
         #\n\
         # 配置文件搜索路径（优先级从高到低）：\n\
         # 1. 当前工作目录 ./knot-pdf.toml\n\
         # 2. 可执行文件同级目录\n\
         # 3. ~/.config/knot-pdf/knot-pdf.toml\n\
         \n{}",
        toml
    );

    std::fs::write(&target, &content)?;
    eprintln!("✅ 已生成: {}", target.display());
    eprintln!("   可根据需要编辑配置项。");

    Ok(())
}

/// 显示配置文件搜索路径
fn execute_path() -> Result<(), PdfError> {
    let candidates = [
        // 当前目录
        ("当前目录", Some(std::path::PathBuf::from("knot-pdf.toml"))),
        // 可执行文件同级
        (
            "可执行文件目录",
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("knot-pdf.toml"))),
        ),
        // ~/.config/knot-pdf/
        (
            "用户配置目录",
            std::env::var("HOME").ok().map(|h| {
                std::path::PathBuf::from(h)
                    .join(".config")
                    .join("knot-pdf")
                    .join("knot-pdf.toml")
            }),
        ),
    ];

    println!("配置文件搜索路径（优先级从高到低）：\n");

    for (i, (label, path)) in candidates.iter().enumerate() {
        match path {
            Some(p) => {
                let exists = p.exists();
                let marker = if exists { "✅" } else { "  " };
                let status = if exists { "(已找到)" } else { "(未找到)" };
                println!(
                    "  {}. {} {} {} {}",
                    i + 1,
                    marker,
                    label,
                    status,
                    p.display()
                );
            }
            None => {
                println!("  {}.    {} (路径不可用)", i + 1, label);
            }
        }
    }

    println!();

    Ok(())
}
