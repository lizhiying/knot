Milestone: milestone12
Iteration: iteration3 - Pipeline 复用优化与端到端验证

Goal:
1. 通过在 PdfParser 中持有 Pipeline 实例（使用 Mutex 包裹解决 Sync 约束），避免每次解析 PDF 都重新初始化。
2. 端到端验证：用实际 PDF 文件测试完整的 knot-pdf → pageindex-rs → PageNode 链路。

Assumptions:
1. ✅ Pipeline 的所有内部 trait 添加 Send bound 后满足 Send，用 std::sync::Mutex 包裹可满足 Sync。
2. ✅ 测试用 PDF 文件（Attention_Is_All_You_Need.pdf）已存在于 knot-pdf/tests/fixtures/ 中。

Scope:
- ✅ 给 OcrRenderer trait 添加 Send bound（与其他 trait 保持一致）。
- ✅ 修改 PdfParser 持有 Mutex<Pipeline>，复用 Pipeline 实例。
- ✅ 添加端到端集成测试：PDF → PageNode 树验证。
- ✅ 验证编译 + 测试。

Tasks:
- [x] 1. 给 OcrRenderer trait 添加 `Send` bound（knot-pdf/src/render/ocr_render.rs）。
- [x] 2. 修改 PdfParser：用 `Mutex<Pipeline>` 持有 Pipeline 实例，提供 `with_config()` 工厂方法。
- [x] 3. 添加端到端集成测试（pageindex-rs/tests/pdf_integration_test.rs）。
- [x] 4. 编译验证 + 全部测试通过（7 单元 + 2 集成 = 9/9）。

Exit criteria:
1. ✅ Pipeline 在 PdfParser 中被复用，不再每次重新创建。
2. ✅ 端到端测试通过：Attention_Is_All_You_Need.pdf（15 页）→ 31 节点语义树 → 42,722 字符。
3. ✅ 全部现有测试不破坏。

## 端到端测试结果

| 指标         | 数值                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| 文件         | Attention_Is_All_You_Need.pdf                                          |
| 总页数       | 15                                                                     |
| 解析耗时     | 7.20s (debug 模式)                                                     |
| 内容长度     | 42,722 字符                                                            |
| 节点数       | 31                                                                     |
| 子节点数     | 30                                                                     |
| 关键章节识别 | ✅ Abstract, Introduction, Background, Model Architecture, Attention 等 |

## 技术决策记录

### 为什么给 OcrRenderer 添加 Send bound？

Pipeline 结构体包含 `Box<dyn OcrRenderer>`。为了让 `Mutex<Pipeline>` 满足 `Sync`（从而让 PdfParser 满足 `DocumentParser: Send + Sync`），
`Pipeline` 必须满足 `Send`。但 `Box<dyn OcrRenderer>` 在 `OcrRenderer` 没有 `Send` bound 时不是 `Send`。

所有其他 Pipeline 内部的 trait 都已有 `Send + Sync` bound：
- `OcrBackend: Send + Sync` ✅
- `Store: Send + Sync` ✅
- `LayoutDetector: Send + Sync` ✅
- `VisionDescriber: Send + Sync` ✅
- `FormulaRecognizer: Send + Sync` ✅
- `OcrRenderer` 原来没有 ❌ → 现在添加了 `Send` ✅

唯一的实现 `PdfiumOcrRenderer` 已经有 `unsafe impl Send`，所以这是安全的改动。
