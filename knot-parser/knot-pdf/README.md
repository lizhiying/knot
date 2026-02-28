# knot-pdf

> Rust 原生离线 PDF 解析器，专为 RAG（检索增强生成）应用设计。

无需外部服务，无需 LLM 调用——纯 Rust 实现文本抽取、表格识别、OCR 集成，将 PDF 转换为结构化 IR（中间表示），直接用于向量索引和语义检索。

## ✨ 特性

- **纯 Rust 实现** — 无 Python / Java 依赖，编译即用
- **结构化 IR 输出** — 文档 → 页面 → 文本块 / 表格 / 图片，层次清晰
- **表格抽取** — Ruled（有线框）/ Stream（无线框）/ Booktabs（三线表）三种模式自动检测
- **混合表格渲染** — 简单表格输出 Markdown，合并单元格表格自动切换 HTML `<table>`
- **多种导出格式** — Markdown、RAG 扁平化文本、IR JSON、CSV
- **OCR 集成**（可选）— PaddleOCR / Tesseract 后端，扫描件也能解析
- **Vision LLM 集成**（可选）— VLM 自动识别复杂排版页面，增强 booktabs 表格提取
- **乱码自动修复** — 检测字体编码错误，自动切换 VLM/OCR 重新提取
- **多级回退链** — pdfium 文本 → VLM 视觉理解 → PaddleOCR → 空页面
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
│  │ pdfium   │→ │ 文本抽取  │→ │ 布局   │ │
│  │ + lopdf  │  │ BlockIR  │  │ 分析   │ │
│  └──────────┘  └──────────┘  └────────┘ │
│                     │                    │
│              ┌──────▼──────┐             │
│              │  表格检测    │             │
│              │ Stream/Ruled│             │
│              │ /Booktabs   │             │
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

---

## 🔌 集成到你的 Rust 应用

### 第 1 步：添加依赖

```toml
[dependencies]
knot-pdf = { path = "path/to/knot-pdf", features = [
    "pdfium",         # PDFium 高质量文本抽取 + 页面渲染
    "ocr_paddle",     # PaddleOCR PP-OCRv5 扫描件识别
    "vision",         # Vision LLM 表格增强 + 图表理解
    "formula_model",  # 公式 OCR（LaTeX 识别）
    "layout_model",   # ONNX 版面检测（改善分栏识别）
] }
```

> **注意**：knot-pdf 默认启用 OCR。如果没有对应模型文件，运行时会输出下载指引并自动降级（不影响基本文本抽取）。

### 第 1.5 步：下载模型文件（推荐）

knot-pdf 默认开启 OCR 和 Vision LLM 增强。首次使用前需下载对应模型：

#### OCR 模型 — PaddleOCR PP-OCRv5（~165MB）

```bash
# 在项目根目录（或可执行文件同级目录）下创建模型目录
mkdir -p models/ppocrv5 && cd models/ppocrv5

# 从 HuggingFace 下载三个文件
wget https://huggingface.co/OpenPPOCR/PP-OCRv5/resolve/main/det.onnx
wget https://huggingface.co/OpenPPOCR/PP-OCRv5/resolve/main/rec.onnx
wget https://huggingface.co/OpenPPOCR/PP-OCRv5/resolve/main/ppocrv5_dict.txt
```

模型文件目录结构：

```
models/ppocrv5/
├── det.onnx           # 文字检测模型 (84MB)
├── rec.onnx           # 文字识别模型 (88MB)
└── ppocrv5_dict.txt   # 字典文件 (72KB)
```

自动探测路径（按优先级）：
1. `ocr_model_dir` 配置项指定的路径
2. 可执行文件同级目录下的 `models/ppocrv5/`
3. 当前工作目录下的 `models/ppocrv5/`

#### Vision LLM — Ollama 本地部署（推荐）

Vision LLM 用于增强 booktabs 表格提取和复杂排版页面理解：

```bash
# 安装 Ollama (macOS)
brew install ollama

# 启动服务
ollama serve

# 下载 GLM-OCR 模型（推荐，专为文档 OCR 优化）
ollama pull glm-ocr:latest

# 或使用其他视觉模型
ollama pull llava:7b
ollama pull minicpm-v:latest
```

默认配置已指向 `http://localhost:11434/v1/chat/completions`，Ollama 启动后即可自动使用。

#### 不需要模型？

如果不需要 OCR 和 VLM，在配置中关闭即可：

```toml
# knot-pdf.toml
ocr_enabled = false
vision_api_url = ""  # 留空禁用 VLM
```

### 第 2 步：选择 API

knot-pdf 提供 4 种 API，适用于不同场景：

