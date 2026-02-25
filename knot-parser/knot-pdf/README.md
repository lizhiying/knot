# knot-pdf

> Rust 原生离线 PDF 解析器，专为 RAG（检索增强生成）应用设计。

无需外部服务，无需 LLM 调用——纯 Rust 实现文本抽取、表格识别、OCR 集成，将 PDF 转换为结构化 IR（中间表示），直接用于向量索引和语义检索。

## ✨ 特性

- **纯 Rust 实现** — 无 Python / Java 依赖，编译即用
- **结构化 IR 输出** — 文档 → 页面 → 文本块 / 表格 / 图片，层次清晰
- **表格抽取** — 支持有线框（Ruled）和无线框（Stream）两种表格，自动检测抽取模式
- **多种导出格式** — Markdown、RAG 扁平化文本、IR JSON、CSV
- **OCR 集成**（可选）— PaddleOCR / Tesseract 后端，扫描件也能解析
- **异步 API**（可选）— 基于 tokio 的 `async/await`，支持流式逐页推送
- **断点续传**（可选）— 基于 sled 的页面缓存，中断后跳过已完成页
- **内存友好** — 逐页处理 + 及时释放，100 页 PDF 峰值 ~200MB
- **TOML 配置** — 支持配置文件自动发现与加载

## 📐 架构概览

```
PDF 文件
  │
  ▼
┌─────────────────────────────────────────┐
│  Pipeline（逐页处理）                      │
│                                         │
│  ┌──────────┐  ┌──────────┐  ┌────────┐ │
│  │ pdf-extra│→ │ 文本抽取  │→ │ 布局   │ │
│  │ + lopdf  │  │ BlockIR  │  │ 分析   │ │
│  └──────────┘  └──────────┘  └────────┘ │
│                     │                    │
│              ┌──────▼──────┐             │
│              │  表格检测    │             │
│              │ Stream/Ruled│             │
│              │  → TableIR  │             │
│              └─────────────┘             │
│                     │                    │
│         ┌───────────▼────────────┐       │
│         │  页面评分（PageScore）   │       │
│         │  text_score < 阈值?    │       │
│         └───────────┬────────────┘       │
│                     │ 是（扫描页）         │
│              ┌──────▼──────┐             │
│              │  OCR（可选）  │             │
│              │ PaddleOCR   │             │
│              │ / Tesseract │             │
│              └─────────────┘             │
└───────────────────┬─────────────────────┘
                    ▼
             ┌─────────────┐
             │ DocumentIR  │
             │ (JSON 可序列化)│
             └──────┬──────┘
                    │
        ┌───────────┼───────────┐
        ▼           ▼           ▼
   Markdown    RAG 扁平文本   IR JSON
```

## 🚀 快速上手

### 添加依赖

```toml
[dependencies]
knot-pdf = { path = "path/to/knot-pdf" }
```

### 基本使用

```rust
use knot_pdf::{parse_pdf, Config};

fn main() {
    let config = Config::default();
    let doc = parse_pdf("example.pdf", &config).unwrap();

    println!("文档 ID: {}", doc.doc_id);
    println!("页数: {}", doc.pages.len());

    for page in &doc.pages {
        println!("  第 {} 页: {} 个文本块, {} 个表格",
            page.page_index + 1,
            page.blocks.len(),
            page.tables.len(),
        );
    }
}
```

### 导出 Markdown

```rust
use knot_pdf::{parse_pdf, Config, MarkdownRenderer};

let doc = parse_pdf("report.pdf", &Config::default()).unwrap();
let renderer = MarkdownRenderer::new();
let markdown = renderer.render_document(&doc);
std::fs::write("output.md", &markdown).unwrap();
```

### 导出 RAG 扁平化文本

```rust
use knot_pdf::{parse_pdf, Config, RagExporter};

let doc = parse_pdf("report.pdf", &Config::default()).unwrap();
let lines = RagExporter::export_all(&doc);

for line in &lines {
    // 每行包含页码、位置信息和文本内容，可直接用于向量嵌入
    println!("{}", line.text);
}
```

### 逐页迭代器 API

