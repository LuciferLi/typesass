<template>
    <section class="h-full min-h-0">
        <div class="grid h-full min-h-0 gap-0 lg:grid-cols-[420px_minmax(0,1fr)]">
            <session-manage-session-list
                :workspaces="store.codexWorkspaces"
                :selected-workspace-cwd="store.selectedWorkspaceCwd"
                :search-keyword="store.codexThreadKeyword"
                :loading="store.loading"
                :codex-threads="store.codexThreads"
                :selected-thread-id="selectedThreadId"
                :sessions="store.sessions"
                :has-workspace="Boolean(store.selectedWorkspaceCwd)"
                :has-more="store.hasMoreCodexThreads"
                :loading-more="store.loadingMoreThreads"
                @refresh="handleRefreshSessions"
                @select-workspace="handleSelectWorkspace"
                @search="handleSearchThreads"
                @load-more="handleLoadMoreThreads"
                @select="handleSelectThread"
                @open="handleOpenThread" />
            <session-manage-session-thread-viewer :selected-thread="selectedThread" />
        </div>
    </section>
</template>

<script setup lang="ts">
    import { toast } from 'vue-sonner';

    import SessionManageSessionList from '@/components/sessionManage/sessionList.vue';
    import SessionManageSessionThreadViewer from '@/components/sessionManage/sessionThreadViewer.vue';
    import { useSessionManageStore } from '@/stores/sessionManage';

    defineOptions({
        name: 'SessionManageView'
    });

    const store = useSessionManageStore();
    const selectedThreadId = ref('');
    let disposeTaskUpdates: (() => void) | null = null;
    let codexThreadStatusRefreshTimer: ReturnType<typeof window.setInterval> | null = null;
    let refreshingCodexThreadStatus = false;

    const selectedThread = computed(() => {
        return store.codexThreads.find((thread) => thread.id === selectedThreadId.value) ?? null;
    });

    /**
     * 弹出会话管理操作失败提示。
     * 流程：优先展示 Error 中的安全错误说明；未知异常使用兜底文案。
     * 参数：title 为短提示标题，error 为捕获异常，fallbackDescription 为兜底说明。
     * 返回：无返回值。
     * 边界：只处理用户主动操作失败，页面加载失败仍由组件状态展示。
     */
    function showSessionOperationError(title: string, error: unknown, fallbackDescription: string): void {
        toast.error(title, {
            description: error instanceof Error ? error.message : fallbackDescription
        });
    }

    /**
     * 切换当前 CodeX 工作空间并刷新会话。
     * 流程：把工作空间 cwd 交给 store，store 按该目录读取 CodeX 会话列表。
     * 参数：workspaceCwd 为工作空间绝对路径。
     * 返回：无返回值。
     * 边界：切换失败时由 store 写入提示文案，页面保留原选中态。
     */
    function handleSelectWorkspace(workspaceCwd: string): void {
        selectedThreadId.value = '';
        void store.selectCodexWorkspace(workspaceCwd).catch((error: unknown) => {
            showSessionOperationError('切换工作空间失败', error, '读取工作空间会话失败。');
        });
    }

    /**
     * 刷新当前会话列表。
     * 流程：按当前选中的 CodeX 工作空间重新读取会话列表。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有选中工作空间时由 store 返回空会话列表。
     */
    function handleRefreshSessions(): void {
        void store
            .refreshCodexThreads(undefined, true)
            .then(() => {
                if (!store.codexThreads.some((thread) => thread.id === selectedThreadId.value)) {
                    selectedThreadId.value = store.codexThreads[0]?.id ?? '';
                }
            })
            .catch((error: unknown) => {
                showSessionOperationError('刷新会话失败', error, '读取 CodeX 会话失败。');
            });
    }

    /**
     * 搜索当前工作空间下的 CodeX 会话。
     * 流程：把搜索框关键词交给 store，store 重置分页后重新读取第一页。
     * 参数：keyword 为会话标题或 thread ID 关键词。
     * 返回：无返回值。
     * 边界：空关键词恢复默认会话列表。
     */
    function handleSearchThreads(keyword: string): void {
        void store
            .searchCodexThreads(keyword)
            .then(() => {
                selectedThreadId.value = store.codexThreads[0]?.id ?? '';
            })
            .catch((error: unknown) => {
                showSessionOperationError('搜索会话失败', error, '读取搜索结果失败。');
            });
    }

    /**
     * 加载更多 CodeX 会话。
     * 流程：委托 store 使用当前搜索条件读取下一页并追加到列表底部。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有更多数据时组件不会触发。
     */
    function handleLoadMoreThreads(): void {
        void store.loadMoreCodexThreads().catch((error: unknown) => {
            showSessionOperationError('加载更多会话失败', error, '读取更多会话失败。');
        });
    }

    /**
     * 切换右侧会话内容。
     * 流程：左侧列表单击只更新当前 thread ID，右侧组件负责取消旧流并加载新内容。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：无返回值。
     */
    function handleSelectThread(threadId: string): void {
        selectedThreadId.value = threadId;
    }

    /**
     * 打开 CodeX 会话定位。
     * 流程：委托 store 调用 HTTP，再由 Rust 打开受校验的外部 thread。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：无返回值。
     * 边界：未绑定 thread 时按钮已禁用。
     */
    function handleOpenThread(threadId: string): void {
        void store.openExternalThread(threadId).catch((error: unknown) => {
            showSessionOperationError('打开 CodeX 会话失败', error, '打开 CodeX 会话失败。');
        });
    }

    /**
     * 静默刷新左侧 CodeX 会话摘要状态。
     * 流程：页面保持打开时定时刷新第一页摘要，让 running/completed 状态驱动列表 loading 图标。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有工作空间、正在追加加载或上一轮未结束时跳过，避免打断用户当前选择和分页操作。
     */
    function refreshCodexThreadStatusSilently(): void {
        if (!store.selectedWorkspaceCwd || store.loadingMoreThreads || refreshingCodexThreadStatus) return;
        refreshingCodexThreadStatus = true;
        void store.refreshCodexThreads(undefined, true).finally(() => {
            refreshingCodexThreadStatus = false;
        });
    }

    /**
     * 启动左侧会话状态静默刷新。
     * 流程：初始化完成后创建固定间隔轮询，组件卸载时统一清理。
     * 参数：无。
     * 返回：无返回值。
     * 边界：重复调用会先清理旧 timer，避免页面重进后多重轮询。
     */
    function startCodexThreadStatusRefresh(): void {
        if (codexThreadStatusRefreshTimer) window.clearInterval(codexThreadStatusRefreshTimer);
        codexThreadStatusRefreshTimer = window.setInterval(refreshCodexThreadStatusSilently, 5_000);
    }

    onMounted(() => {
        void store
            .initTaskManage()
            .then(() => store.refreshCodexThreads(undefined, true))
            .then(() => {
                selectedThreadId.value = store.codexThreads[0]?.id ?? '';
                startCodexThreadStatusRefresh();
            })
            .then(() => store.listenTaskUpdates())
            .then((dispose) => {
                disposeTaskUpdates = dispose;
            })
            .catch((error: unknown) => {
                showSessionOperationError('初始化会话失败', error, '读取会话和任务状态失败。');
            });
    });

    onUnmounted(() => {
        disposeTaskUpdates?.();
        disposeTaskUpdates = null;
        if (codexThreadStatusRefreshTimer) window.clearInterval(codexThreadStatusRefreshTimer);
        codexThreadStatusRefreshTimer = null;
    });
</script>
