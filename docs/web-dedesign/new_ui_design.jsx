import React, { useState, useEffect } from 'react';
import {
    Search,
    FileText,
    Settings,
    Database,
    Cpu,
    Shield,
    Layout,
    CheckCircle2,
    Clock,
    ChevronRight,
    ChevronDown,
    Command,
    X,
    Zap,
    MoreHorizontal,
    FolderOpen,
    FileCode,
    Globe
} from 'lucide-react';

// --- Theme Definitions ---
// Define CSS variables for each theme to allow seamless switching
const THEMES = {
    dark: {
        id: 'dark',
        name: 'Dark Void',
        type: 'dark',
        colors: {
            '--bg-main': '#0f1115',
            '--bg-sidebar': '#15171c',
            '--bg-card': '#1e2025',
            '--bg-card-hover': '#25282e',
            '--bg-input': '#1a1b1e',
            '--border': '#2d3039',
            '--border-light': '#383c47',
            '--text-primary': '#ececec',
            '--text-secondary': '#9ca3af',
            '--text-muted': '#6b7280',
            '--accent': '#10b981', // Emerald green
            '--accent-glow': 'rgba(16, 185, 129, 0.2)',
            '--highlight': '#3b82f6', // Blue for links/highlights
        }
    },
    light: {
        id: 'light',
        name: 'Pure Light',
        type: 'light',
        colors: {
            '--bg-main': '#f8f9fa',
            '--bg-sidebar': '#f1f3f5',
            '--bg-card': '#ffffff',
            '--bg-card-hover': '#f8f9fa',
            '--bg-input': '#ffffff',
            '--border': '#e5e7eb',
            '--border-light': '#d1d5db',
            '--text-primary': '#111827',
            '--text-secondary': '#4b5563',
            '--text-muted': '#9ca3af',
            '--accent': '#059669',
            '--accent-glow': 'rgba(5, 150, 105, 0.1)',
            '--highlight': '#2563eb',
        }
    },
    warm: {
        id: 'warm',
        name: 'Warm Paper',
        type: 'light',
        colors: {
            '--bg-main': '#fdfbf7', // Warm off-white
            '--bg-sidebar': '#f5f2eb',
            '--bg-card': '#ffffff',
            '--bg-card-hover': '#fbfaf8',
            '--bg-input': '#ffffff',
            '--border': '#e6e2d8',
            '--border-light': '#dcd7cc',
            '--text-primary': '#2c2520', // Warm dark brown-grey
            '--text-secondary': '#635850',
            '--text-muted': '#9e948d',
            '--accent': '#d97706', // Amber
            '--accent-glow': 'rgba(217, 119, 6, 0.1)',
            '--highlight': '#ea580c',
        }
    },
    cool: {
        id: 'cool',
        name: 'Cool Breeze',
        type: 'light',
        colors: {
            '--bg-main': '#f0f4f8', // Cool slate tint
            '--bg-sidebar': '#eef2f6',
            '--bg-card': '#ffffff',
            '--bg-card-hover': '#f8fafc',
            '--bg-input': '#ffffff',
            '--border': '#dae1e7',
            '--border-light': '#cbd5e1',
            '--text-primary': '#0f172a', // Slate 900
            '--text-secondary': '#475569',
            '--text-muted': '#94a3b8',
            '--accent': '#0ea5e9', // Sky blue
            '--accent-glow': 'rgba(14, 165, 233, 0.1)',
            '--highlight': '#3b82f6',
        }
    },
    lavender: {
        id: 'lavender',
        name: 'Soft Lavender',
        type: 'light',
        colors: {
            '--bg-main': '#fbfbfc',
            '--bg-sidebar': '#f7f6fa',
            '--bg-card': '#ffffff',
            '--bg-card-hover': '#fdfaff',
            '--bg-input': '#ffffff',
            '--border': '#e9e8f0',
            '--border-light': '#dddce6',
            '--text-primary': '#2e2a36',
            '--text-secondary': '#5d576b',
            '--text-muted': '#9e9ab0',
            '--accent': '#8b5cf6', // Violet
            '--accent-glow': 'rgba(139, 92, 246, 0.1)',
            '--highlight': '#7c3aed',
        }
    }
};

// --- Components ---

