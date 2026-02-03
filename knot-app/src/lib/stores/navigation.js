// 简单的事件发射器
class NavigationState {
    constructor() {
        this.page = 'home';
        this.listeners = new Set();
    }

    subscribe(callback) {
        this.listeners.add(callback);
        callback(this.page);
        return () => this.listeners.delete(callback);
    }

    navigateTo(newPage) {
        console.log('[NavigationState] Navigating to:', newPage);
        this.page = newPage;
        this.notify();
    }

    notify() {
        this.listeners.forEach(cb => cb(this.page));
    }

    get() {
        return this.page;
    }
}

// 单例导航状态
const navigationState = new NavigationState();

// 页面常量
export const PAGE_HOME = 'home';
export const PAGE_DOC_PARSER = 'doc-parser';

// 创建 Svelte 兼容的 store 接口
export const currentPage = {
    subscribe: (callback) => navigationState.subscribe(callback),
    set: (value) => navigationState.navigateTo(value)
};

// 导航函数
export function navigateTo(page) {
    navigationState.navigateTo(page);
}

// 暴露给全局用于调试
if (typeof window !== 'undefined') {
    window.__navigation__ = navigationState;
}
