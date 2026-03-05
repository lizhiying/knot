<script>
    /**
     * FileList - 文件列表子组件
     * 显示文件图标、文件名、相对路径、大小、修改时间和索引状态
     */
    let { files = [], selectedFile = null, onSelect = () => {} } = $props();

    // 文件类型 → Material Symbols 图标映射
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

    // 文件类型 → 图标颜色
    function getFileIconColor(fileType) {
        const colors = {
            Markdown: "#8b5cf6",
            Text: "#8b5cf6",
            Pdf: "#ef4444",
            Html: "#f97316",
            Word: "#3b82f6",
            PowerPoint: "#f97316",
            Excel: "#10b981",
            Csv: "#10b981",
            Image: "#ec4899",
            Other: "#6b7280",
        };
        return colors[fileType] || "#6b7280";
    }

    // 索引状态 → 展示信息
    function getStatusInfo(status) {
        const info = {
            Indexed: {
                label: "已索引",
                color: "#28c840",
                bg: "rgba(40, 200, 64, 0.12)",
            },
            Outdated: {
                label: "待更新",
                color: "#febc2e",
                bg: "rgba(254, 188, 46, 0.12)",
            },
            Unindexed: {
                label: "未索引",
                color: "rgba(255,255,255,0.4)",
                bg: "rgba(255,255,255,0.06)",
            },
            Indexing: {
                label: "索引中",
                color: "#4a9eff",
                bg: "rgba(74, 158, 255, 0.12)",
            },
            Ignored: {
                label: "已忽略",
                color: "rgba(255,255,255,0.25)",
                bg: "rgba(255,255,255,0.04)",
            },
            Unsupported: {
                label: "不支持",
                color: "rgba(255,255,255,0.2)",
                bg: "rgba(255,255,255,0.03)",
            },
        };
        return info[status] || info.Unindexed;
    }

    // 人类可读的文件大小
    function formatSize(bytes) {
        if (bytes === 0) return "0 B";
        const units = ["B", "KB", "MB", "GB"];
        const i = Math.floor(Math.log(bytes) / Math.log(1024));
        return (
            (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + " " + units[i]
        );
    }

    // 格式化修改时间（相对时间）
    function formatTime(timestamp) {
        if (!timestamp) return "";
        const now = Date.now() / 1000;
        const diff = now - timestamp;
        if (diff < 60) return "刚才";
        if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
        if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
        if (diff < 2592000) return `${Math.floor(diff / 86400)} 天前`;
        const date = new Date(timestamp * 1000);
        return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    }
</script>

<div class="file-list">
    {#each files as file (file.path)}
        {@const statusInfo = getStatusInfo(file.index_status)}
        {@const isSelected = selectedFile?.path === file.path}
        {@const isUnsupported = file.index_status === "Unsupported"}
        {@const isIgnored = file.index_status === "Ignored"}
        <button
            class="file-row"
            class:selected={isSelected}
            class:unsupported={isUnsupported}
            class:ignored={isIgnored}
            onclick={() => onSelect(file)}
        >
            <!-- 文件图标 -->
            <span
                class="material-symbols-outlined file-icon"
                style="color: {getFileIconColor(file.file_type)}"
            >
                {getFileIcon(file.file_type)}
            </span>

            <!-- 文件信息 -->
            <div class="file-info">
                <span class="file-name" class:strikethrough={isIgnored}>
                    {file.name}
                </span>
                {#if file.relative_path !== file.name}
                    <span class="file-path">{file.relative_path}</span>
                {/if}
            </div>

            <!-- 文件大小 -->
            <span class="file-size">{formatSize(file.size)}</span>

            <!-- 修改时间 -->
            <span class="file-time">{formatTime(file.modified)}</span>

            <!-- 索引状态 -->
            <span
                class="status-badge"
                class:pulse={file.index_status === "Indexing"}
                style="color: {statusInfo.color}; background: {statusInfo.bg}"
            >
                {statusInfo.label}
            </span>
        </button>
    {/each}

    {#if files.length === 0}
        <div class="empty-list">
            <span class="material-symbols-outlined empty-icon">folder_open</span
            >
            <p>没有找到匹配的文件</p>
        </div>
    {/if}
</div>

<style>
    .file-list {
        display: flex;
        flex-direction: column;
        gap: 1px;
    }

    .file-row {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 10px 16px;
        background: transparent;
        border: none;
        border-radius: 8px;
        cursor: pointer;
        transition: all 0.15s ease;
        text-align: left;
        width: 100%;
        color: var(--text-primary);
        font-family: inherit;
    }

    .file-row:hover {
        background: rgba(255, 255, 255, 0.04);
    }

    .file-row.selected {
        background: rgba(255, 255, 255, 0.08);
        box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
    }

    .file-row.unsupported {
        opacity: 0.5;
    }

    .file-row.ignored {
        opacity: 0.4;
    }

    .file-icon {
        font-size: 20px;
        flex-shrink: 0;
        width: 24px;
        text-align: center;
    }

    .file-info {
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: 2px;
    }

    .file-name {
        font-size: 13px;
        font-weight: 500;
        color: var(--text-primary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .file-name.strikethrough {
        text-decoration: line-through;
    }

    .file-path {
        font-size: 11px;
        color: var(--text-muted);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .file-size {
        font-size: 11px;
        color: var(--text-muted);
        flex-shrink: 0;
        width: 55px;
        text-align: right;
    }

    .file-time {
        font-size: 11px;
        color: var(--text-muted);
        flex-shrink: 0;
        width: 70px;
        text-align: right;
    }

    .status-badge {
        font-size: 10px;
        font-weight: 600;
        padding: 2px 8px;
        border-radius: 10px;
        flex-shrink: 0;
        letter-spacing: 0.02em;
    }

    .status-badge.pulse {
        animation: status-pulse 1.5s ease-in-out infinite;
    }

    @keyframes status-pulse {
        0%,
        100% {
            opacity: 1;
        }
        50% {
            opacity: 0.5;
        }
    }

    .empty-list {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        padding: 48px 16px;
        color: var(--text-muted);
        gap: 8px;
    }

    .empty-icon {
        font-size: 32px;
        opacity: 0.5;
    }

    .empty-list p {
        font-size: 13px;
    }
</style>
