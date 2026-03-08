<script>
    /**
     * Knowledge - 知识管理主页面
     * 显示目标目录下所有文件的索引状态
     */
    import { invoke } from "@tauri-apps/api/core";
    import { onMount, onDestroy } from "svelte";
    import FileList from "./Knowledge/FileList.svelte";
    import FileDetail from "./Knowledge/FileDetail.svelte";
    import FileChat from "./Knowledge/FileChat.svelte";
    import {
        navigation,
        VIEW_SETTINGS,
    } from "$lib/stores/navigation.svelte.js";

    // 状态
    let files = $state(navigation.knowledgeFiles || []);
    let filteredFiles = $state([]);
    let isLoading = $state(navigation.knowledgeFiles === null); // 首次加载时才显示 loading
    let error = $state(null);
    let dataDir = $state(null);
    let searchQuery = $state("");
    let selectedFile = $state(navigation.knowledgeSelectedFile);
    let activeTypeFilter = $state("all");
    let activeStatusFilter = $state("all");
    let chatFile = $state(navigation.knowledgeChatFile); // 当前聊天的文件，非null时显示聊天面板

    // 将 selectedFile 和 chatFile 同步到 navigation store（视图切换间持久化）
    $effect(() => {
        navigation.knowledgeSelectedFile = selectedFile;
    });
    $effect(() => {
        navigation.knowledgeChatFile = chatFile;
    });

    // 统计数据
    let stats = $derived({
        total: files.length,
        indexed: files.filter((f) => f.index_status === "Indexed").length,
        outdated: files.filter((f) => f.index_status === "Outdated").length,
        unindexed: files.filter((f) => f.index_status === "Unindexed").length,
        unsupported: files.filter((f) => f.index_status === "Unsupported")
            .length,
        ignored: files.filter((f) => f.index_status === "Ignored").length,
    });

    // 文件类型过滤选项
    const typeFilters = [
        { key: "all", label: "全部", icon: "folder" },
        {
            key: "text",
            label: "文本",
            icon: "description",
            types: ["Markdown", "Text", "Html"],
        },
        { key: "pdf", label: "PDF", icon: "picture_as_pdf", types: ["Pdf"] },
        {
            key: "office",
            label: "Office",
            icon: "business_center",
            types: ["Word", "PowerPoint", "Excel"],
        },
        {
            key: "media",
            label: "媒体",
            icon: "perm_media",
            types: ["Image", "Csv"],
        },
    ];

    // 过滤逻辑
    $effect(() => {
        let result = files;

        // 搜索过滤
        if (searchQuery.trim()) {
            const q = searchQuery.toLowerCase();
            result = result.filter(
                (f) =>
                    f.name.toLowerCase().includes(q) ||
                    f.relative_path.toLowerCase().includes(q),
            );
        }

        // 类型过滤
        if (activeTypeFilter !== "all") {
            const filter = typeFilters.find((f) => f.key === activeTypeFilter);
            if (filter?.types) {
                result = result.filter((f) =>
                    filter.types.includes(f.file_type),
                );
            }
        }

        // 状态过滤
        if (activeStatusFilter !== "all") {
            result = result.filter(
                (f) => f.index_status === activeStatusFilter,
            );
        }

        filteredFiles = result;
    });

    let searchInputRef = $state(null);

    function handleKeydown(e) {
        // Cmd/Ctrl+F → 聚焦搜索
        if ((e.metaKey || e.ctrlKey) && e.key === "f") {
            e.preventDefault();
            searchInputRef?.focus();
            return;
        }

        // Esc → 关闭详情面板
        if (e.key === "Escape") {
            if (chatFile) {
                chatFile = null;
            } else if (selectedFile) {
                selectedFile = null;
            }
            return;
        }

        // ↑↓ 方向键 → 导航文件列表
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const list = filteredFiles;
            if (list.length === 0) return;

            const currentIdx = selectedFile
                ? list.findIndex((f) => f.path === selectedFile.path)
                : -1;

            let nextIdx;
            if (e.key === "ArrowDown") {
                nextIdx = currentIdx < list.length - 1 ? currentIdx + 1 : 0;
            } else {
                nextIdx = currentIdx > 0 ? currentIdx - 1 : list.length - 1;
            }

            selectedFile = list[nextIdx];
            chatFile = null;
            return;
        }

        // Enter → 查看详情（如果已选择文件）
        if (e.key === "Enter" && selectedFile && !chatFile) {
            // 已选择文件时，Enter 不做额外操作，详情已自动显示
            return;
        }
    }

    onMount(async () => {
        // 只有首次加载时才网络请求，否则用缓存
        if (navigation.knowledgeFiles === null) {
            loadFiles();
        } else {
            // 从缓存恢复时仍需获取 dataDir（用于空状态判断）
            try {
                const config = await invoke("get_app_config");
                dataDir = config.data_dir;
            } catch (_) {}
        }
        window.addEventListener("keydown", handleKeydown);
    });

    onDestroy(() => {
        window.removeEventListener("keydown", handleKeydown);
    });

    async function loadFiles() {
        isLoading = true;
        error = null;

        try {
            // 获取配置
            const config = await invoke("get_app_config");
            dataDir = config.data_dir;

            if (!dataDir) {
                isLoading = false;
                return;
            }

            // 获取文件列表
            files = await invoke("list_knowledge_files");
            navigation.knowledgeFiles = files; // 缓存到 store
            console.log(`[Knowledge] Loaded ${files.length} files`);
        } catch (err) {
            console.error("[Knowledge] Error loading files:", err);
            error = err.toString();
        } finally {
            isLoading = false;
        }
    }

    function handleFileSelect(file) {
        selectedFile = file;
    }

    function handleCloseDetail() {
        selectedFile = null;
        chatFile = null;
    }

    function handleStartChat(file) {
        chatFile = file;
    }

    function handleBackFromChat() {
        chatFile = null;
    }

    async function handleUnignore(filePath) {
        try {
            await invoke("unignore_file", { filePath });
            await loadFiles();
        } catch (err) {
            console.error("[Knowledge] Unignore error:", err);
        }
    }

    function goToSettings() {
        navigation.setSettingsTab("general");
        navigation.setActiveView(VIEW_SETTINGS);
    }
