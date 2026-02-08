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

## 3. 索引与服务配置优化 (Indexing & Service Configuration Optimization)

**已完成 (Completed)**:

1.  **修复文件监控死循环 (Fix File Watcher Infinite Loop)**:
    - 问题：索引数据库 `knot_index.lance` 位于监控目录下，导致“写入索引 -> 触发监控 -> 再次索引”的死循环。
    - 解决：在文件监控层级直接过滤掉 `knot_index.lance` 目录及隐藏文件/目录（以 `.` 开头）。

2.  **索引存储外置 (Externalize Index Storage)**:
    - 问题：在用户文档目录下生成 `knot_index.lance` 会污染用户空间，且可能与同步软件冲突。
    - 解决：
        - 将索引统一移至 `~/.knot/indexes/<Path-MD5-Hash>/knot_index.lance`。
        - 使用源目录绝对路径的 MD5 哈希作为文件夹名，确保跨磁盘同名目录（如 `/Vol1/test` 和 `/Vol2/test`）的索引隔离。
        - 实现了 `get_index_path` 逻辑，支持不同 Workspace 自动切换对应索引。

3.  **服务端口调整 (Prevert Port Conflicts)**:
    - 问题：原默认端口 1420 (Tauri), 8080 (Parsing), 8081 (Chat) 为常用端口，易与其他开发工具冲突。
    - 解决：更改为不常用端口：
        - Frontend (Tauri): `14420`
        - Parsing LLM (OCRFlux): `18080`
        - Chat LLM (Qwen): `18081`
