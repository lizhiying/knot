Milestone: milestone11
Iteration: iteration1 - 基础设施与 Schema 迁移

Goal:
建立新的多字段索引结构，引入 ICU 支持和增强型英文分词器，完成端到端的 Schema 自动迁移。

Assumptions:
1. ~~`tantivy-analysis-contrib` 兼容当前的 Tantivy 版本。~~ **实际不兼容，已自实现 ICU Tokenizer。**
2. ~~ICU 库在当前 macOS 环境下能正常加载。~~ **使用 icu_segmenter（纯 Rust），无系统依赖。**
3. 自动迁移（Reindex）对当前小规模测试集是可接受的。

Scope:
- ✅ 添加 ICU 相关依赖。
- ✅ 定义并注册 `en_knot` (Lowercase + Stemmer) 分词器。
- ✅ 自实现 `ICUTokenizer`（基于 `icu_segmenter::WordSegmenter`）。
- ✅ 扩展 Schema 增加 `text_icu` 和 `file_name_std` 字段。
- ✅ 实现基础的多字段数据分发。

Tasks:
- [x] 1. 在 `knot-core/Cargo.toml` 中添加 `icu_segmenter` 依赖（替代不兼容的 `tantivy-analysis-contrib`）。
- [x] 2. 在 `tokenizer.rs` 中实现 `ICUTokenizer`（基于 `icu_segmenter::WordSegmenter`）。
- [x] 3. 在 `store.rs` 中定义并注册 `en_knot` (Simple + Lowercase + Stemmer + StopWords) 分词器。
- [x] 4. 在 `store.rs` 中注册 `icu` 分词器。
- [x] 5. 扩展 Schema：新增 `text_icu` 和 `file_name_std` 字段。
- [x] 6. 改进 `create_tantivy_index` 的自动迁移检测逻辑（检测新字段存在性）。
- [x] 7. 修改 `add_records` 方法，将内容分发到新字段 (`text_icu`, `file_name_std`)。

Exit criteria:
1. ✅ 应用启动时 FTS 索引成功包含新字段。
2. ✅ 英文 Stemmer 工作正常（en_knot 分词器注册成功）。
3. ✅ ICU 分词器注册成功且不导致程序崩溃。
4. ✅ 5 个分词器测试全部通过（jieba x2, icu x3）。

## 技术决策记录

### 为什么不用 tantivy-analysis-contrib？
- `tantivy-analysis-contrib 0.12` 依赖 `tantivy-tokenizer-api v0.6`
- 我们的 `tantivy 0.22` 使用 `tantivy-tokenizer-api v0.3`
- Trait 不兼容，编译报错

### 替代方案：自实现 ICUTokenizer
- 使用 `icu_segmenter` crate（ICU4X 项目，纯 Rust）
- 基于 `WordSegmenter::new_auto()` 进行 Unicode 标准分词
- 过滤纯空白和纯标点片段
- 无系统动态库依赖，打包简单
