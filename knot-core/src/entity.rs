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

// --- 内部辅助函数 ---

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
    let entity_id = name.to_lowercase();
    // 过滤太短的（单个字母或数字）
    if entity_id.chars().count() < 2 {
        return;
    }
    entities
        .entry(entity_id.clone())
        .or_insert_with(|| EntityRecord {
            entity_id,
            name: name.to_string(),
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
            r#"SELECT e.entity_id, e.name, e.entity_type, r.relation_type, 1 as depth
            FROM relations r JOIN entities e ON r.to_entity = e.entity_id
            WHERE r.from_entity = ?
            UNION
            SELECT e.entity_id, e.name, e.entity_type, r.relation_type, 1 as depth
            FROM relations r JOIN entities e ON r.from_entity = e.entity_id
            WHERE r.to_entity = ?"#,
        )
        .bind(&entity_id)
        .bind(&entity_id)
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
}
