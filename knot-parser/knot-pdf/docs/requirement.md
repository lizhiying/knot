需求文档：knot-pdf（Rust 离线 PDF 解析库）

1. 背景与目标

1.1 背景

RAG 应用中用户经常针对 PDF 内容提问，尤其是表格中的数值、对比、汇总、增长率等。现有“全页渲染→VLM OCR→Markdown”方案质量高，但在离线、低内存、无模型环境下不可用且性能差。

1.2 核心目标

构建一个 Rust 原生、离线优先、低内存占用、模块化可扩展 的 PDF 解析库，用于桌面/本地 RAG App：
	•	离线可用：无网络、无 LLM 情况下，仍可解析绝大多数 born-digital PDF（嵌入文本 PDF），并在扫描件上提供 OCR 兜底（可选）。
	•	RAG 优先：输出不以“漂亮 Markdown”为第一目标，而以“可检索、可定位、可计算”为第一目标。
	•	表格可问答：将表格解析为结构化 TableIR，支持后续由确定性引擎（SQL/DataFrame）完成计算与回答。
	•	低内存：逐页处理、限制峰值、可断点续传。

⸻

2. 范围定义

2.1 In Scope（必须实现）
	1.	Fast Track（born-digital）

	•	逐页抽取文本（带 bbox/字体信息尽量有）
	•	复原阅读顺序（多列/段落/列表尽量合理）
	•	页眉页脚去重（跨页重复文本识别）
	•	图片/图形区域检测（至少给出 bbox 和引用）

	2.	表格：结构化输出优先

	•	提供表格候选区域检测
	•	规则型表格抽取（至少覆盖常见两类）：
	•	ruled（有线表格）：网格/线段 → cell
	•	stream（无框表格）：行聚类 + 列聚类
	•	失败时保证“信息不丢”：输出 table_as_text / table_as_kv_lines

	3.	离线 OCR 兜底（可配置开关）

	•	仅对“疑似扫描页/文字不可用页”启用
	•	支持 region-level 渲染 + OCR（不做全页无脑 OCR）
	•	OCR 后也要回填到同一套 IR

	4.	统一 IR（中间表示）

	•	DocumentIR / PageIR / BlockIR / TableIR / ImageIR
	•	序列化能力（serde）
	•	可转换为 Markdown（用于可读性）但不作为唯一输出

	5.	并发与资源控制

	•	Pipeline 化处理（有界队列）
	•	限制渲染/OCR 并发与内存峰值
	•	可配置 max_memory_mb / max_concurrency

	6.	断点续传与缓存

	•	基于 Store 抽象（默认 sled）
	•	记录每页状态、耗时、来源、质量评分

⸻

2.2 Out of Scope（明确不做/后置）
	•	追求 OCRFlux/VLM 级“全场景高保真 Markdown”输出（尤其复杂表格、跨页合并、图表语义解释）
	•	复杂图表语义理解（趋势解释、图中关系推断）
	•	任意自然语言自动分析（NL→SQL/DSL）能力（属于上层 RAG 组件，不属于解析库）
	•	高级公式/LaTeX 识别与重建（后续增强）

⸻

3. 用户场景与成功标准

3.1 典型问题类型
	•	“表 2 中 2023 年收入是多少？”
	•	“各地区销量求和并排序”
	•	“2023 比 2022 增长率”
	•	“某产品在 Q1-Q4 的变化趋势”

3.2 成功标准（面向 RAG）
	•	对 born-digital PDF：文本召回稳定，数字/缩写不易丢失
	•	表格能落成结构化（至少行列可定位），并能在上层做 sum/topk/growth 等计算
	•	即使表格结构失败，也能通过 table_as_text 进行检索命中与人工理解

⸻

4. 架构与模块设计

4.1 总体架构（分层）
	•	API 层：同步/异步解析接口，配置入口
	•	Core Pipeline 层：逐页读取、解析、评分、fallback、输出 IR
	•	Backends 层（可插拔）：
	•	PdfBackend：文本/字符/bbox/图片/（可选渲染）
	•	OcrBackend：离线 OCR（可选）
	•	Store：缓存与断点
	•	Renderers 层：
	•	MarkdownRenderer
	•	RAGSerializer（表格行/单元格扁平化为检索友好格式）

⸻

5. 统一 IR 设计（必须实现）

5.1 DocumentIR
	•	doc_id（hash）
	•	metadata（title/author/created…尽力）
	•	outline（如果能拿到）
	•	pages（可 streaming 输出）
	•	diagnostics（全局）

