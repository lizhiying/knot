# Milestone 14: RAG 增强 — 文档级摘要与上下文扩展

## 目标

针对当前 Naive RAG 架构在跨文档推理和同文档跨章节关联方面的不足（详见 [rag-limitations-analysis.md](../rag-limitations-analysis.md)），实施两项高收益、低成本的改进：

1. **P1 - 文档级摘要索引**：每个文档生成一条摘要记录参与搜索 ✅ 已完成
2. **P0 - 上下文窗口扩展**：搜索命中后自动拉取 parent/sibling 节点作为补充上下文

## 背景

当前 RAG 系统的核心问题：

```
每个 VectorRecord 是孤立的知识碎片
  ✅ 知道自己的内容、文件路径、层级位置
  ❌ 不知道文档全局、兄弟章节、跨文档关联
```

导致以下查询类型表现不佳：

| 查询类型         | 当前能力 |
| ---------------- | -------- |
| 单点事实查找     | ✅ 优秀   |
| 单文档单章节问答 | ✅ 优秀   |
| 全文档摘要/概览  | ❌ 不支持 |
| 同文档跨章节推理 | ⚠️ 弱     |
| 跨文档比较/综合  | ❌ 弱     |

## 成功标准

1. ✅ 每个被索引的文档额外生成一条文档级摘要 VectorRecord
2. 搜索命中某节点后，能自动获取其 parent 和 sibling 节点内容
3. 全文档概览类查询能命中文档摘要记录
4. 不引入额外的存储引擎或 Schema 变更
5. 所有现有测试通过 + 新增测试覆盖

---

## 任务列表

### Phase 1: 文档级摘要索引 ✅

| 任务                                          | 状态   | 说明                                        |
| --------------------------------------------- | ------ | ------------------------------------------- |
| 1.1 分析 RAG 架构局限性                       | ✅ 完成 | 输出 `docs/rag-limitations-analysis.md`     |
| 1.2 实现 `build_doc_summary()`                | ✅ 完成 | 遍历 PageNode 树，构建目录 + 各章节首句摘要 |
| 1.3 实现 `collect_outline()`                  | ✅ 完成 | 递归收集章节标题（树形缩进）+ 首行摘要      |
| 1.4 修改 `index_file()` 生成摘要 VectorRecord | ✅ 完成 | 额外的 embedding + 入库                     |
| 1.5 添加 `test_build_doc_summary` 测试        | ✅ 完成 | 验证摘要格式、缩进、首句提取                |

**摘要格式示例**：

```
[文档概览] ml.md

# 机器学习教程

目录与摘要:
- 机器学习概述: 本章介绍机器学习的基本概念和发展历史。
  - 监督学习: SVM 和决策树是常见方法。
  - 无监督学习: 聚类和降维技术。
- 深度学习: 神经网络的前馈与反向传播。
```

**VectorRecord 字段**：

| 字段          | 值                                   |
| ------------- | ------------------------------------ |
| `id`          | `"{file_path}-doc-summary"`          |
| `text`        | `"[文档概览] {file_name}\n\n{摘要}"` |
| `vector`      | 摘要文本的 embedding                 |
| `parent_id`   | `None`                               |
| `breadcrumbs` | `None`                               |

**核心代码变更**：[knot-core/src/index.rs](../../knot-core/src/index.rs)

### Phase 2: 上下文窗口扩展（P0）✅

#### 设计方案

**问题**：天真地拉取全部 parent + siblings 会导致上下文膨胀 10 倍以上。

假设一章有 10 个小节（各 350 tokens），搜索命中其中一个：

| 方案               | 返回内容                       | tokens/条 | 5 条结果总 tokens |
| ------------------ | ------------------------------ | --------- | ----------------- |
| 当前               | 仅命中节点                     | ~400      | ~2,000            |
| 天真扩展           | parent + 全部 siblings         | ~4,000 ⚠️  | ~20,000 ⚠️         |
| **方案 D（推荐）** | parent 标题 + 前后各 1 sibling | ~750      | ~4,000 ✅          |

**采用方案 D：有限扩展，增幅约 2 倍，完全可控。**

#### 扩展规则

```
搜索命中节点 X（400 tokens）
  │
  ├── parent title:    "第3章"（仅标题，~10 tokens）
  ├── prev sibling:    "3.1 实验方法"（截取前 200 tokens）
  └── next sibling:    "3.3 实验结论"（截取前 200 tokens）
  
  总计: 400 + 10 + 200 + 200 = 810 tokens（可控）
```

**关键约束**：

| 约束                     | 值            | 说明             |
| ------------------------ | ------------- | ---------------- |
| 前后 sibling 数量        | 各 1 个       | 不拉全部兄弟节点 |
| 每个 sibling 最大 tokens | 200           | 超长截断         |
| parent 内容              | 仅 title      | 不取完整 content |
| 总扩展上限               | 500 tokens/条 | 超出则按比例截断 |

#### 配置开关

在 `AppConfig` 中新增字段，**默认开启**：

```rust
// knot-app/src-tauri/src/main.rs
struct AppConfig {
    // ... existing fields ...
    /// 搜索时是否自动扩展上下文（拉取 parent/sibling 节点）
    #[serde(default = "default_context_expansion_enabled")]
    context_expansion_enabled: bool,
}

fn default_context_expansion_enabled() -> bool {
    true // 默认开启
}
```

需要配套：
- Tauri command: `set_context_expansion_enabled`
- 前端设置页面增加开关
- 搜索时读取配置决定是否扩展

#### 扩展效果对比

