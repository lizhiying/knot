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
    import { invoke } from "@tauri-apps/api/core";
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

    async function handleSearch(query) {
        if (isStreaming) return;
        if (!query.trim()) return;

        // 切换到加载状态
        isLoading = true;
        searchIconName = "search";
        spotlightWidth = 896;
        showResults = true;
        isMainContentVisible = true;

        // Switch to search view
        navigation.setActiveView(VIEW_SEARCH);

        // 显示思考状态
        insightState = {
            status: "Searching & Analyzing...",
            statusType: "analyzing",
            isThinking: true,
            content: "",
            showCursor: false,
        };
        searchResults = [];

        try {
            // Call Backend
            console.log("[Spotlight] Invoking rag_query:", query);
            const response = await invoke("rag_query", { query: query });
            console.log("[Spotlight] Response:", response);

            // Map Sources
            searchResults = response.sources.map((s, idx) => ({
                id: idx + 1,
                title: s.file_path.split("/").pop() || s.file_path, // approximate title
                score: s.score,
                snippet: s.text,
                path: s.context || s.file_path,
            }));

            // 显示结果
            isLoading = false;

            insightState = {
                status: "Synthesizing Insight",
                statusType: "analyzing",
                isThinking: false,
                content: "",
                showCursor: true,
            };

            // 模拟流式输出 (Backend currently returns full string)
            isStreaming = true;
            await streamResponse(response.answer); // Reuse existing simulation for UX
            isStreaming = false;

            // 完成状态
            insightState = {
                ...insightState,
                status: "Insight Complete",
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

    // 流式输出模拟
    async function streamResponse(html) {
        return new Promise((resolve) => {
            let currentContent = "";
            let index = 0;
            const chars = html.split("");

            const interval = setInterval(() => {
                if (index < chars.length) {
                    // 批量添加字符以加快速度
                    const batchSize = 3;
                    for (
                        let i = 0;
                        i < batchSize && index < chars.length;
                        i++
                    ) {
                        currentContent += chars[index];
                        index++;
                    }
                    insightState = { ...insightState, content: currentContent };
                } else {
                    clearInterval(interval);
                    resolve();
                }
            }, 10);
        });
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
                <div
                    class="flex-1 flex items-center justify-center flex-col gap-4"
                >
                    <span class="material-symbols-outlined text-2xl opacity-50"
                        >library_books</span
                    >
                    <p>Knowledge View Coming Soon</p>
                </div>
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
