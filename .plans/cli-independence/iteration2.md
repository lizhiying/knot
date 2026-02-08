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
- [ ] 实现 download 命令框架 (参数解析)
- [ ] 添加模型下载逻辑 (复用 knot-app 的 Downloader)
- [ ] 集成 LlamaSidecar 到 knot-cli
- [ ] 实现 ask 命令 (检索 + LLM 生成)
- [ ] 实现流式终端输出 (打字机效果)
- [ ] 添加 --json 参数支持 query/ask
- [ ] 添加 --source 参数限制搜索范围

Exit criteria:
- `knot-cli download` 成功下载 Embedding 模型
- `knot-cli download --model llm` 成功下载 LLM 模型
- `knot-cli ask -q "测试问题"` 返回 LLM 生成的回答
- `knot-cli query -t "测试" --json` 返回有效 JSON