```
当前:
  用户: "实验结果验证了什么假设？"
  搜索 → [第3章-实验结果片段]
  LLM 只看到: "SVM 准确率 95.2%, 决策树 91.3%..."
  回答: "实验使用了 SVM 和决策树"（缺少假设信息 ❌）

改进后:
  搜索 → [第3章-实验结果片段]
       + parent: "第3章 实验验证"
       + prev: "第3章-实验方法: 本文验证假设H1(时间复杂度)和H2(准确率)..."
       + next: "第3章-实验结论: 实验验证了H1但否定了H2..."
  LLM 看到完整上下文
  回答: "实验结果验证了H1（时间复杂度假设），但否定了H2（准确率假设）"（✅）
```

#### 任务分解

| 任务                                              | 状态   | 文件                             | 说明                                            |
| ------------------------------------------------- | ------ | -------------------------------- | ----------------------------------------------- |
| 2.1 `AppConfig` 新增 `context_expansion_enabled`  | ✅ 完成 | `knot-app/src-tauri/src/main.rs` | 默认 `true`，配套 Tauri command                 |
| 2.2 `KnotStore` 添加 `get_text_by_id()`           | ✅ 完成 | `knot-core/src/store.rs`         | 根据 id 查询单条记录文本                        |
| 2.3 `KnotStore` 添加 `get_records_by_parent_id()` | ✅ 完成 | `knot-core/src/store.rs`         | 根据 parent_id + file_path 查询同级节点         |
| 2.4 实现 `expand_search_context()`                | ✅ 完成 | `knot-core/src/store.rs`         | 组装 parent title + prev/next sibling，截断控制 |
| 2.5 `rag_search`/`rag_query` 集成上下文扩展       | ✅ 完成 | `knot-app/src-tauri/src/main.rs` | 搜索后根据配置调用 `expand_search_context()`    |
| 2.6 `SearchResult` 新增 `expanded_context` 字段   | ✅ 完成 | `knot-core/src/store.rs`         | 存放扩展的上下文文本                            |
| 2.7 前端 `rag_search` 透传扩展上下文              | ✅ 完成 | `knot-app/src-tauri/src/main.rs` | 扩展内容拼入 LLM 的 context 格式中              |
| 2.8 前端设置页面增加开关                          | ✅ 完成 | `knot-app/src/`                  | UI 开关组件（LLM 配置区域）                     |
| 2.9 添加测试                                      | ✅ 完成 | `knot-core/src/store.rs`         | truncate_text + SearchResult 默认值验证         |

---

### Phase 3: 多跳检索（P2）✅

#### 设计方案

**方案 B：关键词扩展**，始终执行两轮搜索，无需条件判断。

```
搜索流程:
  第 1 轮: 正常搜索 "实验结果"
    → 命中 [第3章-实验结果: "SVM准确率95.2%, 决策树91.3%"]

  提取关键词: "支持向量机", "决策树", "准确率", "分支", "神经网络"

  第 2 轮: 搜索 "实验结果 支持向量机 决策树 准确率 分支 神经网络"
    → 命中 [第2章-相关工作: "基线方法使用逻辑回归"]（新结果）
    → 命中 [第3章-实验结果]（重复，去重）

  合并去重: 3 条 → 2 条独立结果
```

**性能影响**：额外 ~100ms（1 次 embedding + 1 次搜索），总耗时约 2 倍。

#### 任务分解

| 任务                                      | 状态   | 文件                             | 说明                                          |
| ----------------------------------------- | ------ | -------------------------------- | --------------------------------------------- |
| 3.1 `AppConfig` 新增 `multi_hop_enabled`  | ✅ 完成 | `knot-app/src-tauri/src/main.rs` | 默认 true，配套 Tauri command                 |
| 3.2 实现 `extract_key_terms()`            | ✅ 完成 | `knot-core/src/store.rs`         | Jieba 分词，过滤停用词和查询词，返回 top 5    |
| 3.3 实现 `merge_search_results()`         | ✅ 完成 | `knot-core/src/store.rs`         | 按 id 去重，保留高分，按分数降序排列          |
| 3.4 `rag_search`/`rag_query` 集成两轮搜索 | ✅ 完成 | `knot-app/src-tauri/src/main.rs` | 始终两轮，扩展查询+合并                       |
| 3.5 前端设置页面增加"多跳检索"开关        | ✅ 完成 | `Settings.svelte`                | UI 开关组件                                   |
| 3.6 添加测试                              | ✅ 完成 | `knot-core/src/store.rs`         | extract_key_terms + merge_search_results 测试 |

---

## 涉及文件

| 文件                                          | 变更类型 | Phase | 说明                                       |
| --------------------------------------------- | -------- | ----- | ------------------------------------------ |
| `knot-core/src/index.rs`                      | 已修改   | 1     | `build_doc_summary()`, `collect_outline()` |
| `knot-core/src/store.rs`                      | 已修改   | 2, 3  | 上下文扩展 + 多跳检索工具方法              |
| `knot-app/src-tauri/src/main.rs`              | 已修改   | 2, 3  | `AppConfig` 扩展 + 搜索流程集成            |
| `knot-app/src/lib/components/Settings.svelte` | 已修改   | 2, 3  | "上下文扩展" + "多跳检索"开关              |
| `docs/rag-limitations-analysis.md`            | 已创建   | —     | RAG 架构局限性分析                         |
| `docs/milestones/milestone14.md`              | 已创建   | —     | 本文件                                     |

## 后续方向（P3-P4）

以下为更长期的改进方向，不在本 milestone 范围内：

| 方向            | 复杂度 | 说明                            |
| --------------- | ------ | ------------------------------- |
| P3 - 文档引用图 | ⭐⭐⭐    | 解析 `[text](url)` 建立引用关系 |
| P4 - GraphRAG   | ⭐⭐⭐⭐⭐  | 构建知识图谱，实体-关系推理     |