```rust
use knot_pdf::{parse_pdf_pages, Config};

let config = Config::default();
for result in parse_pdf_pages("large.pdf", &config).unwrap() {
    match result {
        Ok(page) => println!("第 {} 页: {} 个块", page.page_index + 1, page.blocks.len()),
        Err(e) => eprintln!("页面失败: {}", e),
    }
}
```

### 表格数据访问

```rust
use knot_pdf::{parse_pdf, Config};

let doc = parse_pdf("financial.pdf", &Config::default()).unwrap();

for page in &doc.pages {
    for table in &page.tables {
        // 表格元信息
        println!("表格 {} ({}模式): {} 列 × {} 行",
            table.table_id,
            format!("{:?}", table.extraction_mode),
            table.headers.len(),
            table.rows.len(),
        );

        // 导出为 Markdown 表格
        println!("{}", table.to_markdown());

        // 导出为 CSV
        // println!("{}", table.to_csv());

        // 导出为 KV 行（用于 RAG 检索）
        // for line in table.to_kv_lines() { ... }
    }
}
```

### 异步 API

需要启用 `async` feature：

```toml
[dependencies]
knot-pdf = { path = "path/to/knot-pdf", features = ["async"] }
```

```rust
use knot_pdf::{parse_pdf_async, parse_pdf_stream, Config};

// 方式 1: 异步获取完整文档
let doc = parse_pdf_async("example.pdf", Config::default()).await?;

// 方式 2: 流式逐页推送（有界 channel，自动 backpressure）
let mut handle = parse_pdf_stream("large.pdf", Config::default(), 4).await?;
while let Some(result) = handle.receiver.recv().await {
    match result {
        Ok(page) => { /* 处理页面 */ },
        Err(e) => { /* 处理错误 */ },
    }
}
let stats = handle.handle.await??;

// 方式 3: 回调 API（带 Semaphore 限流）
let stats = parse_pdf_with_handler("example.pdf", Config::default(), |page| {
    println!("处理第 {} 页", page.page_index + 1);
}).await?;
```

## ⚙️ 配置

### 代码配置

```rust
use knot_pdf::Config;

let mut config = Config::default();
config.strip_headers_footers = true;  // 剔除页眉页脚
config.max_columns = 3;               // 多列检测上限
config.emit_markdown = true;           // 输出 Markdown
config.page_timeout_secs = 30;         // 单页超时 30 秒
config.max_memory_mb = 200;            // 内存限制 200MB
config.validate();                     // 校验并修正异常值
```

### TOML 配置文件

创建 `knot-pdf.toml`（参考 [`knot-pdf.example.toml`](knot-pdf.example.toml)）：

```toml
# 文本抽取
scoring_text_threshold = 0.3
strip_headers_footers = true
max_columns = 3

# 输出控制
emit_markdown = true
emit_ir_json = false

# OCR 配置（需编译时启用 feature）
ocr_enabled = false
ocr_mode = "auto"          # auto / force_all / disabled
ocr_render_width = 1024

# 资源控制
max_memory_mb = 200
page_timeout_secs = 0      # 0 = 不超时
```

配置文件自动搜索路径（优先级从高到低）：

1. 当前工作目录 `./knot-pdf.toml`
2. 可执行文件同级目录
3. `~/.config/knot-pdf/knot-pdf.toml`

```rust
// 自动搜索并加载配置
let config = Config::load_auto();

// 或从指定文件加载
let config = Config::from_toml_file("my-config.toml").unwrap();
```

## 🔧 Feature Flags

| Feature         | 说明                                    | 依赖                     |
| --------------- | --------------------------------------- | ------------------------ |
| *(默认)*        | 文本抽取 + 表格识别 + Markdown/RAG 导出 | `pdf-extract`, `lopdf`   |
| `async`         | 异步 API（`parse_pdf_async` 等）        | `tokio`                  |
| `cli`           | 命令行工具（`knot-pdf-cli`）            | `clap`, `env_logger`     |
| `ocr_paddle`    | PaddleOCR (PP-OCRv5) 后端               | `pure-onnx-ocr`, `image` |
| `pdfium`        | PDFium 页面渲染（OCR 前置步骤）         | `pdfium-render`, `image` |
| `ocr_tesseract` | Tesseract OCR 后端                      | `leptess`                |
| `store_sled`    | 断点续传（页面级缓存）                  | `sled`                   |

可组合使用：

