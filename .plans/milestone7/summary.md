# Milestone 7 完成总结

## 🎯 目标

构建 RAG 评估系统，优化搜索性能和用户体验。

---

## ✅ 完成内容

### 1. 两阶段 RAG 架构

将原有的单一 `rag_query` 命令拆分为两个独立命令：

| 命令 | 职责 | 耗时 |
|------|------|------|
| `rag_search` | 向量+关键词混合搜索 | ~100ms |
| `rag_generate` | LLM 生成回答 | ~5-7s |

**优势**：搜索结果可立即展示，无需等待 LLM 生成完成。

---

### 2. 搜索性能优化

#### KnotStore 缓存
- **问题**：每次搜索重新初始化 LanceDB + Tantivy (~1.1s)
- **方案**：在 `AppState` 中缓存 `KnotStore` 实例，启动时预热
- **效果**：初始化开销从 ~1.1s 降至 ~0ms

#### Tantivy Index 缓存
- **问题**：每次搜索重建 Schema、注册 Jieba 分词器 (~836ms)
- **方案**：在 `KnotStore` 中缓存 Tantivy Index 实例
- **效果**：关键词搜索从 836ms 降至 <50ms

#### 性能对比

| 阶段 | 优化前 | 优化后 | 提升 |
|------|--------|--------|------|
| Store 初始化 | 1100ms | ~0ms | ∞ |
| Vector 搜索 | 36ms | 36ms | - |
| Keyword 搜索 | 836ms | <50ms | **17x** |
| **总搜索时间** | **~900ms** | **<100ms** | **9x** |

---

### 3. UI 体验优化

#### 分步骨架屏加载
- 搜索中：左右两侧均显示骨架屏
- 搜索完成：左侧显示结果，右侧继续骨架屏
- 生成完成：右侧显示 AI 回答

#### 搜索耗时显示
- 左侧 "HYBRID EVIDENCE" 标题旁显示搜索耗时（如 `0.8s`）
- 右侧 "AI INSIGHT" 标题显示生成耗时（如 `5.2s`）

---

### 4. 评估系统

- `eval_api.rs`：评估 API 接口
- `test/eval/eval.jsonl`：测试数据集
- `test/eval/run_eval.py`：评估脚本
- `test/start_test_server.sh`：测试服务器启动脚本

---

## 📁 修改文件

### 后端 (Rust)
- `knot-app/src-tauri/src/main.rs` - 两阶段 RAG 命令、KnotStore 缓存
- `knot-core/src/store.rs` - Tantivy Index 缓存
- `knot-app/src-tauri/src/eval_api.rs` - 评估 API（新增）

### 前端 (Svelte)
- `SpotlightContainer.svelte` - 分步加载状态管理
- `ResultsPanel.svelte` - Props 传递
- `EvidencePanel.svelte` - 骨架屏和耗时显示

---

## 🔮 后续优化方向

1. **LLM 生成加速**
   - 减少 `n_predict` (2048 → 512)
   - 模型预热
   - 真流式输出

2. **索引规模化**
   - LanceDB IVF-PQ 索引
   - 分片搜索

3. **评估系统完善**
   - 更多测试用例
   - 自动化回归测试

---

## 📊 关键指标

| 指标 | 数值 |
|------|------|
| 搜索延迟 | <100ms |
| LLM 生成 | 5-7s |
| 混合搜索结果数 | 10 条 |
| GPU 层数 | 99 (全卸载) |
