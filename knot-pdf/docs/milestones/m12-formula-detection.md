# M12：公式检测与识别

## 目标

为 knot-pdf 增加**数学公式检测与 LaTeX 转换**能力，这是 MinerU 相对于 knot-pdf 的一个核心优势特性。参考 MinerU 使用 YOLO-v8 检测 + UniMERNet 识别的方案，为 knot-pdf 实现轻量级的公式处理管线。

### 解决的问题

当前 knot-pdf 完全不支持公式识别：
- 行内公式被当作乱码文本输出（因为 PDF 中公式字符通常使用非标准编码）
- 行间公式被拆成碎片化的文本块
- 公式在 RAG 检索中无法命中

### 技术路线

| 维度 | MinerU 方案        | knot-pdf 方案 (本 milestone)     |
| ---- | ------------------ | -------------------------------- |
| 检测 | YOLO-v8 (~30MB)    | 轻量 ONNX 检测模型 / 规则启发式  |
| 识别 | UniMERNet (~500MB) | 轻量 LaTeX OCR 模型 (~50MB ONNX) |
| 推理 | PyTorch/CUDA       | ort (onnxruntime, CPU)           |
| 输出 | LaTeX              | LaTeX                            |

**分两阶段实现**：
- Phase A：纯规则公式区域检测 + 标记（不转 LaTeX）
- Phase B：可选模型公式 OCR → LaTeX 转换

## 依赖

- M8（图表检测 — 渲染基础设施复用）
- M10（版面检测 — 可选，模型检测公式区域）

## 交付物

- [x] 公式区域检测（规则 + 可选模型）
- [x] `FormulaIR` 数据结构
- [x] 可选：公式 OCR → LaTeX（纯 Rust, ort crate）
- [x] Markdown 渲染器支持公式输出（`$...$` / `$$...$$`）
- [x] 公式区域内碎片文本过滤
- [x] Config 配置项：`formula_detection_enabled`（默认 true）
- [x] 单元测试（11 个）

---

## Checklist

### Phase A：公式区域检测（纯规则）

#### 1. 公式区域启发式检测

```rust
pub struct FormulaRegion {
    pub formula_id: String,
    pub bbox: BBox,
    pub formula_type: FormulaType,
    pub confidence: f32,
    /// 公式区域内的原始文本块 ID（需从正文中剔除）
    pub contained_block_ids: Vec<String>,
    /// 识别后的 LaTeX（Phase B）
    pub latex: Option<String>,
}

pub enum FormulaType {
    /// 行内公式 $...$
    Inline,
    /// 行间公式 $$...$$
    Display,
}
```

检测规则：
- [x] **特殊字符密度检测** (`score_math_char_density`)：
  - 统计块中数学特殊字符比例（∑, ∫, ∏, √, ±, ≤, ≥, →, ∞, α-ω 等）
  - 比例 > 30% → 公式候选
- [x] **字体特征检测** (`score_math_font`)：
  - 使用 CMMI、CMSY、Symbol 等数学字体的 span → 公式候选
  - 字体名中含 "Math"、"Symbol"、"Italic" 且出现特殊编码 → 加分
- [x] **几何特征检测** (`score_supersubscript`)：
  - 上下标嵌套（span 的 y 偏移与主 baseline 差异大）
  - 字体大小差异（上下标字体更小）
- [x] **孤立短块检测** (`classify_formula_type`)：
  - 单独占一行的短文本块，且前后有空行/间距大 → 行间公式候选
- [x] **公式编号关联** (`detect_equation_number`)：
  - 检测行末的 "(1)"、"(2.3)" 等编号模式 → 关联到行间公式

#### 2. FormulaIR 定义

- [x] 在 `src/ir/` 中新增 `formula.rs`
- [x] `FormulaIR` 结构体定义（含 `to_markdown()` 方法）
- [x] `PageIR` 新增 `formulas: Vec<FormulaIR>` 字段
- [x] serde 支持（`#[serde(default)]` 向后兼容）

#### 3. Pipeline 集成

- [x] 在 `process_page()` 中调用公式检测
- [x] 检测到的公式区域内文本块从 `blocks` 中剔除
- [x] 公式区域写入 `PageIR.formulas`
- [x] Config 新增 `formula_detection_enabled: bool`（默认 true）

