//! HTTP API for RAG Evaluation
//!
//! 在 Tauri 应用启动时运行一个轻量 HTTP 服务，供 Python 评测脚本调用。

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

use knot_core::llm::LlamaClient;
use knot_core::store::KnotStore;
use pageindex_rs::EmbeddingProvider;

/// 评测 API 共享状态
#[derive(Clone)]
pub struct EvalApiState {
    pub thread_safe_embedding: Arc<RwLock<Option<Arc<dyn EmbeddingProvider + Send + Sync>>>>,
    pub chat_client: Arc<RwLock<Option<Arc<LlamaClient>>>>,
    pub index_path: Arc<RwLock<Option<String>>>,
}

/// RAG 查询请求
#[derive(Deserialize)]
pub struct RagQueryRequest {
    pub query: String,
}

/// RAG 引用来源
#[derive(Serialize)]
pub struct RagCitation {
    pub doc_path: String,
    pub heading: Option<String>,
    pub quote: String,
    pub score: f32,
}

/// RAG 查询响应
#[derive(Serialize)]
pub struct RagQueryResponse {
    pub answer: String,
    pub citations: Vec<RagCitation>,
    pub refused: bool,
}

/// 健康检查
async fn health_check() -> &'static str {
    "OK"
}

/// RAG 查询接口
async fn rag_query(
    State(state): State<EvalApiState>,
    Json(req): Json<RagQueryRequest>,
) -> Result<Json<RagQueryResponse>, (StatusCode, String)> {
    use pageindex_rs::LlmProvider;

    let query = req.query;
    println!("[EvalAPI] RAG query: {}", query);

    // 1. 获取 index_path
    let index_path = {
        let guard = state.index_path.read().await;
        guard.clone()
    }
    .ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Index path not set".to_string(),
    ))?;

    // 2. 打开 Store
    let store = KnotStore::new(&index_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Store error: {}", e),
        )
    })?;

    // 3. 生成查询 Embedding
    let embedding_provider = {
        let guard = state.thread_safe_embedding.read().await;
        guard.clone()
    }
    .ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Embedding engine not ready".to_string(),
    ))?;

    let query_vec = embedding_provider
        .generate_embedding(&query)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 4. 搜索 (使用默认阈值)
    let distance_threshold = 1.5; // TODO: 从配置获取
    let search_results = store
        .search(query_vec, &query, distance_threshold)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    println!("[EvalAPI] Found {} results", search_results.len());

    // 5. 构建上下文
    let mut context_str = String::new();
    let mut citations = Vec::new();

    for (i, res) in search_results.iter().take(5).enumerate() {
        let heading = res.breadcrumbs.clone().unwrap_or_default();
        context_str.push_str(&format!(
            "[{}] (匹配度: {:.0}%) 文件: {} - 章节: {}\n内容: {}\n\n",
            i + 1,
            res.score,
            res.file_path,
            heading,
            res.text
        ));

        citations.push(RagCitation {
            doc_path: res.file_path.clone(),
            heading: res.breadcrumbs.clone(),
            quote: res.text.clone(),
            score: res.score,
        });
    }

    // 6. 调用 LLM
    let llm_client = {
        let guard = state.chat_client.read().await;
        guard.clone()
    }
    .ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Chat LLM not ready".to_string(),
    ))?;

    let prompt = format!(
        r#"<|im_start|>system
你是一个智能助手。请根据参考文档回答用户问题。

**回答原则**：
1. **开门见山**：直接把文档中找到的关键信息放在第一句。
2. **去除客套**：不要使用"根据参考文档..."等前缀。
3. **详细展开**：在核心答案之后，引用文档细节进行说明。
4. 只有当文档完全不包含相关信息时，才说"无法找到答案"。
<|im_end|>
<|im_start|>user
参考文档：
{}

用户问题: {}<|im_end|>
<|im_start|>assistant
"#,
        context_str, query
    );

    let answer = llm_client
        .generate_content(&prompt)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 检测是否拒答
    let refused =
        answer.contains("无法找到") || answer.contains("未找到") || answer.contains("没有相关信息");

    Ok(Json(RagQueryResponse {
        answer,
        citations,
        refused,
    }))
}

/// LLM 评判请求
#[derive(Deserialize)]
pub struct LlmJudgeRequest {
    pub question: String,
    pub answer_gold: String,
    pub answer_pred: String,
}

/// LLM 评判响应
#[derive(Serialize)]
pub struct LlmJudgeResponse {
    pub score: f32,        // 0.0 - 1.0
    pub correct: bool,     // 是否判定为正确
    pub reasoning: String, // 评判理由
}

/// LLM 评判接口
async fn llm_judge(
    State(state): State<EvalApiState>,
    Json(req): Json<LlmJudgeRequest>,
) -> Result<Json<LlmJudgeResponse>, (StatusCode, String)> {
    use pageindex_rs::LlmProvider;

    println!("[EvalAPI] LLM judge: {}", req.question);

    let llm_client = {
        let guard = state.chat_client.read().await;
        guard.clone()
    }
    .ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Chat LLM not ready".to_string(),
    ))?;

    let prompt = format!(
        r#"<|im_start|>system
你是一个答案评判专家。请评估"模型回答"是否正确回答了用户问题。

**评判标准**：
1. 核心信息正确：模型回答是否包含标准答案中的关键信息
2. 语义一致：即使措辞不同，语义是否一致
3. 无明显错误：回答中没有与标准答案矛盾的内容
4. 拒答处理：如果模型表示"无法找到答案"而标准答案有内容，则判定为错误

**输出格式**（必须严格遵守）：
SCORE: [0-100的整数分数]
CORRECT: [YES/NO]
REASON: [一句话说明评判理由]
<|im_end|>
<|im_start|>user
**用户问题**：{}

**标准答案**：{}

**模型回答**：{}
<|im_end|>
<|im_start|>assistant
"#,
        req.question, req.answer_gold, req.answer_pred
    );

    let response = llm_client
        .generate_content(&prompt)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 解析 LLM 输出
    let mut score: f32 = 0.0;
    let mut correct = false;
    let mut reasoning = String::new();

    for line in response.lines() {
        let line = line.trim();
        if line.starts_with("SCORE:") {
            if let Ok(s) = line.replace("SCORE:", "").trim().parse::<f32>() {
                score = (s / 100.0).clamp(0.0, 1.0);
            }
        } else if line.starts_with("CORRECT:") {
            let val = line.replace("CORRECT:", "").trim().to_uppercase();
            correct = val == "YES" || val == "TRUE";
        } else if line.starts_with("REASON:") {
            reasoning = line.replace("REASON:", "").trim().to_string();
        }
    }

    // 如果解析失败，使用简单启发式
    if reasoning.is_empty() {
        reasoning = response.lines().next().unwrap_or("").to_string();
    }

    Ok(Json(LlmJudgeResponse {
        score,
        correct,
        reasoning,
    }))
}

/// 创建评测 API 路由
pub fn create_eval_router(state: EvalApiState) -> Router {
    // 添加 CORS 支持，允许来自任意来源的跨域请求
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_check))
        .route("/rag/query", post(rag_query))
        .route("/llm/judge", post(llm_judge))
        .layer(cors)
        .with_state(state)
}

/// 启动评测 HTTP 服务器
pub async fn start_eval_server(state: EvalApiState, port: u16) {
    let app = create_eval_router(state);

    let addr = format!("127.0.0.1:{}", port);
    println!("[EvalAPI] Starting HTTP server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