```bash
# 仅基本功能
cargo build

# 启用异步 + OCR
cargo build --features async,ocr_paddle,pdfium

# 启用全部
cargo build --features async,ocr_paddle,pdfium,store_sled

# 编译 CLI 命令行工具
cargo build --release --features cli
```

## 🖥 CLI 命令行工具

编译后即可在终端直接使用，无需编写代码：

```bash
cargo build --release --features cli
# 二进制文件在 target/release/knot-pdf-cli
```

### 子命令一览

| 命令       | 说明                           | 示例                                                 |
| ---------- | ------------------------------ | ---------------------------------------------------- |
| `parse`    | 解析 PDF → IR JSON             | `knot-pdf-cli parse report.pdf --pretty -o out.json` |
| `markdown` | 解析 PDF → Markdown            | `knot-pdf-cli markdown report.pdf -o out.md`         |
| `rag`      | 解析 PDF → RAG 扁平化文本      | `knot-pdf-cli rag report.pdf --format jsonl`         |
| `info`     | 显示 PDF 基础信息              | `knot-pdf-cli info report.pdf`                       |
| `config`   | 配置管理（查看/生成/搜索路径） | `knot-pdf-cli config show`                           |

### 使用示例

```bash
# 解析 PDF 为美化 JSON
knot-pdf-cli parse report.pdf --pretty -o report.json

# 仅解析第 1-5 页
knot-pdf-cli parse report.pdf --pages 1-5 -o partial.json

# 导出 Markdown（不含表格和图片引用）
knot-pdf-cli markdown report.pdf -o report.md --no-tables --no-images

# 导出 RAG 文本（JSONL 格式，仅文本块）
knot-pdf-cli rag report.pdf --format jsonl --type blocks -o rag.jsonl

# 查看 PDF 信息
knot-pdf-cli info report.pdf

# 查看 PDF 信息（JSON 格式）
knot-pdf-cli info report.pdf --json

# 使用指定配置文件
knot-pdf-cli -c custom.toml parse report.pdf

# 静默模式（仅输出结果）
knot-pdf-cli -q markdown report.pdf -o output.md

# 详细日志（调试用）
knot-pdf-cli -vvv parse report.pdf --include-timings

# 初始化配置文件
knot-pdf-cli config init

# 查看配置文件搜索路径
knot-pdf-cli config path
```

### 全局选项

| 选项            | 说明                                                   |
| --------------- | ------------------------------------------------------ |
| `-c, --config`  | 指定配置文件路径（默认自动搜索）                       |
| `-v, --verbose` | 输出详细日志（`-v` INFO / `-vv` DEBUG / `-vvv` TRACE） |
| `-q, --quiet`   | 静默模式（仅输出结果）                                 |
| `-h, --help`    | 显示帮助                                               |
| `-V, --version` | 显示版本                                               |

### 页码范围格式

`--pages` 支持灵活的页码范围指定：

| 格式     | 说明           | 示例                 |
| -------- | -------------- | -------------------- |
| 单页     | 指定单个页码   | `--pages 3`          |
| 范围     | 起始-结束      | `--pages 1-5`        |
| 列表     | 逗号分隔       | `--pages 1,3,5`      |
| 混合     | 范围和列表混合 | `--pages 1-3,5,8-10` |
| 开放末尾 | 某页到最后     | `--pages 5-`         |

### 退出码

| 退出码 | 含义                     |
| ------ | ------------------------ |
| 0      | 成功                     |
| 1      | 一般错误（文件不存在等） |
| 2      | PDF 文件已加密           |
| 3      | PDF 文件已损坏           |


## 📊 IR 数据结构

