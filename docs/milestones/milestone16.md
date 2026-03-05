# Milestone 16: Knowledge 页面 — 文件索引管理与知识探索

## 目标

实现 app 中的 Knowledge 页面，替换当前 "Coming Soon" 占位符。该页面提供对目标目录（Settings 中配置的 `data_dir`）下所有有效文件的可视化索引管理能力，包括查看索引状态、浏览索引数据、单文件 RAG 聊天、重新索引、以及排除文件等功能。

## 成功指标

- 能在 Knowledge 页面中浏览目标目录下所有支持的文件（递归扫描子目录）
- 每个文件清晰显示索引状态（未索引、索引中、已索引）
- 已索引文件可点击查看索引详情（chunk 列表、向量数量、文件大小等）
- 已索引文件支持单文件 RAG 聊天（仅基于该文件内容回答问题）
- 已索引文件支持一键重新索引
- 支持从索引中排除文件（加入忽略列表，非物理删除）
- 忽略列表中的文件不会被索引或文件变更监控

## 前置条件

- ✅ Milestone 14 已完成（RAG 搜索系统 + 上下文扩展 + 多跳检索）
- ✅ Milestone 15 已完成（GraphRAG 知识图谱增强）
- ✅ Settings 页面已有 `data_dir` 配置功能
- ✅ 文件索引/监控系统已稳定（`KnotIndexer` + `FileRegistry` + `DirectoryWatcher`）

## 支持的文件类型

| 类型     | 扩展名                     | 索引支持 | 备注                                 |
| -------- | -------------------------- | -------- | ------------------------------------ |
| 文本文件 | `.md`, `.txt`, `.html`     | ✅ 索引   | 现有支持，本 milestone 主要聚焦类型  |
| PDF 文件 | `.pdf`                     | ✅ 索引   | 现有支持                             |
| Office   | `.docx`, `.pptx`, `.xlsx`  | ❌ 仅展示 | 在列表中显示但不索引，后续 milestone |
| CSV      | `.csv`                     | ❌ 仅展示 | 在列表中显示但不索引，后续 milestone |
| 图片     | `.png`, `.jpg`, `.jpeg` 等 | ❌ 仅展示 | 在列表中显示但不索引，后续 milestone |

> **范围说明**：本 milestone 仅对 `.md`、`.txt`、`.html`、`.pdf` 做内容索引。Office、CSV、图片文件在 Knowledge 列表中展示（文件名、大小、路径等元数据），但**不做内容解析和索引**，它们的索引状态固定显示为「不支持」。这些文件类型的解析将在后续 milestone 中实现。

## 迭代规划

### Iteration 1: 文件列表 + 索引状态（端到端 vertical slice）

**目标**: 在 Knowledge 页面显示目标目录下的所有有效文件及其索引状态。

详见 [iteration1.md](../../.plans/milestone16/iteration1.md)

#### 后端任务

| #   | 任务                                       | 说明                                                                                                                                 |
| --- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| 1.1 | 新增 `list_knowledge_files` Tauri command  | 递归扫描 `data_dir`，返回所有文件列表（路径、文件名、大小、修改时间、文件类型）；包含不可索引类型但标记为 `Unsupported`              |
| 1.2 | 新增 `get_file_index_status` Tauri command | 查询 `FileRegistry` 判断文件索引状态：`unindexed`（未在 registry 中）、`indexed`（hash 匹配）、`outdated`（hash 不匹配，需重新索引） |
| 1.3 | 实现忽略列表存储                           | 在 `AppConfig` 中新增 `ignored_files: Vec<String>` 字段，存储被排除的文件路径                                                        |

#### 前端任务

| #   | 任务                         | 说明                                                         |
| --- | ---------------------------- | ------------------------------------------------------------ |
| 1.5 | 创建 `Knowledge.svelte` 组件 | 替换 SpotlightContainer 中的 "Coming Soon" 占位符            |
| 1.6 | 文件列表 UI                  | 显示文件图标（按类型区分）、文件名、相对路径、大小、修改时间 |
| 1.7 | 索引状态标签                 | 每个文件旁显示状态 badge：🟢 已索引 / 🟡 待更新 / ⚪ 未索引     |
| 1.8 | 搜索/过滤                    | 支持按文件名搜索，按文件类型/索引状态筛选                    |
| 1.9 | 目录树视图（可选）           | 按目录层级展示文件，支持折叠/展开                            |