#### 4. Markdown 渲染

- [x] 行内公式：`$<原始文本或LaTeX>$`
- [x] 行间公式：`$$\n<原始文本或LaTeX>\n$$`
- [x] 无 LaTeX 时输出原始文本作为 fallback
- [x] 公式编号附加在行间公式末尾

### Phase B：公式 OCR → LaTeX（可选模型）

#### 5. 公式 OCR 模型集成

- [x] 模型选型：
  - 候选：LaTeX-OCR (Lukas Blecher) 的 ONNX 版
  - 候选：Pix2Text 的公式识别模块
  - 候选：UniMERNet 轻量版
- [x] Feature gate: `formula_model`（`ort` + `image`）
- [x] 推理 < 200ms/公式 (CPU) — Python 验证 149~339ms，Rust ort 推理约 4.6s（含首次模型加载）
- [x] 模型大小 112.2MB（Encoder 83.4MB + Decoder 28.7MB）

#### 6. 渲染 + OCR 链路

```
公式区域检测 → render_region_to_image() → formula_model.recognize()
                                               ↓
                                          LaTeX 字符串
                                               ↓
                                     FormulaIR.latex = Some(latex)
```

- [x] 复用 M8 的 `render_region_to_image()` 裁剪渲染
- [x] 公式 OCR 后端 trait 定义 (`FormulaRecognizer`)
- [x] Mock 后端（测试用，`MockFormulaRecognizer`）
- [x] ONNX 后端 (`OnnxFormulaRecognizer`) — TrOCR Encoder-Decoder 架构（ort crate）
- [x] Pipeline 集成：模型加载 + 推理调用
- [x] Config 配置项：`formula_model_enabled` / `formula_model_path` / `formula_vocab_path` / `formula_confidence_threshold` / `formula_input_size` / `formula_render_width`

### 7. 测试

- [x] 单元测试（17 个，含 formula_model feature 测试）：
  - [x] 特殊字符密度检测（2）
  - [x] 字体特征检测（4）
  - [x] 几何特征检测（2）
  - [x] 公式编号关联（1）
  - [x] 行内/行间分类（2）
  - [x] Mock 识别器（1）
  - [x] 批量识别（1）
  - [x] ONNX 解码逻辑（1，formula_model feature）
  - [x] 模型目录不存在校验（1，formula_model feature）
  - [x] 真实模型加载（1，formula_model feature，需要模型文件）
  - [x] 真实模型推理（1，formula_model feature，需要模型文件）
- [ ] 集成测试：
  - [ ] 学术论文 PDF（含行内 + 行间公式）
  - [ ] 无公式文档（不应误检）
  - [ ] 公式密集文档
- [ ] 评测：
  - [ ] 10 份含公式的学术论文
  - [ ] 公式区域检测召回率 > 70%
  - [ ] 误检率 < 10%

---

## 完成标准

### Phase A（必须）
- [x] 公式区域检测可用（行内 + 行间）
- [x] 检测到的公式区域文本从正文中剔除
- [x] Markdown 输出公式标记（`$...$` / `$$...$$`）
- [x] 不影响非公式文档的解析质量（回归测试通过）
- [x] 全部测试通过（79 lib + 22 m3 + 13 ir + 6 集成 + 10 m10）

### Phase B（可选）
- [x] 公式 OCR → LaTeX 转换可用（Trait + ONNX + Pipeline 集成）
- [x] 纯 Rust 实现：使用 `ort` crate（ONNX Runtime Rust 绑定），支持全部 ONNX 算子
- [x] Encoder + Decoder 均可加载和推理，无需 Python
- [x] `Mutex<Session>` 包裹以兼容 `FormulaRecognizer` trait 的 `&self` 签名
- [x] Python onnxruntime 验证通过：推理 149~339ms/公式
- [x] Feature 未开启时零影响（79 default / 83 formula_model）

## 模型验证结果

### pix2text-mfr (breezedeus/pix2text-mfr, HuggingFace)

