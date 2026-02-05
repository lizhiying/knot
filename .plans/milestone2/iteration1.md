Milestone: milestone2
Iteration: iteration1

Goal: 跑通“从网络下载模型”到“App 加载外部模型”的主流程，不追求完美的 UI 和自动队列。

Assumptions:
- 模型文件在 `AppData` 目录可以被 Rust 正常读取。
- 可以使用 HTTP Range 请求或基础 GET 请求下载。
- 暂时不需要处理断点续传。

Scope:
1.  **后端 (Rust)**:
    *   实现 `ModelPathManager`: 能够区分“内嵌路径”和“外部下载路径”。
    *   实现 `Downloader`: 基础的 HTTP GET 下载，支持写入文件系统。
    *   实现 `ModelSourceConfig`: 国内/国外 URL 管理，包含**自动检测 (Auto-detect)** 逻辑（基于时区或连通性）。
2.  **模型加载**:
    *   修改 OCR 引擎初始化逻辑：先查 `AppData/models`，没有则报错或Fallback（此阶段暂不Strip）。
3.  **UI (Svelte)**:
    *   设置页新增 "模型管理" 面板。
    *   **Region Control**: 显示自动检测结果，并允许手动切换源 (CN/Global)。
    *   "Download OCR" 按钮 (点击即开始下载)。

Tasks:
- [x] 后端: 实现 `ModelPathManager` 和此逻辑 <!-- id: 1 -->
- [x] 后端: 实现 `Downloader` (reqwest) <!-- id: 2 -->
- [x] 后端: 实现 `ModelSourceConfig` (Region Detection) <!-- id: 3 -->
- [x] 后端: 修改 OCR 引擎加载路径逻辑 <!-- id: 4 -->
- [x] 前端: 实现模型管理面板 UI <!-- id: 5 -->
- [x] 前端: 集成下载按钮与后端 Command <!-- id: 6 -->

Exit criteria:
- 点击下载能成功将 `OCRFlux-3B.Q4_K_M.gguf` 下载到 `AppData`。
- App 能识别并加载该路径下的模型。
