//! 实体提取与关系发现模块（GraphRAG）
//!
//! 从文档文本中提取实体（人物、组织、技术、概念）和关系（共现），
//! 用于构建知识图谱，增强 RAG 搜索的关联推理能力。

use std::collections::{HashMap, HashSet};

/// 实体类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityType {
    Person,
    Organization,
    Technology,
    Concept,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Person => "Person",
            EntityType::Organization => "Organization",
            EntityType::Technology => "Technology",
            EntityType::Concept => "Concept",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Person" => EntityType::Person,
            "Organization" => EntityType::Organization,
            "Technology" => EntityType::Technology,
            _ => EntityType::Concept,
        }
    }
}

/// 提取出的实体
#[derive(Debug, Clone)]
pub struct EntityRecord {
    /// 唯一标识（name 的小写形式）
    pub entity_id: String,
    /// 显示名称（保留原始大小写）
    pub name: String,
    /// 实体类型
    pub entity_type: EntityType,
    /// 来源文件路径
    pub source_file: String,
    /// 来源 VectorRecord 的 chunk ID
    pub chunk_id: String,
}

/// 实体之间的关系
#[derive(Debug, Clone)]
pub struct RelationRecord {
    /// 起始实体 ID
    pub from_entity: String,
    /// 目标实体 ID
    pub to_entity: String,
    /// 关系类型
    pub relation_type: String,
    /// 来源文件路径
    pub source_file: String,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
}

/// 图数据（用于前端可视化）
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// 图节点
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub entity_type: String,
    pub weight: f32,
}

/// 图边
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub weight: f32,
}

/// 基于规则的实体提取器
///
/// 提取策略：
/// 1. 英文专有名词：连续大写开头的词（如 "GPT-4", "OpenAI", "Machine Learning"）
/// 2. 中文引号术语：「」或""内的术语
/// 3. 英文引号术语：单/双引号内的短术语
/// 4. 技术术语模式：连续大写缩写（如 "RLHF", "CNN", "LLM"）
pub fn extract_entities_rule_based(
    text: &str,
    source_file: &str,
    chunk_id: &str,
) -> Vec<EntityRecord> {
    // 边界情况：空文本或纯空白
    let text = text.trim();
    if text.is_empty() || text.len() < 3 {
        return Vec::new();
    }

    // 边界情况：超长文本截断（避免性能问题，UTF-8 安全）
    let text = truncate_utf8_safe(text, 10000);

    let mut entities: HashMap<String, EntityRecord> = HashMap::new();

    // 策略 1: 英文大写开头的专有名词（2+ 个字符，包含连字符和数字）
    // 匹配: GPT-4, OpenAI, Machine Learning, BERT
    let words: Vec<&str> = text.split_whitespace().collect();
    for word in &words {
        let cleaned = word.trim_matches(|c: char| c.is_ascii_punctuation() && c != '-');
        if cleaned.len() >= 2 && is_proper_noun(cleaned) {
            add_entity(
                &mut entities,
                cleaned,
                EntityType::Technology,
                source_file,
                chunk_id,
            );
        }
    }

    // 策略 2: 全大写缩写词（3+ 个字母，如 RLHF, CNN, API）
    for word in &words {
        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
        if cleaned.len() >= 3
            && cleaned
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && cleaned.chars().any(|c| c.is_ascii_uppercase())
        {
            add_entity(
                &mut entities,
                cleaned,
                EntityType::Technology,
                source_file,
                chunk_id,
            );
        }
    }

    // 策略 3: 中文引号内的术语（「」和 ""）
    extract_quoted_terms(text, '「', '」', &mut entities, source_file, chunk_id);
    extract_quoted_terms(
        text,
        '\u{201C}',
        '\u{201D}',
        &mut entities,
        source_file,
        chunk_id,
    );

    // 策略 4: 英文双引号内的短术语（<= 5 个词）
    extract_short_quoted(text, '"', '"', 5, &mut entities, source_file, chunk_id);

    entities.into_values().collect()
}

/// 从同一段落中出现的实体对提取共现关系
pub fn extract_cooccurrence_relations(
    entities: &[EntityRecord],
    source_file: &str,
) -> Vec<RelationRecord> {
    let mut relations = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    // 对同一 chunk 内的实体两两配对
    let mut by_chunk: HashMap<&str, Vec<&EntityRecord>> = HashMap::new();
    for e in entities {
        by_chunk.entry(e.chunk_id.as_str()).or_default().push(e);
    }

    for (_chunk_id, chunk_entities) in &by_chunk {
        for i in 0..chunk_entities.len() {
            for j in (i + 1)..chunk_entities.len() {
                let a = &chunk_entities[i];
                let b = &chunk_entities[j];
                if a.entity_id == b.entity_id {
                    continue;
                }

                // 确保一致的顺序（避免 (A,B) 和 (B,A) 重复）
                let (from, to) = if a.entity_id < b.entity_id {
                    (&a.entity_id, &b.entity_id)
                } else {
                    (&b.entity_id, &a.entity_id)
                };

                let key = (from.clone(), to.clone());
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);

                relations.push(RelationRecord {
                    from_entity: from.clone(),
                    to_entity: to.clone(),
                    relation_type: "co-occurrence".to_string(),
                    source_file: source_file.to_string(),
                    confidence: 0.5,
                });
            }
        }
    }

    relations
}

