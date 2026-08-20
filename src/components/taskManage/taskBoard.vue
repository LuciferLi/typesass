<template>
    <section class="flex h-full min-h-0 w-full max-w-full overflow-x-auto overflow-y-hidden pb-2">
        <div class="grid h-full min-h-0 w-full min-w-[1180px] max-w-[1480px] grid-cols-5 gap-3 overflow-hidden">
            <section
                v-for="column in columns"
                :key="column.status"
                class="flex h-full min-w-0 flex-col overflow-hidden rounded-lg border border-border bg-card"
                @dragover.prevent
                @drop="handleDrop(column.status)">
                <header class="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-3">
                    <div class="flex items-center gap-2">
                        <component
                            :is="column.icon"
                            theme="outline"
                            size="15" />
                        <span class="text-[13px] font-medium text-foreground">{{ column.label }}</span>
                    </div>
                    <div class="flex items-center gap-1.5">
                        <ui-badge variant="secondary">{{ taskListByStatus(column.status).length }}</ui-badge>
                        <ui-button
                            v-if="column.status === 'created'"
                            variant="ghost"
                            size="icon-sm"
                            type="button"
                            :disabled="saving"
                            @click.stop="emit('create')">
                            <plus
                                theme="outline"
                                size="14" />
                            <span class="sr-only">创建任务</span>
                        </ui-button>
                    </div>
                </header>
                <div
                    class="flex min-h-0 flex-1 basis-0 flex-col gap-2 overflow-y-auto overscroll-contain p-2 pr-1"
                    data-disable-window-drag>
                    <article
                        v-for="task in taskListByStatus(column.status)"
                        :key="task.id"
                        class="grid shrink-0 cursor-pointer gap-3 rounded-md border border-border bg-background p-3 shadow-sm transition-colors hover:border-primary/45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        draggable="true"
                        role="button"
                        tabindex="0"
                        @click="emit('detail', task)"
                        @dragstart="handleDragStart(task)"
                        @keydown.enter.prevent="emit('detail', task)"
                        @keydown.space.prevent="emit('detail', task)">
                        <div class="flex items-start justify-between gap-2">
                            <div class="min-w-0">
                                <div class="line-clamp-2 text-[13px] font-medium leading-5 text-foreground">
                                    {{ task.title }}
                                </div>
                                <div class="mt-1 truncate text-[11px] text-muted-foreground">
                                    {{ formatTime(task.updatedAt) }}
                                </div>
                            </div>
                            <ui-button
                                v-if="task.externalThreadId"
                                variant="ghost"
                                size="icon-sm"
                                type="button"
                                @click.stop="emit('open', task.externalThreadId)">
                                <focus
                                    theme="outline"
                                    size="15" />
                                <span class="sr-only">定位 CodeX 会话</span>
                            </ui-button>
                        </div>
                        <p class="line-clamp-3 text-[12px] leading-5 text-muted-foreground">
                            {{ formatPromptSummary(task.prompt) }}
                        </p>
                        <div
                            v-if="promptImageMap[task.id]?.length"
                            class="grid grid-cols-3 gap-1.5">
                            <button
                                v-for="image in promptImageMap[task.id]?.slice(0, 3)"
                                :key="image.src"
                                type="button"
                                class="group relative aspect-video overflow-hidden rounded border border-border bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                :title="image.alt || '预览图片'"
                                @click.stop="handlePreviewImage(image)">
                                <img
                                    :src="image.src"
                                    :alt="image.alt || '任务截图缩略图'"
                                    class="h-full w-full object-cover transition-transform group-hover:scale-105" />
                                <span
                                    class="absolute inset-x-0 bottom-0 bg-overlay/70 px-1.5 py-0.5 text-left text-[10px] leading-4 text-white opacity-0 transition-opacity group-hover:opacity-100">
                                    预览
                                </span>
                            </button>
                        </div>
                        <p
                            v-if="task.lastError"
                            class="line-clamp-2 text-[12px] leading-5 text-destructive">
                            {{ task.lastError }}
                        </p>
                        <p
                            v-if="taskStatusHint(task)"
                            class="rounded border border-border bg-muted/45 px-2 py-1 text-[11px] leading-4 text-muted-foreground">
                            {{ taskStatusHint(task) }}
                        </p>
                        <div class="flex items-center justify-between gap-2">
                            <div class="flex min-w-0 items-center gap-1.5">
                                <ui-badge variant="outline">{{ taskStatusLabel(task.status) }}</ui-badge>
                                <ui-badge
                                    v-if="projectNameById[task.projectId]"
                                    variant="secondary"
                                    class="min-w-0 max-w-[140px] truncate">
                                    {{ projectNameById[task.projectId] }}
                                </ui-badge>
                            </div>
                            <div class="flex items-center gap-1">
                                <ui-button
                                    v-if="canEditTask(task.status)"
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    :disabled="saving"
                                    title="修改任务"
                                    @click.stop="emit('edit', task)">
                                    <edit
                                        theme="outline"
                                        size="15" />
                                    <span class="sr-only">修改任务</span>
                                </ui-button>
                                <ui-button
                                    v-if="task.status === 'created' || task.status === 'failed'"
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    :disabled="saving"
                                    @click.stop="emit('queue', task.id)">
                                    <play-one
                                        theme="outline"
                                        size="15" />
                                    <span class="sr-only">进入排队中</span>
                                </ui-button>
                                <ui-button
                                    v-if="task.status === 'waiting_acceptance'"
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    :disabled="saving"
                                    @click.stop="emit('complete', task.id)">
                                    <check-one
                                        theme="outline"
                                        size="15" />
                                    <span class="sr-only">标记完成</span>
                                </ui-button>
                                <ui-button
                                    v-if="canDeleteTask(task.status)"
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    :disabled="saving"
                                    title="删除任务"
                                    @click.stop="emit('delete', task)">
                                    <delete
                                        theme="outline"
                                        size="15" />
                                    <span class="sr-only">删除任务</span>
                                </ui-button>
                            </div>
                        </div>
                    </article>
                    <div
                        v-if="!taskListByStatus(column.status).length"
                        class="rounded-md border border-dashed border-border px-3 py-8 text-center text-[12px] text-muted-foreground">
                        暂无任务
                    </div>
                </div>
            </section>
        </div>
        <ui-dialog v-model:open="previewDialogOpen">
            <ui-dialog-content class="max-h-[calc(100vh-2rem)] max-w-[min(960px,calc(100vw-2rem))] gap-3 p-4">
                <ui-dialog-header class="pr-8">
                    <ui-dialog-title class="text-[15px] leading-6">{{
                        previewImage?.alt || '图片预览'
                    }}</ui-dialog-title>
                    <ui-dialog-description class="sr-only">预览任务内容中携带的浏览器截图。</ui-dialog-description>
                </ui-dialog-header>
                <div class="min-h-0 overflow-auto rounded-md border border-border bg-background">
                    <img
                        v-if="previewImage"
                        :src="previewImage.src"
                        :alt="previewImage.alt || '任务截图预览'"
                        class="max-h-[calc(100vh-10rem)] w-full object-contain" />
                </div>
            </ui-dialog-content>
        </ui-dialog>
    </section>
