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

- [ ] 7.1 提取上下文预算计算 — 将 `rag_generate` 中的 `max_context_chars` 计算逻辑提取为
  独立函数 `compute_context_budget(config) -> usize`，在 `rag_search` 和 `rag_generate` 中复用。
  公式：`(context_size / 2 - max_tokens - prompt_overhead) * chars_per_token`

- [ ] 7.2 Tabular 字符数预估 — 添加函数 `estimate_markdown_chars(block: &DataBlock) -> usize`，
  根据列数、行数、样本 cell 宽度估算注入为 Markdown 表格后的字符数。
  公式：`(header_line + sep_line + row_count * avg_row_width) + metadata_overhead`

- [ ] 7.3 Text 切片字符数统计 — 在 `rag_search` 中，收集所有 text 切片的字符总数
  `text_chars_total`，包括格式化开销（`[序号] 文件: ... 内容: ...`）

- [ ] 7.4 动态策略选择器 — 在 `rag_search` 中 tabular 处理入口替换硬编码 `row_count > 50` 判断：
  ```
  let budget = compute_context_budget(&config);
  let tabular_chars = blocks.iter().map(|b| estimate_markdown_chars(b)).sum();
  let text_chars = text_results.iter().map(|r| estimate_text_entry_chars(r)).sum();
  
  if tabular_chars + text_chars <= budget * 90% {
      // 策略 1: 全量 Markdown 注入
      inject_all_blocks_as_markdown(...)
  } else {
      // 策略 2: DuckDB Text-to-SQL
      run_text_to_sql(...)
  }
  ```

- [ ] 7.5 预算感知的 Text 切片截取 — Text 切片注入时，累计字符数不超过
  `budget - tabular_used_chars`。超出预算的低分 text 切片不注入，
  避免 `rag_generate` 中的截断丢失有用信息。

- [ ] 7.6 rag_generate 简化 — `rag_search` 已保证上下文不超限后，`rag_generate` 中的
  两阶段压缩逻辑理论上不再触发（作为兜底保留），但移除不必要的 `max_context_chars` 重复计算。

- [ ] 7.7 日志增强 — 添加预算分配日志，方便调试和验证：
  ```
  [rag_search] Budget: 13736 chars
  [rag_search] Text slices: 3 items, 4200 chars
  [rag_search] Tabular estimate: 2 blocks, 8500 chars -> FITS (direct inject)
  或
  [rag_search] Tabular estimate: 2 blocks, 18000 chars -> EXCEEDS (using SQL)
  ```

Exit criteria:
- 硬编码的 `row_count > 50` 判断被移除，改为动态预算判断
- 小表格（如 10 行 × 3 列）即使查询涉及 Excel，也不触发 DuckDB SQL，直接 Markdown 注入
- 大表格（如 500 行 × 10 列）自动走 SQL 路径，上下文不超限
- text 切片 + tabular 数据总量精确控制在预算内，`rag_generate` 中的两阶段压缩极少触发
- 日志清晰显示预算分配决策过程
