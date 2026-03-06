# Milestone 17: Excel 结构化数据解析与分析引擎

## 目标

在不依赖外部 Python 环境的前提下，基于纯 Rust 生态，实现对 Excel 文件（`.xlsx`、`.xls`）的本地读取、清洗、展平、结构化存储，并通过 Text-to-SQL 技术实现自然语言的数据统计与分析。本里程碑将创建独立的 `knot-excel` crate，并集成到现有的 RAG 索引体系中。

## 背景

真实业务场景中，大量高价值数据以 Excel 格式存在。传统文本 Chunking 方案会破坏 Excel 的二维表结构，导致检索和问答失效。需要专门的解析引擎来处理 Excel 特有的结构（合并单元格、多级表头、多数据块混排等），并提供结构化查询能力。

## 成功指标

- 能解析标准 `.xlsx` / `.xls` 文件，提取出清洗后的扁平化 DataFrame
- 已索引的 Excel 文件能通过向量搜索被命中（Table Profile Chunk）
- 用户提问时，能通过 LLM 生成 SQL -> DuckDB 执行 -> 返回结构化结果
- 混合文档场景（PDF + Excel 同时命中）能智能路由，综合两类信息回答
- 多步查询场景（中间结果膨胀）有完善的降级策略，不会超出 LLM 上下文限制
- 支持合并单元格、多级表头等"中国式复杂报表"的降维处理
- 单 Sheet 多数据块能被正确切割和独立索引

## 前置条件

- Milestone 16 已完成（Knowledge 页面、文件索引管理、单文件 RAG 聊天）
- `knot-pdf` / `knot-parser` 架构成熟，可作为 `knot-excel` 的参考
- `FileRegistry` + `DirectoryWatcher` 已支持 `.xlsx` 文件类型识别（目前仅展示，不索引）

## 核心技术栈（Rust Native）

| 组件       | Crate      | 用途                                                               |
| ---------- | ---------- | ------------------------------------------------------------------ |
| 读取与解析 | `calamine` | 高速读取 `.xlsx`/`.xls`，获取二维数据、数据类型及合并单元格信息    |
| 清洗与降维 | `polars`   | Arrow 内存格式，`forward_fill` 填充、过滤空行、构建 DataFrame      |
| 查询与计算 | `duckdb`   | 嵌入式 OLAP 引擎，零拷贝读取 Polars DataFrame，执行 LLM 生成的 SQL |
| LLM 交互   | 现有 LLM   | 理解 Schema 并生成 SQL 语句，不直接接触全量数据                    |

## 迭代规划

### Iteration 1: 最小可用 -- 标准表读取 + Table Profile 索引（端到端 vertical slice）

**目标**: 跑通「读取标准 Excel -> 生成 Table Profile -> 存入向量库 -> 搜索命中」的完整链路。本阶段假设每个 Sheet 只有一个规整的标准二维表（第一行为表头，无合并单元格）。

#### 后端任务

| #   | 任务                                      | 说明                                                                                                                       |
| --- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 1.1 | 创建 `knot-excel` crate                   | 在 workspace 中新建独立项目，依赖 `calamine`、`polars`；结构参考 `knot-pdf`，包含 `Config`、`Pipeline`、`Error` 等基础模块 |
| 1.2 | 实现 `ExcelReader`                        | 封装 Calamine，遍历所有可见 Sheet，读取 Bounding Box 内数据为 `Vec<Vec<DataType>>`；自动识别第一行为表头，其余为数据体     |
| 1.3 | 实现 `DataFrame` 构建                     | 将读取的二维数据转换为 Polars `DataFrame`，推断列类型（String/Float/Int/Date），drop 全 Null 行和列                        |
| 1.4 | 实现 `TableProfile` 生成                  | 为每个 DataFrame 生成结构化摘要文本：元数据（文件路径_Sheet名）、Schema（列名+类型）、数据抽样（前 3 行）                  |
| 1.5 | 在 `knot-parser` 中新增 `excel.rs` format | 实现 `DocumentParser` trait，调用 `knot-excel` 解析 Excel 文件，将 `TableProfile` 转换为 `PageNode` 树                     |
| 1.6 | 修改 `KnotIndexer` 索引逻辑               | 在 `.xlsx`/`.xls` 文件索引时，标记 `doc_type: "tabular"` 到 VectorRecord metadata                                          |
| 1.7 | 修改 `monitor.rs` 文件类型支持            | 将 `.xlsx`/`.xls` 从"仅展示"提升为"可索引"                                                                                 |

#### 数据流

