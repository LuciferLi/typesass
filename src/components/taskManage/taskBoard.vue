<template>
    <section class="h-full min-h-0 w-full max-w-full overflow-hidden">
        <div class="grid h-full min-h-0 w-full grid-cols-5 gap-3">
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
                <div class="grid min-h-0 flex-1 basis-0 content-start gap-2 overflow-y-auto overscroll-contain p-2">
                    <article
                        v-for="task in taskListByStatus(column.status)"
                        :key="task.id"
                        class="grid gap-3 rounded-md border border-border bg-background p-3 shadow-sm"
                        draggable="true"
                        @dragstart="handleDragStart(task)">
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
                                variant="ghost"
                                size="icon-sm"
                                type="button"
                                :disabled="!task.externalThreadId"
                                @click="emit('open', task.externalThreadId)">
                                <focus
                                    theme="outline"
                                    size="15" />
                                <span class="sr-only">定位 CodeX 会话</span>
                            </ui-button>
                        </div>
                        <p class="line-clamp-3 text-[12px] leading-5 text-muted-foreground">{{ task.prompt }}</p>
                        <p
                            v-if="task.lastError"
                            class="line-clamp-2 text-[12px] leading-5 text-destructive">
                            {{ task.lastError }}
                        </p>
                        <div class="flex items-center justify-between gap-2">
                            <ui-badge variant="outline">{{ taskStatusLabel(task.status) }}</ui-badge>
                            <div class="flex items-center gap-1">
                                <ui-button
                                    v-if="canEditTask(task.status)"
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    :disabled="saving"
                                    title="修改任务"
                                    @click="emit('edit', task)">
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
                                    @click="emit('queue', task.id)">
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
                                    @click="emit('complete', task.id)">
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
                                    @click="emit('delete', task)">
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
    </section>
</template>

<script setup lang="ts">
    import { CheckOne, Delete, Edit, Focus, Loading, PlayOne, Plus, Time, ToBottomOne } from '@icon-park/vue-next';
    import type { Component } from 'vue';

    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Button as UiButton } from '@/components/ui/button';
    import type { SessionTaskModel, SessionTaskStatusType } from '@/model/sessionManage';

    defineOptions({
        name: 'TaskManageTaskBoard'
    });

    const props = defineProps<{
        // 当前项目下所有任务列表。
        tasks: SessionTaskModel[];
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
    }>();

    const draggingTask = ref<SessionTaskModel | null>(null);

    const columns: { status: SessionTaskStatusType; label: string; icon: Component }[] = [
        { status: 'created', label: '已创建', icon: Time },
        { status: 'queued', label: '等待中', icon: ToBottomOne },
        { status: 'running', label: '执行中', icon: Loading },
        { status: 'waiting_acceptance', label: '待验收', icon: PlayOne },
        { status: 'completed', label: '已完成', icon: CheckOne }
    ];

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
</script>
