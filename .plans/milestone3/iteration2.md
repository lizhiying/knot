Milestone: milestone3
Iteration: iteration2

Goal: 前端界面适配新的语义树结构，并可视化通过并发下载的模型进度。
Assumptions: Iteration 1 的后端接口已就绪。
Scope: `knot-app` (Svelte), `src-tauri` (Integration).

Tasks:
- [x] [Frontend] 更新 `DocParserPage`：适配后端返回的 Semantic Tree 数据结构 (Title node vs Page node) <!-- id: 0 -->
- [x] [Frontend] 优化 Tree View 样式：区分章节节点和页面节点（如有） <!-- id: 1 -->
- [x] [Frontend] 更新 Download UI：处理新的并发进度事件 (例如显示 "Downloading 3 files..." 或多个进度条) <!-- id: 2 -->
- [x] [Integration] 联调真实 PDF 解析，验证树结构正确性 <!-- id: 3 -->
- [x] [Integration] 联调模型下载，验证中断、失败、完成等状态的正确处理 <!-- id: 4 -->

Exit criteria:
- 用户导入 PDF 后，左侧大纲显示为章节标题结构。
- 点击设置页下载模型，能看到并行下载的效果，且最终全部成功。
