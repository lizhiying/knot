<script>
    /**
     * Doc Parser 页面
     * 参考 knot-doc-parser.html 的设计风格重构
     */
    import { onMount } from "svelte";
    import { marked } from "marked";
    import { listen } from "@tauri-apps/api/event";

    // 配置 marked：启用 GFM 表格、换行、原始 HTML 渲染
    marked.setOptions({
        breaks: true,
        gfm: true,
    });

    const { invoke } = window.__TAURI__.core;
    const { open } = window.__TAURI__.dialog;

    let selectedFilePath = $state(null);
    let fileName = $state("选择文件...");
    let fileExtension = $state("");
    let parseDisabled = $state(true);
    let isParsing = $state(false);

    // Progress
    let showProgress = $state(false);
    let progressPercent = $state(0);
    let progressText = $state("Parsing document...");

    // Tab state
    let activeTab = $state("preview");

    // Content
    let treeContent = $state("");
    let renderContent = $state("");
    let jsonContent = $state("");

    // Metadata from parsing
    let parseMetadata = $state(null);

    async function selectFile() {
        try {
            const selected = await open({
                multiple: false,
                filters: [
                    {
                        name: "Supported Documents",
                        extensions: [
                            "md",
                            "markdown",
                            "pdf",
                            "docx",
                            "pptx",
                            "xlsx",
                        ],
                    },
                ],
            });

            if (selected) {
                selectedFilePath = selected;
                const fn =
                    selected.split("/").pop() || selected.split("\\").pop();
                fileName = fn;
                const ext = fn.split(".").pop()?.toUpperCase() || "";
                fileExtension = ext;
                parseDisabled = false;

                // Reset
                showProgress = false;
                progressPercent = 0;
                treeContent = "";
                renderContent = "";
                jsonContent = "";
                parseMetadata = null;
            }
        } catch (err) {
            console.error("选择文件失败:", err);
        }
    }

    async function parseFile() {
        if (!selectedFilePath || isParsing) return;

        isParsing = true;
        parseDisabled = true;
        showProgress = true;
        progressPercent = 5;
        progressText = "Initializing...";
        renderContent = ""; // 清空之前的内容

        try {
            const result = await invoke("parse_file", {
                path: selectedFilePath,
            });
            displayResult(result);
            progressPercent = 100;
            parseMetadata = result.metadata;
        } catch (err) {
            console.error("解析失败:", err);
            // Show explicit error dialog
            try {
                // Assuming tauri-plugin-dialog is available as 'message' from window.__TAURI__.dialog?
                // Checks imports: const { open } = window.__TAURI__.dialog.
                // Need to import message.
                const { message } = window.__TAURI__.dialog;
                await message(`解析失败: ${err}`, {
                    title: "Error",
                    kind: "error",
                });
            } catch (e) {
                alert(`解析失败: ${err}`);
            }

            treeContent = `<div class="empty-state"><span class="material-symbols-outlined">error</span><p>解析失败: ${err}</p></div>`;
            renderContent = `<div class="empty-state"><span class="material-symbols-outlined">error</span><p>解析失败</p></div>`;
        } finally {
            isParsing = false;
            parseDisabled = false;
            // Hide progress bar after a delay
            setTimeout(() => {
                showProgress = false;
                progressPercent = 0;
            }, 500);

            // Stop the engine to free resources
            try {
                await invoke("stop_parsing_llm");
            } catch (e) {
                console.error("Failed to stop LLM:", e);
            }
        }
    }

    function displayResult(node) {
        // Tree view
        treeContent = renderTreeNode(node);

        // JSON view
        const replacer = (key, value) => {
            if (key === "embedding" && Array.isArray(value)) {
                return `[... ${value.length} dimensions ...]`;
            }
            if (
                typeof value === "string" &&
                value.length > 500 &&
                value.startsWith("iVBOR")
            ) {
                return value.substring(0, 50) + "... [Base64 Truncated]";
            }
            return value;
        };
        jsonContent = JSON.stringify(node, replacer, 2);

        // Render view
        function aggregateContent(n) {
            let result = n.content || "";
            if (n.children && n.children.length > 0) {
                for (const child of n.children) {
                    const childContent = aggregateContent(child);
                    if (childContent) {
                        if (result) result += "\n\n";
                        result += childContent;
                    }
                }
            }
            return result;
        }
        let fullContent = aggregateContent(node);

        // Image store
        function aggregateImageStore(n, store) {
            if (n.metadata?.extra) {
                Object.assign(store, n.metadata.extra);
            }
            if (n.children) {
                for (const child of n.children) {
                    aggregateImageStore(child, store);
                }
            }
        }
        let imageStore = {};
        aggregateImageStore(node, imageStore);

        // PDF images
        let imgRegex = /<Image>\((\d+),(\d+)\),\((\d+),(\d+)\)<\/Image>/g;
        fullContent = fullContent.replace(imgRegex, (match, x1, y1, x2, y2) => {
            const key = `image:(${x1},${y1})-(${x2},${y2})`;
            const base64 = imageStore[key];
            if (base64) return `![Figure](data:image/jpeg;base64,${base64})`;
            return `> *[Image Data Not Found: ${key}]*`;
        });

        // DOCX images
        const docxImgRegex = /<Image rId="([^"]+)"><\/Image>/g;
        fullContent = fullContent.replace(docxImgRegex, (match, rId) => {
            const key = `image:${rId}`;
            const base64 = imageStore[key];
            if (base64) return `![Figure](data:image/png;base64,${base64})`;
            return `> *[Image Data Not Found: ${key}]*`;
        });

        if (fullContent) {
            renderContent = marked.parse(fullContent);
        } else {
            renderContent = "";
        }
    }

    function renderTreeNode(node, depth = 0) {
        const hasChildren = node.children && node.children.length > 0;
        const contentPreview = node.content
            ? node.content.length > 300
                ? node.content.substring(0, 300) + "..."
                : node.content
            : "";

        let pageInfo = "";
        if (node.metadata?.page_number) {
            pageInfo = `<span>📄 Page ${node.metadata.page_number}</span>`;
        } else if (node.metadata?.line_number) {
            pageInfo = `<span>📍 Line ${node.metadata.line_number}</span>`;
        }

        let html = `
    <div class="tree-node" style="--depth: ${depth}">
      <div class="tree-node-header">
        <span class="tree-toggle">${hasChildren ? "▼" : "•"}</span>
        <span class="tree-title">${escapeHtml(node.title || "(无标题)")}</span>
        <span class="tree-level">Level ${node.level}</span>
      </div>
      ${contentPreview ? `<div class="tree-content">${escapeHtml(contentPreview)}</div>` : ""}
      <div class="tree-meta">
        <span>📝 ${node.metadata?.token_count || 0} tokens</span>
        ${pageInfo}
        ${node.summary ? `<span>📋 有摘要</span>` : ""}
        ${node.embedding ? `<span>🔢 有向量</span>` : ""}
      </div>`;

        if (hasChildren) {
            html += `<div class="tree-children">`;
            for (const child of node.children) {
                html += renderTreeNode(child, depth + 1);
            }
            html += `</div>`;
        }

        html += `</div>`;
        return html;
    }

    function escapeHtml(text) {
        const div = document.createElement("div");
        div.textContent = text;
        return div.innerHTML;
    }

    function switchTab(tab) {
        activeTab = tab;
    }

    onMount(() => {
        // Listen for progress events
        const unlisten = listen("parse-progress", (event) => {
            const { current, total } = event.payload;
            let percent = Math.round((current / total) * 100);
            if (percent >= 100) percent = 95;
            progressPercent = percent;
            progressText = `Parsing page ${current}/${total}...`;
            if (!showProgress) showProgress = true;
        });

        // Listen for per-page markdown content
        const unlisten2 = listen("parse-page-ready", (event) => {
            const { pageIndex, totalPages, markdown } = event.payload;
            if (markdown && markdown.trim()) {
                // 追加到 render preview（每页之间加分页线）
                const pageHtml = marked.parse(markdown);
                const separator = renderContent
                    ? `<hr class="page-divider" /><div class="page-label">Page ${pageIndex + 1}</div>`
                    : `<div class="page-label">Page ${pageIndex + 1}</div>`;
                renderContent = (renderContent || "") + separator + pageHtml;
                // 自动切到 preview tab
                if (activeTab !== "preview") {
                    activeTab = "preview";
                }
            }
        });

        return () => {
            unlisten.then((fn) => fn());
            unlisten2.then((fn) => fn());
        };
    });
