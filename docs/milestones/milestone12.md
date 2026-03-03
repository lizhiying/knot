# Milestone 12: 集成 knot-pdf 优化 PDF 解析

## 1. 当前 PDF 解析方案分析

目前 `pageindex-rs` 中的 PDF 解析采用的是 **"LLM 视觉驱动的 OCR 方案"**（渲染成图片 → LLM 多模态理解 → Markdown 文本）。

### 1.1 核心处理流程

PDF 文件的解析流转周期如下：
1. **页面渲染**：使用 `pdfium-render` 将 PDF 逐页渲染为 JPEG 图片（配置宽度为 768px，这是适配 3B 级别多模态大模型的"甜点分辨率"）。
2. **LLM 图像理解 (OCR)**：将单页图片转换为 Base64，配合精心设计的 OCRFlux Prompt，调用本地 LLM（`llama-server`）的多模态接口，要求 LLM 将图片内容转换为带排版格式的 Markdown 文本。
3. **图文嵌套分析**：
   - 匹配 LLM 提取的图像占据区域坐标宏 `<Image>(x1,y1),(x2,y2)</Image>`。
   - 对每一个坐标区域，从原渲染图中裁剪子图。
   - 再次调用 LLM 专门对该局部子图进行「图表分析」理解。
   - 将分析文本附加至最终内容中。
4. **语义树构建**：使用 `pulldown-cmark` 将解析得到的扁平化 Markdown 解析为事件流，根据 Markdown 中的标题层级（H1, H2...）自动构建父子相关的组织树（`PageNode` 树）。
5. **树精简 (Tree Thinning)**：当生成的节点 Token 过小（如碎片内容）且满足特定条件时，自动将其与父/兄节点合并，避免产生无法提供完整语义的过小向量分片。

### 1.2 现有方案的优势

- **解析质量极高**：由于使用了全视觉大模型的理解能力，能近乎完美地理解复杂的非结构化多列排版、图文混排文档。
- **天然的图表理解**：相比于统计算法，可以直接获得图文/图表的语义解释，极大提升 RAG 查询中的信息丰富度。
- **输出格式标准**：将非结构化布局直接转换为有语义的 Markdown 树。

### 1.3 存在的痛点与劣势

1. **解析速度存在严重瓶颈**：由于强制完全依赖视觉大模型，每一页都需要至少进行一次 LLM 图像推理（通常耗时 5-30 秒不等）。一本 100 页的文档可能需要半小时以上才能建库。
2. **极高的系统依赖**：
   - 如果用户没有启动 LLM 服务，PDF 解析直接中断失败。
   - 强依赖预先安装/捆绑 PDFium 外部动态库。
3. **资源极度浪费**：对于绝大多数"原生的、包含可选纯文本的（非扫描件）PDF"，目前依然将其栅格化为图片再交给大模型去"重新认字"，不仅损失了原本 100% 准确的文本，还白白消耗了惊人的算力时间。
4. **串行处理导致等待过长**：当前代码为 `for` 循环同步等待每一页的 API 响应，没有充分利用多线程并发机制。

---

## 2. knot-pdf 能力概览

`knot-pdf` 是一个 **纯 Rust 实现的离线 PDF 解析器**，专为 RAG 应用设计。以下是其核心能力：

### 2.1 核心特性

| 能力               | 说明                                                          |
| ------------------ | ------------------------------------------------------------- |
| 纯 Rust 实现       | 无 Python/Java 依赖，编译即用                                 |
| 结构化 IR 输出     | `DocumentIR → PageIR → BlockIR / TableIR / ImageIR`，层次清晰 |
| 表格自动检测       | Ruled（有线框）/ Stream（无线框）/ Booktabs（三线表）三种模式 |
| 混合表格渲染       | 简单表格输出 Markdown，合并单元格自动切换 HTML `<table>`      |
| 多种导出格式       | Markdown、RAG 扁平化文本、IR JSON                             |
| OCR 集成（可选）   | PaddleOCR / Tesseract 后端，扫描件也能解析                    |
| Vision LLM（可选） | VLM 增强 booktabs 表格、复杂排版页面解析                      |
| 乱码自动修复       | 检测字体编码错误，自动切换 VLM/OCR 重新提取                   |
| 多级回退链         | pdfium 文本 → VLM 视觉 → PaddleOCR → 空页面                   |
| 异步 API           | 基于 tokio 的 `async/await`，支持流式逐页推送                 |
| 内存友好           | 逐页处理 + 及时释放，100 页 PDF 峰值 ~200MB                   |

