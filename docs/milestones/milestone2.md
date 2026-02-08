# Milestone 2: 模型动态分发与管理 (Model Dynamic Distribution & Management)

## 1. 目标 (Goal)
**核心目标**: 将体积巨大的 OCR 和 LLM 模型从 App 安装包中剥离，实现“按需下载”与“热加载”，显著减小安装包体积，并解决国内网络环境下的模型下载问题。

**关键指标**:
*   Installer 体积减少 3GB+。
*   用户在设置页点击下载后，无需重启 App 即可使用 OCR 和 LLM 功能。
*   国内用户默认走镜像源，国外用户走 HuggingFace 源。

## 2. 核心需求梳理 (Requirements)
*   **模型管理**:
    *   **Strip**: 移除 `OCRFlux-3B` (GGUF + MMProj) 和 `Qwen3-1.7B`。
    *   **Keep**: 保留 `bge-small-zh` (体积小，用于 Embedding，必须内嵌)。
*   **下载体验**:
    *   **引导**: 安装后引导至设置页。
    *   **顺序**: 优先下载 OCR (PDF解析强依赖)，其次 LLM。
    *   **检查**: 下载前检查 **磁盘空间**。
    *   **反馈**: 实时 **进度提醒**。
    *   **路径**: 存入 App 用户数据目录 (User Data Dir)，也就是 `app_data_dir`。
*   **网络源 (Mirroring)**:
    *   **Global**: HuggingFace Main.
    *   **CN**: hf-mirror.com.

### 模型清单与地址

| 模型组件 | 文件名 | 大小 (Est.) | 优先级 |
| :--- | :--- | :--- | :--- |
| **OCR (Main)** | `OCRFlux-3B.Q4_K_M.gguf` | ~2.1 GB | **High** |
| **OCR (Proj)** | `OCRFlux-3B.mmproj-f16.gguf` | ~1.34 GB | **High** |
| **LLM** | `Qwen3-1.7B-Q4_K_M.gguf` | ~1.1 GB | Medium |

**下载源配置**:
*   **Global**: `https://huggingface.co/...`
*   **CN**: `https://hf-mirror.com/...` (详细链接见附录)

---

## 3. 迭代计划 (Iteration Plan)

为了稳健地实现这一复杂功能，我们将 Milestone 2 拆解为 3 个迭代。

### Iteration 1: 核心下载架构与手动源切换 (Core Infrastructure)
**目标**: 跑通“从网络下载模型”到“App 加载外部模型”的主流程，不追求完美的 UI 和自动队列。

**功能范围**:
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

**Exit Criteria**:
*   [ ] 点击下载能成功将 `OCRFlux-3B.Q4_K_M.gguf` 下载到 `AppData`。
*   [ ] App 能识别并加载该路径下的模型。

### Iteration 2: 队列管理、进度与健壮性 (Queue & Robustness)
**目标**: 完善用户体验，实现自动化的下载队列、进度条和磁盘检查。

**功能范围**:
1.  **下载队列 (Priority Queue)**:
    *   实现逻辑：用户点击“一键下载”，自动将 3 个文件加入队列。
    *   顺序：`OCR Main` -> `OCR Proj` -> `LLM`。
2.  **磁盘检查**:
    *   下载前检查 `mount` 点剩余空间。如果 `< 5GB`，弹窗阻断。
3.  **UI 完善**:
    *   每个文件独立的进度条 (百分比 + 网速)。
    *   总体状态 ("Downloading 1/3...")。
    *   错误处理 (下载失败重试)。

**Exit Criteria**:
*   [ ] 可以一键按顺序下载所有模型。
*   [ ] 磁盘空间不足时有提示。
*   [ ] UI 能够流畅显示进度，不卡顿。

### Iteration 3: 热加载与发布构建 (Hot-Reload & Release)
**目标**: 移除内嵌模型，实现真正的 Installer 瘦身，并确保“无需重启”。

**功能范围**:
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

**Exit Criteria**:
*   [ ] 最终构建出的 `.dmg` / `.app` 体积大幅减小。
*   [ ] 在无模型状态下启动 App 不崩溃。
*   [ ] 下载完成后，立刻可以使用 OCR 解析 PDF，无需重启。

---

## 4. 技术细节 (Feature Spec)

### 文件存储结构
```text
$APP_DATA_DIR/
  └── models/
      ├── OCRFlux-3B.Q4_K_M.gguf
      ├── OCRFlux-3B.mmproj-f16.gguf
      └── Qwen3-1.7B-Q4_K_M.gguf
```

### 下载地址配置 (Config)

**Default (International):**
```
OCR_MAIN: https://huggingface.co/mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.Q4_K_M.gguf
OCR_PROJ: https://huggingface.co/mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.mmproj-f16.gguf
LLM_MAIN: https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf
```

**CN (Mirror):**
```
OCR_MAIN: https://hf-mirror.com/mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.Q4_K_M.gguf
OCR_PROJ: https://hf-mirror.com/mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.mmproj-f16.gguf
LLM_MAIN: https://hf-mirror.com/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf
```
