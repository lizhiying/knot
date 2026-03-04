# knot-pdf 项目架构全景

> 最后更新：2026-02-27  |  版本：0.1.0  |  总代码量：19,500+ 行（源码） + 7,157 行（测试）

## 1. 项目简介

**knot-pdf** 是一个纯 Rust 实现的离线 PDF 解析器，专为 RAG（检索增强生成）场景设计。它能够从 PDF 文件中提取结构化内容（文本、表格、图片、公式），输出 Markdown 或 JSON 格式，不依赖外部服务即可运行。

### 核心特点

| 特性             | 说明                                          |
| ---------------- | --------------------------------------------- |
| **纯 Rust**      | 核心不依赖 Python/Java，编译即用              |
| **离线优先**     | 不需要网络，所有处理在本地完成                |
| **Feature Gate** | 11 个可选 feature，按需启用，最小化二进制体积 |
| **多后端**       | pdf-rs（默认）/ pdfium（可选）双 PDF 引擎     |
| **Hybrid 模式**  | Fast Track + 模型增强 + VLM 外部调用三级策略  |
| **RAG 友好**     | 自动分块、评分、去噪，输出可直接用于向量化    |

---

## 2. 架构总览

```
┌─────────────────────────────────────────────────────────┐
│                      CLI / API                          │
│  knot-pdf parse / markdown / rag / info / config        │
├─────────────────────────────────────────────────────────┤
│                     Pipeline                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐  │
│  │ Backend  │→│  Scoring  │→│  Layout   │→│ Table  │  │
│  │(pdf-rs/  │  │(PageScore)│  │(XY-Cut/  │  │(Stream/│  │
│  │ pdfium)  │  │          │  │ Model)   │  │ Ruled) │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────┘  │
│       ↓              ↓              ↓            ↓      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐  │
│  │  OCR     │  │  Figure  │  │ Formula  │  │Hybrid  │  │
│  │(Tess/   │  │(Detect/  │  │(Detect/  │  │(Strat/ │  │
│  │ Paddle)  │  │ Render)  │  │ ONNX)    │  │ VLM)   │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────┘  │
│       ↓              ↓              ↓            ↓      │
│  ┌─────────────────────────────────────────────────┐    │
│  │              PostProcess Pipeline               │    │
│  │  Watermark → Footnote → List → URL Fixer        │    │
│  └─────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────┤
│                   IR (中间表示)                          │
│  DocumentIR → PageIR → BlockIR / TableIR / ImageIR /   │
│                         FormulaIR                       │
├─────────────────────────────────────────────────────────┤
│                   Render (输出)                          │
│  MarkdownRenderer / RagExporter / JSON                  │
└─────────────────────────────────────────────────────────┘
```

---

## 3. 模块详解

### 3.1 `backend/` — PDF 引擎抽象 (1,376 行, 4 文件)

提供统一的 PDF 访问接口，屏蔽底层 PDF 库差异。

| 文件        | 说明                                                                                                      |
| ----------- | --------------------------------------------------------------------------------------------------------- |
| `traits.rs` | `PdfBackend` trait 定义：`page_count`, `page_info`, `extract_chars`, `extract_images`, `extract_graphics` |
| `pdf_rs.rs` | 基于 `pdf-extract` + `lopdf` 的默认后端，零额外依赖                                                       |
| `pdfium.rs` | 基于 `pdfium-render` 的可选后端（feature: `pdfium`），文本提取更准确                                      |
| `mod.rs`    | 统一导出                                                                                                  |

**关键类型：**
- `ExtractedChar` — 单个字符信息（text, bbox, font_name, font_size）
- `ExtractedImage` — 嵌入图片信息（data, bbox, format）
- `GraphicElement` — 矢量图形元素（线段/矩形/曲线）

### 3.2 `ir/` — 中间表示 (666 行, 8 文件)

所有解析结果的核心数据结构，支持 JSON 序列化/反序列化。