### 2.2 性能基准

| 指标                    | 数据                  | 条件                    |
| ----------------------- | --------------------- | ----------------------- |
| 100 页 born-digital PDF | **18.3s** (5.5 页/秒) | release 模式, macOS     |
| 单页平均文本抽取        | ~156ms                | release 模式            |
| 表格列映射正确率        | 100%                  | 40 个 stream+ruled 样本 |
| RAG 命中率              | 83.3%                 | 30 个问答对评测         |

### 2.3 与 pageindex-rs 的关键差异

| 维度       | pageindex-rs (现有)     | knot-pdf                                     |
| ---------- | ----------------------- | -------------------------------------------- |
| 文本提取   | 渲染图片 → LLM 识别     | PDF 结构化抽取（lopdf / pdfium）             |
| 100 页耗时 | ~30 分钟                | ~18 秒                                       |
| LLM 依赖   | **必须**                | 可选（仅扫描件/乱码时回退）                  |
| 表格处理   | LLM 理解                | 规则检测 + 自动分类（Ruled/Stream/Booktabs） |
| 输出格式   | PageNode 树（Markdown） | DocumentIR（结构化 IR）                      |
| 扫描件支持 | 全部走 LLM              | PaddleOCR + VLM 回退链                       |
| 状态管理   | 无                      | 断点续传（sled 缓存）                        |

---

## 3. 集成架构设计

### 3.1 目标

将 `knot-pdf` 作为 `pageindex-rs` 的 **PDF 解析后端**，替换现有的 LLM-OCR 方案。集成后的调用链为：

```
knot-core (KnotIndexer)
    │
    ▼
pageindex-rs (IndexDispatcher)
    │
    ├── MarkdownParser  (原有，不变)
    ├── DocxParser      (原有，不变)
    └── PdfParser       (重写，使用 knot-pdf)
            │
            ▼
        knot-pdf (Pipeline)
            │
            ▼
        DocumentIR → 转换 → PageNode 树
```

### 3.2 核心转换：DocumentIR → PageNode

集成的关键在于将 `knot-pdf` 的 `DocumentIR` 输出转换为 `pageindex-rs` 的 `PageNode` 树结构。

**knot-pdf 的输出结构：**
```
DocumentIR
├── doc_id: String
├── metadata: DocumentMetadata (title, author, ...)
├── outline: Vec<OutlineItem> (文档大纲/目录)
├── pages: Vec<PageIR>
│   └── PageIR
│       ├── page_index: usize
│       ├── blocks: Vec<BlockIR> (文本块，含 bbox + role)
│       ├── tables: Vec<TableIR> (结构化表格)
│       ├── images: Vec<ImageIR> (图片引用)
│       └── text_score: f32 (文本质量评分)
└── diagnostics: Diagnostics
```

**pageindex-rs 的目标结构：**
```
PageNode (root)
├── node_id: "root"
├── title: "文档标题"
├── content: "" (聚合内容)
├── metadata: NodeMeta { file_path, page_number, token_count, extra }
└── children: Vec<PageNode> (语义树)
    ├── PageNode { title: "第一章", level: 1, ... }
    │   └── PageNode { title: "1.1 小节", level: 2, ... }
    └── PageNode { title: "第二章", level: 1, ... }
```

**转换策略：**

