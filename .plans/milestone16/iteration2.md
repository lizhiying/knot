Milestone: milestone16 - Knowledge 页面
Iteration: Iteration 2 — 索引管理（详情 + 重新索引 + 排除文件）

Goal:
实现文件索引管理的核心操作：点击文件查看索引详情（chunk 列表）、手动重新索引、将文件加入忽略列表。
让用户能精细管理索引数据，控制哪些文件参与 RAG 搜索。

Assumptions:
- 文件详情面板采用右侧侧边栏形式（左右分栏，左文件列表 + 右详情），复用 Iteration 1 的布局
- `get_file_chunks` 直接查 Tantivy 索引，返回 chunk 列表（id、text 预览、breadcrumbs）
- `reindex_file` 复用现有 `KnotIndexer.index_file()`，无需新建解析逻辑
- 忽略列表在 `AppConfig.ignored_files` 中持久化（JSON 文件）
- `ignore_file` 操作同步清理所有存储（KnotStore + FileRegistry + EntityGraph）
- `index_directory` 和 `DirectoryWatcher` 需检查忽略列表，跳过命中文件
- 重新索引操作为同步/awaitable，完成后前端刷新状态

Scope:
后端: 4 个新 Tauri command + KnotStore 扩展 + 索引/监控集成忽略列表
前端: FileDetail 面板 + 操作按钮 + 忽略列表管理 + 事件监听

Tasks:
- [x] 2.1 后端: 实现 `KnotStore.get_file_chunks()` 方法
  - 在 Tantivy 中按 `file_path` 精确匹配查询所有 chunk
  - 返回 `Vec<ChunkSummary>` { id, preview(前200字), breadcrumbs }
  - 过滤 doc-summary 记录（id 以 `-doc-summary` 结尾的跳过）
  - 修改: `knot-core/src/store.rs`

- [x] 2.2 后端: 实现 `get_file_index_detail` Tauri command
  - 入参: `file_path: String`
  - 调用 `KnotStore.get_file_chunks()` 获取 chunk 列表
  - 查询 `FileRegistry` 获取 `indexed_at` 和 `content_hash`
  - 返回 `FileIndexDetail` { file_path, chunk_count, chunks, indexed_at, content_hash }
  - 修改: `knot-app/src-tauri/src/main.rs`

- [x] 2.3 后端: 实现 `reindex_file` Tauri command
  - 入参: `file_path: String`
  - 流程: 删除旧索引 (`KnotStore.delete_file`) → 清除注册表 (`FileRegistry.remove_file`) → 重新解析 (`KnotIndexer.index_file`) → 写入新索引 (`KnotStore.add_records`) → 更新注册表 hash → 如果启用 GraphRAG 则更新实体图
  - 需获取 `AppState` 中的 embedding provider 来构造 `KnotIndexer`
  - 发送 `indexing-status` 事件通知前端
  - 修改: `knot-app/src-tauri/src/main.rs`

- [x] 2.4 后端: 实现 `ignore_file` 和 `unignore_file` Tauri command
  - `ignore_file(file_path)`:
    1. 加入 `AppConfig.ignored_files` → 保存配置
    2. `KnotStore.delete_file()` 删除索引记录
    3. `FileRegistry.remove_file()` 删除注册记录
    4. 如果 GraphRAG 启用: `EntityGraph.delete_by_file()` 删除实体
  - `unignore_file(file_path)`:
    1. 从 `AppConfig.ignored_files` 移除 → 保存配置
    2. 不自动重新索引（用户可手动触发）
  - 修改: `knot-app/src-tauri/src/main.rs`

- [x] 2.5 后端: 索引/监控集成忽略列表
  - `index_directory()`: walkdir 扫描时检查 `ignored_files`，跳过命中文件
  - `DirectoryWatcher` 事件处理: 收到文件变更时检查忽略列表，命中则跳过
  - 忽略列表从 `AppConfig` 读取（需传入 `app: AppHandle` 来 `load_config`）
  - 修改: `knot-app/src-tauri/src/main.rs`（`start_background_indexing` 函数内）

- [x] 2.6 前端: 创建 `FileDetail.svelte` 文件详情面板
  - 右侧侧边栏，点击文件列表中的已索引文件时打开
  - 顶部: 文件名 + 文件类型图标 + 索引状态 badge
  - 信息区: 文件路径、大小、索引时间、content hash
  - Chunk 列表: 可折叠列表，每个 chunk 显示 breadcrumbs 路径 + text 预览（前 200 字）
  - 操作按钮区: 「重新索引」「排除文件」
  - 加载状态: 骨架屏 loading
  - 对不支持索引的文件: 显示"该文件类型暂不支持索引"提示
  - 修改: 新建 `knot-app/src/lib/components/Knowledge/FileDetail.svelte`，修改 `Knowledge.svelte` 集成

- [x] 2.7 前端: 排除文件交互 + 忽略列表管理
  - 排除操作: 点击「排除文件」→ 弹出确认对话框 → 调用 `ignore_file` → 刷新列表
  - 排除后的文件显示为灰色 + 删除线（Ignored 状态）
  - 忽略列表管理: 在 Knowledge 页面添加「已忽略文件」展开区域或 tab
  - 恢复操作: 忽略列表中每个文件旁有「恢复」按钮 → 调用 `unignore_file` → 刷新
  - 修改: `Knowledge.svelte`, `FileDetail.svelte`

Exit criteria:
- 点击已索引文件，右侧显示详情面板（chunk 列表 + 元数据）
- 「重新索引」按钮点击后文件被重新解析，状态更新
- 「排除文件」操作后文件从索引中删除，状态变为 Ignored
- 忽略列表中的文件可通过「恢复」按钮恢复
- 后台索引和文件监控自动跳过忽略列表中的文件
- 可 demo: 查看详情 → 重新索引 → 排除文件 → 忽略列表恢复