/// 从 VectorRecord 列表中批量提取实体和关系
///
/// 返回 (entities, relations)，调用方可直接写入 EntityGraph。
/// 会跳过 doc-summary 类型的记录。
pub fn extract_from_records(
    records: &[crate::store::VectorRecord],
) -> (Vec<EntityRecord>, Vec<RelationRecord>) {
    let start = std::time::Instant::now();
    let mut all_entities = Vec::new();
    let mut chunk_count = 0;
    for record in records {
        // 跳过文档摘要记录
        if record.id.ends_with("-doc-summary") {
            continue;
        }
        chunk_count += 1;
        let entities = extract_entities_rule_based(&record.text, &record.file_path, &record.id);
        all_entities.extend(entities);
    }

    let source_file = records.first().map(|r| r.file_path.as_str()).unwrap_or("");
    let relations = extract_cooccurrence_relations(&all_entities, source_file);

    let elapsed = start.elapsed();
    if chunk_count > 0 && elapsed.as_millis() > 10 {
        println!(
            "[GraphRAG] Rule extraction: {} chunks, {} entities, {} relations in {:?}",
            chunk_count,
            all_entities.len(),
            relations.len(),
            elapsed
        );
    }

    (all_entities, relations)
}

// --- 关系类型 ---

/// 关系类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationType {
    /// 共现（同段落出现）
    CoOccurrence,
    /// 开发者/创建者（X 开发了 Y）
    DevelopedBy,
    /// 使用技术（X 使用了 Y）
    Uses,
    /// 属于分类（X 属于 Y）
    BelongsTo,
    /// 对比关系（X 与 Y 对比）
    ComparedWith,
    /// 因果关系（X 导致 Y）
    CausedBy,
    /// 时序关系（X 在 Y 之后）
    FollowedBy,
}

impl RelationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::CoOccurrence => "co-occurrence",
            RelationType::DevelopedBy => "developed-by",
            RelationType::Uses => "uses",
            RelationType::BelongsTo => "belongs-to",
            RelationType::ComparedWith => "compared-with",
            RelationType::CausedBy => "caused-by",
            RelationType::FollowedBy => "followed-by",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "developed-by" | "developed_by" | "created-by" => RelationType::DevelopedBy,
            "uses" | "used-by" | "use" => RelationType::Uses,
            "belongs-to" | "belongs_to" | "category" | "type-of" => RelationType::BelongsTo,
            "compared-with" | "compared_with" | "vs" | "compare" => RelationType::ComparedWith,
            "caused-by" | "caused_by" | "cause" | "leads-to" => RelationType::CausedBy,
            "followed-by" | "followed_by" | "after" | "then" => RelationType::FollowedBy,
            _ => RelationType::CoOccurrence,
        }
    }
}

// --- LLM 实体提取 ---

/// 生成 LLM 实体提取的 prompt
pub fn build_entity_extraction_prompt(text: &str) -> String {
    format!(
        r#"<|im_start|>system
You are an entity extraction assistant. Extract entities and relations from text.
Output ONLY valid JSON array, no other text.<|im_end|>
<|im_start|>user
Extract entities and their relationships from the following text.
Output a JSON object with "entities" and "relations" arrays.

Entity types: Person, Organization, Technology, Concept
Relation types: developed-by, uses, belongs-to, compared-with, caused-by, followed-by, co-occurrence

Example output:
{{"entities":[{{"name":"GPT-4","type":"Technology"}},{{"name":"OpenAI","type":"Organization"}}],"relations":[{{"from":"GPT-4","to":"OpenAI","type":"developed-by"}}]}}

Text:
{}

Output JSON only:<|im_end|>
<|im_start|>assistant
"#,
        // 截断过长的文本，避免超出 LLM 上下文（UTF-8 安全）
        truncate_utf8_safe(text, 1500)
    )
}

/// 解析 LLM 返回的 JSON 结果为实体和关系
pub fn parse_llm_entity_response(
    response: &str,
    source_file: &str,
    chunk_id: &str,
) -> Option<(Vec<EntityRecord>, Vec<RelationRecord>)> {
    // 尝试寻找 JSON 对象
    let json_str = extract_json_from_response(response)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let mut entities = Vec::new();
    let mut relations = Vec::new();

    // 解析实体
    if let Some(entity_arr) = parsed.get("entities").and_then(|v| v.as_array()) {
        for item in entity_arr {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let etype = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("Concept");
            if name.len() >= 2 {
                entities.push(EntityRecord {
                    entity_id: name.to_lowercase(),
                    name: name.to_string(),
                    entity_type: EntityType::from_str(etype),
                    source_file: source_file.to_string(),
                    chunk_id: chunk_id.to_string(),
                });
            }
        }
    }

    // 解析关系
    if let Some(rel_arr) = parsed.get("relations").and_then(|v| v.as_array()) {
        for item in rel_arr {
            let from = item.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let to = item.get("to").and_then(|v| v.as_str()).unwrap_or("");
            let rtype = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("co-occurrence");
            if !from.is_empty() && !to.is_empty() {
                relations.push(RelationRecord {
                    from_entity: from.to_lowercase(),
                    to_entity: to.to_lowercase(),
                    relation_type: RelationType::from_str(rtype).as_str().to_string(),
                    source_file: source_file.to_string(),
                    confidence: 0.8,
                });
            }
        }
    }

    if entities.is_empty() {
        None
    } else {
        Some((entities, relations))
    }
}