1. **Markdown 中转**：使用 `MarkdownRenderer` 将 `DocumentIR` 渲染为 Markdown 文本。
2. **语义树构建**：复用现有的 `SemanticTreeBuilder::build_from_pages()` 将 Markdown 解析为 `PageNode` 树。
3. **元数据映射**：
   - `DocumentIR.metadata.title` → `PageNode.title`
   - `PageIR.page_index` → `NodeMeta.page_number`
   - `PageIR.text_score` → `NodeMeta.extra["text_score"]`
   - `TableIR.extraction_mode` → `NodeMeta.extra["table_mode"]`

### 3.3 混合模式设计

为保留现有 LLM 方案作为高质量回退，设计两种解析模式：

```rust
pub enum PdfParseMode {
    /// 快速模式：仅使用 knot-pdf 结构化抽取
    /// 适用于：原生文字 PDF、大批量建库
    Fast,

    /// 混合模式：knot-pdf 先行 + LLM 兜底
    /// 适用于：扫描件混合文档、高质量要求场景
    Hybrid,

    /// Legacy 模式：完全使用 LLM OCR（兼容旧行为）
    Legacy,
}
```

**混合模式的工作流：**
```
PDF 文件
  │
  ▼
knot-pdf Pipeline 解析
  │
  ├── text_score >= 0.3 → 直接使用 knot-pdf 结果 ✅
  │
  └── text_score < 0.3 (扫描页/乱码)
      │
      ├── OCR 可用? → PaddleOCR 提取 → 使用 OCR 结果
      └── LLM 可用? → 回退到 LLM 视觉方案
```

---

## 4. 分阶段实施计划

### Phase 1: 基础集成（核心路径）

**目标**：替换 `pageindex-rs` 的 `PdfParser`，使用 `knot-pdf` 作为默认的 PDF 解析后端。

**代码改动清单：**

#### 4.1.1 pageindex-rs/Cargo.toml — 添加 knot-pdf 依赖

```toml
[dependencies]
# 新增
knot-pdf = { path = "../knot-parser/knot-pdf", features = ["pdfium"] }

# 可移除（knot-pdf 内部已包含）
# lopdf = "0.32"           # knot-pdf 已依赖
# pdfium-render = "0.8"    # knot-pdf 已依赖
# base64 = "0.21"          # knot-pdf 已依赖
# image = "0.25"           # knot-pdf 已依赖
```

#### 4.1.2 pageindex-rs/src/formats/pdf.rs — 重写 PdfParser

核心改动：将原来 ~300 行的 LLM-OCR 方案替换为 ~80 行的 knot-pdf 调用。

```rust
use crate::{DocumentParser, PageIndexConfig, PageIndexError, PageNode, NodeMeta};
use async_trait::async_trait;
use knot_pdf::{parse_pdf, Config as PdfConfig, MarkdownRenderer};
use std::path::Path;
use std::collections::HashMap;

pub struct PdfParser;

impl PdfParser {
    pub fn new() -> Self {
        Self
    }

    /// 将 knot-pdf 的 DocumentIR 转换为 pageindex-rs 的 PageNode 列表
    fn convert_to_page_nodes(
        doc: &knot_pdf::DocumentIR,
        file_path: &str,
    ) -> Vec<PageNode> {
        let renderer = MarkdownRenderer::new();
        let mut pages = Vec::new();

        for page in &doc.pages {
            let markdown = renderer.render_page(page);
            if markdown.trim().is_empty() {
                continue;
            }

            let mut extra = HashMap::new();
            extra.insert("text_score".to_string(), page.text_score.to_string());
            extra.insert("is_scanned".to_string(), page.is_scanned_guess.to_string());

            // 记录表格信息
            if !page.tables.is_empty() {
                let table_modes: Vec<String> = page.tables.iter()
                    .map(|t| format!("{:?}", t.extraction_mode))
                    .collect();
                extra.insert("table_modes".to_string(), table_modes.join(","));
            }

            pages.push(PageNode {
                node_id: format!("page-{}", page.page_index + 1),
                title: format!("Page {}", page.page_index + 1),
                level: 1,
                content: markdown,
                summary: None,
                embedding: None,
                metadata: NodeMeta {
                    file_path: file_path.to_string(),
                    page_number: Some((page.page_index + 1) as u32),
                    line_number: None,
                    token_count: 0,
                    extra,
                },
                children: Vec::new(),
            });
        }

        pages
    }
}

#[async_trait]
impl DocumentParser for PdfParser {
    fn can_handle(&self, extension: &str) -> bool {
        matches!(extension, "pdf")
    }

    async fn parse(
        &self,
        path: &Path,
        _config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError> {
        let start_time = std::time::Instant::now();

        // 1. 使用 knot-pdf 解析 PDF → DocumentIR
        let pdf_config = PdfConfig::default();
        let doc = parse_pdf(path, &pdf_config)
            .map_err(|e| PageIndexError::ParseError(format!("knot-pdf error: {}", e)))?;

        // 2. 转换为 PageNode 列表
        let file_path = path.to_string_lossy().to_string();
        let pages = Self::convert_to_page_nodes(&doc, &file_path);

        // 3. 构建语义树
        let title = doc.metadata.title.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        let semantic_root = crate::core::tree_builder::SemanticTreeBuilder::build_from_pages(
            title,
            file_path,
            pages,
        );

        // 4. 记录处理时间
        let duration = start_time.elapsed();
        let mut final_root = semantic_root;
        final_root.metadata.extra.insert(
            "processing_time_ms".to_string(),
            duration.as_millis().to_string(),
        );
        final_root.metadata.extra.insert(
            "processing_time_display".to_string(),
            format!("{:.2}s", duration.as_secs_f64()),
        );
        final_root.metadata.extra.insert(
            "parser".to_string(),
            "knot-pdf".to_string(),
        );

        Ok(final_root)
    }
}
```

