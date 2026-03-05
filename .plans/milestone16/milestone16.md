Milestone: milestone16 - Knowledge 页面（文件索引管理与知识探索）

单句总结: 实现 Knowledge 页面，展示目标目录下所有文件的索引状态，支持查看索引详情、重新索引、排除文件和单文件 RAG 聊天。

Iterations:
- Iteration 1: 端到端最小可用 — 后端文件扫描 + 前端文件列表 + 索引状态展示
- Iteration 2: 索引管理 — 文件详情面板 + 重新索引 + 排除文件 + 忽略列表
- Iteration 3: 单文件聊天 + 体验打磨 — 限定文件 RAG 聊天 + 动画 + 快捷键 + 空状态

前置条件:
- [x] Milestone 14 已完成（RAG 搜索系统 + 上下文扩展 + 多跳检索）
- [x] Milestone 15 已完成（GraphRAG 知识图谱增强）
- [x] Settings 页面已有 data_dir 配置
- [x] KnotIndexer + FileRegistry + DirectoryWatcher 系统已稳定

索引范围（本 milestone）:
- ✅ .md, .txt, .html, .pdf — 内容解析 + 索引
- ❌ .docx, .pptx, .xlsx, .csv, 图片 — 仅在列表中展示，不做内容索引

状态: 未开始

详细文档: docs/milestones/milestone16.md
