<script>
    /**
     * FileChat - 单文件 RAG 聊天组件
     * 基于指定文件内容回答问题
     */
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { marked } from "marked";

    let { file = null, onBack = () => {} } = $props();

    let query = $state("");
    let phase = $state("idle"); // idle | searching | generating | done | error
    let sources = $state([]);
    let answer = $state("");
    let showCursor = $state(false);
    let errorMsg = $state("");
    let inputRef = $state(null);

    // Markdown 渲染
    let renderedAnswer = $derived(
        answer ? marked.parse(answer, { breaks: true }) : "",
    );

    async function handleSubmit() {
        if (
            !query.trim() ||
            !file ||
            phase === "searching" ||
            phase === "generating"
        )
            return;

        const q = query.trim();
        answer = "";
        sources = [];
        errorMsg = "";
        phase = "searching";
        showCursor = false;

        try {
            // 1. 搜索（限定在单文件）
            const searchResponse = await invoke("rag_search", {
                query: q,
                filePath: file.path,
            });

            sources = searchResponse.sources || [];

            if (
                !searchResponse.context ||
                searchResponse.context.trim() === ""
            ) {
                answer = "在该文件中未找到与问题相关的内容。";
                phase = "done";
                return;
            }

            // 2. 生成回答
            phase = "generating";

            let isFirstToken = true;
            const unlisten = await listen("llm-token", (event) => {
                if (isFirstToken) {
                    showCursor = true;
                    isFirstToken = false;
                }
                answer += event.payload;
            });

            try {
                await invoke("rag_generate", {
                    query: q,
                    context: searchResponse.context,
                });
            } finally {
                unlisten();
            }

            showCursor = false;
            phase = "done";
        } catch (err) {
            errorMsg = err.toString();
            phase = "error";
            showCursor = false;
        }
    }

    function handleKeydown(e) {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            handleSubmit();
        }
    }

    function resetChat() {
        query = "";
        answer = "";
        sources = [];
        phase = "idle";
        errorMsg = "";
    }
</script>

