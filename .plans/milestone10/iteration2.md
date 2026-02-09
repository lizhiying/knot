Milestone: milestone10 - 搜索质量优化
Iteration: iteration2 - 风险消除与调优

## Goal
消除最大风险，替换脆弱的硬编码：调优阈值参数，改进混合搜索融合策略。

## Assumptions
1. Iteration 1 已完成，基础距离分数和过滤已可用
2. 需要实际数据验证阈值是否合适
3. RRF 融合比简单分数相加效果更好

## Scope
- ✅ 调优距离阈值（基于实际测试数据）
- ✅ 实现 RRF (Reciprocal Rank Fusion) 融合
- ✅ 标准化 BM25 分数
- ✅ 支持设置页面配置阈值（环境变量仅用于日志对比）
- ❌ 不修改前端显示（Iteration 3）

## Tasks
- [x] 3. 实现 RRF 融合算法 (k=60)
- [x] 4. 标准化 Tantivy BM25 分数到 0-100 范围
- [x] 5. 调整向量/关键词权重 (向量 0.6 + 关键词 0.4)
- [x] 6. 添加环境变量 `KNOT_DISTANCE_THRESHOLD`（仅日志对比，设置页面优先）

## Exit Criteria
1. 有效查询的 Precision@5 > 80%
2. 无效查询返回空结果的准确率 > 95%
3. 混合搜索结果优于纯向量或纯关键词搜索
4. 阈值可通过设置页面配置

## Demo
对比演示：
1. 展示 RRF 融合 vs 简单相加的结果排序差异
2. 展示不同阈值对结果的影响