```
Excel 文件
  -> ExcelReader (calamine): 读取 Sheet -> Vec<Vec<DataType>>
  -> DataFrame 构建 (polars): 推断类型、清理空值
  -> TableProfile 生成: 结构化摘要文本
  -> PageNode 转换 (knot-parser): 适配现有索引体系
  -> VectorRecord + Embedding -> LanceDB + Tantivy
```

#### 前端任务

| #   | 任务               | 说明                                                                   |
| --- | ------------------ | ---------------------------------------------------------------------- |
| 1.8 | Knowledge 页面更新 | `.xlsx` 文件的索引状态从 `Unsupported` 变为动态状态（未索引/已索引等） |
| 1.9 | 表格类型文件图标   | 为 Excel 文件显示专属图标（`table_chart` 或 Excel 图标）               |

#### 验收标准

- 一个标准的 `.xlsx` 文件（如销售报表）能被成功索引
- 在 RAG 搜索中输入"销售数据"能命中该 Excel 的 Table Profile Chunk
- Knowledge 页面正确显示 Excel 文件的索引状态

---

### Iteration 2: 混合查询路由 + Text-to-SQL 引擎

**目标**: 实现混合查询路由器（Hybrid Query Router），根据搜索结果中的文档类型自动选择查询策略。支持三种场景：纯文本 RAG、纯 Text-to-SQL、以及混合文档的并行查询与合并。同时解决多步查询和中间结果膨胀问题。

#### 核心设计一：混合查询路由

用户提问时，搜索结果可能命中多种文档类型。**不能简单地"二选一"**，而是需要**分流并行 + LLM 合并**：

| 场景         | 搜索结果命中的 doc_type     | 查询策略                                       |
| ------------ | --------------------------- | ---------------------------------------------- |
| **纯文本**   | 全部是 `text`               | 走现有 RAG 路径（完全不变）                    |
| **纯表格**   | 全部是 `tabular`            | 所有关联 DataFrame 挂载到 DuckDB, Text-to-SQL  |
| **混合文档** | 同时包含 `text` + `tabular` | 两路并行：文本上下文 + SQL 查询结果, LLM 综合  |
| **多表格**   | 多个不同来源的 `tabular`    | DuckDB 同时挂载多个 DataFrame，LLM 可生成 JOIN |

> **关键决策**：混合场景下，不是让 LLM 分别回答再拼接，而是将文本上下文和 SQL 查询结果**同时注入一个 Prompt**，LLM 综合两类信息给出统一回答。这避免了信息割裂和答案冲突。

#### 核心设计二：多步查询与结果膨胀控制

**问题场景**：用户问"销量增长最快的前 5 个产品的库存状态如何？"——这需要两步：先从销售表算增长率取 TOP5，再用这 5 个产品去库存表查。如果第一步返回 10000 行中间数据，直接塞进 LLM 上下文（当前 8192 tokens/slot）会溢出。

**核心原则**：**中间数据不经过 LLM，让 DuckDB 自己完成多步计算**

四层降级策略：

```
          策略优先级                            数据是否经过 LLM
  +-----------------------+
  | 1. CTE / 子查询       | ----------- 不经过，DuckDB 内存完成  (最优)
  |                       |
  | 2. DuckDB 临时表链    | ----------- 不经过，DuckDB 临时表    (次优)
  |                       |
  | 3. 结果摘要化 + 多步  | ----------- 只传摘要，不传全量      (降级)
  |                       |
  | 4. 截断 + 警告        | ----------- 硬截断前 20 行          (兜底)
  +-----------------------+
```

**策略 1：CTE / 子查询（优先）**

Prompt 中明确引导 LLM 使用 CTE（WITH 语句）或子查询，在一条 SQL 中完成多步逻辑：

```sql
-- LLM 应被引导生成这样的 SQL，中间结果完全在 DuckDB 内存中流转
WITH top_products AS (
    SELECT "产品",
           ("本期销量" - "上期销量") / NULLIF("上期销量", 0) AS growth_rate
    FROM sales
    ORDER BY growth_rate DESC
    LIMIT 5
)
SELECT tp."产品", tp.growth_rate, inv."库存量", inv."仓库"
FROM top_products tp
JOIN inventory inv ON tp."产品" = inv."产品"
```

Prompt 中的约束指令：
```
优先使用 WITH (CTE) 或子查询实现多步逻辑，避免拆分为多条 SQL。
DuckDB 完整支持 CTE、窗口函数、QUALIFY 子句。
```

**策略 2：DuckDB 临时表链（CTE 不够用时）**

当 LLM 确实生成了多条 SQL（用 `;` 分隔），`QueryEngine` 自动将中间 SQL 的结果注册为 DuckDB 临时表，后续 SQL 直接引用临时表名：

