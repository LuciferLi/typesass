<template>
    <aside class="flex min-h-0 flex-col rounded-lg border border-border bg-card">
        <div class="border-b border-border p-4">
            <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                    <div class="text-[14px] font-medium text-foreground">工作空间</div>
                </div>
                <div class="flex items-center gap-1">
                    <ui-button
                        size="icon-sm"
                        variant="outline"
                        type="button"
                        :disabled="saving"
                        @click="emit('refresh')">
                        <refresh
                            theme="outline"
                            size="15" />
                        <span class="sr-only">刷新工作空间</span>
                    </ui-button>
                    <ui-button
                        size="icon-sm"
                        variant="outline"
                        type="button"
                        :disabled="saving"
                        @click="emit('create')">
                        <plus
                            theme="outline"
                            size="15" />
                        <span class="sr-only">新建项目</span>
                    </ui-button>
                </div>
            </div>
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto p-2">
            <button
                v-for="project in projects"
                :key="project.id"
                type="button"
                :class="[
                    'mb-1 grid w-full gap-1 rounded-md px-3 py-2 text-left transition-colors hover:bg-secondary',
                    project.id === selectedProjectId ? 'bg-secondary text-foreground' : 'text-muted-foreground'
                ]"
                @click="emit('select', project.id)">
                <span class="flex min-w-0 items-center gap-2 text-[13px] font-medium">
                    <folder-open
                        theme="outline"
                        size="15" />
                    <span class="truncate">{{ project.name }}</span>
                </span>
                <span class="truncate pl-6 text-[11px] text-muted-foreground">{{ project.workspacePath }}</span>
                <span class="pl-6 text-[11px] text-muted-foreground">
                    {{ project.taskCount }} 个任务 / {{ project.sessionCount }} 个会话
                </span>
            </button>
            <button
                v-for="workspace in codexWorkspaces"
                :key="workspace.cwd"
                type="button"
                :class="[
                    'mb-1 grid w-full gap-1 rounded-md px-3 py-2 text-left transition-colors hover:bg-secondary',
                    workspace.cwd === selectedWorkspacePath ? 'bg-secondary text-foreground' : 'text-muted-foreground'
                ]"
                @click="emit('selectWorkspace', workspace.cwd)">
                <span class="flex min-w-0 items-center gap-2 text-[13px] font-medium">
                    <folder-open
                        theme="outline"
                        size="15" />
                    <span class="truncate">{{ workspace.title }}</span>
                </span>
                <span class="truncate pl-6 text-[11px] text-muted-foreground">{{ workspace.cwd }}</span>
                <span class="pl-6 text-[11px] text-muted-foreground">{{ workspace.threadCount }} 个 CodeX 会话</span>
            </button>
            <div
                v-if="!projects.length && !codexWorkspaces.length"
                class="grid place-items-center px-4 py-10 text-center text-[13px] text-muted-foreground">
                暂无工作空间
            </div>
        </div>
    </aside>
</template>

<script setup lang="ts">
    import { FolderOpen, Plus, Refresh } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import type { CodexWorkspaceModel, SessionProjectModel } from '@/model/sessionManage';

    defineOptions({
        name: 'SessionManageProjectList'
    });

    defineProps<{
        // 本地项目列表，用于左侧工作空间选择。
        projects: SessionProjectModel[];
        // CodeX 本地状态库读取到的工作空间列表，用于会话管理直接展示。
        codexWorkspaces?: CodexWorkspaceModel[];
        // 当前选中项目 ID。
        selectedProjectId: string;
        // 当前选中的 CodeX 工作空间路径。
        selectedWorkspacePath?: string;
        // 是否正在保存，用于禁用新建按钮。
        saving: boolean;
    }>();

    const emit = defineEmits<{
        // 触发创建项目弹窗。
        create: [];
        // 刷新工作空间列表。
        refresh: [];
        // 切换当前项目。
        select: [projectId: string];
        // 切换当前 CodeX 工作空间。
        selectWorkspace: [workspacePath: string];
    }>();
</script>
