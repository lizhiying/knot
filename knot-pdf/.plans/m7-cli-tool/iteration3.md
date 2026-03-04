# Milestone: M7 CLI 命令行工具
# Iteration: 3 — 体验打磨 + 测试

## 目标

提升用户体验（进度条、退出码、日志控制）、完善集成测试、编写 README 使用文档。完成后 CLI 工具达到可发布质量。

## 假设

- Iteration 1 + 2 已完成，5 个子命令全部可用
- 功能逻辑已正确，本轮聚焦体验和质量

## 范围

- 进度显示与静默模式
- 日志级别控制（-v / -vv / -vvv / -q）
- 标准退出码（0 成功 / 1 一般错误 / 2 加密 / 3 损坏）
- CLI 端到端集成测试
- README 使用文档
- 配置合并逻辑（命令行参数 > 配置文件 > 默认值）

## 任务

- [x] 实现进度显示：解析时输出 `正在解析.../解析完成: N 页, 耗时 Xs`，`--quiet` 模式下抑制
- [x] 实现日志级别控制：`-v` → INFO、`-vv` → DEBUG、`-vvv` → TRACE、`-q` → 禁用
- [x] 规范退出码：成功=0、一般错误=1、加密=2、损坏=3
- [x] 实现配置合并：命令行中的参数（如 `--no-tables`）覆盖配置文件中的对应值
- [x] CLI 集成测试（`tests/m7_cli_tests.rs`）：20 个测试覆盖 5 个子命令 + 错误场景
- [x] 在主 README 添加 CLI 使用章节（命令一览 / 使用示例 / 全局选项 / 页码格式 / 退出码）

## 退出标准

- [x] 解析大 PDF 时终端显示进度信息
- [x] `--quiet` 模式下无任何额外输出
- [x] `-vvv` 模式下输出详细日志
- [x] 加密 PDF → 退出码 2，损坏 PDF → 退出码 3
- [x] 集成测试全部通过
- [x] README 包含完整的 CLI 使用示例

## 验证结果（2026-02-25）

```
✅ cargo test --features cli --test m7_cli_tests
   test result: ok. 20 passed; 0 failed; 0 ignored

   测试明细：
   - test_help_output              ✅
   - test_version_output           ✅
   - test_parse_help               ✅
   - test_parse_nonexistent_file   ✅ (退出码 1)
   - test_parse_quiet_nonexistent  ✅ (quiet 模式无输出)
   - test_parse_to_file            ✅ (JSON 可反序列化, --pages 过滤正确)
   - test_parse_pretty             ✅ (缩进 JSON)
   - test_markdown_to_file         ✅ (输出含 Page 标记)
   - test_markdown_no_tables       ✅ (--no-tables 减少输出)
   - test_rag_text_output          ✅ (含 "页=1" 文本)
   - test_rag_jsonl_output         ✅ (每行有效 JSON)
   - test_rag_csv_output           ✅ (含 CSV 表头)
   - test_info_text_output         ✅ (含文件名/页数/元数据/概览)
   - test_info_json_output         ✅ (total_pages=100, 可反序列化)
   - test_config_show              ✅ (含 TOML 配置键)
   - test_config_init              ✅ (生成文件)
   - test_config_init_no_overwrite ✅ (重复 init 跳过)
   - test_config_path              ✅ (3 个搜索路径)
   - test_exit_code_file_not_found ✅ (退出码 1)
   - test_pages_filter             ✅ (--pages 2,5 → 2 页, page_index 正确)
```

