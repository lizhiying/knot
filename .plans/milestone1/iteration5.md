# Milestone: milestone1
Iteration: iteration5

Goal: 实现 App 端完整的 RAG 交互流程，包括设置页索引管理和 Spotlight 问答/聊天界面。

## Scope
1.  **Settings (General)**: 选择 Data Dir (文档目录)，触发后台索引。
2.  **Spotlight UI**:
    -   Top: 主搜索框。
    -   Bottom-Left: 引用源 (Search Results & Scores)。
    -   Bottom-Right: LLM 回答区 + 后续聊天输入框。
3.  **Backend**:
    -   持久化配置 (store `data_dir` preference).
    -   后台索引任务管理 (Background Indexing).
    -   RAG Query 接口 (Search + Generate).

## Tasks

### 1. Backend: Configuration & State
- [x] **Config Persistence**:
    -   使用 `tauri-plugin-store` 或简单 JSON 文件保存用户设置 (`target_dir`, `data_db_path`).
    -   App 启动时加载配置。
- [x] **Background Indexer**:
    -   实现 `cmd_set_target_dir(path)`: 保存路径并启动索引任务。
    -   实现 `cmd_get_indexing_status()`: 返回进度/状态 (Partial, emitted events).
    -   确保索引器运行在后台线程，不阻塞 UI。

### 2. Backend: RAG Commands
- [x] **Command `rag_search`**:
    -   Input: `query`, `chat_history`.
    -   Logic:
        1.  `knot_core::Store::search` (Hybrid/Vector).
        2.  Construct Prompt (Context + Query).
        3.  `knot_core::LlmClient::stream_generate`. (Implemented `rag_query`, simulated stream in UI)
    -   Output: (Results List, LLM Stream).

### 3. Frontend: Settings Page
- [x] Add **General** Tab.
- [x] Add "Document Directory" File Picker.
- [x] Show Indexing Status (Indexing... / Ready).

### 4. Frontend: Spotlight UI Refactor
- [x] **Chat Mode**:
    -   In Right Column bottom of Search tab: Add small input for "Follow-up questions".
    -   Maintain conversation context in Frontend state (UI only for now).

## Verification Steps
1.  **Setup**: Open Settings, select `/Users/lizhiying/Projects/mynotes/test`.
2.  **Indexing**: Verify UI shows "Indexing...", verify CPU usage (backend working).
3.  **Query**:
    -   Open Spotlight (`Cmd+K` or similar).
    -   Type a known keyword from notes.
    -   **Verify Left**: Correct file appears.
    -   **Verify Right**: LLM generates answer based on file.
4.  **Chat**:
    -   Type "Tell me more" in follow-up box.
    -   **Verify**: LLM answers using previous context.
