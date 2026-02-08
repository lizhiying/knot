Milestone: milestone7 - RAG 评估系统
Iteration: 1 - 端到端流程验证

Goal:
建立最小可运行的评测流程，验证从 QA 输入到结果输出的完整链路。使用 stubbed 数据和简化逻辑，确保流程跑通。

Assumptions:
- Knot RAG 接口通过 Tauri 命令 `rag_query` 可用（需启动 Knot App）
- CLI Query 命令使用 mock embedding，评测需使用 App 的 IPC 调用
- 使用少量手工 QA 数据（5-10 题）验证流程
- 评估逻辑使用简单字符串匹配

Scope:
- 手工创建少量测试 QA 数据
- 编写评测驱动器框架
- 调用 RAG 接口获取预测结果
- 实现基础评分逻辑（简单匹配）
- 输出简单 JSON 结果

Tasks:
- [x] 创建 `test/eval/` 目录结构
- [x] 在 Tauri 后端添加 HTTP 评测桥接接口（或通过 Tauri invoke 暴露给 Python）
- [x] 手工编写 5-10 个测试 QA 放入 `test/eval/eval.jsonl`
- [x] 编写评测驱动器脚本 `test/eval/run_eval.py`
- [x] 实现 RAG 接口调用逻辑（通过 HTTP 桥接）
- [x] 实现基础评分：答案匹配率
- [x] 输出 `test/eval/eval_result.json` 基础结构
- [ ] 端到端运行验证

Exit criteria:
- 能够运行 `python test/eval/run_eval.py` 完成评测
- 输出 JSON 结果文件包含问题、预测答案、分数
- 至少跑通 5 个测试用例
