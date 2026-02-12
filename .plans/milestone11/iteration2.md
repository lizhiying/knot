Milestone: milestone11
Iteration: iteration2 - 多字段查询逻辑与权重调优

Goal:
实现多字段加权搜索，确保不同语言的匹配结果能根据优先级（权重）正确排序，解决中英混写召回率问题。

Assumptions:
1. Iteration 1 已完成，索引中已有足够字段。 ✅
2. `QueryParser` 能够处理多字段 OR 逻辑。 ✅

Scope:
- ✅ 更新 `search` 核心逻辑，支持多字段检索。
- ✅ 设置各字段 Boost 权重。
- （CJK 写入判断推迟到 Iteration 3，当前全量写入更简单且不影响质量）

Tasks:
- [x] 1. 修改 `search` 方法，将 `QueryParser` 配置为检索所有 6 个文本字段 (`text_zh`, `text_std`, `text_icu`, `file_name`, `file_name_std`, `path_tags`)。
- [x] 2. 为每个字段设置 Boost：`file_name: 8.0`, `text_zh: 5.0`, `file_name_std: 5.0`, `text_std: 3.0`, `path_tags: 2.0`, `text_icu: 1.0`。
- [x] 3. 验证混合搜索：搜索 "rust入门" 时，结果正确命中并分数 99+。
- [x] 4. 验证英文 Stemmer：搜索 "running" 能命中含 "run" 的文档。

Exit criteria:
1. ✅ 中英混写查询能够同时从中文和英文字段召回结果。
2. ✅ 搜索排序符合预期：文件名匹配 > 中文匹配 > 英文匹配 > ICU 兜底。
3. ✅ Schema 迁移自动完成，重新索引后工作正常。

## 测试结果

| 查询       | 首位结果                   | 分数  | 来源   |
| :--------- | :------------------------- | :---- | :----- |
| `rust入门` | ch01-00-getting-started.md | 99.03 | Hybrid |
| `入门`     | ch01-00-getting-started.md | 97.19 | Hybrid |
| `running`  | ch01-03-hello-cargo.md     | 96.57 | Hybrid |

## 权重体系

```
file_name (Jieba)     = 8.0   # 文件名中文匹配最高优先
text_zh (Jieba)       = 5.0   # 中文正文
file_name_std (en_knot) = 5.0 # 文件名英文
text_std (en_knot)    = 3.0   # 英文正文 (含 Stemmer)
path_tags (default)   = 2.0   # 路径标签
text_icu (ICU)        = 1.0   # 泛语言兜底
```
