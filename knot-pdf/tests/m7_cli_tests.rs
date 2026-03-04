//! CLI 端到端集成测试
//!
//! 运行命令：`cargo test --features cli --test m7_cli_tests`

use std::path::PathBuf;
use std::process::Command;

/// 获取 CLI 二进制路径
fn cli_bin() -> PathBuf {
    // cargo test 会把二进制编译到 target/debug/
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("knot-pdf-cli");
    path
}

/// 获取测试 PDF 路径
fn test_pdf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("bench_100pages.pdf")
}

/// 构建 CLI 二进制（确保编译）
fn ensure_built() {
    let status = Command::new("cargo")
        .args(["build", "--features", "cli", "--bin", "knot-pdf-cli"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build CLI");
    assert!(status.success(), "CLI build failed");
}

// ============================================================
// --help 测试
// ============================================================

#[test]
fn test_help_output() {
    ensure_built();
    let output = Command::new(cli_bin())
        .arg("--help")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("parse"));
    assert!(stdout.contains("markdown"));
    assert!(stdout.contains("rag"));
    assert!(stdout.contains("info"));
    assert!(stdout.contains("config"));
}

#[test]
fn test_version_output() {
    ensure_built();
    let output = Command::new(cli_bin())
        .arg("--version")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("knot-pdf-cli"));
}

// ============================================================
// parse 子命令测试
// ============================================================

#[test]
fn test_parse_help() {
    ensure_built();
    let output = Command::new(cli_bin())
        .args(["parse", "--help"])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("IR JSON"));
    assert!(stdout.contains("--pretty"));
    assert!(stdout.contains("--pages"));
    assert!(stdout.contains("--output"));
}

#[test]
fn test_parse_nonexistent_file() {
    ensure_built();
    let output = Command::new(cli_bin())
        .args(["parse", "nonexistent_file.pdf"])
        .output()
        .expect("Failed to run CLI");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("文件不存在"));
}

#[test]
fn test_parse_quiet_nonexistent() {
    ensure_built();
    let output = Command::new(cli_bin())
        .args(["-q", "parse", "nonexistent_file.pdf"])
        .output()
        .expect("Failed to run CLI");

    assert!(!output.status.success());
    // quiet 模式下不输出错误信息
    assert!(output.stderr.is_empty());
}

#[test]
fn test_parse_to_file() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        eprintln!("跳过: 测试 PDF 不存在");
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let output_file = tmpdir.path().join("output.json");

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "parse",
            pdf.to_str().unwrap(),
            "-o",
            output_file.to_str().unwrap(),
            "--pages",
            "1",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(
        output.status.success(),
        "parse 命令失败: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 验证输出文件存在且可反序列化
    let content = std::fs::read_to_string(&output_file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&content).expect("JSON 解析失败");
    assert!(doc["pages"].is_array());
    assert_eq!(doc["pages"].as_array().unwrap().len(), 1); // 只有 1 页
}

#[test]
fn test_parse_pretty() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let output_file = tmpdir.path().join("pretty.json");

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "parse",
            pdf.to_str().unwrap(),
            "-o",
            output_file.to_str().unwrap(),
            "--pretty",
            "--pages",
            "1",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let content = std::fs::read_to_string(&output_file).unwrap();
    // pretty 输出应该包含缩进
    assert!(content.contains("  "));
    assert!(content.contains("\"doc_id\""));
}

// ============================================================
// markdown 子命令测试
// ============================================================

#[test]
fn test_markdown_to_file() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let output_file = tmpdir.path().join("output.md");

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "markdown",
            pdf.to_str().unwrap(),
            "-o",
            output_file.to_str().unwrap(),
            "--pages",
            "1-2",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let content = std::fs::read_to_string(&output_file).unwrap();
    assert!(!content.is_empty(), "Markdown 输出不应为空");
    // 应包含页码标记
    assert!(content.contains("Page 1"), "应包含 Page 1 标记");
}

#[test]
fn test_markdown_no_tables() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let out_with = tmpdir.path().join("with_tables.md");
    let out_without = tmpdir.path().join("without_tables.md");

    // 有表格
    Command::new(cli_bin())
        .args([
            "-q",
            "markdown",
            pdf.to_str().unwrap(),
            "-o",
            out_with.to_str().unwrap(),
            "--pages",
            "1",
        ])
        .output()
        .expect("Failed to run CLI");

    // 无表格
    Command::new(cli_bin())
        .args([
            "-q",
            "markdown",
            pdf.to_str().unwrap(),
            "-o",
            out_without.to_str().unwrap(),
            "--pages",
            "1",
            "--no-tables",
        ])
        .output()
        .expect("Failed to run CLI");

    let with = std::fs::read_to_string(&out_with).unwrap();
    let without = std::fs::read_to_string(&out_without).unwrap();

    // 无表格的输出应该更短（或等长如果该页无表格）
    assert!(without.len() <= with.len());
}