5.2 PageIR（关键）

字段建议：
	•	page_index
	•	size / rotation
	•	blocks: Vec
	•	tables: Vec（可为空）
	•	images: Vec
	•	diagnostics: PageDiagnostics
	•	text_score（0-1）
	•	is_scanned_guess（bool）
	•	source（born_digital / ocr / mixed）
	•	timings（extract/render/ocr）

5.3 BlockIR
	•	bbox
	•	role（Body/Header/Footer/Title/List/Caption/Unknown）
	•	lines/spans（尽可能保留 font_size/bold）
	•	normalized_text（用于索引）

5.4 TableIR（RAG 核心）

最低要求：
	•	table_id
	•	page_index
	•	bbox
	•	extraction_mode（ruled/stream/unknown）
	•	headers（可为空但要有）
	•	rows（行数组）
	•	cells（二维或稀疏 map）
	•	cell_types（number/text/percent/currency/date/unknown）
	•	fallback_text（必须有）：table_as_text（列名=值/行序列）

约束：TableIR 必须能被上层稳定转换为：

	•	CSV（用于计算）
	•	KV-lines（用于检索）
	•	Markdown（用于阅读）

5.5 ImageIR
	•	image_id
	•	page_index
	•	bbox
	•	format（png/jpg/unknown）
	•	bytes_ref（可选：避免内存复制，支持延迟加载）
	•	caption_refs（关联文本块 id）

⸻

6. 核心流程与策略

6.1 Pipeline（有界、低内存）
	1.	打开 PDF → 读取页信息（page count / sizes）
	2.	每页：
	•	Fast Track：抽字符/文本块 + bbox
	•	质量评分（PageScore）
	•	表格候选检测 + 抽取（规则）
	•	图片/图形候选记录
	•	若 PageScore 低：进入 OCR fallback（可配置）
	3.	输出 PageIR（写入 Store / 回调给上层）
	4.	释放当前页中间对象（drop）

6.2 PageScore（决定是否 OCR/是否降级）

建议指标：
	•	printable_char_count
	•	printable_ratio
	•	unique_ratio / entropy proxy
	•	text_area_coverage（bbox 覆盖面积 / page area）
	•	median_font_size（防止标题页误判）
	•	suspicious_garbled_rate

输出：
	•	score: 0..1
	•	reason flags（LowText/HighGarbled/LowCoverage）

6.3 表格策略（强制“信息不丢”）
	•	优先输出结构化 TableIR
	•	若结构化不可靠：输出 fallback_text（必须）
	•	表格索引策略：在 renderer 层生成
	•	table_row_lines: “表=… 页=… 行key=… 列=… 值=…”
	•	table_cell_lines: 单元格粒度（数字检索更稳）

6.4 OCR fallback（可选）

触发条件（可配置）：
	•	PageScore < threshold
	•	或用户强制 mode=ocr_only
执行方式：
	•	优先 region OCR（根据页面空白/图片区域/文本缺失区域）
	•	渲染宽度默认 512（可调）
	•	OCR 输出也走 BlockIR（带 bbox 近似）

⸻

7. 对外 API 需求

7.1 同步 API
	•	parse_pdf(path, config) -> DocumentIR（或 iterator）
	•	大文档推荐：parse_pdf_pages(path, config) -> impl Iterator<Item=PageIR>

7.2 异步 API（tokio feature）
	•	parse_pdf_async(path, config, handler: impl Fn(PageIR) + Send)

7.3 配置项（Config）
	•	memory.max_memory_mb
	•	pipeline.page_queue_size
	•	concurrency.ocr_workers / render_workers
	•	scoring.text_threshold
	•	ocr.enabled / languages
	•	store.enabled / store_path
	•	output.emit_markdown / emit_ir_json

⸻

8. 非功能需求（NFR）

8.1 性能
	•	born-digital：100 页解析在普通电脑上应为秒级～十几秒级（取决于后端）
	•	OCR fallback：仅对需要的页启用，避免全页 OCR

8.2 内存
	•	峰值可控：默认 max_memory_mb=200
	•	逐页释放：任何情况下不得缓存整本 PDF 的渲染图片

8.3 可靠性
	•	加密 PDF：明确错误返回（不崩溃）
	•	损坏 PDF：尽可能跳页继续解析，并记录 diagnostics
	•	超大 PDF：支持断点续传与中断恢复

8.4 许可与分发
	•	核心库默认选择 宽松许可证依赖（MIT/Apache2）
	•	不使用 AGPL/JVM 类表格引擎

