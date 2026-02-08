<script>
    import { theme } from "$lib/stores/theme.svelte.js";
    import { onMount } from "svelte";
    import {
        register,
        unregisterAll,
    } from "@tauri-apps/plugin-global-shortcut";
    import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

    import { invoke } from "@tauri-apps/api/core";
    import { open, ask, message } from "@tauri-apps/plugin-dialog";
    import { navigation } from "$lib/stores/navigation.svelte.js";
    import ModelManager from "./ModelManager.svelte";
    import { listen } from "@tauri-apps/api/event"; // Also missing listen used in onMount

    const tabs = [
        { id: "general", label: "General", icon: "settings" },
        { id: "models", label: "Models", icon: "smart_toy" },
        { id: "theme", label: "Appearance", icon: "palette" },
        { id: "about", label: "About", icon: "info" },
    ];

    // Config state
    let dataDir = $state("");
    let isStreamingEnabled = $state(true);
    let indexingStatus = $state("ready");
    let isRecording = $state(false);
    let shortcutKey = $state("");
    let savedShortcut = $state("");
    let logoError = $state(false);

    const THEMES = {
        light: {
            id: "light",
            name: "Light",
            type: "Classic",
            colors: {
                "--bg-main": "#ffffff",
                "--bg-card": "#f4f4f5",
                "--bg-secondary": "#e4e4e7",
                "--border-color": "#d4d4d8",
                "--text-primary": "#18181b",
                "--accent-primary": "#3b82f6",
            },
        },
        dark: {
            id: "dark",
            name: "Dark",
            type: "Classic",
            colors: {
                "--bg-main": "#09090b",
                "--bg-card": "#18181b",
                "--bg-secondary": "#27272a",
                "--border-color": "#3f3f46",
                "--text-primary": "#f4f4f5",
                "--accent-primary": "#3b82f6",
            },
        },
        system: {
            id: "system",
            name: "System",
            type: "Auto",
            colors: {
                "--bg-main": "#71717a",
                "--bg-card": "#52525b",
                "--bg-secondary": "#3f3f46",
                "--border-color": "#27272a",
                "--text-primary": "#fafafa",
                "--accent-primary": "#3b82f6",
            },
        },
    };

    async function selectDataDir() {
        try {
            const selected = await open({
                directory: true,
                multiple: false,
                defaultPath: dataDir || undefined,
            });
            if (selected) {
                await invoke("set_data_dir", { path: selected });
                dataDir = selected;
                await message(
                    "Data directory updated. Indexing will start in background.",
                    { title: "Success", kind: "info" },
                );
            }
        } catch (err) {
            console.error("Failed to select dir:", err);
        }
    }

    async function saveShortcut() {
        if (!shortcutKey) return;
        try {
            await unregisterAll();
            await register(shortcutKey, (event) => {
                if (event.state === "Pressed") {
                    toggleSpotlight();
                }
            });
            localStorage.setItem("knot_global_shortcut", shortcutKey);
            savedShortcut = shortcutKey;
            await message("Global shortcut saved!", {
                title: "Success",
                kind: "info",
            });
        } catch (err) {
            console.error("Failed to save shortcut:", err);
            await message("Failed to save shortcut: " + err, {
                title: "Error",
                kind: "error",
            });
        }
    }

    async function toggleSpotlight() {
        const win = getCurrentWebviewWindow();
        if (await win.isVisible()) {
            await win.hide();
        } else {
            await win.show();
            await win.setFocus();
        }
    }

    async function resetIndex() {
        const confirmed = await ask(
            "Are you sure you want to clear the index? This will delete all indexed data and require a full re-scan.",
            { title: "Clear Index", kind: "warning" },
        );

        if (!confirmed) return;

        try {
            await invoke("reset_index");
            indexingStatus = "Index cleared."; // Update status immediately

            await message(
                "Index cleared successfully. You can now click 'Re-index' to rebuild it.",
                { title: "Success", kind: "info" },
            );
        } catch (err) {
            console.error("Failed to reset index:", err);
            await message("Failed to reset index: " + err, {
                title: "Error",
                kind: "error",
            });
        }
    }

    async function handleReindex() {
        if (!dataDir) return;

        try {
            // Trigger indexing by re-setting the data dir (which calls start_background_indexing)
            await invoke("set_data_dir", { path: dataDir });
            indexingStatus = "starting scan...";

            await message("Re-indexing started in background.", {
                title: "Success",
                kind: "info",
            });
        } catch (err) {
            console.error("Failed to re-index:", err);
            await message("Failed to re-index: " + err, {
                title: "Error",
                kind: "error",
            });
        }
    }

    async function toggleStreaming() {
        try {
            const newState = !isStreamingEnabled;
            // Optimistic update
            isStreamingEnabled = newState;
            await invoke("set_streaming_enabled", { enabled: newState });
        } catch (err) {
            console.error("Failed to toggle streaming:", err);
            // Revert on failure
            isStreamingEnabled = !isStreamingEnabled;
            await message("Failed to update setting: " + err, {
                title: "Error",
                kind: "error",
            });
        }
    }

    function handleKeyDown(e) {
        if (!isRecording) return;
        e.preventDefault();

        const keys = [];
        if (e.metaKey) keys.push("Command");
        if (e.ctrlKey) keys.push("Ctrl");
        if (e.altKey) keys.push("Alt");
        if (e.shiftKey) keys.push("Shift");

        // Don't add modifiers as main key
        if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return;

        let key = e.key.toUpperCase();
        if (key === " ") key = "Space";
        keys.push(key);

        shortcutKey = keys.join("+");
        isRecording = false; // Stop recording after one combination
    }

    onMount(async () => {
        theme.init();

        // Load Config
        try {
            const config = await invoke("get_app_config");
            if (config.data_dir) {
                dataDir = config.data_dir;
            }
            if (config.streaming_enabled !== undefined) {
                isStreamingEnabled = config.streaming_enabled;
            }
        } catch (e) {
            console.error("Failed to load config:", e);
        }

        // Listen for indexing status
        const unlisten = await listen("indexing-status", (event) => {
            indexingStatus = event.payload;
        });

        // Load saved shortcut
        const saved = localStorage.getItem("knot_global_shortcut");
        if (saved) {
            savedShortcut = saved;
            shortcutKey = saved;
            // Register it
            try {
                await unregisterAll(); // Clean slate
                await register(saved, (event) => {
                    if (event.state === "Pressed") {
                        toggleSpotlight();
                    }
                });
            } catch (e) {
                console.error("Failed to restore shortcut:", e);
            }
        }

        return () => {
            unlisten();
        };
    });
