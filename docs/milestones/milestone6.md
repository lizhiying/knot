# Milestone 6: Advanced Indexing & Search

## Current Architecture Overview (as of Milestone 5)

### 1. Index Schema

目前 Knot 使用双写架构（Dual-write），同一份文档数据会被同时写入 **LanceDB**（向量检索）和 **Tantivy**（关键词检索）。

#### LanceDB Schema (Vector Store)
- **Initialization**: `knot-core/src/store.rs` -> `KnotStore::get_schema()`
- **Table Name**: `vectors`
- **Embedding Dimension**: 512 (Mock/BGE-M3)

| Field Name    | Type                         | Description                                  |
| :------------ | :--------------------------- | :------------------------------------------- |
| `id`          | Utf8                         | Unique content hash (or UUID)                |
| `text`        | Utf8                         | The actual content text (chunk)              |
| `vector`      | FixedSizeList<Float32> [512] | Embedding vector of the `text`               |
| `file_path`   | Utf8                         | Absolute path to the source file             |
| `parent_id`   | Utf8 (Nullable)              | ID of the parent node (if hierarchical)      |
| `breadcrumbs` | Utf8 (Nullable)              | Context path (e.g., "Chapter 1 > Section 2") |

#### Tantivy Schema (Keyword Store)
- **Initialization**: `knot-core/src/store.rs` -> `KnotStore::ensure_tantivy_index()`
- **Tokenizer**: `jieba` (Chinese support), `LowerCaser`, `RemoveLongFilter(40)`, `StopWordFilter`.

| Field Name    | Type   | Options         | Description                         |
| :------------ | :----- | :-------------- | :---------------------------------- |
| `id`          | STRING | STORED          | Unique ID (Exact match only)        |
| `file_path`   | STRING | STORED          | Absolute path (Exact match only)    |
| `text`        | TEXT   | INDEXED, STORED | Main content (Tokenized with Jieba) |
| `parent_id`   | STRING | STORED          | Parent ID (Exact match only)        |
| `breadcrumbs` | STRING | STORED          | Context path (Exact match only)     |

### 2. Data Flow & Initialization

#### Initialization Logic
1.  **Store Creation**: `main.rs` calls `KnotStore::new(path)`.
2.  **Tantivy Init**: `KnotStore::new` calls `ensure_tantivy_index` to register the schema and Jieba tokenizer.
3.  **LanceDB Init**: Lazy initialization in `add_records`. If table `vectors` doesn't exist, it creates it using the Arrow schema.

#### Indexing Pipeline
The indexing process is driven by `KnotIndexer` (`knot-core/src/index.rs`):
1.  **Scanning**: `index_directory` walks the folder structure.
2.  **Parsing**: `pageindex_rs` parses files (Markdown/PDF) into a `PageNode` tree.
3.  **Enrichment**: `enrich_node` calculates embeddings for each node's content.
4.  **Flattening**: `flatten_tree` converts the tree into a list of `VectorRecord`s.
5.  **Storage**: `KnotStore::add_records` takes `Vec<VectorRecord>` and writes to both DBs.

### 3. Sample Data Snapshot

A typical record representing a paragraph in a Markdown file:

```json
{
  "id": "a1b2c3d4...",
  "text": "Knot allows you to search your local documents using both vector and keyword search.",
  "vector": [0.012, -0.045, 0.113, ...], // 512 floats
  "file_path": "/Users/lizhiying/Documents/Project_Knot.md",
  "parent_id": "root_node_id_123",
  "breadcrumbs": "Introduction > Features"
}
```

## Proposed Improvements for Milestone 6

Based on the current structure, we have identified the following gaps:

1.  **Filename Search**: `file_path` is currently `STRING` (exact match). Users cannot search for "Knot" to find "Project_Knot.md".
2.  **Vector Context**: Embeddings only use `text`. Filename semantics are lost in vector space.

### Planned Changes (Milestone 6)

#### 1. Advanced Multilingual Search (Dual Indexing)
为了兼顾中文精准分词和全球语言支持，采用**双重索引策略**：
- [ ] **Schema Design**:
    -   `file_name` (Index: ICU, Stored: Yes): 文件名强匹配。
    -   `path_tags` (Index: Simple, Stored: Yes): 路径语义标签。
    -   `text_zh` (Index: Jieba, Stored: No): 负责中文语义搜索。
    -   `text_std` (Index: ICU, Stored: No): 负责通用多语言搜索。
    -   `content` (Index: No, Stored: Yes): 仅用于结果展示（Snippet）。

#### 2. Metadata & Structure Boosting
采用**Multi-field Boost**策略提升相关性：
- [ ] **Ranking Strategy**:
    -   **File Name**: Boost **3.0** (最高权重，ICU 分词)
    -   **Path Tags**: Boost **1.5** (中等权重，Simple 分词)
    -   **Content** (`text_zh` OR `text_std`): Boost **1.0** (基础权重)
    -   *Logic*: Score = `file_name` * 3.0 + `path_tags` * 1.5 + (`text_zh` OR `text_std`) * 1.0

#### 3. Path Processing Logic (Smart Tags)
为了生成高质量的 `path_tags`，实施以下处理：
- [ ] **Prefix Stripping**: 自动切除系统通用前缀 (e.g., `~/`, `/Users/xxx/`, `C:\`).
- [ ] **Depth Analysis**: 提取靠近文件的 2-3 层父目录 (e.g., `.../Rust/Knot/src/main.rs` -> "Knot, Rust").
- [ ] **Project Root Detection**: 自动识别包含 `.git`, `Cargo.toml`, `.project` 的目录为项目根，提取从根开始的路径。
- [ ] **RAG Enhancement**: 利用路径信息进行搜索结果**聚合** (Collapse by Project) 和**多模态推断** (e.g. assets 目录关联图片)。

#### 4. Vector Context Injection
- [ ] **Content Injection**: 在写入 LanceDB 前，将文件名注入到每个 Chunk 的头部，保留文档级上下文。
    -   **Format**: `Document Title: [file_name]\n\n[original_content]`
    -   **Motivation**: 确保即使 Chunk 正文未提及关键术语（如项目名），也能通过头部注入的元数据被召回。
