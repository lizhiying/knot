<script>
    /**
     * FileDetail - 文件详情面板
     * 显示已索引文件的 chunk 列表、元数据、操作按钮
     */
    import { invoke } from "@tauri-apps/api/core";

    let {
        file = null,
        onClose = () => {},
        onReindex = () => {},
        onIgnore = () => {},
        onChat = () => {},
    } = $props();

    let detail = $state(null);
    let isLoading = $state(false);
    let isReindexing = $state(false);
    let showIgnoreConfirm = $state(false);
    let error = $state(null);
    let expandedChunks = $state(new Set());

    // 当 file 变更时加载详情
    $effect(() => {
        // 显式读取 file.path 确保文件切换时 effect 重新触发
        const currentPath = file?.path;
        const currentStatus = file?.index_status;

        if (
            currentPath &&
            (currentStatus === "Indexed" || currentStatus === "Outdated")
        ) {
            // 重置 UI 状态
            expandedChunks = new Set();
            showIgnoreConfirm = false;
            error = null;
            loadDetail(currentPath);
        } else {
            detail = null;
        }
    });

    async function loadDetail(filePath) {
        if (!filePath) return;
        isLoading = true;
        error = null;
        detail = null;
        try {
            detail = await invoke("get_file_index_detail", {
                filePath,
            });
        } catch (err) {
            error = err.toString();
        } finally {
            isLoading = false;
        }
    }

    async function handleReindex() {
        if (!file || isReindexing) return;
        isReindexing = true;
        try {
            await invoke("reindex_file", { filePath: file.path });
            await loadDetail(file.path);
            onReindex();
        } catch (err) {
            error = err.toString();
        } finally {
            isReindexing = false;
        }
    }

    async function handleIgnore() {
        if (!file) return;
        try {
            await invoke("ignore_file", { filePath: file.path });
            showIgnoreConfirm = false;
            onIgnore();
        } catch (err) {
            error = err.toString();
        }
    }

    function toggleChunk(id) {
        const next = new Set(expandedChunks);
        if (next.has(id)) {
            next.delete(id);
        } else {
            next.add(id);
        }
        expandedChunks = next;
    }

    function formatTime(timestamp) {
        if (!timestamp) return "未知";
        const d = new Date(timestamp * 1000);
        return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    }

    // 文件类型图标
    function getFileIcon(fileType) {
        const icons = {
            Markdown: "description",
            Text: "description",
            Pdf: "picture_as_pdf",
            Html: "code",
            Word: "article",
            PowerPoint: "slideshow",
            Excel: "table_chart",
            Csv: "table_chart",
            Image: "image",
            Other: "draft",
        };
        return icons[fileType] || "draft";
    }

    function getStatusInfo(status) {
        const info = {
            Indexed: { label: "已索引", color: "#28c840" },
            Outdated: { label: "待更新", color: "#febc2e" },
            Unindexed: { label: "未索引", color: "rgba(255,255,255,0.4)" },
            Unsupported: { label: "不支持", color: "rgba(255,255,255,0.2)" },
            Ignored: { label: "已忽略", color: "rgba(255,255,255,0.25)" },
        };
        return info[status] || info.Unindexed;
    }

    function formatSize(bytes) {
        if (bytes === 0) return "0 B";
        const units = ["B", "KB", "MB", "GB"];
        const i = Math.floor(Math.log(bytes) / Math.log(1024));
        return (
            (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + " " + units[i]
        );
    }
</script>

{#if file}
    {@const statusInfo = getStatusInfo(file.index_status)}
    <div class="detail-panel">
        <!-- 标题栏 -->
        <div class="detail-header">
            <div class="detail-title">
                <span class="material-symbols-outlined file-icon">
                    {getFileIcon(file.file_type)}
                </span>
                <div class="title-text">
                    <h3>{file.name}</h3>
                    <span class="detail-path">{file.relative_path}</span>
                </div>
            </div>
            <button class="close-btn" onclick={onClose}>
                <span class="material-symbols-outlined">close</span>
            </button>
        </div>

        <!-- 状态 + 元数据 -->
        <div class="meta-section">
            <div class="meta-row">
                <span class="meta-label">状态</span>
                <span class="meta-value" style="color: {statusInfo.color}"
                    >{statusInfo.label}</span
                >
            </div>
            <div class="meta-row">
                <span class="meta-label">大小</span>
                <span class="meta-value">{formatSize(file.size)}</span>
            </div>
            {#if detail}
                <div class="meta-row">
                    <span class="meta-label">Chunks</span>
                    <span class="meta-value">{detail.chunk_count}</span>
                </div>
                {#if detail.indexed_at}
                    <div class="meta-row">
                        <span class="meta-label">索引时间</span>
                        <span class="meta-value"
                            >{formatTime(detail.indexed_at)}</span
                        >
                    </div>
                {/if}
                {#if detail.content_hash}
                    <div class="meta-row">
                        <span class="meta-label">Hash</span>
                        <span class="meta-value hash"
                            >{detail.content_hash.slice(0, 16)}...</span
                        >
                    </div>
                {/if}
            {/if}
        </div>

        {#if file.index_status === "Unsupported"}
            <div class="unsupported-hint">
                <span class="material-symbols-outlined">info</span>
                <p>该文件类型暂不支持内容索引，将在后续版本中支持。</p>
            </div>
        {:else if file.index_status === "Ignored"}
            <div class="unsupported-hint">
                <span class="material-symbols-outlined">block</span>
                <p>该文件已被排除，不会被索引。</p>
            </div>
        {:else}
            <!-- 操作按钮 -->
            <div class="action-bar">
                <button
                    class="action-btn chat"
                    onclick={() => onChat(file)}
                    disabled={file.index_status !== "Indexed" &&
                        file.index_status !== "Outdated"}
                >
                    <span class="material-symbols-outlined">chat</span>
                    聊天
                </button>
                <button
                    class="action-btn reindex"
                    onclick={handleReindex}
                    disabled={isReindexing}
                >
                    <span
                        class="material-symbols-outlined"
                        class:spinning={isReindexing}
                    >
                        {isReindexing ? "progress_activity" : "refresh"}
                    </span>
                    {isReindexing ? "索引中..." : "重新索引"}
                </button>
                <button
                    class="action-btn ignore"
                    onclick={() => (showIgnoreConfirm = true)}
                >
                    <span class="material-symbols-outlined">block</span>
                    排除
                </button>
            </div>

            <!-- 排除确认 -->
            {#if showIgnoreConfirm}
                <div class="confirm-bar">
                    <p>确定排除此文件？索引数据将被删除。</p>
                    <div class="confirm-actions">
                        <button
                            class="confirm-btn cancel"
                            onclick={() => (showIgnoreConfirm = false)}
                            >取消</button
                        >
                        <button
                            class="confirm-btn danger"
                            onclick={handleIgnore}>确定排除</button
                        >
                    </div>
                </div>
            {/if}

            <!-- Chunk 列表 -->
            {#if isLoading}
                <div class="chunks-loading">
                    {#each Array(4) as _, i}
                        <div
                            class="skeleton-chunk"
                            style="animation-delay: {i * 0.1}s"
                        >
                            <div class="skeleton-breadcrumb"></div>
                            <div class="skeleton-text"></div>
                        </div>
                    {/each}
                </div>
            {:else if detail && detail.chunks.length > 0}
                <div class="chunks-header">
                    <span class="material-symbols-outlined">segment</span>
                    Chunks ({detail.chunk_count})
                </div>
                <div class="chunks-list">
                    {#each detail.chunks as chunk (chunk.id)}
                        <button
                            class="chunk-item"
                            class:expanded={expandedChunks.has(chunk.id)}
                            onclick={() => toggleChunk(chunk.id)}
                        >
                            {#if chunk.breadcrumbs}
                                <span class="chunk-breadcrumbs"
                                    >{chunk.breadcrumbs}</span
                                >
                            {/if}
                            <p
                                class="chunk-preview"
                                class:expanded={expandedChunks.has(chunk.id)}
                            >
                                {chunk.preview}
                            </p>
                        </button>
                    {/each}
                </div>
            {:else if detail && detail.chunks.length === 0}
                <div class="no-chunks">
                    <span class="material-symbols-outlined">info</span>
                    <p>无 chunk 数据</p>
                </div>
            {/if}
        {/if}

        {#if error}
            <div class="error-msg">
                <span class="material-symbols-outlined">error</span>
                {error}
            </div>
        {/if}
    </div>
{/if}

<style>
    .detail-panel {
        display: flex;
        flex-direction: column;
        height: 100%;
        overflow-y: auto;
        padding: 16px;
        border-left: 1px solid var(--border-color);
        background: var(--bg-secondary);
    }

    .detail-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        margin-bottom: 16px;
    }

    .detail-title {
        display: flex;
        gap: 10px;
        align-items: flex-start;
        min-width: 0;
        flex: 1;
    }

    .file-icon {
        font-size: 22px;
        color: var(--accent-primary);
        flex-shrink: 0;
        margin-top: 1px;
    }

    .title-text {
        min-width: 0;
    }

    .title-text h3 {
        font-size: 14px;
        font-weight: 600;
        color: var(--text-primary);
        margin: 0;
        word-break: break-all;
    }

    .detail-path {
        font-size: 11px;
        color: var(--text-muted);
        word-break: break-all;
    }

    .close-btn {
        background: none;
        border: none;
        color: var(--text-muted);
        cursor: pointer;
        padding: 4px;
        border-radius: 4px;
        flex-shrink: 0;
    }

    .close-btn:hover {
        color: var(--text-primary);
        background: var(--bg-card-hover);
    }

    .close-btn .material-symbols-outlined {
        font-size: 18px;
    }

    /* 元数据 */
    .meta-section {
        display: flex;
        flex-direction: column;
        gap: 6px;
        padding: 12px;
        background: var(--bg-card);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        margin-bottom: 12px;
    }

    .meta-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
    }

    .meta-label {
        font-size: 11px;
        color: var(--text-muted);
    }

    .meta-value {
        font-size: 12px;
        color: var(--text-secondary);
        font-weight: 500;
    }

    .meta-value.hash {
        font-family: "Fira Code", monospace;
        font-size: 10px;
    }

    /* 操作按钮 */
    .action-bar {
        display: flex;
        gap: 8px;
        margin-bottom: 12px;
    }

    .action-btn {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 8px 12px;
        border: 1px solid var(--border-color);
        border-radius: 8px;
        background: var(--bg-card);
        color: var(--text-secondary);
        font-size: 12px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        font-family: inherit;
    }

    .action-btn:hover:not(:disabled) {
        background: var(--bg-card-hover);
        color: var(--text-primary);
    }

    .action-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .action-btn .material-symbols-outlined {
        font-size: 16px;
    }

    .action-btn.reindex:hover:not(:disabled) {
        border-color: var(--accent-primary);
    }

    .action-btn.chat:hover:not(:disabled) {
        border-color: var(--accent-primary);
        color: var(--accent-primary);
    }

    .action-btn.ignore:hover {
        border-color: #ef4444;
        color: #ef4444;
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

    /* 确认栏 */
    .confirm-bar {
        padding: 10px 12px;
        background: rgba(239, 68, 68, 0.08);
        border: 1px solid rgba(239, 68, 68, 0.2);
        border-radius: 8px;
        margin-bottom: 12px;
    }

    .confirm-bar p {
        font-size: 12px;
        color: var(--text-secondary);
        margin: 0 0 8px 0;
    }

    .confirm-actions {
        display: flex;
        gap: 8px;
    }

    .confirm-btn {
        flex: 1;
        padding: 6px 12px;
        border-radius: 6px;
        border: 1px solid var(--border-color);
        font-size: 11px;
        font-weight: 500;
        cursor: pointer;
        font-family: inherit;
    }

    .confirm-btn.cancel {
        background: rgba(255, 255, 255, 0.04);
        color: var(--text-secondary);
    }

    .confirm-btn.cancel:hover {
        background: rgba(255, 255, 255, 0.08);
    }

    .confirm-btn.danger {
        background: rgba(239, 68, 68, 0.15);
        color: #ef4444;
        border-color: rgba(239, 68, 68, 0.3);
    }

    .confirm-btn.danger:hover {
        background: rgba(239, 68, 68, 0.25);
    }

    /* Chunks */
    .chunks-header {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 12px;
        font-weight: 600;
        color: var(--text-secondary);
        margin-bottom: 8px;
    }

    .chunks-header .material-symbols-outlined {
        font-size: 16px;
        color: var(--text-muted);
    }

    .chunks-list {
        display: flex;
        flex-direction: column;
        gap: 4px;
    }

    .chunk-item {
        padding: 8px 10px;
        background: var(--bg-card);
        border: 1px solid var(--border-color);
        border-radius: 6px;
        cursor: pointer;
        text-align: left;
        transition: all 0.15s ease;
        width: 100%;
        font-family: inherit;
        color: var(--text-primary);
    }

    .chunk-item:hover {
        background: var(--bg-card-hover);
        border-color: var(--border-light);
    }

    .chunk-breadcrumbs {
        display: block;
        font-size: 10px;
        color: var(--accent-primary);
        margin-bottom: 4px;
        opacity: 0.8;
    }

    .chunk-preview {
        font-size: 11px;
        color: var(--text-muted);
        line-height: 1.5;
        margin: 0;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }

    .chunk-preview.expanded {
        -webkit-line-clamp: unset;
        overflow: visible;
    }

    .unsupported-hint,
    .no-chunks {
        display: flex;
        align-items: flex-start;
        gap: 8px;
        padding: 12px;
        background: var(--bg-card);
        border-radius: 8px;
        margin-top: 8px;
    }

    .unsupported-hint .material-symbols-outlined,
    .no-chunks .material-symbols-outlined {
        font-size: 18px;
        color: var(--text-muted);
        flex-shrink: 0;
    }

    .unsupported-hint p,
    .no-chunks p {
        font-size: 12px;
        color: var(--text-muted);
        margin: 0;
        line-height: 1.5;
    }

    .error-msg {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 8px 12px;
        background: rgba(239, 68, 68, 0.08);
        border-radius: 6px;
        font-size: 11px;
        color: #ef4444;
        margin-top: 8px;
    }

    .error-msg .material-symbols-outlined {
        font-size: 16px;
    }

    /* 骨架屏 */
    .chunks-loading {
        display: flex;
        flex-direction: column;
        gap: 8px;
    }

    .skeleton-chunk {
        padding: 10px;
        border-radius: 6px;
        animation: skeleton-fade 1.2s ease-in-out infinite;
    }

    .skeleton-breadcrumb {
        height: 10px;
        width: 40%;
        background: var(--skeleton-start);
        border-radius: 3px;
        margin-bottom: 6px;
    }

    .skeleton-text {
        height: 10px;
        width: 80%;
        background: var(--skeleton-start);
        border-radius: 3px;
    }

    @keyframes skeleton-fade {
        0%,
        100% {
            opacity: 0.4;
        }
        50% {
            opacity: 0.8;
        }
    }
</style>
