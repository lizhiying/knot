Milestone: milestone2
Iteration: iteration3

Goal: 移除内嵌模型，实现真正的 Installer 瘦身，并确保“无需重启”。

Assumptions:
- 模型下载功能稳定完善。
- 引擎支持重载配置。

Scope:
1.  **构建剥离 (Strip)**:
    *   修改 `tauri.conf.json` 或构建脚本，将 OCR 和 LLM 模型从 `resources` 中排除。
    *   保留 `bge-small-zh`。
2.  **热加载 (Hot-Reload)**:
    *   在 `ModelManager` 中实现 `reload_ocr_engine()` 和 `reload_llm_engine()`。
    *   当下载状态变为 `Completed` 时，自动触发 Reload 信号。
    *   前端收到 `ModelReady` 事件，更新 UI 状态（移除“去下载”提示）。
3.  **引导流程**:
    *   App 启动时检查模型状态。
    *   若缺失，Toast/Banner 提示“核心模型未安装，请前往设置下载”。

Tasks:
- [x] 配置: 修改构建脚本剥离模型 <!-- id: 1 -->
- [x] 后端: 实现 `ModelManager` 热加载逻辑 <!-- id: 2 -->
- [x] 前端: 处理 `ModelReady` 事件更新状态 <!-- id: 3 -->
- [x] 前端: 实现缺失模型的引导提示 <!-- id: 4 -->
- [x] 验证: 全流程测试 (安装 -> 下载 -> 使用) <!-- id: 5 -->

Exit criteria:
- 最终构建出的 `.dmg` / `.app` 体积大幅减小。
- 在无模型状态下启动 App 不崩溃。
- 下载完成后，立刻可以使用 OCR 解析 PDF，无需重启。
