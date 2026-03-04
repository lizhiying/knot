# M3：表格 Stream 抽取 + fallback_text（RAG 表格可问）

## 目标

实现无框表格（stream table）的结构化抽取，包括表格候选区域检测、行列聚类、表头推断，并输出完整的 `TableIR`。同时实现强制 `fallback_text` 机制，确保即使结构化失败也能保证"信息不丢"。完成后，RAG 系统可对表格进行数值查询与计算。

## 依赖

- M1（IR 定义 + Fast Track 基础文本）
- M2（PageScore + 多列复排）

## 交付物

- 表格候选区域检测模块
- Stream 表格抽取引擎
- `fallback_text` 生成器
- RAG 扁平化导出完善（`table_row_lines` / `table_cell_lines`）

---

## Checklist

### 1. 表格候选区域检测

- [x] 基于文本对齐特征的检测算法：
  - [x] 多行 x 坐标对齐分析
  - [x] 列间距规律性检测
  - [x] 数值密度分析（数字/百分比/货币符号集中区域）
- [x] 候选区域输出：
  - [x] `bbox`：表格候选区域边界
  - [x] `confidence: f32`：置信度
  - [x] `candidate_type`：stream / ruled / unknown
- [x] 候选区域与普通文本块的分离
- [x] 边界情况处理：
  - [x] 单列列表 vs 单列表格区分
  - [x] 嵌套表格（暂不支持，标记 warning）

### 2. Stream 表格抽取

- [x] 行聚类：
  - [x] 基于 y 坐标和行距的行分组
  - [x] 跨行文本处理（单元格内换行）
  - [x] 行间距自适应阈值
- [x] 列聚类：
  - [x] 基于 x 坐标分布的列边界检测
  - [x] 左对齐 / 右对齐 / 居中对齐识别
  - [x] 列宽自适应
- [x] 表头推断：
  - [x] 第一行表头（默认策略）
  - [x] 多行表头检测（合并单元格表头）
  - [x] 基于字体特征的表头识别（加粗/字号）
- [x] 单元格文本投影：
  - [x] 将文本块投影到 row × column 网格
  - [x] 处理单元格为空的情况
  - [x] 处理文本跨列的情况

### 3. Cell 类型推断

- [x] 实现 `CellType` 检测：
  - [x] `Number`：纯数字（含千分位逗号）
  - [x] `Percent`：百分比（如 12.3%）
  - [x] `Currency`：货币（如 ¥1,234 / $5,678 / 1,234万元）
  - [x] `Date`：日期格式
  - [x] `Text`：普通文本
  - [x] `Unknown`：无法判断
- [x] 基于列统计的类型校正（同列多数类型一致）

### 4. fallback_text 生成（强制）

- [x] `table_as_text` 格式：
  - [x] KV 行格式：`列名=值` 逐行输出
  - [x] 保留行序号信息
- [x] `table_as_kv_lines` 格式：
  - [x] 每个单元格一行：`表=T1 页=3 行=2 列=收入 值=1234`
- [x] 结构化成功时也必须生成 `fallback_text`
- [x] 结构化失败时，`fallback_text` 为唯一输出（不丢信息）

### 5. RAG 扁平化导出完善

- [x] `table_row_lines` 生成：
  - [x] 格式：`表=T1 页=3 行key=2023年 列=收入 值=1234 列=支出 值=567`
  - [x] 表头信息嵌入每行
- [x] `table_cell_lines` 生成：
  - [x] 格式：`表=T1 页=3 行=2 列=收入 值=1234 类型=number`
  - [x] 单元格粒度（数字检索更稳）
- [x] `TableIR` → CSV 导出
- [x] `TableIR` → Markdown 表格导出

### 6. 输出 TableIR 整合

- [x] 将抽取结果填充到 `TableIR` 所有字段：
  - [x] `table_id` 生成
  - [x] `extraction_mode = Stream`
  - [x] `headers` / `rows` / `cells`
  - [x] `cell_types`
  - [x] `fallback_text`（必须）
- [x] `TableIR` 挂载到 `PageIR.tables`

### 7. 测试

- [x] 单元测试：
  - [x] 行聚类算法正确性
  - [x] 列聚类算法正确性
  - [x] 表头推断逻辑
  - [x] CellType 检测各类型
  - [x] fallback_text 生成格式
