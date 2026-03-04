# Milestone: M7 CLI 命令行工具
# Iteration: 1 — 端到端最小可用

## 目标

实现一个能编译运行的 `knot-pdf-cli` 命令行程序，支持 `parse` 和 `markdown` 两个核心子命令，能将 PDF 解析结果输出到 stdout 或文件。这是最小的端到端垂直切片——用户能真正在终端里运行命令处理 PDF。

## 假设

- M1~M6 所有功能已就绪，`knot-pdf` 库的 `parse_pdf`、`MarkdownRenderer` 等 API 稳定可用
- 使用 clap 4.x 的 derive 模式
- 暂不处理进度条、彩色输出等体验优化
- 暂不支持页码范围过滤（全量解析）
- 配置仅支持 `--config` 指定文件路径或默认 `Config::load_auto()`

## 范围

- 创建 `knot-pdf-cli` 二进制 crate
- clap 命令行骨架（顶层 + 2 个子命令）
- `parse` 子命令：PDF → IR JSON（stdout / -o 文件）
- `markdown` 子命令：PDF → Markdown（stdout / -o 文件）
- 基础错误处理（文件不存在 → 友好提示）
- 基础日志（env_logger）

## 任务

- [x] 创建 `knot-pdf-cli/Cargo.toml`，依赖 `knot-pdf`（path）、`clap`（derive）、`env_logger`、`serde_json`
- [x] 创建 `knot-pdf-cli/src/main.rs`：clap 顶层结构 + 全局选项（`--config`, `-v`, `-q`）+ 子命令枚举
- [x] 实现 `parse` 子命令（`src/commands/parse.rs`）：解析 PDF → `DocumentIR` → JSON，支持 `--pretty` 和 `-o`
- [x] 实现 `markdown` 子命令（`src/commands/markdown.rs`）：解析 PDF → Markdown，支持 `-o`
- [x] 基础错误处理：文件不存在 / PDF 加密 / PDF 损坏时输出友好错误信息
- [x] 验证编译运行：`cargo run -p knot-pdf-cli -- parse tests/fixtures/sample.pdf --pretty`

## 退出标准

- [x] `cargo build -p knot-pdf-cli` 编译成功
- [x] `knot-pdf-cli parse sample.pdf` 输出有效 JSON 到 stdout
- [x] `knot-pdf-cli parse sample.pdf -o output.json --pretty` 输出格式化 JSON 到文件
- [x] `knot-pdf-cli markdown sample.pdf` 输出 Markdown 到 stdout
- [x] `knot-pdf-cli markdown sample.pdf -o output.md` 输出 Markdown 到文件
- [x] `knot-pdf-cli --help` 显示帮助信息
- [x] 文件不存在时输出友好错误（不 panic）

## 验证结果（2026-02-25）

```
✅ cargo build --features cli --bin knot-pdf-cli        → 编译成功
✅ knot-pdf-cli --help                                  → 显示完整帮助（parse / markdown 子命令）
✅ knot-pdf-cli parse --help                            → 显示 parse 子命令参数
✅ knot-pdf-cli markdown --help                         → 显示 markdown 子命令参数
✅ knot-pdf-cli parse bench_100pages.pdf --pretty -o    → 34MB JSON，100 页，18.43s（release）
✅ knot-pdf-cli markdown bench_100pages.pdf -o          → 11.7MB Markdown，100 页，18.46s（release）
✅ knot-pdf-cli parse nonexistent.pdf                   → "错误: IO error: 文件不存在" + 退出码 1
✅ knot-pdf-cli -q parse nonexistent.pdf                → 静默模式，无输出，退出码 1
```