⸻

1. 验收标准（Acceptance Criteria）

9.1 功能验收
	•	能解析 born-digital PDF 并输出 PageIR（含 blocks）
	•	表格能输出 TableIR，且每个 TableIR 必须包含 fallback_text
	•	OCR 开启时，对扫描页能输出可检索文本（即使结构一般）
	•	支持 store 断点续传：中断后可从指定页继续

9.2 质量验收（RAG 角度）
	•	对表格数值类问题：扁平化索引能命中（数字/年份/百分比）
	•	对常见两列表格：列映射正确率达到可用水平（用自建样本评测）

⸻

10. Checklist（按模块/阶段）

10.1 基础工程 Checklist
	•	建立 crate：knot-pdf
	•	feature 设计：
	•	default（纯解析）
	•	async（tokio）
	•	store_sled
	•	pdfium（渲染增强，可选）
	•	ocr_paddle（Pure Rust PaddleOCR 离线 OCR，可选）
	•	CI：
	•	fmt/clippy/test
	•	最小样本解析测试（小 PDF）

10.2 IR Checklist（必须先完成）
	•	定义 DocumentIR/PageIR/BlockIR/TableIR/ImageIR
	•	serde 序列化/反序列化
	•	ID 体系（doc_id/page_id/table_id）
	•	fallback_text 强制字段（TableIR）
	•	MarkdownRenderer（最低可读输出）
	•	RAG 扁平化导出：
	•	table_row_lines
	•	table_cell_lines
	•	block_lines（带 page/bbox 引用）

10.3 Fast Track 文本抽取 Checklist
	•	PdfBackend trait 定义（extract_chars/extract_blocks/extract_images）
	•	实现一个默认后端（优先 bbox/char 信息）
	•	阅读顺序重建：
	•	行聚类（y）
	•	列检测（x 分布）
	•	段落合并（行距/缩进）
	•	页眉页脚检测：
	•	跨页重复块识别
	•	标记 role=Header/Footer 并可配置剔除

10.4 PageScore Checklist
	•	printable_char_count
	•	printable_ratio
	•	garbled_rate
	•	text_area_coverage
	•	median_font_size
	•	输出 score + flags
	•	单元测试：典型 born-digital / 扫描页 / 标题页

10.5 表格候选 + 规则抽取 Checklist
	•	表格候选检测：
	•	文本对齐特征（多行 x 对齐）
	•	[ ]（可选）线段/矩形特征（若后端支持）
	•	stream 表格抽取：
	•	行聚类
	•	列聚类
	•	header 推断（第一行/多行表头）
	•	ruled 表格抽取（若能拿线段）：
	•	线段归一化
	•	网格生成
	•	cell bbox
	•	文本投影到 cell
	•	输出 TableIR：
	•	headers/rows/cells（尽力）
	•	cell_types（number/percent/currency…）
	•	fallback_text（必须）
	•	评测样本集：
	•	20 个 stream 表格
	•	20 个 ruled 表格
	•	10 个复杂/失败表格（验证 fallback）

10.6 OCR Fallback Checklist（可选特性）
	•	OcrBackend trait
	•	触发条件（PageScore 阈值 + 配置）
	•	region 渲染策略（若无 region 信息，先整页低分辨率）
	•	OCR 输出写回 BlockIR（带 bbox 近似）
	•	OCR 质量评分（可打印率/字数/置信度若可得）
	•	内存释放与限流（Semaphore）

10.6.1 OCR 后端技术选型决策

目标：OCR 必须内置到 app 中，不要求用户额外安装 Tesseract 等系统依赖，需跨平台支持 macOS / Linux / Windows。

三种候选方案对比：

| 维度       | ① pure-onnx-ocr                                                    | ② kreuzberg-tesseract                                   | ③ ocr-rs                                |
| ---------- | ------------------------------------------------------------------ | ------------------------------------------------------- | --------------------------------------- |
| 核心原理   | Pure Rust 重新实现 PaddleOCR（DBNet + SVTR），使用 tract-onnx 推理 | 构建时从源码编译 Tesseract/Leptonica 并静态链接进二进制 | Pure Rust 基于 PaddleOCR + MNN 推理框架 |
| 系统依赖   | ❌ 零依赖，cargo build 即可                                         | ❌ 零运行时依赖（构建时自动编译 C++）                    | ❌ 零依赖                                |
| 中文支持   | ✅ PaddleOCR PP-OCRv5 模型，中/英/日/韩                             | ✅ Tesseract 原生多语言                                  | ✅ PaddleOCR 模型                        |
| 跨平台     | ✅ Mac/Linux/Windows/WASM                                           | ✅ Mac/Linux/Windows                                     | ✅ Mac/Linux/Windows                     |
| 模型文件   | 需分发 det.onnx + rec.onnx (~20-50MB)                              | 需分发 .traineddata 语言包                              | 需分发 MNN 模型                         |
| 构建复杂度 | ⭐ 低（纯 Rust）                                                    | ⭐⭐⭐ 高（首次编译 C++ >5分钟）                           | ⭐⭐ 中等                                 |
| 二进制体积 | 中等（模型文件外挂）                                               | 较大（静态链接整个 Tesseract）                          | 中等                                    |