/// 从 LLM 响应中提取 JSON 内容
fn extract_json_from_response(response: &str) -> Option<String> {
    let trimmed = response.trim();

    // 直接是 JSON
    if trimmed.starts_with('{') {
        // 找到匹配的结束大括号
        let mut depth = 0;
        let mut end = 0;
        for (i, c) in trimmed.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end > 0 {
            return Some(trimmed[..end].to_string());
        }
    }

    // Markdown code block: ```json ... ```
    if let Some(start) = trimmed.find("```json") {
        let content_start = start + 7;
        if let Some(end) = trimmed[content_start..].find("```") {
            return Some(
                trimmed[content_start..content_start + end]
                    .trim()
                    .to_string(),
            );
        }
    }

    // 寻找第一个 { 到最后一个 }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            return Some(trimmed[start..=end].to_string());
        }
    }

    None
}

/// 混合提取：先尝试 LLM，失败则降级到规则提取
///
/// `llm_fn` 是一个异步函数，接受 prompt 并返回 LLM 的文本响应。
/// 如果 `llm_fn` 为 None 或者 LLM 调用失败，自动降级到规则提取。
///
/// 性能优化：
/// - 添加耗时统计日志
/// - 短文本（< 200 字符）直接跳过 LLM，用规则提取（节省 LLM 调用）
pub async fn extract_from_records_with_llm<F, Fut>(
    records: &[crate::store::VectorRecord],
    llm_fn: Option<F>,
) -> (Vec<EntityRecord>, Vec<RelationRecord>)
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Option<String>>,
{
    let total_start = std::time::Instant::now();

    let llm_fn = match llm_fn {
        Some(f) => f,
        None => return extract_from_records(records),
    };

    let mut all_entities = Vec::new();
    let mut all_relations = Vec::new();
    let mut llm_success = 0;
    let mut llm_fallback = 0;
    let mut llm_skipped = 0;

    for record in records {
        if record.id.ends_with("-doc-summary") {
            continue;
        }

        // 短文本直接用规则提取（LLM 调用开销不值得）
        if record.text.len() < 200 {
            llm_skipped += 1;
            let entities = extract_entities_rule_based(&record.text, &record.file_path, &record.id);
            let relations = extract_cooccurrence_relations(&entities, &record.file_path);
            all_entities.extend(entities);
            all_relations.extend(relations);
            continue;
        }

        // 尝试 LLM 提取
        let chunk_start = std::time::Instant::now();
        let prompt = build_entity_extraction_prompt(&record.text);
        let llm_result = (llm_fn)(prompt).await;

        if let Some(response) = llm_result {
            if let Some((entities, relations)) =
                parse_llm_entity_response(&response, &record.file_path, &record.id)
            {
                llm_success += 1;
                println!(
                    "[GraphRAG] LLM chunk {:?}: {} entities in {:?}",
                    record.id,
                    entities.len(),
                    chunk_start.elapsed()
                );
                all_entities.extend(entities);
                all_relations.extend(relations);
                continue;
            }
        }

        // 降级到规则提取
        llm_fallback += 1;
        let entities = extract_entities_rule_based(&record.text, &record.file_path, &record.id);
        let source_file = &record.file_path;
        let relations = extract_cooccurrence_relations(&entities, source_file);
        all_entities.extend(entities);
        all_relations.extend(relations);
    }

    println!(
        "[GraphRAG] Extraction complete: {} LLM, {} fallback, {} skipped (short), {} entities, {} relations in {:?}",
        llm_success, llm_fallback, llm_skipped,
        all_entities.len(), all_relations.len(),
        total_start.elapsed()
    );

    (all_entities, all_relations)
}

// --- 内部辅助函数 ---

/// UTF-8 安全的字符串截断
///
/// 在 max_bytes 以内的最近 char boundary 处截断，避免在中文等多字节字符中间截断导致 panic。
fn truncate_utf8_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // 从 max_bytes 往前找到最近的 char boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 判断一个词是否为英文专有名词
fn is_proper_noun(word: &str) -> bool {
    let first_char = word.chars().next().unwrap_or('a');
    if !first_char.is_ascii_uppercase() {
        return false;
    }

    // 排除常见的英文句首大写词
    let common_words: HashSet<&str> = [
        "The",
        "This",
        "That",
        "These",
        "Those",
        "It",
        "Its",
        "In",
        "On",
        "At",
        "To",
        "For",
        "Of",
        "With",
        "By",
        "From",
        "And",
        "Or",
        "But",
        "Not",
        "No",
        "If",
        "Then",
        "So",
        "We",
        "You",
        "He",
        "She",
        "They",
        "Our",
        "Your",
        "His",
        "Her",
        "Are",
        "Is",
        "Was",
        "Were",
        "Be",
        "Been",
        "Have",
        "Has",
        "Had",
        "Do",
        "Does",
        "Did",
        "Will",
        "Would",
        "Could",
        "Should",
        "May",
        "Might",
        "Can",
        "Shall",
        "Each",
        "Every",
        "Some",
        "Any",
        "All",
        "Most",
        "Many",
        "Much",
        "Few",
        "More",
        "Here",
        "There",
        "When",
        "Where",
        "How",
        "What",
        "Why",
        "Who",
        "After",
        "Before",
        "During",
        "Since",
        "Until",
        "While",
        "However",
        "Therefore",
        "Furthermore",
        "Moreover",
        "Although",
        "Because",
        "Since",
        "Unless",
        "Whether",
        "While",
    ]
    .iter()
    .copied()
    .collect();

    !common_words.contains(word)
}

