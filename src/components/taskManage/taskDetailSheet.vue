<template>
    <ui-sheet v-model:open="open">
        <ui-sheet-content
            side="right"
            class="bottom-4 top-4 flex h-auto max-h-[calc(100vh-2rem)] w-[420px] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-l-lg p-0">
            <ui-sheet-header class="shrink-0 border-b border-border px-5 py-4">
                <ui-sheet-title class="pr-8 text-[16px] leading-6">{{ task?.title || '任务详情' }}</ui-sheet-title>
                <ui-sheet-description class="sr-only">展示当前任务卡片的完整详情。</ui-sheet-description>
            </ui-sheet-header>

            <div
                v-if="task"
                class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
                <section class="grid gap-4">
                    <div class="flex items-center justify-between gap-3">
                        <ui-badge variant="outline">{{ taskStatusLabel(task.status) }}</ui-badge>
                        <span class="text-[12px] text-muted-foreground">{{ formatTime(task.updatedAt) }}</span>
                    </div>

                    <div class="grid gap-2 rounded-md border border-border bg-card p-3">
                        <span class="text-[12px] font-medium text-foreground">任务内容</span>
                        <p class="whitespace-pre-wrap break-words text-[13px] leading-6 text-muted-foreground">
                            {{ formattedPromptText }}
                        </p>
                        <div
                            v-if="promptImages.length"
                            class="grid grid-cols-2 gap-2">
                            <button
                                v-for="image in promptImages"
                                :key="image.src"
                                type="button"
                                class="group relative aspect-video overflow-hidden rounded-md border border-border bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                :title="image.alt || '预览图片'"
                                @click="handlePreviewImage(image)">
                                <img
                                    :src="image.src"
                                    :alt="image.alt || '任务截图缩略图'"
                                    class="h-full w-full object-cover transition-transform group-hover:scale-105" />
                                <span
                                    class="absolute inset-x-0 bottom-0 bg-overlay/70 px-2 py-1 text-left text-[11px] leading-4 text-white opacity-0 transition-opacity group-hover:opacity-100">
                                    预览
                                </span>
                            </button>
                        </div>
                    </div>

                    <div
                        v-if="task.lastError"
                        class="grid gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3">
                        <span class="text-[12px] font-medium text-destructive">最近错误</span>
                        <p class="whitespace-pre-wrap break-words text-[13px] leading-6 text-destructive">
                            {{ task.lastError }}
                        </p>
                    </div>

                    <div class="grid gap-2 rounded-md border border-border bg-card p-3">
                        <span class="text-[12px] font-medium text-foreground">任务信息</span>
                        <dl class="grid gap-2 text-[12px]">
                            <div
                                v-for="row in taskMetaRows"
                                :key="row.label"
                                class="grid grid-cols-[86px_minmax(0,1fr)] gap-3">
                                <dt class="text-muted-foreground">{{ row.label }}</dt>
                                <dd class="break-all text-foreground">{{ row.value }}</dd>
                            </div>
                        </dl>
                    </div>

                    <div
                        v-if="formattedResultJson"
                        class="grid gap-2 rounded-md border border-border bg-card p-3">
                        <span class="text-[12px] font-medium text-foreground">执行结果</span>
                        <pre
                            class="max-h-[260px] overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-3 text-[11px] leading-5 text-muted-foreground"
                            >{{ formattedResultJson }}</pre
                        >
                    </div>
                </section>
            </div>
        </ui-sheet-content>
    </ui-sheet>
    <ui-dialog v-model:open="previewDialogOpen">
        <ui-dialog-content class="max-h-[calc(100vh-2rem)] max-w-[min(960px,calc(100vw-2rem))] gap-3 p-4">
            <ui-dialog-header class="pr-8">
                <ui-dialog-title class="text-[15px] leading-6">{{ previewImage?.alt || '图片预览' }}</ui-dialog-title>
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
</template>

