Milestone: milestone10 - 搜索质量优化
Iteration: iteration1 - 端到端验证

## Goal
实现最小可运行的搜索质量优化：使用真实向量距离分数，过滤低质量结果，让随机字符串查询返回空结果。

## Assumptions
1. LanceDB 向量搜索返回的 RecordBatch 包含 `_distance` 列
2. L2 距离阈值 1.5 是合理的初始值（可在 Iteration 2 调优）
3. 不需要修改前端接口，只修改后端返回的分数和结果数量

## Scope
- ✅ 修改 `knot-core/src/store.rs` 的 `search` 方法
- ✅ 新增 `batches_to_results_with_distance` 方法
- ✅ 添加距离阈值常量
- ❌ 不实现 RRF 融合（Iteration 2）
- ❌ 不配置化阈值（Iteration 2）
- ❌ 不优化前端显示（Iteration 3）

## Tasks
- [x] 1. 调研 LanceDB RecordBatch 的 `_distance` 列格式
- [x] 2. 新增 `CandidateWithDistance` 结构体和 `batches_to_results_with_distance` 方法
- [x] 3. 添加 `VECTOR_DISTANCE_THRESHOLD` 常量 (初始值 1.5)
- [x] 4. 修改 `search` 方法：获取距离 → 转换分数 → 过滤低质量结果
- [x] 5. 添加设置页面阈值配置 UI（滑块 0.5-3.0）
- [x] 6. 修改 search 方法签名，支持传入阈值参数

## Exit Criteria
1. 输入 "safsf" 返回 0 个结果
2. 输入 "如何使用 Rust 的生命周期" 返回结果，分数范围 0-100（非固定 50）
3. 搜索时间无明显增加（< 100ms）

## Demo
在应用中演示：
1. 输入随机字符串，显示 "未找到相关结果"
2. 输入有效查询，显示带真实分数的结果列表