/// 添加实体到 map（去重）
fn add_entity(
    entities: &mut HashMap<String, EntityRecord>,
    name: &str,
    entity_type: EntityType,
    source_file: &str,
    chunk_id: &str,
) {
    // 清理特殊字符（保留字母、数字、连字符、空格、中文字符）
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == ' ' || *c == '_' || *c > '\u{4E00}')
        .collect();
    let cleaned = cleaned.trim();

    // 过滤：太短（< 2 字符）或太长（> 100 字符）
    let char_count = cleaned.chars().count();
    if char_count < 2 || char_count > 100 {
        return;
    }

    // 过滤：纯数字（如 "010", "083", "100", "1270017878"）
    if cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == ' ')
    {
        return;
    }

    // 过滤：字母太少（至少需要 2 个字母或中文字符，排除 "10KB" 等边界情况）
    let alpha_count = cleaned
        .chars()
        .filter(|c| c.is_alphabetic() || *c > '\u{4E00}')
        .count();
    if alpha_count < 2 {
        return;
    }

    // 过滤：Markdown/HTML 噪音（如 "br2", "img src", "div class"）
    let lower = cleaned.to_lowercase();
    let html_noise = [
        "br",
        "img",
        "div",
        "span",
        "src",
        "href",
        "class",
        "style",
        "width",
        "height",
        "px",
        "em",
        "rem",
        "auto",
        "none",
        "true",
        "false",
        "null",
        "undefined",
        "return",
        "var",
        "let",
        "const",
        "function",
        "import",
        "export",
        "default",
        "module",
    ];
    if html_noise.contains(&lower.as_str()) {
        return;
    }

    // 过滤：名字含换行或过长空白（通常是解析噪音）
    if cleaned.contains('\n') || cleaned.matches(' ').count() > 8 {
        return;
    }

    let entity_id = cleaned.to_lowercase();
    entities
        .entry(entity_id.clone())
        .or_insert_with(|| EntityRecord {
            entity_id,
            name: cleaned.to_string(),
            entity_type,
            source_file: source_file.to_string(),
            chunk_id: chunk_id.to_string(),
        });
}

/// 提取指定引号内的术语
fn extract_quoted_terms(
    text: &str,
    open: char,
    close: char,
    entities: &mut HashMap<String, EntityRecord>,
    source_file: &str,
    chunk_id: &str,
) {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == open {
            let mut term = String::new();
            for inner in chars.by_ref() {
                if inner == close {
                    break;
                }
                term.push(inner);
            }
            let trimmed = term.trim();
            // 只取 2-20 个字符的术语
            let char_count = trimmed.chars().count();
            if char_count >= 2 && char_count <= 20 {
                add_entity(
                    entities,
                    trimmed,
                    EntityType::Concept,
                    source_file,
                    chunk_id,
                );
            }
        }
    }
}

/// 提取英文引号内的短术语
fn extract_short_quoted(
    text: &str,
    open: char,
    close: char,
    max_words: usize,
    entities: &mut HashMap<String, EntityRecord>,
    source_file: &str,
    chunk_id: &str,
) {
    let mut in_quote = false;
    let mut term = String::new();

    for c in text.chars() {
        if c == open && !in_quote {
            in_quote = true;
            term.clear();
        } else if c == close && in_quote {
            in_quote = false;
            let trimmed = term.trim();
            let word_count = trimmed.split_whitespace().count();
            let char_count = trimmed.chars().count();
            if word_count >= 1 && word_count <= max_words && char_count >= 2 {
                add_entity(
                    entities,
                    trimmed,
                    EntityType::Concept,
                    source_file,
                    chunk_id,
                );
            }
        } else if in_quote {
            term.push(c);
        }
    }
}

// --- SQLite 实体图存储 ---

use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};

/// 基于 SQLite 的实体图存储
#[derive(Clone)]
pub struct EntityGraph {
    pool: Pool<Sqlite>,
}

/// 图查询返回的关联实体
#[derive(Debug, Clone)]
pub struct RelatedEntity {
    pub entity_id: String,
    pub name: String,
    pub entity_type: String,
    pub relation_type: String,
    pub depth: i32,
}