最终选型：pure-onnx-ocr（方案①）

理由：
	•	Pure Rust 零依赖：用户只需 cargo build，无需安装任何系统库。
	•	PaddleOCR 模型：天然中英文支持，对中文 PDF 场景优于 Tesseract。
	•	跨平台无忧：Mac/Linux/Windows 甚至 WASM 均可编译。
	•	API 清晰：OcrEngineBuilder → OcrEngine → run_from_image()，与 OcrBackend trait 适配。
	•	模型文件可通过 app bundle 或 include_bytes! 嵌入。

feature 名称更新：ocr_tesseract → ocr_paddle

10.6.2 OCR 完整链路

### 架构概览

```
Pipeline::parse(pdf_path)
│
├── [1] 初始化阶段（Pipeline::new）
│   ├── OcrBackend:    PaddleOcrBackend（ocr_paddle feature）
│   │                  ↳ 加载 ONNX 模型（det.onnx + rec.onnx + ppocrv5_dict.txt）
│   └── OcrRenderer:   PdfiumOcrRenderer（pdfium feature）
│                      ↳ 动态加载 libpdfium.dylib
│
├── [2] 预处理阶段
│   └── ocr_renderer.set_pdf_path(pdf_path)
│       ↳ 通知渲染器当前 PDF 文件路径
│
└── [3] 逐页处理
    ├── process_page(page_idx) → PageIR
    │   └── PageScore 评分 → text_score / is_scanned_guess
    │
    ├── should_trigger_ocr(&page_ir, &config)?
    │   ├── ocr_enabled == false → 跳过
    │   ├── ocr_mode == Disabled → 跳过
    │   ├── ocr_mode == ForceAll → 触发
    │   └── ocr_mode == Auto → text_score < scoring_text_threshold 时触发
    │
    └── OCR 执行（如果触发）
        ├── ocr_renderer.render_page_to_image(page_idx, render_width)
        │   ↳ PdfiumOcrRenderer: Pdfium 渲染页面 → RGB → PNG 字节
        ├── ocr_backend.ocr_full_page(&png_data)
        │   ↳ PaddleOcrBackend: det（文字检测）→ rec（文字识别）→ Vec<OcrBlock>
        └── run_ocr_and_update_page(&mut page_ir, ocr_backend, &img_data)
            ↳ 将 OcrBlock 写回 PageIR.blocks（带 bbox）
```

### 组件说明

| 组件                | 文件                          | Feature Gate | 说明                                                             |
| ------------------- | ----------------------------- | ------------ | ---------------------------------------------------------------- |
| `OcrBackend` trait  | `src/ocr/traits.rs`           | —            | OCR 后端抽象：`ocr_region()` / `ocr_full_page()`                 |
| `PaddleOcrBackend`  | `src/ocr/paddle.rs`           | `ocr_paddle` | Pure Rust PaddleOCR（tract-onnx 推理），Mutex 包装满足 Send+Sync |
| `MockOcrBackend`    | `src/ocr/mock.rs`             | —            | 测试用 mock，返回固定文本                                        |
| `OcrRenderer` trait | `src/render/ocr_render.rs`    | —            | 页面渲染抽象：`render_page_to_image()` / `set_pdf_path()`        |
| `PdfiumOcrRenderer` | `src/render/pdfium_render.rs` | `pdfium`     | 基于 libpdfium 的真实渲染器，Mutex 包装                          |
| `MockOcrRenderer`   | `src/render/ocr_render.rs`    | —            | 返回空字节的 mock                                                |
| `OcrTrigger`        | `src/ocr/trigger.rs`          | —            | `should_trigger_ocr()` 触发条件判断                              |
| `OcrIntegration`    | `src/ocr/integration.rs`      | —            | `run_ocr_and_update_page()` 结果回填 PageIR                      |