| API                  | 适用场景             | 返回值                                         |
| -------------------- | -------------------- | ---------------------------------------------- |
| `parse_pdf()`        | 一次性解析整个 PDF   | `Result<DocumentIR, PdfError>`                 |
| `parse_pdf_pages()`  | 逐页迭代，节省内存   | `Result<impl Iterator<Item = Result<PageIR>>>` |
| `parse_pdf_async()`  | 异步获取完整文档     | `Future<Result<DocumentIR>>`                   |
| `parse_pdf_stream()` | 异步流式推送，生产级 | `AsyncParseHandle`（含 channel receiver）      |

#### API 1: 同步——获取完整文档（最常用）

```rust
use knot_pdf::{parse_pdf, Config, MarkdownRenderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建配置（使用默认值，或从 TOML 加载）
    let config = Config::default();

    // 2. 解析 PDF → DocumentIR
    let doc = parse_pdf("report.pdf", &config)?;

    // 3. 访问结构化数据
    println!("文档 ID: {}", doc.doc_id);
    println!("页数: {}", doc.pages.len());

    for page in &doc.pages {
        println!("  第 {} 页: {} 个文本块, {} 个表格, {} 个图片",
            page.page_index + 1,
            page.blocks.len(),
            page.tables.len(),
            page.images.len(),
        );

        // 遍历文本块
        for block in &page.blocks {
            println!("    [{}] {}", format!("{:?}", block.role), &block.normalized_text[..80.min(block.normalized_text.len())]);
        }
    }

    // 4. 导出为 Markdown
    let renderer = MarkdownRenderer::new();
    let markdown = renderer.render_document(&doc);
    std::fs::write("output.md", &markdown)?;

    Ok(())
}
```

#### API 2: 同步——逐页迭代器（大文件友好）

```rust
use knot_pdf::{parse_pdf_pages, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();

    // 逐页处理，每次只在内存中保留一个 PageIR
    for result in parse_pdf_pages("large-500pages.pdf", &config)? {
        match result {
            Ok(page) => {
                println!("第 {} 页: {} 个块", page.page_index + 1, page.blocks.len());
                // ... 处理完后 page 自动释放
            }
            Err(e) => eprintln!("页面解析失败: {}", e),
        }
    }

    Ok(())
}
```

#### API 3: 异步——获取完整文档

需要 `async` feature：

```toml
knot-pdf = { path = "...", features = ["async"] }
```

```rust
use knot_pdf::{parse_pdf_async, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = parse_pdf_async("report.pdf", Config::default()).await?;
    println!("解析完成: {} 页", doc.pages.len());
    Ok(())
}
```

#### API 4: 异步——流式推送（生产级）

```rust
use knot_pdf::{parse_pdf_stream, parse_pdf_with_handler, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方式 A: Channel 流式接收（有界 channel，自动 backpressure）
    let mut handle = parse_pdf_stream("large.pdf", Config::default(), 4).await?;
    while let Some(result) = handle.receiver.recv().await {
        match result {
            Ok(page) => println!("收到第 {} 页", page.page_index + 1),
            Err(e) => eprintln!("错误: {}", e),
        }
    }
    let stats = handle.handle.await??;
    println!("共处理 {} 页", stats.total_pages);

    // 方式 B: 回调 API（更简洁）
    let stats = parse_pdf_with_handler("report.pdf", Config::default(), |page| {
        println!("处理第 {} 页: {} 个块", page.page_index + 1, page.blocks.len());
    }).await?;

    Ok(())
}
```

### 第 3 步：使用 Pipeline 进行高级控制

如果需要更细粒度的控制（如注入自定义 OCR 后端），可以直接使用 `Pipeline`：

```rust
use knot_pdf::{Pipeline, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::default();
    config.ocr_enabled = true;
    config.page_timeout_secs = 30;
    config.page_indices = Some(vec![0, 1, 2]); // 只解析前 3 页

    let pipeline = Pipeline::new(config);
    let doc = pipeline.parse(std::path::Path::new("report.pdf"))?;

    println!("解析 {} 页", doc.pages.len());
    Ok(())
}
```

### 第 4 步：访问表格数据

表格通过 `TableIR` 结构提供结构化访问和多格式导出：

```rust
use knot_pdf::{parse_pdf, Config};

let doc = parse_pdf("financial.pdf", &Config::default())?;

for page in &doc.pages {
    for table in &page.tables {
        // 表格元信息
        println!("表格 {} ({:?}): {} 列 × {} 行",
            table.table_id,
            table.extraction_mode,
            table.headers.len(),
            table.rows.len(),
        );

        // 导出为 Markdown（简单表格）或 HTML（含合并单元格的表格）
        println!("{}", table.to_markdown_or_html());

        // 强制 Markdown 格式
        println!("{}", table.to_markdown());

        // 强制 HTML 格式（保留 rowspan/colspan）
        println!("{}", table.to_html());

        // 按行列访问单元格
        for row in &table.rows {
            for cell in &row.cells {
                print!("[{}] ", cell.text);
            }
            println!();
        }
    }
}
```

