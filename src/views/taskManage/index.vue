<template>
    <div class="grid h-full min-h-0">
        <section class="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-4 overflow-hidden">
            <div class="grid min-w-0 grid-cols-[minmax(280px,420px)_minmax(0,1fr)_auto] items-center gap-3">
                <div class="grid min-w-[280px] max-w-full gap-2 sm:w-[420px]">
                    <span class="text-[13px] font-medium text-foreground">工作空间</span>
                    <ui-select-root
                        v-model="selectedProjectId"
                        :disabled="!store.projects.length || store.loading">
                        <ui-select-trigger class="w-full">
                            <ui-select-value placeholder="先创建项目并绑定工作空间" />
                        </ui-select-trigger>
                        <ui-select-content>
                            <ui-select-item
                                v-for="project in store.projects"
                                :key="project.id"
                                :value="project.id">
                                <div class="grid min-w-0 gap-1">
                                    <span class="truncate text-[13px]">{{ project.name }}</span>
                                    <span class="truncate text-[11px] text-muted-foreground">{{
                                        project.workspacePath
                                    }}</span>
                                </div>
                            </ui-select-item>
                        </ui-select-content>
                    </ui-select-root>
                </div>
                <span></span>
                <div class="flex shrink-0 items-center gap-2 justify-self-end">
                    <ui-button
                        size="icon-sm"
                        variant="outline"
                        type="button"
                        :disabled="store.loading"
                        @click="handleRefreshWorkspaces">
                        <refresh
                            theme="outline"
                            size="15" />
                        <span class="sr-only">刷新工作空间</span>
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="store.saving"
                        @click="handleOpenProjectDialog">
                        <plus
                            theme="outline"
                            size="16" />
                        <span>新建项目</span>
                    </ui-button>
                </div>
            </div>

            <div class="h-full min-h-0 min-w-0 flex-1 overflow-hidden">
                <ui-page-state
                    v-if="store.loading"
                    :icon="Folder"
                    title="正在读取任务数据"
                    description="正在从 typesass App 本地任务库读取项目、任务和会话记录。"
                    class="h-full" />
                <ui-page-state
                    v-else-if="!store.workspaceDataReady || !store.selectedProject"
                    :icon="taskStateIcon"
                    :title="taskStateTitle"
                    :description="taskStateDescription"
                    class="h-full">
                    <template #action>
                        <ui-button
                            v-if="store.workspaceDataReady && !store.projects.length"
                            type="button"
                            :disabled="store.saving"
                            @click="handleOpenProjectDialog">
                            <folder-plus
                                theme="outline"
                                size="16" />
                            <span>创建项目</span>
                        </ui-button>
                    </template>
                </ui-page-state>
                <task-manage-task-board
                    v-else
                    :tasks="store.tasks"
                    :saving="store.saving"
                    @create="handleOpenTaskDialog"
                    @queue="handleQueueTask"
                    @complete="handleCompleteTask"
                    @open="handleOpenThread" />
            </div>
        </section>

        <ui-dialog v-model:open="projectDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>新建项目</ui-dialog-title>
                    <ui-dialog-description>任务必须归属到项目，项目会绑定一个 CodeX 工作空间。</ui-dialog-description>
                </ui-dialog-header>
                <div class="grid gap-4">
                    <label class="grid gap-2">
                        <span class="text-[13px] text-foreground">项目名称</span>
                        <ui-input
                            v-model="projectForm.name"
                            placeholder="例如：AI Tool 会话管理" />
                    </label>
                    <label class="grid gap-2">
                        <span class="text-[13px] text-foreground">工作空间路径</span>
                        <ui-input
                            v-model="projectForm.workspacePath"
                            placeholder="/Users/lucifer/Documents/source/t/monorepo" />
                    </label>
                </div>
                <ui-dialog-footer>
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="store.saving"
                        @click="projectDialogOpen = false">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="store.saving"
                        @click="handleCreateProject">
                        {{ store.saving ? '创建中' : '创建' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>

        <ui-dialog v-model:open="taskDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>新建任务</ui-dialog-title>
                    <ui-dialog-description
                        >任务创建后停留在已创建状态，点击播放或拖到排队中才会执行。</ui-dialog-description
                    >
                </ui-dialog-header>
                <div class="grid gap-4">
                    <label class="grid gap-2">
                        <span class="text-[13px] text-foreground">任务标题</span>
                        <ui-input
                            v-model="taskForm.title"
                            placeholder="例如：实现会话管理页面" />
                    </label>
                    <label class="grid gap-2">
                        <span class="text-[13px] text-foreground">任务内容</span>
                        <ui-textarea
                            v-model="taskForm.prompt"
                            class="min-h-[160px]"
                            placeholder="写给 CodeX 的执行说明" />
                    </label>
                </div>
                <ui-dialog-footer>
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="store.saving"
                        @click="taskDialogOpen = false">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="store.saving"
                        @click="handleCreateTask">
                        {{ store.saving ? '创建中' : '创建' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
    </div>
</template>

<script setup lang="ts">
    import { Folder, FolderPlus, Plus, Refresh } from '@icon-park/vue-next';

    import TaskManageTaskBoard from '@/components/taskManage/taskBoard.vue';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import { Input as UiInput } from '@/components/ui/input';
    import { PageState as UiPageState } from '@/components/ui/pageState';
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import { Textarea as UiTextarea } from '@/components/ui/textarea';
    import { useSessionManageStore } from '@/stores/sessionManage';

    defineOptions({
        name: 'TaskManageView'
    });

    const store = useSessionManageStore();
    const projectDialogOpen = ref(false);
    const taskDialogOpen = ref(false);
    const projectForm = reactive({
        name: '',
        workspacePath: ''
    });
    const taskForm = reactive({
        title: '',
        prompt: ''
    });
    let stopTaskUpdates: (() => void) | null = null;
    const selectedProjectId = computed({
        get: () => store.selectedProjectId,
        set: (projectId: string) => {
            void store.selectProject(projectId);
        }
    });
    const taskStateIcon = computed(() => (store.projects.length ? Folder : FolderPlus));
    const taskStateTitle = computed(() => {
        if (!store.workspaceDataReady) return '请先打开 typesass App';
        if (!store.projects.length) return '先创建项目并绑定工作空间';
        return '请选择项目';
    });
    const taskStateDescription = computed(() => {
        if (!store.workspaceDataReady) {
            return '任务项目、工作空间和任务卡片都保存在本机客户端。请先打开 typesass App，连接成功后再创建项目或管理任务。';
        }
        if (!store.projects.length) {
            return '任务必须归属到一个项目，项目会绑定本机 CodeX 工作空间。创建项目后，这里才会展示任务看板。';
        }
        return '检测到多个项目，但本地还没有保存上次选择。请选择一个项目后，再创建和管理任务。';
    });

    /**
     * 刷新工作空间数据。
     * 流程：读取本地 SQLite 项目与当前项目任务，并同步 CodeX 工作空间。
     * 参数：无。
     * 返回：无返回值。
     * 边界：后台任务完成后也会自动刷新。
     */
    function handleRefreshWorkspaces(): void {
        void store.initSessionManage();
    }

    /**
     * 打开新建项目弹窗。
     * 流程：优先把当前选中工作空间回填到项目表单，减少重复输入。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有选中工作空间时保持表单原值，由用户手动输入。
     */
    function handleOpenProjectDialog(): void {
        projectForm.workspacePath =
            store.selectedProject?.workspacePath ?? store.selectedWorkspaceCwd ?? projectForm.workspacePath;
        projectDialogOpen.value = true;
    }

    /**
     * 打开当前项目的新建任务弹窗。
     * 流程：确认已经选中项目后打开任务表单，任务创建后会进入已创建列。
     * 参数：无。
     * 返回：无返回值。
     * 边界：未选中项目时不打开弹窗，避免创建无项目归属的任务。
     */
    function handleOpenTaskDialog(): void {
        if (!store.selectedProject) return;
        taskDialogOpen.value = true;
    }

    /**
     * 创建项目并重置表单。
     * 流程：写入项目后关闭弹窗，任务列表切到新项目。
     * 参数：无。
     * 返回：无返回值。
     * 边界：失败时保持弹窗打开。
     */
    function handleCreateProject(): void {
        void store
            .addProject({
                name: projectForm.name,
                workspacePath: projectForm.workspacePath
            })
            .then(() => {
                projectDialogOpen.value = false;
                projectForm.name = '';
                projectForm.workspacePath = '';
            });
    }

    /**
     * 创建当前项目下的任务卡片。
     * 流程：读取当前项目 ID 后写入已创建任务，成功后清空表单。
     * 参数：无。
     * 返回：无返回值。
     * 边界：未选中项目时不提交。
     */
    function handleCreateTask(): void {
        const projectId = store.selectedProject?.id;
        if (!projectId) return;
        void store
            .addTask({
                projectId,
                title: taskForm.title,
                prompt: taskForm.prompt
            })
            .then(() => {
                taskDialogOpen.value = false;
                taskForm.title = '';
                taskForm.prompt = '';
            });
    }

    /**
     * 将任务推入排队并触发自动执行。
     * 流程：委托 store 调用 Tauri command，后台会自动创建 CodeX 会话。
     * 参数：taskId 为目标任务 ID。
     * 返回：无返回值。
     * 边界：不允许从 queued/running/waiting_acceptance/completed 重复排队。
     */
    function handleQueueTask(taskId: string): void {
        void store.queueTask(taskId);
    }

    /**
     * 将待验收任务标记为已完成。
     * 流程：委托 store 完成状态流转并刷新看板。
     * 参数：taskId 为目标任务 ID。
     * 返回：无返回值。
     * 边界：只有待验收任务可完成。
     */
    function handleCompleteTask(taskId: string): void {
        void store.completeTask(taskId);
    }

    /**
     * 定位任务绑定的 CodeX 会话。
     * 流程：使用 deeplink 打开 CodeX Desktop 对应 thread。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：无返回值。
     * 边界：未绑定 thread 的卡片按钮已禁用。
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