#### 数据流

```
Knowledge 页面打开
  → 调用 get_app_config 获取 data_dir
  → 调用 list_knowledge_files(data_dir)
  → 后端: walkdir 扫描 + FileRegistry 查询状态
  → 返回: Vec<KnowledgeFile> { path, name, size, modified, file_type, index_status }
  → 前端: 渲染文件列表 + 状态标签
```

---

### Iteration 2: 索引详情 + 重新索引 + 排除文件

**目标**: 支持查看已索引文件的详细数据、手动触发重新索引、将文件加入忽略列表。

详见 [iteration2.md](../../.plans/milestone16/iteration2.md)

#### 后端任务

| #   | 任务                                         | 说明                                                                                                 |
| --- | -------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| 2.1 | 新增 `get_file_index_detail` Tauri command   | 查询 Tantivy 中该文件的所有 chunk（id、text 前 200 字、breadcrumbs），返回 chunk 数量、总 token 约数 |
| 2.2 | 新增 `reindex_file` Tauri command            | 删除旧索引 → 重新解析 → 写入新索引 → 更新 FileRegistry hash                                          |
| 2.3 | 新增 `ignore_file` Tauri command             | 将文件路径加入 `AppConfig.ignored_files`，同时从索引和 FileRegistry 中删除该文件数据                 |
| 2.4 | 新增 `unignore_file` Tauri command           | 从忽略列表中移除文件，使其恢复正常索引                                                               |
| 2.5 | 修改 `index_directory` 和 `DirectoryWatcher` | 在扫描和监控时跳过 `ignored_files` 中的文件                                                          |
| 2.6 | 新增 `KnotStore.get_file_chunks()`           | 在 Tantivy 中按 `file_path` 查询该文件的所有 chunk 记录                                              |

#### 前端任务

| #    | 任务                         | 说明                                                                                         |
| ---- | ---------------------------- | -------------------------------------------------------------------------------------------- |
| 2.7  | 文件详情面板（侧边栏或弹窗） | 点击已索引文件后展示：chunk 列表（可折叠）、每个 chunk 的前 200 字文本预览、breadcrumbs 路径 |
| 2.8  | 重新索引按钮                 | 点击后显示加载状态、完成后刷新文件状态                                                       |
| 2.9  | 排除文件功能                 | 文件右键菜单或操作按钮，点击后二次确认，排除后文件显示为灰色/划线或从列表中隐藏              |
| 2.10 | 忽略列表管理                 | 在 Knowledge 页面或 Settings 页面中显示已忽略文件列表，支持恢复                              |
| 2.11 | 索引进度事件监听             | 监听 `indexing-status` 事件，实时更新当前正在索引的文件的状态                                |

---

### Iteration 3: 单文件 RAG 聊天 + 体验打磨

**目标**: 支持对已索引文件进行单文件 RAG 聊天，优化整体交互体验。

详见 [iteration3.md](../../.plans/milestone16/iteration3.md)

#### 后端任务

| #   | 任务                                   | 说明                                                                               |
| --- | -------------------------------------- | ---------------------------------------------------------------------------------- |
| 3.1 | 新增 `rag_search_file` Tauri command   | 类似 `rag_search`，但搜索范围限定在指定文件（Tantivy query 增加 `file_path` 过滤） |
| 3.2 | 新增 `rag_generate_file` Tauri command | 使用限定文件的搜索上下文生成 LLM 回答                                              |
| 3.3 | 修改 `KnotStore.search()`              | 新增可选 `file_filter: Option<&str>` 参数，限定向量搜索和关键词搜索的文件范围      |

#### 前端任务

| #   | 任务           | 说明                                                               |
| --- | -------------- | ------------------------------------------------------------------ |
| 3.4 | 单文件聊天界面 | 在文件详情面板底部或独立视图中，提供输入框 + 流式回答展示          |
| 3.5 | 聊天上下文提示 | 顶部显示 "正在与 xxx.pdf 对话"，明确聊天范围                       |
| 3.6 | 聊天引用高亮   | 回答中引用的 chunk 在详情面板中高亮显示                            |
| 3.7 | 空状态处理     | 未设置 data_dir 时的引导提示，目录为空时的友好提示                 |
| 3.8 | 加载 & 动画    | 骨架屏加载动画、状态切换过渡动画、文件列表虚拟滚动（大文件量场景） |
| 3.9 | 键盘快捷键     | 上下方向键选择文件、Enter 查看详情、Esc 返回列表                   |