<div class="chat-container">
    <!-- 顶部标题 -->
    <div class="chat-header">
        <button class="back-btn" onclick={onBack}>
            <span class="material-symbols-outlined">arrow_back</span>
        </button>
        <div class="chat-title">
            <span class="material-symbols-outlined title-icon">chat</span>
            <span class="title-text">与 <strong>{file?.name}</strong> 对话</span
            >
        </div>
    </div>

    <!-- 主内容区 -->
    <div class="chat-body">
        {#if phase === "idle" && !answer}
            <!-- 空状态引导 -->
            <div class="chat-empty">
                <span class="material-symbols-outlined empty-icon">forum</span>
                <h4>向这个文件提问</h4>
                <p>AI 将仅基于该文件的内容回答你的问题</p>
            </div>
        {/if}

        {#if phase === "searching"}
            <div class="status-bar">
                <span class="material-symbols-outlined spinning"
                    >progress_activity</span
                >
                <span>正在搜索文件内容...</span>
            </div>
        {/if}

        {#if phase === "generating" && !answer}
            <div class="status-bar">
                <span class="material-symbols-outlined spinning"
                    >progress_activity</span
                >
                <span>正在生成回答...</span>
            </div>
        {/if}

        <!-- 回答区域 -->
        {#if answer}
            <div class="answer-section">
                <div class="answer-header">
                    <span class="material-symbols-outlined">smart_toy</span>
                    AI 回答
                </div>
                <div class="answer-content" class:streaming={showCursor}>
                    {@html renderedAnswer}
                    {#if showCursor}
                        <span class="cursor-blink">▊</span>
                    {/if}
                </div>
            </div>
        {/if}

        <!-- 引用来源 -->
        {#if sources.length > 0 && (phase === "done" || phase === "generating")}
            <div class="sources-section">
                <div class="sources-header">
                    <span class="material-symbols-outlined">format_quote</span>
                    引用来源 ({sources.length})
                </div>
                {#each sources as source, i}
                    <div class="source-card">
                        <div class="source-meta">
                            <span class="source-index">#{i + 1}</span>
                            {#if source.context}
                                <span class="source-context"
                                    >{source.context}</span
                                >
                            {/if}
                            <span class="source-score"
                                >{source.score.toFixed(0)}%</span
                            >
                        </div>
                        <p class="source-text">
                            {source.text.slice(0, 200)}{source.text.length > 200
                                ? "..."
                                : ""}
                        </p>
                    </div>
                {/each}
            </div>
        {/if}

        <!-- 错误 -->
        {#if phase === "error"}
            <div class="error-section">
                <span class="material-symbols-outlined">error</span>
                <p>{errorMsg}</p>
                <button class="retry-btn" onclick={resetChat}>重试</button>
            </div>
        {/if}
    </div>

    <!-- 输入区域 -->
    <div class="chat-input-area">
        <div class="input-wrapper">
            <input
                bind:this={inputRef}
                type="text"
                placeholder="输入你的问题..."
                bind:value={query}
                onkeydown={handleKeydown}
                disabled={phase === "searching" || phase === "generating"}
            />
            <button
                class="send-btn"
                onclick={handleSubmit}
                disabled={!query.trim() ||
                    phase === "searching" ||
                    phase === "generating"}
            >
                <span class="material-symbols-outlined">
                    {phase === "searching" || phase === "generating"
                        ? "progress_activity"
                        : "send"}
                </span>
            </button>
        </div>
        {#if phase === "done"}
            <button class="new-question-btn" onclick={resetChat}>
                <span class="material-symbols-outlined">add</span>
                新问题
            </button>
        {/if}
    </div>
</div>

<style>
    .chat-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        overflow: hidden;
    }

    .chat-header {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 12px 16px;
        border-bottom: 1px solid var(--border-color);
        flex-shrink: 0;
    }

    .back-btn {
        background: none;
        border: none;
        color: var(--text-muted);
        cursor: pointer;
        padding: 4px;
        border-radius: 4px;
        display: flex;
    }

    .back-btn:hover {
        color: var(--text-primary);
        background: var(--bg-card-hover);
    }

    .back-btn .material-symbols-outlined {
        font-size: 18px;
    }

    .chat-title {
        display: flex;
        align-items: center;
        gap: 6px;
        min-width: 0;
    }

    .title-icon {
        font-size: 16px;
        color: var(--accent-primary);
    }

    .title-text {
        font-size: 12px;
        color: var(--text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .title-text strong {
        color: var(--text-primary);
    }

    .chat-body {
        flex: 1;
        overflow-y: auto;
        padding: 16px;
        display: flex;
        flex-direction: column;
        gap: 12px;
    }

    .chat-empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        flex: 1;
        text-align: center;
        gap: 8px;
        color: var(--text-muted);
    }

    .empty-icon {
        font-size: 32px;
        opacity: 0.4;
    }

    .chat-empty h4 {
        font-size: 14px;
        font-weight: 600;
        color: var(--text-secondary);
        margin: 0;
    }

    .chat-empty p {
        font-size: 12px;
        margin: 0;
    }

    .status-bar {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 10px 12px;
        background: var(--bg-card);
        border-radius: 8px;
        font-size: 12px;
        color: var(--text-muted);
    }

    .status-bar .material-symbols-outlined {
        font-size: 16px;
        color: var(--accent-primary);
    }

    .spinning {
        animation: spin 1s linear infinite;
    }

    @keyframes spin {
        from {
            transform: rotate(0deg);
        }
        to {
            transform: rotate(360deg);
        }
    }

    /* 回答 */
    .answer-section {
        border-radius: 10px;
        overflow: hidden;
    }

    .answer-header {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
        font-weight: 600;
        color: var(--text-muted);
        padding: 0 0 8px 0;
    }

    .answer-header .material-symbols-outlined {
        font-size: 14px;
        color: var(--accent-primary);
    }

    .answer-content {
        font-size: 13px;
        line-height: 1.7;
        color: var(--text-primary);
        padding: 12px 14px;
        background: var(--bg-card);
        border-radius: 8px;
        border: 1px solid var(--border-color);
    }

    .answer-content :global(p) {
        margin: 0 0 8px 0;
    }

    .answer-content :global(p:last-child) {
        margin: 0;
    }

    .answer-content :global(code) {
        background: var(--code-bg);
        padding: 1px 4px;
        border-radius: 3px;
        font-size: 12px;
    }

    .answer-content :global(pre) {
        background: var(--code-bg);
        padding: 10px 12px;
        border-radius: 6px;
        overflow-x: auto;
        margin: 8px 0;
    }

    .answer-content :global(ul),
    .answer-content :global(ol) {
        padding-left: 20px;
        margin: 4px 0;
    }

    .cursor-blink {
        animation: blink 0.8s step-end infinite;
        color: var(--accent-primary);
        font-size: 12px;
    }

    @keyframes blink {
        0%,
        100% {
            opacity: 1;
        }
        50% {
            opacity: 0;
        }
    }

    /* 引用来源 */
    .sources-section {
        margin-top: 4px;
    }

    .sources-header {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
        font-weight: 600;
        color: var(--text-muted);
        margin-bottom: 6px;
    }

    .sources-header .material-symbols-outlined {
        font-size: 14px;
    }

    .source-card {
        padding: 8px 10px;
        background: var(--bg-card);
        border: 1px solid var(--border-color);
        border-radius: 6px;
        margin-bottom: 4px;
    }

    .source-meta {
        display: flex;
        align-items: center;
        gap: 6px;
        margin-bottom: 4px;
    }

    .source-index {
        font-size: 10px;
        font-weight: 700;
        color: var(--accent-primary);
    }

    .source-context {
        font-size: 10px;
        color: var(--text-muted);
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .source-score {
        font-size: 10px;
        color: var(--text-muted);
        flex-shrink: 0;
    }

    .source-text {
        font-size: 11px;
        color: var(--text-muted);
        line-height: 1.5;
        margin: 0;
    }

    /* 错误 */
    .error-section {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 8px;
        padding: 16px;
        background: rgba(239, 68, 68, 0.06);
        border-radius: 8px;
    }

    .error-section .material-symbols-outlined {
        font-size: 24px;
        color: #ef4444;
    }

    .error-section p {
        font-size: 12px;
        color: var(--text-muted);
        text-align: center;
        margin: 0;
    }

    .retry-btn {
        padding: 6px 16px;
        background: var(--bg-card);
        border: 1px solid var(--border-color);
        border-radius: 6px;
        color: var(--text-secondary);
        font-size: 12px;
        cursor: pointer;
        font-family: inherit;
    }

    .retry-btn:hover {
        background: var(--bg-card-hover);
    }

    /* 输入区 */
    .chat-input-area {
        padding: 12px 16px;
        border-top: 1px solid var(--border-color);
        flex-shrink: 0;
        display: flex;
        gap: 8px;
        align-items: center;
    }

    .input-wrapper {
        flex: 1;
        display: flex;
        align-items: center;
        gap: 6px;
        background: var(--bg-input);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 0 8px 0 12px;
        transition: border-color 0.15s ease;
    }

    .input-wrapper:focus-within {
        border-color: var(--accent-primary);
    }

    .input-wrapper input {
        flex: 1;
        background: none;
        border: none;
        outline: none;
        color: var(--text-primary);
        font-size: 13px;
        padding: 9px 0;
        font-family: inherit;
    }

    .input-wrapper input::placeholder {
        color: var(--text-muted);
    }

    .input-wrapper input:disabled {
        opacity: 0.5;
    }

    .send-btn {
        background: none;
        border: none;
        color: var(--accent-primary);
        cursor: pointer;
        padding: 4px;
        border-radius: 4px;
        display: flex;
        transition: all 0.15s ease;
    }

    .send-btn:hover:not(:disabled) {
        background: var(--bg-card-hover);
    }

    .send-btn:disabled {
        opacity: 0.3;
        cursor: not-allowed;
    }

    .send-btn .material-symbols-outlined {
        font-size: 18px;
    }

    .new-question-btn {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 6px 12px;
        background: var(--bg-card);
        border: 1px solid var(--border-color);
        border-radius: 6px;
        color: var(--text-muted);
        font-size: 11px;
        cursor: pointer;
        transition: all 0.15s ease;
        font-family: inherit;
        flex-shrink: 0;
    }

    .new-question-btn:hover {
        background: var(--bg-card-hover);
        color: var(--text-primary);
    }

    .new-question-btn .material-symbols-outlined {
        font-size: 14px;
    }
</style>
