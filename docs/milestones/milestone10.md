# Milestone 10: 搜索质量优化

## 问题背景

当前搜索系统存在以下问题：

### 问题 1: 随机字符串也能搜出结果

输入无意义的字符串（如 "safsf"），向量搜索仍然返回 5 个结果，分数都是 50。

**根本原因**：
1. 向量搜索 (LanceDB `nearest_to`) 总是返回"最近"的 N 个结果，即使它们实际上完全不相关
2. 向量搜索结果被赋予**固定分数 50**，而不是使用实际的相似度距离
3. 没有基于相似度阈值过滤低质量结果

**问题代码** (`knot-core/src/store.rs` 第 344-347 行)：
```rust
for mut c in candidates {
    c.score = 50.0;  // ❌ 固定分数 50！
    results_map.insert(c.id.clone(), c);
}
```

## 优化目标

| 目标                 | 当前状态            | 期望状态               |
| :------------------- | :------------------ | :--------------------- |
| 无意义查询返回空结果 | 返回 5 个不相关结果 | 返回 0 个结果          |
| 向量搜索分数         | 固定 50 分          | 真实相似度分数 (0-100) |
| 低质量结果过滤       | 无过滤              | 相似度 < 阈值时过滤    |
| 混合搜索融合         | 简单相加            | 基于 RRF 或权重融合    |

## 实施方案

### Phase 1: 使用真实向量距离分数 (P0)

**任务**：从 LanceDB 获取实际的距离分数，并转换为相似度分数

| 任务                | 复杂度 | 说明                                                 |
| :------------------ | :----- | :--------------------------------------------------- |
| 获取 `_distance` 列 | 低     | LanceDB 向量搜索自动返回 `_distance` 列              |
| 距离转相似度        | 低     | 使用公式：`similarity = max(0, 100 - distance * 50)` |
| 更新分数赋值逻辑    | 低     | 替换固定 `c.score = 50.0`                            |

**距离转相似度公式说明**：
- LanceDB 默认使用 **L2 (欧氏距离)** 或 **Cosine Distance**
- L2 距离范围：`[0, ∞)`，距离越小越相似
- 转换公式：`similarity = max(0, 100 - distance * 50)`
  - 距离 = 0 → 相似度 = 100
  - 距离 = 1 → 相似度 = 50
  - 距离 = 2 → 相似度 = 0

### Phase 2: 添加相似度阈值过滤 (P0)

**任务**：过滤掉相似度过低的结果

| 任务               | 复杂度 | 说明                               |
| :----------------- | :----- | :--------------------------------- |
| 添加向量距离阈值   | 低     | 默认阈值：距离 > 1.5 的结果被过滤  |
| 添加最小相似度阈值 | 低     | 默认阈值：相似度 < 30 的结果被过滤 |
| 配置化阈值         | 中     | 支持通过环境变量或配置文件调整     |

**阈值选择依据**：
- BGE 模型的 L2 距离通常：
  - 高度相关：0.0 - 0.8
  - 中度相关：0.8 - 1.2
  - 弱相关：1.2 - 1.5
  - 不相关：> 1.5

### Phase 3: 优化混合搜索融合 (P1)

**任务**：改进向量搜索和关键词搜索的融合策略

| 任务                              | 复杂度 | 说明                                  |
| :-------------------------------- | :----- | :------------------------------------ |
| 实现 RRF (Reciprocal Rank Fusion) | 中     | 基于排名的融合算法，更鲁棒            |
| 调整向量/关键词权重               | 低     | 向量 0.6 + 关键词 0.4                 |
| 标准化 BM25 分数                  | 中     | Tantivy BM25 分数范围不固定，需标准化 |

**RRF 公式**：
```
RRF_score = Σ 1 / (k + rank_i)
```
其中 `k` 通常取 60，`rank_i` 是文档在第 i 个检索器中的排名。

### Phase 4: 智能空结果处理 (P1)

**任务**：当搜索质量过低时，返回有意义的空结果

| 任务              | 复杂度 | 说明                                           |
| :---------------- | :----- | :--------------------------------------------- |
| 检测无意义查询    | 中     | 向量距离太大 且 关键词无匹配                   |
| 返回空结果 + 提示 | 低     | 返回 `{ results: [], hint: "未找到相关内容" }` |
| 前端显示优化      | 低     | 空结果时显示友好提示                           |

## 代码修改计划

### 1. `knot-core/src/store.rs` - search 方法

```rust
// Phase 1: 获取真实距离并转换
pub async fn search(
    &self,
    query_vector: Vec<f32>,
    query_text: &str,
) -> Result<Vec<SearchResult>> {
    // ...
    
    // 1. LanceDB Vector Search - 获取 _distance 列
    if table_names.contains(&self.table_name) {
        let table = self.conn.open_table(&self.table_name).execute().await?;
        let vec_query = table.query().nearest_to(query_vector)?;
        let vec_results_stream = vec_query.limit(20).execute().await?;
        let vec_results_batches: Vec<RecordBatch> = vec_results_stream.try_collect().await?;
        
        // 改进：使用真实距离计算相似度
        let candidates = self.batches_to_results_with_distance(vec_results_batches);
        
        for c in candidates {
            // Phase 2: 过滤低质量结果
            if c.distance > VECTOR_DISTANCE_THRESHOLD {
                continue; // 跳过距离过大的结果
            }
            
            // 距离转相似度
            let similarity = (100.0 - c.distance * 50.0).max(0.0);
            let mut result = c.result;
            result.score = similarity;
            results_map.insert(result.id.clone(), result);
        }
    }
    
    // ...
}
```