---

## 架构设计

### 数据模型

```rust
// 后端返回给前端的文件信息
#[derive(serde::Serialize)]
struct KnowledgeFile {
    /// 文件绝对路径
    path: String,
    /// 文件名
    name: String,
    /// 相对于 data_dir 的路径
    relative_path: String,
    /// 文件大小（字节）
    size: u64,
    /// 最后修改时间（Unix 时间戳）
    modified: i64,
    /// 文件类型分类
    file_type: FileType,
    /// 索引状态
    index_status: IndexState,
}

#[derive(serde::Serialize)]
enum FileType {
    Markdown,    // .md
    Text,        // .txt
    Pdf,         // .pdf
    Html,        // .html
    Word,        // .docx（仅展示，本 milestone 不索引）
    PowerPoint,  // .pptx（仅展示，本 milestone 不索引）
    Excel,       // .xlsx（仅展示，本 milestone 不索引）
    Csv,         // .csv（仅展示，本 milestone 不索引）
    Image,       // .png/.jpg/...（仅展示，本 milestone 不索引）
    Other,       // 其他不支持的文件类型
}

#[derive(serde::Serialize)]
enum IndexState {
    /// 未索引（不在 FileRegistry 中）
    Unindexed,
    /// 索引中（正在处理）
    Indexing,
    /// 已索引（hash 匹配）
    Indexed,
    /// 待更新（文件已变更，hash 不匹配）
    Outdated,
    /// 已忽略（在忽略列表中）
    Ignored,
    /// 不支持索引（Office/CSV/图片等，仅展示）
    Unsupported,
}
```

### 文件详情数据

```rust
#[derive(serde::Serialize)]
struct FileIndexDetail {
    /// 文件路径
    file_path: String,
    /// chunk 总数
    chunk_count: usize,
    /// 各 chunk 的摘要信息
    chunks: Vec<ChunkSummary>,
    /// 索引时间
    indexed_at: Option<i64>,
    /// 文件内容 hash
    content_hash: Option<String>,
}

#[derive(serde::Serialize)]
struct ChunkSummary {
    /// chunk ID
    id: String,
    /// 文本预览（前 200 字符）
    preview: String,
    /// breadcrumbs 层级路径
    breadcrumbs: Option<String>,
}
```

### 忽略列表设计

```rust
// AppConfig 扩展
struct AppConfig {
    // ... existing fields ...
    
    /// 忽略文件列表：这些文件不会被索引和监控
    #[serde(default)]
    ignored_files: Vec<String>,
}
```

**忽略规则**：
- 忽略列表存储文件的绝对路径
- `index_directory()` 扫描时检查忽略列表，跳过命中的文件
- `DirectoryWatcher` 收到变更事件时检查忽略列表，跳过命中的文件
- `ignore_file` 操作会同时：
  1. 将路径加入 `AppConfig.ignored_files`
  2. 从 KnotStore（LanceDB + Tantivy）中删除该文件的所有记录
  3. 从 FileRegistry 中删除该文件的记录
  4. 如果启用了 GraphRAG，从 EntityGraph 中删除该文件的实体和关系

### 单文件 RAG 聊天流程

```
用户在 Knowledge 页面选择文件 → 点击 "Chat"
  → 显示聊天界面，顶部标注文件名
  → 用户输入问题
  → 前端调用 rag_search_file(file_path, query)
    → 后端: KnotStore.search() 增加 file_path 过滤
    → 仅搜索该文件下的 chunk
  → 前端调用 rag_generate_file(query, context)
    → 后端: 与 rag_generate 逻辑相同，使用限定上下文
  → 流式显示回答
```

## 涉及文件

