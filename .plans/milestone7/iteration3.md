Milestone: milestone7 - RAG 评估系统
Iteration: 3 - HTML 报告与质量优化

Goal:
生成美观的 HTML 可视化评估报告，优化用户体验，完善边缘情况处理。

Assumptions:
- Iteration 2 已完成，完整指标数据可用
- JSON 结果结构稳定
- 需要支持中文显示

Scope:
- HTML 报告生成
- 数据可视化（图表）
- 交互功能（搜索、排序）
- 边缘情况处理
- 改进建议生成

Tasks:
- [ ] 编写 HTML 报告生成器 `test/eval/generate_report.py`
- [ ] 实现概览面板（核心指标卡片）
- [ ] 实现按题型统计图表（饼图/柱状图）
- [ ] 实现按文档统计表格
- [ ] 实现详细结果表格（可搜索、可排序）
- [ ] 实现失败样例高亮展示
- [ ] 自动生成改进建议模块
- [ ] 优化中文显示与样式

Exit criteria:
- 运行评测后自动生成 `test/eval/eval_report.html`
- HTML 报告在浏览器中正常显示，支持中文
- 包含交互式表格，可搜索、可排序
- 包含可视化图表展示各项指标
- 失败样例清晰展示，便于分析
