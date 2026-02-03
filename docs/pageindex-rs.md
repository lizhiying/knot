**pageindex-rs** 设计为一个解耦、高性能的 Rust 库，是确保 **Knot** 能够应对万级文档挑战的关键。

以下是为你准备的 **`pageindex-rs` 核心代码定义**和**库需求文档 (RD)**。

---

## 第一部分：pageindex-rs 核心定义 (Rust)

我们将采用“插件化”设计。`PageIndex` 负责调度，具体的格式解析器负责将文件转化为统一的 `PageNode` 树。

### 1. 核心数据结构

```rust
use serde::{Deserialize, Serialize};    
use std::collections::HashMap;

/// PageNode 是 PageIndex 逻辑树的最小单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageNode {
    pub node_id: String,             // 唯一标识符（如 "0001"）
    pub title: String,               // 标题或节点名称
    pub level: u32,                  // 树层级（1 为根，2 为子章节...）
    pub content: String,             // 节点的原始文本或 Markdown
    pub summary: Option<String>,     // LLM 生成的摘要（可选）
    pub embedding: Option<Vec<f32>>, // 节点的向量表示
    pub metadata: NodeMeta,          // 节点的元数据
    pub children: Vec<PageNode>,     // 子节点
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub file_path: String,
    pub page_number: Option<u32>, // PDF 专用
    pub line_number: Option<u32>, // MD/Word 专用
    pub token_count: usize,
    pub extra: HashMap<String, String>, // 存储坐标、图片 OCR 等额外信息
}
```

### 2. 外部能力接口 (Trait)

pageindex-rs 核心库只定义接口，具体的模型实现（Vision, LLM, Embedding）由外部（如 Knot App）注入。

#### A. 视觉引擎接口 (VisionProvider)

```rust
pub trait VisionProvider: Send + Sync {
    /// 核心接口：将图像直接解析为结构化数据
    /// 支持两种返回模式：
    /// 1. 结构化 Markdown（适合端到端模型如 Qwen-VL）
    /// 2. 原始布局元素（适合 Florence-2 这种需要后端重组的模型）
    fn process_page(&self, image_bytes: &[u8]) -> Result<VisionOutput, PageIndexError>;
}

/// 视觉输出的枚举：兼容不同模型的输出风格
pub enum VisionOutput {
    /// 模式 A：模型直接给出了带结构的 Markdown 内容（如 OCRFlux-3B）
    StructuredMarkdown(String),
    
    /// 模式 B：模型只给出了坐标和标签，需要 pageindex-rs 进一步处理（如 Florence-2）
    LayoutElements(Vec<LayoutElement>),
}
```

#### B. LLM 与 Embedding 接口

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 核心接口：输入文本，输出摘要
    async fn generate_summary(&self, text: &str) -> Result<String, PageIndexError>;

    /// 通用接口：输入 Prompt，输出内容
    async fn generate_content(&self, prompt: &str) -> Result<String, PageIndexError>;

    /// 多模态接口：输入 Prompt 和图片数据，输出内容
    async fn generate_content_with_image(
        &self,
        prompt: &str,
        image_data: &[u8],
    ) -> Result<String, PageIndexError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// 核心接口：输入文本，输出向量
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, PageIndexError>;
}
```

### 3. 解析器接口与配置

#### A. 核心解析 Trait (DocumentParser)

这是所有插件（MD, Office, PDF）必须实现的接口。它确保了无论底层使用什么库，输出都是统一的 `PageNode`。

```rust
#[async_trait]
pub trait DocumentParser: Send + Sync {
    /// 检查该解析器是否能处理指定后缀的文件
    fn can_handle(&self, extension: &str) -> bool;

