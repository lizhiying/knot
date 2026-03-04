# Milestone: M7 CLI 命令行工具
# Iteration: 2 — 功能补全

## 目标

补全剩余的 3 个子命令（`rag`、`info`、`config`），实现页码范围过滤功能，支持 RAG 导出的多种输出格式（text / jsonl / csv）。完成后 CLI 的功能覆盖面达到设计的 100%。

## 假设

- Iteration 1 已完成，`parse` 和 `markdown` 子命令可正常工作
- clap 骨架已建立，新增子命令只需添加模块
- 页码范围解析独立为工具函数，可被所有子命令复用

## 范围

- `rag` 子命令：RAG 扁平化文本导出，支持 text / jsonl / csv 格式
- `info` 子命令：PDF 基础信息展示（页数/元数据/大纲/每页概览）
- `config` 子命令：配置管理（show / init / path）
- 页码范围解析器（支持 `1-5`、`1,3,5`、`1-3,5,8-10`、`5-`）
- 为 `parse` 和 `markdown` 追加 `--pages` 选项

## 任务

- [x] 实现页码范围解析器（`src/utils/page_range.rs`）：解析 `--pages` 参数为页码集合
- [x] 实现 `rag` 子命令（`src/commands/rag.rs`）：支持 `--format`（text/jsonl/csv）、`--type`（all/blocks/table-rows/table-cells）、`--pages`、`-o`
- [x] 实现 `info` 子命令（`src/commands/info.rs`）：输出 PDF 基础信息，支持 `--json`
- [x] 实现 `config` 子命令（`src/commands/config.rs`）：`show` / `init` / `path` 三个子子命令
- [x] 为 `parse` 和 `markdown` 子命令追加 `--pages` 选项
- [x] 页码范围解析器单元测试（12 个测试全部通过）

## 退出标准

- [x] `knot-pdf-cli rag sample.pdf` 输出 RAG 文本
- [x] `knot-pdf-cli rag sample.pdf --format jsonl` 输出 JSONL 格式
- [x] `knot-pdf-cli rag sample.pdf --pages 1-3 --type blocks` 仅输出前 3 页的文本块
- [x] `knot-pdf-cli info sample.pdf` 显示 PDF 信息（含元数据/页面概览/汇总）
- [x] `knot-pdf-cli info sample.pdf --json` 输出 JSON 格式信息
- [x] `knot-pdf-cli config show` 显示当前配置（TOML 格式）
- [x] `knot-pdf-cli config init` 在当前目录生成 `knot-pdf.toml`
- [x] `knot-pdf-cli config path` 列出搜索路径及存在状态
- [x] `knot-pdf-cli parse sample.pdf --pages 1-3` 仅输出前 3 页
- [x] 页码范围解析器处理各种格式正确

## 验证结果（2026-02-25）

```
✅ 页码范围解析器: 12/12 单元测试通过
✅ rag --format jsonl --type blocks --pages 1-2  → JSONL 格式，仅文本块，仅前 2 页
✅ info bench_100pages.pdf  → 显示文件大小/元数据/100 页概览（自动折叠中间页）
✅ config show    → 输出当前配置 TOML
✅ config init    → 生成 knot-pdf.toml（含注释头）
✅ config path    → 列出 3 个搜索路径及存在状态
✅ parse --pages  → 页码过滤正常
✅ markdown --pages → 页码过滤正常
```
