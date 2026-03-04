# 版面检测模型使用指南

## 快速开始

### 1. 启用 feature

```bash
# 编译时启用 layout_model feature
cargo build --features layout_model
```

### 2. 下载模型

推荐使用 DocLayout-YOLO 的 ONNX 模型（基于 DocStructBench 训练）：

```bash
# DocLayout-YOLO DocStructBench (75MB, 高精度)
wget https://huggingface.co/wybxc/DocLayout-YOLO-DocStructBench-onnx/resolve/main/doclayout_yolo_docstructbench_imgsz1024.onnx \
  -O doclayout_yolo.onnx
```

或者使用更小的 YOLOv11 nano 版本（需自行从 .pt 转 ONNX）：

```bash
# 使用 ultralytics 转换 (需要 Python 环境)
pip install ultralytics
yolo export model=yolo11n.pt format=onnx imgsz=640
```

### 3. 配置

通过 `knot-pdf.toml` 配置文件：

```toml
# 启用版面检测
layout_model_enabled = true

# 模型文件路径（可选，默认自动搜索 doclayout_yolo.onnx / layout_model.onnx）
layout_model_path = "/path/to/doclayout_yolo.onnx"

# 检测置信度阈值（默认 0.5）
layout_confidence_threshold = 0.5

# 模型输入分辨率（默认 640）
# 1024 用于 DocLayout-YOLO DocStructBench
layout_input_size = 1024
```

或通过代码配置：

```rust
use knot_pdf::{Config, parse_pdf};
use std::path::PathBuf;

let mut config = Config::default();
config.layout_model_enabled = true;
config.layout_model_path = Some(PathBuf::from("doclayout_yolo.onnx"));
config.layout_input_size = 1024; // 匹配模型训练分辨率
config.layout_confidence_threshold = 0.5;

let doc = parse_pdf("document.pdf", &config)?;
```

## 模型格式要求

### 输入

- 格式: `(1, 3, H, W)` — NCHW float32 RGB 图片
- 像素值: 归一化到 `[0, 1]`
- 分辨率: 取决于模型（640 或 1024）

### 输出

标准 YOLO 格式：
- 形状: `(1, C, N)` 或 `(1, N, C)`
- 其中 `C = 4 + num_classes`
- 前 4 维: `[cx, cy, w, h]` — 中心坐标 + 宽高，相对于输入尺寸
- 后 `num_classes` 维: 各类别置信度分数

### 支持的类别体系

#### DocStructBench (默认, 11 类)

| ID  | 类别            | 说明                |
| --- | --------------- | ------------------- |
| 0   | Title           | 文档标题            |
| 1   | Text            | 正文/段落           |
| 2   | Abandon         | 水印/页码等（忽略） |
| 3   | Figure          | 图片                |
| 4   | Figure_caption  | 图注                |
| 5   | Table           | 表格                |
| 6   | Table_caption   | 表注                |
| 7   | Table_footnote  | 表格脚注            |
| 8   | Isolate_Formula | 公式                |
| 9   | Formula_Caption | 公式说明            |
| 10  | (reserved)      | 保留                |

#### DocLayNet (11 类)

| ID  | 类别           | 说明     |
| --- | -------------- | -------- |
| 0   | Caption        | 说明文字 |
| 1   | Footnote       | 脚注     |
| 2   | Formula        | 公式     |
| 3   | List-item      | 列表项   |
| 4   | Page-footer    | 页脚     |
| 5   | Page-header    | 页眉     |
| 6   | Picture        | 图片     |
| 7   | Section-header | 章节标题 |
| 8   | Table          | 表格     |
| 9   | Text           | 正文     |
| 10  | Title          | 标题     |

## 工作原理

```
PDF 页面
    ↓
backend.render_page_to_image()  →  页面图片 (PNG)
    ↓
OnnxLayoutDetector.preprocess()  →  归一化 float32 tensor
    ↓
tract-onnx 模型推理  →  原始检测结果
    ↓
OnnxLayoutDetector.postprocess()  →  坐标还原 + 置信度过滤
    ↓
nms()  →  非极大值抑制，去除重叠
    ↓
merge_layout_with_blocks()  →  与规则检测结果融合
    ↓
增强的 BlockIR.role 分类
```

## 性能特征

- **推理引擎**: tract-onnx (Pure Rust, 无 C/C++ 依赖)
- **运行环境**: CPU 即可（无需 GPU）
- **预计速度**: 单页 50-200ms (取决于模型大小和 CPU)
- **内存占用**: 模型加载时一次性分配

## 无模型回退

当 `layout_model_enabled = false`（默认）或无模型文件时：
- 使用纯规则方法检测版面（标题字体大小、位置等启发式方法）
- **零性能开销**
- 所有行为与未启用版面检测时完全一致
