# M10：轻量级版面检测

## 目标

引入**轻量级深度学习版面检测模型**，为 knot-pdf 提供类似 MinerU 的 DocLayout-YOLO 的版面分析能力。区别于 MinerU 使用大型 GPU 模型的方案，knot-pdf 选择可在 CPU 上高效运行的 ONNX 小模型，保持"离线优先、低资源"的定位。

### 解决的问题

当前 knot-pdf 通过纯规则方式检测版面元素（表格、页眉页脚、多栏等），在以下场景泛化能力不足：
- 非标准布局（旁注、侧边栏、浮动框）
- 复杂图表与文本的混排
- 无框表格 vs 列表的区分
- 标题层级判断（H1/H2/H3/正文）

### 技术路线

参考 MinerU 的版面检测方案，但做轻量化适配：

| 维度     | MinerU                          | knot-pdf (本 milestone)             |
| -------- | ------------------------------- | ----------------------------------- |
| 模型     | DocLayout-YOLO (~100MB+)        | ONNX 轻量目标检测 (~10-30MB)        |
| 推理引擎 | PyTorch/CUDA                    | tract-onnx (Pure Rust)              |
| 运行环境 | GPU 推荐                        | CPU 即可                            |
| 分类类别 | 段落/标题/表格/图片/公式/列表等 | 段落/标题/表格/图片/列表 (不含公式) |

## 依赖

- M8（图表检测 — Path object 基础设施）
- M9（XY-Cut — 阅读顺序）

## 交付物

- [x] 版面检测基础设施（trait、数据结构、NMS、IoU、融合逻辑）
- [x] 版面检测结果与现有规则方法融合（`merge_layout_with_blocks`）
- [x] `BlockIR.role` 分类增强（新增 Heading/PageNumber/Sidebar）
- [x] Pipeline 集成（当 `layout_model_enabled=true` 时自动调用）
- [x] Config 配置项（4 个版面检测字段）
- [x] ONNX 模型推理集成（`tract-onnx` Pure Rust 引擎）
- [x] Feature gate: `layout_model`（可选依赖，默认不编译）

---

## Checklist

### 1. 模型选型与准备

- [x] 选择轻量级版面检测模型：
  - 推荐: DocLayout-YOLO DocStructBench (ONNX, 75MB)
  - 替代: YOLOv11-nano DocLayNet (.pt→ONNX, ~6MB)
  - 替代: PP-PicoDet DocLayNet
- [x] 支持标准 YOLO ONNX 格式（自动检测 (1,C,N) 和 (1,N,C) 两种输出布局）
- [ ] 模型大小约束：< 30MB（压缩后）— 需使用 nano/small 版本
- [ ] 推理速度约束：单页 < 200ms (CPU) — 需实际测量

### 2. 推理引擎集成

新增模块 `src/layout/detect.rs` ✅：

```rust
/// 版面检测后端 trait
pub trait LayoutDetector: Send + Sync {
    fn detect(&self, image_data: &[u8], page_width: f32, page_height: f32)
        -> Result<Vec<LayoutRegion>, PdfError>;
    fn name(&self) -> &str;
}

pub struct LayoutRegion {
    pub bbox: BBox,
    pub label: LayoutLabel,
    pub confidence: f32,
}

pub enum LayoutLabel {
    Title, Heading, Paragraph, Table, Figure,
    List, Caption, Header, Footer, PageNumber,
    Formula, Unknown,
}
```

- [x] `LayoutDetector` trait 定义
- [x] `LayoutLabel` 枚举（12 个类别，覆盖 DocLayNet 标注体系）
- [x] `LayoutRegion` 数据结构（含 serde 序列化）
- [x] `LayoutLabel::from_class_id()` — 模型类别 ID 映射
- [x] `LayoutLabel::to_block_role()` — 转换为 BlockRole
- [x] `LayoutLabel::is_text_role()` — 判断文本角色
- [x] `MockLayoutDetector` — 测试用 mock
- [x] `compute_iou()` — IoU 计算
- [x] `nms()` — 非极大值抑制
- [x] `merge_layout_with_blocks()` — 模型 + 规则融合逻辑
- [x] `OnnxLayoutDetector`：基于 `tract-onnx` 的实现 ✅
  - [x] 模型加载（支持从文件路径 / bytes 加载）
  - [x] 图片预处理（resize + NCHW normalize，像素值 0~1）
  - [x] 后处理（置信度过滤 + 坐标还原 + NMS）
  - [x] 支持 DocStructBench / DocLayNet 两种类别体系
  - [x] 自动检测输出布局 (1,C,N) vs (1,N,C)
- [x] Feature gate: `layout_model`（默认不开启）✅
  - [x] `tract-onnx` + `image` 仅在 feature 启用时编译
  - [x] Pipeline 中使用 `cfg(feature)` 条件编译

### 3. 与 Pipeline 集成

```
现有流程:
  extract_chars → build_blocks → rules-based role assignment

增强流程 (layout_model_enabled=true):
  extract_chars → build_blocks
       ↓
  backend.render_page_to_image() → layout_detector.detect()
       ↓
  nms() → merge_layout_with_blocks()  ← 模型结果 + 规则结果融合
       ↓
  refined role assignment
```

