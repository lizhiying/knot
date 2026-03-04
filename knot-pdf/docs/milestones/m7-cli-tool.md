# M7：CLI 命令行工具

## 目标

为 `knot-pdf` 提供独立的命令行可执行程序 `knot-pdf-cli`，用户可以通过终端直接调用 PDF 解析、Markdown 导出、RAG 文本导出、IR JSON 导出等功能，无需编写 Rust 代码。支持 TOML 配置文件加载、多种输出格式选择、进度显示等。

## 依赖

- M1 ~ M6（全部核心功能就绪）

## 交付物

- [x] 独立可执行 crate `knot-pdf-cli`
- [x] 5 个子命令：`parse`、`markdown`、`rag`、`info`、`config`
- [x] 命令行参数解析（clap 4 derive）
- [x] 输出到 stdout 或文件（`-o` / `--output`）
- [x] 进度显示与静默模式
- [x] 配置文件自动加载 + 命令行覆盖
- [x] 完整的 `--help` 说明
- [x] 集成测试（CLI 端到端验证，20 个测试）

---

## Checklist

### 1. 工程结构

- [x] 创建 `knot-pdf-cli` 二进制 crate：
  - [x] `Cargo.toml`：`cli` feature（clap 4 derive + env_logger），`[[bin]]` 目标
  - [x] `src/bin/knot_pdf_cli/main.rs`：入口 + clap 命令分发
  - [x] `src/bin/knot_pdf_cli/commands/` 目录：每个子命令一个模块
  - [x] `src/bin/knot_pdf_cli/utils/` 目录：页码范围解析 + 输出工具
- [x] 编译验证：`cargo build --features cli --bin knot-pdf-cli`

### 2. 命令行参数设计（clap derive）

顶层结构：

```
knot-pdf-cli [OPTIONS] <COMMAND>

全局选项：
  -c, --config <FILE>    指定配置文件路径（默认自动搜索 knot-pdf.toml）
  -v, --verbose          输出详细日志（可叠加 -vv -vvv）
  -q, --quiet            静默模式（仅输出结果，不输出进度/日志）
      --no-color         禁用彩色输出
  -h, --help             显示帮助
  -V, --version          显示版本

子命令：
  parse      解析 PDF 并输出 IR JSON
  markdown   解析 PDF 并导出 Markdown
  rag        解析 PDF 并导出 RAG 扁平化文本
  info       显示 PDF 基础信息（页数/元数据/大纲）
  config     显示/生成配置文件
```

### 3. `parse` 子命令

```
knot-pdf-cli parse [OPTIONS] <INPUT>

参数：
  <INPUT>                  输入 PDF 文件路径

选项：
  -o, --output <FILE>      输出文件路径（默认 stdout）
      --pages <RANGE>      页码范围，如 "1-5" 或 "1,3,5"（默认全部）
      --pretty             美化 JSON 输出（默认紧凑）
      --include-timings    输出中包含耗时统计
      --include-diagnostics 输出中包含诊断信息
```

- [x] 解析 PDF → `DocumentIR`
- [x] 序列化为 JSON 输出
- [x] 支持页码范围过滤
- [x] 支持 `--pretty` 美化输出
- [x] 支持 `-o` 输出到文件

### 4. `markdown` 子命令

```
knot-pdf-cli markdown [OPTIONS] <INPUT>

参数：
  <INPUT>                  输入 PDF 文件路径

选项：
  -o, --output <FILE>      输出文件路径（默认 stdout）
      --pages <RANGE>      页码范围
      --no-tables          不包含表格
      --no-images          不包含图片引用
      --no-page-markers    不包含页码标记
```

- [x] 解析 PDF → `DocumentIR` → Markdown
- [x] 支持 `MarkdownRenderer` 选项透传（`--no-tables` / `--no-images` / `--no-page-markers`）
- [x] 支持页码范围过滤
- [x] 支持 `-o` 输出到文件

### 5. `rag` 子命令

```
knot-pdf-cli rag [OPTIONS] <INPUT>

参数：
  <INPUT>                  输入 PDF 文件路径

选项：
  -o, --output <FILE>      输出文件路径（默认 stdout）
      --pages <RANGE>      页码范围
      --format <FMT>       输出格式：text（默认）/ jsonl / csv
      --type <TYPE>        行类型过滤：all（默认）/ blocks / table-rows / table-cells
```

- [x] 解析 PDF → `DocumentIR` → RAG 扁平化文本
- [x] 支持多种输出格式（纯文本 / JSONL / CSV）
- [x] 支持按行类型过滤（all / blocks / table-rows / table-cells）
- [x] 支持页码范围过滤