```
DocumentIR
  ├── doc_id: String (SHA-256)
  ├── source_path: PathBuf
  ├── pages: Vec<PageIR>
  │     ├── page_index: usize
  │     ├── size: PageSize
  │     ├── rotation: f32
  │     ├── blocks: Vec<BlockIR>        ← 文本块
  │     │     ├── block_id, bbox, role
  │     │     ├── lines: Vec<TextLine>
  │     │     │     └── spans: Vec<TextSpan>  ← 含 font_name, font_size, is_bold
  │     │     └── normalized_text
  │     ├── tables: Vec<TableIR>        ← 表格
  │     │     ├── rows: Vec<TableRow>
  │     │     │     └── cells: Vec<TableCell>
  │     │     ├── extraction_mode: Ruled/Stream
  │     │     └── fallback_text
  │     ├── images: Vec<ImageIR>        ← 图片（含 is_qrcode 标志）
  │     ├── formulas: Vec<FormulaIR>    ← 公式 (M12)
  │     │     ├── formula_type: Inline/Display
  │     │     ├── raw_text / latex
  │     │     └── equation_number
  │     ├── diagnostics: PageDiagnostics
  │     │     └── parse_strategy (M14)
  │     ├── text_score: f32
  │     ├── is_scanned_guess: bool
  │     ├── source: BornDigital/Ocr/Mixed
  │     └── timings: Timings
  └── metadata: DocumentMetadata
```

**BlockRole 枚举：**
`Body` | `Header` | `Footer` | `Title` | `Heading` | `List` | `Caption` | `PageNumber` | `Sidebar` | `Watermark` | `Footnote` | `Unknown`

### 3.3 `pipeline/` — 解析核心 (1,499 行, 2 文件)

**`Pipeline`** 是整个解析器的调度核心，协调所有模块的工作。

```rust
pub struct Pipeline {
    config: Config,
    store: Option<Box<dyn Store>>,           // 缓存 (sled)
    ocr_backend: Option<Box<dyn OcrBackend>>,
    ocr_renderer: Option<Box<dyn OcrRenderer>>,
    layout_detector: Option<Box<dyn LayoutDetector>>,
    formula_recognizer: Option<Box<dyn FormulaRecognizer>>,
    vision_describer: Option<VisionDescriber>, // Vision LLM 图片描述
    postprocess_pipeline: PostProcessPipeline,  // M13
    max_ocr_workers: usize,
}
```

**处理流程（per page）：**

```
1.  extract_chars()          → 提取字符
2.  extract_images()         → 提取图片（含原始字节用于 QR 检测）
3.  build_blocks()           → 构建文本块（含多栏检测、稀疏列检测）
4.  detect_qrcode()          → 二维码检测（bbox 宽高比 + 像素分析）
5.  likely_complex_ppt?      → 提前检测 PPT 复杂布局（跳过逐图 VLM）
6.  chart_regions_detect()   → 图表区域检测（文本聚类 + Vision LLM）
7.  embedded_image_vlm()     → 嵌入图片 VLM 描述（跳过 QR 和复杂 PPT）
8.  extract_tables()         → 表格提取（Stream + Ruled + Booktabs）
9.  layout_detect()          → 版面检测：修正 BlockRole (M10)
10. compute_page_score()     → 文本质量评分
11. select_parse_strategy()  → 策略选择 (M14)
12. detect_formulas()        → 公式检测 (M12)
13. formula_ocr()            → 公式 OCR → LaTeX (M12, 可选)
14. postprocess_pipeline()   → 后处理：水印/脚注/列表/URL (M13)
15. complex_ppt_vlm?         → PPT 复杂布局 Vision LLM 全页回退
16. → PageIR
```

**API：**
```rust
// 同步
let doc = parse_pdf("file.pdf", &Config::default())?;
let pages = parse_pdf_pages("file.pdf", &config)?;

// 异步 (feature: async)
let doc = parse_pdf_async("file.pdf", &config).await?;
let stream = parse_pdf_stream("file.pdf", &config);
```

### 3.4 `config.rs` — 配置系统 (560 行)

