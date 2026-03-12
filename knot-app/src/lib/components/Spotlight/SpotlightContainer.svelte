<script>
    /**
     * Spotlight 主容器组件
     * 管理整个 Spotlight 界面的状态和交互
     */
    import SearchHeader from "./SearchHeader.svelte";
    import ResultsPanel from "./ResultsPanel.svelte";
    import SpotlightFooter from "./SpotlightFooter.svelte";
    import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
    import { currentMonitor } from "@tauri-apps/api/window";
    import {
        navigation,
        VIEW_SEARCH,
        VIEW_DOC_PARSER,
        VIEW_KNOWLEDGE,
        VIEW_SETTINGS,
    } from "$lib/stores/navigation.svelte.js";
    import DocParser from "./../DocParser.svelte";
    import Settings from "./../Settings.svelte";
    import Knowledge from "./../Knowledge.svelte";
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { onMount } from "svelte";

    const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

    let { visible = true } = $props();

    // 状态管理
    let isLoading = $state(false);
    let isStreaming = $state(false);
    let showResults = $state(false);
    let searchIconName = $state("search");
    let spotlightWidth = $state(896);
    let searchResults = $state([]);
    let highlightedCardId = $state(null);
    let searchQuery = $state("");
    let isModelReady = $state(true);

    let isMainContentVisible = $state(false);

    $effect(() => {
        if (!visible) {
            isMainContentVisible = false;
        }
    });

    // Watch query to reset results
    $effect(() => {
        if (!searchQuery.trim()) {
            showResults = false;
        }
    });

    // 洞察状态
    let insightState = $state({
        status: "Ready",
        statusType: "ready",
        isThinking: false,
        content: "",
        showCursor: false,
    });

    // Mock 数据
    const MOCK_DATA = {
        results: [
            {
                id: 1,
                title: "Knot_Vision_2025.pdf",
                score: 0.98,
                snippet:
                    "The foundational pillar of Knot RAG is local-first data processing. By leveraging vector extensions, we ensure retrieval latency stays sub-50ms.",
                path: "Company > Strategy > High_Level",
            },
            {
                id: 2,
                title: "Security_Audit_Report.docx",
                score: 0.89,
                snippet:
                    "User data isolation is maintained via namespace-encrypted embedding databases. No external API calls are triggered for the initial retrieval phase.",
                path: "Internal > IT > Compliance",
            },
            {
                id: 3,
                title: "Architecture_Diagrams.excalidraw",
                score: 0.76,
                snippet:
                    "Spotlight interface serves as the primary entry point for all RAG queries, providing instant visual feedback on document provenance.",
                path: "Projects > Design > Assets",
            },
        ],
        response: `<p><strong>Knot RAG</strong> represents a paradigm shift in how users interact with their private documents.</p>
<p>Unlike traditional search, Knot utilizes a <strong>Hybrid Retrieval</strong> system <span class="citation-tag inline-flex items-center justify-center w-4 h-4 rounded bg-blue-500/20 text-blue-400 text-[9px] font-bold border border-blue-500/30 ml-1" data-id="1">1</span> that merges semantic understanding with precise keyword matching.</p>
<p>Key technical advantages:</p>
<ul style="margin-left: 1.5rem; margin-bottom: 1rem;">
  <li><strong>Latency</strong>: Average response time of 120ms for synthesis.</li>
  <li><strong>Privacy</strong>: 100% on-device processing using quantized weights <span class="citation-tag inline-flex items-center justify-center w-4 h-4 rounded bg-blue-500/20 text-blue-400 text-[9px] font-bold border border-blue-500/30 ml-1" data-id="2">2</span>.</li>
  <li><strong>Transparency</strong>: Every sentence is backed by verified evidence <span class="citation-tag inline-flex items-center justify-center w-4 h-4 rounded bg-blue-500/20 text-blue-400 text-[9px] font-bold border border-blue-500/30 ml-1" data-id="3">3</span>.</li>
</ul>
<p>Currently, your local index contains 12,403 verified documents across all synced folders.</p>`,
    };

    // 搜索处理
    onMount(() => {
        // Startup check removed (handled by HomePage/Onboarding)
    });

    let isSearching = $state(false); // 左侧骨架屏状态
    let searchDuration = $state(0); // 搜索耗时

    let currentGenerationId = 0; // 用于追踪当前生成轮次

    async function handleSearch(query) {
        if (!query.trim()) return;

        // 无条件取消旧的生成（后端用 generation_id 确保幂等安全）
        console.log("[Spotlight] Cancelling any previous generation...");
        await invoke("cancel_generation");
        isStreaming = false;

        // 递增 generationId，旧生成的 token 将被忽略
        currentGenerationId += 1;
        const thisGenerationId = currentGenerationId;

        // 切换到加载状态
        isLoading = true;
        searchIconName = "search";
        spotlightWidth = 896;
        showResults = true;
        isMainContentVisible = true;

        // Switch to search view
        navigation.setActiveView(VIEW_SEARCH);

        // 阶段1：快速搜索
        const searchStartTime = performance.now();
        isSearching = true; // 开启左侧骨架屏
        searchDuration = 0; // 重置耗时

        insightState = {
            status: "Searching...",
            statusType: "analyzing",
            isThinking: true, // 开启右侧骨架屏
            content: "",
            showCursor: false,
        };
        searchResults = [];

        let searchContext = ""; // 保存上下文供 LLM 生成使用

        try {
            // 调用 rag_search（快速返回）
            console.log("[Spotlight] Invoking rag_search:", query);
            const searchResponse = await invoke("rag_search", {
                query: query,
                filePath: null,
            });

            // 如果在搜索期间用户又发起了新搜索，丢弃本次结果
            if (thisGenerationId !== currentGenerationId) return;

            // 记录搜索耗时
            const durationSec = (performance.now() - searchStartTime) / 1000;
            searchDuration = Number(durationSec.toFixed(1)); // 保留一位小数
            console.log(
                `[Spotlight] rag_search Response (${searchDuration}s):`,
                searchResponse,
            );

            // 立即显示搜索结果，关闭左侧骨架屏
            searchResults = searchResponse.sources.map((s, idx) => ({
                id: idx + 1,
                title: s.file_path.split("/").pop() || s.file_path,
                score: s.score,
                snippet: s.text,
                path: s.context || s.file_path,
                source: s.source,
            }));
            searchContext = searchResponse.context;
            isSearching = false; // 关闭左侧骨架屏
            isLoading = false;

            // 检查是否有搜索结果
            if (searchResults.length === 0 || !searchContext.trim()) {
                // 没有搜索结果，显示提示而不调用 LLM
                insightState = {
                    status: "No Results",
                    statusType: "complete",
                    isThinking: false,
                    content:
                        "在文档库中未找到与您问题相关的内容。请尝试：\n\n• 使用不同的关键词\n• 检查拼写是否正确\n• 使用更通用的搜索词",
                    showCursor: false,
                };
                return;
            }

            // === SQL 直接回答路径：跳过 LLM，直接展示结果 ===
            if (searchResponse.direct_answer) {
                console.log(
                    "[Spotlight] SQL direct answer available, skipping LLM",
                );
                const totalDuration = (
                    (performance.now() - searchStartTime) /
                    1000
                ).toFixed(1);
                insightState = {
                    status: `SQL Query (${totalDuration}s)`,
                    statusType: "complete",
                    isThinking: false,
                    content: searchResponse.direct_answer,
                    showCursor: false,
                };
                return;
            }

            // 阶段2：LLM 生成（非 SQL 路径）
            const generateStartTime = performance.now();
            insightState = {
                status: `Generating Insight...`,
                statusType: "analyzing",
                isThinking: true, // 右侧继续保持骨架屏
                content: "",
                showCursor: false,
            };

            console.log("[Spotlight] Invoking rag_generate...");

            // 准备接收流式输出
            insightState = {
                ...insightState,
                content: "", // Clear content
                // Keep isThinking true until first token
            };

            let isFirstToken = true;
            const unlisten = await listen("llm-token", (event) => {
                // 如果已经不是当前生成轮次，忽略 token
                if (thisGenerationId !== currentGenerationId) return;

                if (isFirstToken) {
                    // 收到第一个 token，关闭骨架屏，显示光标
                    insightState = {
                        ...insightState,
                        isThinking: false,
                        showCursor: true,
                    };
                    isFirstToken = false;
                }
                insightState.content += event.payload;
            });

            isStreaming = true;
            try {
                await invoke("rag_generate", {
                    query: query,
                    context: searchContext,
                });
            } finally {
                unlisten(); // 停止监听
                // 只有当前生成轮次才重置 isStreaming
                if (thisGenerationId === currentGenerationId) {
                    isStreaming = false;
                }
            }

            // 如果已经被新搜索取代，不更新完成状态
            if (thisGenerationId !== currentGenerationId) return;

            const generateDuration = (
                (performance.now() - generateStartTime) /
                1000
            ).toFixed(1);

            // 完成状态
            insightState = {
                ...insightState,
                status: `AI Insight (${generateDuration}s)`,
                statusType: "complete",
                showCursor: false,
            };
        } catch (err) {
            console.error("[Spotlight] Search Failed:", err);
            isLoading = false;

            // Check if error is related to missing models/engine not ready
            const errorStr = err.toString();
            if (
                errorStr.includes("not ready") ||
                errorStr.includes("LLM") ||
                errorStr.includes("Model")
            ) {
                // Redirect to models
                navigation.setSettingsTab("models");
                navigation.setActiveView(VIEW_SETTINGS);
                // Do NOT show error state, just redirect
                return;
            }

            insightState = {
                status: "Error occurred",
                statusType: "error",
                isThinking: false,
                content: `Failed to search: ${err}`,
                showCursor: false,
            };
        }
    }

    // 高亮卡片
    function handleHighlightCard(id) {
        highlightedCardId = id;
        const card = document.getElementById(`card-${id}`);
        if (card) card.classList.add("highlighted");
    }

    function handleUnhighlightCard(id) {
        highlightedCardId = null;
        const card = document.getElementById(`card-${id}`);
        if (card) card.classList.remove("highlighted");
    }

    function handleWindowControl(action) {
        const win = getCurrentWebviewWindow();
        switch (action) {
            case "close":
                isMainContentVisible = false;
                win.hide();
                break;
            case "minimize":
                win.minimize();
                break;
            case "maximize":
                win.toggleMaximize();
                break;
        }
    }

    let containerElement = $state();
    let resizeObserver = $state();
    let hasInitialSizeSet = $state(false);
    let hasInitialPositionSet = $state(false);

    function updateWindowSize() {
        if (!containerElement) return Promise.resolve();

        return new Promise((resolve) => {
            requestAnimationFrame(() => {
                const rect = containerElement.getBoundingClientRect();
                if (rect.width > 0 && rect.height > 0) {
                    getCurrentWebviewWindow()
                        .setSize(new LogicalSize(rect.width, rect.height))
                        .then(() => {
                            hasInitialSizeSet = true;
                            resolve();
                        })
                        .catch((e) => {
                            console.error(e);
                            resolve();
                        });
                } else {
                    resolve();
                }
            });
        });
    }

    function setupResizeObserver() {
        if (!containerElement || resizeObserver) return;

        resizeObserver = new ResizeObserver((entries) => {
            for (const entry of entries) {
                const { width, height } = entry.contentRect;
                if (width > 0 && height > 0 && hasInitialSizeSet) {
                    getCurrentWebviewWindow()
                        .setSize(new LogicalSize(width, height))
                        .catch(console.error);
                }
            }
        });

        resizeObserver.observe(containerElement);
    }

    function cleanupResizeObserver() {
        if (resizeObserver) {
            resizeObserver.disconnect();
            resizeObserver = null;
        }
    }

    async function adjustWindowPosition() {
        const win = getCurrentWebviewWindow();
        const monitor = await currentMonitor();
        if (!monitor) return;

        const screenHeight = monitor.size.height;
        const screenWidth = monitor.size.width;

        const winSize = await win.innerSize();

        const x = Math.round((screenWidth - winSize.width) / 2);
        const y = Math.round(screenHeight * 0.2); // 20% from top

        // [Debug] Log calculated position
        console.log(
            `[Spotlight] Screen: ${screenWidth}x${screenHeight}, Window: ${winSize.width}x${winSize.height}, New Pos: ${x},${y}`,
        );

        await win.setPosition(new PhysicalPosition(x, y));
    }

    $effect(() => {
        // [Debug] 打印状态，帮助调试
        console.log("[Spotlight] $effect run:", {
            visible,
            containerElement,
            hasInitialSizeSet,
        });

        if (visible && containerElement && !hasInitialSizeSet) {
            // [Debug] 只要开启开发者工具 (右键 -> 检查元素)，代码会在这里暂停
            // debugger;

            updateWindowSize().then(async () => {
                console.log(
                    "[Spotlight] updateWindowSize done. Adjusting position...",
                );
                if (!hasInitialPositionSet) {
                    await adjustWindowPosition();
                    hasInitialPositionSet = true;
                }
                setupResizeObserver();
            });
        }

        return () => {
            cleanupResizeObserver();
        };
    });

    $effect(() => {
        spotlightWidth;
        showResults;
        isMainContentVisible; // Track visibility change
        if (hasInitialSizeSet) {
            updateWindowSize();
        }
    });