    /// 核心解析函数
    /// input: 文件路径
    /// config: 包含 VisionProvider 引用及其他转换参数
    async fn parse(
        &self,
        path: &std::path::Path,
        config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError>;
}
```

#### B. 全局配置结构体 (PageIndexConfig)

该配置通过生命周期 `'a` 持有外部注入的引擎引用，避免了库内部管理重型模型的开销。

```rust
pub struct PageIndexConfig<'a> {
    /// 外部注入的视觉引擎（可选）
    pub vision_provider: Option<&'a dyn VisionProvider>,
    
    /// 外部注入的 LLM 引擎（可选）
    pub llm_provider: Option<&'a dyn LlmProvider>,

    /// 外部注入的 Embedding 引擎（可选）
    pub embedding_provider: Option<&'a dyn EmbeddingProvider>,

    /// 节点合并阈值（以 Token 数为单位）
    pub min_token_threshold: usize,
    
    /// 触发摘要生成的最小 Token 数
    pub summary_token_threshold: usize,

    /// 摘要生成开关
    pub enable_auto_summary: bool,

    /// 文档语言偏好
    pub default_language: String,
}
```

#### C. 调度器实现 (IndexDispatcher)

调度器是库的入口，负责根据文件类型路由到不同的解析器，并处理树优化（Thinning）、摘要生成和向量化。

```rust
pub struct IndexDispatcher {
    parsers: Vec<Box<dyn DocumentParser>>,
}

impl IndexDispatcher {
    pub fn new() -> Self {
        Self {
            parsers: vec![
                Box::new(MarkdownParser::new()),
                Box::new(crate::formats::docx::DocxParser::new()), // 封装 undoc
                Box::new(crate::formats::pdf::PdfParser::new()),
            ],
        }
    }

    /// 外部调用的主入口
    pub async fn index_file(&self, path: &std::path::Path, config: &PageIndexConfig<'_>) -> Result<PageNode, PageIndexError> {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        
        // 1. 路由到具体的解析器
        let parser = self.parsers.iter()
            .find(|p| p.can_handle(ext))
            .ok_or(PageIndexError::UnsupportedFormat(ext.to_string()))?;

        // 2. 执行解析得到原始树
        let mut root_node = parser.parse(path, config).await?;

        // 3. 树优化：执行 Thinning 逻辑，合并碎片节点
        self.apply_tree_thinning(&mut root_node, config.min_token_threshold);
        
        // 4. (Pipeline) 生成摘要
        self.inject_summaries(&mut root_node, config).await;
        
        // 5. (Pipeline) 生成向量
        self.inject_embeddings(&mut root_node, config).await;

        Ok(root_node)
    }
}
```

### 4. 关键解析器实现细节

#### MarkdownParser
- **算法**: 使用基于栈（Stack）的状态机处理 Markdown AST。
- **层级处理**: 完美支持 `#` 到 `######` 的任意嵌套缩进。
- **Token 计算**: 实时计算每个节点的 Token 数，为后续的 Thinning 做准备。

#### DocxParser (Office)
- **核心逻辑**: 采用 "Convert-to-Markdown" 策略。
  1. 使用 `undoc` 库读取 `.docx` 的底层 XML 结构。
  2. 提取段落、标题、列表和表格。
  3. **TOC 增强**: 通过 Regex 智能识别 Word 中的目录页（"Start......1"），并将其转化为 Markdown 表格，防止目录污染全文索引。
  4. 生成中间态 Markdown 文本。
  5. 复用 `MarkdownParser` 将中间文本转化为语义树 `PageNode`。

---

## 第二部分：pageindex-rs 需求文档 & 进度

### 1. 项目目标

构建一个纯 Rust 编写的高性能、多模态文档解析库。其核心任务是**“将非结构化文档转化为结构化语义树”**。

### 2. 核心功能需求与状态

#### 语义树构建
* 将长文档转化为层级清晰的 JSON 树，支持按章节、按摘要检索。

#### F1：多格式层级解析
*   `[Done]` **Markdown:** 完美识别 `#` 层级，支持代码块保留。
*   `[Done]` **Word (.docx):** 通过 `undoc` 集成，支持标题层级提取和 TOC 识别。
*   `[TODO]` **PDF (Vision-based):** 视觉解析模块仍在开发中，使用 **OCRFlux-3B.Q4_K_M** 模型对每一页进行视觉解析。

#### F2：树优化 (Tree Optimization)
*   `[Done]` **节点合并 (Thinning):** 支持递归合并 token 数过少的叶子节点。
*   `[Done]` **Pipeline 集成:** 摘要生成和 Embedding 生成已集成到 Dispatcher 流程中。

---

## 第三部分：Knot 如何集成 pageindex-rs

在 **Knot** 的 Tauri 项目中，集成流程如下：

1. **启动阶段：** Knot 初始化 AI 引擎（OCRFlux-3B, Embedding Model 等）。
2. **实现接口：** Knot 实现 `VisionProvider`, `LlmProvider`, `EmbeddingProvider` trait。
3. **调用：**
   ```rust
   let config = PageIndexConfig::new()
       .with_vision_provider(&my_vision_engine)
       .with_llm_provider(&my_llm_engine)
       .with_embedding_provider(&my_embedding_engine);
       
   let dispatcher = IndexDispatcher::new();
   let tree = dispatcher.index_file(path, &config).await?;
   ```
4. **存储：** 将返回的 `PageNode` 写入向量数据库。