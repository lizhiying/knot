# Milestone 3: 体验优化与解析重构 (User Experience Optimization & Parser Refactoring)

## 1. PDF 树状结构重构 (PDF Tree Structure)

**现状 (Current Status)**:
`pageindex-rs/src/formats/pdf.rs` 目前是基于 **“物理页面” (Physical Pages)** 来构建树状结构的，而不是基于内容的逻辑结构（如章节标题）。

```text
Root (PDF 文件名)
├── Page 1 (节点)
├── Page 2 (节点)
├── Page 3 (节点)
└── ...
```

**需求 (Requirement)**:
希望根据提取出来的 H1, H2 标题来重构树（类似 MarkdownParser 的逻辑）。
需要在提取完所有页面的文本后，再进行一次 **“作为整体”** 的 Markdown 结构化解析 (Parsing/Chunking)，从而生成基于语义的层级结构。

## 2. 模型下载优化 (Model Download Optimization)

**现状 (Current Status)**:
现在下载模型时，虽然会把所有模型都下载下来，但是采用的是 **串行下载 (One by One)** 策略，速度较慢。

**需求 (Requirement)**:
需要优化下载逻辑，支持 **并发下载 (Concurrent Download)** 所有的模型文件。
如果技术上可行，最好支持 **多线程下载 (Multi-threaded Download)** 单个文件以进一步提升速度。
