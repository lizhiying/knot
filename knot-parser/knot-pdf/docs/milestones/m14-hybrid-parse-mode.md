# M14：Hybrid 解析模式 & 多后端统一

## 目标

参考 MinerU 的 **Hybrid Backend** 设计理念，为 knot-pdf 实现统一的混合解析框架。核心思想是：**文字型 PDF 走 Fast Track（已有，性能极好），扫描型/图片型 PDF 走 VLM/OCR 通道**，两者的输出统一到同一套 IR 中。

### MinerU Hybrid 的启发

MinerU Hybrid Backend 的核心优势：
1. 文字型 PDF 直接提取文本（快速、准确）
2. 扫描型 PDF 走模型通道（103种语言 OCR、表格模型、公式模型）
3. 两个通道的结果融合到统一输出

knot-pdf 可以借鉴这种分层策略，让已经做得很好的 Fast Track（M1）作为基础，上层叠加可选的模型增强。

## 依赖

- M1 ~ M13（所有前置功能模块）

## 交付物

- [x] 统一 Backend 抽象框架重构
- [x] Fast Track + Model 两层解析的混合策略
- [x] VLM 外部调用接口（trait + Mock 实现）
- [x] 页面级别的解析策略自动选择
- [x] 结果融合逻辑
- [x] Pipeline 集成
- [ ] HTTP VLM 后端实现（待接入实际 API）

---

## Checklist

### 1. 解析模式定义

```rust
pub enum ParseMode {
    /// 仅使用 Fast Track（纯文本提取 + 规则），最快
    FastTrack,
    /// Fast Track + 可选模型增强（版面/表格/公式模型），平衡
    Enhanced,
    /// 完整混合模式：Fast Track + 模型 + VLM 外部调用，最高质量
    Full,
    /// 自动选择：按页面特征决定
    Auto,
}
```

- [x] 在 Config 中新增 `parse_mode: ParseMode`（默认 `Auto`）
- [x] `Auto` 模式的决策逻辑：
  - PageScore ≥ 0.7 → FastTrackOnly
  - PageScore < vlm_score_threshold + VLM 可用 → FullWithVlm
  - 其余 → FastTrackPlusModels

### 2. 模型增强层（Enhanced 模式）

将 M10/M11/M12 的模型功能统一编排：

```
Fast Track 输出 (blocks, tables, images)
         ↓
  Layout Model (M10) → 修正 block roles
         ↓
  Table Model (M11) → 增强表格结构
         ↓
  Formula Detection (M12) → 标记公式区域
         ↓
  Post-processing (M13) → 噪声过滤
         ↓
  最终 PageIR
```

- [x] 已在 Pipeline 中按顺序调用各模型（M10→M12→M13）
- [x] 每个模型独立可开关（Config 中各 feature 控制）
- [x] 模型失败时回退到规则结果（不中断）
- [x] 策略记录到 `PageDiagnostics.parse_strategy`

### 3. VLM 外部调用接口（Full 模式）

- [x] `VlmBackend` trait 定义
- [x] `VlmParseResult` 结构体（markdown + blocks + tables + confidence）
- [x] `MockVlmBackend`：测试用（固定返回内容）
- [x] Markdown → BlockIR 简化转换
- [ ] `HttpVlmBackend`：HTTP API 调用外部 VLM
  - 支持 OpenAI 兼容格式（GPT-4o / Claude / InternVL 等）
  - 配置: `vlm_api_url`, `vlm_api_key`, `vlm_model`
- [ ] Feature gate: `vlm`
- [ ] 超时 + 重试 + 限流

### 4. 页面级策略自动选择

- [x] `select_parse_strategy(text_score, config)` 函数
- [x] 三种策略枚举：`FastTrackOnly` / `FastTrackPlusModels` / `FullWithVlm`
- [x] 策略记录到 `PageDiagnostics.parse_strategy`
- [x] Pipeline.process_page() 中自动调用

### 5. 结果融合

- [x] `fuse_vlm_result(page, vlm_result)` 函数
- [x] **高质量文本**（text_score > 0.5）：保留 Fast Track 文本，VLM 修正 block roles
- [x] **低质量文本**（text_score ≤ 0.5）：VLM 内容补充/替换
- [x] **无 Fast Track 结果**：直接使用 VLM 结果
- [x] 融合来源追踪：`FusionSource::FastTrack / Vlm / Merged`

### 6. 配置项

- [x] `parse_mode: ParseMode`（默认 Auto）
- [x] `vlm_enabled: bool`（默认 false）
- [x] `vlm_api_url: Option<String>`
- [x] `vlm_api_key: Option<String>`
- [x] `vlm_model: Option<String>`
- [x] `vlm_timeout_secs: u32`（默认 30）
- [x] `vlm_max_retries: u32`（默认 2）
- [x] `vlm_score_threshold: f32`（默认 0.3）

### 7. 测试

- [x] 单元测试（18 个）：
  - [x] 策略选择逻辑（10：8 in mod + 2 in strategy）
  - [x] VLM 后端 Mock（4）
  - [x] 结果融合（4）
- [ ] 集成测试：
  - [ ] FastTrack 模式 vs Enhanced 模式的质量对比
  - [ ] Mock VLM 集成端到端测试

---

## 完成标准

- [x] 三种 ParseMode 均可配置（FastTrack / Enhanced / Full）
- [x] Auto 模式正确选择策略
- [x] VLM 后端 trait 可扩展（Mock 已实现，HTTP 待接入）
- [x] 不破坏现有 FastTrack 模式的功能和性能
- [x] 策略选择记录到诊断信息
- [x] 全部 lib 测试通过（129 个）
- [ ] HTTP VLM 后端实际接入验证

## 实现文件清单

| 文件                     | 类型             | 说明                                                |
| ------------------------ | ---------------- | --------------------------------------------------- |
| `src/hybrid/mod.rs`      | **新增** ~90 行  | Hybrid 模块入口 + 8 个策略选择测试                  |
| `src/hybrid/strategy.rs` | **新增** ~110 行 | ParseStrategy 枚举 + select_parse_strategy + 2 测试 |
| `src/hybrid/vlm.rs`      | **新增** ~175 行 | VlmBackend trait + MockVlmBackend + markdown→blocks |
| `src/hybrid/fusion.rs`   | **新增** ~200 行 | 结果融合：role 修正 + 内容补充 + 4 测试             |
| `src/config.rs`          | 修改             | ParseMode 枚举 + 8 个 VLM 配置项                    |
| `src/ir/types.rs`        | 修改             | PageDiagnostics 新增 parse_strategy 字段            |
| `src/lib.rs`             | 修改             | 注册 hybrid 模块                                    |
| `src/pipeline/mod.rs`    | 修改             | 集成策略选择 + 记录到诊断信息                       |
