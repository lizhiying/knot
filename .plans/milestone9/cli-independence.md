# Milestone 9: CLI 独立可用性

## 目标

让 `knot-cli` 成为一个独立可用的命令行 RAG 工具，可以脱离 `knot-app` 运行，支持：
- 下载模型 (`download`)
- 查看状态 (`status`)
- 索引文档 (`index`)
- 语义检索 (`query`)
- RAG 问答 (`ask`)

## 成功标准

1. 用户可以仅安装 `knot-cli` 二进制文件
2. 首次运行 `knot-cli download` 自动下载所需模型
3. `knot-cli index` 可以使用真实 Embedding 模型索引文档
4. `knot-cli query` 可以返回有意义的搜索结果
5. `knot-cli ask` 可以生成 RAG 回答

## 迭代计划

| 迭代        | 目标                            | 文档                           |
| :---------- | :------------------------------ | :----------------------------- |
| Iteration 1 | 端到端流程验证 (Embedding 可用) | [iteration1.md](iteration1.md) |
| Iteration 2 | LLM 集成 + 完整 RAG             | [iteration2.md](iteration2.md) |
| Iteration 3 | 体验优化 + 质量加固             | [iteration3.md](iteration3.md) |

## 关键假设

- 用户机器有网络连接，可以下载模型
- `~/.knot/` 目录可写
- 用户有足够磁盘空间存储模型 (~4GB)

## 非目标 (Out of Scope)

- 跨平台打包分发
- GUI 集成
- 多语言支持
