# knot-markdown 技术文档

## 概述

`knot-markdown` 是 knot 生态中的 Markdown 解析 crate，负责将 Markdown 文本解析为结构化的 **heading 树**（`MarkdownNode`）。它是一个纯解析层，不涉及 PDF/DOCX 等格式，也不关心 embedding、摘要等上层逻辑。

### 在 knot 生态中的位置

```
                    knot-app / knot-cli
                         │
                    knot-core（索引引擎）
                         │
                    knot-parser（文档解析调度器）
                    ┌────┼────┐
               knot-pdf  │  knot-markdown ← 本文档
                         │
                      undoc（DOCX）
```

`knot-parser` 通过 `knot-markdown` 解析 `.md` 文件，也利用它将 PDF 转出的 Markdown 内容构建为语义树。

### 依赖

| 依赖             | 版本 | 用途                                          |
| ---------------- | ---- | --------------------------------------------- |
| `pulldown-cmark` | 0.10 | CommonMark 规范的 Markdown 解析器，生成事件流 |
| `serde`          | 1.0  | `MarkdownNode` 的序列化/反序列化支持          |
| `thiserror`      | 2.0  | 错误类型定义                                  |

---

## 核心数据结构

### MarkdownNode

```rust
pub struct MarkdownNode {
    pub node_id: String,              // 唯一标识符
    pub title: String,                // heading 的文本（如 "Introduction"）
    pub level: u32,                   // 层级：0=root, 1=H1, 2=H2, ...
    pub content: String,              // 节点下的全部文本内容（含 title 自身）
    pub token_count: usize,           // 大约的 token 数（空格分词估算）
    pub children: Vec<MarkdownNode>,  // 子 heading 节点
}
```

**设计特点**：

- **轻量**：不包含 `embedding`、`summary`、`page_number` 等字段，这些由上层 `knot-parser` 转换为 `PageNode` 时补充
- **content 包含 title**：heading 的文本同时存储在 `title` 和 `content` 中，`content` 代表该 heading 节点下的完整原始文本
- **树形结构**：通过 `children` 形成层级关系，`level` 决定父子关系

### 树结构示例

对于以下 Markdown：

```markdown
# 第一章
第一章内容
## 1.1 节
1.1 内容
## 1.2 节
1.2 内容
# 第二章
第二章内容
```

解析结果：

```
root (L0, title="文档名")
├── "第一章" (L1)
│   ├── content: "第一章\n第一章内容\n"
│   ├── "1.1 节" (L2)
│   │   └── content: "1.1 节\n1.1 内容\n"
│   └── "1.2 节" (L2)
│       └── content: "1.2 节\n1.2 内容\n"
└── "第二章" (L1)
    └── content: "第二章\n第二章内容\n"
```

---

## 公开 API

### `parse_text()` — 解析单个 Markdown 文档

```rust
pub fn parse_text(
    content: &str,              // Markdown 文本
    title: &str,                // 文档标题（用于 root 节点）
    file_path: impl Into<String>, // 文件路径（用于生成 node_id）
) -> Result<MarkdownNode, MarkdownError>
```

**适用场景**：解析 `.md` 文件。

**node_id 格式**：`"{file_path}-{counter}"` ，如 `"readme.md-1"`, `"readme.md-2"`

### `build_from_pages()` — 解析多页文档

```rust
pub fn build_from_pages(
    root_title: String,         // 文档标题
    file_path: String,          // 文件路径
    pages: Vec<PageContent>,    // 多页内容
) -> MarkdownNode
```

**适用场景**：PDF 经 `knot-pdf` 转为每页 Markdown 后，合并构建跨页语义树。

**关键特性**：多页共享同一个栈，heading 可以跨页连续。例如第 1 页的 `# Introduction` 的内容可以延续到第 2 页，直到遇到下一个同级或更高级 heading。

**node_id 格式**：`"node-{counter}"`，如 `"node-1"`, `"node-2"`

### `count_tokens()` — Token 计数

```rust
pub fn count_tokens(text: &str) -> usize
```

简单按空格分割估算 token 数。对中文文本暂不准确（中文词之间无空格），但作为 RAG 分片的粗略阈值判断已够用。

---

## 解析算法详解

两个公开函数共享同一套**栈驱动的 heading 树构建算法**，分为两个阶段。

### 阶段一：pulldown-cmark 事件流

`pulldown-cmark` 将 Markdown 文本解析为 SAX 风格的事件序列：

