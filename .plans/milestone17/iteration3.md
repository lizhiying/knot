Milestone: milestone17
Iteration: 复杂报表处理 — 合并单元格 + 多级表头降维

Goal:
处理"中国式复杂报表"：多级表头降维拼接、合并单元格空值填充、脏数据行过滤。
提升 `knot-excel` 对真实业务场景中复杂 Excel 文件的解析能力。

Assumptions:
- Iteration 1 和 2 已完成：标准 Excel 索引和 Text-to-SQL 查询已可用
- Calamine 的 `merged_regions()` API 能正确返回合并区域坐标
- Polars 的 `.forward_fill()` 可用于处理垂直合并导致的空值
- 复杂报表中常见模式：多行表头、垂直合并维度列、表头/表尾说明文字
- 启发式算法无法覆盖所有格式，需要降级方案（回退为纯文本输出）

Scope:
- 获取和处理合并单元格信息
- 实现多级表头检测与降维拼接
- 实现数据体 forward_fill（垂直合并空值填充）
- 实现脏数据行过滤（说明文字行、合计/备注行）
- 更新 TableProfile 生成逻辑

Tasks:
- [x] 3.1 获取合并单元格信息 — 通过启发式检测识别合并区域的影响（空值继承），构建 `detect_data_start` 跳过合并导致的说明行
- [x] 3.2 实现多级表头检测 — 启发式算法：`detect_header_rows` 通过 text→numeric 转换点判断表头终止行（扫描前 max_header_rows 行的类型统计）
- [x] 3.3 实现多级表头降维拼接 — `merge_multi_level_headers` 将 N 行同一列的表头文本自上而下拼接（如 `["上半年", "Q1", "收入"]` -> `"上半年_Q1_收入"`），智能去重（相邻相同值不重复拼接）
- [x] 3.4 实现数据体 `forward_fill` — 针对前 3 列或 30% 的列（取较小者），空值前向填充。解决了 Excel 垂直合并导致的部门/类别列空值问题
- [x] 3.5 实现脏数据行过滤 — `detect_data_start` 跳过表头前说明行（>=40% 列有值才认为是表头开始），`is_dirty_row` 过滤表尾备注行（匹配 "备注/制表人/合计" 等模式）
- [x] 3.6 更新 `TableProfile` 生成逻辑 — description 已包含 header_levels 和 merged_region_count 信息，to_chunk_text 正确输出降维后的列名

Exit criteria:
- [x] 一个带合并单元格和多级表头的复杂报表（如财务报表）能被正确解析 — test_complex_report.xlsx 3级表头正确降维
- [x] `forward_fill` 后数据完整性不丢失（维度列全部填充，无遗漏空值） — test_forward_fill_behavior 验证
- [x] 降维后的列名可读且唯一（如 `"上半年_Q1_收入"`） — 实际输出验证
- [x] 脏数据行被正确过滤，不影响正常数据行 — test_dirty_row_filter 验证
- [x] TableProfile 正确反映表的复杂度信息 — description 包含层级数
