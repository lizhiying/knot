# M7: CLI 命令行工具 — 里程碑总结

## 目标

为 knot-pdf 提供独立的命令行工具 `knot-pdf-cli`，使用户无需编写 Rust 代码即可通过终端调用 PDF 解析、导出等功能。

## 迭代计划

| 迭代        | 重点           | 交付物                                                                 |
| ----------- | -------------- | ---------------------------------------------------------------------- |
| Iteration 1 | 端到端最小可用 | `parse` + `markdown` 子命令可运行，能输出 JSON/Markdown 到 stdout/文件 |
| Iteration 2 | 功能补全       | `rag` + `info` + `config` 子命令，页码范围过滤，多种输出格式           |
| Iteration 3 | 体验打磨       | 进度显示，错误退出码，日志控制，集成测试，文档                         |

## 技术决策

- **独立 crate**：`knot-pdf-cli` 作为独立二进制 crate，依赖 `knot-pdf` 库
- **clap derive**：使用 clap 的 derive 模式定义命令行参数
- **配置合并**：命令行参数 > 配置文件 > 默认值

## 依赖

- M1 ~ M6 全部完成
- 外部 crate：`clap`（命令行参数）、`env_logger`（日志）、`serde_json`（JSON 输出）