```
输入: "# Hello\nSome text\n## Sub"

事件流:
  Start(Heading H1)
  Text("Hello")
  End(Heading H1)
  Start(Paragraph)
  Text("Some text")
  End(Paragraph)
  Start(Heading H2)
  Text("Sub")
  End(Heading H2)
```

### 阶段二：栈驱动的树构建

使用 `Vec<MarkdownNode>` 作为栈。**核心规则**：

> 遇到 H*n* heading 时，弹出栈顶所有 `level ≥ n` 的节点，将它们归入各自父节点的 `children`，然后压入新的 level=n 节点。

#### 算法流程图

```
初始状态:  stack = [root(L0)]

遇到事件:
  Start(Heading Ln) →  1. 弹出栈中所有 level ≥ n 的节点，归入 parent.children
                        2. 创建新节点 level=n，压栈
                        3. 开始捕获 title

  End(Heading)      →  停止捕获 title，追加 '\n' 到 content

  Text(text)        →  追加到栈顶节点的 content
                        如果正在捕获 title，也追加到 title

  其他元素          →  追加格式化后的文本到栈顶节点的 content

事件流结束:
  清栈: 逐个弹出所有节点，归入 parent.children
  返回 root
```

#### 详细执行示例

输入：

```markdown
# A
text-a
## B
text-b
# C
text-c
```

| 步骤 | 事件             | 操作                                                                                | 栈状态 (底→顶)         |
| ---- | ---------------- | ----------------------------------------------------------------------------------- | ---------------------- |
| 0    | —                | 初始化                                                                              | `[root(L0)]`           |
| 1    | `Start(H1)`      | L0 < L1，不弹栈；压入 A(L1)                                                         | `[root, A(L1)]`        |
| 2    | `Text("A")`      | A.title="A", A.content+="A"                                                         | `[root, A(L1)]`        |
| 3    | `End(H1)`        | A.content+="\n"                                                                     | `[root, A(L1)]`        |
| 4    | `Start(P)`       | A.content+="\n"                                                                     | `[root, A(L1)]`        |
| 5    | `Text("text-a")` | A.content+="text-a"                                                                 | `[root, A(L1)]`        |
| 6    | `End(P)`         | A.content+="\n"                                                                     | `[root, A(L1)]`        |
| 7    | `Start(H2)`      | L1 < L2，不弹栈；压入 B(L2)                                                         | `[root, A(L1), B(L2)]` |
| 8    | `Text("B")`      | B.title="B", B.content+="B"                                                         | `[root, A(L1), B(L2)]` |
| 9    | `Text("text-b")` | B.content+="text-b"                                                                 | `[root, A(L1), B(L2)]` |
| 10   | `Start(H1)`      | **弹栈**：B(L2≥1) → A.children.push(B); A(L1≥1) → root.children.push(A); 压入 C(L1) | `[root, C(L1)]`        |
| 11   | `Text("C")`      | C.title="C"                                                                         | `[root, C(L1)]`        |
| 12   | `Text("text-c")` | C.content+="text-c"                                                                 | `[root, C(L1)]`        |
| 13   | 事件流结束       | **清栈**：C → root.children.push(C)                                                 | `[root]`               |

**最终输出**：

```
root(L0)
├── A(L1) → children: [B(L2)]
│   └── B(L2) → content: "B\ntext-b\n"
└── C(L1) → content: "C\ntext-c\n"
```

---

## 支持的 Markdown 元素

两个公开函数统一处理以下 pulldown-cmark 事件：

| 元素类型     | 事件                                 | content 中的格式                            |
| ------------ | ------------------------------------ | ------------------------------------------- |
| **Heading**  | `Start/End(Heading)`                 | 构建树层级，文本写入 `title` + `content`    |
| **段落**     | `Start/End(Paragraph)`               | 段落之间保证 `\n\n` 空行分隔                |
| **纯文本**   | `Text(text)`                         | 原样追加                                    |
| **代码块**   | `Start/End(CodeBlock)`               | 包裹 `` ```lang ... ``` ``                  |
| **换行**     | `SoftBreak / HardBreak`              | 追加 `\n`                                   |
| **无序列表** | `Start/End(List(None))`              | 前缀 `"- "`，嵌套缩进 2 空格/层             |
| **有序列表** | `Start/End(List(Some(n)))`           | 前缀 `"1. "`, `"2. "`, ...，嵌套缩进 2 空格 |
| **列表项**   | `Start/End(Item)`                    | 根据列表类型选择前缀                        |
| **表格**     | `Table/TableHead/TableRow/TableCell` | `"                                          | cell1 | cell2 \n"` 格式 |
| **图片**     | `Start(Image)`                       | `"![title](url)"`                           |
| **HTML**     | `Html / InlineHtml`                  | 原样保留                                    |
| **链接**     | `Start/End(Link)`                    | `"[text](url)"` 完整保留                    |
| **加粗**     | `Start/End(Strong)`                  | `"**text**"` 保留标记                       |
| **斜体**     | `Start/End(Emphasis)`                | `"*text*"` 保留标记                         |
| **删除线**   | `Start/End(Strikethrough)`           | `"~~text~~"` 保留标记                       |
| **行内代码** | `Code(text)`                         | `` "`text`" `` 保留标记                     |

