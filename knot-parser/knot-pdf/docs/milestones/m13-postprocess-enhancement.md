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

- [ ] 水印检测与过滤
- [ ] 脚注/尾注检测与分离
- [ ] 段落跨页合并
- [ ] 列表识别增强
- [ ] URL / 超链接修复
- [ ] 后处理 Pipeline 框架

---

## Checklist

### 1. 后处理 Pipeline 框架

新增 `src/postprocess/` 模块，建立可插拔的后处理管线：

```rust
/// 后处理器 trait
pub trait PostProcessor: Send + Sync {
    fn name(&self) -> &str;
    fn process(&self, doc: &mut DocumentIR, config: &Config);
}

/// 后处理管线
pub struct PostProcessPipeline {
    processors: Vec<Box<dyn PostProcessor>>,
}
```

- [ ] `PostProcessor` trait 定义
- [ ] `PostProcessPipeline` 管理器（按注册顺序执行）
- [ ] 默认处理器注册顺序定义
- [ ] Config 控制每个处理器的开关

### 2. 水印检测与过滤

水印的典型特征：
- 文字跨越整页、角度倾斜（rotation ≠ 0）
- 文字颜色为浅灰色（如果可获取）
- 文字内容在多页重复
- 文字 z-order 靠后（被正文覆盖）

- [ ] 检测倾斜文本块（rotation > 10° 的文本）
- [ ] 检测跨页重复的大面积文本（非页眉页脚区域）
- [ ] 滤波：将检测到的水印块标记 `role = Watermark`
- [ ] Config: `remove_watermark: bool`（默认 true）

### 3. 脚注/尾注检测与分离

- [ ] **位置检测**：页面底部 15% 区域内的小字号文本
- [ ] **编号检测**：以数字 + 上标或方括号开头（如 "¹"、"[1]"）
- [ ] **分隔线检测**：脚注区域上方是否有水平短线
- [ ] 将脚注块标记 `role = Footnote`
- [ ] 在 `PageIR` 中新增 `footnotes: Vec<FootnoteIR>` 字段（可选分离输出）
- [ ] Config: `separate_footnotes: bool`（默认 false）

### 4. 段落跨页合并

MinerU 的后处理会尝试将跨页断裂的段落合并：

- [ ] **断裂检测**：
  - 前一页最后一个 block 不以句号/问号/叹号结尾
  - 下一页第一个 block 不以大写字母/数字序号开头
  - 且两者在 x 坐标范围相近
- [ ] **合并策略**：
  - 标记 `BlockIR` 的 `continues_from_previous_page: bool`
  - 在 DocumentIR 级别提供 `merged_paragraphs()` 方法
- [ ] Config: `merge_cross_page_paragraphs: bool`（默认 true）

### 5. 列表识别增强

参考 MinerU 的列表识别规则：

- [ ] **有序列表检测**：
  - "1."、"2."、"(a)"、"(i)"、"①" 等模式
  - 连续编号 + x 坐标对齐
- [ ] **无序列表检测**：
  - "•"、"-"、"–"、"▪"、"◦" 等标记
  - 缩进一致
- [ ] 标记 `role = ListItem`
- [ ] 在 `BlockIR` 中新增 `list_level: Option<u32>`（嵌套级别）

### 6. URL / 超链接修复

PDF 中的长 URL 常被拆成多个 span 甚至多个 block：

- [ ] **URL 碎片重组**：
  - 检测以 "http://"、"https://"、"mailto:" 开头的 span
  - 将后续无空白连接的 span 合并
- [ ] **邮箱检测**：
  - 检测 `xxx@xxx.xxx` 模式
  - 确保不被拆分
- [ ] 在 span 级别标记 `is_url: bool`

### 7. 测试

- [ ] 每个后处理器的单元测试
- [ ] 集成测试：
  - [ ] 含水印的 PDF → 验证水印被过滤
  - [ ] 含脚注的论文 → 验证脚注被分离
  - [ ] 跨页段落 → 验证合并标记正确
  - [ ] 含列表的文档 → 验证列表层级正确
  - [ ] 含长 URL 的文档 → 验证 URL 完整
- [ ] 回归测试：
  - [ ] 现有 65 份评测 PDF 的输出不退化

---

## 完成标准

- [ ] 后处理 Pipeline 框架可用，可按需组合处理器
- [ ] 水印检测覆盖常见斜排文字水印
- [ ] 脚注识别准确率 > 80%（在学术论文场景下）
- [ ] 跨页段落合并标记正确
- [ ] 列表层级识别可用
- [ ] URL 不被碎片化
- [ ] 现有评测指标不退化
- [ ] 全部测试通过