#### 4.1.3 关键变化说明

| 变化       | 旧实现                        | 新实现                          |
| ---------- | ----------------------------- | ------------------------------- |
| 依赖       | pdfium + LLM + image + base64 | knot-pdf（内含全部）            |
| LLM 要求   | **必须**提供 LlmProvider      | 不需要                          |
| 代码量     | ~296 行                       | ~80 行                          |
| 100 页耗时 | ~30 分钟                      | ~18 秒                          |
| 表格处理   | LLM "看图说话"                | 结构化检测 + Markdown/HTML 导出 |

---

### Phase 2: 启用 OCR 支持（扫描件增强）

**目标**：对于扫描件或乱码页，启用 knot-pdf 的 OCR 回退链。

**改动点：**

1. `pageindex-rs/Cargo.toml` 添加 OCR feature：

```toml
[features]
default = []
office = ["dep:undoc"]
vision = []
ocr = ["knot-pdf/ocr_paddle", "knot-pdf/pdfium"]  # 新增
```

2. `PdfParser` 中的配置感知：

```rust
// 根据 PageIndexConfig 决定是否启用 OCR
let mut pdf_config = PdfConfig::default();
if config.llm_provider.is_none() {
    // 没有 LLM 时，启用 OCR 作为扫描件的回退
    pdf_config.ocr_enabled = true;
    pdf_config.ocr_mode = knot_pdf::config::OcrMode::Auto;
}
```

---

### Phase 3: 混合模式（LLM 高质量回退）

**目标**：低质量页面（text_score < 0.3）自动回退到 LLM 视觉方案。

**改动点：**

1. 解析后检查 `text_score`：

```rust
// 在 parse() 方法中，解析后检查每页的质量
for page in &doc.pages {
    if page.text_score < 0.3 && config.llm_provider.is_some() {
        // 该页需要 LLM 重新解析
        // 使用 pdfium 渲染页面图片 → LLM 解析
        // 替换该页的 Markdown 内容
    }
}
```

2. 这部分复用 `knot-pdf` 已有的 Vision LLM 集成能力（`vision` feature），无需在 pageindex-rs 中重新实现。

---

### Phase 4: knot-core 改动（可选优化）

**目标**：knot-core 层面优化 PDF 索引流程。

**当前调用链：**
```
KnotIndexer::index_file()
    → IndexDispatcher::index_file()
        → PdfParser::parse()        // 改用 knot-pdf
    → enrich_node() (生成 embedding)
    → flatten_tree() (扁平化为 VectorRecord)
```

