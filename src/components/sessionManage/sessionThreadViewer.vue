<template>
    <section class="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] border-l border-border bg-background">
        <header class="flex min-w-0 items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div class="grid min-w-0 gap-0.5">
                <h2 class="truncate text-[14px] font-medium text-foreground">
                    {{ selectedThread?.title || '请选择会话' }}
                </h2>
                <p class="truncate text-[11px] leading-4 text-muted-foreground">
                    {{ selectedThread?.id || '从左侧选择一个会话后查看内容' }}
                </p>
            </div>
            <span
                class="shrink-0 rounded border px-2 py-1 text-[11px] leading-none"
                :class="connectionBadgeClass">
                {{ connectionText }}
            </span>
        </header>

        <div
            ref="scrollContainerRef"
            class="min-h-0 overflow-y-auto px-4 py-3">
            <div
                v-if="!selectedThread"
                class="grid h-full min-h-[360px] place-items-center text-center">
                <div class="grid max-w-sm justify-items-center gap-3">
                    <div
                        class="grid h-12 w-12 place-items-center rounded-lg border border-border bg-muted/40 text-muted-foreground">
                        <doc-detail
                            theme="outline"
                            size="20" />
                    </div>
                    <div class="grid gap-1">
                        <h3 class="text-[14px] font-medium text-foreground">会话内容区域</h3>
                        <p class="text-[13px] leading-5 text-muted-foreground">
                            左侧切换会话后，这里会读取最近消息并持续接收会话流。
                        </p>
                    </div>
                </div>
            </div>

            <div
                v-else-if="loading"
                class="flex h-full min-h-[360px] items-center justify-center gap-2 text-[13px] text-muted-foreground">
                <loading
                    class="animate-spin"
                    theme="outline"
                    size="16" />
                <span>读取会话内容中</span>
            </div>

            <div
                v-else-if="errorMessage"
                class="grid h-full min-h-[360px] place-items-center">
                <div class="grid max-w-md gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-4">
                    <h3 class="text-[14px] font-medium text-destructive">会话内容读取失败</h3>
                    <p class="text-[13px] leading-5 text-muted-foreground">{{ errorMessage }}</p>
                    <ui-button
                        class="w-fit"
                        variant="outline"
                        size="sm"
                        type="button"
                        @click="handleRetry">
                        <refresh
                            theme="outline"
                            size="14" />
                        <span>重试</span>
                    </ui-button>
                </div>
            </div>

            <div
                v-else-if="!messages.length"
                class="grid h-full min-h-[360px] place-items-center text-[13px] text-muted-foreground">
                当前会话暂无可展示消息
            </div>

            <div
                v-else
                class="mx-auto grid max-w-5xl gap-3 pb-6">
                <article
                    v-for="message in messages"
                    :key="`${message.messageOrder}-${message.role}`"
                    class="grid gap-2 rounded-md border border-border/75 bg-card p-3"
                    :class="message.role === 'user' ? 'ml-auto w-[min(760px,92%)]' : 'mr-auto w-full'">
                    <div class="flex min-w-0 items-center justify-between gap-2">
                        <span class="text-[12px] font-medium text-foreground">
                            {{ message.role === 'user' ? '用户' : '助手' }}
                        </span>
                        <span class="truncate text-[11px] text-muted-foreground">
                            {{ formatMessageTime(message.createdAt) }}
                        </span>
                    </div>
                    <div class="grid gap-2 text-[13px] leading-6 text-foreground">
                        <template
                            v-for="(block, blockIndex) in buildMessageBlocks(message)"
                            :key="`${message.messageOrder}-${blockIndex}`">
                            <pre
                                v-if="block.type === 'code'"
                                class="max-h-[420px] overflow-auto rounded-md border border-border bg-muted/45 p-3 text-[12px] leading-5 text-foreground"><code>{{ block.content }}</code></pre>
                            <p
                                v-else
                                class="whitespace-pre-wrap break-words">
                                {{ block.content }}
                            </p>
                        </template>
                    </div>
                    <ui-button
                        v-if="isLongMessage(message)"
                        class="w-fit"
                        variant="ghost"
                        size="sm"
                        type="button"
                        @click="handleToggleMessage(message.messageOrder)">
                        <down
                            class="transition-transform"
                            :class="expandedMessageOrders.has(message.messageOrder) ? 'rotate-180' : ''"
                            theme="outline"
                            size="14" />
                        <span>{{ expandedMessageOrders.has(message.messageOrder) ? '收起' : '展开完整内容' }}</span>
                    </ui-button>
                </article>
            </div>
        </div>
    </section>
