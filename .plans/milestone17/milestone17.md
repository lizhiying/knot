# Milestone 17: Excel 结构化数据解析与分析引擎

## 成果目标

在不依赖外部 Python 环境的前提下，基于纯 Rust 生态，实现对 Excel 文件的本地读取、清洗、展平、结构化存储，并通过 Text-to-SQL 技术实现自然语言的数据统计与分析。

## 成功指标

- 能解析标准 `.xlsx` / `.xls` 文件，提取出清洗后的扁平化 DataFrame
- 已索引的 Excel 文件能通过向量搜索被命中（Table Profile Chunk）
- 用户提问时，能通过 LLM 生成 SQL -> DuckDB 执行 -> 返回结构化结果
- 混合文档场景（PDF + Excel 同时命中）能智能路由，综合两类信息回答
- 多步查询场景有完善的降级策略，不超出 LLM 上下文限制
- 支持合并单元格、多级表头等"中国式复杂报表"的降维处理
- 单 Sheet 多数据块能被正确切割和独立索引

## 核心技术栈

| 组件       | Crate      | 用途                             |
| ---------- | ---------- | -------------------------------- |
| 读取与解析 | `calamine` | 高速读取 `.xlsx`/`.xls`          |
| 清洗与降维 | `polars`   | Arrow 格式 DataFrame 操作        |
| 查询与计算 | `duckdb`   | 嵌入式 OLAP，执行 LLM 生成的 SQL |
| LLM 交互   | 现有 LLM   | 理解 Schema 并生成 SQL           |

## 迭代概览

| 迭代        | 名称                                       | 核心价值                                                        |
| ----------- | ------------------------------------------ | --------------------------------------------------------------- |
| Iteration 1 | 最小可用 — 标准表读取 + Table Profile 索引 | 端到端 vertical slice: 读取 -> 构建 Profile -> 索引 -> 搜索命中 |
| Iteration 2 | 混合查询路由 + Text-to-SQL 引擎            | HybridQueryRouter + DuckDB 查询 + 结果膨胀控制                  |
| Iteration 3 | 复杂报表处理 — 合并单元格 + 多级表头降维   | 处理"中国式复杂报表"                                            |
| Iteration 4 | 智能数据区探测 — 多数据块切割与跨表查询    | 单 Sheet 多数据块切割 + 跨表 JOIN                               |

## 关键风险

- Polars 编译体积大 → 按需引入 feature flags
- DuckDB Rust binding 成熟度 → 评估 `duckdb-rs` 稳定性
- 本地 LLM 生成 SQL 质量不稳定 → 2 次自动重试 + 错误反馈
- 本地 LLM 上下文窗口有限 (8192 tokens) → 4 层降级策略
- 复杂报表启发式算法不能覆盖所有格式 → 降级为纯文本输出

## 详细文档

- 里程碑详细规划：[milestone17.md](file:///Users/lizhiying/Projects/knot/source/knot-workspaces/docs/milestones/milestone17.md)
- Iteration 1: [iteration1.md](file:///Users/lizhiying/Projects/knot/source/knot-workspaces/.plans/milestone17/iteration1.md)
- Iteration 2: [iteration2.md](file:///Users/lizhiying/Projects/knot/source/knot-workspaces/.plans/milestone17/iteration2.md)
- Iteration 3: [iteration3.md](file:///Users/lizhiying/Projects/knot/source/knot-workspaces/.plans/milestone17/iteration3.md)
- Iteration 4: [iteration4.md](file:///Users/lizhiying/Projects/knot/source/knot-workspaces/.plans/milestone17/iteration4.md)