```rust
impl QueryEngine {
    /// 执行可能包含多条 SQL 的查询
    /// 中间结果自动注册为 DuckDB 临时表，不回传给 LLM
    pub fn execute_multi_step(&self, sql_text: &str) -> Result<QueryResult> {
        let statements: Vec<&str> = sql_text.split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for (i, stmt) in statements.iter().enumerate() {
            if i < statements.len() - 1 {
                // 中间 SQL：结果注册为临时表 step_N
                let wrapped = format!(
                    "CREATE OR REPLACE TEMP TABLE step_{} AS ({})",
                    i, stmt
                );
                self.conn.execute(&wrapped)?;
            } else {
                // 最后一条 SQL：执行并返回结果
                return self.execute_and_format(stmt);
            }
        }
    }
}
```

**策略 3：结果摘要化（中间结果必须回传 LLM 时的降级方案）**

当策略 1/2 失败（比如 SQL 执行报错需要 LLM 看中间结果来修改），不传全量数据，而是传**结果摘要**：

```rust
pub struct ResultSummary {
    pub row_count: usize,
    pub columns: Vec<ColumnSummary>,
    pub sample_rows: Vec<Vec<String>>,  // 前 5 行
}

pub struct ColumnSummary {
    pub name: String,
    pub dtype: String,
    pub null_count: usize,
    pub distinct_count: usize,
    // 数值列的统计信息
    pub min: Option<String>,
    pub max: Option<String>,
    pub avg: Option<String>,
}

impl ResultSummarizer {
    const MAX_FULL_ROWS: usize = 20;  // <= 20 行直接传全量

    pub fn summarize(result: &QueryResult) -> ResultContext {
        if result.rows.len() <= Self::MAX_FULL_ROWS {
            // 结果不大，直接传全量 Markdown 表格
            ResultContext::Full(result.to_markdown())
        } else {
            // 结果太大，生成统计摘要
            ResultContext::Summary(ResultSummary {
                row_count: result.rows.len(),
                columns: Self::compute_column_stats(result),
                sample_rows: result.rows[..5].to_vec(),
            })
        }
    }
}
```

摘要化后注入 LLM Prompt 的格式：
```
上一步查询结果摘要（共 2,847 行，因数据量大仅展示统计信息）：
- 列 "产品"(String): 42 个不同值
- 列 "销量"(Int64): min=0, max=9999, avg=234.5
- 列 "日期"(Date): 范围 2024-01-01 ~ 2024-12-31

前 5 行样本：
| 产品  | 销量 | 日期       |
| 产品A | 100  | 2024-01-15 |
| 产品A | 120  | 2024-02-15 |
| ...   | ...  | ...        |

请基于以上信息生成下一步 SQL。
```

**策略 4：硬截断 + 警告（最终兜底）**

如果所有策略都失败，硬截断结果到前 20 行，并在回答中附加警告："数据较多，仅展示部分结果。"

#### SQL 执行完整流程

```
LLM 生成 SQL
  |
  v
QueryEngine.execute()
  |
  +-- 解析：单条 SQL or 多条（分号分隔）?
  |
  +-- [单条 SQL] ------------------------------------+
  |    执行 -> 检查结果行数                          |
  |    |                                             |
  |    +-- <= 20 行: 全量 Markdown 返回  ---------> 合入 Prompt -> LLM 回答
  |    |                                             |
  |    +-- > 20 行: 摘要化               ---------> 合入 Prompt -> LLM 回答
  |                                                  |  (附注 "完整数据已查询，
  |                                                  |   共 N 行，此处仅展示摘要")
  |                                                  |
  +-- [多条 SQL] ------------------------------------+
       |
       +-- 前 N-1 条: CREATE TEMP TABLE step_i AS (...)
       |   (中间数据留在 DuckDB 内存，不经过 LLM)
       |
       +-- 最后 1 条: 执行并返回结果
       |   -> 同样检查行数, 走全量/摘要逻辑
       |
       +-- 如果某步执行失败:
            -> 将该步 SQL + 错误信息 + 临时表 schema 摘要
            -> 发给 LLM 要求修复（不传全量数据）
            -> 重试最多 2 次
```

#### 后端任务