</script>

<div class="knowledge-container">
    <!-- 标题栏 -->
    <div class="knowledge-header">
        <div class="header-left">
            <span class="material-symbols-outlined header-icon"
                >library_books</span
            >
            <h2>Knowledge</h2>
        </div>
        <button class="refresh-btn" onclick={loadFiles} disabled={isLoading}>
            <span class="material-symbols-outlined" class:spinning={isLoading}>
                refresh
            </span>
        </button>
    </div>

    {#if !dataDir && !isLoading}
        <!-- 未设置目录的空状态 -->
        <div class="empty-state">
            <div class="empty-icon-wrapper">
                <span class="material-symbols-outlined empty-main-icon"
                    >folder_off</span
                >
            </div>
            <h3>尚未设置索引目录</h3>
            <p>
                请先在设置中选择要索引的文件目录，Knowledge
                将帮你管理和探索其中的文件。
            </p>
            <button class="setup-btn" onclick={goToSettings}>
                <span class="material-symbols-outlined">settings</span>
                前往设置
            </button>
        </div>
    {:else if error}
        <!-- 错误状态 -->
        <div class="empty-state">
            <div class="empty-icon-wrapper error">
                <span class="material-symbols-outlined empty-main-icon"
                    >error_outline</span
                >
            </div>
            <h3>加载失败</h3>
            <p>{error}</p>
            <button class="setup-btn" onclick={loadFiles}>
                <span class="material-symbols-outlined">refresh</span>
                重试
            </button>
        </div>
    {:else}
        <!-- 搜索 + 过滤栏 -->
        <div class="toolbar">
            <div class="search-box">
                <span class="material-symbols-outlined search-icon">search</span
                >
                <input
                    type="text"
                    placeholder="搜索文件名..."
                    bind:value={searchQuery}
                    bind:this={searchInputRef}
                />
                {#if searchQuery}
                    <button
                        class="clear-btn"
                        onclick={() => (searchQuery = "")}
                    >
                        <span class="material-symbols-outlined">close</span>
                    </button>
                {/if}
            </div>

            <div class="filter-group">
                {#each typeFilters as filter}
                    <button
                        class="filter-chip"
                        class:active={activeTypeFilter === filter.key}
                        onclick={() => (activeTypeFilter = filter.key)}
                    >
                        <span class="material-symbols-outlined"
                            >{filter.icon}</span
                        >
                        {filter.label}
                    </button>
                {/each}
            </div>
        </div>

        <!-- 主内容区：左右分栏 -->
        <div class="main-content" class:has-detail={selectedFile}>
            <!-- 左侧：文件列表 -->
            <div class="file-list-container">
                {#if isLoading}
                    <!-- 骨架屏 -->
                    <div class="skeleton-list">
                        {#each Array(6) as _, i}
                            <div
                                class="skeleton-row"
                                style="animation-delay: {i * 0.08}s"
                            >
                                <div class="skeleton-icon"></div>
                                <div class="skeleton-info">
                                    <div class="skeleton-name"></div>
                                    <div class="skeleton-path"></div>
                                </div>
                                <div class="skeleton-badge"></div>
                            </div>
                        {/each}
                    </div>
                {:else}
                    <FileList
                        files={filteredFiles}
                        {selectedFile}
                        onSelect={handleFileSelect}
                    />
                    {#if filteredFiles.length > 0 && searchQuery}
                        <div class="filter-hint">
                            显示 {filteredFiles.length} / {files.length} 个文件
                        </div>
                    {/if}

                    <!-- 忽略列表 -->
                    {#if stats.ignored > 0}
                        <div class="ignored-section">
                            <div class="ignored-header">
                                <span class="material-symbols-outlined"
                                    >visibility_off</span
                                >
                                已忽略 ({stats.ignored})
                            </div>
                            {#each files.filter((f) => f.index_status === "Ignored") as ignoredFile (ignoredFile.path)}
                                <div class="ignored-item">
                                    <span class="ignored-name"
                                        >{ignoredFile.name}</span
                                    >
                                    <button
                                        class="restore-btn"
                                        onclick={() =>
                                            handleUnignore(ignoredFile.path)}
                                    >
                                        恢复
                                    </button>
                                </div>
                            {/each}
                        </div>
                    {/if}
                {/if}
            </div>

            <!-- 右侧：详情/聊天面板 -->
            {#if selectedFile}
                <div class="detail-container">
                    {#if chatFile}
                        <FileChat file={chatFile} onBack={handleBackFromChat} />
                    {:else}
                        <FileDetail
                            file={selectedFile}
                            onClose={handleCloseDetail}
                            onReindex={loadFiles}
                            onIgnore={() => {
                                selectedFile = null;
                                chatFile = null;
                                loadFiles();
                            }}
                            onChat={handleStartChat}
                        />
                    {/if}
                </div>
            {/if}
        </div>
    {/if}
</div>

<style>
    .knowledge-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        padding: 0;
        overflow: hidden;
    }

    .main-content {
        flex: 1;
        display: flex;
        min-height: 0;
        overflow: hidden;
    }

    .main-content .file-list-container {
        flex: 1;
        min-width: 0;
    }

    .main-content.has-detail .file-list-container {
        flex: 0 0 55%;
    }

    .detail-container {
        flex: 0 0 45%;
        min-width: 0;
        overflow: hidden;
        animation: slide-in 0.2s ease-out;
    }

    @keyframes slide-in {
        from {
            opacity: 0;
            transform: translateX(20px);
        }
        to {
            opacity: 1;
            transform: translateX(0);
        }
    }

    /* 标题栏 */
    .knowledge-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 16px 20px 12px;
        flex-shrink: 0;
    }

    .header-left {
        display: flex;
        align-items: center;
        gap: 10px;
    }

    .header-icon {
        font-size: 22px;
        color: var(--accent-primary);
        opacity: 0.8;
    }

    .knowledge-header h2 {
        font-size: 16px;
        font-weight: 600;
        color: var(--text-primary);
        margin: 0;
    }

    .refresh-btn {
        background: none;
        border: none;
        color: var(--text-muted);
        cursor: pointer;
        padding: 6px;
        border-radius: 6px;
        display: flex;
        align-items: center;
        transition: all 0.15s ease;
    }

    .refresh-btn:hover {
        background: rgba(255, 255, 255, 0.06);
        color: var(--text-primary);
    }

    .refresh-btn .material-symbols-outlined {
        font-size: 18px;
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

    /* 搜索 + 过滤 */
    .toolbar {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 0 20px 10px;
        flex-shrink: 0;
    }

    .search-box {
        display: flex;
        align-items: center;
        gap: 6px;
        background: rgba(255, 255, 255, 0.04);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 0 10px;
        flex: 1;
        max-width: 240px;
        transition: border-color 0.15s ease;
    }

    .search-box:focus-within {
        border-color: var(--accent-primary);
    }

    .search-icon {
        font-size: 16px;
        color: var(--text-muted);
        flex-shrink: 0;
    }

    .search-box input {
        background: none;
        border: none;
        outline: none;
        color: var(--text-primary);
        font-size: 12px;
        padding: 7px 0;
        width: 100%;
        font-family: inherit;
    }

    .search-box input::placeholder {
        color: var(--text-muted);
    }

    .clear-btn {
        background: none;
        border: none;
        color: var(--text-muted);
        cursor: pointer;
        padding: 2px;
        display: flex;
        border-radius: 4px;
    }

    .clear-btn:hover {
        color: var(--text-primary);
    }

    .clear-btn .material-symbols-outlined {
        font-size: 14px;
    }

    .filter-group {
        display: flex;
        gap: 4px;
    }

    .filter-chip {
        display: flex;
        align-items: center;
        gap: 4px;
        background: none;
        border: 1px solid transparent;
        border-radius: 6px;
        padding: 4px 8px;
        font-size: 11px;
        color: var(--text-muted);
        cursor: pointer;
        transition: all 0.15s ease;
        font-family: inherit;
        white-space: nowrap;
    }

    .filter-chip:hover {
        background: rgba(255, 255, 255, 0.04);
        color: var(--text-secondary);
    }

    .filter-chip.active {
        background: rgba(255, 255, 255, 0.08);
        color: var(--text-primary);
        border-color: rgba(255, 255, 255, 0.1);
    }

    .filter-chip .material-symbols-outlined {
        font-size: 14px;
    }

    /* 文件列表容器 */
    .file-list-container {
        flex: 1;
        overflow-y: auto;
        padding: 0 12px 12px;
        min-height: 0;
    }

    .filter-hint {
        text-align: center;
        font-size: 11px;
        color: var(--text-muted);
        padding: 8px;
    }

    /* 忽略列表 */
    .ignored-section {
        margin-top: 16px;
        padding: 0 8px;
    }

    .ignored-header {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
        font-weight: 600;
        color: var(--text-muted);
        padding: 8px 8px 6px;
    }

    .ignored-header .material-symbols-outlined {
        font-size: 14px;
    }

    .ignored-item {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 6px 8px;
        border-radius: 6px;
    }

    .ignored-item:hover {
        background: rgba(255, 255, 255, 0.03);
    }

    .ignored-name {
        font-size: 11px;
        color: var(--text-muted);
        text-decoration: line-through;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        flex: 1;
        min-width: 0;
    }

    .restore-btn {
        background: none;
        border: 1px solid var(--border-color);
        color: var(--text-muted);
        font-size: 10px;
        padding: 2px 8px;
        border-radius: 4px;
        cursor: pointer;
        transition: all 0.15s ease;
        font-family: inherit;
        flex-shrink: 0;
    }

    .restore-btn:hover {
        background: rgba(255, 255, 255, 0.06);
        color: var(--text-primary);
        border-color: var(--accent-primary);
    }

    /* 空状态 */
    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        flex: 1;
        padding: 32px;
        text-align: center;
        gap: 12px;
    }

    .empty-icon-wrapper {
        width: 64px;
        height: 64px;
        border-radius: 16px;
        background: rgba(255, 255, 255, 0.04);
        display: flex;
        align-items: center;
        justify-content: center;
        margin-bottom: 4px;
    }

    .empty-icon-wrapper.error {
        background: rgba(239, 68, 68, 0.1);
    }

    .empty-main-icon {
        font-size: 28px;
        color: var(--text-muted);
    }

    .empty-icon-wrapper.error .empty-main-icon {
        color: #ef4444;
    }

    .empty-state h3 {
        font-size: 15px;
        font-weight: 600;
        color: var(--text-primary);
        margin: 0;
    }

    .empty-state p {
        font-size: 12px;
        color: var(--text-muted);
        max-width: 280px;
        line-height: 1.5;
        margin: 0;
    }

    .setup-btn {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 8px 16px;
        background: rgba(255, 255, 255, 0.06);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        color: var(--text-primary);
        font-size: 12px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        margin-top: 4px;
        font-family: inherit;
    }

    .setup-btn:hover {
        background: rgba(255, 255, 255, 0.1);
        border-color: var(--accent-primary);
    }

    .setup-btn .material-symbols-outlined {
        font-size: 16px;
    }

    /* 骨架屏 */
    .skeleton-list {
        display: flex;
        flex-direction: column;
        gap: 2px;
        padding: 0 4px;
    }

    .skeleton-row {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 12px 16px;
        animation: skeleton-fade 1.2s ease-in-out infinite;
    }

    .skeleton-icon {
        width: 24px;
        height: 24px;
        border-radius: 6px;
        background: var(--skeleton-start);
        flex-shrink: 0;
    }

    .skeleton-info {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 6px;
    }

    .skeleton-name {
        height: 12px;
        width: 60%;
        border-radius: 4px;
        background: var(--skeleton-start);
    }

    .skeleton-path {
        height: 10px;
        width: 40%;
        border-radius: 4px;
        background: var(--skeleton-start);
    }

    .skeleton-badge {
        width: 48px;
        height: 18px;
        border-radius: 10px;
        background: var(--skeleton-start);
        flex-shrink: 0;
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
