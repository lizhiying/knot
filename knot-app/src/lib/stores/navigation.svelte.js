// Svelte 5 响应式状态 - 使用 class 和 getter/setter 来确保响应式追踪

// 页面常量
export const PAGE_HOME = 'home';
export const PAGE_DOC_PARSER = 'doc-parser';

// Shared Workspace Views
export const VIEW_SEARCH = 'search';
export const VIEW_DOC_PARSER = 'doc-parser';
export const VIEW_KNOWLEDGE = 'knowledge';
export const VIEW_SETTINGS = 'settings';

// 使用 class 包装响应式状态
class NavigationState {
    current = $state(PAGE_HOME);
    activeView = $state(VIEW_SEARCH);

    get page() {
        return this.current;
    }

    get view() {
        return this.activeView;
    }

    navigateTo(page) {
        console.log('[Navigation] Setting page to:', page);
        this.current = page;
    }

    setActiveView(view) {
        console.log('[Navigation] Setting active view to:', view);
        this.activeView = view;
    }
}

// 创建单例实例
export const navigation = new NavigationState();

// 便捷函数
export function navigateTo(page) {
    navigation.navigateTo(page);
}

export function setActiveView(view) {
    navigation.setActiveView(view);
}

// 便捷函数获取当前页面
export function getCurrentPage() {
    return navigation.current;
}