</script>

<div class="h-full flex overflow-hidden text-[var(--text-primary)]">
    <!-- Sidebar -->
    <div
        class="w-[220px] border-r border-[var(--border-color)] flex flex-col shrink-0"
    >
        <div class="mt-4 flex items-center px-4 text-sm font-semibold">
            <span>Settings</span>
        </div>
        <div class="flex flex-col gap-1 p-4">
            {#each tabs as tab}
                <button
                    class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors {navigation.settingsTab ===
                    tab.id
                        ? 'bg-[var(--bg-card)] text-[var(--accent-primary)] shadow-sm'
                        : 'text-[var(--text-secondary)] hover:bg-[var(--bg-card-hover)] hover:text-[var(--text-primary)]'}"
                    onclick={() => (navigation.settingsTab = tab.id)}
                >
                    <span class="material-symbols-outlined text-[18px]"
                        >{tab.icon}</span
                    >
                    {tab.label}
                </button>
            {/each}
        </div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto bg-[var(--bg-primary)]">
        <div class="mx-auto p-6">
            {#if navigation.settingsTab === "theme"}
                <!-- ... theme content ... -->
                <div class="mb-8">
                    <h3 class="text-base font-semibold mb-4 pb-1">
                        Appearance
                    </h3>

                    <div
                        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6"
                    >
                        <div class="flex items-center justify-between mb-6">
                            <div>
                                <h3
                                    class="font-medium text-sm text-[var(--text-primary)]"
                                >
                                    Theme Preference
                                </h3>
                                <p
                                    class="text-[var(--text-secondary)] text-xs mt-1"
                                >
                                    Choose how the interface looks on your
                                    device.
                                </p>
                            </div>
                        </div>

                        <div class="grid grid-cols-3 gap-4">
                            {#each Object.entries(THEMES) as [key, t]}
                                <button
                                    class="relative group rounded-xl border-2 transition-all text-left overflow-hidden {theme.mode ===
                                    t.id
                                        ? 'border-[var(--accent-primary)]'
                                        : 'border-transparent hover:border-[var(--border-color)]'}"
                                    onclick={() => theme.setTheme(t.id)}
                                >
                                    <div
                                        class="aspect-[1.6] w-full relative"
                                        style="background-color: {t.colors[
                                            '--bg-main'
                                        ]};"
                                    >
                                        <!-- Mock Window -->
                                        <div
                                            class="absolute inset-4 rounded-lg shadow-sm flex flex-col overflow-hidden"
                                            style="background-color: {t.colors[
                                                '--bg-card'
                                            ]};"
                                        >
                                            <div
                                                class="h-3 w-full border-b flex items-center px-2 gap-1"
                                                style="border-color: {t.colors[
                                                    '--border-color'
                                                ]}"
                                            >
                                                <div
                                                    class="w-1.5 h-1.5 rounded-full opacity-20"
                                                    style="background-color: {t
                                                        .colors[
                                                        '--text-primary'
                                                    ]}"
                                                ></div>
                                            </div>
                                            <div class="p-3">
                                                <div
                                                    class="h-2 w-1/2 rounded mb-2 opacity-50"
                                                    style="background-color: {t
                                                        .colors[
                                                        '--bg-secondary'
                                                    ]}"
                                                ></div>
                                                <div
                                                    class="h-2 w-3/4 rounded mb-2 opacity-50"
                                                    style="background-color: {t
                                                        .colors[
                                                        '--bg-secondary'
                                                    ]}"
                                                ></div>
                                                <div
                                                    class="h-2 w-1/4 rounded"
                                                    style="background-color: {t
                                                        .colors[
                                                        '--accent-primary'
                                                    ]}"
                                                ></div>
                                            </div>
                                        </div>

                                        <!-- Checkmark overlay if active -->
                                        {#if theme.mode === t.id}
                                            <div
                                                class="absolute bottom-2 right-2 w-5 h-5 rounded-full bg-[var(--accent-primary)] flex items-center justify-center text-white shadow-md z-10"
                                            >
                                                <span
                                                    class="material-symbols-outlined text-[12px] font-bold"
                                                    style="font-size: 12px;"
                                                    >check</span
                                                >
                                            </div>
                                        {/if}
                                    </div>
                                    <div
                                        class="p-3 bg-[var(--bg-card)] border-t border-[var(--border-color)]"
                                    >
                                        <p
                                            class="text-sm font-medium text-[var(--text-primary)]"
                                        >
                                            {t.name}
                                        </p>
                                        <p
                                            class="text-xs text-[var(--text-secondary)] capitalize"
                                        >
                                            {t.type}
                                        </p>
                                    </div>
                                </button>
                            {/each}
                        </div>
                    </div>
                </div>
            {:else if navigation.settingsTab === "models"}
                <ModelManager />
            {:else if navigation.settingsTab === "general"}
                <div class="mb-8">
                    <h3 class="text-base font-semibold mb-4 pb-1">General</h3>

                    <!-- Document Directory Section -->
                    <div
                        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6 mb-6"
                    >
                        <div class="flex items-start justify-between">
                            <div>
                                <h3
                                    class="font-medium text-sm text-[var(--text-primary)]"
                                >
                                    Document Directory
                                </h3>
                                <p
                                    class="text-[var(--text-secondary)] text-xs mt-1"
                                >
                                    Select the folder containing your markdown
                                    notes/documents.
                                </p>
                            </div>
                        </div>
                        <div class="mt-4 flex items-center gap-3">
                            <input
                                type="text"
                                class="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-card)] border border-[var(--border-color)] text-sm text-[var(--text-primary)] focus:outline-none opacity-70 cursor-not-allowed"
                                value={dataDir}
                                readonly
                                placeholder="No directory selected"
                            />
                            <button
                                class="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-[var(--accent-primary)] text-white hover:brightness-110 shadow-sm"
                                onclick={selectDataDir}
                            >
                                Change
                            </button>
                        </div>
                        {#if indexingStatus}
                            <div class="mt-2 text-xs flex items-center gap-2">
                                <span
                                    class="w-1.5 h-1.5 rounded-full {indexingStatus ===
                                    'ready'
                                        ? 'bg-green-500'
                                        : 'bg-yellow-500 animate-pulse'}"
                                ></span>
                                <span
                                    class="text-[var(--text-secondary)] capitalize"
                                    >{indexingStatus}</span
                                >
                            </div>
                        {/if}
                    </div>

                    <!-- Streaming Preference -->
                    <div
                        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6 mb-6"
                    >
                        <div class="flex items-center justify-between">
                            <div>
                                <h3
                                    class="font-medium text-sm text-[var(--text-primary)]"
                                >
                                    Response Streaming
                                </h3>
                                <p
                                    class="text-[var(--text-secondary)] text-xs mt-1"
                                >
                                    Typewriter effect for AI responses. Disable
                                    if you prefer seeing the full answer at
                                    once.
                                </p>
                            </div>
                            <button
                                class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors {isStreamingEnabled
                                    ? 'bg-[var(--accent-primary)]'
                                    : 'bg-[var(--bg-card)] border border-[var(--border-color)]'}"
                                onclick={toggleStreaming}
                            >
                                <span
                                    class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform {isStreamingEnabled
                                        ? 'translate-x-6'
                                        : 'translate-x-1'}"
                                ></span>
                            </button>
                        </div>
                    </div>

                    <!-- Index Management Section -->
                    <div
                        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6 mb-6"
                    >
                        <div class="flex items-start justify-between">
                            <div>
                                <h3
                                    class="font-medium text-sm text-[var(--text-primary)]"
                                >
                                    Index Management
                                </h3>
                                <p
                                    class="text-[var(--text-secondary)] text-xs mt-1"
                                >
                                    Clear the index if you encounter issues or
                                    want to force a full re-scan.
                                </p>
                            </div>
                        </div>
                        <div class="mt-4 flex gap-3">
                            <button
                                class="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-red-500/10 text-red-500 hover:bg-red-500/20 shadow-sm border border-red-500/20"
                                onclick={resetIndex}
                            >
                                Clear Index
                            </button>
                            <button
                                class="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-[var(--bg-card)] border border-[var(--border-color)] text-[var(--text-primary)] hover:bg-[var(--bg-card-hover)] shadow-sm disabled:opacity-50 disabled:cursor-not-allowed"
                                onclick={handleReindex}
                                disabled={!dataDir}
                            >
                                Re-index
                            </button>
                        </div>
                    </div>

                    <div
                        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6"
                    >
                        <div class="flex items-start justify-between">
                            <div>
                                <h3
                                    class="font-medium text-sm text-[var(--text-primary)]"
                                >
                                    Global Shortcut
                                </h3>
                                <p
                                    class="text-[var(--text-secondary)] text-xs mt-1"
                                >
                                    Shortcut to toggle the main window
                                    visibility.
                                </p>
                            </div>
                        </div>

                        <div class="mt-4 flex items-center gap-3">
                            <div class="relative">
                                <input
                                    type="text"
                                    class="w-48 px-3 py-2 rounded-lg bg-[var(--bg-card)] border {isRecording
                                        ? 'border-[var(--accent-primary)] ring-1 ring-[var(--accent-primary)]'
                                        : 'border-[var(--border-color)]'} text-sm text-[var(--text-primary)] focus:outline-none"
                                    value={shortcutKey}
                                    readonly
                                    placeholder="Click to record..."
                                    onclick={() => {
                                        isRecording = true;
                                        shortcutKey = "";
                                    }}
                                    onkeydown={handleKeyDown}
                                    onblur={() => (isRecording = false)}
                                />
                                {#if isRecording}
                                    <span
                                        class="absolute right-3 top-1/2 -translate-y-1/2 flex h-2 w-2"
                                    >
                                        <span
                                            class="animate-ping absolute inline-flex h-full w-full rounded-full bg-[var(--accent-primary)] opacity-75"
                                        ></span>
                                        <span
                                            class="relative inline-flex rounded-full h-2 w-2 bg-[var(--accent-primary)]"
                                        ></span>
                                    </span>
                                {/if}
                            </div>

                            <button
                                class="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-[var(--bg-card)] border border-[var(--border-color)] hover:bg-[var(--bg-card-hover)] text-[var(--text-primary)] disabled:opacity-50"
                                onclick={saveShortcut}
                                disabled={!shortcutKey ||
                                    shortcutKey === savedShortcut}
                            >
                                {shortcutKey === savedShortcut
                                    ? "Saved"
                                    : "Save"}
                            </button>
                        </div>
                        {#if savedShortcut}
                            <p
                                class="text-[10px] text-[var(--text-muted)] mt-2"
                            >
                                Current active shortcut: <span
                                    class="font-mono text-[var(--accent-primary)]"
                                    >{savedShortcut}</span
                                >
                            </p>
                        {/if}
                    </div>
                </div>
            {:else if navigation.settingsTab === "about"}
                <div class="mb-8">
                    <h3 class="text-base font-semibold mb-4 pb-1">About</h3>
                    <div
                        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6 flex flex-col items-center justify-center text-center py-10"
                    >
                        <div
                            class="w-16 h-16 rounded-xl bg-[var(--bg-card)] border border-[var(--border-color)] flex items-center justify-center mb-4 shadow-sm"
                        >
                            {#if !logoError}
                                <img
                                    src="/icons/128x128.png"
                                    alt="Logo"
                                    class="w-10 h-10 object-contain opacity-80"
                                    onerror={() => (logoError = true)}
                                />
                            {:else}
                                <span class="material-symbols-outlined text-3xl"
                                    >apps</span
                                >
                            {/if}
                        </div>
                        <h4
                            class="text-lg font-semibold text-[var(--text-primary)] mb-1"
                        >
                            Knot
                        </h4>
                        <p class="text-xs text-[var(--text-secondary)]">
                            Version 0.1.0 (Beta)
                        </p>
                    </div>
                </div>
            {/if}
        </div>
    </div>
</div>

<style>
    /* Styling for scrollbars inside the content area */
    ::-webkit-scrollbar {
        width: 8px;
        height: 8px;
    }

    ::-webkit-scrollbar-track {
        background: transparent;
    }

    ::-webkit-scrollbar-thumb {
        background: #27272a;
        border-radius: 4px;
    }

    ::-webkit-scrollbar-thumb:hover {
        background: #3f3f46;
    }
</style>