> **Heading 中的行内格式**：如 `# **Bold** Title`，`title` 和 `content` 都会保留加粗标记，即 `title = "**Bold** Title"`。

---

## `parse_text` 与 `build_from_pages` 的区别

两个函数的**事件处理逻辑完全一致**，差异仅在于：

|                  | `parse_text`                          | `build_from_pages`          |
| ---------------- | ------------------------------------- | --------------------------- |
| **输入**         | 单个 `&str`                           | `Vec<PageContent>`          |
| **Parser 创建**  | 1 个                                  | 每页创建 1 个（共享栈）     |
| **node_id**      | `"{file_path}-{n}"`                   | `"node-{n}"`                |
| **返回**         | `Result<MarkdownNode, MarkdownError>` | `MarkdownNode` （不会失败） |
| **页间分隔**     | —                                     | 每页末尾追加 `\n\n`         |
| **跨页 heading** | —                                     | ✅ 支持（栈在页之间不重置）  |

---

## 完整数据流：从文件到搜索结果

以下是一个 `.md` 文件从解析到入库到被搜索到的完整链路：

```
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │                            knot-app / knot-cli                                  │
 │                            （触发索引、发起搜索）                                │
 └──────────────────────────────┬───────────────────────────────────┬───────────────┘
                                │ index_directory()                 │ search()
                                ▼                                   ▼
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │                              knot-core                                           │
 │   KnotIndexer                                          KnotStore                 │
 │   ┌─────────────────────────────────────┐  ┌───────────────────────────────────┐ │
 │   │ ① dispatcher.index_file()           │  │ ⑥ add_records(Vec<VectorRecord>)  │ │
 │   │ ② enrich_node() → 生成 embedding   │  │   → LanceDB 写入向量             │ │
 │   │ ③ flatten_tree() → Vec<VectorRecord>│  │   → Tantivy 写入全文索引          │ │
 │   └─────────────┬───────────────────────┘  │ ⑦ search() → Vec<SearchResult>   │ │
 │                 │                           └───────────────────────────────────┘ │
 └─────────────────┼───────────────────────────────────────────────────────────────┘
                   │ ① 调用
                   ▼
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │                            knot-parser                                           │
 │   IndexDispatcher.index_file()                                                   │
 │   ┌──────────────────────────────────────────────────┐                           │
 │   │ 1. 路由到 MarkdownParser（.md）或 PdfParser      │                           │
 │   │ 2. 调用 parser.parse() → PageNode 树             │                           │
 │   │ 3. apply_tree_thinning() → 合并碎片节点          │                           │
 │   └──────────────────────────────────────────────────┘                           │
 │                                                                                  │
 │   MarkdownParser.parse()                  SemanticTreeBuilder.build_from_pages() │
 │   ┌──────────────────────┐                ┌────────────────────────────────────┐ │
 │   │ knot_markdown::       │                │ knot_markdown::                    │ │
 │   │   parse_text()        │                │   build_from_pages()               │ │
 │   │ → MarkdownNode        │                │ → MarkdownNode                     │ │
 │   │ → markdown_node_to_   │                │ → markdown_node_to_page_node()     │ │
 │   │   page_node()         │                │                                    │ │
 │   └──────────────────────┘                └────────────────────────────────────┘ │
 └──────────────────────────────────────────────────────────────────────────────────┘
```

### 第 ① 步：文件解析（knot-parser）

`IndexDispatcher` 是解析入口，根据文件扩展名路由到对应解析器。

**对于 .md 文件**：