### 第 5 步：导出 RAG 文本

`RagExporter` 将文档扁平化为适合向量嵌入的文本行：

```rust
use knot_pdf::{parse_pdf, Config, RagExporter};

let doc = parse_pdf("report.pdf", &Config::default())?;
let lines = RagExporter::export_all(&doc);

for line in &lines {
    // 每行包含页码、位置信息和文本内容，直接用于向量嵌入
    println!("{}", line.text);
}
```

---

## ⚙️ 配置详解

### 代码创建配置

```rust
use knot_pdf::Config;

let mut config = Config::default();

// ── 文本抽取 ──
config.strip_headers_footers = true;   // 剔除页眉页脚（默认 true）
config.max_columns = 3;                // 多列检测上限（默认 3）

// ── 输出格式 ──
config.emit_markdown = true;           // 输出 Markdown（默认 true）
config.emit_ir_json = false;           // 输出 IR JSON（默认 false）

// ── OCR（需编译时启用 ocr_paddle/ocr_tesseract feature）──
config.ocr_enabled = true;             // 启用 OCR（默认 false）
config.ocr_mode = knot_pdf::config::OcrMode::Auto;  // Auto / ForceAll / Disabled
config.ocr_render_width = 1024;        // OCR 渲染宽度（默认 512）

// ── Vision LLM（需编译时启用 vision feature）──
config.vision_api_url = Some("http://localhost:11434/v1/chat/completions".into());
config.vision_model = "gpt-4o".into(); // 或 Ollama 本地模型

// ── 资源控制 ──
config.page_timeout_secs = 30;         // 单页超时 30 秒（0 = 不超时）
config.max_memory_mb = 200;            // 内存限制 200MB

// ── 页码过滤 ──
config.page_indices = Some(vec![0, 1, 4]); // 只解析第 1、2、5 页（0-indexed）

config.validate();  // 校验并修正异常值
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

# Vision LLM（需编译时启用 vision feature）
vision_api_url = "http://localhost:11434/v1/chat/completions"
vision_model = "gpt-4o"

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

// 导出当前配置到 TOML
config.save_toml_file("knot-pdf.toml").unwrap();
```

### 常用配置方案

```rust
use knot_pdf::Config;

// 方案 1: 最快速度（纯文本 PDF，不需要 OCR）
let fast = Config::default();

// 方案 2: 启用 OCR（扫描件混合文档）
let mut ocr_config = Config::default();
ocr_config.ocr_enabled = true;
ocr_config.ocr_render_width = 1024;

// 方案 3: 启用 Vision LLM（复杂排版 + 表格增强）
let mut vision_config = Config::default();
vision_config.vision_api_url = Some("http://localhost:11434/v1/chat/completions".into());
vision_config.vision_model = "glm-ocr:latest".into();

// 方案 4: 生产环境（超时保护 + 内存限制）
let mut prod_config = Config::default();
prod_config.page_timeout_secs = 30;
prod_config.max_memory_mb = 500;
prod_config.ocr_enabled = true;
```

---

## 🔧 Feature Flags

| Feature         | 说明                                    | 依赖                      |
| --------------- | --------------------------------------- | ------------------------- |
| *(默认)*        | 文本抽取 + 表格识别 + Markdown/RAG 导出 | `pdf-extract`, `lopdf`    |
| `async`         | 异步 API（`parse_pdf_async` 等）        | `tokio`                   |
| `cli`           | 命令行工具（`knot-pdf-cli`）            | `clap`, `env_logger`      |
| `pdfium`        | PDFium 高质量文本抽取 + 页面渲染        | `pdfium-render`, `image`  |
| `ocr_paddle`    | PaddleOCR (PP-OCRv5) 后端               | `pure-onnx-ocr`, `image`  |
| `ocr_tesseract` | Tesseract OCR 后端                      | `leptess`                 |
| `vision`        | Vision LLM 图片描述 + 表格增强          | `ureq`, `base64`, `image` |
| `layout_model`  | ONNX 版面检测模型                       | `tract-onnx`, `image`     |
| `formula_model` | 公式 OCR 识别模型                       | `ort`, `image`            |
| `store_sled`    | 断点续传（页面级缓存）                  | `sled`                    |

可组合使用：