impl EntityGraph {
    pub async fn new(db_path: &str) -> Result<Self> {
        let db_url = format!("sqlite://{}?mode=rwc", db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS entities (
                entity_id TEXT PRIMARY KEY, name TEXT NOT NULL,
                entity_type TEXT NOT NULL, source_file TEXT NOT NULL, chunk_id TEXT
            )"#,
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_entity_name ON entities(name)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_entity_file ON entities(source_file)")
            .execute(&pool)
            .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS relations (
                from_entity TEXT NOT NULL, to_entity TEXT NOT NULL,
                relation_type TEXT NOT NULL, source_file TEXT NOT NULL,
                confidence REAL DEFAULT 1.0,
                PRIMARY KEY (from_entity, to_entity, relation_type)
            )"#,
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rel_from ON relations(from_entity)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rel_to ON relations(to_entity)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rel_file ON relations(source_file)")
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }

    pub async fn add_entities(&self, entities: &[EntityRecord]) -> Result<()> {
        for entity in entities {
            sqlx::query(
                r#"INSERT INTO entities (entity_id, name, entity_type, source_file, chunk_id)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(entity_id) DO UPDATE SET
                    name = excluded.name, entity_type = excluded.entity_type,
                    source_file = excluded.source_file, chunk_id = excluded.chunk_id"#,
            )
            .bind(&entity.entity_id)
            .bind(&entity.name)
            .bind(entity.entity_type.as_str())
            .bind(&entity.source_file)
            .bind(&entity.chunk_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn add_relations(&self, relations: &[RelationRecord]) -> Result<()> {
        for rel in relations {
            sqlx::query(
                r#"INSERT INTO relations (from_entity, to_entity, relation_type, source_file, confidence)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(from_entity, to_entity, relation_type) DO UPDATE SET
                    source_file = excluded.source_file, confidence = excluded.confidence"#,
            )
            .bind(&rel.from_entity)
            .bind(&rel.to_entity)
            .bind(&rel.relation_type)
            .bind(&rel.source_file)
            .bind(rel.confidence)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn delete_by_file(&self, source_file: &str) -> Result<()> {
        sqlx::query("DELETE FROM relations WHERE source_file = ?")
            .bind(source_file)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM entities WHERE source_file = ?")
            .bind(source_file)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_related_entities(&self, entity_name: &str) -> Result<Vec<RelatedEntity>> {
        let entity_id = entity_name.to_lowercase();
        let rows = sqlx::query(
            r#"SELECT e.entity_id, e.name, e.entity_type, r.relation_type, 1 as depth,
            (SELECT COUNT(*) FROM relations r2 WHERE r2.from_entity = e.entity_id OR r2.to_entity = e.entity_id) as rel_count
            FROM relations r JOIN entities e ON r.to_entity = e.entity_id
            WHERE r.from_entity = ?
            UNION
            SELECT e.entity_id, e.name, e.entity_type, r.relation_type, 1 as depth,
            (SELECT COUNT(*) FROM relations r2 WHERE r2.from_entity = e.entity_id OR r2.to_entity = e.entity_id) as rel_count
            FROM relations r JOIN entities e ON r.from_entity = e.entity_id
            WHERE r.to_entity = ?
            ORDER BY rel_count DESC
            LIMIT 20"#,
        )
        .bind(&entity_id)
        .bind(&entity_id)
        .fetch_all(&self.pool)
        .await?;

        let results: Vec<RelatedEntity> = rows
            .into_iter()
            .map(|r| RelatedEntity {
                entity_id: r.get("entity_id"),
                name: r.get("name"),
                entity_type: r.get("entity_type"),
                relation_type: r.get("relation_type"),
                depth: r.get("depth"),
            })
            .filter(|r| {
                // 过滤纯数字和单字符噪音
                let has_alpha = r.name.chars().any(|c| c.is_alphabetic() || c > '\u{4E00}');
                has_alpha && r.name.len() >= 2
            })
            .take(10)
            .collect();

        Ok(results)
    }

    pub async fn get_entity_chunk_ids(&self, entity_name: &str) -> Result<Vec<String>> {
        let entity_id = entity_name.to_lowercase();
        let rows = sqlx::query("SELECT chunk_id FROM entities WHERE entity_id = ?")
            .bind(&entity_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.get::<Option<String>, _>("chunk_id"))
            .collect())
    }

    pub async fn entity_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM entities")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("cnt"))
    }

    pub async fn relation_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM relations")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("cnt"))
    }

    /// 获取所有关系类型及其数量（用于统计）
    pub async fn relation_type_stats(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT relation_type, COUNT(*) as cnt FROM relations GROUP BY relation_type ORDER BY cnt DESC"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<String, _>("relation_type"), r.get::<i64, _>("cnt")))
            .collect())
    }

    /// 获取出现最频繁的实体（Top N）
    pub async fn top_entities(&self, limit: i32) -> Result<Vec<(String, String, i64)>> {
        let rows = sqlx::query(
            r#"SELECT e.name, e.entity_type,
                (SELECT COUNT(*) FROM relations WHERE from_entity = e.entity_id OR to_entity = e.entity_id) as rel_count
            FROM entities e
            ORDER BY rel_count DESC
            LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("name"),
                    r.get::<String, _>("entity_type"),
                    r.get::<i64, _>("rel_count"),
                )
            })
            .collect())
    }