```rust
// knot-parser/src/formats/md.rs
impl DocumentParser for MarkdownParser {
    fn can_handle(&self, extension: &str) -> bool {
        matches!(extension, "md" | "markdown")
    }

    async fn parse(&self, path: &Path, _config: &PageIndexConfig) -> Result<PageNode> {
        let content = fs::read_to_string(path)?;
        let title = path.file_stem().unwrap_or_default().to_string_lossy();
        // 调用 knot-markdown 解析
        let md_node = knot_markdown::parse_text(&content, &title, path.to_string_lossy())?;
        // 转换为 PageNode
        Ok(Self::markdown_node_to_page_node(md_node))
    }
}
```

**对于 PDF 文件**：

```rust
// knot-parser/src/core/tree_builder.rs
pub fn build_from_pages(root_title: String, file_path: String, pages: Vec<PageNode>) -> PageNode {
    // 将 PageNode 转为 knot_markdown::PageContent
    let md_pages: Vec<PageContent> = pages.iter()
        .map(|p| PageContent {
            content: p.content.clone(),
            page_number: p.metadata.page_number,
        })
        .collect();

    // 调用 knot-markdown 构建跨页 heading 树
    let md_root = knot_markdown::build_from_pages(root_title, file_path.clone(), md_pages);

    // 转换为 PageNode 树
    Self::markdown_node_to_page_node(md_root, &file_path, &pages)
}
```

### MarkdownNode → PageNode 转换

两种路径最终都通过 `markdown_node_to_page_node()` 递归转换：

```rust
fn markdown_node_to_page_node(mn: MarkdownNode) -> PageNode {
    PageNode {
        node_id: mn.node_id,
        title: mn.title,
        level: mn.level,
        content: mn.content,           // 保留完整 Markdown 内容（含行内格式）
        summary: None,                 // ← 上层（dispatcher）填充
        embedding: None,               // ← 上层（knot-core）填充
        metadata: NodeMeta {
            file_path: String::new(),
            page_number: None,         // ← PDF 流程会恢复
            line_number: None,
            token_count: mn.token_count,
            extra: HashMap::new(),
        },
        children: mn.children.into_iter()
            .map(Self::markdown_node_to_page_node).collect(),
    }
}
```

### 第 ② 步：Tree Thinning（knot-parser）

`IndexDispatcher` 在解析后执行 **tree thinning**（树瘦身），合并过小的碎片节点：

```
合并策略：
1. 后序遍历：先处理子节点
2. 如果当前节点 token 过少，且只有一个子节点 → 合并
3. 如果当前节点 token 过少，且所有子节点都是小节点 → 全部合并
4. 否则保持结构不变

合并时会重建 Markdown 格式：
  "## 子标题\n子内容" 追加到父节点的 content
```

**作用**：避免 RAG 检索时命中过于碎片化的内容。例如只有一行 "参见第 3 章" 的节点，会被合并到父节点中。

### 第 ③ 步：Embedding 生成（knot-core）

`KnotIndexer.enrich_node()` 递归遍历 `PageNode` 树，为每个有内容的节点生成 embedding 向量：

```rust
async fn enrich_node(&self, node: &mut PageNode, file_name: &str,
                     directory_tags: &str, breadcrumbs: &[String]) -> Result<()> {
    if node.embedding.is_none() && !node.content.is_empty() {
        // 构建层级上下文
        let breadcrumb_str = if breadcrumbs.is_empty() {
            String::new()
        } else {
            format!("Section: {}\n", breadcrumbs.join(" > "))
        };

        let enriched_text = format!(
            "File: {} | Path: {}\n{}{}",
            file_name, directory_tags, breadcrumb_str, node.content
        );

        let vec = self.embedding_provider.generate_embedding(&enriched_text).await?;
        node.embedding = Some(vec);
    }

    // 子节点的 breadcrumbs += 当前 title
    let mut child_breadcrumbs = breadcrumbs.to_vec();
    if !node.title.is_empty() {
        child_breadcrumbs.push(node.title.clone());
    }

    for child in &mut node.children {
        Box::pin(self.enrich_node(child, file_name, directory_tags, &child_breadcrumbs)).await?;
    }
    Ok(())
}
```

**Embedding 输入文本格式**：

```
File: readme.md | Path: docs project
Section: 第一章 > 1.1 机器学习基础
监督学习是一种..." 
```

包含三部分上下文：
1. **文件信息**：文件名 + 目录标签
2. **Breadcrumbs**：父级 heading 路径（递归累积）
3. **Content**：节点原始内容

### 第 ④ 步：展平为记录（knot-core）

`flatten_recursive()` 将树形 `PageNode` 展平为 `Vec<VectorRecord>`，每个有 embedding 的节点对应一条记录：