支持 TOML 文件加载 (`knot-pdf.toml`)，所有选项均有合理默认值。

| 分类         | 配置项                       | 默认值  | 说明                         |
| ------------ | ---------------------------- | ------- | ---------------------------- |
| **评分**     | `scoring_text_threshold`     | 0.3     | 低于此值判定为扫描页         |
|              | `garbled_threshold`          | 0.3     | 乱码检测阈值                 |
| **文本**     | `line_merge_tolerance`       | 5.0     | 行合并 y 容差                |
|              | `word_spacing_ratio`         | 0.3     | 词间距检测系数               |
|              | `tight_spacing_ratio`        | 0.15    | 紧凑间距系数                 |
| **布局**     | `multi_column_gap_threshold` | 15.0    | 多栏间距阈值                 |
|              | `reading_order_method`       | `XYCut` | 阅读顺序算法                 |
| **表格**     | `table_min_rows`             | 2       | 最小行数                     |
|              | `table_min_cols`             | 2       | 最小列数                     |
| **OCR**      | `ocr_mode`                   | `Auto`  | OCR 触发模式                 |
|              | `ocr_workers`                | 2       | OCR 并发数                   |
| **公式**     | `formula_detection_enabled`  | true    | 是否检测公式                 |
|              | `formula_model_enabled`      | false   | 是否启用 ONNX OCR            |
| **后处理**   | `postprocess_enabled`        | true    | 后处理管线开关               |
|              | `remove_watermark`           | true    | 水印过滤                     |
|              | `separate_footnotes`         | false   | 脚注分离                     |
| **混合模式** | `parse_mode`                 | `Auto`  | FastTrack/Enhanced/Full/Auto |
|              | `vlm_enabled`                | false   | VLM 外部调用                 |
|              | `vlm_score_threshold`        | 0.3     | VLM 触发分数阈值             |

### 3.5 `scoring/` — 文本质量评分 (314 行, 2 文件)

为每一页计算 `text_score`（0.0~1.0），用于判断是否为扫描页、是否需要 OCR、选择解析策略。

评分维度：字符覆盖率、字体多样性、乱码比例、Unicode 范围分布。

### 3.6 `table/` — 表格提取 (4,980 行, 9 文件)

**最大的模块**，实现了三种表格提取模式：

| 模式         | 文件              | 原理                               |
| ------------ | ----------------- | ---------------------------------- |
| **Stream**   | `stream.rs`       | 无边框表格：基于文本对齐推断列边界 |
| **Ruled**    | `ruled.rs`        | 有边框表格：基于矢量线段检测行列   |
| **Booktabs** | `ruled.rs` (内含) | 三线表：仅水平线，列用文本对齐推断 |

附加模块：
- `enhance.rs` — 表格增强（单元格类型检测、合并单元格修复）
- `structure_detect.rs` — 表格结构检测规则
- `onnx_structure.rs` — ONNX 表格结构模型（feature: `table_model`）

### 3.7 `layout/` — 版面分析 (3,462 行, 5 文件)

| 文件               | 说明                                                     |
| ------------------ | -------------------------------------------------------- |
| `reading_order.rs` | 阅读顺序重建：支持 LTR/TTB 两种策略                      |
| `xy_cut.rs`        | **XY-Cut 递归分割**：自动检测多栏布局，递归分割页面 (M9) |
| `detect.rs`        | 规则版面检测：标题/页眉页脚/页码/侧栏角色分类 (M10)      |
| `onnx_detect.rs`   | ONNX 版面检测模型接口（feature: `layout_model`）(M10)    |
| `mod.rs`           | `LayoutDetector` trait + 默认实现                        |

### 3.8 `formula/` — 公式检测与识别 (1,077 行, 4 文件)

| 文件                | 说明                                                                   |
| ------------------- | ---------------------------------------------------------------------- |
| `detect.rs`         | 5 种启发式检测：数学字符密度、数学字体、上下标几何、孤立短块、公式编号 |
| `recognize.rs`      | `FormulaRecognizer` trait + `MockFormulaRecognizer`                    |
| `onnx_recognize.rs` | **纯 Rust ONNX 推理**（`ort` crate）：TrOCR Encoder-Decoder 架构 (M12) |
| `mod.rs`            | 模块入口（含 feature gate: `formula_model`）                           |

