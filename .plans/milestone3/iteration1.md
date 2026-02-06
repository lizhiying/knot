Milestone: milestone3
Iteration: iteration1

Goal: 实现基于语义的 PDF 树构建算法，并重构下载器以支持并发文件下载。
Assumptions: PDF 文本提取逻辑已就绪，仅需“后处理”重组树结构。Rust 异步环境支持 `join_all` 等并发原语。
Scope: `pageindex-rs` (Rust), `src-tauri` (Rust). 无 UI 变更。

Tasks:
- [x] [pageindex-rs] 实现 `SemanticTreeBuilder`：接收所有页面的 Flat Text/Nodes，基于 H1/H2 聚类生成逻辑树 <!-- id: 0 -->
- [x] [pageindex-rs] 添加 Unit Test 验证逻辑树生成算法 (Mock 数据) <!-- id: 1 -->
- [x] [src-tauri] 重构 `Downloader`：支持 `download_multiple(files: Vec<ModelConfig>)` 接口 <!-- id: 2 -->
- [x] [src-tauri] 实现并发下载逻辑 (使用 `tokio::spawn` 或 `futures::future::join_all`) <!-- id: 3 -->
- [x] [src-tauri] 调整 Progress Event：支持发送 `BatchProgress` 或独立 File ID 的进度 <!-- id: 4 -->

Exit criteria:
- `pageindex-rs` 单元测试通过，能将平铺的页面数据转换为逻辑树结构。
- 后端能同时触发多个文件的下载任务，并能在 Log 中看到交替的进度更新。
