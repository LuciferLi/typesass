<template>
    <div class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden">
        <section class="flex h-full min-h-0 min-w-0 flex-col gap-4 overflow-hidden">
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
                                :key="JSON.stringify([project.id, project.name, project.workspacePath])"
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
                        size="icon-sm"
                        variant="outline"
                        type="button"
                        :disabled="store.saving || !store.selectedProject"
                        title="编辑当前项目"
                        @click="handleOpenEditProjectDialog">
                        <edit
                            theme="outline"
                            size="15" />
                        <span class="sr-only">编辑当前项目</span>
                    </ui-button>
                    <ui-button
                        size="icon-sm"
                        variant="outline"
                        type="button"
                        :disabled="store.saving || !store.selectedProject"
                        title="删除当前空项目"
                        @click="deleteProjectDialogOpen = true">
                        <delete
                            theme="outline"
                            size="15" />
                        <span class="sr-only">删除当前空项目</span>
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

            <p
                v-if="store.message"
                class="text-[12px] leading-5 text-muted-foreground"
                role="status">
                {{ store.message }}
            </p>

            <div class="min-h-0 min-w-0 flex-1 overflow-hidden">
                <ui-page-state
                    v-if="store.loading"
                    :icon="Folder"
                    title="正在读取任务数据"
                    description="正在从 CodexMan App 本地任务库读取项目、任务和会话记录。"
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
                    @detail="handleOpenTaskDetailSheet"
                    @edit="handleOpenEditTaskDialog"
                    @delete="handleOpenDeleteTaskDialog"
                    @queue="handleQueueTask"
                    @complete="handleCompleteTask"
                    @open="handleOpenThread" />
            </div>
        </section>

        <task-manage-task-detail-sheet
            v-model:open="taskDetailSheetOpen"
            :task="detailTask" />

        <ui-dialog v-model:open="projectDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>{{ editingProjectId ? '编辑项目' : '新建项目' }}</ui-dialog-title>
                    <ui-dialog-description>
                        项目绑定一个 CodeX 工作空间；修改路径只影响后续任务，已有会话保留执行时路径。
                    </ui-dialog-description>
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
                        @click="handleSaveProject">
                        {{ store.saving ? '保存中' : editingProjectId ? '保存' : '创建' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>

        <ui-dialog v-model:open="deleteProjectDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>删除空项目</ui-dialog-title>
                    <ui-dialog-description>
                        仅没有任何任务或会话记录的项目可以删除。包含业务记录时客户端会拒绝操作，不会级联清理。
                    </ui-dialog-description>
                </ui-dialog-header>
                <p class="text-sm text-muted-foreground">{{ store.selectedProject?.name }}</p>
                <ui-dialog-footer>
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="store.saving"
                        @click="deleteProjectDialogOpen = false">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="store.saving || !store.selectedProject"
                        @click="handleDeleteProject">
                        {{ store.saving ? '删除中' : '确认删除' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>

        <ui-dialog v-model:open="taskDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>{{ editingTaskId ? '修改任务' : '新建任务' }}</ui-dialog-title>
                    <ui-dialog-description
                        >只有已创建和等待中的任务可以修改，已经执行过的任务会保留历史内容。</ui-dialog-description
                    >
                </ui-dialog-header>
                <div class="grid gap-4">
                    <label class="grid gap-2">
                        <span class="text-[13px] text-foreground">任务标题</span>
                        <ui-input
                            v-model="taskForm.title"
                            :maxlength="SESSION_TASK_TITLE_MAX_CHARS"
                            placeholder="例如：实现会话管理页面" />
                    </label>
                    <label class="grid gap-2">
                        <span class="text-[13px] text-foreground">任务内容</span>
                        <ui-textarea
                            v-model="taskForm.prompt"
                            class="min-h-[160px]"
                            :maxlength="SESSION_TASK_PROMPT_MAX_CHARS"
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
                        @click="handleSaveTask">
                        {{ store.saving ? '保存中' : editingTaskId ? '保存' : '创建' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>

        <ui-dialog v-model:open="deleteTaskDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>删除任务</ui-dialog-title>
                    <ui-dialog-description>
                        进行中的任务不能删除。删除后任务卡片和关联本地记录会从当前项目移除。
                    </ui-dialog-description>
                </ui-dialog-header>
                <p class="text-sm text-muted-foreground">{{ pendingDeleteTask?.title }}</p>
                <ui-dialog-footer>
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="store.saving"
                        @click="deleteTaskDialogOpen = false">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="store.saving || !pendingDeleteTask"
                        @click="handleDeleteTask">
                        {{ store.saving ? '删除中' : '确认删除' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
    </div>
</template>

<script setup lang="ts">
    import { Delete, Edit, Folder, FolderPlus, Plus, Refresh } from '@icon-park/vue-next';
    import { toast } from 'vue-sonner';

    import TaskManageTaskBoard from '@/components/taskManage/taskBoard.vue';
    import TaskManageTaskDetailSheet from '@/components/taskManage/taskDetailSheet.vue';
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
    import type { CodexConnectionStateType } from '@/model/codexConnection';
    import {
        SESSION_TASK_PROMPT_MAX_CHARS,
        SESSION_TASK_TITLE_MAX_CHARS,
        type SessionTaskModel
    } from '@/model/sessionManage';
    import { useCodexConnectionStore } from '@/stores/codexConnection';
    import { useSessionManageStore } from '@/stores/sessionManage';

    defineOptions({
        name: 'TaskManageView'
    });

    const store = useSessionManageStore();
    const codexConnectionStore = useCodexConnectionStore();
    const projectDialogOpen = ref(false);
    const deleteProjectDialogOpen = ref(false);
    const editingProjectId = ref('');
    const taskDialogOpen = ref(false);
    const editingTaskId = ref('');
    const deleteTaskDialogOpen = ref(false);
    const pendingDeleteTask = ref<SessionTaskModel | null>(null);
    const taskDetailSheetOpen = ref(false);
    const detailTask = ref<SessionTaskModel | null>(null);
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
            void store.selectProject(projectId).catch(() => {
                // Store 已恢复原项目选择并展示 HTTP 错误，Select 无需再生成额外状态。
            });
        }
    });
    const taskStateIcon = computed(() => (store.projects.length ? Folder : FolderPlus));
    const taskStateTitle = computed(() => {
        if (!store.workspaceDataReady) return '请先打开 CodexMan App';
        if (!store.projects.length) return '先创建项目并绑定工作空间';
        return '请选择项目';
    });
    const taskStateDescription = computed(() => {
        if (!store.workspaceDataReady) {
            return '任务项目、工作空间和任务卡片都保存在本机客户端。请先打开 CodexMan App，连接成功后再创建项目或管理任务。';
        }
        if (!store.projects.length) {
            return '任务必须归属到一个项目，项目会绑定本机 CodeX 工作空间。创建项目后，这里才会展示任务看板。';
        }
        return '检测到多个项目，但本地还没有保存上次选择。请选择一个项目后，再创建和管理任务。';
    });

    /**
     * 映射任务执行前的 Codex 连接提示标题。
     * 流程：按当前连接状态返回面向用户的短标题，用于 Sonner 弹出说明。
     * 参数：state 为全局 Codex 连接状态。
     * 返回：适合 toast 展示的标题。
     * 边界：未知状态不误报断连，只提示稍后重试。
     */
    function codexQueueNoticeTitle(state: CodexConnectionStateType): string {
        const titleMap: Record<CodexConnectionStateType, string> = {
            connected: 'Codex 已连接',
            disconnected: 'Codex 未连接，暂不能执行任务',
            restarting: 'Codex 正在重启，暂不能执行任务',
            blocked: 'Codex 连接受阻，暂不能执行任务',
            unsupported: '当前平台不支持 Codex 执行任务',
            checking: '正在检测 Codex 连接',
            unknown: '暂时无法确认 Codex 连接'
        };
        return titleMap[state];
    }

    /**
     * 弹出任务执行前的 Codex 连接说明。
     * 流程：优先展示服务端安全说明；缺少说明时给出统一处理建议。
     * 参数：无。
     * 返回：无返回值。
     * 边界：只展示 Sonner，不写页面消息，避免把底层错误码铺到任务看板上。
     */
    function showCodexQueueNotice(): void {
        toast.warning(codexQueueNoticeTitle(codexConnectionStore.connectionState), {
            description:
                codexConnectionStore.message ||
                '请先在左侧状态栏确认 Codex 已连接；连接恢复后再点击任务卡片上的执行按钮。'
        });
    }

    /**
     * 弹出任务管理操作失败提示。
     * 流程：优先展示 Error 中的安全错误说明；未知异常使用调用方传入的兜底文案。
     * 参数：title 为短提示标题，error 为捕获到的异常，fallbackDescription 为未知异常说明。
     * 返回：无返回值。
     * 边界：只使用 Sonner 短提示，不覆盖页面级加载状态。
     */
    function showTaskOperationError(title: string, error: unknown, fallbackDescription: string): void {
        toast.error(title, {
            description: error instanceof Error ? error.message : fallbackDescription
        });
    }

    /**
     * 刷新工作空间数据。
     * 流程：读取本地 SQLite 项目与当前项目任务，并同步 CodeX 工作空间。
     * 参数：无。
     * 返回：无返回值。
     * 边界：后台任务完成后也会自动刷新。
     */
    function handleRefreshWorkspaces(): void {
        void store.initTaskManage();
    }

    /**
     * 打开新建项目弹窗。
     * 流程：优先把当前选中工作空间回填到项目表单，减少重复输入。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有选中工作空间时保持表单原值，由用户手动输入。
     */
    function handleOpenProjectDialog(): void {
        editingProjectId.value = '';
        projectForm.name = '';
        projectForm.workspacePath = store.selectedWorkspaceCwd;
        projectDialogOpen.value = true;
    }

    /**
     * 打开当前项目编辑弹窗。
     * 流程：读取当前 HTTP 聚合数据中的项目 ID、名称和工作空间填充同一表单。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有选中项目时保持关闭，不生成临时项目。
     */
    function handleOpenEditProjectDialog(): void {
        const project = store.selectedProject;
        if (!project) return;
        editingProjectId.value = project.id;
        projectForm.name = project.name;
        projectForm.workspacePath = project.workspacePath;
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
        editingTaskId.value = '';
        taskForm.title = '';
        taskForm.prompt = '';
        taskDialogOpen.value = true;
    }

    /**
     * 打开任务编辑弹窗。
     * 流程：只允许已创建和等待中任务进入编辑表单，并把当前任务标题与描述回填。
     * 参数：task 为当前看板任务。
     * 返回：无返回值。
     * 边界：后端仍做事务状态校验；并发状态变化时保存会失败并保留弹窗。
     */
    function handleOpenEditTaskDialog(task: SessionTaskModel): void {
        if (task.status !== 'created' && task.status !== 'queued') return;
        editingTaskId.value = task.id;
        taskForm.title = task.title;
        taskForm.prompt = task.prompt;
        taskDialogOpen.value = true;
    }

    /**
     * 打开任务删除确认弹窗。
     * 流程：除进行中任务外记录待删除任务并展示确认框。
     * 参数：task 为当前看板任务。
     * 返回：无返回值。
     * 边界：后端仍拒绝并发变成 running 的任务。
     */
    function handleOpenDeleteTaskDialog(task: SessionTaskModel): void {
        if (task.status === 'running') return;
        pendingDeleteTask.value = task;
        deleteTaskDialogOpen.value = true;
    }

    /**
     * 打开任务详情侧窗。
     * 流程：记录当前点击的任务对象，并打开右侧 Sheet 展示完整任务信息。
     * 参数：task 为当前点击的看板任务。
     * 返回：无返回值。
     * 边界：任务按钮事件会阻止冒泡，不会误触发详情侧窗。
     */
    function handleOpenTaskDetailSheet(task: SessionTaskModel): void {
        detailTask.value = task;
        taskDetailSheetOpen.value = true;
    }

    /**
     * 创建或编辑项目并重置表单。
     * 流程：按 editingProjectId 选择真实新增或更新 HTTP，成功后关闭弹窗并刷新当前项目。
     * 参数：无。
     * 返回：无返回值。
     * 边界：失败时保持弹窗打开。
     */
    async function handleSaveProject(): Promise<void> {
        const isEditingProject = Boolean(editingProjectId.value);
        try {
            if (editingProjectId.value) {
                await store.editProject({
                    id: editingProjectId.value,
                    name: projectForm.name,
                    workspacePath: projectForm.workspacePath
                });
            } else {
                await store.addProject({
                    name: projectForm.name,
                    workspacePath: projectForm.workspacePath
                });
            }
            projectDialogOpen.value = false;
            editingProjectId.value = '';
            projectForm.name = '';
            projectForm.workspacePath = '';
            toast.success(isEditingProject ? '项目已更新' : '项目已创建');
        } catch (error) {
            showTaskOperationError(isEditingProject ? '编辑项目失败' : '创建项目失败', error, '项目保存失败。');
        }
    }

    /**
     * 删除当前空项目。
     * 流程：把当前项目 ID 交给 Store 调用 HTTP，等待 Rust Immediate 事务确认后关闭确认框。
     * 参数：无。
     * 返回：删除完成 Promise。
     * 边界：没有选中项目直接返回；有关联任务或会话时保留弹窗和全部数据。
     */
    async function handleDeleteProject(): Promise<void> {
        const projectId = store.selectedProject?.id;
        if (!projectId) return;
        try {
            await store.removeProject(projectId);
            deleteProjectDialogOpen.value = false;
            toast.success('空项目已删除');
        } catch (error) {
            showTaskOperationError('删除项目失败', error, '空项目删除失败。');
        }
    }

    /**
     * 创建或编辑当前项目下的任务卡片。
     * 流程：有 editingTaskId 时更新可编辑任务，否则读取当前项目 ID 后写入已创建任务，成功后清空表单。
     * 参数：无。
     * 返回：无返回值。
     * 边界：未选中项目时不提交；并发状态变化由后端返回错误。
     */
    async function handleSaveTask(): Promise<void> {
        const isEditingTask = Boolean(editingTaskId.value);
        const projectId = store.selectedProject?.id ?? '';
        if (!isEditingTask && !projectId) return;
        try {
            if (editingTaskId.value) {
                await store.editTask({
                    id: editingTaskId.value,
                    title: taskForm.title,
                    prompt: taskForm.prompt
                });
            } else {
                await store.addTask({
                    projectId,
                    title: taskForm.title,
                    prompt: taskForm.prompt
                });
            }
            taskDialogOpen.value = false;
            editingTaskId.value = '';
            taskForm.title = '';
            taskForm.prompt = '';
            toast.success(isEditingTask ? '任务已更新' : '任务已创建');
        } catch (error) {
            showTaskOperationError(isEditingTask ? '修改任务失败' : '创建任务失败', error, '任务保存失败。');
        }
    }

    /**
     * 删除当前确认的任务。
     * 流程：把待删除任务 ID 交给 Store 调用 HTTP，等待 Rust 事务确认后关闭确认框。
     * 参数：无。
     * 返回：删除完成 Promise。
     * 边界：没有待删除任务直接返回；进行中或并发变更失败时保留数据。
     */
    async function handleDeleteTask(): Promise<void> {
        const task = pendingDeleteTask.value;
        if (!task) return;
        try {
            await store.removeTask(task.id);
            deleteTaskDialogOpen.value = false;
            pendingDeleteTask.value = null;
            toast.success('任务已删除');
        } catch (error) {
            showTaskOperationError('删除任务失败', error, '任务删除失败。');
        }
    }

    /**
     * 将任务推入排队并触发自动执行。
     * 流程：委托 store 调用 HTTP，Rust 后台调度器会自动创建 CodeX 会话。
     * 参数：taskId 为目标任务 ID。
     * 返回：无返回值。
     * 边界：不允许从 queued/running/waiting_acceptance/completed 重复排队。
     */
    async function handleQueueTask(taskId: string): Promise<void> {
        if (!codexConnectionStore.connected) {
            showCodexQueueNotice();
            void codexConnectionStore.refreshConnection(false);
            return;
        }
        try {
            await store.queueTask(taskId);
            toast.success('任务已进入排队');
        } catch (error) {
            await codexConnectionStore.refreshConnection(false);
            if (!codexConnectionStore.connected) {
                store.message = '';
                showCodexQueueNotice();
                return;
            }
            showTaskOperationError('任务入队失败', error, '任务入队失败。');
        }
    }

    /**
     * 将待验收任务标记为已完成。
     * 流程：委托 store 完成状态流转并刷新看板。
     * 参数：taskId 为目标任务 ID。
     * 返回：无返回值。
     * 边界：只有待验收任务可完成。
     */
    async function handleCompleteTask(taskId: string): Promise<void> {
        try {
            await store.completeTask(taskId);
            toast.success('任务已标记完成');
        } catch (error) {
            showTaskOperationError('验收任务失败', error, '任务验收失败。');
        }
    }

    /**
     * 定位任务绑定的 CodeX 会话。
     * 流程：使用 deeplink 打开 CodeX Desktop 对应 thread。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：无返回值。
     * 边界：未绑定 thread 的卡片按钮已禁用。
     */
    function handleOpenThread(threadId: string): void {
        void store.openExternalThread(threadId).catch((error: unknown) => {
            showTaskOperationError('打开 CodeX 会话失败', error, '打开 CodeX 会话失败。');
        });
    }

    onMounted(() => {
        void store.initTaskManage();
        void store.listenTaskUpdates().then((dispose) => {
            stopTaskUpdates = dispose;
        });
    });

    onUnmounted(() => {
        if (stopTaskUpdates) stopTaskUpdates();
    });
</script>
