<script>
    /**
     * FileChat - 单文件 RAG 聊天组件
     * 基于指定文件内容回答问题
     * 支持 Excel 文件的 Text-to-SQL 查询
     */
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { marked } from "marked";

    let { file = null, onBack = () => {} } = $props();

    let query = $state("");
    // Phase: idle | searching | querying_sql | analyzing | generating | done | error
    let phase = $state("idle");
    let sources = $state([]);
    let answer = $state("");
    let showCursor = $state(false);
    let errorMsg = $state("");
    let inputRef = $state(null);

    // SQL 查询结果
    let sqlResult = $state(null);
    let sqlExpanded = $state(false);
    let sqlPhaseText = $state("");

    // 判断是否为 Excel 文件
    let isExcelFile = $derived(
        file?.name?.match(/\.(xlsx|xls|xlsm|xlsb)$/i) != null,
    );

    // Markdown 渲染
    let renderedAnswer = $derived(
        answer ? marked.parse(answer, { breaks: true }) : "",
    );

    async function handleSubmit() {
        if (
            !query.trim() ||
            !file ||
            phase === "searching" ||
            phase === "querying_sql" ||
            phase === "analyzing" ||
            phase === "generating"
        )
            return;

        const q = query.trim();
        answer = "";
        sources = [];
        errorMsg = "";
        sqlResult = null;
        sqlExpanded = false;
        showCursor = false;
        phase = "searching";

        try {
            // 1. 搜索（限定在单文件）
            const searchResponse = await invoke("rag_search", {
                query: q,
                filePath: file.path,
            });

            sources = searchResponse.sources || [];

            // 检查是否有 tabular 来源
            const hasTabular = sources.some((s) => s.source === "Tabular");

            if (
                !searchResponse.context ||
                searchResponse.context.trim() === ""
            ) {
                answer = isExcelFile
                    ? "在该表格中未找到与问题相关的数据。"
                    : "在该文件中未找到与问题相关的内容。";
                phase = "done";
                return;
            }

            // 2. Excel 文件 → 尝试 Text-to-SQL 查询
            let contextForGeneration = searchResponse.context;

            if (isExcelFile && hasTabular) {
                phase = "querying_sql";
                sqlPhaseText = "正在生成 SQL 查询...";

                try {
                    const sqlResponse = await invoke("query_excel_table", {
                        filePath: file.path,
                        query: q,
                    });

                    sqlResult = sqlResponse;
                    sqlPhaseText = "正在执行查询...";

                    // 将 SQL 结果作为 context 补充
                    let sqlContext = `\n[SQL 查询结果]\n执行的 SQL: ${sqlResponse.sql}\n`;
                    if (sqlResponse.summary_text) {
                        sqlContext += sqlResponse.summary_text;
                    } else {
                        // 小结果集，构建 Markdown 表格
                        sqlContext += `| ${sqlResponse.columns.join(" | ")} |\n`;
                        sqlContext += `| ${sqlResponse.columns.map(() => "---").join(" | ")} |\n`;
                        for (const row of sqlResponse.rows) {
                            sqlContext += `| ${row.join(" | ")} |\n`;
                        }
                    }
                    contextForGeneration =
                        sqlContext + "\n" + searchResponse.context;
                } catch (sqlErr) {
                    // SQL 失败不阻塞，fallback 到普通 RAG
                    console.warn(
                        "[FileChat] SQL query failed, falling back:",
                        sqlErr,
                    );
                }
            } else if (hasTabular) {
                phase = "analyzing";
                await new Promise((r) => setTimeout(r, 300));
            }

            // 3. 生成回答
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
                    context: contextForGeneration,
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
        sqlResult = null;
        sqlExpanded = false;
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
                {#if isExcelFile}
                    <span class="material-symbols-outlined empty-icon"
                        >table_chart</span
                    >
                    <h4>向这个表格提问</h4>
                    <p>AI 将基于表格数据回答你的问题，支持数据查询和计算</p>
                {:else}
                    <span class="material-symbols-outlined empty-icon"
                        >forum</span
                    >
                    <h4>向这个文件提问</h4>
                    <p>AI 将仅基于该文件的内容回答你的问题</p>
                {/if}
            </div>
        {/if}

        {#if phase === "searching"}
            <div class="status-bar">
                <span class="material-symbols-outlined spinning"
                    >progress_activity</span
                >
                <span
                    >{isExcelFile
                        ? "正在搜索表格数据..."
                        : "正在搜索文件内容..."}</span
                >
            </div>
        {/if}

        {#if phase === "querying_sql"}
            <div class="status-bar status-sql">
                <span class="material-symbols-outlined spinning">database</span>
                <span>{sqlPhaseText}</span>
            </div>
        {/if}

        {#if phase === "analyzing"}
            <div class="status-bar status-analyzing">
                <span class="material-symbols-outlined spinning"
                    >data_check</span
                >
                <span>正在分析表格数据...</span>
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

        <!-- SQL 查询结果展示 -->
        {#if sqlResult}
            <div class="sql-result-section">
                <div class="sql-result-header">
                    <span
                        class="material-symbols-outlined"
                        style="font-size:14px;color:#8b5cf6">database</span
                    >
                    <span class="sql-result-title">SQL 查询结果</span>
                    <span class="sql-result-meta"
                        >{sqlResult.row_count} 行{sqlResult.retried
                            ? " · 已重试"
                            : ""}</span
                    >
                </div>

                <!-- 可折叠 SQL 语句 -->
                <button
                    class="sql-toggle"
                    onclick={() => (sqlExpanded = !sqlExpanded)}
                >
                    <span
                        class="material-symbols-outlined"
                        style="font-size:12px"
                    >
                        {sqlExpanded ? "expand_less" : "expand_more"}
                    </span>
                    <code class="sql-preview"
                        >{sqlExpanded ? "收起 SQL" : "查看 SQL"}</code
                    >
                </button>
                {#if sqlExpanded}
                    <pre class="sql-code"><code>{sqlResult.sql}</code></pre>
                {/if}

                <!-- 数据表格 -->
                {#if sqlResult.is_summarized}
                    <div class="sql-summary-notice">
                        <span
                            class="material-symbols-outlined"
                            style="font-size:13px">info</span
                        >
                        数据量较大（{sqlResult.row_count} 行），已自动汇总展示
                    </div>
                {/if}

                {#if !sqlResult.is_summarized && sqlResult.rows.length > 0}
                    <div class="sql-table-wrapper">
                        <table class="sql-table">
                            <thead>
                                <tr>
                                    {#each sqlResult.columns as col}
                                        <th>{col}</th>
                                    {/each}
                                </tr>
                            </thead>
                            <tbody>
                                {#each sqlResult.rows as row}
                                    <tr>
                                        {#each row as cell}
                                            <td>{cell}</td>
                                        {/each}
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                {/if}
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
                    <div
                        class="source-card"
                        class:tabular-source={source.source === "Tabular"}
                    >
                        <div class="source-meta">
                            <span class="source-index">#{i + 1}</span>
                            {#if source.source === "Tabular"}
                                <span
                                    class="material-symbols-outlined source-type-icon"
                                    style="font-size:12px;color:#10b981"
                                    >table_chart</span
                                >
                            {/if}
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
                disabled={phase === "searching" ||
                    phase === "querying_sql" ||
                    phase === "analyzing" ||
                    phase === "generating"}
            />
            <button
                class="send-btn"
                onclick={handleSubmit}
                disabled={!query.trim() ||
                    phase === "searching" ||
                    phase === "querying_sql" ||
                    phase === "analyzing" ||
                    phase === "generating"}
            >
                <span class="material-symbols-outlined">
                    {phase === "searching" ||
                    phase === "querying_sql" ||
                    phase === "analyzing" ||
                    phase === "generating"
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

    .status-analyzing .material-symbols-outlined {
        color: #10b981;
    }

    .status-sql .material-symbols-outlined {
        color: #8b5cf6;
    }

    /* SQL 查询结果 */
    .sql-result-section {
        background: var(--bg-card);
        border: 1px solid var(--border-color);
        border-left: 3px solid #8b5cf6;
        border-radius: 8px;
        padding: 10px 12px;
    }

    .sql-result-header {
        display: flex;
        align-items: center;
        gap: 6px;
        margin-bottom: 6px;
    }

    .sql-result-title {
        font-size: 12px;
        font-weight: 600;
        color: var(--text-secondary);
    }

    .sql-result-meta {
        font-size: 10px;
        color: var(--text-muted);
        margin-left: auto;
    }

    .sql-toggle {
        display: flex;
        align-items: center;
        gap: 4px;
        background: none;
        border: none;
        color: var(--text-muted);
        cursor: pointer;
        padding: 2px 0;
        font-family: inherit;
        font-size: 11px;
        transition: color 0.15s ease;
    }

    .sql-toggle:hover {
        color: #8b5cf6;
    }

    .sql-preview {
        font-size: 11px;
        background: none;
        padding: 0;
    }

    .sql-code {
        background: rgba(139, 92, 246, 0.06);
        border: 1px solid rgba(139, 92, 246, 0.15);
        border-radius: 6px;
        padding: 8px 10px;
        margin: 6px 0;
        overflow-x: auto;
        font-size: 11px;
        line-height: 1.5;
        color: var(--text-primary);
    }

    .sql-code code {
        font-family: "SF Mono", "Fira Code", monospace;
        white-space: pre-wrap;
        word-break: break-all;
    }

    .sql-summary-notice {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 6px 8px;
        margin-top: 6px;
        background: rgba(139, 92, 246, 0.06);
        border-radius: 4px;
        font-size: 11px;
        color: var(--text-muted);
    }

    .sql-table-wrapper {
        margin-top: 8px;
        overflow-x: auto;
        border-radius: 6px;
        border: 1px solid var(--border-color);
    }

    .sql-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 11px;
    }

    .sql-table th,
    .sql-table td {
        padding: 5px 8px;
        border: 1px solid var(--border-color);
        text-align: left;
        white-space: nowrap;
    }

    .sql-table th {
        background: rgba(139, 92, 246, 0.08);
        font-weight: 600;
        color: var(--text-secondary);
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.3px;
    }

    .sql-table tr:nth-child(even) {
        background: var(--bg-card-hover, rgba(255, 255, 255, 0.02));
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

    .tabular-source {
        border-left: 2px solid #10b981;
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

    .answer-content :global(table) {
        width: 100%;
        border-collapse: collapse;
        margin: 8px 0;
        font-size: 12px;
    }

    .answer-content :global(th),
    .answer-content :global(td) {
        padding: 6px 10px;
        border: 1px solid var(--border-color);
        text-align: left;
    }

    .answer-content :global(th) {
        background: var(--bg-card-hover, rgba(255, 255, 255, 0.05));
        font-weight: 600;
        white-space: nowrap;
    }

    .answer-content :global(tr:nth-child(even)) {
        background: var(--bg-card-hover, rgba(255, 255, 255, 0.02));
    }

    .answer-content :global(strong) {
        color: var(--text-primary);
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
