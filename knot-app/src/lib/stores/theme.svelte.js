class ThemeState {
    mode = $state('dark'); // 'dark' | 'light'

    constructor() {
        // init is called explicitly or lazily
    }

    init() {
        if (typeof window === 'undefined') return;

        const savedTheme = localStorage.getItem('theme');
        if (savedTheme) {
            this.mode = savedTheme;
        } else {
            const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            this.mode = prefersDark ? 'dark' : 'light';
        }
        this.apply();

        // Sync across windows
        window.addEventListener('storage', (e) => {
            if (e.key === 'theme') {
                this.mode = e.newValue;
                this.apply();
            }
        });
    }

    toggle() {
        this.mode = this.mode === 'dark' ? 'light' : 'dark';
        this.apply();
        localStorage.setItem('theme', this.mode);
    }

    setTheme(newTheme) {
        if (newTheme) {
            this.mode = newTheme;
            this.apply();
            localStorage.setItem('theme', this.mode);
        }
    }

    apply() {
        if (typeof document === 'undefined') return;

        document.documentElement.setAttribute('data-theme', this.mode);

        const isDark = this.mode === 'dark'; // Add other dark themes here if they exist
        if (isDark) {
            document.documentElement.classList.add('dark');
            document.documentElement.classList.remove('light');
        } else {
            document.documentElement.classList.add('light');
            document.documentElement.classList.remove('dark');
        }
    }
}

export const theme = new ThemeState();
