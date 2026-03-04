# 表格结构识别模型指南

## 模型选型

### 候选模型对比

| 模型                     | 大小   | 架构     | 输出         | tract 兼容性              | 推荐度   |
| ------------------------ | ------ | -------- | ------------ | ------------------------- | -------- |
| Table Transformer (DETR) | 112 MB | DETR     | bbox + class | ❌ 不兼容 (symbolic dims)  | 不推荐   |
| Table Transformer 量化版 | 30 MB  | DETR     | bbox + class | ❌ 不兼容 (dynamic shapes) | 不推荐   |
| SLANet (PaddleOCR)       | 7.3 MB | CNN+LSTM | HTML tokens  | ⚠️ 需验证                  | 待测试   |
| YOLO 表格检测            | ~10 MB | YOLOv8   | bbox + class | ✅ 兼容                    | **推荐** |

### 推荐方案：YOLO 表格结构检测

Table Transformer 基于 DETR 架构，内部使用大量 symbolic dimensions 和复杂的
attention 机制，tract-onnx 目前无法正确加载。

推荐使用 **YOLO 系列**的表格结构检测模型，因为：
1. YOLO 输出格式简单：`(1, C, N)` 或 `(1, N, C)`
2. 无 symbolic dimensions
3. 与 M10 版面检测的 YOLO 基础设施共享
4. 推理速度快（< 50ms CPU）

### 模型训练建议

如果需要自训练表格结构检测模型：

```bash
# 使用 ultralytics YOLOv8 训练
# 标注 5 类：row, column, column_header, projected_row_header, spanning_cell
# 输入尺寸 640x640

pip install ultralytics
yolo train model=yolov8n.pt data=table_structure.yaml epochs=100 imgsz=640
yolo export model=best.pt format=onnx imgsz=640 simplify=True
```

## 使用方式

### 放置模型文件

将 ONNX 模型文件放到 `models/` 目录下：

```
knot-pdf/
  models/
    table_structure.onnx   ← 放这里
    .gitignore             ← 已配置忽略 *.onnx
```

### 配置

在 `knot-pdf.toml` 中启用：

```toml
table_model_enabled = true
table_model_path = "models/table_structure.onnx"
table_confidence_threshold = 0.5
table_input_size = 640
```

### 编译

```bash
# 启用表格模型 feature
cargo build --features table_model

# 同时启用版面检测和表格模型
cargo build --features "layout_model,table_model"
```

## 代码架构

```
src/table/
├── structure_detect.rs   # trait 定义 + 数据结构 + 融合算法
├── onnx_structure.rs     # ONNX 推理实现 (DETR + YOLO)
├── enhance.rs            # 置信度评估 + 合并单元格 + IoU 消歧
└── mod.rs                # 模块注册
```

### 核心接口

```rust
// 表格结构检测 trait
pub trait TableStructureDetector: Send + Sync {
    fn detect(&self, image_data: &[u8], table_bbox: &BBox)
        -> Result<TableStructureResult, PdfError>;
    fn name(&self) -> &str;
}

// 检测结果
pub struct TableStructureResult {
    pub row_separators: Vec<TableGridLine>,  // 行分割线
    pub col_separators: Vec<TableGridLine>,  // 列分割线
    pub elements: Vec<TableElement>,         // 原始检测元素
    pub header_bbox: Option<BBox>,           // 表头区域
    pub spanning_cells: Vec<BBox>,           // 合并单元格
}

// 融合算法
pub fn merge_grid_lines(
    rule_lines: &[TableGridLine],
    model_lines: &[TableGridLine],
    tolerance: f32,              // 距离阈值
    min_model_confidence: f32,   // 最低模型置信度
) -> Vec<TableGridLine>;
```

### 输出类别

| 类别 ID | 标签               | 说明           |
| ------- | ------------------ | -------------- |
| 0       | Row                | 行区域         |
| 1       | Column             | 列区域         |
| 2       | ColumnHeader       | 列表头         |
| 3       | ProjectedRowHeader | 行表头（投影） |
| 4       | SpanningCell       | 合并单元格     |

## 回退机制

当模型不可用或推理失败时，系统自动回退到纯规则方法：

1. `table_model` feature 未启用 → 使用 `MockTableStructureDetector`
2. 模型文件不存在 → 回退到规则方法
3. 推理失败 → 回退到规则方法
4. 模型置信度过低 → `merge_grid_lines` 中忽略低置信度结果

## tract-onnx 兼容性说明

tract-onnx 0.21 对以下 ONNX 特性支持有限：

- ❌ Symbolic dimensions (如 `batch_size`, `height`, `width`)
- ❌ 复杂的 Reshape + Gather 组合
- ❌ 部分 DETR/Transformer 架构的 attention 操作
- ✅ 标准 CNN 卷积
- ✅ YOLO 风格的简单输出格式
- ✅ 固定尺寸输入的简单模型

建议模型满足以下条件：
1. 所有维度为固定数值（无 symbolic dims）
2. 使用 `onnxsim` 简化后无报错
3. opset version ≤ 14
4. 输入 shape 固定为 `(1, 3, H, W)`