const Badge = ({ score, type = 'high' }) => {
    const isHigh = type === 'high';
    return (
        <span className={`text-xs font-mono px-1.5 py-0.5 rounded border ${isHigh
            ? 'text-[var(--accent)] border-[var(--accent)] bg-[var(--accent-glow)]'
            : 'text-[var(--text-muted)] border-[var(--border)]'
            }`}>
            {score}
        </span>
    );
};

const Citation = ({ num }) => (
    <span className="inline-flex items-center justify-center w-4 h-4 text-[10px] rounded-[4px] bg-[var(--highlight)] text-white ml-1 align-top cursor-pointer hover:opacity-80">
        {num}
    </span>
);

const FileCard = ({ title, desc, path, score, icon: Icon, type }) => (
    <div className="group p-4 rounded-xl border border-[var(--border)] bg-[var(--bg-card)] hover:bg-[var(--bg-card-hover)] transition-all cursor-pointer mb-3 shadow-sm hover:shadow-md">
        <div className="flex justify-between items-start mb-2">
            <div className="flex items-center gap-2">
                <div className={`p-1.5 rounded-lg ${type === 'pdf' ? 'bg-blue-500/10 text-blue-500' : type === 'doc' ? 'bg-indigo-500/10 text-indigo-500' : 'bg-orange-500/10 text-orange-500'}`}>
                    <Icon size={16} />
                </div>
                <span className="font-medium text-[var(--text-primary)] text-sm truncate max-w-[180px]">{title}</span>
            </div>
            <Badge score={score} />
        </div>
        <p className="text-xs text-[var(--text-secondary)] line-clamp-2 leading-relaxed mb-3">
            {desc}
        </p>
        <div className="flex items-center text-[10px] text-[var(--text-muted)] gap-2">
            <FolderOpen size={10} />
            <span>{path}</span>
        </div>
    </div>
);

const ViewSearch = () => {
    return (
        <div className="grid grid-cols-12 gap-6 h-full p-6 pt-2">
            {/* Left Column: Evidence */}
            <div className="col-span-5 flex flex-col h-full overflow-hidden">
                <div className="flex justify-between items-center mb-4">
                    <h3 className="text-xs font-bold tracking-wider text-[var(--text-muted)] uppercase">Hybrid Evidence</h3>
                    <span className="text-[10px] px-2 py-0.5 rounded-full bg-[var(--bg-card)] border border-[var(--border)] text-[var(--text-secondary)]">3 results match</span>
                </div>

                <div className="overflow-y-auto pr-2 flex-1 scrollbar-hide">
                    <FileCard
                        title="Knot_Vision_2025.pdf"
                        desc="The foundational pillar of Knot RAG is local-first data processing. By leveraging vector extensions, we..."
                        path="Company > Strategy > High_Level"
                        score="0.98"
                        icon={FileText}
                        type="pdf"
                    />
                    <FileCard
                        title="Security_Audit_Report.docx"
                        desc="User data isolation is maintained via namespace-encrypted embedding databases. No external API..."
                        path="Internal > IT > Compliance"
                        score="0.89"
                        icon={Shield}
                        type="doc"
                    />
                    <FileCard
                        title="Architecture_Diagrams.excalidraw"
                        desc="Spotlight interface serves as the primary entry point for all RAG queries, providing instant visual feedback..."
                        path="Projects > Design > Assets"
                        score="0.76"
                        icon={Layout}
                        type="draw"
                    />
                </div>
            </div>

            {/* Right Column: Insight */}
            <div className="col-span-7 flex flex-col h-full overflow-hidden pl-4 border-l border-[var(--border)]">
                <div className="flex items-center gap-2 mb-6">
                    <div className="w-2 h-2 rounded-full bg-[var(--accent)] animate-pulse"></div>
                    <span className="text-xs font-bold tracking-wider text-[var(--text-muted)] uppercase">Insight Complete</span>
                </div>

                <div className="prose prose-sm max-w-none">
                    <p className="text-[var(--text-primary)] leading-7 mb-6">
                        <strong className="text-[var(--text-primary)]">Knot RAG</strong> represents a paradigm shift in how users interact with their private documents.
                    </p>
                    <p className="text-[var(--text-secondary)] leading-7 mb-6">
                        Unlike traditional search, Knot utilizes a <strong className="text-[var(--text-primary)]">Hybrid Retrieval</strong> system <Citation num={1} /> that merges semantic understanding with precise keyword matching.
                    </p>

                    <h4 className="text-sm font-semibold text-[var(--text-primary)] mb-3">Key technical advantages:</h4>
                    <ul className="space-y-2 mb-6 text-[var(--text-secondary)]">
                        <li className="flex gap-2">
                            <span className="font-semibold text-[var(--text-primary)] min-w-[80px]">Latency:</span>
                            <span>Average response time of 120ms for synthesis.</span>
                        </li>
                        <li className="flex gap-2">
                            <span className="font-semibold text-[var(--text-primary)] min-w-[80px]">Privacy:</span>
                            <span>100% on-device processing using quantized weights <Citation num={2} />.</span>
                        </li>
                        <li className="flex gap-2">
                            <span className="font-semibold text-[var(--text-primary)] min-w-[80px]">Transparency:</span>
                            <span>Every sentence is backed by verified evidence <Citation num={3} />.</span>
                        </li>
                    </ul>

                    <p className="text-[var(--text-secondary)] leading-7">
                        Currently, your local index contains <strong className="text-[var(--text-primary)]">12,403 verified documents</strong> across all synced folders.
                    </p>
                </div>
            </div>
        </div>
    );
};

