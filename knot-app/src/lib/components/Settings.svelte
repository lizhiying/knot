<script>
    import { theme } from "$lib/stores/theme.svelte.js";
    import { onMount } from "svelte";
    import {
        register,
        unregisterAll,
    } from "@tauri-apps/plugin-global-shortcut";
    import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

    let activeTab = $state("general"); // Default to general for testing

    // Global Shortcut State
    let shortcutKey = $state("");
    let isRecording = $state(false);
    let savedShortcut = $state("");
    let logoError = $state(false);

    // ... existing tabs const ...
    const tabs = [
        { id: "general", label: "General", icon: "settings" },
        { id: "theme", label: "Appearance", icon: "palette" },
        { id: "about", label: "About", icon: "info" },
    ];

    const THEMES = {
        dark: {
            id: "dark",
            name: "Dark Void",
            type: "dark",
            colors: {
                "--bg-main": "#0f1115",
                "--bg-secondary": "#15171c",
                "--bg-card": "#1e2025",
                "--border-color": "#2d3039",
                "--text-primary": "#ececec",
                "--accent-primary": "#10b981",
            },
        },
        light: {
            id: "light",
            name: "Pure Light",
            type: "light",
            colors: {
                "--bg-main": "#f8f9fa",
                "--bg-secondary": "#f1f3f5",
                "--bg-card": "#ffffff",
                "--border-color": "#e5e7eb",
                "--text-primary": "#111827",
                "--accent-primary": "#059669",
            },
        },
        warm: {
            id: "warm",
            name: "Warm Paper",
            type: "light",
            colors: {
                "--bg-main": "#fdfbf7",
                "--bg-secondary": "#f5f2eb",
                "--bg-card": "#ffffff",
                "--border-color": "#e6e2d8",
                "--text-primary": "#2c2520",
                "--accent-primary": "#d97706",
            },
        },
        cool: {
            id: "cool",
            name: "Cool Breeze",
            type: "light",
            colors: {
                "--bg-main": "#f0f4f8",
                "--bg-secondary": "#eef2f6",
                "--bg-card": "#ffffff",
                "--border-color": "#dae1e7",
                "--text-primary": "#0f172a",
                "--accent-primary": "#0ea5e9",
            },
        },
        lavender: {
            id: "lavender",
            name: "Soft Lavender",
            type: "light",
            colors: {
                "--bg-main": "#fbfbfc",
                "--bg-secondary": "#f7f6fa",
                "--bg-card": "#ffffff",
                "--border-color": "#e9e8f0",
                "--text-primary": "#2e2a36",
                "--accent-primary": "#8b5cf6",
            },
        },
    };

    async function toggleSpotlight() {
        const win = getCurrentWebviewWindow();
        const isVisible = await win.isVisible();
        if (isVisible) {
            await win.hide();
        } else {
            await win.show();
            await win.setFocus();
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
            savedShortcut = shortcutKey;
            localStorage.setItem("knot_global_shortcut", shortcutKey);
            console.log("Shortcut registered:", shortcutKey);
        } catch (err) {
            console.error("Failed to register shortcut:", err);
            // alert("Failed to register shortcut: " + err);
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
                    class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors {activeTab ===
                    tab.id
                        ? 'bg-[var(--bg-card)] text-[var(--accent-primary)] shadow-sm'
                        : 'text-[var(--text-secondary)] hover:bg-[var(--bg-card-hover)] hover:text-[var(--text-primary)]'}"
                    onclick={() => (activeTab = tab.id)}
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
            {#if activeTab === "theme"}
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
            {:else if activeTab === "general"}
                <div class="mb-8">
                    <h3 class="text-base font-semibold mb-4 pb-1">General</h3>

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
            {:else if activeTab === "about"}
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
