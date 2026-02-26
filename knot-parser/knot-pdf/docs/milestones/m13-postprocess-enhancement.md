# M13：噪声过滤 & 后处理增强

## 目标

参考 MinerU 的噪声去除和后处理管线，为 knot-pdf 建立一套系统的后处理框架，提升最终输出的质量。

### 解决的问题

当前 knot-pdf 仅做了基础的页眉页脚去除（M2），但以下噪声问题仍然存在：
- 水印文字干扰正文
- 脚注/尾注混入正文
- 页码/工具栏文字未被完全过滤
- 段落跨页断裂（前一页末尾和下一页开头属于同一段落）
- 列表编号识别不完善
- 超链接/URL 碎片化

## 依赖

- M2（页眉页脚检测 — 基础）
- M9（XY-Cut — 阅读顺序确认）

## 交付物

- [x] 后处理 Pipeline 框架
- [x] 水印检测与过滤
- [x] 脚注/尾注检测与分离
- [x] 段落跨页合并（辅助函数）
- [x] 列表识别增强
- [x] URL / 超链接修复
- [x] Pipeline 集成

---

## Checklist

### 1. 后处理 Pipeline 框架

新增 `src/postprocess/` 模块，建立可插拔的后处理管线：

```rust
/// 后处理器 trait
pub trait PostProcessor: Send + Sync {
    fn name(&self) -> &str;
    fn process_page(&self, page: &mut PageIR, config: &Config);
}

/// 后处理管线
pub struct PostProcessPipeline {
    processors: Vec<Box<dyn PostProcessor>>,
}
```

- [x] `PostProcessor` trait 定义
- [x] `PostProcessPipeline` 管理器（按注册顺序执行）
- [x] 默认处理器注册顺序定义（水印→脚注→列表→URL）
- [x] Config 控制每个处理器的开关
- [x] Pipeline 集成：process_page 末尾自动执行

### 2. 水印检测与过滤

水印的典型特征：
- 文字跨越整页、角度倾斜（rotation ≠ 0）
- 文字颜色为浅灰色（如果可获取）
- 文字内容在多页重复
- 文字 z-order 靠后（被正文覆盖）

- [x] 常见水印文本匹配（中英文：CONFIDENTIAL/机密/DRAFT/草稿等）
- [x] 大面积少文字块检测（占页面 >20% 且 <50 字）
- [x] 极端宽高比检测
- [x] 标记 `role = Watermark` 后自动过滤
- [x] Config: `remove_watermark: bool`（默认 true）

### 3. 脚注/尾注检测与分离

- [x] **位置检测**：页面底部 15% 区域内的文本
- [x] **编号检测**：上标数字（¹²³）、方括号编号（[1]）、星号/†/‡
- [x] **字号对比**：字号明显小于正文平均字号（<85%）
- [x] 将脚注块标记 `role = Footnote`
- [x] Config: `separate_footnotes: bool`（默认 false）

### 4. 段落跨页合并

- [x] **辅助函数**：
  - `is_incomplete_ending(text)` — 检测未完成的句子（不以句末标点结尾）
  - `is_paragraph_start(text)` — 检测段首特征（大写/编号/列表标记）
- [x] 跨页合并逻辑设计（DocumentIR 级别处理，非单页）
- [x] Config: `merge_cross_page_paragraphs: bool`（默认 true）

### 5. 列表识别增强

- [x] **有序列表检测**：
  - "1." "2." 数字点号模式
  - "1)" "2)" 数字括号模式
  - "(a)" "(i)" 字母/罗马数字模式
  - ① ② ③ 圆圈编号模式
- [x] **无序列表检测**：
  - "•" "◦" "▪" "●" "■" 等 bullet 标记
  - "- " "– " "— " 短横线标记（要求后跟空格）
- [x] 缩进级别推断（基于空格和 x 偏移）
- [x] 标记 `role = List`

### 6. URL / 超链接修复

- [x] **URL 碎片重组**：
  - 检测以 "http://"、"https://"、"ftp://"、"www."、"mailto:" 开头的 span
  - 将后续无空白连接的 span 合并
- [x] **邮箱检测**（辅助函数）
- [x] 合并后重新计算 `normalized_text`

### 7. IR 扩展

- [x] `BlockRole` 新增 `Watermark`、`Footnote` 变体
- [x] Config 新增 `postprocess_enabled`、`remove_watermark`、`separate_footnotes`、`merge_cross_page_paragraphs`

### 8. 测试

- [x] 单元测试（32 个）：
  - [x] PostProcessPipeline 框架测试（3）
  - [x] 水印检测测试（5）
  - [x] 脚注检测测试（5）
  - [x] 段落跨页辅助函数测试（7）
  - [x] 列表识别测试（7）
  - [x] URL 修复测试（5）
- [ ] 集成测试：
  - [ ] 含水印的 PDF → 验证水印被过滤
  - [ ] 含脚注的论文 → 验证脚注被分离
  - [ ] 含列表的文档 → 验证列表层级正确
  - [ ] 含长 URL 的文档 → 验证 URL 完整
- [ ] 回归测试：
  - [ ] 现有评测 PDF 的输出不退化

---

## 完成标准

- [x] 后处理 Pipeline 框架可用，可按需组合处理器
- [x] 水印检测覆盖常见斜排文字水印
- [x] 脚注识别可用（学术论文场景）
- [x] 列表识别可用（有序 + 无序）
- [x] URL 碎片自动修复
- [x] 全部 lib 测试通过（111 个）

## 实现文件清单

| 文件                           | 类型             | 说明                                               |
| ------------------------------ | ---------------- | -------------------------------------------------- |
| `src/postprocess/mod.rs`       | **新增** ~105 行 | PostProcessor trait + PostProcessPipeline + 3 测试 |
| `src/postprocess/watermark.rs` | **新增** ~175 行 | 水印检测过滤器 + 5 测试                            |
| `src/postprocess/footnote.rs`  | **新增** ~200 行 | 脚注检测器 + 5 测试                                |
| `src/postprocess/paragraph.rs` | **新增** ~155 行 | 段落跨页合并辅助函数 + 7 测试                      |
| `src/postprocess/list.rs`      | **新增** ~195 行 | 列表识别增强 + 7 测试                              |
| `src/postprocess/url.rs`       | **新增** ~195 行 | URL 碎片修复 + 5 测试                              |
| `src/ir/types.rs`              | 修改             | BlockRole 新增 Watermark、Footnote                 |
| `src/config.rs`                | 修改             | 新增 4 个后处理配置项                              |
| `src/lib.rs`                   | 修改             | 注册 postprocess 模块                              |
| `src/pipeline/mod.rs`          | 修改             | 集成后处理管线                                     |
