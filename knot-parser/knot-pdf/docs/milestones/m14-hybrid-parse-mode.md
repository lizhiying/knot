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

- [ ] 统一 Backend 抽象框架重构
- [ ] Fast Track + Model 两层解析的混合策略
- [ ] VLM 外部调用接口（可选，调用外部 LLM API）
- [ ] 页面级别的解析策略自动选择
- [ ] 评测对比

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

- [ ] 在 Config 中新增 `parse_mode: ParseMode`（默认 `Auto`）
- [ ] `Auto` 模式的决策逻辑：
  - PageScore 高 → FastTrack
  - PageScore 低 + 有模型 → Enhanced
  - PageScore 极低（纯扫描件）+ 有 VLM API → Full

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

- [ ] 模型编排器 `ModelOrchestrator`：按顺序调用各模型
- [ ] 每个模型独立可开关
- [ ] 模型失败时回退到规则结果（不中断）
- [ ] 耗时统计到 `Timings`

### 3. VLM 外部调用接口（Full 模式）

对于 Fast Track 无法处理的页面（扫描件、复杂图表等），提供调用外部 VLM API 的能力：

```rust
/// VLM 后端 trait
pub trait VlmBackend: Send + Sync {
    /// 发送页面图片，获取结构化解析结果
    fn parse_page_image(
        &self,
        image_data: &[u8],
        prompt: &str,
    ) -> Result<VlmParseResult, PdfError>;
}

pub struct VlmParseResult {
    pub markdown: String,
    pub blocks: Vec<BlockIR>,
    pub tables: Vec<TableIR>,
    pub confidence: f32,
}
```

- [ ] `VlmBackend` trait 定义
- [ ] `HttpVlmBackend`：通过 HTTP API 调用外部 VLM 服务
  - 支持 OpenAI 兼容格式（GPT-4o / Claude / InternVL 等）
  - 配置: `vlm_api_url`, `vlm_api_key`, `vlm_model`
- [ ] `MockVlmBackend`：测试用
- [ ] VLM 结果解析（将 Markdown/JSON 回填到 IR）
- [ ] Feature gate: `vlm`
- [ ] 超时 + 重试 + 限流

### 4. 页面级策略自动选择

```rust
fn select_parse_strategy(page: &PageIR, config: &Config) -> ParseStrategy {
    match config.parse_mode {
        ParseMode::FastTrack => ParseStrategy::FastTrackOnly,
        ParseMode::Enhanced => ParseStrategy::FastTrackPlusModels,
        ParseMode::Full => {
            if page.text_score > 0.7 {
                ParseStrategy::FastTrackPlusModels  // 文字质量好，不需要 VLM
            } else {
                ParseStrategy::FullWithVlm           // 文字质量差，调用 VLM
            }
        }
        ParseMode::Auto => auto_select(page, config),
    }
}
```

- [ ] 策略选择函数
- [ ] 每页的策略记录到 `PageDiagnostics`
- [ ] 统计：文档中有多少页走了各策略

### 5. 结果融合

当 Fast Track 和 VLM 都有结果时的融合策略：

- [ ] **文本优先级**：Fast Track 提取的文本通常更准确（无幻觉），VLM 结果作为补充
- [ ] **结构优先级**：VLM 判断的标题/列表/表格结构通常更准确，用于修正 block roles
- [ ] **冲突解决**：出现冲突时，根据 PageScore 决定信任哪一方
- [ ] 融合结果写入同一 `PageIR`，标记来源

### 6. 配置项

```rust
/// 解析模式
pub parse_mode: ParseMode,  // 默认 Auto

/// VLM 配置 (Full 模式)
pub vlm_enabled: bool,           // 默认 false
pub vlm_api_url: Option<String>, // VLM API 地址
pub vlm_api_key: Option<String>, // API Key
pub vlm_model: Option<String>,   // 模型名称
pub vlm_timeout_secs: u32,       // 超时秒数（默认 30）
pub vlm_max_retries: u32,        // 最大重试次数（默认 2）
pub vlm_score_threshold: f32,    // PageScore 低于此值时触发 VLM（默认 0.3）
```

### 7. 测试

- [ ] 策略选择逻辑的单元测试
- [ ] 模型编排器的集成测试（使用 Mock）
- [ ] VLM 后端的集成测试（使用 Mock）
- [ ] 结果融合逻辑测试
- [ ] 端到端评测：
  - [ ] FastTrack 模式 vs Enhanced 模式 vs Full 模式的质量对比
  - [ ] 各模式的性能对比

---

## 完成标准

- [ ] 三种 ParseMode 均可工作（FastTrack / Enhanced / Full）
- [ ] Auto 模式正确选择策略
- [ ] VLM 后端可通过 HTTP 调用外部服务
- [ ] 不破坏现有 FastTrack 模式的功能和性能
- [ ] Enhanced 模式在有模型时质量提升可测量
- [ ] Full 模式在扫描件上输出可搜索文本
- [ ] 全部测试通过