| #    | 任务                                     | 说明                                                                                                                              |
| ---- | ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| 2.1  | 扩展 `VectorRecord` 增加 `doc_type` 字段 | 在 LanceDB Schema 和 Tantivy Schema 中新增 `doc_type: String` 字段（值为 `"text"` 或 `"tabular"`），兼容存量数据（默认 `"text"`） |
| 2.2  | 扩展 `SearchResult` 增加 `doc_type` 字段 | 搜索结果携带文档类型信息，供路由器判断                                                                                            |
| 2.3  | 集成 `duckdb` crate                      | 在 `knot-excel` 中添加 DuckDB 依赖，实现 Arrow 零拷贝注册 Polars DataFrame 为临时表                                               |
| 2.4  | 实现 `QueryEngine`                       | 封装 DuckDB 连接管理、多 DataFrame 注册/注销、SQL 执行（含多步临时表链）、结果格式化（Markdown 表格）                             |
| 2.5  | 实现 `SqlGenerator` Prompt 构建          | 将用户 Query + 所有关联临时表名 + Schema 信息组装为 LLM Prompt，**引导 LLM 优先使用 CTE/子查询一步完成**                          |
| 2.6  | 实现 SQL 执行容错 + 多步重试             | SQL 执行失败时重试（最多 2 次）；多步 SQL 自动走临时表链；失败时只传 schema 摘要给 LLM                                            |
| 2.7  | 实现 `ResultSummarizer`                  | 当 SQL 结果超过 20 行时，生成统计摘要（行数、列统计、前 5 行样本）替代全量数据注入 LLM Prompt                                     |
| 2.8  | 实现 `HybridQueryRouter`                 | 核心路由逻辑：按 `doc_type` 分流搜索结果, 判断场景, 选择查询策略, 合并结果上下文                                                  |
| 2.9  | 修改 `rag_search` 集成路由逻辑           | 搜索完成后，根据 `doc_type` 分流，对 tabular 结果执行 SQL 查询，将查询结果附加到 context 中                                       |
| 2.10 | 新增 `query_excel_table` Tauri command   | 单文件模式：接收用户 Query + 文件路径，直接走 Text-to-SQL 路径                                                                    |

#### 数据流（混合文档场景）

```
用户提问 "根据策略文件和销售数据，分析2024年Q3的销售策略效果"
  |
  v
  RAG 搜索 (向量 + 关键词)
  |
  +-- SearchResult #1: strategy.pdf     doc_type="text"     score=92%
  +-- SearchResult #2: report.xlsx/销售  doc_type="tabular"  score=88%
  +-- SearchResult #3: report.xlsx/目标  doc_type="tabular"  score=85%
  +-- SearchResult #4: meeting.md       doc_type="text"     score=80%
  |
  v
  HybridQueryRouter: 检测到混合文档 (text + tabular)
  |
  +--- 文本通道 -------------------------------------------+
  |    搜索结果 #1, #4 -> 拼接为文本上下文                  |
  |    "[1] 文件: strategy.pdf                              |
  |     内容: Q3策略重点是..."                               |
  |                                                         |
  +--- 表格通道 -------------------------------------------+
  |    搜索结果 #2, #3 -> 加载对应 DataFrame                |
  |    -> DuckDB 同时注册两个临时表                         |
  |    -> LLM 生成 SQL (优先 CTE 一步完成)                  |
  |    -> DuckDB 执行 -> ResultSummarizer 处理              |
  |    -> <= 20 行: 全量 Markdown                           |
  |    -> > 20 行: 统计摘要 + 前 5 行样本                   |
  |                                                         |
  +--- 合并上下文 -----------------------------------------+
       |
       v
  构建综合 Prompt -> LLM 综合生成回答 -> 流式返回前端
```

#### `HybridQueryRouter` 核心逻辑

```rust
/// 混合查询路由器
pub struct HybridQueryRouter;

pub enum QueryPlan {
    /// 纯文本 RAG（现有路径，不变）
    TextOnly {
        text_context: String,
    },
    /// 纯结构化查询（所有结果都是表格）
    TabularOnly {
        table_sources: Vec<TabularSource>,
    },
    /// 混合查询（文本 + 表格并行）
    Hybrid {
        text_context: String,
        table_sources: Vec<TabularSource>,
    },
}

pub struct TabularSource {
    pub file_path: String,
    pub sheet_name: String,
    pub block_index: usize,
    pub source_id: String,  // 用于 DuckDB 表名
    pub profile: TableProfile,  // Schema + 数据抽样
}

impl HybridQueryRouter {
    /// 根据搜索结果分流，生成查询计划
    pub fn plan(results: &[SearchResult]) -> QueryPlan {
        let text_results: Vec<_> = results.iter()
            .filter(|r| r.doc_type == "text")
            .collect();
        let tabular_results: Vec<_> = results.iter()
            .filter(|r| r.doc_type == "tabular")
            .collect();

        match (text_results.is_empty(), tabular_results.is_empty()) {
            (false, true)  => QueryPlan::TextOnly { /* ... */ },
            (true, false)  => QueryPlan::TabularOnly { /* ... */ },
            (false, false) => QueryPlan::Hybrid { /* ... */ },
            (true, true)   => QueryPlan::TextOnly { text_context: String::new() },
        }
    }
}
```