- [x] 集成测试：
  - [x] 端到端 stream 表格抽取验证
  - [x] 验证 headers / rows / cells 结构正确
  - [x] 验证 fallback_text 信息完整
- [x] CI 验证：
  - [x] `cargo fmt --check` 零差异
  - [x] `cargo clippy -D warnings` 零警告
  - [x] `cargo test --all-features` 67 测试全通过

---

## 完成标准

- [x] Stream 表格抽取覆盖常见两列/多列无框表格
- [x] 每个 `TableIR` 必须包含非空 `fallback_text`
- [x] `table_row_lines` / `table_cell_lines` 格式正确可用
- [x] CSV / Markdown 导出正确
- [x] 所有单元测试通过，CI 通过

---

## 实现总结

### 新增文件

| 文件 | 行数 | 功能 |
|------|------|------|
| `src/table/mod.rs` | 60 | 表格模块入口，`extract_tables()` 核心 API，候选检测→Stream抽取 pipeline |
| `src/table/candidate.rs` | 347 | 表格候选区域检测：行聚类、x坐标对齐分析、列间距规律性、数值密度、置信度评分 |
| `src/table/stream.rs` | 392 | Stream 表格抽取引擎：行/列聚类、列边界检测、网格投影、表头推断、CellType推断、fallback_text生成 |
| `src/table/cell_type.rs` | 183 | CellType 推断：Number/Percent/Currency/Date/Text/Unknown，支持中文货币后缀（万元/亿元等） |
| `src/table/fallback.rs` | 103 | fallback_text 生成器：KV行格式、kv_lines（单元格粒度）、row_lines（行粒度） |
| `tests/m3_tests.rs` | 405 | 22 个测试：候选检测、Stream抽取、CellType、fallback_text、CSV/Markdown导出、serde roundtrip、端到端 |

### 修改文件

| 文件 | 变更 |
|------|------|
| `src/lib.rs` | 新增 `pub mod table` 导出 |
| `src/ir/types.rs` | `CellType` 新增 `Hash` derive（用于列类型统计） |
| `src/pipeline/mod.rs` | `process_page` 集成表格抽取，填充 `PageIR.tables` |

### 核心设计决策

1. **候选区域检测**：基于多行 x 坐标对齐一致性 + 列数方差 + 数值密度三维评分，置信度 > 0.3 才进入抽取
2. **列边界检测**：收集所有行的字符间隙，按间隙大小聚类确定列分割点，自适应列宽
3. **表头推断**：双策略——字体特征（加粗/字号大于正文）+ 内容特征（非数字占比 > 50%）
4. **CellType 推断**：先逐单元格检测，再按列统计多数类型校正，确保同列类型一致
5. **fallback_text 强制生成**：无论结构化是否成功，每个 TableIR 必须包含非空 fallback_text，格式为 `[表ID 页N]\n行1: 列名=值 ...`
6. **货币后缀匹配顺序**：长后缀优先（`万元` 排在 `元` 前面），防止 `1,234万元` 被错误匹配

### CI 状态

- ✅ `cargo fmt --check` — 零格式差异
- ✅ `cargo clippy --all-features -- -D warnings` — 零警告
- ✅ `cargo test --all-features` — **67 测试全通过**
  - 6 单元测试（CellType）
  - 6 集成测试（PDF解析）
  - 13 IR 测试（serde/导出）
  - 19 M2 测试（PageScore/布局/页眉页脚）
  - 22 M3 测试（表格全流程）
  - 1 doc-test

### 测试覆盖

| 测试类别 | 数量 | 覆盖内容 |
|----------|------|----------|
| 候选区域检测 | 3 | 简单表格检测、纯文本排除、空输入 |
| Stream 抽取 | 3 | 端到端抽取、空输入、单行排除 |
| CellType | 5 | Number/Percent/Currency/Date/Text+Unknown 各类型变体 |
| fallback_text | 3 | KV行格式、kv_lines格式、row_lines格式 |
| CSV/Markdown导出 | 2 | CSV正确性、Markdown正确性 |
| RAG导出 | 2 | kv_lines、row_lines |
| serde | 1 | TableIR JSON roundtrip（含CellType） |
| 端到端 | 2 | extract_tables全流程、空页面 |
| 列类型一致性 | 1 | column_types与headers长度一致、类型正确 |
