# M11：表格识别增强 — 无框表格 & 模型辅助

## 目标

增强 knot-pdf 的表格识别能力，参考 MinerU 使用 TableMaster/RapidTable 的思路，为 knot-pdf 增加：

1. **无框表格检测增强**：基于文本对齐的无框表格检测逻辑，覆盖 MinerU 支持但 knot-pdf 当前薄弱的场景
2. **可选模型辅助**：集成轻量级表格结构识别 ONNX 模型，提升复杂表格的行列映射准确率
3. **表格后处理增强**：IoU 重叠消歧、跨列合并单元格检测

### 解决的问题

当前 knot-pdf 的表格处理存在以下不足：
- 无框表格（stream table）主要依赖文本间距聚类，对不规则间距的表格容易出错
- 无法检测合并单元格（colspan/rowspan）
- 缺少表格区域与非表格区域的置信度判断
- booktabs 表格在跨栏场景下的干扰问题（已在 M4 后修复部分）

### 参考

- MinerU 依赖：[RapidTable](https://github.com/RapidAI/RapidTable)
- MinerU 依赖：[TableStructureRec](https://github.com/RapidAI/TableStructureRec)

## 依赖

- M3（stream 表格抽取）
- M4（ruled 表格抽取）
- M10（版面检测 — 表格区域定位，可选）

## 交付物

- [x] 无框表格检测增强（纯规则，不依赖模型）
- [x] 合并单元格检测
- [x] 表格置信度评估
- [x] 可选：轻量级表格结构识别模型集成 (YOLO ONNX)
- [x] IoU 重叠消歧
- [x] 单元测试（19 个 enhance + 10 structure_detect + 2 onnx_structure = 31）

---

## Checklist

### 1. 无框表格检测增强（纯规则）

当前的 stream 表格检测基于行间距聚类 + 列间距聚类，需增强以下逻辑：

- [x] **列对齐一致性检测** (`evaluate_column_alignment`)：
  - 统计每行文本块的 X 起点分布
  - 如果 3+ 行的 X 起点有 3+ 个稳定聚类中心 → 高置信度表格
  - 利用标准差判断对齐质量
- [x] **行间距均匀性检测** (`evaluate_row_spacing`)：
  - 连续行的 Y 间距标准差 / 平均间距 < 0.3 → 表格候选
  - 与正文段落间距做区分
- [x] **数字/数据密度检测** (`evaluate_data_density`)：
  - 区域内数字字符比例 > 30% → 更可能是表格
  - 包含 %、$、¥、#、小数点等标记 → 数据表格权重加分
- [x] **表头检测增强** (`detect_header_row`)：
  - 分析第一行/前两行的字体粗细/大小与后续行的差异
  - 首行字体 > 1.2x 后续行 → 表头 (策略 1)
  - 首行加粗 > 50% 且后续行 < 20% → 表头 (策略 2)
  - 支持双行表头检测 (策略 3)

### 2. 合并单元格检测

- [x] **Colspan（跨列）检测**：
  - 某行中某个 cell 的宽度覆盖了相邻 2+ 列的范围 → 标记为 colspan
  - 在 `TableCell` 中新增 `colspan: u32`（默认 1）— 已有
- [x] **Rowspan（跨行）检测**：
  - 某列中相邻行有相同非空文本 → 标记为 rowspan
  - 在 `TableCell` 中新增 `rowspan: u32`（默认 1）— 已有
- [x] **空单元格推断**：
  - 如果某行某列位置没有文本 → 插入空 cell
  - 检查是否被上方的 rowspan 覆盖

### 3. 表格置信度评估

新增 `TableConfidence` 评估模块：

```rust
pub struct TableConfidence {
    /// 总置信度 (0.0 ~ 1.0)
    pub score: f32,
    /// 分项置信度
    pub alignment_score: f32,   // 列对齐质量
    pub spacing_score: f32,     // 行间距均匀性
    pub data_density_score: f32, // 数据密度
    pub model_score: Option<f32>, // 模型置信度（如有）
}
```

- [x] 实现 `evaluate_table_confidence()` 函数
- [x] 在 `TableIR` 中新增 `confidence: Option<TableConfidence>`
- [x] 低置信度表格（< 0.3）标记 `diagnostics` 告警 (`flag_low_confidence_tables`)
- [x] 综合增强入口 (`enhance_tables`) — 一键执行全部增强流程

### 4. 可选：轻量级表格结构识别模型

类似 M10 的版面检测，集成一个轻量级的表格结构识别 ONNX 模型：

- [x] 模型选型：
  - 主选：Table Transformer (DETR) — `microsoft/table-transformer-structure-recognition`
  - 候选：YOLO 系列表格检测模型
  - 支持 5 类检测: row / column / column header / projected row header / spanning cell
- [x] Feature gate: `table_model`（`tract-onnx` + `image`，与 `layout_model` 共享依赖）
- [x] 输入：表格区域的渲染图片 (PNG → NCHW float32 归一化)
- [x] 输出：行列分割线的坐标（从 row/column bbox 推导）
- [x] 与规则方法的融合 (`merge_grid_lines`)：
  - 模型输出的行列线 + 规则检测的行列线 → 距离 < tolerance 去重，保留高置信度
  - 模型置信度低时回退到纯规则（`min_model_confidence` 参数控制）
- [x] `TableStructureDetector` trait + `MockTableStructureDetector`
- [x] `OnnxTableStructureDetector` — 支持 DETR 和 YOLO 两种架构，自动推断 num_classes
- [x] Config 配置项：`table_model_enabled` / `table_model_path` / `table_confidence_threshold` / `table_input_size`
- [x] 模型加载+推理验证通过（YOLOv8n ONNX, 12.2MB）
- [ ] 单页表格模型推理 < 100ms (CPU) — 当前 208ms (release, 80 类 COCO 通用模型)，换用 5 类专用模型可达标

### 5. IoU 重叠消歧

参考 MinerU 的后处理逻辑：

- [x] **表格区域重叠检测** (`deduplicate_tables_by_iou`)：
  - 如果两个表格候选区域 IoU > 0.5 → 合并为一个
  - 保留 cell 数更多的那个
- [x] **表格与文本块重叠处理** (`is_block_inside_table`)：
  - 如果文本块 80%+ 面积在表格 bbox 内 → 归入表格
  - 如果部分重叠 → 根据重叠面积比决定

### 6. 测试

- [x] 单元测试（31 个）：
  - [x] 列对齐一致性检测算法
  - [x] 行间距均匀性评估（均匀/不均匀）
  - [x] 数据密度评估（数字/文本）
  - [x] 合并单元格检测（colspan/rowspan）
  - [x] 置信度评估（结构化 + serde）
  - [x] IoU 重叠消歧
  - [x] 表格内外判断
  - [x] 值聚类算法
  - [x] 表头检测（加粗/字体大小/无表头/双行表头）
  - [x] 低置信度告警
  - [x] 分割线融合算法
  - [x] 模型类别映射
- [ ] 评测：
  - [ ] 使用现有 eval_samples 中的 20 个 stream 表格重新评测
  - [ ] 新增 10 个含合并单元格的表格样本
  - [ ] 对比增强前后的列映射正确率
  - [ ] 对比增强前后的行检测正确率

---

## 完成标准

- [ ] 无框表格检测准确率提升 > 10%（对比 M3 基线，使用扩展样本集）
- [x] 合并单元格检测可用（colspan/rowspan 字段输出正确）
- [x] 每个 TableIR 含置信度评估（`confidence` 字段）
- [x] 不破坏现有 ruled 表格的正确率（回归测试通过）
- [x] 可选模型功能在 feature 未开启时零影响
- [x] 全部测试通过（66 lib + 22 m3 + 13 ir + 6 集成 + 10 m10 + 3 m11_model）

## 实现文件清单

| 文件                            | 类型              | 说明                                                                                          |
| ------------------------------- | ----------------- | --------------------------------------------------------------------------------------------- |
| `src/table/enhance.rs`          | **新增** ~1200 行 | TableConfidence、合并单元格、IoU 消歧、表头检测、低置信度告警、enhance_tables 入口、19 个测试 |
| `src/table/structure_detect.rs` | **新增** ~370 行  | TableStructureDetector trait、数据结构、分割线融合、10 个测试                                 |
| `src/table/onnx_structure.rs`   | **新增** ~340 行  | OnnxTableStructureDetector (DETR + YOLO)、2 个测试                                            |
| `src/table/mod.rs`              | 修改              | 注册 enhance/structure_detect/onnx_structure 模块                                             |
| `src/ir/table.rs`               | 修改              | TableIR 新增 `confidence` 字段                                                                |
| `src/config.rs`                 | 修改              | 新增 4 个表格模型配置项                                                                       |
| `Cargo.toml`                    | 修改              | 新增 `table_model` feature gate                                                               |
| `src/table/ruled.rs`            | 修改              | TableIR 构造器添加 confidence: None                                                           |
| `src/table/stream.rs`           | 修改              | TableIR 构造器添加 confidence: None                                                           |