const ViewParser = () => {
    return (
        <div className="grid grid-cols-12 gap-0 h-full">
            {/* Left: Pipeline Status */}
            <div className="col-span-4 p-6 border-r border-[var(--border)] flex flex-col gap-6">
                <div>
                    <h3 className="text-sm font-semibold text-[var(--text-primary)] mb-4">Parsing Analysis</h3>
                    <div className="p-4 rounded-xl bg-[var(--bg-card)] border border-[var(--border)] shadow-sm mb-4">
                        <div className="flex items-center gap-3 mb-2">
                            <div className="p-1 rounded-full bg-[var(--accent)] text-white">
                                <CheckCircle2 size={14} />
                            </div>
                            <span className="text-sm font-medium text-[var(--text-primary)]">Parse Complete</span>
                        </div>
                        <p className="text-xs text-[var(--text-secondary)] pl-8">Structure extracted successfully.</p>
                    </div>

                    <div className="p-4 rounded-xl bg-[var(--bg-card)] border border-[var(--border)] shadow-sm">
                        <div className="flex items-center gap-2 mb-3 text-[var(--text-primary)] font-medium text-xs">
                            <FileCode size={14} />
                            File Metrics
                        </div>
                        <div className="grid grid-cols-2 gap-y-2 text-xs">
                            <span className="text-[var(--text-muted)]">Format</span>
                            <span className="text-right text-[var(--text-primary)]">MD</span>
                            <span className="text-[var(--text-muted)]">Tokens</span>
                            <span className="text-right text-[var(--text-primary)]">1,204</span>
                            <span className="text-[var(--text-muted)]">Encoding</span>
                            <span className="text-right text-[var(--text-primary)]">UTF-8</span>
                        </div>
                    </div>
                </div>

                <div className="flex-1">
                    <h3 className="text-xs font-bold text-[var(--text-muted)] uppercase mb-4 flex items-center gap-2">
                        <Cpu size={12} /> Processing Pipeline
                    </h3>
                    <div className="relative pl-2 space-y-6">
                        {/* Timeline Line */}
                        <div className="absolute left-[11px] top-2 bottom-2 w-0.5 bg-[var(--border)]"></div>

                        {/* Step 1 */}
                        <div className="relative flex items-start gap-4">
                            <div className="z-10 w-5 h-5 rounded-full bg-[var(--accent)] border-2 border-[var(--bg-main)] flex items-center justify-center">
                                <div className="w-1.5 h-1.5 rounded-full bg-white"></div>
                            </div>
                            <div>
                                <p className="text-sm font-medium text-[var(--text-primary)]">Layout Analysis</p>
                                <p className="text-xs text-[var(--text-muted)]">Ready</p>
                            </div>
                        </div>

                        {/* Step 2 */}
                        <div className="relative flex items-start gap-4">
                            <div className="z-10 w-5 h-5 rounded-full border-2 border-[var(--border-light)] bg-[var(--bg-main)]"></div>
                            <div>
                                <p className="text-sm font-medium text-[var(--text-muted)]">Text Extraction</p>
                                <p className="text-xs text-[var(--text-muted)]">Pending</p>
                            </div>
                        </div>

                        {/* Step 3 */}
                        <div className="relative flex items-start gap-4">
                            <div className="z-10 w-5 h-5 rounded-full bg-[var(--accent)] border-2 border-[var(--bg-main)] flex items-center justify-center">
                                <CheckCircle2 size={12} className="text-white" />
                            </div>
                            <div>
                                <p className="text-sm font-medium text-[var(--text-primary)]">Semantic Indexing</p>
                                <p className="text-xs text-[var(--text-muted)]">Completed</p>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            {/* Right: Tree View */}
            <div className="col-span-8 p-6 bg-[var(--bg-sidebar)]">
                <div className="flex justify-between items-center mb-4">
                    <div className="flex items-center gap-3 p-2 rounded-lg bg-[var(--bg-card)] border border-[var(--border)] shadow-sm">
                        <FileText size={14} className="text-[var(--text-muted)]" />
                        <span className="text-sm font-medium text-[var(--text-primary)]">10网络安全防护.md</span>
                        <span className="text-[10px] px-1.5 rounded bg-[var(--border)] text-[var(--text-secondary)]">MD</span>
                    </div>
                    <button className="bg-[var(--text-primary)] text-[var(--bg-main)] px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-2 hover:opacity-90">
                        <Zap size={12} /> Structure Parse
                    </button>
                </div>

                {/* Tree Visual */}
                <div className="border border-[var(--border)] rounded-xl bg-[var(--bg-card)] h-[400px] overflow-hidden flex flex-col">
                    <div className="flex items-center border-b border-[var(--border)] px-4 py-2 gap-4">
                        <button className="text-xs font-medium text-[var(--text-primary)] flex items-center gap-1">
                            <Layout size={12} /> Structure Tree
                        </button>
                        <button className="text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)]">Render Preview</button>
                        <button className="text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)] ml-auto">JSON</button>
                    </div>

                    <div className="p-6 overflow-y-auto relative">
                        {/* Tree Node Level 0 */}
                        <div className="flex flex-col relative">
                            <div className="flex items-center gap-2 mb-4">
                                <ChevronDown size={14} className="text-[var(--text-muted)]" />
                                <span className="text-sm font-semibold text-[var(--text-primary)]">10网络安全防护</span>
                                <span className="ml-auto text-[10px] text-[var(--accent)] bg-[var(--accent-glow)] px-1.5 rounded">Level 0</span>
                            </div>

                            {/* Vertical Line */}
                            <div className="absolute left-[7px] top-6 bottom-0 w-px bg-[var(--border)]"></div>

                            {/* Tree Node Level 1 */}
                            <div className="pl-6 mb-4 relative">
                                <div className="flex items-center gap-2 mb-2 p-2 rounded-lg bg-[var(--bg-main)] border border-[var(--border)] w-fit">
                                    <ChevronDown size={14} className="text-[var(--text-muted)]" />
                                    <span className="text-sm font-medium text-[var(--text-primary)]">网络安全防护</span>
                                    <span className="ml-4 text-[10px] text-[var(--accent)] bg-[var(--accent-glow)] px-1.5 rounded">Level 1</span>
                                </div>
                                <div className="absolute left-[31px] top-9 bottom-0 w-px bg-[var(--border)]"></div>

                                {/* Tree Node Level 2 */}
                                <div className="pl-8 mt-4">
                                    <div className="flex items-center gap-2 p-2 rounded-lg bg-[var(--bg-main)] border border-[var(--border)] w-fit">
                                        <ChevronRight size={14} className="text-[var(--text-muted)]" />
                                        <span className="text-sm text-[var(--text-primary)]">安全威胁概览</span>
                                        <span className="ml-4 text-[10px] text-[var(--accent)] bg-[var(--accent-glow)] px-1.5 rounded">Level 2</span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
};

const ThemeCard = ({ themeKey, activeTheme, setTheme }) => {
    const theme = THEMES[themeKey];
    const isActive = activeTheme === themeKey;

    // Create a mini preview of the theme colors
    const bgMain = theme.colors['--bg-main'];
    const bgCard = theme.colors['--bg-card'];
    const accent = theme.colors['--accent'];
    const text = theme.colors['--text-primary'];

    return (
        <div
            onClick={() => setTheme(themeKey)}
            className={`relative group cursor-pointer rounded-xl border-2 transition-all duration-200 overflow-hidden ${isActive ? 'border-[var(--accent)]' : 'border-transparent hover:border-[var(--border)]'
                }`}
        >
            <div className="aspect-[1.6] w-full relative" style={{ backgroundColor: bgMain }}>
                {/* Mock Window inside card */}
                <div className="absolute inset-4 rounded-lg shadow-sm flex flex-col overflow-hidden" style={{ backgroundColor: bgCard }}>
                    <div className="h-3 w-full border-b flex items-center px-2 gap-1" style={{ borderColor: theme.colors['--border'] }}>
                        <div className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: text, opacity: 0.2 }}></div>
                    </div>
                    <div className="p-3">
                        <div className="h-2 w-1/2 rounded mb-2" style={{ backgroundColor: theme.colors['--bg-sidebar'] }}></div>
                        <div className="h-2 w-3/4 rounded mb-2" style={{ backgroundColor: theme.colors['--bg-sidebar'] }}></div>
                        <div className="h-2 w-1/4 rounded bg-[var(--accent)]" style={{ backgroundColor: accent }}></div>
                    </div>
                </div>

                {/* Checkmark overlay if active */}
                {isActive && (
                    <div className="absolute bottom-2 right-2 w-5 h-5 rounded-full bg-[var(--accent)] flex items-center justify-center text-white shadow-md">
                        <CheckCircle2 size={12} fill="white" className="text-white" />
                    </div>
                )}
            </div>
            <div className="p-3 bg-[var(--bg-card)] border-t border-[var(--border)]">
                <p className="text-sm font-medium text-[var(--text-primary)]">{theme.name}</p>
                <p className="text-xs text-[var(--text-secondary)] capitalize">{theme.type}</p>
            </div>
        </div>
    );
};