</template>

<script setup lang="ts">
    import { DocDetail, Down, Refresh } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import type {
        CodexThreadMessageModel,
        CodexThreadStreamEventModel,
        CodexThreadSummaryModel
    } from '@/model/sessionManage';
    import { readCodexThreadMessages, streamCodexThreadEvents } from '@/service/tauri/command';

    defineOptions({
        name: 'SessionManageSessionThreadViewer'
    });

    const props = defineProps<{
        // 当前左侧选中的真实 CodeX 会话；为空时展示引导占位。
        selectedThread: CodexThreadSummaryModel | null;
    }>();

    /** 会话流连接状态。 */
    type ThreadViewerConnectionState = 'idle' | 'loading' | 'connected' | 'reconnecting' | 'failed';

    /** 右侧消息渲染块。 */
    interface ThreadMessageRenderBlock {
        /** 块类型；code 保持等宽和滚动，text 走普通段落换行。 */
        type: 'text' | 'code';
        /** 当前块正文。 */
        content: string;
    }

    const loading = ref(false);
    const errorMessage = ref('');
    const connectionState = ref<ThreadViewerConnectionState>('idle');
    const messages = ref<CodexThreadMessageModel[]>([]);
    const latestEventSeq = ref(0);
    const expandedMessageOrders = ref<Set<number>>(new Set<number>());
    const scrollContainerRef = ref<HTMLElement | null>(null);
    let streamAbortController: AbortController | null = null;

    const connectionText = computed<string>(() => {
        if (!props.selectedThread) return '未选择';
        if (connectionState.value === 'loading') return '加载中';
        if (connectionState.value === 'connected') return '已连接';
        if (connectionState.value === 'reconnecting') return '重连中';
        if (connectionState.value === 'failed') return '连接失败';
        return '待连接';
    });

    const connectionBadgeClass = computed<string>(() => {
        if (connectionState.value === 'connected') return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700';
        if (connectionState.value === 'failed') return 'border-destructive/30 bg-destructive/10 text-destructive';
        return 'border-border bg-muted/40 text-muted-foreground';
    });

    // 需要响应左侧选中 thread 切换：必须释放上一条 SSE，避免多个会话流并发占用连接和内存。
    watch(
        () => props.selectedThread?.id ?? '',
        (threadId) => {
            void loadThread(threadId);
        },
        { immediate: true }
    );

    /**
     * 滚动到底部。
     * 流程：等待 DOM 根据消息更新完成后，把右侧滚动容器移动到底部。
     * 参数：无。
     * 返回：无返回值。
     * 边界：容器尚未挂载时直接返回。
     */
    function scrollToBottom(): void {
        void nextTick(() => {
            const container = scrollContainerRef.value;
            if (!container) return;
            container.scrollTop = container.scrollHeight;
        });
    }

    /**
     * 取消当前会话流。
     * 流程：中止 fetch readable stream 并释放 controller 引用。
     * 参数：无。
     * 返回：无返回值。
     * 边界：重复取消安全无副作用。
     */
    function stopStream(): void {
        streamAbortController?.abort();
        streamAbortController = null;
    }

    /**
     * 载入指定会话正文并建立 SSE。
     * 流程：取消旧流、清空旧状态、读取 HTTP 消息窗口，再用 fetch stream 接收 snapshot/heartbeat/delta。
     * 参数：threadId 为当前选中的 CodeX thread ID。
     * 返回：加载完成 Promise。
     * 异常边界：切换会话造成的 AbortError 不展示错误；其它错误展示在右侧面板。
     */
    async function loadThread(threadId: string): Promise<void> {
        stopStream();
        latestEventSeq.value = 0;
        errorMessage.value = '';
        expandedMessageOrders.value = new Set<number>();
        if (!threadId) {
            messages.value = [];
            connectionState.value = 'idle';
            return;
        }
        loading.value = true;
        connectionState.value = 'loading';
        try {
            const response = await readCodexThreadMessages(threadId);
            messages.value = response.messages;
            scrollToBottom();
            loading.value = false;
            streamAbortController = new AbortController();
            await streamCodexThreadEvents(threadId, streamAbortController.signal, handleStreamEvent);
        } catch (error) {
            if (error instanceof Error && error.name === 'AbortError') return;
            errorMessage.value = error instanceof Error ? error.message : '读取会话内容失败。';
            connectionState.value = 'failed';
        } finally {
            loading.value = false;
        }
    }

    /**
     * 合并会话流事件。
     * 流程：按 seq 幂等丢弃旧事件；snapshot 整体替换窗口，messageDelta 按 messageOrder 更新或追加。
     * 参数：event 为 service 已解析的类型化 SSE 事件。
     * 返回：无返回值。
     * 边界：heartbeat 只更新连接态，不触发消息重渲染。
     */
    function handleStreamEvent(event: CodexThreadStreamEventModel): void {
        if (event.seq <= latestEventSeq.value) return;
        latestEventSeq.value = event.seq;
        if (event.type === 'heartbeat') {
            connectionState.value = 'connected';
            return;
        }
        if (event.type === 'snapshot') {
            messages.value = event.messages;
            connectionState.value = 'connected';
            scrollToBottom();
            return;
        }
        const index = messages.value.findIndex((message) => message.messageOrder === event.message.messageOrder);
        if (index >= 0) {
            messages.value.splice(index, 1, event.message);
        } else {
            messages.value.push(event.message);
            scrollToBottom();
        }
        connectionState.value = 'connected';
    }

    /**
     * 重新读取当前会话。
     * 流程：复用当前选中 thread ID 重新执行加载链路。
     * 参数：无。
     * 返回：无返回值。
     * 边界：未选中会话时不触发请求。
     */
    function handleRetry(): void {
        void loadThread(props.selectedThread?.id ?? '');
    }

    /**
     * 判断消息是否需要折叠。
     * 流程：按字符数量和行数双阈值判断，避免超长 Markdown 或日志直接撑爆页面。
     * 参数：message 为当前消息。
     * 返回：超过任一阈值时返回 true。
     */
    function isLongMessage(message: CodexThreadMessageModel): boolean {
        return message.content.length > 20_000 || message.content.split('\n').length > 300;
    }

    /**
     * 切换单条长消息展开状态。
     * 流程：复制 Set 后替换响应式引用，保证 Vue 能感知变化。
     * 参数：messageOrder 为目标消息顺序。
     * 返回：无返回值。
     */
    function handleToggleMessage(messageOrder: number): void {
        const next = new Set(expandedMessageOrders.value);
        if (next.has(messageOrder)) next.delete(messageOrder);
        else next.add(messageOrder);
        expandedMessageOrders.value = next;
    }

    /**
     * 获取消息用于渲染的正文。
     * 流程：普通消息原样展示；长消息未展开时按行数和字符数截断并追加提示。
     * 参数：message 为当前消息。
     * 返回：用于分块渲染的正文。
     */
    function visibleMessageContent(message: CodexThreadMessageModel): string {
        if (!isLongMessage(message) || expandedMessageOrders.value.has(message.messageOrder)) return message.content;
        const lines = message.content.split('\n').slice(0, 120).join('\n');
        return `${lines.slice(0, 12_000)}\n\n内容较长，已折叠。`;
    }

    /**
     * 构建消息渲染块。
     * 流程：按 Markdown 代码围栏拆分 code/text，文本继续按空行合并为段落，保留换行和长词折行。
     * 参数：message 为当前消息。
     * 返回：可直接在模板中安全插值渲染的块列表。
     * 边界：不使用 v-html，避免外部会话正文注入页面。
     */
    function buildMessageBlocks(message: CodexThreadMessageModel): ThreadMessageRenderBlock[] {
        const source = visibleMessageContent(message);
        const blocks: ThreadMessageRenderBlock[] = [];
        const parts = source.split(/```[\w-]*\n?/);
        const fenceCount = (source.match(/```/g) ?? []).length;
        for (let index = 0; index < parts.length; index += 1) {
            const content = parts[index]?.trim();
            if (!content) continue;
            const isCode = index % 2 === 1 && fenceCount >= index + 1;
            blocks.push({ type: isCode ? 'code' : 'text', content });
        }
        return blocks.length ? blocks : [{ type: 'text', content: source }];
    }

    /**
     * 格式化消息时间。
     * 流程：支持毫秒时间戳和 ISO 字符串；非法或空时间返回空展示。
     * 参数：value 为服务端时间字符串。
     * 返回：本地化后的短日期时间。
     */
    function formatMessageTime(value: string): string {
        if (!value) return '';
        const timestamp = /^\d+$/.test(value) ? Number(value) : Date.parse(value);
        if (!Number.isFinite(timestamp)) return '';
        return new Intl.DateTimeFormat('zh-CN', {
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit'
        }).format(new Date(timestamp));
    }

    onUnmounted(() => {
        stopStream();
    });
</script>
