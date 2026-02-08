Milestone: cli-independence
Iteration: 4 - LLM 服务常驻优化

Goal:
优化 LLM 加载性能，避免每次 ask 都重新加载模型，提供更流畅的使用体验。

Assumptions:
- 核心 RAG 功能已在 Iteration 1-3 中完成
- LLM 端口改为 28081（避免与其他服务冲突）

Scope:
- REPL 交互模式（短期方案）
- 守护进程服务（长期方案）
- 自动检测复用已运行的服务

Tasks:
- [x] 修改 LLM 端口从 8081 到 28081
- [x] 实现 `chat` 命令（REPL 交互模式）
- [x] 实现 `serve` 命令（启动守护进程）
- [x] ask 命令自动检测并复用已运行的服务
- [x] 添加 `serve --stop` 停止服务

Exit criteria:
- `chat` 命令可以连续对话，只加载一次模型
- `serve` 命令可以在后台运行 LLM 服务
- `ask` 命令检测到已有服务时直接使用，无需重新加载
- 端口改为 28081
