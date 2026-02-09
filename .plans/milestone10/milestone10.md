# Milestone 10: 搜索质量优化

## 目标

**让搜索系统返回有意义的结果**：无意义查询返回空结果，有效查询返回真实相似度分数。

## 完成状态: ✅ 已完成

## 成功指标

- [x] 输入随机字符串 "safsf" 返回 0 个结果
- [x] 有效查询返回真实相似度分数 (0-100)
- [x] 低质量结果被过滤（通过可配置的距离阈值）

## 核心问题

当前向量搜索无条件返回结果，且分数固定为 50，导致：
- 无意义查询也返回 5 个结果
- 无法区分高质量和低质量结果

## 迭代完成情况

| 迭代        | 目标                                    | 状态     |
| :---------- | :-------------------------------------- | :------- |
| Iteration 1 | 端到端验证：使用真实距离分数 + 基础过滤 | ✅ 已完成 |
| Iteration 2 | 风险消除：调优阈值 + 混合搜索融合       | ✅ 已完成 |
| Iteration 3 | 质量完善：前端优化 + 边缘情况处理       | ✅ 已完成 |

---

## Iteration 1: 端到端验证 ✅

### 完成内容
- [x] 新增 `CandidateWithDistance` 结构体
- [x] 新增 `batches_to_results_with_distance` 方法提取 LanceDB `_distance` 列
- [x] 实现距离到相似度分数转换：`score = 100 - distance * 50`
- [x] 添加可配置距离阈值过滤
- [x] 设置页面新增阈值滑块 UI (0.5-1.0)

### 技术细节
- **距离公式**: L2 (欧几里得距离)
- **分数转换**: `similarity = max(0, 100 - distance * 50)`
- **默认阈值**: 0.75 (过滤距离 > 0.75 的结果)

---

## Iteration 2: RRF 混合搜索融合 ✅

### 完成内容
- [x] 实现 RRF (Reciprocal Rank Fusion) 融合算法
- [x] 标准化 Tantivy BM25 分数到 0-100 范围
- [x] 调整向量/关键词权重 (向量 60% + 关键词 40%)
- [x] 环境变量 `KNOT_DISTANCE_THRESHOLD` 支持（仅日志对比）

### RRF 融合公式
```
RRF(d) = Σ 1 / (k + rank(d))
```
- **k = 60** (标准 RRF 常数)
- **向量权重**: 0.6
- **关键词权重**: 0.4

### 搜索结果来源标记
| Source  | 含义                 |
| :------ | :------------------- |
| Vector  | 仅向量搜索匹配       |
| Keyword | 仅关键词搜索匹配     |
| Hybrid  | 两者都匹配（高质量） |

---

## Iteration 3: 用户体验优化 ✅

### 完成内容
- [x] 前端空结果友好提示："未找到相关内容，请尝试其他关键词"
- [x] 空查询直接返回空结果
- [x] 超长查询截断处理 (> 500 字符)
- [x] 搜索性能日志输出

### 新增 LLM 配置功能
- [x] `llm_context_size`: LLM 上下文窗口大小（默认 8192，需重启生效）
- [x] `llm_max_tokens`: 最大生成 token 数（默认 1024）
- [x] `llm_think_enabled`: Think 模式开关（默认关闭）
- [x] 动态计算最大上下文字符数
- [x] 两阶段上下文压缩策略

### 中英文分词优化
- [x] 添加 `preprocess_query` 函数
- [x] 自动在中英文边界插入空格
- [x] 例：`"rust入门"` → `"rust 入门"`

---

## 最终配置

### 默认值
| 配置项                      | 默认值 | 说明           |
| :-------------------------- | :----- | :------------- |
| `vector_distance_threshold` | 0.75   | 向量距离阈值   |
| `llm_context_size`          | 8192   | LLM 上下文窗口 |
| `llm_max_tokens`            | 1024   | 最大生成 token |
| `llm_think_enabled`         | false  | Think 模式     |

### 上下文计算公式
```
max_context_chars = (context_size / 2 - max_tokens - 300) * 2
默认：(8192 / 2 - 1024 - 300) * 2 = 5144 字符
```

---

## 修改的文件

| 文件                                                              | 修改内容                                |
| :---------------------------------------------------------------- | :-------------------------------------- |
| `knot-core/src/store.rs`                                          | RRF 融合、距离过滤、分词预处理          |
| `knot-core/src/llm.rs`                                            | `spawn_with_context`、`max_tokens` 参数 |
| `knot-app/src-tauri/src/main.rs`                                  | AppConfig 扩展、LLM 配置命令            |
| `knot-app/src/lib/components/Settings.svelte`                     | LLM 配置 UI                             |
| `knot-app/src/lib/components/Spotlight/SpotlightContainer.svelte` | 空结果检测                              |

---

## 相关文档

- 详细设计：[docs/milestones/milestone10.md](../../docs/milestones/milestone10.md)
- 代码位置：`knot-core/src/store.rs` - `search` 方法
