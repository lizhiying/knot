Milestone: milestone17
Iteration: 上下文预算动态分配 — 消除硬编码阈值

Goal:
当前 `rag_search` 使用硬编码阈值（50 行）判断表格是否为"大表格"来决定注入策略，
存在以下问题：
1. **50 行表格 × n 列 + 其他 text 切片**可能已远超 LLM 上下文窗口（~13,700 字符）
2. **20 行表格 × 3 列**可能完全放得下，却也被走 SQL 路径，增加了不必要的 LLM 调用
3. 上下文超限检测在 `rag_generate`（太晚），`rag_search` 阶段没有预算感知

本迭代核心改进：**将上下文预算计算提前到 `rag_search` 阶段**，根据实际数据量动态决定
是直接注入 Markdown 还是走 DuckDB Text-to-SQL，并合理分配 text 与 tabular 的预算。

Assumptions:
- Iteration 6 已完成：Markdown 表格渲染和 SQL 结果分页功能可用
- LLM 上下文窗口配置已存在于 AppState（context_size=16384, max_tokens=1024）
- 中文 token/char 比例约 1:2（1 token ≈ 2 字符）
- `inject_block_as_markdown` 注入的字符数可预估：header + separator + n_rows × n_cols × avg_cell_width
- DuckDB Text-to-SQL 路径需额外 1 次 LLM 调用（生成 SQL），但结果更精确
- 当前 `rag_generate` 中的两阶段压缩作为最终兜底仍保留

Scope:

- **Part A: 上下文预算引擎**
  - 在 `rag_search` 中计算 `max_context_chars` 预算（复用 rag_generate 同款公式）
  - 估算各数据源占用：text 切片字符数 + tabular Markdown 字符数
  - 根据预算总量决定 tabular 注入策略

- **Part B: 动态 Tabular 策略选择**
  - 策略 1（直接注入）：tabular + text 总量 ≤ 预算 → 全部 Markdown 注入
  - 策略 2（SQL 查询）：总量 > 预算 → tabular 走 DuckDB Text-to-SQL，只注入查询结果
  - 策略 3（SQL + 预算压缩）：即使 SQL 结果仍超预算 → 使用 ResultSummarizer 摘要化

- **Part C: 预算分配优先级**
  - tabular 数据优先：表格数据精确度高，分配优先
  - text 切片按匹配度排序，从高到低截取
  - 保留足够 headroom 给 system prompt 和 query

Tasks:

- [x] 7.1 提取上下文预算计算 — `compute_context_budget(config)` 共享函数，在 `rag_search` 和 `rag_generate` 中复用

- [x] 7.2 Tabular 字符数预估 — `estimate_markdown_chars(block)` 函数，采样前 10 行计算平均 cell 宽度，预估全量注入字符数

- [x] 7.3 Text 切片字符数统计 — 在 rag_search 中计算 text_chars_estimate（含扩展上下文 + 格式化开销）

- [x] 7.4 动态策略选择器 — 替换硬编码 `row_count > 50`：计算 tabular + text 总字符数，与 budget×0.9 对比，动态决定直接注入或走 SQL

- [x] 7.5 预算感知的 Text 切片截取 — Text 注入时累计字符数，超出 text_budget 后停止注入低分切片

- [x] 7.6 rag_generate 简化 — 替换重复预算计算为 compute_context_budget 调用

- [x] 7.7 日志增强 — Budget/Text/Tabular/Total/Strategy 日志 + Text budget exhausted 警告

Exit criteria:
- 硬编码的 `row_count > 50` 判断被移除，改为动态预算判断
- 小表格（如 10 行 × 3 列）即使查询涉及 Excel，也不触发 DuckDB SQL，直接 Markdown 注入
- 大表格（如 500 行 × 10 列）自动走 SQL 路径，上下文不超限
- text 切片 + tabular 数据总量精确控制在预算内，`rag_generate` 中的两阶段压缩极少触发
- 日志清晰显示预算分配决策过程