#### 混合场景下的 Prompt 模板

```
你是一个智能助手。请综合参考文档和数据分析结果，直接回答用户问题。

## 参考文档
{text_context}

## 数据分析结果
以下是从 Excel 数据中查询得到的结果：

SQL: {executed_sql}

{markdown_table_result_or_summary}

## 用户问题
{user_query}

## 回答原则
1. 综合文本信息和数据分析结果给出完整回答
2. 引用具体数据时标明来源（文档 or 数据表）
3. 如果数据和文档有矛盾，优先以数据为准并指出差异
```

#### 前端任务

| #    | 任务               | 说明                                                                             |
| ---- | ------------------ | -------------------------------------------------------------------------------- |
| 2.11 | 结构化查询结果展示 | 当 RAG 回答包含表格数据时，以格式化的 Markdown 表格展示（而非纯文本）            |
| 2.12 | 查询状态多阶段指示 | 显示分阶段状态："正在搜索..." -> "正在分析表格数据..." -> "正在生成回答..."      |
| 2.13 | 数据来源类型标注   | Sources 列表中区分文本来源和数据来源，数据来源显示执行的 SQL                     |
| 2.14 | 单文件 Excel 聊天  | Knowledge 页面支持对已索引的 Excel 文件进行单文件聊天（直接走 Text-to-SQL 路径） |
| 2.15 | 大结果集警告提示   | 当结果被摘要化时，显示"数据量较大（N 行），已自动汇总展示"的提示                 |

#### 验收标准

- **纯表格场景**: 提问"XX 表的 YY 列总和是多少"时，系统能自动生成 SQL 并返回正确结果
- **混合文档场景**: 同时命中 PDF 和 Excel 时，回答能综合文本描述和数据结果
- **多 Excel 场景**: 命中多个 Excel 数据块时，DuckDB 能同时挂载并支持跨表查询
- **多步查询场景**: "增长最快的产品库存如何"能通过 CTE 或临时表链一次性完成
- **结果膨胀场景**: SQL 返回 1000+ 行时，自动摘要化，不超出 LLM 上下文限制
- SQL 执行失败时能自动重试并修复（最多 2 次）
- 结果以 Markdown 表格形式展示

---

### Iteration 3: 复杂报表处理 -- 合并单元格 + 多级表头降维

**目标**: 处理"中国式复杂报表"：多级表头降维拼接、合并单元格空值填充、脏数据行过滤。

#### 后端任务

| #   | 任务                         | 说明                                                                                                               |
| --- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| 3.1 | 获取合并单元格信息           | 通过 Calamine 的 `merged_regions()` API 获取所有合并区域的坐标范围                                                 |
| 3.2 | 实现多级表头检测             | 启发式算法：识别数据块前 N 行为表头区（连续的全文本行），处理表头合并导致的空值继承                                |
| 3.3 | 实现多级表头降维拼接         | 将 N 行同一列的表头文本自上而下拼接（如 `["2025", "上半年", "收入"]` -> `"2025_上半年_收入"`），确保列名唯一且非空 |
| 3.4 | 实现数据体 `forward_fill`    | 针对维度列（如"部门"、"地区"），因 Excel 垂直合并导致空值的情况，调用 Polars `.forward_fill()` 向下填充            |
| 3.5 | 实现脏数据行过滤             | 识别并剔除表头前的说明文字行、表尾的合计/备注行（启发式规则：文本内容跨越多列合并、且不符合数据体类型模式）        |
| 3.6 | 更新 `TableProfile` 生成逻辑 | 在摘要中标注表头层级数、合并区域数量等结构复杂度信息，帮助 LLM 更好理解表结构                                      |

#### 验收标准

- 一个带合并单元格和多级表头的复杂报表（如财务报表）能被正确解析
- `forward_fill` 后数据完整性不丢失
- 降维后的列名可读且唯一

---

### Iteration 4: 智能数据区探测 -- 多数据块切割与跨表查询

**目标**: 处理单 Sheet 内包含多个数据块（表格混排）的情况，支持 DuckDB 同时挂载多个 DataFrame 进行跨表查询。

#### 后端任务

