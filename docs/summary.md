# rag-skill-test vs Knot 检索实现对比分析

## 一、rag-skill-test 项目实现方法

### 1. 核心架构：AI Skill 驱动的指令式检索

`rag-skill-test` 本质上是一个 **AI Agent Skill**，它本身**不包含任何运行时代码**，而是一套写给 AI Agent（如 Claude）的指令文档。其核心实现方式是：

| 组件           | 实现方式                                             |
| -------------- | ---------------------------------------------------- |
| 索引系统       | 人工编写的 `data_structure.md` 分层索引文件          |
| 检索引擎       | AI Agent 的工具调用（grep、Read、pdftotext、pandas） |
| 向量搜索       | ❌ 无                                                 |
| Embedding 模型 | ❌ 无                                                 |
| 分词/NLP       | ❌ 无，依赖 AI Agent 的自然语言理解                   |
| 存储层         | 原始文件系统目录                                     |

### 2. 检索流程

```
用户提问 → AI Agent 理解意图
    → 读取 data_structure.md（分层索引导航）
    → 根据相关度选择子目录/文件
    → 遇到 PDF/Excel → 先读取 references 学习处理方法
    → 使用 grep/pdftotext/pandas 提取内容
    → 局部读取匹配上下文（避免全文加载）
    → 最多 5 轮迭代检索
    → 组织答案并溯源
```

### 3. 核心设计理念

- **分层索引导航**：每个目录用 `data_structure.md` 做人工索引
- **先学习再处理**：强制 AI 先读 references 文档学习工具用法
- **渐进式检索**：先 grep 定位关键词，再局部读取上下文
- **多轮迭代**：最多 5 次迭代逐步缩小范围

---

## 二、Knot 项目实现方法

### 1. 核心架构：本地混合检索引擎

Knot 是一个完整的 **本地桌面应用**（Tauri + Rust），包含自研的索引、搜索、向量等全套子系统：

| 组件           | 实现方式                                                    |
| -------------- | ----------------------------------------------------------- |
| 索引系统       | `pageindex-rs`（自研 Rust 库，自动解析文档结构树）          |
| 检索引擎       | **Tantivy**（Rust 全文搜索引擎，类 Lucene）                 |
| 向量搜索       | **LanceDB**（嵌入式向量数据库）                             |
| Embedding 模型 | **BGE-small-zh-v1.5**（本地 ONNX 推理）                     |
| 分词           | **Jieba 中文** + **en_knot 英文(Stemmer)** + **ICU 泛语言** |
| 存储层         | LanceDB + Tantivy 索引 + SQLite（FileRegistry 增量追踪）    |
| LLM            | **llama-server**（本地 sidecar 进程）                       |
| 融合排序       | **RRF（Reciprocal Rank Fusion）** 混合排序                  |

### 2. 检索流程

```
文档 → pageindex-rs 解析为 PageNode 树
    → PathProcessor 提取文件名/目录标签
    → EmbeddingEngine(BGE) 生成向量（注入路径上下文）
    → VectorRecord 写入 LanceDB + Tantivy（多字段冗余存储）

用户查询 → preprocess_query（边界空格 + 噪音去重）
    → 并行执行:
        1. LanceDB 向量搜索（Top-20, 距离阈值过滤）
        2. Tantivy BM25 关键词搜索（多字段加权）
    → RRF 融合排序（Vector 0.6 + Keyword 0.4）
    → 返回 Top-10 结果
```

---

## 三、关键差异对比

| 维度         | rag-skill-test               | Knot                                            |
| ------------ | ---------------------------- | ----------------------------------------------- |
| 运行方式     | AI Agent 运行时解释执行      | 本地编译运行的 Rust 应用                        |
| 索引构建     | 人工编写 `data_structure.md` | 自动化（walkdir + pageindex-rs 结构化解析）     |
| 搜索速度     | 5-15 秒（多轮工具调用）      | **<100ms**（缓存索引，毫秒级 response）         |
| 向量语义搜索 | ❌ 无                         | ✅ BGE 向量 + L2 距离                            |
| 关键词搜索   | grep（逐文件）               | Tantivy BM25（6 字段加权）                      |
| 多语言支持   | 依赖 AI 理解能力             | Jieba + Stemmer + ICU 三分词器                  |
| 增量更新     | 不需要（每次现场读取）       | blake3 哈希 + FileRegistry 增量索引             |
| 文件格式     | MD/PDF/Excel（现场处理）     | MD/TXT（已支持），PDF/DOCX（pageindex-rs 支持） |
| 结果排序     | AI 主观判断                  | RRF 数学融合（客观排序）                        |
| Token 成本   | 每次查询 2K-8K token         | 0 token（本地计算）                             |
| 部署依赖     | 需要 AI Agent 环境           | 独立应用，无需网络                              |
| 可扩展性     | 200MB，30+ 文件              | 理论支持百万级文档                              |
| 准确率       | ~85%（官方数据）             | 取决于 embedding + BM25 质量                    |

---

## 四、各自的优缺点

### rag-skill-test 的优势

1. **零基础设施**：不需要任何索引构建、模型部署，直接用 AI Agent 的能力
2. **灵活性极高**：AI 能理解语义、推理、总结，不受固定算法限制
3. **多格式无缝处理**：PDF/Excel 通过学习 references 现场处理，不需要预处理管线
4. **低门槛**：只需写好 Skill 文档和 data_structure.md 即可使用

### rag-skill-test 的劣势

1. **速度慢**：每次查询都是多轮工具调用（5-15秒），无法实现实时搜索
2. **成本高**：每次查询消耗 2K-8K token
3. **不可扩展**：200MB 数据就是上限，无法处理大规模知识库
4. **不可复现**：AI 的检索路径每次可能不同
5. **无离线能力**：依赖 AI Agent 环境

### Knot 的优势

1. **极速**：毫秒级搜索，适合实时交互式搜索场景
2. **混合检索**：向量语义搜索 + BM25 关键词搜索，互为补充
3. **完全本地**：无需网络，无 token 成本，数据隐私安全
4. **可扩展**：理论支持百万级文档
5. **增量索引**：只处理变更文件，高效

### Knot 的劣势

1. **格式受限**：目前主要支持 MD/TXT，PDF/Excel 的全格式解析还在完善中
2. **需要预处理**：文档必须先索引才能搜索
3. **缺乏推理能力**：纯检索，无法像 AI 一样理解、推理和总结

---

## 五、启发与借鉴

从 rag-skill-test 中，Knot 可以借鉴以下思路：

1. **🔹 分层索引概念**：rag-skill-test 的 `data_structure.md` 是一个很好的"目录级摘要"概念。Knot 可以在向量索引中加入**目录级别的上下文摘要**，提升跨文件检索的准确性。

2. **🔹 多格式处理管线**：rag-skill-test 有清晰的 PDF/Excel 处理指南。Knot 的 `pageindex-rs` 已经有 PDF 和 DOCX 的解析器，可以进一步完善 Excel 等格式的支持。

3. **🔹 迭代检索机制**：rag-skill-test 的多轮迭代检索思路可以启发 Knot 做 **query expansion**（查询扩展）——当首次搜索结果不理想时，自动扩展同义词或相关词进行二次搜索。

4. **🔹 RAG 融合**：当 Knot 的 LLM sidecar 成熟后，可以把 Knot 的检索结果 + LLM 总结能力结合，实现真正的 **本地 RAG 系统**，这将同时拥有两个项目的优势。