### 初始化流程

**OCR 后端（Pipeline::new）：**
1. 如果 `config.ocr_enabled == true` 且编译了 `ocr_paddle` feature：
   - 优先使用 `config.ocr_model_dir`
   - 若未设置，自动探测：可执行文件同级 `models/ppocrv5/` → 当前目录 `models/ppocrv5/`
   - 找到后调用 `PaddleOcrBackend::new(model_dir)` 加载 ONNX 模型
   - 初始化失败则 warn 并 fallback 到 `MockOcrBackend`
2. 若无 `ocr_paddle` feature，fallback 到 `MockOcrBackend`

**OCR 渲染器（Pipeline::new）：**
1. 如果编译了 `pdfium` feature：
   - 调用 `PdfiumOcrRenderer::new(None)` 自动搜索 `libpdfium.dylib`
   - 初始化失败则 fallback 到 `MockOcrRenderer`
2. 若无 `pdfium` feature，使用 `MockOcrRenderer`

### libpdfium 搜索路径

`PdfiumOcrRenderer` 按以下优先级搜索 `libpdfium.dylib`：

1. `PdfiumOcrRenderer::new(Some(path))` 参数指定路径
2. 可执行文件同级目录（`std::env::current_exe()` 的 parent）
3. 可执行文件上级的 `bin/` 目录
4. 当前工作目录 `./`
5. 系统库路径（`Pdfium::bind_to_system_library()`）

### 模型文件结构

```
models/ppocrv5/
├── det.onnx            # 文字检测模型（DBNet）
├── rec.onnx            # 文字识别模型（SVTR）
└── ppocrv5_dict.txt    # 字典文件（中英日韩）
```

### 配置项

```rust
Config {
    ocr_enabled: bool,           // 是否启用 OCR（默认 false）
    ocr_mode: OcrMode,           // Auto / ForceAll / Disabled
    ocr_model_dir: Option<PathBuf>, // 模型目录（None 则自动探测）
    ocr_render_width: u32,       // 渲染宽度（默认 1024px）
    ocr_workers: usize,          // OCR 并发数（同步模式天然串行）
    scoring_text_threshold: f32, // Auto 模式的触发阈值（默认 0.3）
}
```

### Feature 组合

| 场景             | Features                             | 效果                                                  |
| ---------------- | ------------------------------------ | ----------------------------------------------------- |
| 纯文本解析       | `default`                            | 无 OCR，无渲染                                        |
| OCR（mock 渲染） | `ocr_paddle`                         | 真实 OCR 引擎 + mock 渲染器（无法产出有意义结果）     |
| 完整 OCR         | `ocr_paddle,pdfium`                  | 真实 OCR + 真实渲染（需 libpdfium.dylib + ONNX 模型） |
| 所有功能         | `ocr_paddle,pdfium,store_sled,async` | 完整功能集                                            |

10.7 Pipeline 并发与资源控制 Checklist
	•	有界队列（mpsc）与 backpressure
	•	分阶段 worker：
	•	extract worker（少并发）
	•	ocr worker（可并发但限流）
	•	max_memory_mb enforcement（至少做到缓存/队列受控）
	•	超时与取消（async 模式）

10.8 Store / 断点续传 Checklist
	•	Store trait
	•	SledStore 实现（可选）
	•	记录状态：
	•	page done
	•	page diagnostics
	•	ir blob（可选压缩）
	•	恢复策略：从最后完成页继续

10.9 测试与验收 Checklist
	•	单元测试：
	•	PageScore
	•	多列复排
	•	stream 表格聚类
	•	集成测试：
	•	10 份 born-digital
	•	5 份扫描件（若 OCR 开启）
	•	基准测试（criterion）：
	•	100 页 born-digital
	•	20 页扫描件（OCR）
	•	验收脚本：
	•	输出 IR JSON
	•	输出扁平化索引文本（表格行/cell）
	•	抽查 30 个“表格查数/求和/增长率”问答的检索命中

⸻

11. 里程碑（建议）
	•	M1：IR + Fast Track 基础文本 + Markdown/RAG 导出（可用）
	•	M2：PageScore + 页眉页脚 + 多列复排稳健化
	•	M3：表格 stream 抽取 + fallback_text（RAG 表格可问）
	•	M4：表格 ruled 抽取（如果后端支持线段/图形）
	•	M5：OCR fallback（可选）+ Store 断点续传
	•	M6：性能/内存调优 + 样本集评测与回归

⸻