const ViewSettings = ({ currentTheme, setCurrentTheme }) => {
    return (
        <div className="grid grid-cols-12 h-full">
            {/* Settings Sidebar */}
            <div className="col-span-3 border-r border-[var(--border)] bg-[var(--bg-sidebar)] p-4 pt-6">
                <h2 className="text-sm font-bold text-[var(--text-primary)] px-3 mb-6">Settings</h2>
                <div className="space-y-1">
                    <button className="w-full text-left px-3 py-2 rounded-lg text-sm text-[var(--text-secondary)] hover:bg-[var(--bg-card)] transition-colors flex items-center gap-2">
                        <Settings size={16} /> General
                    </button>
                    <button className="w-full text-left px-3 py-2 rounded-lg text-sm font-medium bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm flex items-center gap-2 border border-[var(--border)]">
                        <Layout size={16} /> Appearance
                    </button>
                    <button className="w-full text-left px-3 py-2 rounded-lg text-sm text-[var(--text-secondary)] hover:bg-[var(--bg-card)] transition-colors flex items-center gap-2">
                        <Shield size={16} /> About
                    </button>
                </div>
            </div>

            {/* Settings Content */}
            <div className="col-span-9 p-8 overflow-y-auto">
                <h2 className="text-xl font-semibold text-[var(--text-primary)] mb-8">Appearance</h2>

                <div className="bg-[var(--bg-sidebar)] rounded-2xl p-6 border border-[var(--border)]">
                    <div className="mb-6">
                        <h3 className="text-base font-medium text-[var(--text-primary)] mb-1">Theme Preference</h3>
                        <p className="text-sm text-[var(--text-secondary)]">Choose how the interface looks on your device.</p>
                    </div>

                    <div className="grid grid-cols-3 gap-6">
                        {Object.keys(THEMES).map((key) => (
                            <ThemeCard
                                key={key}
                                themeKey={key}
                                activeTheme={currentTheme}
                                setTheme={setCurrentTheme}
                            />
                        ))}
                    </div>
                </div>
            </div>
        </div>
    );
};


