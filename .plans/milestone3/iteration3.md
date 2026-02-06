Milestone: milestone3
Iteration: iteration3

Goal: 将模型下载目录更改为用户主目录下的 `~/.knot/models`，并确保目录自动创建。

Scope: `src-tauri` (Model Manager).

Tasks:
- [x] [src-tauri] 引入 `directories` crate 或使用 `tauri::api::path` 获取 Home 目录 <!-- id: 0 -->
- [x] [src-tauri] 修改 `ModelPathManager` 或相关逻辑，将默认下载路径设为 `~/.knot/models` <!-- id: 1 -->
- [x] [src-tauri] 在应用启动或下载前检查并创建该目录 (`fs::create_dir_all`) <!-- id: 2 -->
- [/] [Verification] 验证模型是否下载到新目录，且应用能正确加载 <!-- id: 3 -->

Exit criteria:
- 点击下载后，文件出现在 `~/.knot/models/` 中。
- 应用重启后能识别已下载的模型。