</script>

{#if visible}
    <div
        bind:this={containerElement}
        class="spotlight-container glass rounded-2xl overflow-hidden fade-up relative flex flex-col"
        style="width: {spotlightWidth}px; height: {isMainContentVisible
            ? 666
            : 'auto'}px; transition: all 0.4s cubic-bezier(0.16, 1, 0.3, 1);"
    >
        <!-- Custom Window Controls -->
        <div class="window-controls">
            <button
                class="control close"
                onclick={() => handleWindowControl("close")}
                aria-label="Close"
            ></button>
            <button
                class="control minimize"
                onclick={() => handleWindowControl("minimize")}
                aria-label="Minimize"
            ></button>
            <button
                class="control maximize"
                onclick={() => handleWindowControl("maximize")}
                aria-label="Maximize"
            ></button>
        </div>

        <SearchHeader
            {isLoading}
            iconName={searchIconName}
            onSearch={handleSearch}
            bind:value={searchQuery}
            disabled={!isModelReady}
        />

        <!-- Main Content Area -->
        <div
            class="flex-1 min-h-0 relative flex flex-col"
            style="background-color: color-mix(in srgb, var(--bg-primary), transparent 14%);"
            class:hidden={!isMainContentVisible}
        >
            {#if navigation.view === VIEW_SEARCH}
                <!-- Fixed Height Search Results Container -->
                <div class="flex-1 overflow-hidden min-h-0 relative h-full">
                    <ResultsPanel
                        visible={showResults}
                        results={searchResults}
                        {insightState}
                        {isSearching}
                        {searchDuration}
                        {highlightedCardId}
                        onHighlightCard={handleHighlightCard}
                        onUnhighlightCard={handleUnhighlightCard}
                    />
                </div>
            {:else if navigation.view === VIEW_DOC_PARSER}
                <DocParser />
            {:else if navigation.view === VIEW_SETTINGS}
                <Settings />
            {:else if navigation.view === VIEW_KNOWLEDGE}
                <Knowledge />
            {/if}
        </div>

        <div class="flex-none">
            <SpotlightFooter
                hasTopBorder={isMainContentVisible}
                onNavigate={() => (isMainContentVisible = true)}
            />
        </div>

        <!-- Window Border Overlay -->
        <div class="window-border"></div>
    </div>
{/if}

<style>
    .window-controls {
        position: absolute;
        top: 28px;
        left: 20px;
        display: flex;
        gap: 6px;
        z-index: 50;
    }

    .control {
        width: 12px;
        height: 12px;
        border-radius: 50%;
        border: none;
        padding: 0;
        cursor: pointer;
        position: relative;
        overflow: hidden;
        transition: transform 0.1s;
    }

    .control:active {
        transform: scale(0.9);
    }

    .close {
        background-color: #ff5f57;
        border: 1px solid #e0443e;
    }
    .minimize {
        background-color: #febc2e;
        border: 1px solid #d89e24;
    }
    .maximize {
        background-color: #28c840;
        border: 1px solid #1aab29;
    }

    /* Hover icons CSS */
    .control:hover::after {
        content: "";
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        font-family: system-ui, sans-serif;
        font-size: 8px;
        color: rgba(0, 0, 0, 0.6);
        font-weight: 700;
        line-height: 1;
    }

    .close:hover::after {
        content: "✕";
        font-size: 7px;
    }
    .minimize:hover::after {
        content: "−";
    }
    .maximize:hover::after {
        content: "+";
        font-size: 10px;
    }

    /* ===================== Window Border Overlay ===================== */
    .window-border {
        position: absolute;
        inset: 0;
        border-radius: 16px; /* rounded-2xl matches 1rem (16px) */
        box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
        pointer-events: none;
        z-index: 9999;
    }
</style>