| #   | 任务                     | 说明                                                                                                                               |
| --- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| 4.1 | 实现数据块切割算法       | 基于两种启发式规则识别数据块边界：1.**空白楚河汉界** -- 连续的全空行/列作为分隔；2.**数据类型跳变** -- 从全文本区突变为数值/日期区 |
| 4.2 | 实现数据块独立索引       | 每个数据块生成独立的 `TableProfile`，元数据标记 `[文件路径]_[Sheet名]_[数据块ID]`                                                  |
| 4.3 | 支持 DuckDB 多表挂载     | 同一文件的多个数据块 / 跨文件的多个 DataFrame 可同时注册为 DuckDB 临时表                                                           |
| 4.4 | 增强 SQL Prompt 支持多表 | LLM Prompt 中列出所有关联表的 Schema，允许生成 `JOIN` 或 `UNION ALL` 语句                                                          |
| 4.5 | 图表和图片元素过滤       | 忽略 `xl/charts/` 和 `xl/media/` 中的视觉元素，只关注单元格数据                                                                    |

#### 验收标准

- 单 Sheet 包含 2 个以上数据表时，能被正确切割为独立的 DataFrame
- 跨数据块的 JOIN 查询能正确执行
- 图表/图片元素不干扰数据提取

---

## 架构设计

### 项目结构（参考 knot-pdf）

```
knot-excel/
+-- Cargo.toml
+-- src/
|   +-- lib.rs              # 公共 API：parse_excel(), Pipeline
|   +-- config.rs           # ExcelConfig（解析选项、数据块探测阈值等）
|   +-- error.rs            # ExcelError 定义
|   +-- reader/             # 模块一：数据读取
|   |   +-- mod.rs
|   |   +-- sheet.rs        # Sheet 遍历、Bounding Box 读取
|   |   +-- block.rs        # 数据块切割算法（Iteration 4）
|   +-- transform/          # 模块二：数据清洗与展平
|   |   +-- mod.rs
|   |   +-- header.rs       # 多级表头检测与降维拼接
|   |   +-- merge.rs        # 合并单元格处理 + forward_fill
|   |   +-- filter.rs       # 脏数据过滤
|   +-- profile/            # 模块三：Table Profile 生成
|   |   +-- mod.rs
|   |   +-- summary.rs      # Schema 提取 + 数据抽样 + Chunk 文本构建
|   +-- query/              # 模块四：查询引擎（Iteration 2）
|   |   +-- mod.rs
|   |   +-- engine.rs       # DuckDB 连接管理 + DataFrame 注册 + 多步执行
|   |   +-- sql.rs          # SQL Prompt 构建 + 结果格式化 + 结果摘要化
|   +-- pipeline.rs         # Pipeline：串联 Reader -> Transform -> Profile 完整流程
+-- tests/
    +-- ...
```

### 核心数据模型

```rust
/// 解析后的单个数据块
pub struct DataBlock {
    /// 来源标识：file_path + sheet_name + block_index
    pub source_id: String,
    /// Sheet 名称
    pub sheet_name: String,
    /// 数据块在 Sheet 中的索引（0-based）
    pub block_index: usize,
    /// 清洗后的 Polars DataFrame
    pub dataframe: polars::frame::DataFrame,
    /// 原始表头层级数（1 = 标准单行表头）
    pub header_levels: usize,
    /// 合并单元格区域数量
    pub merged_region_count: usize,
}

/// 数据块的结构化摘要（用于向量化索引）
pub struct TableProfile {
    /// 来源标识
    pub source_id: String,
    /// 文件路径
    pub file_path: String,
    /// Sheet 名称
    pub sheet_name: String,
    /// 列名列表
    pub column_names: Vec<String>,
    /// 列类型（推断后的 Polars DataType 的字符串表示）
    pub column_types: Vec<String>,
    /// 行数
    pub row_count: usize,
    /// 前 3 行数据抽样（JSON 格式）
    pub sample_rows: Vec<Vec<String>>,
    /// 额外描述（如"含合并单元格"、"多级表头"等）
    pub description: String,
}

/// 查询结果
pub struct QueryResult {
    /// 执行的 SQL 语句（可能包含多步）
    pub sql: String,
    /// 结果表头
    pub columns: Vec<String>,
    /// 结果数据行
    pub rows: Vec<Vec<String>>,
    /// 是否经过重试修复
    pub retried: bool,
    /// 多步执行时的中间步骤数
    pub intermediate_steps: usize,
}

/// SQL 结果上下文（传递给 LLM 的内容）
pub enum ResultContext {
    /// 结果不大（<= 20 行），全量传递
    Full(String),
    /// 结果过大，传递统计摘要
    Summary(ResultSummary),
}

/// 结果摘要
pub struct ResultSummary {
    pub row_count: usize,
    pub columns: Vec<ColumnSummary>,
    pub sample_rows: Vec<Vec<String>>,  // 前 5 行
}

pub struct ColumnSummary {
    pub name: String,
    pub dtype: String,
    pub null_count: usize,
    pub distinct_count: usize,
    pub min: Option<String>,
    pub max: Option<String>,
    pub avg: Option<String>,
}
```

