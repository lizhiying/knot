Milestone: milestone11
Iteration: iteration3 - 噪音控制与边缘情况处理

Goal:
解决无效低质量结果（如单个高频字母匹配）的干扰，增强极端边界下的分词鲁棒性。

Assumptions:
1. 之前的迭代已确保基本召回和排序正常。 ✅
2. 干扰结果主要来自于对极短 token 的过度匹配。 ✅
3. QueryParser 默认为 OR 模式，无需额外配置。 ✅

Scope:
- ✅ 增强 `preprocess_query` 处理更多字符边界。
- ✅ 噪音 token 去重（短 token 只保留一次）。
- ✅ 预处理提升为 `pub`，在嵌入生成前统一调用。
- ✅ CLI 和 Tauri App 的所有搜索入口已同步更新。

Tasks:
- [x] 1. 增强 `preprocess_query`：扩展 CJK 检测范围到日韩文；处理数字与 CJK 的边界。
- [x] 2. 增强 `preprocess_query`：添加短 token 去重逻辑（长度 ≤ 2 的 token 只保留第一次出现）。
- [x] 3. 将 `preprocess_query` 改为 `pub`，供外部在生成嵌入前调用。
- [x] 4. 更新 CLI 3 个搜索入口点：在 `generate_embedding` 前调用 `preprocess_query`。
- [x] 5. 更新 Tauri App 2 个搜索入口点：在 `generate_embedding` 前调用 `preprocess_query`。
- [x] 6. 回归测试："s s s 入门" 首位命中 "入门指南" 文档（分数 99.03）。
- [x] 7. 回归测试："rust入门" 和 "running" 等已有测试场景保持正常。

Exit criteria:
1. ✅ "s s s 入门" 查询首位返回相关文档（分数 99+）。
2. ✅ 无效字符搜索不导致低相关性结果刷屏。
3. ✅ 全部 9 个单元测试通过。
4. ✅ CLI 和 Tauri App 编译通过。

## 测试结果

| 查询         | 首位结果                   | 分数  | 来源   | 说明               |
| :----------- | :------------------------- | :---- | :----- | :----------------- |
| `s s s 入门` | ch01-00-getting-started.md | 99.03 | Hybrid | ✅ 噪音去重后命中   |
| `rust入门`   | ch01-00-getting-started.md | 99.03 | Hybrid | ✅ 中英边界插入空格 |
| `running`    | ch01-03-hello-cargo.md     | 96.57 | Hybrid | ✅ Stemmer 词干匹配 |

## 关键修复

### 问题：向量和关键词使用不一致的查询文本
- **原因**：`preprocess_query` 只在 `search()` 内部调用，但向量嵌入在外部用原始文本生成
- **解决**：将 `preprocess_query` 升级为 `pub`，在所有搜索入口的嵌入生成前统一调用
- **影响点**：CLI 3 处 + Tauri App 2 处 = 共 5 处入口
