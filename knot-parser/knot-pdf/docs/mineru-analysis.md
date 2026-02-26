# MinerU PDF 解析库深度分析

> 撰写时间: 2026-02-26
> 目的: 分析 MinerU 的技术细节，与 knot-pdf 做对比，提取可借鉴的思路

## 一、项目概述

**MinerU** 是由 [OpenDataLab](https://github.com/opendatalab) 团队开发的开源 PDF 内容提取工具，诞生于 [InternLM](https://github.com/InternLM/InternLM) 大模型预训练过程中，最初为了解决科学文献中的符号转换问题。它能将复杂 PDF 文档转换为机器可读格式（Markdown、JSON），特别适用于 LLM/RAG 工作流的数据预处理。

- **GitHub**: [opendatalab/MinerU](https://github.com/opendatalab/MinerU)
- **论文**: [arXiv: 2409.18839](https://arxiv.org/abs/2409.18839) — *MinerU: An Open-Source Solution for Precise Document Content Extraction*
- **许可证**: AGPL-3.0
- **安装**: `pip install mineru`

---

## 二、核心架构 — 多阶段管线

MinerU 之所以效果好，核心在于它采用了**模型驱动 + 规则精调**的多阶段处理管线，而非简单的文字提取。

### 2.1 整体流水线

```
PDF 输入
  │
  ├─ ① 文档预处理 (校验完整性、分类: 文字型 / 扫描型)
  │
  ├─ ② 版面检测 (Layout Detection) — DocLayout-YOLO / YOLO-v10 / LayoutLMv3
  │     └─ 识别: 段落、标题、表格、图片、公式、页眉页脚
  │
  ├─ ③ OCR 文字识别 — PaddleOCR (109种语言)
  │     └─ 仅扫描型/乱码 PDF 触发
  │
  ├─ ④ 结构分析 (Structure Analysis)
  │     ├─ 元素分组 & 多栏布局解析
  │     ├─ 阅读顺序排序 (基于模型 + xy-cut 算法)
  │     └─ 噪声去除 (页眉页脚、页码)
  │
  ├─ ⑤ 专项识别
  │     ├─ 表格 → HTML (TableMaster / RapidTable / StructEqTable)
  │     └─ 公式 → LaTeX (UniMERNet + YOLO-v8 公式检测)
  │
  ├─ ⑥ 后处理 (重叠区域 IoU 消歧、边界修正)
  │
  └─ ⑦ 格式输出 (Markdown / JSON / HTML / LaTeX)
```

### 2.2 三种 Backend 模式

| Backend        | 特点                             | 适用场景                       |
| -------------- | -------------------------------- | ------------------------------ |
| **`pipeline`** | 经典多模型级联, 每个任务独立模型 | 通用场景, 可在 CPU 运行        |
| **`vlm`**      | 使用视觉语言模型(VLM)统一处理    | 复杂文档, 需要 GPU             |
| **`hybrid`**   | 融合 pipeline + VLM 的优势       | **推荐模式**, 减少幻觉, 更准确 |

**Hybrid Backend 的优势** (最新推荐):
- 文字型 PDF 直接提取文本（不依赖 OCR），速度快
- 支持 109 种语言的 OCR（用于扫描型文档）
- 大幅减少模型解析幻觉
- 密集公式、不规则排版等复杂场景显著提升

---

## 三、关键技术细节

### 3.1 版面检测 — DocLayout-YOLO

这是 MinerU 效果好的**第一个关键因素**。

- 基于 **YOLO-v10** 改进，针对文档版面分析优化
- 使用**多样化预训练数据**增强泛化能力（学术论文、法律文档、商业报告等）
- 架构改进：增强了对**不同尺度实例**的感知能力（小标题 vs 整页表格）
- 可检测的类别：段落、标题、表格、图片、公式、列表、页眉/页脚、脚注等

### 3.2 MinerU 2.5 — 两阶段解耦式解析 (Coarse-to-Fine)

这是 MinerU 的**最新核心创新**，一个 **1.2B 参数**的视觉语言模型：

```
┌──────────────────────────────────────────────────┐
│  Stage I: 粗粒度全局版面分析                        │
│  ┌────────────────────────────────────────────┐   │
│  │ 输入: 低分辨率缩略图 (1036×1036)              │   │
│  │ 模型: NaViT + Patch Merger + LLM            │   │
│  │ 输出: 全局布局结构 (区域类型 + 位置)            │   │
│  └────────────────────────────────────────────┘   │
│                      ↓                            │
│  Stage II: 细粒度高精度内容识别                      │
│  ┌────────────────────────────────────────────┐   │
│  │ 输入: 原始分辨率的裁剪区域                      │   │
│  │ 按类型分发: 文本/公式/表格 各用专属 prompt       │   │
│  │ 输出: 精确文本/LaTeX/HTML                     │   │
│  └────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────┘
```

**为什么这种设计效果好？**

1. **避免 token 冗余**：低分辨率分析全局结构，避免处理大量无关像素
2. **保留细节**：对关键区域用原始分辨率识别，公式、表格不丢精度
3. **高效推理**：支持 vLLM 加速，大文档处理速度显著提升

### 3.3 表格识别

MinerU 的表格处理链路：

1. **版面检测阶段**识别表格区域
2. **表格结构识别** — 使用 TableMaster / RapidTable / StructEqTable
3. 输出标准 **HTML 格式**（含 `<tr><td>` 结构）
4. 支持有线表、无线表、旋转表、部分边框表格

### 3.4 公式识别

1. **公式检测** — 自训练的 YOLO-v8 模型定位行内/行间公式
2. **公式识别** — [UniMERNet](https://github.com/opendatalab/UniMERNet) 转换为 LaTeX
3. 支持复杂长公式、混合语言公式
4. 提供独立开关可关闭行内公式识别（减少误检）

### 3.5 阅读顺序

- 基于 [layoutreader](https://github.com/ppaanngggg/layoutreader) + [xy-cut](https://github.com/Sanster/xy-cut) 算法
- 模型基于空间分布预测阅读顺序
- 支持单栏、多栏、复杂布局
- **已知限制**：极端复杂布局下可能排序错误（在 TODO 中标注为持续改进方向）

### 3.6 OCR 子系统

- 使用 [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) (含 PyTorch 移植版)
- 支持 **109 种语言**的文字检测与识别
- 自动检测扫描型/乱码 PDF 并启用 OCR
- 密集公式区域建议强制启用 OCR（避免公式位置偏移）

---

## 四、源码结构

```
MinerU/
├── mineru/                    # 核心 Python 包
│   ├── backend/               # 后端引擎实现
│   │   ├── pipeline/          # 经典多模型级联后端
│   │   ├── vlm/               # VLM 视觉语言模型后端
│   │   └── hybrid/            # 混合后端
│   ├── models/                # 模型定义与接口
│   ├── utils/                 # 工具函数 (bbox绘制, 引擎工具等)
│   └── ...
├── demo/                      # 示例代码
├── docker/                    # Docker 部署配置
├── docs/                      # 文档
├── projects/                  # 子项目
├── tests/                     # 测试
├── mineru.template.json       # 配置模板 (模型路径等)
├── pyproject.toml             # 项目元数据
└── mkdocs.yml                 # 文档站配置
```

---

## 五、依赖的关键外部项目

| 项目                                                                  | 用途                                     |
| --------------------------------------------------------------------- | ---------------------------------------- |
| **[PDF-Extract-Kit](https://github.com/opendatalab/PDF-Extract-Kit)** | 底层 PDF 提取工具包, MinerU 的模型基础   |
| **[DocLayout-YOLO](https://github.com/opendatalab/DocLayout-YOLO)**   | 文档版面检测                             |
| **[UniMERNet](https://github.com/opendatalab/UniMERNet)**             | 数学公式识别 → LaTeX                     |
| **[RapidTable](https://github.com/RapidAI/RapidTable)**               | 表格结构识别                             |
| **[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)**            | 多语言 OCR                               |
| **[layoutreader](https://github.com/ppaanngggg/layoutreader)**        | 阅读顺序预测                             |
| **[xy-cut](https://github.com/Sanster/xy-cut)**                       | 文档空间分割算法                         |
| **[pypdfium2](https://github.com/pypdfium2-team/pypdfium2)**          | PDF 渲染 (将 PDF 页面转为图片供模型使用) |
| **[pdfminer.six](https://github.com/pdfminer/pdfminer.six)**          | 文字型 PDF 的文本直接提取                |
| **[vLLM](https://github.com/vllm-project/vllm)**                      | VLM 推理加速引擎                         |

---

## 六、与 knot-pdf 的对比分析

| 维度         | **knot-pdf** (Rust)                          | **MinerU** (Python)                          |
| ------------ | -------------------------------------------- | -------------------------------------------- |
| **文字提取** | 直接解析 PDF 内容流 (Tj/TJ 操作符)，速度极快 | pipeline模式也直接提取；VLM 模式通过视觉识别 |
| **版面检测** | 规则启发式 (间距分析、隐式网格检测)          | 深度学习模型 (DocLayout-YOLO)，更鲁棒        |
| **多栏处理** | `detect_sparse_column_grid` 等启发式方法     | 模型直接识别多栏结构                         |
| **表格提取** | ruled table + booktabs 检测，基于线条分析    | 独立表格识别模型 (TableMaster/RapidTable)    |
| **公式识别** | 不支持                                       | UniMERNet 转 LaTeX                           |
| **阅读顺序** | 基于几何位置排序                             | 模型 + xy-cut 算法                           |
| **OCR**      | 需外部支持                                   | 内置 PaddleOCR (109种语言)                   |
| **性能**     | 毫秒级 (纯 Rust, 无模型依赖)                 | 秒级到分钟级 (依赖 GPU 推理)                 |
| **部署难度** | 极低 (单二进制)                              | 较高 (需要下载 ~GB 级模型文件)               |

### MinerU 效果好的根本原因

1. **模型驱动 vs 规则驱动**: MinerU 用深度学习模型处理版面检测、表格结构识别、公式识别等任务，比纯规则方法对"未见过的布局"泛化能力更强
2. **两阶段解耦**: 粗粒度先确定结构，细粒度再精确识别，既高效又精确
3. **专项模型组合**: 每个子任务（版面/表格/公式/OCR）都用专门的 SOTA 模型，而非一个模型解决所有问题
4. **精细的前后处理规则**: 在模型之上叠加了大量规则（重叠消歧、噪声过滤、边界修正），弥补模型的不足

---

## 七、可以借鉴的思路

对于 knot-pdf 项目，以下是一些可以从 MinerU 借鉴的方向：

### 7.1 xy-cut 阅读顺序算法

MinerU 使用的 xy-cut 算法是一个**纯几何算法**，不依赖模型，可以直接用 Rust 实现。

**核心思想**: 递归地在 X 方向和 Y 方向寻找最大空白切割线，将页面分割为阅读块，天然适用于多栏布局。

```
页面
├─ Y-cut (水平分割) → 上部(标题区) + 下部(正文区)
│   └─ X-cut (垂直分割) → 左栏 + 右栏
│       ├─ 左栏: Y-cut → 段落1, 段落2, ...
│       └─ 右栏: Y-cut → 段落3, 段落4, ...
```

这对 knot-pdf 当前的 `reading_order.rs` 模块会有直接帮助。

### 7.2 布局检测后处理 — IoU 消歧

MinerU 在模型检测后使用基于**包含关系 (containment)** 和 **IoU (Intersection over Union)** 的启发式方法来处理重叠 bounding box。这个思路可以用在 knot-pdf 的元素分组中。

### 7.3 混合模式的设计思路

MinerU hybrid backend 的核心理念非常值得借鉴：
- **文字型 PDF** → 直接提取文本（knot-pdf 已经做得很好）
- **扫描型 PDF** → 走 OCR/模型通道
- 这种分层策略可以让 knot-pdf 在保持高性能的同时，通过可选的外部模型集成来增强复杂场景的处理能力

### 7.4 表格处理的增强方向

- MinerU 支持**无线表**（通过文本对齐推断列边界）、**旋转表**、**部分边框表**
- knot-pdf 目前主要支持 ruled table 和 booktabs table
- 可以考虑增加基于文本对齐的无线表检测逻辑（纯规则方式也可以实现）

---

## 八、已知限制与待改进

MinerU 官方标注的已知问题：

- 阅读顺序在极端复杂布局下可能错误
- 竖排文本支持有限
- 目录和列表通过规则识别，不常见格式可能遗漏
- 代码块在布局模型中尚未支持
- 漫画书、艺术画册、小学教材等无法良好解析
- 复杂表格的行/列识别可能出错
- 小众语言的 OCR 精度有限（拉丁变音符号、阿拉伯文易混字符）
- 部分公式在 Markdown 中不能正确渲染