export default function App() {
    const [activeTab, setActiveTab] = useState('search');
    const [themeKey, setThemeKey] = useState('dark');
    const [searchQuery, setSearchQuery] = useState('sdf');

    // Inject CSS variables based on selected theme
    useEffect(() => {
        const theme = THEMES[themeKey];
        const root = document.documentElement;
        Object.entries(theme.colors).forEach(([key, value]) => {
            root.style.setProperty(key, value);
        });
    }, [themeKey]);

    return (
        <div className="flex items-center justify-center min-h-screen bg-gray-900 p-8 font-sans selection:bg-[var(--accent-glow)] selection:text-[var(--accent)]">
            {/* Main Window Frame */}
            <div
                className="w-[1000px] h-[700px] rounded-2xl overflow-hidden shadow-2xl flex flex-col relative transition-colors duration-300"
                style={{ backgroundColor: 'var(--bg-main)' }}
            >

                {/* Top Search Bar / Header (Always Visible) */}
                <div className="h-16 border-b border-[var(--border)] flex items-center px-6 gap-4 shrink-0 bg-[var(--bg-main)]">
                    {/* Window Controls */}
                    <div className="flex gap-2 mr-4">
                        <div className="w-3 h-3 rounded-full bg-red-500/80 hover:bg-red-500 cursor-pointer"></div>
                        <div className="w-3 h-3 rounded-full bg-yellow-500/80 hover:bg-yellow-500 cursor-pointer"></div>
                        <div className="w-3 h-3 rounded-full bg-green-500/80 hover:bg-green-500 cursor-pointer"></div>
                    </div>

                    {/* Search Input */}
                    <div className="flex-1 max-w-xl relative group">
                        <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)] group-focus-within:text-[var(--accent)] transition-colors" size={18} />
                        <input
                            type="text"
                            value={searchQuery}
                            onChange={(e) => setSearchQuery(e.target.value)}
                            className="w-full bg-transparent text-xl font-medium text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none pl-10 h-10"
                            placeholder="Ask anything..."
                        />
                        {/* Blinking Cursor Simulation if needed, but standard input caret works */}
                    </div>
                </div>

                {/* Main Content Area */}
                <div className="flex-1 overflow-hidden relative">
                    {activeTab === 'search' && <ViewSearch />}
                    {activeTab === 'parser' && <ViewParser />}
                    {activeTab === 'settings' && <ViewSettings currentTheme={themeKey} setCurrentTheme={setThemeKey} />}
                </div>

                {/* Bottom Status / Navigation Bar */}
                <div className="h-12 border-t border-[var(--border)] bg-[var(--bg-main)] flex items-center px-6 justify-between text-xs font-medium text-[var(--text-secondary)] shrink-0 select-none">
                    <div className="flex items-center gap-1">
                        <button
                            onClick={() => setActiveTab('search')}
                            className={`px-3 py-1.5 rounded-md transition-all ${activeTab === 'search' ? 'bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm border border-[var(--border)]' : 'hover:bg-[var(--bg-card)] hover:text-[var(--text-primary)]'}`}
                        >
                            Search
                        </button>
                        <button
                            onClick={() => setActiveTab('parser')}
                            className={`px-3 py-1.5 rounded-md transition-all ${activeTab === 'parser' ? 'bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm border border-[var(--border)]' : 'hover:bg-[var(--bg-card)] hover:text-[var(--text-primary)]'}`}
                        >
                            Doc Parser Demo
                        </button>
                        <button className="px-3 py-1.5 rounded-md hover:bg-[var(--bg-card)] hover:text-[var(--text-primary)] transition-all">
                            Knowledges
                        </button>
                        <button
                            onClick={() => setActiveTab('settings')}
                            className={`px-3 py-1.5 rounded-md transition-all ${activeTab === 'settings' ? 'bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm border border-[var(--border)]' : 'hover:bg-[var(--bg-card)] hover:text-[var(--text-primary)]'}`}
                        >
                            Settings
                        </button>
                    </div>

                    <div className="flex items-center gap-6">
                        <div className="flex items-center gap-2">
                            <Database size={12} className="text-[var(--text-muted)]" />
                            <span>12.4k Docs</span>
                        </div>
                        <div className="flex items-center gap-2">
                            <Zap size={12} className="text-[var(--accent)]" fill="currentColor" />
                            <span className="text-[var(--text-primary)]">RAG Active</span>
                        </div>
                        <div className="h-4 w-px bg-[var(--border)]"></div>
                        <div className="flex items-center gap-2">
                            <span className="px-1.5 py-0.5 rounded border border-[var(--border)] bg-[var(--bg-card)] text-[10px] font-mono">↵ Search</span>
                            <span className="px-1.5 py-0.5 rounded border border-[var(--border)] bg-[var(--bg-card)] text-[10px] font-mono">ESC Close</span>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}