```
DocumentIR
├── doc_id: String              # 基于内容的 SHA256 哈希
├── metadata: DocumentMetadata  # 标题/作者/创建日期...
├── outline: Vec<OutlineItem>   # 文档大纲
├── pages: Vec<PageIR>          # 页面列表
│   └── PageIR
│       ├── page_index: usize
│       ├── size: PageSize
│       ├── text_score: f32         # 文本质量评分 (0.0~1.0)
│       ├── is_scanned_guess: bool  # 是否疑似扫描页
│       ├── blocks: Vec<BlockIR>    # 文本块
│       │   └── BlockIR
│       │       ├── normalized_text: String
│       │       ├── bbox: BBox
│       │       └── role: BlockRole  # Body/Header/Title/List...
│       ├── tables: Vec<TableIR>    # 表格
│       │   └── TableIR
│       │       ├── headers: Vec<String>
│       │       ├── rows: Vec<TableRow>
│       │       ├── extraction_mode: Ruled | Stream
│       │       ├── fallback_text: String
│       │       └── to_markdown() / to_csv() / to_kv_lines()
│       ├── images: Vec<ImageIR>    # 图片引用
│       └── timings: Timings        # 耗时统计
└── diagnostics: Diagnostics        # 全局诊断/警告
```

## 📈 性能指标

| 指标                    | 数据                  | 条件                    |
| ----------------------- | --------------------- | ----------------------- |
| 100 页 born-digital PDF | **18.3s** (5.5 页/秒) | release 模式, macOS     |
| 单页平均文本抽取        | ~156ms                | release 模式            |
| Markdown 渲染 100 页    | 11.7MB / 7ms          | —                       |
| 峰值内存 (100 页)       | ~200MB                | 逐页处理 + 及时释放     |
| 表格列映射正确率        | 100%                  | 40 个 stream+ruled 样本 |
| RAG 命中率              | 83.3% (25/30)         | 30 个问答对评测         |
| OCR 单页 (PaddleOCR)    | ~20.6s/页             | PP-OCRv5, 1024px        |

## 🗂 项目结构

```
knot-pdf/
├── src/
│   ├── lib.rs              # 公开 API 入口
│   ├── config.rs           # Config 结构体 + TOML 加载
│   ├── error.rs            # PdfError 错误类型
│   ├── bin/knot_pdf_cli/   # CLI 命令行工具（feature = "cli"）
│   │   ├── main.rs         # CLI 入口 + clap 参数解析
│   │   ├── commands/       # 子命令（parse/markdown/rag/info/config）
│   │   └── utils/          # 工具（页码范围解析/输出写入）
│   ├── pipeline/           # 解析 Pipeline（同步 + 异步）
│   ├── backend/            # PDF 底层文本/图形抽取
│   ├── ir/                 # IR 数据结构（Document/Page/Block/Table/Image）
│   ├── table/              # 表格检测与抽取（Stream + Ruled）
│   ├── layout/             # 布局分析（多列/阅读顺序）
│   ├── scoring/            # 页面文本质量评分
│   ├── render/             # 渲染导出（Markdown / RAG / OCR 渲染）
│   ├── ocr/                # OCR 后端（PaddleOCR / Tesseract）
│   ├── store/              # 断点续传（sled）
│   ├── hf_detect/          # 页眉页脚检测
│   └── mem_track.rs        # 内存监控（macOS/Linux RSS）
├── tests/
│   ├── fixtures/           # 测试 PDF 和评测样本集
│   ├── m7_cli_tests.rs     # CLI 端到端集成测试（20 个测试）
│   ├── m6_eval.rs          # 样本集评测（表格正确率 + RAG 命中率）
│   ├── m6_e2e_bench.rs     # 端到端性能基准
│   └── ...                 # 各模块单元测试
├── benches/                # criterion 性能基准
├── scripts/                # PDF 生成脚本
├── docs/milestones/        # 里程碑文档
├── knot-pdf.example.toml   # 配置文件示例
└── Cargo.toml
```

## 错误处理

knot-pdf 不会 panic。所有错误通过 `Result<T, PdfError>` 返回：

```rust
use knot_pdf::PdfError;

match parse_pdf("file.pdf", &config) {
    Ok(doc) => { /* 正常处理 */ },
    Err(PdfError::Io(e)) => eprintln!("文件读取失败: {}", e),
    Err(PdfError::Encrypted) => eprintln!("PDF 已加密"),
    Err(PdfError::Corrupted(msg)) => eprintln!("PDF 损坏: {}", msg),
    Err(PdfError::Timeout(msg)) => eprintln!("处理超时: {}", msg),
    Err(e) => eprintln!("其他错误: {}", e),
}
```

对于多页 PDF，单页失败不影响其他页面——Pipeline 会跳过失败页并记录诊断信息到 `page.diagnostics`。

## 许可证

MIT OR Apache-2.0