### 2. 新增常量

```rust
// 向量距离阈值：距离 > 此值的结果被过滤
const VECTOR_DISTANCE_THRESHOLD: f32 = 1.5;

// 最终相似度阈值：分数 < 此值的结果被过滤
const MIN_SIMILARITY_THRESHOLD: f32 = 30.0;
```

### 3. 新增方法：`batches_to_results_with_distance`

```rust
struct CandidateWithDistance {
    result: SearchResult,
    distance: f32,
}

fn batches_to_results_with_distance(&self, batches: Vec<RecordBatch>) -> Vec<CandidateWithDistance> {
    let mut candidates = Vec::new();
    for batch in batches {
        // ... 解析各列
        
        // 获取 _distance 列 (LanceDB 自动添加)
        let distances = batch
            .column_by_name("_distance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
        
        for i in 0..batch.num_rows() {
            let distance = distances.map(|d| d.value(i)).unwrap_or(f32::MAX);
            candidates.push(CandidateWithDistance {
                result: SearchResult { ... },
                distance,
            });
        }
    }
    candidates
}
```

## 测试用例

### 测试 1: 随机字符串查询

```bash
# 输入: "safsf" (无意义字符串)
# 期望: 返回 0 个结果
knot-cli query -t "safsf"
# 输出: No results found.
```

### 测试 2: 正常查询

```bash
# 输入: "如何使用 Rust 的生命周期"
# 期望: 返回高质量结果，相似度 > 50
knot-cli query -t "如何使用 Rust 的生命周期"
# 输出: [1] ch10-03-lifetime-syntax.md (Score: 85.2) ...
```

### 测试 3: 部分匹配查询

```bash
# 输入: "rustbook" (部分匹配文件名)
# 期望: 通过关键词搜索匹配，即使向量相似度一般
knot-cli query -t "rustbook"
# 输出: [1] rust-book-cn/... (Score: 72.1, Source: Keyword) ...
```

## 预期效果

| 查询类型           | 修改前                  | 修改后                           |
| :----------------- | :---------------------- | :------------------------------- |
| 随机字符串 "safsf" | 返回 5 个结果 (分数 50) | 返回 0 个结果                    |
| 中文语义查询       | 返回结果 (分数 50 固定) | 返回结果 (分数 0-100 真实相似度) |
| 精确关键词         | 返回结果 (混合)         | 返回结果 (更高分数，来源标记)    |
| 完全不相关         | 返回向量最近邻          | 返回空结果 + 提示                |

## 实施优先级

1. **Phase 1 + 2** (必须): 真实分数 + 阈值过滤 → 解决核心问题
2. **Phase 3** (推荐): 混合搜索优化 → 提升搜索质量
3. **Phase 4** (可选): 空结果处理 → 改善用户体验

## 参考资料

- [LanceDB Vector Search](https://lancedb.github.io/lancedb/basic/#vector-search)
- [Reciprocal Rank Fusion](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf)
- [Tantivy BM25 Scoring](https://docs.rs/tantivy/latest/tantivy/query/trait.Scorer.html)

---

## 实现摘要 (2026-02-09)

### 最终实现的核心算法

#### 1. 向量距离阈值过滤
- **默认阈值**: `0.75` (可通过设置页面调整 0.5-2.0)
- **环境变量对比**: `KNOT_DISTANCE_THRESHOLD` (仅日志对比，不覆盖设置)
- **距离转相似度公式**: `similarity = max(0, 100 - distance * 50)`
- **阈值选择依据**: 随机字符串的向量距离约为 0.8-0.86，阈值 0.75 可过滤无关结果

#### 2. RRF 融合算法
```rust
// RRF 公式: score = w_vec * (1 / (k + vec_rank)) + w_kw * (1 / (k + kw_rank))
const RRF_K: f32 = 60.0;           // RRF 常数
const VECTOR_WEIGHT: f32 = 0.6;    // 向量搜索权重 60%
const KEYWORD_WEIGHT: f32 = 0.4;   // 关键词搜索权重 40%
```

#### 3. BM25 分数标准化
- 使用 min-max 标准化将 Tantivy BM25 分数映射到 0-100 范围
- 公式: `normalized = (score - min) / (max - min) * 100`

### 边缘情况处理

| 边缘情况            | 处理方式                          |
| :------------------ | :-------------------------------- |
| 空查询              | 直接返回空结果，不调用向量搜索    |
| 超长查询 (>500字符) | 截断到 500 字符后处理             |
| 无相关结果          | 前端显示友好提示 "未找到相关结果" |

### 涉及的代码文件

1. `knot-core/src/store.rs` - 核心搜索算法
2. `knot-app/src-tauri/src/main.rs` - Tauri 命令和配置管理
3. `knot-app/src/lib/components/Settings.svelte` - 阈值配置 UI
4. `knot-app/src/lib/components/Spotlight/EvidencePanel.svelte` - 空结果提示 UI

### 配置选项

| 配置项                          | 默认值 | 说明                                                   |
| :------------------------------ | :----- | :----------------------------------------------------- |
| `vector_distance_threshold`     | 0.75   | 向量搜索距离阈值，距离大于此值的结果被过滤（设置页面） |
| `KNOT_DISTANCE_THRESHOLD` (env) | -      | 环境变量，仅用于日志对比，**不覆盖**设置页面的值       |