**模型信息（pix2text-mfr）：**
- Encoder: DeiT (83.4 MB ONNX)
- Decoder: TrOCR (28.7 MB ONNX)
- 词表: 1200 tokens
- 推理: ~150-340ms/公式 (CPU)

### 3.9 `postprocess/` — 后处理管线 (1,214 行, 6 文件)

可插拔的后处理框架，按顺序执行：

```rust
pub trait PostProcessor: Send + Sync {
    fn name(&self) -> &str;
    fn process_page(&self, page: &mut PageIR, config: &Config);
}
```

| 处理器             | 文件           | 功能                                                       |
| ------------------ | -------------- | ---------------------------------------------------------- |
| `WatermarkFilter`  | `watermark.rs` | 常见水印文本匹配 + 大面积检测 → 标记并移除（联系方式豁免） |
| `FootnoteDetector` | `footnote.rs`  | 底部区域 + 脚注标记(¹/[1]/*/†) + 字号对比                  |
| `ListDetector`     | `list.rs`      | 有序(1./①/(a))和无序(•/-/–)列表 → `role=List`              |
| `UrlFixer`         | `url.rs`       | 碎片化 URL span 合并                                       |
| `ParagraphMerger`  | `paragraph.rs` | 跨页段落检测辅助函数                                       |

### 3.10 `hybrid/` — 混合解析模式 (580 行, 4 文件)

三级解析策略框架 (M14)：

```
text_score ≥ 0.7  →  FastTrackOnly     (最快，纯规则)
text_score 中等    →  FastTrackPlusModels (模型增强)
text_score < 0.3  →  FullWithVlm        (VLM 外部调用)
```

| 文件          | 说明                                                     |
| ------------- | -------------------------------------------------------- |
| `strategy.rs` | `select_parse_strategy()` + `ParseStrategy` 枚举         |
| `vlm.rs`      | `VlmBackend` trait + `MockVlmBackend` + Markdown→BlockIR |
| `fusion.rs`   | Fast Track + VLM 结果融合（role 修正 / 内容补充）        |

### 3.11 其他模块

| 模块           | 行数     | 说明                                                         |
| -------------- | -------- | ------------------------------------------------------------ |
| `ocr/`         | 452 行   | OCR 后端抽象：Tesseract / PaddleOCR / Mock                   |
| `figure/`      | 417 行   | 图表区域检测与渲染（矢量图→PNG）                             |
| `render/`      | 700+ 行  | Markdown 渲染器 + RAG 导出器 + OCR 表格自动格式化            |
| `hf_detect/`   | 303 行   | 页眉/页脚/页码检测（跨页重复文本分析）                       |
| `store/`       | 182 行   | sled 缓存（避免重复解析）                                    |
| `vision/`      | 200+ 行  | Vision LLM 图片描述（OpenAI 兼容 API，含错误诊断和大小限制） |
| `mem_track.rs` | 163 行   | RSS 内存监控                                                 |
| `bin/`         | 1,042 行 | CLI 工具（parse/markdown/rag/info/config 子命令）            |

---

## 4. Feature Gate 清单

所有可选功能通过 Cargo feature 控制，默认构建零额外依赖：

| Feature         | 依赖                 | 说明                          |
| --------------- | -------------------- | ----------------------------- |
| `pdfium`        | pdfium-render, image | Pdfium PDF 后端（文本更准确） |
| `ocr_tesseract` | leptess              | Tesseract OCR 引擎            |
| `ocr_paddle`    | pure-onnx-ocr, image | PaddleOCR 引擎                |
| `layout_model`  | tract-onnx, image    | ONNX 版面检测模型             |
| `table_model`   | tract-onnx, image    | ONNX 表格结构模型             |
| `formula_model` | ort, image           | ONNX 公式 OCR 模型（TrOCR）   |
| `vision`        | ureq, base64         | Vision LLM 外部调用           |
| `async`         | tokio                | 异步解析 API                  |
| `store_sled`    | sled                 | sled KV 缓存                  |
| `cli`           | clap, env_logger     | CLI 工具                      |

**组合示例：**
```bash
# 最小构建（纯规则解析，零额外依赖）
cargo build --lib

# 推荐生产构建（Pdfium + 公式 + CLI）
cargo build --features "pdfium,formula_model,cli"

# 全功能构建
cargo build --features "pdfium,ocr_tesseract,layout_model,table_model,formula_model,vision,async,store_sled,cli"
```

---

## 4.1 构建 & 全局安装

### 一键安装

```bash
# 使用安装脚本（推荐）
bash scripts/install_knot_pdf.sh
```

安装脚本会：
1. 编译 `knot-pdf-cli` 并安装到 `~/.cargo/bin/`
2. 安装后全局可用（确保 `~/.cargo/bin` 在 `$PATH` 中）
3. 如果 `~/.config/knot-pdf/knot-pdf.toml` 不存在，自动创建默认配置

### 手动安装

```bash
# 进入项目目录
cd knot-parser/knot-pdf

# 编译安装到 ~/.cargo/bin/
# Features 按需选择，cli 是必须的
cargo install --path . \
    --features "cli,pdfium,ocr_paddle,vision,formula_model" \
    --bin knot-pdf-cli \
    --force

# 验证安装
knot-pdf-cli --version
knot-pdf-cli --help
```

### 确保 PATH 包含 cargo bin

如果安装后 `knot-pdf-cli` 命令找不到，在 `~/.zshrc` 中添加：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### 卸载

```bash
cargo uninstall knot-pdf
```

---

## 4.2 配置文件

### 配置搜索顺序

`knot-pdf-cli` 启动时按以下优先级自动搜索配置文件：

| 优先级 | 路径                                   | 说明                       |
| ------ | -------------------------------------- | -------------------------- |
| 1      | `./knot-pdf.toml`                      | 当前工作目录（项目级配置） |
| 2      | `<exe_dir>/knot-pdf.toml`              | 可执行文件同级目录         |
| 3      | **`~/.config/knot-pdf/knot-pdf.toml`** | 用户全局配置（推荐）       |

也可通过 `-c` 参数指定：

```bash
knot-pdf-cli -c /path/to/my-config.toml markdown input.pdf
```

### 配置文件位置

```
~/.config/knot-pdf/knot-pdf.toml
```

### 配置文件结构

```toml
# ── 文本抽取 ──────────────────────────────────────────
scoring_text_threshold = 0.3   # 低于此值判定为扫描页
garbled_threshold = 0.2        # 乱码检测阈值
strip_headers_footers = true   # 去除页眉页脚
max_columns = 3                # 多栏检测上限

# ── 布局分析 ──────────────────────────────────────────
reading_order_method = "xy_cut"  # 阅读顺序算法

# ── OCR 配置（需 --features ocr_paddle 或 ocr_tesseract）──
ocr_enabled = true
ocr_mode = "auto"              # auto | force_all | disabled
ocr_languages = ["eng"]
ocr_render_width = 1024
ocr_workers = 1

# ── 图表检测 (M8) ────────────────────────────────────
figure_detection_enabled = true

# ── 公式检测 (M12) ───────────────────────────────────
formula_detection_enabled = true   # 公式区域检测（纯规则）
formula_model_enabled = false      # 公式 OCR（需 --features formula_model）

# ── 后处理 (M13) ─────────────────────────────────────
postprocess_enabled = true         # 后处理管线总开关
remove_watermark = true            # 水印过滤
separate_footnotes = false         # 脚注分离
merge_cross_page_paragraphs = true # 跨页段落合并

# ── 混合解析模式 (M14) ───────────────────────────────
parse_mode = "auto"    # auto | fast_track | enhanced | full
vlm_enabled = false    # VLM 外部调用

# ── Vision LLM（图表语义理解，需 --features vision）───
vision_api_url = "http://localhost:11434/v1/chat/completions"
vision_model = "glm-ocr:latest"

# ── 输出控制 ──────────────────────────────────────────
emit_markdown = true
emit_ir_json = false

# ── 资源控制 ──────────────────────────────────────────
max_memory_mb = 200
page_timeout_secs = 0  # 0 = 不超时
```

> 完整配置模板见 `scripts/knot-pdf.default.toml`

### 查看当前生效的配置

```bash
knot-pdf-cli config show
```

---

## 5. 数据流

### 5.1 单页处理流程

```
PDF 文件
  │
  ▼
Backend.extract_chars()         ← 提取字符（text, bbox, font）
Backend.extract_images()        ← 提取嵌入图片（含原始字节）
Backend.extract_graphics()      ← 提取矢量线段
  │
  ├─→ build_blocks()            ← 字符 → 行 → 块，含多栏检测
  │     ├─→ detect_implicit_grids()      ← 隐式网格检测（保护跨全宽标题）
  │     ├─→ detect_sparse_column_grid()  ← 稀疏列检测
  │     └─→ xy_cut / reading_order()     ← 阅读顺序重建
  │
  ├─→ detect_qrcode()           ← 二维码检测（宽高比 + 像素对比度）
  │
  ├─→ likely_complex_ppt?       ← PPT 复杂布局提前检测（优化跳过逐图 VLM）
  │
  ├─→ chart_region_vlm()        ← 图表 VLM 描述（跳过复杂 PPT）
  │
  ├─→ embedded_image_vlm()      ← 嵌入图片 VLM（跳过 QR 和复杂 PPT）
  │
  ├─→ extract_tables()          ← 检测 + 提取表格
  │     ├─→ has_enough_lines() → Stream 模式
  │     ├─→ detect_ruled_table() → Ruled 模式
  │     └─→ detect_booktabs() → Booktabs 模式
  │
  ├─→ layout_detect()           ← [可选] 版面角色修正
  │
  ├─→ compute_page_score()      ← 文本质量评分 → text_score
  │
  ├─→ select_parse_strategy()   ← 策略选择
  │
  ├─→ detect_formulas()         ← 公式区域检测
  │     └─→ formula_ocr()       ← [可选] LaTeX 识别
  │
  ├─→ PostProcessPipeline       ← 后处理
  │     ├─→ WatermarkFilter     （联系方式豁免，避免 PPT 尾页误删）
  │     ├─→ FootnoteDetector
  │     ├─→ ListDetector
  │     └─→ UrlFixer
  │
  └─→ complex_ppt_vlm?          ← PPT 复杂布局全页 VLM 回退
            │
            ▼
         PageIR                  ← 最终输出
```

### 5.2 文档级流程

```
Pipeline.parse(path)
  │
  ├─→ Backend::open(path)       ← 打开 PDF
  ├─→ compute_doc_id(data)      ← SHA-256 文档 ID
  ├─→ for page in 0..page_count:
  │     ├─→ Store.get(page)?    ← 缓存命中?
  │     ├─→ process_page()      ← 逐页处理（见上）
  │     └─→ Store.put(page)     ← 写入缓存
  │
  ├─→ header_footer_detect()    ← 跨页重复文本检测
  │
  └─→ DocumentIR               ← 最终文档
        ├─→ MarkdownRenderer    ← 输出 Markdown
        └─→ RagExporter         ← 输出 RAG 文本块
```

---

## 6. 测试体系

| 类别              | 文件                             | 测试数 | 说明                 |
| ----------------- | -------------------------------- | ------ | -------------------- |
| **Lib 单元测试**  | `src/**` 内 `#[cfg(test)]`       | 129    | 内嵌在各模块中       |
| **集成测试**      | `tests/integration_tests.rs`     | 6      | 端到端 PDF 解析验证  |
| **IR 测试**       | `tests/ir_tests.rs`              | 13     | IR 序列化/反序列化   |
| **M2 测试**       | `tests/m2_tests.rs`              | —      | 页眉页脚检测         |
| **M3 测试**       | `tests/m3_tests.rs`              | 22     | Stream 表格提取      |
| **M4 测试**       | `tests/m4_tests.rs`              | —      | Ruled 表格提取       |
| **M5 测试**       | `tests/m5_tests.rs`              | —      | OCR 回退             |
| **M6 测试**       | `tests/m6_tests.rs`              | —      | 性能评测             |
| **M9 测试**       | `tests/m9_xycut_tests.rs`        | —      | XY-Cut 算法          |
| **M10 测试**      | `tests/m10_layout_tests.rs`      | 10     | 版面检测             |
| **M11 测试**      | `tests/m11_table_model_tests.rs` | —      | 表格模型             |
| **M12 测试**      | `tests/m12_formula_tests.rs`     | 7      | 公式检测 + ONNX 推理 |
| **评测 fixtures** | `tests/fixtures/eval_output/`    | —      | 85 份 PDF 的期望输出 |

**运行测试：**
```bash
# 默认测试（无可选 feature）
cargo test --lib

# 含公式模型测试
cargo test --lib --features formula_model

# 全部集成测试
cargo test
```

---

## 7. Milestone 进度

| #   | Milestone                   | 状态 | 核心交付                                     |
| --- | --------------------------- | ---- | -------------------------------------------- |
| M1  | IR + Fast Track             | ✅    | 基础 PDF 解析 → IR                           |
| M2  | PageScore + 页眉页脚 + 多栏 | ✅    | 文本评分、页眉页脚检测、多栏支持             |
| M3  | Stream 表格                 | ✅    | 无边框表格提取                               |
| M4  | Ruled 表格                  | ✅    | 有边框表格提取                               |
| M5  | OCR 回退 + Store            | ✅    | 扫描件 OCR、sled 缓存                        |
| M6  | 性能评测                    | ✅    | 基准测试、评测框架                           |
| M7  | CLI 工具                    | ✅    | knot-pdf 命令行                              |
| M8  | 图表检测                    | ✅    | 矢量图渲染、嵌入图提取                       |
| M9  | XY-Cut 阅读顺序             | ✅    | 递归分割阅读顺序                             |
| M10 | 版面检测                    | ✅    | 规则 + ONNX 版面检测                         |
| M11 | 表格模型增强                | ✅    | Booktabs、ONNX 结构检测                      |
| M12 | 公式检测与识别              | ✅    | 规则检测 + 纯 Rust ONNX OCR                  |
| M13 | 后处理增强                  | ✅    | 水印/脚注/列表/URL 后处理管线                |
| M14 | Hybrid 解析模式             | ✅    | 三级策略 + VLM 接口 + 结果融合               |
| —   | QR 检测 + VLM 性能优化      | ✅    | 二维码自动标记、PPT VLM 跳过、OCR 表格格式化 |
| M15 | 基准对比                    | 📋    | MinerU 对比评测（规划中）                    |

---

## 8. 目录结构

```
knot-pdf/
├── Cargo.toml                    # 依赖 + 11 个 feature gates
├── src/
│   ├── lib.rs                    # 库入口（pub mod + 顶层 API）
│   ├── config.rs                 # 配置系统（TOML + 默认值）
│   ├── error.rs                  # 错误类型
│   ├── mem_track.rs              # 内存监控
│   ├── backend/                  # PDF 引擎抽象
│   │   ├── traits.rs             #   PdfBackend trait
│   │   ├── pdf_rs.rs             #   pdf-extract 后端
│   │   └── pdfium.rs             #   pdfium-render 后端
│   ├── ir/                       # 中间表示
│   │   ├── document.rs           #   DocumentIR
│   │   ├── page.rs               #   PageIR
│   │   ├── block.rs              #   BlockIR / TextLine / TextSpan
│   │   ├── table.rs              #   TableIR / TableRow / TableCell
│   │   ├── image.rs              #   ImageIR
│   │   ├── formula.rs            #   FormulaIR (M12)
│   │   └── types.rs              #   BBox, BlockRole, Timings...
│   ├── pipeline/                 # 解析核心
│   │   ├── mod.rs                #   Pipeline struct + process_page
│   │   └── async_pipeline.rs     #   异步 API
│   ├── scoring/                  # 文本质量评分
│   ├── layout/                   # 版面分析
│   │   ├── reading_order.rs      #   阅读顺序
│   │   ├── xy_cut.rs             #   XY-Cut 递归分割 (M9)
│   │   ├── detect.rs             #   规则版面检测 (M10)
│   │   └── onnx_detect.rs        #   ONNX 模型检测 (M10)
│   ├── table/                    # 表格提取
│   │   ├── stream.rs             #   Stream 模式
│   │   ├── ruled.rs              #   Ruled + Booktabs 模式
│   │   ├── enhance.rs            #   表格增强 (M11)
│   │   ├── structure_detect.rs   #   结构检测规则 (M11)
│   │   └── onnx_structure.rs     #   ONNX 模型 (M11)
│   ├── formula/                  # 公式检测与识别 (M12)
│   │   ├── detect.rs             #   启发式检测
│   │   ├── recognize.rs          #   FormulaRecognizer trait
│   │   └── onnx_recognize.rs     #   ONNX TrOCR 推理
│   ├── postprocess/              # 后处理管线 (M13)
│   │   ├── mod.rs                #   PostProcessor trait + Pipeline
│   │   ├── watermark.rs          #   水印过滤
│   │   ├── footnote.rs           #   脚注检测
│   │   ├── list.rs               #   列表识别
│   │   ├── url.rs                #   URL 修复
│   │   └── paragraph.rs          #   段落跨页检测
│   ├── hybrid/                   # 混合解析模式 (M14)
│   │   ├── strategy.rs           #   策略选择
│   │   ├── vlm.rs                #   VLM 后端 trait
│   │   └── fusion.rs             #   结果融合
│   ├── ocr/                      # OCR 后端
│   ├── figure/                   # 图表检测
│   ├── render/                   # 输出渲染器
│   ├── hf_detect/                # 页眉页脚检测
│   ├── store/                    # 缓存层
│   ├── vision/                   # Vision LLM
│   └── bin/                      # CLI 工具
├── tests/                        # 集成测试（17 个文件）
├── docs/                         # 文档
│   ├── milestones/               #   15 个 Milestone 文档
│   └── *.md                      #   技术指南
├── models/                       # ONNX 模型文件（gitignored）
└── scripts/
    ├── install_knot_pdf.sh       # 一键安装脚本
    └── knot-pdf.default.toml     # 默认配置文件模板
```

---

## 9. 快速上手

### Rust API

```rust
use knot_pdf::{parse_pdf, Config};

fn main() {
    let config = Config::default();
    let doc = parse_pdf("paper.pdf", &config).unwrap();

    println!("Pages: {}", doc.pages.len());

    for page in &doc.pages {
        println!("Page {}: {} blocks, {} tables, strategy={}",
            page.page_index,
            page.blocks.len(),
            page.tables.len(),
            page.diagnostics.parse_strategy.as_deref().unwrap_or("N/A"),
        );

        for block in &page.blocks {
            println!("  [{:?}] {}", block.role, block.full_text());
        }
    }
}
```

### CLI 使用

```bash
# 输出 Markdown
knot-pdf-cli markdown input.pdf -o output.md

# 输出 RAG 文本
knot-pdf-cli rag input.pdf -o output.txt

# 输出 JSON IR
knot-pdf-cli parse input.pdf -o output.json

# 查看 PDF 信息
knot-pdf-cli info input.pdf

# 查看当前配置
knot-pdf-cli config show

# 指定配置文件
knot-pdf-cli -c ~/.config/knot-pdf/knot-pdf.toml markdown input.pdf

# 详细日志
knot-pdf-cli -vv markdown input.pdf
```