### 与现有系统的集成点

```
knot-excel (新 crate)
  +-- 被 knot-parser 的 excel.rs format handler 调用
  +-- 通过 DocumentParser trait 适配 PageNode 体系
  +-- 被 knot-app Tauri commands 直接调用（Text-to-SQL 查询）

knot-parser
  +-- formats/excel.rs (新文件)
       +-- impl DocumentParser for ExcelParser
       +-- 调用 knot-excel::parse_excel() -> TableProfile -> PageNode

knot-core
  +-- store.rs
  |    +-- VectorRecord 新增 doc_type: String 字段
  |    +-- SearchResult 新增 doc_type: String 字段
  |    +-- LanceDB Schema 新增 doc_type 列
  |    +-- Tantivy Schema 新增 doc_type indexed field
  +-- index.rs
       +-- 索引时按文件类型写入 doc_type ("text" 或 "tabular")

knot-app
  +-- main.rs
       +-- HybridQueryRouter: 搜索后按 doc_type 分流
       +-- 文本通道: 走现有 rag_generate 路径
       +-- 表格通道: 加载 DataFrame -> DuckDB -> Text-to-SQL
       +-- ResultSummarizer: 结果膨胀控制
       +-- 混合合并: 两路结果注入同一 Prompt -> LLM 综合回答
       +-- query_excel_table command (单文件 Text-to-SQL)
```

### doc_type 字段的 Schema 变更

```rust
// === knot-core/src/store.rs ===

// VectorRecord 新增 doc_type 字段
pub struct VectorRecord {
    pub id: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub file_path: String,
    pub parent_id: Option<String>,
    pub breadcrumbs: Option<String>,
    pub doc_type: String,  // NEW: "text" | "tabular"
}

// SearchResult 新增 doc_type 字段
pub struct SearchResult {
    pub id: String,
    pub text: String,
    pub file_path: String,
    pub score: f32,
    pub parent_id: Option<String>,
    pub breadcrumbs: Option<String>,
    pub source: SearchSource,
    pub expanded_context: Option<String>,
    pub doc_type: String,  // NEW: "text" | "tabular"
}

// LanceDB Schema 新增列
fn get_schema(&self) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        // ... existing fields ...
        Field::new("doc_type", DataType::Utf8, false),  // NEW
    ]))
}

// Tantivy Schema 新增 indexed field
fn build_tantivy_schema() -> TantivySchema {
    let mut builder = TantivySchema::builder();
    // ... existing fields ...
    builder.add_text_field("doc_type", STRING | STORED);  // NEW: 精确匹配
    builder.build()
}
```

**兼容性**：存量数据没有 `doc_type` 字段。在搜索时，若该字段为空或缺失，默认视为 `"text"`。这确保升级后现有索引数据不受影响。

### Table Profile -> PageNode 转换

```rust
// knot-parser/src/formats/excel.rs
// 每个 DataBlock 的 TableProfile 转换为一个 PageNode（叶子节点）
// 一个 Excel 文件的 PageNode 树结构：
//
// Root: "report.xlsx"
//   +-- Sheet1: "销售数据"
//   |   +-- Block 0: "该表格包含列 [日期, 产品, 销量, 金额]，共 150 行..."
//   |   +-- Block 1: "该表格包含列 [地区, 季度, 目标]，共 12 行..."
//   +-- Sheet2: "库存数据"
//       +-- Block 0: "该表格包含列 [SKU, 名称, 库存量, 仓库]，共 500 行..."
```

### Text-to-SQL Prompt 设计

```
你是一个专业的 SQL 分析师。根据以下表结构和用户问题，生成一条兼容 DuckDB 语法的 SQL 查询语句。

## 可用表

表名: sheet1_block0
列信息:
  - 日期 (Date)
  - 产品 (String)
  - 销量 (Int64)
  - 金额 (Float64)

数据示例（前 3 行）:
| 日期       | 产品   | 销量 | 金额     |
| 2024-01-15 | 产品A  | 100  | 15000.0  |
| 2024-01-16 | 产品B  | 200  | 30000.0  |
| 2024-01-17 | 产品A  | 150  | 22500.0  |

## 用户问题
{user_query}

## 要求
1. 仅返回 SQL 语句本身，不要任何解释
2. 使用 DuckDB 兼容语法
3. 列名需用双引号包裹（中文列名）
4. 优先使用 WITH (CTE) 或子查询实现多步逻辑，避免拆分为多条 SQL
5. DuckDB 完整支持 CTE、窗口函数（ROW_NUMBER, LAG, LEAD）、QUALIFY 子句
```

