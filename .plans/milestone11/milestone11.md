# Milestone 11: 分词深度优化与多语言搜索增强 ✅

## 目标
建立一个稳健的多语言搜索系统，通过多字段索引（Jieba 用于中文、Stemmer 用于英文、ICU 用于泛语言）和权重分级，提升中英混写和多语言文档的检索质量。

## 成功指标
- [x] 英文搜索支持词干提取（如搜 "models" 命中 "model"）。
- [x] 中英混写（如 "rust入门"）在两个字段同时命中，排名靠前。
- [x] 无效噪音（如 "s s s 入门"）被有效过滤或降权。
- [x] 泛语言（如德语、日语）在无特殊配置下可通过 ICU 字段检索。

## 核心设计
1. **多字段复制索引**：同一正文内容分发到 `text_zh` (Jieba), `text_std` (en_knot), `text_icu` (ICU)。
2. **权重分级**：`file_name` (8.0) > `text_zh` (5.0) = `file_name_std` (5.0) > `text_std` (3.0) > `path_tags` (2.0) > `text_icu` (1.0)。
3. **噪音控制**：查询预处理去重 + 多字符类型边界处理。

## 迭代规划
| 迭代            | 目标                     | 状态   |
| :-------------- | :----------------------- | :----- |
| **Iteration 1** | 基础设施与 Schema 迁移   | ✅ 完成 |
| **Iteration 2** | 多字段查询逻辑与权重调优 | ✅ 完成 |
| **Iteration 3** | 噪音控制与边缘情况处理   | ✅ 完成 |

## 相关文件
- 详细设计：[docs/milestones/milestone11.md](../../docs/milestones/milestone11.md)
- 代码位置：`knot-core/src/store.rs`, `knot-core/src/tokenizer.rs`
