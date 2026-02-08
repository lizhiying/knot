Milestone: cli-independence
Iteration: 2 - LLM 集成 + 完整 RAG

Goal:
实现 `ask` 命令和 `download` 命令，让 CLI 具备完整的 RAG 能力和自主模型管理。

Assumptions:
- llama-server 二进制可从固定 URL 下载或已预置
- 模型下载 URL 可配置

Scope:
- 实现 download 命令
- 实现 ask 命令 (集成 LlamaSidecar + LlamaClient)
- 流式终端输出
- JSON 输出格式

Tasks:
- [x] 实现 download 命令框架 (参数解析)
- [x] 添加模型下载逻辑 (基于 reqwest + indicatif 进度条)
- [x] 集成 LlamaSidecar 到 knot-cli
- [x] 实现 ask 命令 (检索 + LLM 生成)
- [x] 实现流式终端输出 (打字机效果)
- [x] 添加 --json 参数支持 query/ask
- [x] 添加 --source 参数限制搜索范围 (已在 iteration1 实现)

Exit criteria:
- `cargo run -p knot-cli -- download` 成功下载 Embedding 模型
- `cargo run -p knot-cli -- download --model llm` 成功下载 LLM 模型
- `cargo run -p knot-cli -- ask -q "rust"` 返回 LLM 生成的回答
- `cargo run -p knot-cli -- query -t "测试" --json` 返回有效 JSON

将来可以用 cargo install --path knot-cli 安装到系统，然后直接用 knot-cli ask -q "rust" --json 运行