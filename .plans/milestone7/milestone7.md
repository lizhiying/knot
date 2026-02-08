# Milestone 7: RAG 评估系统

## 里程碑目标
构建一个完整的 RAG 效果评估系统，用于量化测试 Knot 的问答质量，输出 JSON 结果数据和 HTML 可视化报告。

## 成功指标
- 能够对测试文档集自动生成 QA 问答对
- 能够调用 Knot RAG 接口执行评测
- 输出结构化的 JSON 评测结果
- 输出美观的 HTML 评估报告
- 核心指标：引用一致性、检索命中率、拒答正确率

## 核心流程
```
测试文档 → QA生成 → 调用RAG → 评估打分 → 生成报告(JSON+HTML)
```

## 迭代规划

| 迭代 | 目标 | 状态 |
|------|------|------|
| [Iteration 1](./iteration1.md) | 端到端流程验证 | 待开始 |
| [Iteration 2](./iteration2.md) | 完善评估逻辑与指标 | 待开始 |
| [Iteration 3](./iteration3.md) | HTML 报告与质量优化 | 待开始 |

## 约束条件
- 测试文档位于 `/Users/lizhiying/Projects/knot/source/knot-test-docs/documents/`
- 评估脚本和结果位于 `test/eval/`
- RAG 接口可通过 Tauri 命令 `rag_query` 调用（需要 App 运行）
- CLI 的 Query 命令目前使用 mock embedding，需要增强
- 报告需支持中文显示
