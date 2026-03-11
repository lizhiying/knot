# Excel 解析与索引架构

## 概述

Knot 对 Excel 文件采用 **双引擎** 架构：
- **Tantivy 全文索引**：负责"搜索发现"——用户输入关键词时找到相关 Excel 文件
- **DuckDB 持久缓存**：负责"数据查询"——找到文件后用 SQL 精准查询表格数据

## 索引结构

每个 Excel 文件生成 **3 个 Chunk**（而非之前每个 DataBlock 一个）：

| Chunk | 内容 | 用途 |
|-------|------|------|
| **摘要 chunk** | Sheet 名 + 列名 + 前 3 行样本 | 展示概览、向量搜索 |
| **关键词索引 chunk** | 所有单元格的去重文本值 | Tantivy 全文搜索 |
| **doc-summary** | 自动生成的文档概览 | 文档级搜索匹配 |

### 为什么需要关键词索引 chunk？

假设一个 Excel 文件有 1000 行，用户搜索某个出现在第 500 行的客户名（如"华东供应商"）。如果只保留前 3 行样本，这个关键词不会被 Tantivy 索引，文件就搜不到了。

关键词索引 chunk 从所有行中提取 **去重的文本值**，确保任何单元格内容都能被搜索命中。

### 关键词索引的保护机制

- **纯数字不入索引**：数字查询交给 DuckDB SQL 处理，避免关键词膨胀
- **32KB 截断上限**：超大 Excel 文件做截断保护
- **去重**：同一个值出现多次也只索引 1 次

## DuckDB 持久缓存

### 缓存文件

位于索引目录下：`knot_excel_cache.duckdb`

### 三层更新策略

| 层级 | 时机 | 说明 |
|------|------|------|
| **Layer 1: 索引时写入** | 启动时初始扫描完成后 | 遍历所有 Excel 文件，解析并写入 DuckDB |
| **Layer 2: 文件监控更新** | debounce 后批量更新 | 文件变更时重新解析并更新缓存 |
| **Layer 3: 查询时校验** | 搜索时 mtime 不匹配则懒更新 | 双重保险，确保缓存数据新鲜 |

### 缓存校验

使用文件的 **mtime（修改时间）+ size（文件大小）** 做快速校验。校验不通过时才重新解析。

### 数据表命名

每个 DataBlock 在 DuckDB 中对应一张表，表名格式：
```
t_{file_hash_prefix}_{sheet_name}_{block_index}
```
例如：`t_a1b2c3_销售数据_0`

## 搜索流程

```
用户搜索 "华东供应商"
  ↓
① Tantivy 关键词匹配 → 命中关键词索引 chunk
  ↓
② 找到文件: 销售报表.xlsx
  ↓
③ 检查 DuckDB 缓存有效性（mtime + size）
  ↓ 有效                    ↓ 过期
  直接使用缓存引擎           重新解析 → 更新缓存
  ↓                         ↓
④ LLM 生成 SQL: SELECT * FROM t_xxx WHERE 客户 LIKE '%华东供应商%'
  ↓
⑤ DuckDB 执行 SQL → 返回精准结果
  ↓
⑥ ResultSummarizer 格式化 → 注入 RAG 上下文
```

### 预算控制

搜索时会估算表格数据的 Markdown 字符量：
- **小表格**（总量 < 上下文预算 × 0.9）：直接注入 Markdown 表格，最多 50 行
- **大表格**（超预算）：使用 DuckDB Text-to-SQL，LLM 生成 SQL 精准查询

### SQL 容错

LLM 生成的 SQL 可能有语法错误，系统内置 **最多 2 次重试**：
1. 将错误信息反馈给 LLM
2. LLM 生成修复后的 SQL
3. 重新执行

## 临时文件过滤

以下文件在索引和 Knowledge 列表中被自动排除：

| 类型 | 示例 | 来源 |
|------|------|------|
| Office 临时锁文件 | `~$报表.xlsx` | Excel/Word/PPT 编辑时 |
| 编辑器备份文件 | `file.txt~`, `.swp`, `.bak` | Emacs, Vim 等 |
| 系统文件 | `Thumbs.db`, `desktop.ini` | Windows |
| 下载临时文件 | `.part`, `.crdownload` | 浏览器下载中 |
| 锁文件 | `.lock`, `.lck` | 各种应用 |

## 相关代码

| 模块 | 文件 | 说明 |
|------|------|------|
| Excel Parser | `knot-parser/src/formats/excel.rs` | 生成摘要 + 关键词索引 chunk |
| DuckDB 缓存 | `knot-excel/src/query/cache.rs` | ExcelCache 持久缓存管理 |
| 索引器 | `knot-core/src/index.rs` | KnotIndexer，含 PARSER_VERSION |
| 文件过滤 | `knot-core/src/monitor.rs` | should_index_file() |
| RAG 搜索 | `knot-app/src-tauri/src/main.rs` | rag_search 中的 DuckDB 查询路径 |

## 版本历史

| 版本 | 变更 |
|------|------|
| v1 | 初始版本 |
| v2 | Excel 智能表头检测（跳过标题行），改进财务报表解析 |
| v3 | Excel 索引改为摘要 + 关键词索引（详细数据存 DuckDB） |