</template>

<script setup lang="ts">
    import { CheckOne, Delete, Edit, Focus, Loading, PlayOne, Plus, Time, ToBottomOne } from '@icon-park/vue-next';
    import type { Component } from 'vue';

    import {
        attachmentListToPromptImages,
        extractPromptImages,
        formatPromptText,
        type TaskPromptImageModel
    } from '@/components/taskManage/taskPromptImage';
    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import type { SessionProjectModel, SessionTaskModel, SessionTaskStatusType } from '@/model/sessionManage';

    defineOptions({
        name: 'TaskManageTaskBoard'
    });

    const props = defineProps<{
        // 当前项目下所有任务列表。
        tasks: SessionTaskModel[];
        // 全部可见任务项目列表，用于在聚合看板卡片上显示项目名称标签。
        projects: SessionProjectModel[];
        // 是否正在提交状态操作。
        saving: boolean;
    }>();

    const emit = defineEmits<{
        // 将任务推入排队中。
        queue: [taskId: string];
        // 将待验收任务标记为已完成。
        complete: [taskId: string];
        // 打开任务绑定的 CodeX 会话。
        open: [threadId: string];
        // 创建当前项目下的新任务。
        create: [];
        // 修改可编辑任务。
        edit: [task: SessionTaskModel];
        // 删除非进行中任务。
        delete: [task: SessionTaskModel];
        // 查看任务详情。
        detail: [task: SessionTaskModel];
    }>();

    const draggingTask = ref<SessionTaskModel | null>(null);
    const previewDialogOpen = ref(false);
    const previewImage = ref<TaskPromptImageModel | null>(null);

    /**
     * 构建项目名称索引。
     * 流程：把项目列表转换为 projectId 到项目名称的映射，任务卡片渲染时按自身 projectId 读取。
     * 参数：无显式参数，依赖 props.projects。
     * 返回：项目 ID 到项目名称的映射。
     * 边界：任务所属项目已删除或缺失时不展示标签，避免显示路径或错误占位。
     */
    const projectNameById = computed<Record<string, string>>(() => {
        return props.projects.reduce<Record<string, string>>((nameMap, project) => {
            nameMap[project.id] = project.name;
            return nameMap;
        }, {});
    });

    /**
     * 构建任务图片缓存映射。
     * 流程：当任务列表变化时按任务 ID 解析一次图片，模板渲染缩略图时直接读取映射。
     * 参数：无显式参数，依赖 props.tasks。
     * 返回：以任务 ID 为键、图片列表为值的映射。
     * 边界：无图片任务返回空数组，模板据此不展示缩略图区域。
     */
    const promptImageMap = computed<Record<string, TaskPromptImageModel[]>>(() => {
        return props.tasks.reduce<Record<string, TaskPromptImageModel[]>>((imageMap, task) => {
            const attachmentImages = attachmentListToPromptImages(task.attachments ?? []);
            imageMap[task.id] = attachmentImages.length ? attachmentImages : extractPromptImages(task.prompt);
            return imageMap;
        }, {});
    });

    const columns: { status: SessionTaskStatusType; label: string; icon: Component }[] = [
        { status: 'created', label: '已创建', icon: Time },
        { status: 'queued', label: '等待中', icon: ToBottomOne },
        { status: 'running', label: '执行中', icon: Loading },
        { status: 'waiting_acceptance', label: '待验收', icon: PlayOne },
        { status: 'completed', label: '已完成', icon: CheckOne }
    ];

    /**
     * 统计正在占用执行槽位的任务数量。
     * 流程：从当前看板任务中统计 running 状态，用于等待任务解释为何尚未被调度器领取。
     * 参数：无显式参数，依赖 props.tasks。
     * 返回：当前执行中任务数量。
     * 边界：只使用前端已加载的权威任务快照，不猜测未加载项目的任务状态。
     */
    const runningTaskCount = computed<number>(() => {
        return props.tasks.filter((task) => task.status === 'running').length;
    });

    /**
     * 按状态筛选任务列表。
     * 流程：从 props.tasks 中过滤目标状态，用于每个看板列展示。
     * 参数：status 为看板列状态。
     * 返回：该状态下的任务列表。
     * 边界：失败任务不单独成列，仍通过按钮允许重新进入排队。
     */
    function taskListByStatus(status: SessionTaskStatusType): SessionTaskModel[] {
        if (status === 'created') {
            return props.tasks.filter((task) => task.status === 'created' || task.status === 'failed');
        }
        return props.tasks.filter((task) => task.status === status);
    }

    /**
     * 映射 HTTP 服务返回的权威任务状态展示文案。
     * 流程：逐一映射完整状态枚举，失败状态即使归入首列也保留真实状态标签。
     * 参数：status 为 Rust 状态机经 HTTP 返回的任务状态。
     * 返回：对应中文状态名称。
     * 边界：类型系统已覆盖全部状态，不使用首列名称替代真实状态。
     */
    function taskStatusLabel(status: SessionTaskStatusType): string {
        const labels: Record<SessionTaskStatusType, string> = {
            created: '已创建',
            queued: '等待中',
            running: '执行中',
            waiting_acceptance: '待验收',
            completed: '已完成',
            failed: '失败'
        };
        return labels[status];
    }

    /**
     * 格式化任务卡片摘要文案。
     * 流程：复用任务提示词格式化工具移除图片语法，避免 base64 截图文本挤占任务摘要空间。
     * 参数：prompt 为任务提示词原文。
     * 返回：适合卡片展示的纯文本摘要。
     * 边界：如果任务内容只有图片，展示短横线保留卡片结构。
     */
    function formatPromptSummary(prompt: string): string {
        return formatPromptText(prompt);
    }

    /**
     * 生成任务状态辅助提示。
     * 流程：等待中任务展示当前执行槽位占用情况，执行中任务展示已运行时长，帮助区分正常排队和异常长时间占槽。
     * 参数：task 为当前任务卡片。
     * 返回：需要展示的辅助提示；其它状态返回空字符串。
     * 边界：更新时间不可解析时只展示基础状态原因，不把无效时间渲染成错误时长。
     */
    function taskStatusHint(task: SessionTaskModel): string {
        if (task.status === 'queued') {
            return runningTaskCount.value > 0
                ? `前方 ${runningTaskCount.value} 个任务正在执行，当前任务会在空出槽位后自动开始。`
                : '等待调度器领取；如果长时间不动，请检查 Codex 连接或本地任务日志。';
        }
        if (task.status !== 'running') return '';
        const elapsed = formatElapsedTime(task.updatedAt);
        return elapsed ? `已执行约 ${elapsed}。` : '正在执行，等待 Codex 返回终态。';
    }

    /**
     * 打开任务图片预览弹窗。
     * 流程：保存当前点击的图片资源并打开 Dialog，大图直接复用缩略图 src。
     * 参数：image 为用户点击的任务提示词图片。
     * 返回：无返回值。
     * 边界：点击缩略图时阻止冒泡，避免同时打开任务详情侧窗。
     */
    function handlePreviewImage(image: TaskPromptImageModel): void {
        previewImage.value = image;
        previewDialogOpen.value = true;
    }

    /**
     * 判断任务是否允许修改。
     * 流程：仅放行用户要求的已创建和等待中状态，其它已执行状态隐藏修改入口。
     * 参数：status 为任务真实状态。
     * 返回：允许修改时为 true。
     * 边界：后端仍会在事务内重复校验，前端只负责交互可见性。
     */
    function canEditTask(status: SessionTaskStatusType): boolean {
        return status === 'created' || status === 'queued';
    }

    /**
     * 判断任务是否允许删除。
     * 流程：除 running 外全部状态展示删除入口。
     * 参数：status 为任务真实状态。
     * 返回：允许删除时为 true。
     * 边界：后端仍会在事务内拒绝 running，避免并发状态变化导致误删。
     */
    function canDeleteTask(status: SessionTaskStatusType): boolean {
        return status !== 'running';
    }

    /**
     * 记录当前拖动的任务。
     * 流程：拖动开始时保存任务对象，drop 时按目标列判断是否允许流转。
     * 参数：task 为当前拖动任务。
     * 返回：无返回值。
     * 边界：拖动结束后由 drop 逻辑清空，避免下一次误用旧任务。
     */
    function handleDragStart(task: SessionTaskModel): void {
        draggingTask.value = task;
    }

    /**
     * 处理卡片拖放后的状态流转。
     * 流程：只允许已创建拖到排队中、待验收拖到已完成，其余状态之间忽略。
     * 参数：targetStatus 为目标看板列状态。
     * 返回：无返回值。
     * 边界：系统自动流转的 queued/running/waiting_acceptance 不接受人工拖动。
     */
    function handleDrop(targetStatus: SessionTaskStatusType): void {
        const task = draggingTask.value;
        draggingTask.value = null;
        if (!task) return;
        if (task.status === 'created' && targetStatus === 'queued') {
            emit('queue', task.id);
            return;
        }
        if (task.status === 'waiting_acceptance' && targetStatus === 'completed') {
            emit('complete', task.id);
        }
    }

    /**
     * 格式化任务更新时间。
     * 流程：把 SQLite 时间字符串转换成本地展示格式，失败时返回原值。
     * 参数：value 为任务更新时间。
     * 返回：适合卡片展示的时间。
     * 边界：空时间展示横线。
     */
    function formatTime(value: string): string {
        if (!value) return '-';
        const date = new Date(value);
        if (Number.isNaN(date.getTime())) return value;
        return date.toLocaleString();
    }

    /**
     * 格式化任务运行时长。
     * 流程：用当前时间减去任务最近更新时间并转换为分钟或小时级中文提示。
     * 参数：value 为任务更新时间。
     * 返回：可读时长；时间为空、非法或来自未来时返回空字符串。
     * 边界：不足一分钟按“不到 1 分钟”展示，避免刚进入执行状态时显示 0 分钟。
     */
    function formatElapsedTime(value: string): string {
        if (!value) return '';
        const startedAt = new Date(value).getTime();
        if (Number.isNaN(startedAt)) return '';
        const elapsedMinutes = Math.floor((Date.now() - startedAt) / 60_000);
        if (elapsedMinutes < 0) return '';
        if (elapsedMinutes < 1) return '不到 1 分钟';
        if (elapsedMinutes < 60) return `${elapsedMinutes} 分钟`;
        const hours = Math.floor(elapsedMinutes / 60);
        const minutes = elapsedMinutes % 60;
        return minutes ? `${hours} 小时 ${minutes} 分钟` : `${hours} 小时`;
    }
</script>