</script>

<!-- Main Layout derived from ViewParser in new_ui_design.jsx -->
<div
    class="h-full flex overflow-hidden text-[var(--text-primary)] doc-parser-app"
>
    <!-- Left Sidebar: Pipeline & Analysis -->
    <div
        class="w-[320px] flex flex-col border-r border-[var(--border-color)] shrink-0"
    >
        <!-- Header -->
        <div class="mt-4 flex items-center px-4 shrink-0">
            <span class="text-sm font-semibold text-[var(--text-primary)]"
                >Parsing Analysis</span
            >
        </div>

        <div class="flex-1 overflow-y-auto p-4 flex flex-col gap-2">
            <!-- Status Card -->
            <div>
                <div
                    class="p-4 rounded-xl bg-[var(--bg-card)] border border-[var(--border-color)] shadow-sm mb-4"
                >
                    <div class="flex items-center gap-3 mb-2">
                        <div
                            class="p-1 rounded-full bg-[var(--accent-primary)] text-white flex items-center justify-center"
                        >
                            <span class="material-symbols-outlined text-[14px]"
                                >{parseMetadata
                                    ? "check_circle"
                                    : "pending"}</span
                            >
                        </div>
                        <span
                            class="text-sm font-medium text-[var(--text-primary)]"
                        >
                            {parseMetadata ? "Parse Complete" : "Waiting..."}
                        </span>
                    </div>
                    <p class="text-xs text-[var(--text-secondary)] pl-8">
                        {parseMetadata
                            ? "Structure extracted successfully."
                            : "Waiting for file to be parsed..."}
                    </p>
                </div>

                <!-- Metrics Card -->
                <div
                    class="p-4 rounded-xl bg-[var(--bg-card)] border border-[var(--border-color)] shadow-sm"
                >
                    <div
                        class="flex items-center gap-2 mb-3 text-[var(--text-primary)] font-medium text-xs"
                    >
                        <span
                            class="material-symbols-outlined text-[14px] text-[var(--text-muted)]"
                            >description</span
                        >
                        File Metrics
                    </div>
                    <div class="grid grid-cols-2 gap-y-2 text-xs">
                        <span class="text-[var(--text-muted)]">Format</span>
                        <span class="text-right text-[var(--text-primary)]"
                            >{fileExtension || "-"}</span
                        >
                        <span class="text-[var(--text-muted)]">Tokens</span>
                        <span class="text-right text-[var(--text-primary)]"
                            >{parseMetadata?.token_count?.toLocaleString() ||
                                "-"}</span
                        >
                        <span class="text-[var(--text-muted)]">Encoding</span>
                        <span class="text-right text-[var(--text-primary)]"
                            >UTF-8</span
                        >
                    </div>
                </div>
            </div>

            <!-- Pipeline -->
            <div class="flex-1">
                <h3
                    class="text-xs font-bold text-[var(--text-muted)] uppercase mt-4 mb-4 flex items-center gap-2"
                >
                    <span class="material-symbols-outlined text-[14px]"
                        >memory</span
                    > Processing Pipeline
                </h3>
                <div class="relative pl-2 space-y-6">
                    <!-- Timeline Line -->
                    <div
                        class="absolute left-[17px] top-2 bottom-7 w-0.5 bg-[var(--border-color)]"
                    ></div>

                    <!-- Step 1 -->
                    <div class="relative flex items-start gap-4">
                        <div
                            class="z-10 w-5 h-5 rounded-full bg-[var(--accent-primary)] flex items-center justify-center"
                        >
                            <div
                                class="w-1.5 h-1.5 rounded-full bg-white"
                            ></div>
                        </div>
                        <div>
                            <p
                                class="text-sm font-medium text-[var(--text-primary)]"
                            >
                                Layout Analysis
                            </p>
                            <p class="text-xs text-[var(--text-muted)]">
                                Ready
                            </p>
                        </div>
                    </div>

                    <!-- Step 2: Text Extraction -->
                    <div class="relative flex items-start gap-4">
                        <div
                            class="z-10 w-5 h-5 rounded-full border-2 {isParsing ||
                            parseMetadata
                                ? 'bg-[var(--accent-primary)] border-transparent'
                                : 'border-[var(--border-light)] bg-[var(--bg-primary)]'} flex items-center justify-center transition-colors"
                        >
                            {#if isParsing || parseMetadata}
                                <div
                                    class="w-1.5 h-1.5 rounded-full bg-white"
                                ></div>
                            {/if}
                        </div>
                        <div>
                            <p
                                class="text-sm font-medium {isParsing
                                    ? 'text-[var(--text-primary)]'
                                    : 'text-[var(--text-muted)]'}"
                            >
                                Text Extraction
                            </p>
                            <p class="text-xs text-[var(--text-muted)]">
                                {isParsing
                                    ? "Processing..."
                                    : parseMetadata
                                      ? "Completed"
                                      : "Pending"}
                            </p>
                        </div>
                    </div>

                    <!-- Step 3: Semantic Indexing -->
                    <div class="relative flex items-start gap-4">
                        <div
                            class="z-10 w-5 h-5 rounded-full border-2 {parseMetadata
                                ? 'bg-[var(--accent-primary)] border-transparent'
                                : 'border-[var(--border-light)] bg-[var(--bg-primary)]'} flex items-center justify-center transition-colors"
                        >
                            {#if parseMetadata}
                                <span
                                    class="material-symbols-outlined text-white text-[10px]"
                                    >check</span
                                >
                            {/if}
                        </div>
                        <div>
                            <p
                                class="text-sm font-medium {parseMetadata
                                    ? 'text-[var(--text-primary)]'
                                    : 'text-[var(--text-muted)]'}"
                            >
                                Semantic Indexing
                            </p>
                            <p class="text-xs text-[var(--text-muted)]">
                                {parseMetadata ? "Completed" : "Pending"}
                            </p>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- Right Panel: Content -->
    <div
        class="flex-1 flex flex-col p-6 bg-[var(--bg-sidebar)] overflow-hidden"
    >
        <!-- Toolbar -->
        <div class="flex justify-between items-center mb-4 shrink-0">
            <!-- File Chip (Input Trigger) -->
            <div
                class="min-w-[200px] flex items-center gap-3 p-2 rounded-lg bg-[var(--bg-card)] border border-[var(--border-color)] shadow-sm cursor-pointer hover:border-[var(--accent-primary)] transition-colors group select-none"
                role="button"
                tabindex="0"
                onclick={selectFile}
                onkeydown={(e) => e.key === "Enter" && selectFile()}
            >
                <span
                    class="material-symbols-outlined text-[var(--text-muted)] text-[16px] group-hover:text-[var(--accent-primary)] transition-colors"
                >
                    {fileExtension === "PDF" ? "picture_as_pdf" : "description"}
                </span>
                <span
                    class="text-sm font-medium text-[var(--text-primary)] max-w-[200px] truncate"
                >
                    {fileName || "Select File..."}
                </span>
                {#if fileExtension}
                    <span
                        class="text-[10px] px-1.5 rounded bg-[var(--border-color)] text-[var(--text-secondary)] font-mono"
                    >
                        {fileExtension}
                    </span>
                {/if}
            </div>

            <!-- Parse Button -->
            <button
                class="bg-[var(--bg-card)] text-[var(--text-primary)] border border-[var(--border-color)] px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-2 hover:bg-[var(--bg-card-hover)] hover:border-[var(--accent-primary)] disabled:opacity-50 disabled:cursor-not-allowed transition-colors shadow-sm"
                disabled={parseDisabled || isParsing}
                onclick={parseFile}
            >
                {#if isParsing}
                    <span class="material-symbols-outlined text-[13px] spin"
                        >sync</span
                    >
                    Parsing...
                {:else}
                    <span
                        class="material-symbols-outlined text-[13px] text-[var(--accent-primary)]"
                        >bolt</span
                    >
                    Structure Parse
                {/if}
            </button>
        </div>

        <!-- Main Content Card -->
        <div
            class="flex-1 border border-[var(--border-color)] rounded-xl bg-[var(--bg-card)] overflow-hidden flex flex-col shadow-sm"
        >
            <!-- Tabs Header -->
            <div
                class="flex items-center border-b border-[var(--border-color)] px-4 py-2 gap-4 shrink-0 bg-[var(--bg-card)]"
            >
                <button
                    class="text-xs font-medium flex items-center gap-1 transition-colors {activeTab ===
                    'tree'
                        ? 'text-[var(--text-primary)]'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'}"
                    onclick={() => switchTab("tree")}
                >
                    <span
                        class="material-symbols-outlined text-[14px] {activeTab ===
                        'tree'
                            ? 'text-[var(--accent-primary)]'
                            : ''}">account_tree</span
                    >
                    Structure Tree
                </button>
                <button
                    class="text-xs font-medium flex items-center gap-1 transition-colors {activeTab ===
                    'preview'
                        ? 'text-[var(--text-primary)]'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'}"
                    onclick={() => switchTab("preview")}
                >
                    <span
                        class="material-symbols-outlined text-[14px] {activeTab ===
                        'preview'
                            ? 'text-[var(--accent-primary)]'
                            : ''}">visibility</span
                    >
                    Render Preview
                </button>
                <button
                    class="text-xs font-medium flex items-center gap-1 transition-colors ml-auto {activeTab ===
                    'json'
                        ? 'text-[var(--text-primary)]'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'}"
                    onclick={() => switchTab("json")}
                >
                    <span
                        class="material-symbols-outlined text-[14px] {activeTab ===
                        'json'
                            ? 'text-[var(--accent-primary)]'
                            : ''}">data_object</span
                    >
                    JSON
                </button>
            </div>

            <!-- Content Body -->
            <div
                class="flex-1 overflow-y-auto relative p-6 bg-[var(--bg-card)]"
            >
                <!-- Loading State -->
                {#if isParsing && !renderContent}
                    <div
                        class="absolute inset-0 flex flex-col items-center justify-center text-[var(--text-secondary)] bg-[var(--bg-card)] z-10"
                    >
                        <span
                            class="material-symbols-outlined spin text-4xl mb-4 text-[var(--accent-primary)]"
                            >progress_activity</span
                        >
                        <p>{progressText}</p>
                    </div>
                {:else if isParsing && renderContent}
                    <!-- 正在解析但已有内容：显示内容 + 顶部进度条 -->
                    <div class="view-preview animate-fade-in markdown-body">
                        <div
                            class="sticky top-0 z-10 bg-[var(--bg-card)] border-b border-[var(--border-color)] px-4 py-2 mb-4 flex items-center gap-2 text-xs text-[var(--accent-primary)]"
                        >
                            <span class="material-symbols-outlined spin text-sm"
                                >progress_activity</span
                            >
                            <span>{progressText}</span>
                            <div
                                class="flex-1 h-1 bg-[var(--border-color)] rounded-full overflow-hidden ml-2"
                            >
                                <div
                                    class="h-full bg-[var(--accent-primary)] rounded-full transition-all duration-300"
                                    style="width: {progressPercent}%"
                                ></div>
                            </div>
                        </div>
                        {@html renderContent}
                    </div>
                {:else if !treeContent && !renderContent && !jsonContent}
                    <div
                        class="absolute inset-0 flex flex-col items-center justify-center text-[var(--text-muted)]"
                    >
                        <div
                            class="w-16 h-12 rounded-full bg-[var(--bg-sidebar)] flex items-center justify-center mb-2 text-[var(--text-muted)]"
                        >
                            <span class="material-symbols-outlined text-3xl"
                                >drive_folder_upload</span
                            >
                        </div>
                        <p class="text-sm">
                            Select a document using the top bar to start.
                        </p>
                    </div>
                {:else if activeTab === "tree"}
                    <div class="view-tree animate-fade-in relative">
                        <!-- Tree Visualization Line -->
                        <div
                            class="absolute left-[7px] top-6 bottom-0 w-px bg-[var(--border-color)] z-0"
                        ></div>
                        {@html treeContent}
                    </div>
                {:else if activeTab === "preview"}
                    <div class="view-preview animate-fade-in markdown-body">
                        {#if renderContent}
                            {@html renderContent}
                        {:else}
                            <div
                                class="h-full flex flex-col items-center justify-center text-[var(--text-muted)]"
                            >
                                <p>No preview content available.</p>
                            </div>
                        {/if}
                    </div>
                {:else if activeTab === "json"}
                    <div class="view-json animate-fade-in">
                        <pre
                            class="json-code text-xs font-mono bg-[var(--bg-sidebar)] p-4 rounded-lg border border-[var(--border-color)] overflow-auto">{jsonContent ||
                                "{ }"}</pre>
                    </div>
                {/if}
            </div>
        </div>
    </div>
</div>

<style>
    /* Global Styles */
    :global(body) {
        margin: 0;
        padding: 0;
        background: transparent;
    }

    /* Animation Utilities */
    .spin {
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
    .animate-fade-in {
        animation: fadeIn 0.3s ease-out;
    }
    @keyframes fadeIn {
        from {
            opacity: 0;
            transform: translateY(5px);
        }
        to {
            opacity: 1;
            transform: translateY(0);
        }
    }

    /* Tree View Styling (Custom to match design) */
    :global(.tree-node) {
        position: relative;
        z-index: 1;
    }
    :global(.tree-node-header) {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-bottom: 12px;
        padding: 6px;
        border-radius: 8px;
        width: fit-content;
        border: 1px solid transparent;
        transition: all 0.2s;
    }
    :global(.tree-node-header:hover) {
        background: var(--bg-card-hover);
        border-color: var(--border-color);
    }

    :global(.tree-toggle) {
        /* No toggle needed if we show all, but keeping logic. Using standard chevron */
        font-size: 14px;
        color: var(--text-muted);
        display: flex;
        align-items: center;
    }

    :global(.tree-title) {
        font-size: 14px;
        font-weight: 500;
        color: var(--text-primary);
    }

    :global(.tree-level) {
        font-size: 10px;
        color: var(--accent-primary);
        background: var(--accent-glow);
        padding: 2px 6px;
        border-radius: 4px;
        margin-left: 12px;
        font-family: "JetBrains Mono", monospace;
    }

    :global(.tree-children) {
        position: relative;
        padding-left: 24px; /* Indent */
        margin-left: 7px; /* Align with parent line */
        border-left: 1px solid var(--border-color);
    }

    :global(.tree-content) {
        margin-left: 32px;
        font-size: 12px;
        color: var(--text-secondary);
        padding: 8px;
        background: var(--bg-sidebar);
        border-radius: 6px;
        margin-bottom: 8px;
        border: 1px solid var(--border-color);
        /* Line Clamp */
        display: -webkit-box;
        -webkit-line-clamp: 3;
        line-clamp: 3;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }

    :global(.tree-meta) {
        margin-left: 32px;
        margin-bottom: 16px;
        font-size: 10px;
        color: var(--text-muted);
        display: flex;
        gap: 8px;
    }

    /* Markdown Preview */
    :global(.markdown-body) {
        color: var(--text-primary);
        font-size: 14px;
        line-height: 1.6;
    }
    :global(.markdown-body h1, .markdown-body h2, .markdown-body h3) {
        color: var(--text-primary);
        margin-top: 1.5em;
        margin-bottom: 0.5em;
        font-weight: 600;
    }
    :global(.markdown-body code) {
        background: var(--bg-sidebar);
        padding: 0.2em 0.4em;
        border-radius: 4px;
        font-family: "JetBrains Mono", monospace;
        font-size: 85%;
    }
    :global(.markdown-body pre) {
        background: var(--bg-sidebar);
        padding: 16px;
        border-radius: 8px;
        overflow: auto;
    }
    :global(.markdown-body img) {
        border-radius: 8px;
        border: 1px solid var(--border-color);
        max-width: 100%;
    }

    /* 段落间距 */
    :global(.markdown-body p) {
        margin: 0.8em 0;
        line-height: 1.7;
    }

    /* 表格样式 */
    :global(.markdown-body table) {
        border-collapse: collapse;
        width: 100%;
        margin: 1em 0;
        font-size: 13px;
        border: 1px solid var(--border-color);
        border-radius: 6px;
        overflow: hidden;
    }
    :global(.markdown-body thead) {
        background: var(--bg-sidebar);
    }
    :global(.markdown-body th) {
        padding: 8px 12px;
        text-align: left;
        font-weight: 600;
        color: var(--text-primary);
        border-bottom: 2px solid var(--border-color);
        border-right: 1px solid var(--border-color);
        white-space: nowrap;
    }
    :global(.markdown-body td) {
        padding: 6px 12px;
        border-bottom: 1px solid var(--border-color);
        border-right: 1px solid var(--border-color);
        color: var(--text-secondary);
    }
    :global(.markdown-body tr:nth-child(even)) {
        background: var(--bg-sidebar);
    }
    :global(.markdown-body tr:hover) {
        background: var(--bg-card-hover);
    }

    /* 列表样式 */
    :global(.markdown-body ul, .markdown-body ol) {
        padding-left: 1.5em;
        margin: 0.5em 0;
    }
    :global(.markdown-body li) {
        margin: 0.3em 0;
    }

    /* blockquote */
    :global(.markdown-body blockquote) {
        border-left: 3px solid var(--accent-primary);
        padding: 0.5em 1em;
        margin: 1em 0;
        color: var(--text-secondary);
        background: var(--bg-sidebar);
        border-radius: 0 6px 6px 0;
    }

    /* 分割线 */
    :global(.markdown-body hr) {
        border: none;
        border-top: 1px solid var(--border-color);
        margin: 1.5em 0;
    }

    /* 分页线（逐页渲染时的页面分隔） */
    :global(.page-divider) {
        border: none;
        border-top: 2px dashed var(--border-color);
        margin: 2em 0 0.5em 0;
    }
    :global(.page-label) {
        font-size: 10px;
        color: var(--text-muted);
        font-family: "JetBrains Mono", monospace;
        text-transform: uppercase;
        letter-spacing: 1px;
        margin-bottom: 1em;
        padding: 2px 8px;
        background: var(--bg-sidebar);
        border-radius: 4px;
        display: inline-block;
    }
</style>