### 6. `info` 子命令

```
knot-pdf-cli info [OPTIONS] <INPUT>

参数：
  <INPUT>                  输入 PDF 文件路径

选项：
      --json               以 JSON 格式输出
```

- [x] 显示 PDF 基础信息：
  - [x] 文件名、文件大小、doc_id（SHA256）
  - [x] 页数
  - [x] 元数据（标题/作者/创建日期）
  - [x] 大纲条目数
  - [x] 每页文本块数 / 表格数 / 图片数概览（超 20 页自动折叠）
- [x] 支持 `--json` JSON 格式输出

### 7. `config` 子命令

```
knot-pdf-cli config <SUBCOMMAND>

子命令：
  show       显示当前生效的配置（合并后）
  init       在当前目录生成 knot-pdf.toml 示例文件
  path       显示配置文件搜索路径
```

- [x] `config show`：加载并显示当前配置（TOML 格式）
- [x] `config init`：生成 `knot-pdf.toml` 示例文件到当前目录（已存在则跳过）
- [x] `config path`：列出配置文件搜索路径及是否存在

### 8. 通用功能

- [x] 进度显示：
  - [x] 解析时输出进度（`正在解析.../解析完成: N 页, 耗时 Xs`）
  - [x] `--quiet` 模式下不显示进度
- [x] 日志控制：
  - [x] 默认 WARN 级别
  - [x] `-v` → INFO，`-vv` → DEBUG，`-vvv` → TRACE
  - [x] `--quiet` → 禁用所有日志
- [x] 错误处理：
  - [x] 文件不存在 → 友好提示 + 退出码 1
  - [x] PDF 加密 → 退出码 2
  - [x] PDF 损坏 → 退出码 3
  - [x] 其他错误 → 错误信息 + 退出码 1
- [x] 配置合并：命令行参数 > 配置文件 > 默认值

### 9. 页码范围解析

- [x] 支持多种格式：
  - [x] 单页：`3`
  - [x] 范围：`1-5`
  - [x] 列表：`1,3,5`
  - [x] 混合：`1-3,5,8-10`
  - [x] 末尾省略：`5-`（第 5 页到最后）
- [x] 错误处理（超出范围 → 跳过；全部超出 → 报错）

### 10. 测试

- [x] 单元测试：
  - [x] 页码范围解析器（12 个测试）
- [x] 集成测试（CLI 端到端，20 个测试）：
  - [x] `parse` 子命令 → 验证 JSON 输出可反序列化 + --pages 过滤
  - [x] `markdown` 子命令 → 验证 Markdown 输出非空 + --no-tables
  - [x] `rag` 子命令 → 验证 text/jsonl/csv 格式
  - [x] `info` 子命令 → 验证信息完整性 + --json
  - [x] `config show` → 验证 TOML 格式
  - [x] `config init` → 验证文件生成 + 重复跳过
  - [x] `config path` → 验证搜索路径
  - [x] 错误场景：文件不存在（退出码 1）、quiet 模式

---

## 命令行使用示例

```bash
# 解析 PDF 为 IR JSON
knot-pdf-cli parse report.pdf -o report.json --pretty

# 导出 Markdown
knot-pdf-cli markdown report.pdf -o report.md

# 导出 RAG 文本（第 1-5 页，JSONL 格式）
knot-pdf-cli rag report.pdf --pages 1-5 --format jsonl -o report.jsonl

# 查看 PDF 信息
knot-pdf-cli info report.pdf

# 查看 PDF 信息（JSON 格式）
knot-pdf-cli info report.pdf --json

# 使用指定配置文件
knot-pdf-cli -c custom.toml parse report.pdf

# 静默模式 + 输出到文件
knot-pdf-cli -q markdown report.pdf -o output.md

# 初始化配置文件
knot-pdf-cli config init

# 显示当前配置
knot-pdf-cli config show

# 详细模式（调试用）
knot-pdf-cli -vv parse report.pdf --include-timings
```

---

## 完成标准

- [x] `knot-pdf-cli` 可独立编译运行（`cargo build --features cli`）
- [x] 5 个子命令全部可用（parse / markdown / rag / info / config）
- [x] `--help` 显示完整说明
- [x] 支持 `-o` 输出到文件 / stdout
- [x] 支持页码范围过滤（单页/范围/列表/混合/开放末尾）
- [x] 支持 TOML 配置文件加载（`--config` 指定 / `load_auto` 自动搜索）
- [x] 错误场景返回正确退出码（0/1/2/3）
- [x] 集成测试全部通过（20 个端到端 + 12 个单元测试）
- [x] README 包含完整 CLI 使用文档