| 文件                                                              | 变更类型 | Iteration | 说明                                           |
| ----------------------------------------------------------------- | -------- | --------- | ---------------------------------------------- |
| `knot-app/src/lib/components/Knowledge.svelte`                    | 新建     | 1-3       | Knowledge 主页面组件                           |
| `knot-app/src/lib/components/Knowledge/FileList.svelte`           | 新建     | 1         | 文件列表子组件                                 |
| `knot-app/src/lib/components/Knowledge/FileDetail.svelte`         | 新建     | 2         | 文件详情面板                                   |
| `knot-app/src/lib/components/Knowledge/FileChat.svelte`           | 新建     | 3         | 单文件聊天子组件                               |
| `knot-app/src/lib/components/Spotlight/SpotlightContainer.svelte` | 修改     | 1         | 替换 "Coming Soon" 为 Knowledge 组件           |
| `knot-app/src-tauri/src/main.rs`                                  | 修改     | 1-3       | 新增 Tauri commands + AppConfig 扩展           |
| `knot-core/src/store.rs`                                          | 修改     | 2-3       | 新增 `get_file_chunks()`、搜索增加 file_filter |
| `knot-core/src/index.rs`                                          | 修改     | 2         | `index_directory()` 集成忽略列表               |
| `knot-core/src/monitor.rs`                                        | 修改     | 2         | `should_index_file()` 扩展 + 忽略列表检查      |
| `knot-core/src/registry.rs`                                       | 修改     | 1-2       | 新增 `get_file_info()` 查询单文件详细信息      |

## UI 设计要点

### 布局

```
┌─────────────────────────────────────────────┐
│  Knowledge                    [搜索] [过滤] │
├─────────────────────────────────────────────┤
│ 📊 统计栏: 共 128 文件 | 已索引 96 | 待更新 12 │
├──────────────────────┬──────────────────────┤
│                      │                      │
│   文件列表           │   文件详情 / 聊天    │
│                      │                      │
│ 📄 report.pdf  🟢    │  Chunks (15)         │
│ 📝 notes.md   🟢    │  ├─ 第一章: ...      │
│ � paper.pdf  🟡    │  ├─ 1.1 节: ...      │
│ � todo.txt   ⚪    │  └─ ...              │
│ � data.xlsx  ━━    │                      │
│ 🖼️ diagram.png ━━   │  (━━ = 不支持索引)   │
│                      │  [重新索引] [聊天]   │
│                      │                      │
├──────────────────────┴──────────────────────┤
│  状态栏: 索引中... scanning 3/128           │
└─────────────────────────────────────────────┘
```

### 交互细节

1. **文件图标**: 使用 Material Symbols，按文件类型区分图标
   - 📄 PDF → `picture_as_pdf`
   - 📝 Markdown/Text → `description`
   - 📊 Excel/CSV → `table_chart`
   - 📑 Word → `article`
   - 🖼️ Image → `image`
   - 📰 PPT → `slideshow`

2. **状态颜色**: 
   - 🟢 已索引 (`#28c840`) 
   - 🟡 待更新 (`#febc2e`)
   - ⚪ 未索引 (`rgba(255,255,255,0.3)`)
   - � 索引中 (`#4a9eff` 脉冲动画)
   - ⬛ 已忽略 (`rgba(255,255,255,0.15)` + 删除线)
   - ━━ 不支持 (`rgba(255,255,255,0.1)` + 灰色文字)

3. **上下文菜单（右键或 ⋯ 按钮）**:
   - 查看详情
   - 重新索引
   - 单文件聊天
   - 排除文件
   - 在 Finder 中显示

## 风险与约束

| 风险                                 | 影响       | 缓解措施                                         |
| ------------------------------------ | ---------- | ------------------------------------------------ |
| 大目录文件量多（10k+），列表渲染卡顿 | 用户体验差 | 使用虚拟滚动（只渲染可见区域），延迟加载索引状态 |
| 文件扫描耗时导致页面加载慢           | 首次打开慢 | 后台异步扫描 + 骨架屏 + 增量加载                 |
| 忽略列表与已有索引数据不一致         | 数据残留   | ignore_file 操作原子化，同步清理所有存储         |
| 单文件聊天的上下文可能不足           | 回答质量低 | 对小文件直接传全文，大文件使用 RAG 检索          |

## 不在本里程碑范围

以下功能明确**不在** Milestone 16 范围内，将在后续 milestone 中实现：

- Office 文件（`.docx`、`.pptx`、`.xlsx`）的内容解析与索引
- CSV 文件（`.csv`）的内容解析与索引
- 图片文件（`.png`、`.jpg` 等）的 OCR/多模态内容提取与索引
- 文件预览功能（在 Knowledge 页面内直接预览文件内容）
- 批量操作（批量重新索引、批量排除）
