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

| 元素类型     | 事件                                 | content 中的格式                               |
| ------------ | ------------------------------------ | ---------------------------------------------- |
| **Heading**  | `Start/End(Heading)`                 | 构建树层级，文本写入 `title` + `content`       |
| **段落**     | `Start/End(Paragraph)`               | 段落之间保证 `\n\n` 空行分隔                   |
| **纯文本**   | `Text(text)`                         | 原样追加                                       |
| **代码块**   | `Start/End(CodeBlock)`               | 包裹 `` ```lang ... ``` ``                     |
| **换行**     | `SoftBreak / HardBreak`              | 追加 `\n`                                      |
| **无序列表** | `Start/End(List)`                    | 整体前后 `\n`                                  |
| **列表项**   | `Start/End(Item)`                    | 前缀 `"- "`                                    |
| **表格**     | `Table/TableHead/TableRow/TableCell` | `"                                             | cell1 | cell2 \n"` 格式 |
| **图片**     | `Start(Image)`                       | `"![title](url)"`                              |
| **HTML**     | `Html / InlineHtml`                  | 原样保留                                       |
| **其他**     | `_`                                  | 忽略（如 Emphasis, Strong, Link 等不影响结构） |

> **注意**：加粗 `**bold**`、斜体 `*italic*`、链接 `[text](url)` 等行内格式标记会被忽略，但其内部的 `Text` 事件仍会被捕获。也就是说文本内容会保留，但样式标记（星号、方括号等）会丢失。

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

## knot-parser 中的使用

`knot-parser`（原 `pageindex-rs`）通过两种方式使用 `knot-markdown`：

### 1. 解析 .md 文件

```
.md 文件 → fs::read_to_string()
         → knot_markdown::parse_text()
         → MarkdownNode
         → 转换为 PageNode（补充 metadata）
```

转换代码位于 [knot-parser/src/formats/md.rs](../knot-parser/src/formats/md.rs)。

### 2. PDF 语义树构建

```
PDF → knot-pdf Pipeline → DocumentIR（每页 blocks）
    → MarkdownRenderer（每页渲染为 Markdown）
    → knot_markdown::build_from_pages()
    → MarkdownNode 树
    → 转换为 PageNode 树
```

转换代码位于 [knot-parser/src/core/tree_builder.rs](../knot-parser/src/core/tree_builder.rs)。

### MarkdownNode → PageNode 转换

```rust
fn markdown_node_to_page_node(mn: MarkdownNode) -> PageNode {
    PageNode {
        node_id: mn.node_id,
        title: mn.title,
        level: mn.level,
        content: mn.content,
        summary: None,           // ← 上层填充
        embedding: None,         // ← 上层填充
        metadata: NodeMeta {
            file_path: ...,
            page_number: None,   // ← 上层填充
            line_number: None,
            token_count: mn.token_count,
            extra: HashMap::new(),
        },
        children: mn.children.into_iter()
            .map(markdown_node_to_page_node).collect(),
    }
}
```

---

## 已知局限

1. **Token 计数不准确**：`count_tokens()` 按空格分词，对中文等无空格语言严重低估
2. **行内格式丢失**：加粗、斜体、链接等标记被忽略（文本保留，标记丢失）
3. **有序列表**：统一用 `"- "` 前缀，原始序号信息丢失
4. **嵌套列表**：不区分嵌套层级，所有列表项统一为 `"- "`
5. **heading 中的格式**：如 `# **Bold** Title` 中的加粗标记会出现在事件流中但被忽略，title 仍能正确捕获文本
