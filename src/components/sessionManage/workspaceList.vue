<template>
    <aside class="flex min-h-0 flex-col rounded-lg border border-border bg-card">
        <div class="border-b border-border p-3">
            <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                    <div class="text-[14px] font-medium text-foreground">工作空间</div>
                </div>
                <ui-button
                    size="icon-sm"
                    variant="outline"
                    type="button"
                    :disabled="loading"
                    @click="emit('refresh')">
                    <refresh
                        theme="outline"
                        size="15" />
                    <span class="sr-only">刷新工作空间</span>
                </ui-button>
            </div>
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto p-2">
            <ui-item-group>
                <ui-item
                    v-for="workspace in workspaces"
                    :key="workspace.cwd"
                    role="button"
                    tabindex="0"
                    :variant="workspace.cwd === selectedWorkspaceCwd ? 'muted' : 'default'"
                    class="cursor-pointer hover:bg-secondary"
                    @click="handleSelectWorkspace(workspace.cwd)"
                    @keydown.enter.prevent="handleSelectWorkspace(workspace.cwd)"
                    @keydown.space.prevent="handleSelectWorkspace(workspace.cwd)">
                    <ui-item-media variant="icon">
                        <folder-open
                            theme="outline"
                            size="15" />
                    </ui-item-media>
                    <ui-item-content>
                        <div class="flex min-w-0 items-center gap-2">
                            <ui-item-title class="min-w-0 flex-1">{{ workspace.title || workspace.cwd }}</ui-item-title>
                            <span
                                class="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background"
                                role="button"
                                tabindex="0"
                                title="复制工作空间路径"
                                aria-label="复制工作空间路径"
                                @click.stop="handleCopyWorkspacePath(workspace.cwd)"
                                @keydown.enter.stop.prevent="handleCopyWorkspacePath(workspace.cwd)"
                                @keydown.space.stop.prevent="handleCopyWorkspacePath(workspace.cwd)">
                                <copy
                                    theme="outline"
                                    size="14" />
                            </span>
                        </div>
                        <ui-item-footer>{{ workspace.threadCount }} 个会话</ui-item-footer>
                    </ui-item-content>
                </ui-item>
            </ui-item-group>
            <div
                v-if="!workspaces.length"
                class="grid place-items-center px-4 py-10 text-center text-[13px] text-muted-foreground">
                暂无工作空间
            </div>
        </div>
    </aside>
</template>

<script setup lang="ts">
    import { Copy, FolderOpen, Refresh } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import {
        Item as UiItem,
        ItemContent as UiItemContent,
        ItemFooter as UiItemFooter,
        ItemGroup as UiItemGroup,
        ItemMedia as UiItemMedia,
        ItemTitle as UiItemTitle
    } from '@/components/ui/item';
    import type { CodexWorkspaceModel } from '@/model/sessionManage';

    defineOptions({
        name: 'SessionManageWorkspaceList'
    });

    defineProps<{
        // CodeX 最近工作空间列表，用于左侧选择。
        workspaces: CodexWorkspaceModel[];
        // 当前选中的工作空间路径。
        selectedWorkspaceCwd: string;
        // 是否正在刷新数据。
        loading: boolean;
    }>();

    const emit = defineEmits<{
        // 刷新 CodeX 工作空间列表。
        refresh: [];
        // 选择工作空间并刷新右侧会话列表。
        select: [workspaceCwd: string];
    }>();

    /**
     * 选择工作空间并刷新右侧会话。
     * 流程：把点击或键盘触发得到的工作空间路径向父组件抛出。
     * 参数：workspaceCwd 为 CodeX 工作空间绝对路径。
     * 返回：无返回值。
     * 边界：空路径由父级 Store 自行兜底处理。
     */
    function handleSelectWorkspace(workspaceCwd: string): void {
        emit('select', workspaceCwd);
    }

    /**
     * 复制工作空间路径。
     * 流程：优先使用浏览器剪贴板 API；不可用时创建临时 textarea 并执行复制命令。
     * 参数：workspaceCwd 为需要复制的工作空间绝对路径。
     * 返回：无返回值。
     * 边界：剪贴板权限不可用时静默失败，不影响工作空间选择。
     */
    function handleCopyWorkspacePath(workspaceCwd: string): void {
        void copyTextToClipboard(workspaceCwd);
    }

    /**
     * 写入文本到系统剪贴板。
     * 流程：先尝试标准 Clipboard API，失败或不可用时使用选区复制兜底。
     * 参数：text 为需要复制的文本。
     * 返回：复制完成 Promise。
     * 边界：无 DOM 环境或浏览器禁止写入时直接返回，不向用户抛出异常。
     */
    async function copyTextToClipboard(text: string): Promise<void> {
        try {
            if (navigator.clipboard?.writeText) {
                await navigator.clipboard.writeText(text);
                return;
            }
        } catch {
            // 继续走 textarea 兜底。
        }
        if (typeof document === 'undefined') return;
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.setAttribute('readonly', 'true');
        textarea.className = 'fixed -left-[9999px] top-0 opacity-0';
        document.body.appendChild(textarea);
        textarea.select();
        try {
            document.execCommand('copy');
        } catch {
            // 当前环境禁止复制时静默失败，避免影响工作空间选择。
        } finally {
            document.body.removeChild(textarea);
        }
    }
</script>