```bash
# 仅基本功能（最小依赖）
cargo build

# 推荐组合：高质量文本 + OCR + VLM
cargo build --features pdfium,ocr_paddle,vision

# 异步 + OCR + 断点续传
cargo build --features async,ocr_paddle,pdfium,store_sled

# 编译 CLI（包含全部功能）
cargo build --release --features "cli,pdfium,ocr_paddle,vision"
```

---

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
│       │       │   └── TableRow.cells: Vec<TableCell>
│       │       │       └── TableCell { text, row, col, rowspan, colspan }
│       │       ├── extraction_mode: Ruled | Stream | Unknown
│       │       ├── fallback_text: String
│       │       └── to_markdown() / to_html() / to_markdown_or_html()
│       ├── images: Vec<ImageIR>    # 图片引用
│       └── timings: Timings        # 耗时统计
└── diagnostics: Diagnostics        # 全局诊断/警告
```

### 公开导出的类型

```rust
// lib.rs 导出
pub use config::Config;
pub use error::PdfError;
pub use ir::DocumentIR;
pub use pipeline::Pipeline;
pub use render::{MarkdownRenderer, RagExporter};

// 异步 API（需 async feature）
#[cfg(feature = "async")]
pub use pipeline::async_pipeline::{
    parse_pdf_async, parse_pdf_stream, parse_pdf_with_handler,
    AsyncParseHandle, AsyncParseStats,
};

// 顶层便捷函数
pub fn parse_pdf(path, config) -> Result<DocumentIR, PdfError>;
pub fn parse_pdf_pages(path, config) -> Result<impl Iterator<Item = Result<PageIR>>, PdfError>;
```

---

## 🖥 CLI 命令行工具

编译后即可在终端直接使用，无需编写代码：

```bash
cargo install --path . --features "cli,pdfium,ocr_paddle,vision"
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

# 导出 Markdown
knot-pdf-cli markdown report.pdf -o report.md

# 导出 RAG 文本（JSONL 格式）
knot-pdf-cli rag report.pdf --format jsonl -o rag.jsonl

# 查看 PDF 信息
knot-pdf-cli info report.pdf --json

# 使用指定配置文件
knot-pdf-cli -c custom.toml parse report.pdf

# 静默模式（仅输出结果）
knot-pdf-cli -q markdown report.pdf -o output.md

# 详细日志（调试用）
knot-pdf-cli -vvv parse report.pdf
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

---

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

对于多页 PDF，单页失败不影响其他页面——Pipeline 会跳过失败页并记录诊断信息。

### CLI 退出码

| 退出码 | 含义                     |
| ------ | ------------------------ |
| 0      | 成功                     |
| 1      | 一般错误（文件不存在等） |
| 2      | PDF 文件已加密           |
| 3      | PDF 文件已损坏           |

---

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

---

## 🗂 项目结构

```
knot-pdf/
├── src/
│   ├── lib.rs              # 公开 API 入口（parse_pdf, parse_pdf_pages, ...）
│   ├── config.rs           # Config 结构体 + TOML 加载
│   ├── error.rs            # PdfError 错误类型
│   ├── bin/knot_pdf_cli/   # CLI 命令行工具（feature = "cli"）
│   │   ├── main.rs         # CLI 入口 + clap 参数解析
│   │   ├── commands/       # 子命令（parse/markdown/rag/info/config）
│   │   └── utils/          # 工具（页码范围解析/输出写入）
│   ├── pipeline/           # 解析 Pipeline（同步 + 异步）
│   ├── backend/            # PDF 底层文本/图形抽取
│   ├── ir/                 # IR 数据结构（Document/Page/Block/Table/Image）
│   ├── table/              # 表格检测与抽取（Stream + Ruled + Booktabs）
│   ├── layout/             # 布局分析（多列/阅读顺序/隐式网格）
│   ├── scoring/            # 页面文本质量评分
│   ├── render/             # 渲染导出（Markdown / RAG）
│   ├── ocr/                # OCR 后端（PaddleOCR / Tesseract）
│   ├── vision/             # Vision LLM 集成（OpenAI 兼容 API）
│   ├── postprocess/        # 后处理（水印/脚注/段落合并/URL 检测）
│   ├── store/              # 断点续传（sled）
│   ├── hf_detect/          # 页眉页脚检测
│   └── mem_track.rs        # 内存监控（macOS/Linux RSS）
├── tests/
│   ├── fixtures/           # 测试 PDF 和评测样本集
│   └── ...                 # 各模块测试
├── knot-pdf.example.toml   # 配置文件示例
└── Cargo.toml
```

## 许可证

MIT OR Apache-2.0