<script setup lang="ts">
    import {
        attachmentListToPromptImages,
        extractPromptImages,
        formatPromptText,
        type TaskPromptImageModel
    } from '@/components/taskManage/taskPromptImage';
    import { Badge as UiBadge } from '@/components/ui/badge';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import {
        Sheet as UiSheet,
        SheetContent as UiSheetContent,
        SheetDescription as UiSheetDescription,
        SheetHeader as UiSheetHeader,
        SheetTitle as UiSheetTitle
    } from '@/components/ui/sheet';
    import type { SessionTaskModel, SessionTaskStatusType } from '@/model/sessionManage';

    defineOptions({
        name: 'TaskManageTaskDetailSheet'
    });

    const props = defineProps<{
        // 当前展示的任务；为空时侧窗保留结构但不渲染详情内容。
        task: SessionTaskModel | null;
    }>();

    const open = defineModel<boolean>('open', { default: false });
    const previewDialogOpen = ref(false);
    const previewImage = ref<TaskPromptImageModel | null>(null);

    /**
     * 格式化详情侧窗任务内容。
     * 流程：移除 Markdown 图片语法，图片改由缩略图区域承载，正文保留完整文本证据。
     * 参数：无显式参数，依赖当前 task.prompt。
     * 返回：适合详情侧窗展示的正文内容。
     * 边界：无任务或仅图片内容时展示短横线。
     */
    const formattedPromptText = computed<string>(() => {
        return formatPromptText(props.task?.prompt ?? '');
    });

    /**
     * 解析详情侧窗任务截图列表。
     * 流程：从当前任务提示词中解析 Markdown 图片，供缩略图和预览弹窗展示。
     * 参数：无显式参数，依赖当前 task.prompt。
     * 返回：当前任务内容中的图片列表。
     * 边界：无任务或无图片时返回空数组，模板不展示缩略图区域。
     */
    const promptImages = computed<TaskPromptImageModel[]>(() => {
        const attachmentImages = attachmentListToPromptImages(props.task?.attachments ?? []);
        return attachmentImages.length ? attachmentImages : extractPromptImages(props.task?.prompt ?? '');
    });

    const taskMetaRows = computed<{ label: string; value: string }[]>(() => {
        const { task } = props;
        if (!task) return [];
        return [
            { label: '任务 ID', value: task.id },
            { label: '项目 ID', value: task.projectId },
            { label: '本地会话', value: task.currentSessionId || '-' },
            { label: 'CodeX 会话', value: task.externalThreadId || '-' },
            { label: '创建时间', value: formatTime(task.createdAt) },
            { label: '更新时间', value: formatTime(task.updatedAt) }
        ];
    });

    const formattedResultJson = computed(() => {
        const rawResult = props.task?.resultJson.trim() ?? '';
        if (!rawResult || rawResult === '{}') return '';
        try {
            return JSON.stringify(JSON.parse(rawResult), null, 2);
        } catch {
            return rawResult;
        }
    });

    /**
     * 映射任务状态展示文案。
     * 流程：按任务状态枚举返回中文状态名称，供详情侧窗和卡片状态保持一致。
     * 参数：status 为 HTTP 服务返回的任务状态。
     * 返回：对应中文状态名称。
     * 边界：类型系统覆盖全部状态，避免未知状态进入展示。
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
     * 打开任务图片预览弹窗。
     * 流程：保存当前点击的图片资源并打开 Dialog，大图直接复用缩略图 src。
     * 参数：image 为用户点击的任务提示词图片。
     * 返回：无返回值。
     * 边界：预览弹窗独立于详情侧窗，不改变当前任务详情打开状态。
     */
    function handlePreviewImage(image: TaskPromptImageModel): void {
        previewImage.value = image;
        previewDialogOpen.value = true;
    }

    /**
     * 格式化任务时间。
     * 流程：把服务端时间字符串转换成本地展示格式，转换失败时保留原值便于排查数据。
     * 参数：value 为任务时间字符串。
     * 返回：适合详情侧窗展示的时间。
     * 边界：空时间展示横线。
     */
    function formatTime(value: string): string {
        if (!value) return '-';
        const date = new Date(value);
        if (Number.isNaN(date.getTime())) return value;
        return date.toLocaleString();
    }
</script>
