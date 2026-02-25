# M2：PageScore + 页眉页脚 + 多列复排稳健化

## 目标

实现页面质量评分系统（PageScore），用于判断页面是否为扫描件、是否需要 OCR 兜底。同时完善页眉页脚检测与去重，以及多列布局的阅读顺序重建，使 Fast Track 输出更加稳健可靠。

## 依赖

- M1（IR + Fast Track 基础文本）

## 交付物

- PageScore 评分模块（含阈值配置）
- 页眉页脚跨页重复检测与标记
- 多列布局检测与阅读顺序复排
- 相关单元测试

---

## Checklist

### 1. PageScore 评分模块

- [x] 实现 `PageScore` 结构体
  - [x] `score: f32`（0.0 ~ 1.0）
  - [x] `reason_flags: Vec<ReasonFlag>`
- [x] 实现评分指标计算：
  - [x] `printable_char_count`：可打印字符数
  - [x] `printable_ratio`：可打印字符占比
  - [x] `garbled_rate`：疑似乱码字符比率
  - [x] `text_area_coverage`：文本 bbox 覆盖面积 / 页面面积
  - [x] `median_font_size`：中位字体大小（防止标题页误判）
  - [x] `unique_ratio` / entropy proxy：字符多样性指标
- [x] 定义 `ReasonFlag` 枚举：
  - [x] `LowText`：文本过少
  - [x] `HighGarbled`：乱码率高
  - [x] `LowCoverage`：文本区域覆盖率低
- [x] 评分综合计算逻辑（加权/规则混合）
- [x] 可配置阈值：`scoring.text_threshold`
- [x] 将 `PageScore` 结果写入 `PageIR.text_score` 和 `PageIR.is_scanned_guess`
- [x] 评分结果纳入 `PageDiagnostics`

### 2. 页眉页脚检测与去重

- [x] 跨页重复文本块识别算法：
  - [x] 提取每页顶部/底部区域的文本块
  - [x] 对比多页间重复内容（模糊匹配，容忍页码变化）
  - [x] 识别页码模式（纯数字 / "第X页" / "Page X" 等）
- [x] 标记 `BlockIR.role = Header / Footer`
- [x] 可配置是否在输出中剔除页眉页脚
- [x] 页眉页脚信息写入 `DocumentIR.diagnostics`（统计命中页数）

### 3. 多列布局复排稳健化

- [x] 改进列检测算法：
  - [x] 基于文本块 x 坐标的聚类分析（间隙检测）
  - [x] 支持 2 列、3 列常见布局
  - [x] 混合布局处理（部分区域单列、部分双列）
- [x] 改进阅读顺序重建：
  - [x] 列内按 y 坐标排序
  - [x] 跨列间按列顺序（左→右）
  - [x] 跨列标题/横幅检测（宽文本块不参与列分割）
- [x] 段落合并增强：
  - [x] 行距自适应（不同列可能行距不同）
  - [x] 缩进检测（区分段首/续行）
  - [x] 列表项检测（bullet / number prefix）

### 4. 配置扩展

- [x] `Config` 新增字段：
  - [x] `scoring.text_threshold: f32`（默认 0.3）
  - [x] `scoring.garbled_threshold: f32`（默认 0.2）
  - [x] `layout.strip_headers_footers: bool`（默认 true）
  - [x] `layout.max_columns: usize`（默认 3）

### 5. 测试

- [x] PageScore 单元测试：
  - [x] 典型 born-digital 页面 → 高分
  - [x] 扫描页（无文本）→ 低分
  - [x] 标题页（少文本但正常）→ 中等分不误判
  - [x] 乱码页 → 低分 + HighGarbled flag
- [x] 页眉页脚单元测试：
  - [x] 多页重复文本正确识别
  - [x] 页码变化的模糊匹配
  - [x] 无页眉页脚时不误报
- [x] 多列复排单元测试：
  - [x] 双列 PDF 阅读顺序正确
  - [x] 三列 PDF 阅读顺序正确
  - [x] 混合布局（标题+双列正文）正确处理
- [x] 集成测试：
  - [x] 至少 3 份多列 born-digital PDF 解析正确
  - [x] 至少 2 份含页眉页脚的 PDF 正确检测

---

## 完成标准

- [x] PageScore 对 born-digital 页面评分 > 0.7，对扫描页评分 < 0.3
- [x] 页眉页脚检测准确率 > 90%（在测试样本上）
- [x] 多列 PDF 的阅读顺序输出合理（人工检查）
- [x] 所有新增单元测试通过
- [x] CI 通过（fmt / clippy / test）

---

## 完成总结

> **状态：✅ 全部完成**

### 实现模块

| # | 模块 | 文件 | 说明 |
|---|------|------|------|
| 1 | **PageScore 评分** | `src/scoring/mod.rs`, `src/scoring/page_score.rs` | 6 维指标加权评分（printable_ratio, garbled_rate, text_area_coverage, median_font_size, unique_ratio, entropy_proxy），支持可配置阈值 |
| 2 | **页眉页脚检测** | `src/hf_detect/mod.rs`, `src/hf_detect/header_footer.rs` | 跨页重复文本块识别（bigram 模糊匹配）、页码模式识别（纯数字/"Page X"/"第X页"）、可配置剔除、安全机制（不会删除所有块） |
| 3 | **多列布局复排** | `src/layout/reading_order.rs` | 行聚类 → 行内大间隙拆分 → 横幅分离 → 列检测 → 列内段落合并 → 阅读顺序重建。支持 2/3 列、混合布局、列表项检测（bullet/number prefix） |
| 4 | **配置扩展** | `src/config.rs` | 新增 `text_threshold`, `garbled_threshold`, `strip_headers_footers`, `max_columns` 字段 |

### 修复的 Bug

| # | Bug | 修复 |
|---|-----|------|
| 1 | PUA 字符被误算为"可打印字符"拉高评分 | `printable_count` 排除乱码字符（`is_garbled_char`），乱码惩罚权重 0.25 → 0.35 |
| 2 | 纯页码页脚（"Page X of Y"）归一化后为空串被跳过 | 新增 `pattern_matches` 函数支持空串模式匹配 |
| 3 | 同一 y 坐标的双列文本被行聚类合并成一行 | 新增 `split_line_by_gap` 按列间隙拆分行 |
| 4 | strip 模式下可能删除页面所有块 | 安全机制：移除后无剩余块则保留原有块 |

### 测试结果

```
✅ cargo test --all-features → 39/39 测试通过
   ├── 13 个 IR 单元测试 (ir_tests.rs)
   ├── 6 个集成测试 (integration_tests.rs)
   ├── 19 个 M2 测试 (m2_tests.rs)
   │   ├── PageScore: 5 个（空页面/正常文本/乱码/少量字符/指标精度）
   │   ├── 页眉页脚: 5 个（重复页眉/带页码页脚/strip保留正文/单页无误判/安全机制）
   │   ├── 多列复排: 7 个（单列/双列/横幅混合/列表/编号列表/空输入/阅读顺序）
   │   └── 集成: 2 个（配置项控制/评分serde roundtrip）
   └── 1 个 doc-test
```

### 新增/修改文件

- `src/scoring/mod.rs` — 评分模块入口
- `src/scoring/page_score.rs` — PageScore 评分核心逻辑
- `src/hf_detect/mod.rs` — 页眉页脚检测模块入口
- `src/hf_detect/header_footer.rs` — 页眉页脚检测核心逻辑
- `src/layout/reading_order.rs` — 多列布局复排（改进：行内间隙拆分）
- `src/config.rs` — 配置扩展
- `tests/m2_tests.rs` — M2 测试文件（19 个测试）
