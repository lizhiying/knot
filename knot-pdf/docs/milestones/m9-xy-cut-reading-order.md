# M9：XY-Cut 阅读顺序算法

## 目标

引入 MinerU 采用的 **XY-Cut 递归分割算法**，替代当前基于简单几何排序的阅读顺序逻辑。XY-Cut 是一种纯几何算法，不依赖任何模型，通过递归地在水平（X）和垂直（Y）方向寻找最大空白切割线，天然支持多栏布局的正确阅读顺序。

### 解决的问题

当前 `reading_order.rs` 的启发式排序在以下场景表现不佳：
- 双栏或三栏论文：跨栏合并、栏间顺序混乱
- 混合布局（标题跨栏 + 正文双栏）：标题被错误切到某一栏
- 中间插图/表格中断文本流的场景

### 参考

- MinerU 依赖：[xy-cut](https://github.com/Sanster/xy-cut)
- MinerU 依赖：[layoutreader](https://github.com/ppaanngggg/layoutreader)

## 依赖

- M1 ~ M8（现有阅读顺序模块 `layout/reading_order.rs`）

## 交付物

- [x] XY-Cut 阅读顺序算法 Rust 实现
- [x] 与现有阅读顺序逻辑集成（可通过配置切换）
- [x] 多栏/混合布局测试覆盖
- [x] 性能基准（XyCut 开销 -1.3%~-2.8%，远低于 20% 目标上限）

---

## Checklist

### 1. XY-Cut 核心算法

新增文件 `src/layout/xy_cut.rs`：

```rust
/// XY-Cut 递归分割结果
pub struct XyCutNode {
    /// 包围框
    pub bbox: BBox,
    /// 切割方向
    pub cut_direction: CutDirection,
    /// 子节点（叶子节点则包含 block 索引列表）
    pub children: Vec<XyCutNode>,
    /// 叶子节点中的 block 索引
    pub block_indices: Vec<usize>,
}

pub enum CutDirection {
    /// 水平切割（Y 方向分割，上下分组）
    Horizontal,
    /// 垂直切割（X 方向分割，左右分组）
    Vertical,
    /// 叶子节点（不再分割）
    Leaf,
}
```

**算法步骤**：

1. **投影分析**：将所有 block 的 bbox 分别投影到 X 轴和 Y 轴
2. **空白间隙检测**：在投影上找到大于阈值的间隙（gap）
3. **选择最佳切割**：
   - 如果 Y 方向有明显间隙 → 水平切割（先处理上部，再处理下部）
   - 如果 X 方向有明显间隙 → 垂直切割（先处理左侧，再处理右侧）
   - 如果两个方向都有间隙 → 选择间隙比例更大的方向
   - **多栏区域合并**：Y 切割后检查相邻子组是否有一致的 X 间隙模式，如有则合并
4. **递归**：对每个分割出的子区域递归执行，直到无法再分割（叶子节点）
5. **排序**：叶子节点内部按 top→bottom, left→right 排序

- [x] 实现 XY 轴投影与间隙检测（`find_projection_gaps_x/y`）
- [x] 实现切割方向选择（`select_cut_direction`）
- [x] 实现递归分割（`xy_cut_full`）
- [x] 实现多栏区域合并（`merge_multicolumn_groups`）
- [x] 实现最终排序：将递归树展平为线性阅读顺序（`flatten_node`）
- [x] 间隙阈值参数化（默认：页面宽度的 2%，可配置）

### 2. 与现有模块集成

在 `src/layout/mod.rs` 中新增 `xy_cut` 模块，并在 Pipeline 中集成：

```rust
pub enum ReadingOrderMethod {
    /// 现有的启发式排序（默认）
    Heuristic,
    /// XY-Cut 递归分割（推荐用于多栏文档）
    XyCut,
    /// 自动选择（根据页面特征选择最佳方法）
    Auto,
}
```

- [x] `Config` 新增 `reading_order_method: ReadingOrderMethod`
- [x] `Auto` 模式：如果检测到多栏（`has_multi_column_layout` 分析）→ 使用 XyCut，否则使用 Heuristic
- [x] Pipeline `process_page()` 中使用 `build_blocks_with_config()` 根据配置选择算法
- [x] 保证向后兼容：默认值为 `Auto`

### 3. 间隙阈值自适应

- [ ] 根据页面字体大小中位数（`median_font_size`）动态调整间隙阈值
- [ ] 较小字体（论文/报告）→ 较小间隙阈值
- [ ] 较大字体（PPT/海报）→ 较大间隙阈值
- [x] 配置项 `xy_cut_gap_ratio: f32`（默认 0.02，即页面宽度的 2%）

### 4. 混合布局处理

针对"标题跨栏 + 正文双栏"的常见场景：

- [x] 先做 Y-cut 分离出标题区域和正文区域
- [x] 通过 `merge_multicolumn_groups` 将连续的多栏行合并，再对合并区域做 X-cut 分离左右栏
- [x] 验证标题不会被错误切入某一栏

### 5. 测试

- [x] 单元测试：
  - [x] 单栏文档排序正确（`test_single_column_top_to_bottom`）
  - [x] 双栏文档：左栏在前，右栏在后（`test_two_columns`）
  - [x] 三栏文档排序正确（`test_three_columns`）
  - [x] 标题跨栏 + 双栏正文：标题在最前（`test_title_spanning_two_columns`）
  - [x] 中间插图中断的场景（`test_figure_interrupting_text`）
  - [x] 空页面 / 单个 block 的边界情况（`test_empty_input` / `test_single_element`）
  - [x] 学术论文布局（Title + Author + Abstract + 双栏正文）（`test_academic_paper_layout`）
  - [x] 间隙检测基础测试 3 个（`test_find_gaps_*`）
  - [x] 分组测试（`test_split_by_gaps_horizontal`）
- [x] 对比测试：
  - [x] 对同一 PDF 比较 Heuristic 和 XyCut 的输出差异（`test_xycut_multi_column_eval_sample`）
  - [x] 使用 "Attention Is All You Need" 等真实论文验证（`test_xycut_attention_paper_reading_order`）
  - [x] 10 个 born_digital 评测样本回归测试（`test_xycut_no_regression_on_eval_samples`）
- [x] 性能基准：
  - [x] Attention Paper (15p): Heuristic 6487ms vs XyCut 6304ms (**-2.8%**)
  - [x] 100 页 PDF: Heuristic 14419ms vs XyCut 14229ms vs Auto 14199ms (**-1.3%**)

---

## 完成标准

- [x] XY-Cut 算法正确实现，递归深度可控
- [x] 10/10 评测样本通过 XyCut 回归测试
- [x] 不影响单栏文档的现有排序质量（Auto 与 Heuristic 输出一致）
- [x] 配置可切换算法（Heuristic / XyCut / Auto）
- [x] 性能开销负值（XyCut 比 Heuristic 快 1~3%）✅ 远优于 <20% 目标
- [x] 全部测试通过：12个单元测试 + 7个集成测试 + 2个性能基准

## 实现亮点

### 多栏区域合并 (merge_multicolumn_groups)

这是 XY-Cut 实现中最关键的创新点。标准 XY-Cut 算法对"标题跨栏 + 正文双栏"的混合布局处理不佳——它会先做 Y 切割把正文按行拆分，导致左右栏被混在一起。

解决方案：Y 切割后检查相邻子组是否有一致的 X 间隙模式（通过 `has_similar_x_gaps` 比较间隙中心位置），如果有，则将它们合并回一个大组，让后续递归对这个大组做 X 切割来正确分离左右栏。

### 切割方向选择 (select_cut_direction)

当 Y 和 X 方向都有间隙时，传统做法是简单比较间隙大小。但在双栏场景中，Y 间隙（行间距）通常比 X 间隙（列间距）更大，会错误地选择 Y 切割。

我们的策略：检查 Y 切割后每个子组内是否仍存在 X 间隙——如果是，说明是多栏布局，应优先 X 切割。