## 涉及文件

| 文件                                           | 变更类型 | Iteration | 说明                                                                       |
| ---------------------------------------------- | -------- | --------- | -------------------------------------------------------------------------- |
| `knot-excel/` (整个 crate)                     | 新建     | 1-4       | Excel 解析引擎核心                                                         |
| `knot-excel/src/lib.rs`                        | 新建     | 1         | 公共 API 入口                                                              |
| `knot-excel/src/config.rs`                     | 新建     | 1         | 配置项定义                                                                 |
| `knot-excel/src/error.rs`                      | 新建     | 1         | 错误类型                                                                   |
| `knot-excel/src/reader/`                       | 新建     | 1, 4      | Calamine 封装、数据块切割                                                  |
| `knot-excel/src/transform/`                    | 新建     | 3         | 表头降维、合并处理、脏数据过滤                                             |
| `knot-excel/src/profile/`                      | 新建     | 1         | Table Profile 生成                                                         |
| `knot-excel/src/query/`                        | 新建     | 2         | DuckDB 查询引擎 + 多步执行 + 结果摘要化                                    |
| `knot-excel/src/pipeline.rs`                   | 新建     | 1         | Pipeline 串联                                                              |
| `knot-parser/src/formats/excel.rs`             | 新建     | 1         | Excel format handler，实现 DocumentParser trait                            |
| `knot-parser/src/formats/mod.rs`               | 修改     | 1         | 导出 excel 模块                                                            |
| `knot-parser/Cargo.toml`                       | 修改     | 1         | 添加 knot-excel 依赖                                                       |
| `knot-core/src/store.rs`                       | 修改     | 2         | `VectorRecord`/`SearchResult` 增加 `doc_type`；LanceDB/Tantivy Schema 扩展 |
| `knot-core/src/index.rs`                       | 修改     | 1-2       | 索引时写入 `doc_type`（text/tabular）                                      |
| `knot-core/src/monitor.rs`                     | 修改     | 1         | `.xlsx`/`.xls` 提升为可索引类型                                            |
| `knot-app/src-tauri/src/main.rs`               | 修改     | 2         | `HybridQueryRouter` + `ResultSummarizer` + `query_excel_table` command     |
| `knot-app/src/lib/components/Knowledge.svelte` | 修改     | 1-2       | Excel 文件索引状态更新 + 结构化查询结果展示                                |
| `Cargo.toml` (workspace)                       | 修改     | 1         | 将 `knot-excel` 加入 workspace members                                     |

## 风险与约束

| 风险                                  | 影响             | 缓解措施                                                           |
| ------------------------------------- | ---------------- | ------------------------------------------------------------------ |
| Polars 编译体积大                     | 增加编译时间     | 使用 `polars` 的 feature flags 按需引入，仅启用必要的 Series 操作  |
| DuckDB Rust binding 成熟度            | 潜在兼容问题     | 评估 `duckdb-rs` crate 稳定性；备选方案为落盘 Parquet + DuckDB CLI |
| 本地 LLM 生成 SQL 质量不稳定          | 查询可能出错     | 实现 2 次自动重试 + 错误信息反馈；考虑提供 SQL 预览/确认机制       |
| 本地 LLM 上下文窗口有限 (8192 tokens) | 大结果溢出       | 4 层降级策略：CTE -> 临时表链 -> 结果摘要化 -> 硬截断              |
| 复杂报表的启发式算法无法覆盖所有格式  | 部分文件解析失败 | 回退到"原样输出全部内容为文本"的降级方案，确保至少可被文本检索命中 |
| 大型 Excel 文件（100MB+）内存占用     | OOM 风险         | 设置文件大小上限警告，支持按 Sheet 分批加载                        |
| `calamine` 对 `.xls` 旧格式支持有限   | 部分文件不兼容   | 明确标注支持程度，`.xls` 作为 best-effort                          |
| 混合查询延迟增加                      | 体验下降         | 表格通道和文本通道并行执行；SQL 查询有超时机制（5s）               |

## 不在本里程碑范围

以下功能明确**不在** Milestone 17 范围内：

- CSV 文件的解析与索引（将作为独立 milestone 或 knot-excel 的扩展）
- Excel 文件的可视化预览（在 Knowledge 页面内渲染表格）
- Excel 写回功能（生成或修改 Excel 文件）
- 实时协作编辑 Excel
- `.ods`（OpenDocument）格式支持
- 复杂的跨文件自动 JOIN 推理（本 milestone 支持 LLM 手动生成 JOIN SQL，但不做自动 schema 匹配）