- [x] Pipeline struct 新增 `layout_detector` 字段
- [x] Pipeline::new() 根据 config 初始化版面检测器
- [x] process_page() 中在 build_blocks 后可选调用版面检测
- [x] 融合策略实现：
  - [x] 文本角色 (Title/Heading/List/Caption 等) → 直接覆盖 block.role
  - [x] 非文本角色 (Table/Figure) → 不覆盖 block.role（由 TableIR / ImageIR 处理）
  - [x] 低置信度区域 → 忽略
- [x] 无模型时（`layout_model_enabled=false`）→ 回退到纯规则方法（零行为变更）✅

### 4. BlockIR.role 增强

- [x] 新增 `BlockRole::Heading` — H2/H3 级标题
- [x] 新增 `BlockRole::PageNumber` — 页码
- [x] 新增 `BlockRole::Sidebar` — 侧边栏/旁注
- [x] serde 向后兼容验证 ✅

### 5. 配置项

```rust
layout_model_enabled: bool,                    // 默认 false
layout_model_path: Option<PathBuf>,            // 默认 None
layout_confidence_threshold: f32,              // 默认 0.5
layout_input_size: u32,                        // 默认 640
```

- [x] Config 新增 4 个版面检测字段
- [x] 默认值函数（`default_layout_confidence`, `default_layout_input_size`）
- [x] JSON 序列化/反序列化验证 ✅
- [x] TOML 配置支持验证 ✅

### 6. 测试与评测

- [x] 单元测试（11 个）：
  - [x] `test_layout_label_from_class_id` — 类别 ID 映射
  - [x] `test_layout_label_to_block_role` — 角色转换
  - [x] `test_layout_region_serde` — serde 往返
  - [x] `test_compute_iou_no_overlap` / `full_overlap` / `partial_overlap` — IoU
  - [x] `test_nms_basic` / `test_nms_no_overlap` — NMS
  - [x] `test_merge_layout_with_blocks` — 融合逻辑
  - [x] `test_merge_layout_ignore_low_confidence` — 低置信度忽略
  - [x] `test_mock_detector` — Mock 检测器
- [x] 集成测试（10 个）：
  - [x] BlockRole 新增 variant 的 serde 兼容性
  - [x] 旧版 JSON 向后兼容
  - [x] LayoutLabel 完整覆盖验证
  - [x] NMS 置信度排序验证
  - [x] `layout_model_enabled=false` 行为验证
  - [x] Config JSON / TOML 序列化
  - [x] 配置默认值验证
  - [x] Mock 检测器集成
  - [x] 10 个评测样本回归测试
- [ ] 评测（需要实际模型）：
  - [ ] 使用 PubLayNet 测试集评估检测精度
  - [ ] 与纯规则方法对比 `BlockIR.role` 分类准确率
- [ ] 性能基准（需要实际模型）：
  - [ ] 单页版面检测耗时（CPU）
  - [ ] 对比有/无版面检测模型的端到端耗时

---

## 完成标准

- [x] 版面检测基础设施完备（trait、数据结构、NMS、融合、配置）
- [x] Pipeline 集成就绪（只差实际 ONNX 模型）
- [x] BlockIR.role 扩展完成，向后兼容
- [x] 无模型时（feature 未开启），零行为变更 ✅
- [x] 全部现有测试通过（40 lib + 10 集成测试）
- [ ] 版面检测模型可在 CPU 上运行，单页 < 200ms（需下载模型实测）
- [ ] 模型文件 < 30MB（需使用 nano 版本）

## 实现文件清单

| 文件                         | 类型             | 说明                                                |
| ---------------------------- | ---------------- | --------------------------------------------------- |
| `src/layout/onnx_detect.rs`  | **新增** ~290 行 | OnnxLayoutDetector: tract-onnx 推理 + 预处理/后处理 |
| `src/layout/mod.rs`          | 修改             | 注册 detect/onnx_detect 模块，条件导出              |
| `src/ir/types.rs`            | 修改             | BlockRole 新增 Heading/PageNumber/Sidebar           |
| `src/config.rs`              | 修改             | 新增 4 个版面检测配置项                             |
| `src/pipeline/mod.rs`        | 修改             | 集成版面检测到 process_page()，条件加载 ONNX        |
| `tests/m10_layout_tests.rs`  | **新增**         | 10 个集成测试                                       |
| `Cargo.toml`                 | 修改             | 添加 `layout_model` feature + `tract-onnx` 依赖     |
| `docs/layout-model-guide.md` | **新增**         | 模型使用指南                                        |

## 备注

这是一个**可选增强**里程碑。代码实现已 100% 完成：

- ✅ `LayoutDetector` trait 和完整的推理管道
- ✅ `OnnxLayoutDetector` 基于 tract-onnx (Pure Rust)
- ✅ 图片预处理（resize + normalize）和后处理（坐标还原 + NMS）
- ✅ 支持 DocStructBench 和 DocLayNet 两种类别体系
- ✅ Pipeline 自动集成（自动搜索模型文件）
- ✅ `layout_model` feature gate（默认不编译 tract-onnx）
- ✅ 使用指南文档

剩余的是**运维任务**（非代码）：
1. 下载并测试具体模型文件
2. 测量实际推理性能
3. 根据精度评估选择最优模型版本