    /// 带 confidence 权重过滤的关系查询
    pub async fn get_related_entities_filtered(
        &self,
        entity_name: &str,
        min_confidence: f32,
        limit: i32,
    ) -> Result<Vec<RelatedEntity>> {
        let entity_id = entity_name.to_lowercase();
        let rows = sqlx::query(
            r#"SELECT e.entity_id, e.name, e.entity_type, r.relation_type, 1 as depth
            FROM relations r JOIN entities e ON r.to_entity = e.entity_id
            WHERE r.from_entity = ? AND r.confidence >= ?
            UNION
            SELECT e.entity_id, e.name, e.entity_type, r.relation_type, 1 as depth
            FROM relations r JOIN entities e ON r.from_entity = e.entity_id
            WHERE r.to_entity = ? AND r.confidence >= ?
            LIMIT ?"#,
        )
        .bind(&entity_id)
        .bind(min_confidence)
        .bind(&entity_id)
        .bind(min_confidence)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| RelatedEntity {
                entity_id: r.get("entity_id"),
                name: r.get("name"),
                entity_type: r.get("entity_type"),
                relation_type: r.get("relation_type"),
                depth: r.get("depth"),
            })
            .collect())
    }

    /// 获取图数据用于可视化（Top N 实体 + 它们之间的关系）
    pub async fn get_graph_data(&self, max_nodes: i32) -> Result<GraphData> {
        // 获取 top N 实体
        let entity_rows = sqlx::query(
            r#"SELECT DISTINCT e.entity_id, e.name, e.entity_type,
                (SELECT COUNT(*) FROM relations WHERE from_entity = e.entity_id OR to_entity = e.entity_id) as rel_count
            FROM entities e
            ORDER BY rel_count DESC
            LIMIT ?"#,
        )
        .bind(max_nodes)
        .fetch_all(&self.pool)
        .await?;

        let nodes: Vec<GraphNode> = entity_rows
            .iter()
            .map(|r| GraphNode {
                id: r.get::<String, _>("entity_id"),
                label: r.get::<String, _>("name"),
                entity_type: r.get::<String, _>("entity_type"),
                weight: r.get::<i64, _>("rel_count") as f32,
            })
            .collect();

        // 获取这些实体之间的关系
        let entity_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let mut edges = Vec::new();
        if !entity_ids.is_empty() {
            let placeholders = entity_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                "SELECT from_entity, to_entity, relation_type, confidence FROM relations WHERE from_entity IN ({}) AND to_entity IN ({})",
                placeholders, placeholders
            );
            let mut q = sqlx::query(&query);
            for id in &entity_ids {
                q = q.bind(id);
            }
            for id in &entity_ids {
                q = q.bind(id);
            }
            let edge_rows = q.fetch_all(&self.pool).await?;
            edges = edge_rows
                .into_iter()
                .map(|r| GraphEdge {
                    source: r.get("from_entity"),
                    target: r.get("to_entity"),
                    relation_type: r.get("relation_type"),
                    weight: r.get("confidence"),
                })
                .collect();
        }

        Ok(GraphData { nodes, edges })
    }
}

