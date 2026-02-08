Milestone: milestone7 - RAG 评估系统
Iteration: 2 - 完善评估逻辑与指标

Goal:
扩展评测数据集，实现完整的评估指标体系，包括引用一致性、检索命中率、拒答正确率。

Assumptions:
- Iteration 1 已完成，基础流程可用
- RAG 接口能返回 citations 信息
- 需要 LLM 辅助生成更多 QA 数据

Scope:
- 扩展 QA 数据集到 50-100 题
- 实现完整评估指标
- 分类统计（按题型、按文档）
- 失败样例收集

Tasks:
- [ ] 编写 QA 生成器脚本 `test/eval/generate_qa.py`
- [ ] 为 `knot-test-docs/documents/` 中每个文档生成 QA
- [ ] 实现引用一致性评分逻辑
- [ ] 实现检索命中率计算（Recall@k, MRR）
- [ ] 实现拒答正确率检测
- [ ] 扩展 JSON 结果结构（by_type, by_document）
- [ ] 收集并输出 Top 失败样例

Exit criteria:
- QA 数据集达到 50+ 题，覆盖抽取型、多跳、拒答三类
- JSON 结果包含所有核心指标：准确率、引用一致性、Recall@k、拒答率
- 能够查看按题型、按文档的分类统计
- 失败样例列表可用于后续分析
