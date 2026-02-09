# Milestone 7: Knot RAG 功能评估系统

## 核心目标

构建一个 RAG（检索增强生成）效果评估系统，用于量化测试 Knot 的问答质量。

---

## 功能模块

| 模块           | 描述                               |
| -------------- | ---------------------------------- |
| **QA 生成器**  | 为每个测试文档自动生成问答对       |
| **RAG 调用器** | 调用 Knot RAG 功能回答问题         |
| **评估引擎**   | 评估回答的准确性与引用一致性       |
| **报告生成器** | 生成 HTML 评估报告和 JSON 结果数据 |

---

## 技术规格

### 1. QA 问答对结构

```json
{
  "id": "doc01_q03",
  "doc": "docs/install.md",
  "question": "安装时需要哪些前置依赖？",
  "answer_gold": "需要安装 Node.js 18+ 和 pnpm。",
  "evidence_gold": [
    {
      "doc": "docs/install.md",
      "heading": "## 安装前准备",
      "quote": "需要 Node.js 18 及以上版本，并建议使用 pnpm"
    }
  ],
  "type": "extractive",
  "must_refuse": false
}
```

**字段说明：**
- `answer_gold`：期望答案（尽量短、可抽取）
- `evidence_gold.quote`：原文证据（评测引用一致性）
- `type`：题目类型
  - `extractive`：抽取型
  - `multi_hop`：多跳推理
  - `table`：表格类
  - `code`：代码类
  - `refusal`：拒答类
- `must_refuse`：用于"文档里不存在"的题（测幻觉与拒答）

### 2. 题目分布（200-400 题）

| 类型     | 占比 | 说明                   |
| -------- | ---- | ---------------------- |
| 抽取型   | 70%  | 答案可在引用中直接找到 |
| 多段合并 | 20%  | 同一文档不同小节拼合   |
| 拒答题   | 10%  | 文档中不存在的问题     |

**抽取型问题模板：**
- "X 的默认值是什么？"
- "Y 支持哪些模式/选项？"
- "Z 的限制条件是什么？"
- "如何开启/关闭某功能？对应的配置项是什么？"
- "命令行参数 --abc 的含义是什么？"

**拒答题模板：**
- "是否支持 Windows XP？"
- "是否提供企业版 SLA？"
- "是否支持某数据库（文档没提）？"

> 要求系统：没找到就明确说"未在文档中发现"，并给出检索到的最相关片段（而不是编造）。

### 3. 评估指标

| 指标           | 说明                            | 计算方式                              |
| -------------- | ------------------------------- | ------------------------------------- |
| **引用一致性** | 答案是否能被引用片段支撑        | 答案关键短语/数值是否出现在引用片段中 |
| **Recall@k**   | gold evidence 是否在 top-k 里   | 检索命中率                            |
| **MRR**        | 第一次命中 gold 的排名          | 平均倒数排名                          |
| **拒答正确率** | must_refuse=true 时是否正确拒答 | 包含明确拒答语句且不编造答案          |

---

## RAG 输出规范

RAG 系统需返回结构化输出：

```json
{
  "answer": "最终答案（1-4句）",
  "citations": [
    {
      "doc_path": "docs/install.md",
      "heading": "## 安装前准备",
      "quote": "原文1-3句"
    }
  ],
  "refused": false,
  "confidence": 0.85
}
```

**规则：**
- 最终答案：1-4 句
- 引用列表：最多 3 条，每条含 `doc_path`, `heading`, `quote`
- 未找到时：`refused: true` + 固定拒答语句 + 最相关引用

---

## 实现步骤

### Step 1: 准备测试数据
- 测试文档位于 `/Users/lizhiying/Projects/knot/source/knot-test-docs/documents/`

### Step 2: 创建评测数据
- 建立 `test/eval/eval.jsonl` 存放测试题
- 每行一个 JSON 对象

### Step 3: 编写评测驱动器
```
遍历题目 → 调用 Knot RAG 接口 → 获取 answer_pred 和 citations_pred → 计算分数
```

### Step 4: 生成输出

#### 4.1 JSON 结果文件 (`eval_result.json`)

```json
{
  "summary": {
    "total_questions": 300,
    "overall_accuracy": 0.85,
    "citation_consistency": 0.92,
    "refusal_accuracy": 0.78,
    "recall_at_3": 0.88,
    "mrr": 0.76
  },
  "by_type": {
    "extractive": { "count": 210, "accuracy": 0.90 },
    "multi_hop": { "count": 60, "accuracy": 0.75 },
    "refusal": { "count": 30, "accuracy": 0.78 }
  },
  "by_document": [
    {
      "doc": "docs/install.md",
      "question_count": 15,
      "accuracy": 0.87
    }
  ],
  "results": [
    {
      "id": "doc01_q03",
      "doc": "docs/install.md",
      "question": "安装时需要哪些前置依赖？",
      "answer_gold": "需要安装 Node.js 18+ 和 pnpm。",
      "answer_pred": "需要 Node.js 18 及以上版本和 pnpm。",
      "citations_pred": [...],
      "scores": {
        "answer_match": 0.95,
        "citation_hit": true,
        "citation_consistency": 0.90
      },
      "passed": true
    }
  ],
  "failures": [
    {
      "id": "doc02_q05",
      "reason": "citation_mismatch",
      "question": "...",
      "answer_gold": "...",
      "answer_pred": "...",
      "evidence_gold": [...],
      "citations_pred": [...]
    }
  ]
}
```

#### 4.2 HTML 报告文件 (`eval_report.html`)

HTML 报告需包含：

1. **概览面板**
   - 总体准确率（大数字展示）
   - 各核心指标卡片

2. **按题型统计**
   - 饼图/柱状图展示各类型准确率

3. **按文档统计**
   - 表格展示每个文档的表现

4. **详细结果表格**
   - 可搜索、可排序
   - 显示：问题、预期答案、实际答案、分数、状态

5. **失败样例分析**
   - 高亮显示 Top 失败案例
   - 展示完整的预测答案、预测引用、gold 证据对比

6. **改进建议**
   - 基于失败模式自动生成优化建议

---

## 输出文件清单

| 文件               | 格式  | 用途                             |
| ------------------ | ----- | -------------------------------- |
| `eval.jsonl`       | JSONL | 输入：测试题目集                 |
| `eval_result.json` | JSON  | 输出：详细评测结果，供程序使用   |
| `eval_report.html` | HTML  | 输出：可视化评测报告，供人工查看 |

---

## 迭代优化

失败样例是迭代 chunk/embedding/prompt 的燃料：

1. 分析失败模式
2. 调整分块策略
3. 优化 embedding 参数
4. 改进 prompt 模板
5. 重新运行评测