/// 在内存中对实体列表做去重与合并
///
/// 合并策略：
/// - 同 entity_id（lowercase）的实体合并为一个
/// - 保留第一次出现的 name（保留原始大小写）
/// - entity_type 优先选非 Concept 的类型
pub fn dedup_entities(entities: Vec<EntityRecord>) -> Vec<EntityRecord> {
    let mut map: HashMap<String, EntityRecord> = HashMap::new();
    for entity in entities {
        map.entry(entity.entity_id.clone())
            .and_modify(|existing| {
                // 如果 existing 是 Concept 但新的不是，升级类型
                if existing.entity_type == EntityType::Concept
                    && entity.entity_type != EntityType::Concept
                {
                    existing.entity_type = entity.entity_type.clone();
                }
            })
            .or_insert(entity);
    }
    map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_english_proper_nouns() {
        let text = "GPT-4 was developed by OpenAI using RLHF training method.";
        let entities = extract_entities_rule_based(text, "/test.md", "chunk-1");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"GPT-4"),
            "Should extract GPT-4, got: {:?}",
            names
        );
        assert!(
            names.contains(&"OpenAI"),
            "Should extract OpenAI, got: {:?}",
            names
        );
        assert!(
            names.contains(&"RLHF"),
            "Should extract RLHF, got: {:?}",
            names
        );
    }

    #[test]
    fn test_exclude_common_words() {
        let text = "The model is very good. However it needs more data.";
        let entities = extract_entities_rule_based(text, "/test.md", "chunk-1");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&"The"), "Should not extract 'The'");
        assert!(!names.contains(&"However"), "Should not extract 'However'");
    }

    #[test]
    fn test_extract_chinese_quoted_terms() {
        let text = "本文介绍了「支持向量机」和「深度学习」两种方法。";
        let entities = extract_entities_rule_based(text, "/test.md", "chunk-1");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"支持向量机"),
            "Should extract 支持向量机, got: {:?}",
            names
        );
        assert!(
            names.contains(&"深度学习"),
            "Should extract 深度学习, got: {:?}",
            names
        );
    }

    #[test]
    fn test_extract_chinese_double_quoted() {
        let text = "论文提出了\u{201C}注意力机制\u{201D}的概念。";
        let entities = extract_entities_rule_based(text, "/test.md", "chunk-1");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"注意力机制"),
            "Should extract 注意力机制, got: {:?}",
            names
        );
    }

    #[test]
    fn test_uppercase_acronyms() {
        let text = "CNN and LSTM are used in NLP tasks. The API provides REST endpoints.";
        let entities = extract_entities_rule_based(text, "/test.md", "chunk-1");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"CNN"),
            "Should extract CNN, got: {:?}",
            names
        );
        assert!(
            names.contains(&"LSTM"),
            "Should extract LSTM, got: {:?}",
            names
        );
        assert!(
            names.contains(&"NLP"),
            "Should extract NLP, got: {:?}",
            names
        );
        assert!(
            names.contains(&"API"),
            "Should extract API, got: {:?}",
            names
        );
    }

    #[test]
    fn test_entity_dedup() {
        let text = "OpenAI developed GPT-4. OpenAI also created ChatGPT.";
        let entities = extract_entities_rule_based(text, "/test.md", "chunk-1");
        let openai_count = entities.iter().filter(|e| e.name == "OpenAI").count();
        assert_eq!(openai_count, 1, "OpenAI should appear only once (deduped)");
    }

    #[test]
    fn test_cooccurrence_relations() {
        let entities = vec![
            EntityRecord {
                entity_id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                entity_type: EntityType::Technology,
                source_file: "/test.md".to_string(),
                chunk_id: "chunk-1".to_string(),
            },
            EntityRecord {
                entity_id: "openai".to_string(),
                name: "OpenAI".to_string(),
                entity_type: EntityType::Organization,
                source_file: "/test.md".to_string(),
                chunk_id: "chunk-1".to_string(),
            },
            EntityRecord {
                entity_id: "rlhf".to_string(),
                name: "RLHF".to_string(),
                entity_type: EntityType::Technology,
                source_file: "/test.md".to_string(),
                chunk_id: "chunk-1".to_string(),
            },
        ];

        let relations = extract_cooccurrence_relations(&entities, "/test.md");
        // 3 个实体在同一 chunk → C(3,2) = 3 条关系
        assert_eq!(relations.len(), 3, "Should have 3 co-occurrence relations");
        assert!(relations.iter().all(|r| r.relation_type == "co-occurrence"));
    }

    #[test]
    fn test_cooccurrence_different_chunks() {
        let entities = vec![
            EntityRecord {
                entity_id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                entity_type: EntityType::Technology,
                source_file: "/test.md".to_string(),
                chunk_id: "chunk-1".to_string(),
            },
            EntityRecord {
                entity_id: "bert".to_string(),
                name: "BERT".to_string(),
                entity_type: EntityType::Technology,
                source_file: "/test.md".to_string(),
                chunk_id: "chunk-2".to_string(), // 不同 chunk
            },
        ];

        let relations = extract_cooccurrence_relations(&entities, "/test.md");
        // 不同 chunk 的实体不产生共现关系
        assert_eq!(relations.len(), 0, "Different chunks should not co-occur");
    }

    #[test]
    fn test_entity_type() {
        assert_eq!(EntityType::from_str("Person").as_str(), "Person");
        assert_eq!(
            EntityType::from_str("Organization").as_str(),
            "Organization"
        );
        assert_eq!(EntityType::from_str("Technology").as_str(), "Technology");
        assert_eq!(EntityType::from_str("unknown").as_str(), "Concept");
    }

    // --- Iteration 2 Tests ---

    #[test]
    fn test_relation_type_roundtrip() {
        assert_eq!(
            RelationType::from_str("developed-by").as_str(),
            "developed-by"
        );
        assert_eq!(RelationType::from_str("uses").as_str(), "uses");
        assert_eq!(RelationType::from_str("belongs-to").as_str(), "belongs-to");
        assert_eq!(
            RelationType::from_str("compared-with").as_str(),
            "compared-with"
        );
        assert_eq!(RelationType::from_str("caused-by").as_str(), "caused-by");
        assert_eq!(
            RelationType::from_str("followed-by").as_str(),
            "followed-by"
        );
        assert_eq!(RelationType::from_str("unknown").as_str(), "co-occurrence");
    }

    #[test]
    fn test_relation_type_aliases() {
        assert_eq!(
            RelationType::from_str("created-by").as_str(),
            "developed-by"
        );
        assert_eq!(RelationType::from_str("used-by").as_str(), "uses");
        assert_eq!(RelationType::from_str("category").as_str(), "belongs-to");
        assert_eq!(RelationType::from_str("vs").as_str(), "compared-with");
        assert_eq!(RelationType::from_str("leads-to").as_str(), "caused-by");
    }

    #[test]
    fn test_parse_llm_response_valid() {
        let response = r#"{"entities":[{"name":"GPT-4","type":"Technology"},{"name":"OpenAI","type":"Organization"}],"relations":[{"from":"GPT-4","to":"OpenAI","type":"developed-by"}]}"#;
        let result = parse_llm_entity_response(response, "/test.md", "chunk-1");
        assert!(result.is_some());
        let (entities, relations) = result.unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].relation_type, "developed-by");
        assert_eq!(relations[0].confidence, 0.8);
    }

    #[test]
    fn test_parse_llm_response_with_markdown() {
        let response = "Here is the result:\n```json\n{\"entities\":[{\"name\":\"BERT\",\"type\":\"Technology\"}],\"relations\":[]}\n```";
        let result = parse_llm_entity_response(response, "/test.md", "chunk-1");
        assert!(result.is_some());
        let (entities, _) = result.unwrap();
        assert_eq!(entities[0].name, "BERT");
    }

    #[test]
    fn test_parse_llm_response_invalid() {
        let response = "Sorry, I cannot extract entities from this text.";
        let result = parse_llm_entity_response(response, "/test.md", "chunk-1");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_llm_response_empty_entities() {
        let response = r#"{"entities":[],"relations":[]}"#;
        let result = parse_llm_entity_response(response, "/test.md", "chunk-1");
        assert!(result.is_none(), "Empty entities should return None");
    }

    #[test]
    fn test_extract_json_from_response_noise() {
        let response = "Sure! Here is the extracted data: {\"entities\":[{\"name\":\"AI\",\"type\":\"Concept\"}],\"relations\":[]} Let me know if you need more.";
        let result = parse_llm_entity_response(response, "/test.md", "chunk-1");
        assert!(result.is_some());
        let (entities, _) = result.unwrap();
        assert_eq!(entities[0].name, "AI");
    }

    #[test]
    fn test_dedup_entities_basic() {
        let entities = vec![
            EntityRecord {
                entity_id: "openai".to_string(),
                name: "OpenAI".to_string(),
                entity_type: EntityType::Concept,
                source_file: "/a.md".to_string(),
                chunk_id: "1".to_string(),
            },
            EntityRecord {
                entity_id: "openai".to_string(),
                name: "openai".to_string(),
                entity_type: EntityType::Organization,
                source_file: "/b.md".to_string(),
                chunk_id: "2".to_string(),
            },
        ];
        let deduped = dedup_entities(entities);
        assert_eq!(deduped.len(), 1);
        // 应合并为一个，类型升级为 Organization
        assert_eq!(deduped[0].entity_type, EntityType::Organization);
    }

    #[test]
    fn test_dedup_entities_preserves_different() {
        let entities = vec![
            EntityRecord {
                entity_id: "openai".to_string(),
                name: "OpenAI".to_string(),
                entity_type: EntityType::Organization,
                source_file: "/a.md".to_string(),
                chunk_id: "1".to_string(),
            },
            EntityRecord {
                entity_id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                entity_type: EntityType::Technology,
                source_file: "/a.md".to_string(),
                chunk_id: "1".to_string(),
            },
        ];
        let deduped = dedup_entities(entities);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_build_entity_extraction_prompt() {
        let prompt = build_entity_extraction_prompt("GPT-4 was developed by OpenAI.");
        assert!(prompt.contains("GPT-4 was developed by OpenAI."));
        assert!(prompt.contains("entities"));
        assert!(prompt.contains("relations"));
        assert!(prompt.contains("<|im_start|>"));
    }

    #[test]
    fn test_build_prompt_truncates_long_text() {
        let long_text = "a".repeat(3000);
        let prompt = build_entity_extraction_prompt(&long_text);
        // 原文被截断到 1500 字节
        assert!(!prompt.contains(&"a".repeat(3000)));
    }

    // --- Iteration 3 Tests: 边界情况 ---

    #[test]
    fn test_empty_text() {
        let result = extract_entities_rule_based("", "/test.md", "chunk-1");
        assert!(result.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let result = extract_entities_rule_based("   \n\t  ", "/test.md", "chunk-1");
        assert!(result.is_empty());
    }

    #[test]
    fn test_very_short_text() {
        let result = extract_entities_rule_based("hi", "/test.md", "chunk-1");
        assert!(result.is_empty());
    }

    #[test]
    fn test_special_characters_in_entity() {
        // SQL 注入风险字符应被清理
        let text = "The company O'Reilly; DROP TABLE-- published a book.";
        let result = extract_entities_rule_based(text, "/test.md", "chunk-1");
        // 确保不会 panic
        for entity in &result {
            assert!(!entity.name.contains(';'));
            assert!(!entity.name.contains('\''));
        }
    }

    #[test]
    fn test_long_text_truncation() {
        // 超长文本不应 panic
        let long_text = "OpenAI ".repeat(5000);
        let result = extract_entities_rule_based(&long_text, "/test.md", "chunk-1");
        assert!(
            !result.is_empty(),
            "Should still extract from truncated text"
        );
    }

    #[test]
    fn test_entity_name_length_limit() {
        // 超长实体名应被过滤
        let long_name = "A".repeat(200);
        let text = format!("The {} project is important.", long_name);
        let result = extract_entities_rule_based(&text, "/test.md", "chunk-1");
        for entity in &result {
            assert!(entity.name.chars().count() <= 100);
        }
    }
}