```rust
fn flatten_recursive(&self, node: PageNode, file_path: &str,
                     parent_id: Option<String>, mut breadcrumbs: Vec<String>,
                     records: &mut Vec<VectorRecord>) {
    let bc_string = if breadcrumbs.is_empty() {
        None
    } else {
        Some(breadcrumbs.join(" > "))
    };

    if let Some(embedding) = node.embedding {
        if !node.content.is_empty() {
            records.push(VectorRecord {
                id: current_id.clone(),
                text: node.content,          // 原始 Markdown 内容
                vector: embedding,           // embedding 向量
                file_path: file_path.to_string(),
                parent_id: parent_id.clone(),
                breadcrumbs: bc_string,      // "第一章 > 1.1 节"
            });
        }
    }

    breadcrumbs.push(current_title);
    for child in node.children {
        self.flatten_recursive(child, file_path, Some(current_id.clone()),
                               breadcrumbs.clone(), records);
    }
}
```

### 第 ⑤ 步：VectorRecord 数据结构

```rust
// knot-core/src/store.rs
pub struct VectorRecord {
    pub id: String,                    // 节点 ID（如 "readme.md-3"）
    pub text: String,                  // 原始内容（含行内 Markdown 格式）
    pub vector: Vec<f32>,              // embedding 向量（384/512 维）
    pub file_path: String,             // 源文件绝对路径
    pub parent_id: Option<String>,     // 父节点 ID
    pub breadcrumbs: Option<String>,   // 层级路径（如 "第一章 > 1.1 节"）
}
```

### 第 ⑥ 步：入库存储（KnotStore）

`KnotStore.add_records()` 将 `Vec<VectorRecord>` 写入两个存储引擎：

| 存储引擎    | 存储内容                                                        | 用途                             |
| ----------- | --------------------------------------------------------------- | -------------------------------- |
| **LanceDB** | `id`, `text`, `vector`, `file_path`, `parent_id`, `breadcrumbs` | 向量近邻搜索 + 持久化            |
| **Tantivy** | `id`, `text`, `file_path`, `breadcrumbs`, `file_name`           | 全文关键词搜索（jieba/ICU 分词） |

**Tantivy 分词器配置**：

- `jieba`：用于中文分词
- `en_knot`：英文分词（小写 + 简单拆分）
- `icu`：Unicode 国际化分词（覆盖日韩等）

### 第 ⑦ 步：搜索结果

`KnotStore.search()` 执行混合搜索（向量 + 关键词），返回 `SearchResult`：

```rust
pub struct SearchResult {
    pub id: String,
    pub text: String,                  // 原始内容（含 Markdown 格式）
    pub file_path: String,
    pub score: f32,                    // 综合排序得分
    pub parent_id: Option<String>,
    pub breadcrumbs: Option<String>,   // "第一章 > 1.1 节"
    pub source: SearchSource,          // Vector / Keyword / Hybrid
}
```

### 完整数据转换示例

输入文件 `ml.md`：

```markdown
# 机器学习
概述内容
## 监督学习
SVM 和决策树...
```

各阶段数据：

```
阶段 ①: MarkdownNode 树
  root(L0, "ml")
  └── "机器学习"(L1)
      ├── content: "**机器学习**\n概述内容\n"
      └── "监督学习"(L2)
          └── content: "监督学习\nSVM 和决策树...\n"

阶段 ②: PageNode 树（同结构，补充 metadata）

阶段 ③: enrich_node（生成 embedding）
  "机器学习" 节点:
    enriched_text = "File: ml.md | Path: docs\n机器学习\n概述内容\n"
  "监督学习" 节点:
    enriched_text = "File: ml.md | Path: docs\nSection: 机器学习\n监督学习\nSVM 和决策树...\n"
                                                  ↑ breadcrumbs 上下文

阶段 ④: VectorRecord（展平）
  Record 1: { id: "ml.md-1", text: "机器学习\n概述内容\n", breadcrumbs: None }
  Record 2: { id: "ml.md-2", text: "监督学习\nSVM 和决策树...\n", breadcrumbs: Some("机器学习") }

阶段 ⑤: 入库
  → LanceDB: 写入向量 + 元数据
  → Tantivy: 写入全文索引（jieba 分词）

阶段 ⑥: 搜索 "SVM"
  → SearchResult { text: "监督学习\nSVM 和决策树...", breadcrumbs: Some("机器学习"), ... }
```

---

## 已知局限

1. **Token 计数不准确**：`count_tokens()` 按空格分词，对中文等无空格语言严重低估