**可选改动：**

1. **PDF 文件支持建库**：当前 `index_directory()` 仅索引 `.md` 和 `.txt` 文件，需添加 `.pdf` 支持：

```rust
// knot-core/src/index.rs 第 61 行
// 旧：
if ext == "md" || ext == "txt" {
// 新：
if ext == "md" || ext == "txt" || ext == "pdf" {
```

2. **Pipeline 复用**：如果批量索引多个 PDF，应复用 `knot_pdf::Pipeline` 实例避免重复加载模型。这需要在 `PdfParser` 中持有 `Pipeline`：

```rust
pub struct PdfParser {
    pipeline: knot_pdf::Pipeline,
}

impl PdfParser {
    pub fn new() -> Self {
        Self {
            pipeline: knot_pdf::Pipeline::new(PdfConfig::default()),
        }
    }
}
```

---

## 5. 依赖关系变更

### 5.1 Workspace Cargo.toml（无需改动）

`knot-parser/knot-pdf` 已在 workspace members 中：

```toml
[workspace]
members = [
    "pageindex-rs",
    "knot-parser/knot-pdf",
    ...
]
```

### 5.2 pageindex-rs 依赖变更（精简）

集成 knot-pdf 后，以下直接依赖可移除（由 knot-pdf 传递提供）：

| 依赖            | 现状   | 集成后                                      |
| --------------- | ------ | ------------------------------------------- |
| `lopdf`         | `0.32` | 移除（knot-pdf 使用 `0.38`）                |
| `pdfium-render` | `0.8`  | 移除（knot-pdf 已含）                       |
| `base64`        | `0.21` | 移除（knot-pdf 已含）                       |
| `image`         | `0.25` | 移除（knot-pdf 已含）                       |
| `regex`         | `1.10` | 保留（其他模块可能使用）                    |
| `knot-pdf`      | —      | **新增** `path = "../knot-parser/knot-pdf"` |

### 5.3 knot-core 依赖变更（无需改动）

knot-core 通过 pageindex-rs 间接使用 knot-pdf，无需添加直接依赖。

---

## 6. 风险与注意事项

### 6.1 编译时间

knot-pdf 含多个可选 feature（OCR、Vision、Layout 模型等），全功能编译时间较长。建议：
- 开发阶段仅启用 `default` feature（纯文本抽取 + 表格识别）
- 生产部署按需启用 `pdfium,ocr_paddle,vision`

### 6.2 PDFium 动态库

knot-pdf 的 `pdfium` feature 依赖 `libpdfium.dylib`。项目根目录已存在此文件（软链接），确保运行时可加载。

### 6.3 向后兼容

- 集成后输出依然是 `PageNode` 树，`knot-core` 的 `KnotIndexer` 无需感知底层变化。
- Markdown/Docx 等其他格式的解析完全不受影响。
- 旧的 LLM-OCR 代码可保留在 `PdfParser` 中作为 `Legacy` 模式的实现。

### 6.4 测试策略

1. **单元测试**：验证 `DocumentIR → PageNode` 的转换正确性
2. **集成测试**：用现有测试 PDF 对比新旧方案的输出
3. **性能测试**：对比 100 页 PDF 的解析耗时
4. **RAG 命中率**：确保集成后 RAG 检索质量不低于 80%

---

## 7. 时间估算

| 阶段     | 工作内容                                   | 预估工时      |
| -------- | ------------------------------------------ | ------------- |
| Phase 1  | 基础集成（重写 PdfParser + 依赖调整）      | 2-3 小时      |
| Phase 2  | OCR 支持（feature gate + 配置传递）        | 1-2 小时      |
| Phase 3  | 混合模式（text_score 判断 + LLM 回退）     | 2-3 小时      |
| Phase 4  | knot-core 优化（PDF 建库 + Pipeline 复用） | 1-2 小时      |
| 测试     | 全面测试 + RAG 评测                        | 2-3 小时      |
| **总计** |                                            | **8-13 小时** |
