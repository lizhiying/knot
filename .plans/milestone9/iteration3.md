Milestone: cli-independence
Iteration: 3 - 体验优化 + 质量加固

Goal:
提升用户体验，处理边界情况，确保 CLI 可以稳定使用。

Assumptions:
- 核心功能已在 Iteration 1-2 中验证

Scope:
- 配置文件支持
- 进度条显示
- 错误处理优化
- 多目录索引管理

Tasks:
- [x] 实现 ~/.knot/config.toml 配置文件支持
- [x] 索引过程添加进度条 (indicatif)
- [x] 优化错误消息 (用户友好的提示)
- [x] 模型缺失时自动提示 download 命令
- [x] status 命令显示多目录索引详情 (index-list)
- [x] 添加 --verbose 参数显示详细日志
- [x] 添加 index --remove 命令移除索引

Exit criteria:
- 配置文件可覆盖默认模型路径
- 索引过程有可视化进度
- 所有错误都有用户友好的提示
- 文档更新完成
