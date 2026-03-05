Milestone: milestone16 - Knowledge 页面
Iteration: Iteration 1 — 端到端最小可用（文件列表 + 索引状态）

Goal:
跑通「打开 Knowledge 页面 → 后端扫描目录 → 前端显示文件列表 + 索引状态」的完整端到端流程。
替换当前 SpotlightContainer 中的 "Coming Soon" 占位符为可用的 Knowledge 组件。

Assumptions:
- 复用现有 `walkdir` crate 扫描目录，`FileRegistry`（SQLite）查询索引状态
- `list_knowledge_files` 一次性返回所有文件（含状态），前端无需分页（首版简化）
- 不可索引的文件类型（Office/CSV/图片）也在列表中显示，但状态固定为 `Unsupported`
- 忽略列表（`ignored_files`）先在 AppConfig 中定义字段，Iteration 2 才实现 UI 操作
- 前端文件列表先用简单平铺列表，目录树视图为可选项（视时间决定是否实现）
- 文件搜索/过滤先实现前端本地过滤（数据已全量加载到前端）

Scope:
后端: 2 个新 Tauri command + AppConfig 扩展
前端: Knowledge.svelte 组件 + FileList 子组件 + SpotlightContainer 集成

Tasks:
- [x] 1.1 后端: 定义数据结构 `KnowledgeFile`、`FileType`、`IndexState`
  - 在 `main.rs` 中定义 `KnowledgeFile` struct（path, name, relative_path, size, modified, file_type, index_status）
  - 定义 `FileType` 枚举（Markdown, Text, Pdf, Html, Word, PowerPoint, Excel, Csv, Image, Other）
  - 定义 `IndexState` 枚举（Unindexed, Indexing, Indexed, Outdated, Ignored, Unsupported）
  - 在 `AppConfig` 中新增 `ignored_files: Vec<String>` 字段（`#[serde(default)]`）
  - 修改: `knot-app/src-tauri/src/main.rs`

- [x] 1.2 后端: 实现 `list_knowledge_files` Tauri command
  - 读取 AppConfig 获取 `data_dir` 和 `ignored_files`
  - 使用 `walkdir` 递归扫描目录，过滤隐藏文件（`.` 开头）
  - 按扩展名判断 `FileType`，可索引类型（md/txt/html/pdf）查 `FileRegistry` 获取索引状态
  - 不可索引类型状态固定为 `Unsupported`，忽略列表中的文件状态为 `Ignored`
  - 返回 `Vec<KnowledgeFile>`，按文件名排序
  - 注册到 `invoke_handler`
  - 修改: `knot-app/src-tauri/src/main.rs`, `knot-core/src/registry.rs`（新增 `get_file_info` 查询单文件 hash + indexed_at）

- [x] 1.3 前端: 创建 `Knowledge.svelte` 主组件
  - 组件挂载时调用 `get_app_config` 获取 data_dir
  - 如果 data_dir 未设置，显示空状态引导（"请先在设置中选择目录"）
  - 调用 `list_knowledge_files` 获取文件列表
  - 管理加载状态（loading → loaded → error）
  - 布局: 顶部统计栏 + 搜索框 + 文件列表区域
  - 修改: 新建 `knot-app/src/lib/components/Knowledge.svelte`

- [x] 1.4 前端: 创建 `FileList.svelte` 文件列表子组件
  - 接收 `files` 数组作为 prop
  - 每行显示: 文件类型图标（Material Symbols）、文件名、相对路径、大小（人类可读格式）、修改时间
  - 索引状态 badge: 🟢 已索引 / 🟡 待更新 / ⚪ 未索引 / 🔵 索引中(脉冲) / ━━ 不支持 / ⬛ 已忽略
  - 文件类型图标映射: pdf→picture_as_pdf, md/txt→description, html→code, xlsx/csv→table_chart, docx→article, pptx→slideshow, image→image
  - 支持点击选中文件（高亮当前行），为 Iteration 2 的详情面板做准备
  - 修改: 新建 `knot-app/src/lib/components/Knowledge/FileList.svelte`

- [x] 1.5 前端: 统计栏 + 搜索过滤
  - 统计栏: 显示「共 N 文件 | 已索引 M | 待更新 K | 不支持 J」
  - 搜索框: 前端本地过滤，按文件名模糊匹配
  - 过滤按钮: 按文件类型过滤（全部 / 文本 / PDF / Office / 图片）、按索引状态过滤
  - 修改: `knot-app/src/lib/components/Knowledge.svelte`

- [x] 1.6 前端: 集成到 SpotlightContainer
  - 将 `{:else if navigation.view === VIEW_KNOWLEDGE}` 分支中的 "Coming Soon" 替换为 `<Knowledge />`
  - import Knowledge 组件
  - 修改: `knot-app/src/lib/components/Spotlight/SpotlightContainer.svelte`

- [x] 1.7 验证: 端到端 demo
  - 编译通过 `cargo build`
  - 运行 app，设置 data_dir 指向测试目录
  - 切换到 Knowledge tab，验证文件列表正确显示
  - 验证各文件类型图标正确、索引状态正确
  - 验证搜索过滤功能
  - 验证空状态（未设置 data_dir）显示引导信息

Exit criteria:
- 编译通过，现有测试不受影响
- Knowledge 页面替换 "Coming Soon"，显示文件列表
- 文件列表正确显示 data_dir 下的所有文件（递归子目录）
- 已索引文件显示 🟢，未索引显示 ⚪，Office/CSV/图片显示 ━━ 不支持
- 搜索框可按文件名过滤
- 统计栏正确显示各状态文件数量
- 端到端可 demo: 打开 Knowledge → 看到文件列表 → 各状态正确 → 搜索过滤可用