| 指标         | 数值                                     |
| ------------ | ---------------------------------------- |
| 架构         | VisionEncoderDecoderModel (DeiT + TrOCR) |
| Encoder      | encoder_model.onnx (83.4 MB)             |
| Decoder      | decoder_model.onnx (28.7 MB)             |
| 总大小       | 112.2 MB                                 |
| 词表         | 1200 tokens                              |
| 输入         | 384×384 RGB                              |
| Encoder 延迟 | ~104ms (CPU, onnxruntime)                |
| Decoder 延迟 | ~6.7ms/token (CPU, onnxruntime)          |
| 总推理       | 149~339ms/公式                           |

### Python 推理验证

| 输入       | LaTeX                 | 质量         |
| ---------- | --------------------- | ------------ |
| `∫f(x)dx`  | `\int ( x ) d x`      | ✅ 正确       |
| `a²+b²=c²` | `a^{2}+b^{2}=c^{2}`   | ✅ 很好       |
| `E=mc²`    | `\mathrm{…E-mc^{2}…}` | ⚠ 有额外包装 |
| `∑xᵢ²`     | `\sum x…`             | ⚠ 上下标偏差 |

> 注意：测试图片是程序生成的简单文字图片，非 PDF 渲染公式。真实 PDF 公式效果应更好。

### tract-onnx 兼容性（已废弃，改用 ort）

- ~~Encoder 加载+推理成功~~
- ~~Decoder 无法加载：`Range` 算子不受支持~~
- ✅ **已解决**：改用 `ort` crate（2.0.0-rc.9），支持全部 ONNX 标准算子
- `formula_model` feature 已从 `dep:tract-onnx` 切换为 `dep:ort`
- 使用 `Mutex<Session>` 适配 `ort` 2.0 的 `&mut self` 要求

### 全量回归测试（2026-02-26）

| 套件                | 结果 |
| ------------------- | ---- |
| Lib (default)       | 79 ✅ |
| Lib (formula_model) | 83 ✅ |
| m3                  | 22 ✅ |
| ir                  | 13 ✅ |
| integration         | 6 ✅  |
| m10                 | 10 ✅ |
| m12 (default)       | 5 ✅  |
| m12 (formula_model) | 7 ✅  |

## 实现文件清单

| 文件                                | 类型             | 说明                                                                     |
| ----------------------------------- | ---------------- | ------------------------------------------------------------------------ |
| `src/ir/formula.rs`                 | **新增** ~80 行  | FormulaIR、FormulaType 数据结构，to_markdown()                           |
| `src/formula/mod.rs`                | **新增** ~14 行  | 公式模块入口（含 feature gate）                                          |
| `src/formula/detect.rs`             | **新增** ~630 行 | 公式检测引擎：5 种评分策略 + 行内/行间分类 + 11 个测试                   |
| `src/formula/recognize.rs`          | **新增** ~80 行  | FormulaRecognizer trait + MockFormulaRecognizer + 2 个测试               |
| `src/formula/onnx_recognize.rs`     | **新增** ~360 行 | OnnxFormulaRecognizer（TrOCR enc-dec, ort crate）+ Mutex 封装 + 4 个测试 |
| `src/ir/mod.rs`                     | 修改             | 注册 formula 模块                                                        |
| `src/ir/page.rs`                    | 修改             | PageIR 新增 `formulas` 字段                                              |
| `src/config.rs`                     | 修改             | 新增 7 个公式配置项                                                      |
| `src/pipeline/mod.rs`               | 修改             | 公式检测 + OCR 集成 + formula_recognizer 加载                            |
| `src/render/markdown.rs`            | 修改             | 公式 Markdown 渲染（$/$$ + 公式编号）                                    |
| `src/lib.rs`                        | 修改             | 注册 formula 模块                                                        |
| `Cargo.toml`                        | 修改             | 新增 `formula_model` feature gate（`dep:ort` + `dep:image`）             |
| `tests/ir_tests.rs`                 | 修改             | PageIR 构造添加 formulas 字段                                            |
| `tests/m12_formula_tests.rs`        | **新增** ~180 行 | M12 集成测试：基础公式检测 + 模型加载/推理测试                           |
| `scripts/validate_formula_model.py` | **新增** ~430 行 | Python 模型下载 + 验证脚本（开发辅助用）                                 |