// ============================================================
// rag 子命令测试
// ============================================================

#[test]
fn test_rag_text_output() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "rag",
            pdf.to_str().unwrap(),
            "--pages",
            "1",
            "--format",
            "text",
            "--type",
            "blocks",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "RAG 文本输出不应为空");
    // 应包含页码信息
    assert!(stdout.contains("页=1"));
}

#[test]
fn test_rag_jsonl_output() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "rag",
            pdf.to_str().unwrap(),
            "--pages",
            "1",
            "--format",
            "jsonl",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 每行应是有效 JSON
    for line in stdout.lines().filter(|l| !l.is_empty()) {
        let parsed: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("JSONL 行解析失败: {} — 行内容: {}", e, line));
        assert!(parsed["page"].is_number());
        assert!(parsed["text"].is_string());
    }
}

#[test]
fn test_rag_csv_output() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "rag",
            pdf.to_str().unwrap(),
            "--pages",
            "1",
            "--format",
            "csv",
            "--type",
            "blocks",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // CSV 应有表头
    assert!(stdout.starts_with("page,type,text"));
}

// ============================================================
// info 子命令测试
// ============================================================

#[test]
fn test_info_text_output() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let output = Command::new(cli_bin())
        .args(["info", pdf.to_str().unwrap()])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bench_100pages.pdf"));
    assert!(stdout.contains("100")); // 页数
    assert!(stdout.contains("元数据"));
    assert!(stdout.contains("页面概览"));
}

#[test]
fn test_info_json_output() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let output = Command::new(cli_bin())
        .args(["-q", "info", pdf.to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("JSON 解析失败");
    assert_eq!(parsed["total_pages"].as_u64().unwrap(), 100);
    assert!(parsed["metadata"]["title"].is_string());
    assert!(parsed["page_summary"].is_array());
}

// ============================================================
// config 子命令测试
// ============================================================

#[test]
fn test_config_show() {
    ensure_built();
    let output = Command::new(cli_bin())
        .args(["config", "show"])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 应包含 TOML 配置键
    assert!(stdout.contains("scoring_text_threshold"));
    assert!(stdout.contains("strip_headers_footers"));
    assert!(stdout.contains("ocr_enabled"));
}

#[test]
fn test_config_init() {
    ensure_built();
    let tmpdir = tempfile::tempdir().unwrap();

    let output = Command::new(cli_bin())
        .args(["config", "init"])
        .current_dir(tmpdir.path())
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("已生成"));

    // 验证文件已创建
    let config_file = tmpdir.path().join("knot-pdf.toml");
    assert!(config_file.exists(), "knot-pdf.toml 应已创建");

    let content = std::fs::read_to_string(&config_file).unwrap();
    assert!(content.contains("knot-pdf 配置文件"));
    assert!(content.contains("scoring_text_threshold"));
}

#[test]
fn test_config_init_no_overwrite() {
    ensure_built();
    let tmpdir = tempfile::tempdir().unwrap();

    // 第一次 init
    Command::new(cli_bin())
        .args(["config", "init"])
        .current_dir(tmpdir.path())
        .output()
        .expect("Failed to run CLI");

    // 第二次 init 应该跳过
    let output = Command::new(cli_bin())
        .args(["config", "init"])
        .current_dir(tmpdir.path())
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("已存在"));
}

#[test]
fn test_config_path() {
    ensure_built();
    let output = Command::new(cli_bin())
        .args(["config", "path"])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("当前目录"));
    assert!(stdout.contains("可执行文件目录"));
    assert!(stdout.contains("用户配置目录"));
}

// ============================================================
// 退出码测试
// ============================================================

#[test]
fn test_exit_code_file_not_found() {
    ensure_built();
    let output = Command::new(cli_bin())
        .args(["parse", "not_exist.pdf"])
        .output()
        .expect("Failed to run CLI");

    assert_eq!(output.status.code(), Some(1));
}

// ============================================================
// --pages 过滤测试
// ============================================================

#[test]
fn test_pages_filter() {
    ensure_built();
    let pdf = test_pdf();
    if !pdf.exists() {
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let output_file = tmpdir.path().join("filtered.json");

    let output = Command::new(cli_bin())
        .args([
            "-q",
            "parse",
            pdf.to_str().unwrap(),
            "-o",
            output_file.to_str().unwrap(),
            "--pages",
            "2,5",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let content = std::fs::read_to_string(&output_file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&content).unwrap();
    let pages = doc["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2);
    // 页码应该是 1（0-indexed）和 4（0-indexed）
    assert_eq!(pages[0]["page_index"].as_u64().unwrap(), 1);
    assert_eq!(pages[1]["page_index"].as_u64().unwrap(), 4);
}
