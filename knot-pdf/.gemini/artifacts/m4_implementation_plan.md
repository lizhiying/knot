# M4：表格 Ruled 抽取 — 实现计划

## 概述

M4 的核心目标是实现**有线表格（ruled table）**的结构化抽取。通过解析 PDF 内容流中的线段/矩形图形操作符（`m`/`l`/`re`/`S`/`s`/`f` 等），构建网格结构，将文本投影到单元格中，输出 `TableIR`。

## 实现步骤

### Step 1: 扩展后端 — 线段/矩形提取

**文件变更：**
- `src/backend/traits.rs` — 新增 `RawLine`、`RawRect` 数据结构，扩展 `PdfBackend` trait
- `src/backend/pdf_rs.rs` — 实现基于 lopdf 内容流解析的线段/矩形提取

**具体内容：**
1. 定义数据结构：
   - `Point { x: f32, y: f32 }`
   - `LineOrientation { Horizontal, Vertical, Diagonal }`
   - `RawLine { start: Point, end: Point, width: f32, orientation: LineOrientation }`
   - `RawRect { bbox: BBox, width: f32 }`
2. 在 `PdfBackend` trait 中新增：
   - `fn extract_lines(&self, page_index: usize) -> Result<Vec<RawLine>, PdfError>`
   - `fn extract_rects(&self, page_index: usize) -> Result<Vec<RawRect>, PdfError>`
3. 在 `PdfExtractBackend` 中实现：解析 lopdf 的内容流（Content Stream），提取 `m`、`l`、`re`、`S`/`s`/`f` 等图形操作符

### Step 2: 实现 Ruled 表格抽取引擎

**文件变更：**
- `src/table/ruled.rs`（新文件）— 核心 ruled 引擎

**具体内容：**
1. **线段预处理**：
   - 过滤噪声线段（过短 < 5pt / 过细 < 0.1pt）
   - 将矩形转换为线段（宽/高比大的矩形视为横线或竖线）
   - 合并近似共线线段（相同方向、坐标容差内）
   - 分类：水平线 / 垂直线
   - 坐标对齐（snap to grid，容差 2pt）

2. **网格生成**：
   - 水平线 + 垂直线 → 计算交叉点
   - 从交叉点提取行列边界
   - 生成 cell bbox 矩阵

3. **合并单元格检测**：
   - 检测缺失的水平分隔线 → rowspan
   - 检测缺失的垂直分隔线 → colspan

4. **文本投影到 Cell**：
   - 按 bbox 重叠面积将字符投影到对应 cell
   - cell 内文本按 y → x 排序
   - 空 cell 标记为空字符串

5. **表头推断**（ruled 特化）：
   - 首行下方有较粗分隔线 → 表头
   - 首行加粗/字号更大 → 表头
   - 复用 stream 引擎的非数字检测逻辑

6. **输出 TableIR**：
   - `extraction_mode = Ruled`
   - headers / rows / cells（含 rowspan/colspan）
   - column_types（复用 cell_type.rs）
   - fallback_text（必须）

### Step 3: 候选检测增强 + Ruled/Stream 自动切换

**文件变更：**
- `src/table/candidate.rs` — 增加线段密度检测
- `src/table/mod.rs` — 增加 ruled 引擎调用和降级逻辑

**具体内容：**
1. `detect_table_candidates()` 增加参数接收 `RawLine` 和 `RawRect`
2. 根据候选区域内的线段密度判断 ruled / stream
3. ruled 抽取失败 → 降级到 stream → 再失败 → fallback_text
4. `extraction_mode` 正确标记

### Step 4: Pipeline 集成

**文件变更：**
- `src/pipeline/mod.rs` — 在 `process_page` 中调用线段提取并传递给表格模块

### Step 5: 测试

**文件变更：**
- `tests/m4_tests.rs`（新文件）

**具体内容：**
1. 线段归一化单元测试
2. 网格生成单元测试
3. 合并单元格检测测试
4. 文本投影到 cell 测试
5. ruled vs stream 自动切换逻辑测试
6. 降级链路测试

## 执行顺序

```
Step 1 → Step 2 → Step 3 → Step 4 → Step 5
```

每步完成后进行 `cargo check` 确保编译通过。
