# Milestone 3: 体验优化与解析重构

**Goal**: 
重构 PDF 解析逻辑以生成基于语义（H1/H2）的逻辑树，并优化模型下载机制以支持并发下载，提升用户体验和解析质量。

**Success Metrics**:
- PDF 解析结果呈现为 logical structure (Heading-based) 而非 physical structure (Page-based)。
- 模型下载支持多文件并发，总体下载耗时减少。
- 用户界面正确反馈多文件下载进度。

**Iterations**:
1. [Iteration 1: Backend Core Logic](iteration1.md) - 核心算法与下载器并发支持。
2. [Iteration 2: UI Integration & Refinement](iteration2.md) - 前端适配与完整流程验证。
