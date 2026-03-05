Milestone: milestone16 - Knowledge 页面
Iteration: Iteration 3 — 单文件 RAG 聊天 + 体验打磨

Goal:
实现已索引文件的单文件 RAG 聊天功能（仅基于该文件内容回答问题），
同时打磨整体交互体验：空状态引导、加载动画、键盘快捷键、虚拟滚动。

Assumptions:
- 单文件搜索复用现有 `KnotStore.search()` 逻辑，新增 `file_filter` 参数限定搜索范围
- LLM 生成复用现有 `rag_generate` 逻辑，只是搜索上下文限定在单文件
- 流式输出复用现有 `llm-token` 事件机制
- 虚拟滚动仅在文件数量 > 100 时才有明显价值，可用简单方案（CSS overflow + 延迟渲染）
- 上下文扩展和多跳检索在单文件模式下仍然生效（但范围限定在文件内）
- 聊天界面采用简单的单轮问答模式（非多轮对话），每次提问独立

Scope:
后端: KnotStore.search 扩展 + 2 个新 Tauri command（或复用现有 command 加参数）
前端: FileChat 组件 + 体验优化（动画/快捷键/空状态/虚拟滚动）

Tasks:
- [x] 3.1 后端: 扩展 `KnotStore.search()` 支持文件过滤
  - 新增可选参数 `file_filter: Option<&str>`
  - 向量搜索: LanceDB query 增加 `WHERE file_path = ?` 过滤条件
  - 关键词搜索: Tantivy BooleanQuery 增加 `file_path` TermQuery（Must）
  - 当 `file_filter` 为 None 时行为与现有完全一致（不破坏现有搜索）
  - 修改: `knot-core/src/store.rs`

- [x] 3.2 后端: 实现 `rag_search_file` 和 `rag_generate_file` Tauri command
  - `rag_search_file(file_path, query)`: 调用 `KnotStore.search()` 带 `file_filter`，返回 `RagSearchResponse`
  - `rag_generate_file(query, context)`: 与 `rag_generate` 逻辑相同，复用 LLM 生成逻辑
  - 或者: 给现有 `rag_search` 增加可选 `file_path` 参数，减少代码重复
  - 注册到 `invoke_handler`
  - 修改: `knot-app/src-tauri/src/main.rs`

- [x] 3.3 前端: 创建 `FileChat.svelte` 单文件聊天组件
  - 顶部: 「正在与 {filename} 对话」标题 + 关闭按钮
  - 输入区: 搜索式输入框 + 发送按钮
  - 回答区: 流式 Markdown 渲染（复用 AiInsight 的流式展示逻辑）
  - 引用来源: 展示搜索命中的 chunk（简化版 EvidenceCard）
  - 监听 `llm-token` 事件接收流式输出
  - 加载状态: 搜索中 → 生成中 → 完成
  - 修改: 新建 `knot-app/src/lib/components/Knowledge/FileChat.svelte`

- [x] 3.4 前端: 在 FileDetail 中集成聊天入口
  - FileDetail 面板底部增加「与该文件聊天」按钮
  - 点击后切换到聊天视图（替换详情区域或 overlay）
  - 返回按钮可切回详情视图
  - 修改: `knot-app/src/lib/components/Knowledge/FileDetail.svelte`, `Knowledge.svelte`

- [x] 3.5 前端: 空状态与错误处理
  - 未设置 data_dir: 显示引导卡片 "请先在设置中选择要索引的目录" + 跳转设置按钮
  - 目录为空: "该目录下没有找到任何文件"
  - 目录不存在: "目录不存在或无法访问，请重新设置"
  - 加载失败: 错误提示 + 重试按钮
  - 修改: `Knowledge.svelte`

- [x] 3.6 前端: 加载动画 + 过渡效果
  - 文件列表加载: 骨架屏动画（3-5 行的脉冲占位符）
  - 详情面板打开: 侧边栏滑入动画（transform + opacity）
  - 状态切换: badge 状态变化时的微动画
  - 文件列表虚拟滚动: 仅渲染可见区域（当文件数 > 100 时启用）
  - 修改: `Knowledge.svelte`, `FileList.svelte`, `FileDetail.svelte`

- [x] 3.7 前端: 键盘快捷键
  - ↑/↓ 方向键: 在文件列表中选择上/下一个文件
  - Enter: 查看选中文件的详情
  - Esc: 关闭详情面板，返回文件列表
  - Cmd+F / Ctrl+F: 聚焦搜索框
  - 修改: `Knowledge.svelte`, `FileList.svelte`

Exit criteria:
- 已索引文件可点击「聊天」进入单文件对话，输入问题后流式返回回答
- 回答仅基于该文件内容（搜索范围限定在 file_path）
- 空状态（未设置目录、目录为空）有友好提示和引导
- 文件列表有骨架屏加载动画
- 详情面板有滑入过渡效果
- 键盘方向键可导航文件列表，Enter 查看详情，Esc 返回
- 端到端可 demo: 选择文件 → 查看详情 → 聊天提问 → 流式回答 → 键盘导航
