<template>
    <section class="h-full min-h-0">
        <div class="grid h-full min-h-0 gap-3 lg:grid-cols-[280px_minmax(0,1fr)]">
            <session-manage-workspace-list
                :workspaces="store.codexWorkspaces"
                :selected-workspace-cwd="store.selectedWorkspaceCwd"
                :loading="store.loading"
                @refresh="handleRefreshWorkspaces"
                @select="handleSelectWorkspace" />
            <session-manage-session-list
                :codex-threads="store.codexThreads"
                :has-workspace="Boolean(store.selectedWorkspaceCwd)"
                :has-more="store.hasMoreCodexThreads"
                :loading="store.loading"
                :loading-more="store.loadingMoreThreads"
                :search-keyword="store.codexThreadKeyword"
                @refresh="handleRefreshSessions"
                @search="handleSearchThreads"
                @load-more="handleLoadMoreThreads"
                @open="handleOpenThread" />
        </div>
    </section>
</template>

<script setup lang="ts">
    import SessionManageSessionList from '@/components/sessionManage/sessionList.vue';
    import SessionManageWorkspaceList from '@/components/sessionManage/workspaceList.vue';
    import { useSessionManageStore } from '@/stores/sessionManage';

    defineOptions({
        name: 'SessionManageView'
    });

    const store = useSessionManageStore();
    let stopTaskUpdates: (() => void) | null = null;

    /**
     * 切换当前 CodeX 工作空间并刷新会话。
     * 流程：把工作空间 cwd 交给 store，store 按该目录读取 CodeX 会话列表。
     * 参数：workspaceCwd 为工作空间绝对路径。
     * 返回：无返回值。
     * 边界：切换失败时由 store 写入提示文案，页面保留原选中态。
     */
    function handleSelectWorkspace(workspaceCwd: string): void {
        void store.selectCodexWorkspace(workspaceCwd);
    }

    /**
     * 刷新工作空间数据。
     * 流程：刷新 CodeX 工作空间，再按当前工作空间刷新右侧会话。
     * 参数：无。
     * 返回：无返回值。
     * 边界：CodeX 不可用时显示空工作空间列表。
     */
    function handleRefreshWorkspaces(): void {
        void store.initSessionManage();
    }

    /**
     * 刷新当前会话列表。
     * 流程：按当前选中的 CodeX 工作空间重新读取会话列表。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有选中工作空间时由 store 返回空会话列表。
     */
    function handleRefreshSessions(): void {
        void store.refreshCodexThreads(undefined, true);
    }

    /**
     * 搜索当前工作空间下的 CodeX 会话。
     * 流程：把搜索框关键词交给 store，store 重置分页后重新读取第一页。
     * 参数：keyword 为会话标题或 thread ID 关键词。
     * 返回：无返回值。
     * 边界：空关键词恢复默认会话列表。
     */
    function handleSearchThreads(keyword: string): void {
        void store.searchCodexThreads(keyword);
    }

    /**
     * 加载更多 CodeX 会话。
     * 流程：委托 store 使用当前搜索条件读取下一页并追加到列表底部。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有更多数据时组件不会触发。
     */
    function handleLoadMoreThreads(): void {
        void store.loadMoreCodexThreads();
    }

    /**
     * 打开 CodeX 会话定位。
     * 流程：委托 store 使用 Tauri deeplink 打开外部 thread。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：无返回值。
     * 边界：未绑定 thread 时按钮已禁用。
     */
    function handleOpenThread(threadId: string): void {
        void store.openExternalThread(threadId);
    }

    onMounted(() => {
        void store.initSessionManage();
        void store.listenTaskUpdates().then((dispose) => {
            stopTaskUpdates = dispose;
        });
    });

    onUnmounted(() => {
        if (stopTaskUpdates) stopTaskUpdates();
    });
</script>
