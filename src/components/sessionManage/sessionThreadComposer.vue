<template>
    <form
        class="border-t border-white/10 bg-[#111113]/95 px-4 py-3"
        @submit.prevent="handleSubmit">
        <div class="mx-auto grid w-full max-w-[860px] gap-2">
            <div
                v-if="pendingNotice"
                class="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-amber-300/20 bg-amber-300/10 px-3 py-2 text-[12px] leading-5 text-amber-100">
                <span class="min-w-0 truncate">{{ pendingNotice }}</span>
                <button
                    class="shrink-0 text-[12px] text-amber-100/80 hover:text-amber-50"
                    type="button"
                    @click="handleCancelPending">
                    取消
                </button>
            </div>

            <div class="relative rounded-[18px] border border-white/10 bg-[#1f1f23] shadow-lg shadow-black/20">
                <input
                    ref="fileInputRef"
                    class="hidden"
                    type="file"
                    accept="image/png,image/jpeg,image/webp"
                    multiple
                    @change="handleAttachmentInputChange" />

                <div
                    v-if="selectedAttachments.length"
                    class="flex gap-2 overflow-x-auto px-3 pt-3">
                    <div
                        v-for="attachment in selectedAttachments"
                        :key="attachment.id"
                        class="group relative h-14 w-14 shrink-0 overflow-hidden rounded-lg border border-white/10 bg-white/[0.04]">
                        <img
                            class="h-full w-full object-cover"
                            :alt="attachment.name"
                            :src="attachment.dataUrl" />
                        <button
                            class="absolute right-1 top-1 grid h-5 w-5 place-items-center rounded-full bg-black/70 text-white opacity-90 transition hover:bg-black"
                            type="button"
                            :title="`移除图片：${attachment.name}`"
                            @click="handleRemoveAttachment(attachment.id)">
                            <preview-close
                                theme="outline"
                                size="12" />
                        </button>
                    </div>
                </div>

                <textarea
                    ref="textareaRef"
                    v-model="draft"
                    class="block max-h-[180px] min-h-[74px] w-full resize-none bg-transparent px-4 pb-12 pt-3 text-[14px] leading-6 text-white outline-none placeholder:text-white/35 disabled:cursor-not-allowed disabled:text-white/35"
                    :disabled="disabled || sending"
                    :placeholder="placeholderText"
                    rows="3"
                    @compositionend="handleCompositionEnd"
                    @compositionstart="handleCompositionStart"
                    @input="handleInput"
                    @keydown="handleKeydown"
                    @paste="handlePaste" />

                <div
                    v-if="slashPanelVisible"
                    class="absolute bottom-[52px] left-3 z-20 grid w-[260px] overflow-hidden rounded-xl border border-white/10 bg-[#252529] py-1 shadow-2xl shadow-black/40">
                    <button
                        v-for="item in commandItems"
                        :key="item.id"
                        class="grid gap-0.5 px-3 py-2 text-left hover:bg-white/[0.06]"
                        type="button"
                        @mousedown.prevent="handleCommandSelect(item)">
                        <span class="text-[13px] leading-5 text-white/90">{{ item.title }}</span>
                        <span class="text-[11px] leading-4 text-white/45">{{ item.description }}</span>
                    </button>
                </div>

                <div class="absolute inset-x-0 bottom-0 flex items-center justify-between gap-2 px-2.5 py-2">
                    <div class="flex min-w-0 items-center gap-1.5">
                        <button
                            class="inline-flex h-8 w-8 items-center justify-center rounded-full border border-white/10 bg-white/[0.04] text-white/70 hover:bg-white/[0.08] disabled:cursor-not-allowed disabled:opacity-50"
                            type="button"
                            :disabled="disabled || sending || selectedAttachments.length >= ATTACHMENT_LIMIT"
                            :title="attachmentButtonTitle"
                            @click="handleChooseAttachment">
                            <upload
                                theme="outline"
                                size="14" />
                        </button>

                        <button
                            class="inline-flex h-8 items-center gap-1.5 rounded-full border border-white/10 bg-white/[0.04] px-2.5 text-[12px] text-white/70 hover:bg-white/[0.08] disabled:opacity-50"
                            type="button"
                            disabled
                            title="当前 CodeX 模式由原生客户端控制">
                            <command
                                theme="outline"
                                size="13" />
                            <span>Codex</span>
                            <down
                                theme="outline"
                                size="12" />
                        </button>
                    </div>

                    <button
                        class="inline-flex h-8 min-w-8 items-center justify-center rounded-full bg-white text-[#111113] transition hover:bg-white/90 disabled:cursor-not-allowed disabled:bg-white/20 disabled:text-white/35"
                        type="submit"
                        :disabled="submitDisabled"
                        :title="submitTitle">
                        <loading
                            v-if="sending"
                            class="animate-spin"
                            theme="outline"
                            size="15" />
                        <send
                            v-else
                            theme="outline"
                            size="15" />
                    </button>
                </div>
            </div>
        </div>
    </form>
</template>

<script setup lang="ts">
    import { Command, Down, Loading, PreviewClose, Send, Upload } from '@icon-park/vue-next';
    import { toast } from 'vue-sonner';

    import type { SessionTaskAttachmentModel } from '@/model/sessionManage';
    import { openSessionExternalThread, sendCodexThreadMessage } from '@/service/tauri/command';

    defineOptions({
        name: 'SessionThreadComposer'
    });

    /** Slash 面板动作项。 */
    interface ComposerCommandItem {
        /** 动作稳定 ID。 */
        id: 'refresh' | 'openNative';
        /** 动作标题。 */
        title: string;
        /** 动作说明。 */
        description: string;
    }

    /** 输入框待发送消息快照。 */
    interface ComposerPendingMessage {
        /** 待发送正文。 */
        content: string;
        /** 待发送图片附件。 */
        attachments: SessionTaskAttachmentModel[];
    }

    /** 已有会话单条消息最多允许的图片数量。 */
    const ATTACHMENT_LIMIT = 4;
    /** 单张图片 data URL 字符上限，必须与 Rust 任务附件校验保持一致。 */
    const ATTACHMENT_DATA_URL_MAX_LENGTH = 200_000;
    /** 支持直接发送给 CodeX composer 的图片 MIME 集合。 */
    const SUPPORTED_ATTACHMENT_MIME_TYPES = ['image/png', 'image/jpeg', 'image/webp'] as const;

    const props = defineProps<{
        /** 当前是否选中了可发送的会话。 */
        hasThread: boolean;
        /** 当前选中的 CodeX 会话 ID。 */
        threadId: string;
        /** 当前会话是否正在运行，运行中提交会进入页面内存队列。 */
        threadRunning: boolean;
        /** 父级禁用态，例如没有选中会话或读取失败。 */
        disabled: boolean;
    }>();

    const emit = defineEmits<{
        /** 当前会话需要重新读取详情。 */
        refresh: [];
    }>();

    const draft = ref('');
    const composing = ref(false);
    const sending = ref(false);
    const selectedAttachments = ref<SessionTaskAttachmentModel[]>([]);
    const pendingMessageByThreadId = ref<Record<string, ComposerPendingMessage | string>>({});
    const slashPanelVisible = ref(false);
    const textareaRef = ref<HTMLTextAreaElement | null>(null);
    const fileInputRef = ref<HTMLInputElement | null>(null);

    const commandItems: ComposerCommandItem[] = [
        {
            id: 'refresh',
            title: '刷新会话',
            description: '重新读取当前会话详情'
        },
        {
            id: 'openNative',
            title: '打开 CodeX',
            description: '在原生客户端打开当前会话'
        }
    ];

    const trimmedDraft = computed<string>(() => draft.value.trim());
    const pendingMessage = computed<ComposerPendingMessage | null>(() =>
        normalizePendingMessage(pendingMessageByThreadId.value[props.threadId])
    );
    const hasPendingMessage = computed<boolean>(() => Boolean(pendingMessage.value));

    const placeholderText = computed<string>(() => {
        if (!props.hasThread) return '选择会话后继续对话';
        if (hasPendingMessage.value) return '已有一条消息等待发送';
        if (props.threadRunning) return '当前会话运行中，发送后会等待当前回复结束';
        return '继续给 CodeX 发送消息';
    });

    const pendingNotice = computed<string>(() => {
        const pending = pendingMessage.value;
        if (!pending) return '';
        const attachmentText = pending.attachments.length ? `，包含 ${pending.attachments.length} 张图片` : '';
        return `这条消息${attachmentText}正在等待当前回复结束，结束后会自动发送。`;
    });

    const submitDisabled = computed<boolean>(() => {
        return (
            props.disabled ||
            sending.value ||
            hasPendingMessage.value ||
            (!trimmedDraft.value && selectedAttachments.value.length === 0)
        );
    });

    const submitTitle = computed<string>(() => {
        if (hasPendingMessage.value) return '已有待发送消息';
        if (props.threadRunning) return '等待当前回复结束后发送';
        return '发送';
    });

    const attachmentButtonTitle = computed<string>(() => {
        if (selectedAttachments.value.length >= ATTACHMENT_LIMIT) return '最多添加 4 张图片';
        return '添加图片';
    });

    // 左侧状态刷新到非 running 后，自动消费该会话的内存待发送消息。
    watch(
        () => `${props.threadId}:${props.threadRunning ? 'running' : 'idle'}`,
        () => {
            void flushPendingMessage();
        }
    );

    /**
     * 处理文本输入。
     * 流程：同步 textarea 高度并根据首字符斜杠显示命令面板。
     * 参数：无，直接读取当前 v-model。
     * 返回：无。
     * 边界：只在正文等于 `/` 或以 `/` 开头且不含换行时展示面板，避免普通段落误触发。
     */
    function handleInput(): void {
        resizeTextarea();
        slashPanelVisible.value =
            draft.value.startsWith('/') && !draft.value.includes('\n') && draft.value.length <= 32;
    }

    /**
     * 处理按键提交。
     * 流程：Enter 在非输入法组合态下触发表单 submit；Shift+Enter 保留浏览器默认换行。
     * 参数：event 为键盘事件。
     * 返回：无。
     * 边界：输入法确认时不发送，避免中文候选词上屏被误当作提交。
     */
    function handleKeydown(event: KeyboardEvent): void {
        if (event.key === 'Escape' && slashPanelVisible.value) {
            event.preventDefault();
            slashPanelVisible.value = false;
            return;
        }
        if (event.key !== 'Enter' || event.shiftKey || composing.value || event.isComposing) return;
        event.preventDefault();
        handleSubmit();
    }

    /**
     * 记录输入法组合开始。
     * 流程：标记组合态，后续 Enter 只交给输入法确认候选词。
     * 参数：无。
     * 返回：无。
     * 边界：组合态结束前不触发发送。
     */
    function handleCompositionStart(): void {
        composing.value = true;
    }

    /**
     * 记录输入法组合结束。
     * 流程：清除组合态并重新计算命令面板可见性。
     * 参数：无。
     * 返回：无。
     * 边界：compositionend 本身不发送消息。
     */
    function handleCompositionEnd(): void {
        composing.value = false;
        handleInput();
    }

    /**
     * 处理表单提交。
     * 流程：读取裁剪后的正文并通过事件交给父组件；发送前只清空本地草稿，不直接调用 HTTP。
     * 参数：无。
     * 返回：无。
     * 边界：禁用、空内容、待发送未消费时直接忽略。
     */
    function handleSubmit(): void {
        if (submitDisabled.value) return;
        const content = trimmedDraft.value;
        const attachments = selectedAttachments.value.map((attachment) => ({ ...attachment }));
        draft.value = '';
        selectedAttachments.value = [];
        slashPanelVisible.value = false;
        resizeTextarea();
        if (props.threadRunning) {
            pendingMessageByThreadId.value[props.threadId] = {
                content,
                attachments
            };
            toast.info('当前回复结束后会自动发送。');
            return;
        }
        void sendMessage(props.threadId, content, attachments);
    }

    /**
     * 处理 Slash 面板动作。
     * 流程：按动作类型转成父组件事件，随后清空命令输入。
     * 参数：item 为用户选择的命令项。
     * 返回：无。
     * 边界：当前只展示 CodexMan 自有动作，不伪装成 CodeX 原生命令。
     */
    function handleCommandSelect(item: ComposerCommandItem): void {
        draft.value = '';
        slashPanelVisible.value = false;
        if (item.id === 'refresh') emit('refresh');
        if (item.id === 'openNative') handleOpenNativeThread();
        resizeTextarea();
    }

    /**
     * 发送消息到指定会话。
     * 流程：调用公开 HTTP 发送接口，成功后通知父级刷新详情；失败时保留正文和图片为待发送，避免用户输入丢失。
     * 参数：threadId 为目标会话 ID，content 为正文，attachments 为图片附件。
     * 返回：发送完成 Promise。
     * 边界：发送不确定错误不会自动重放，用户可在详情确认后手动取消或等待重试。
     */
    async function sendMessage(
        threadId: string,
        content: string,
        attachments: SessionTaskAttachmentModel[]
    ): Promise<void> {
        if (!threadId || sending.value) return;
        sending.value = true;
        try {
            await sendCodexThreadMessage(threadId, {
                content: content.trim(),
                attachments: normalizeAttachments(attachments)
            });
            delete pendingMessageByThreadId.value[threadId];
            toast.success('消息已发送。');
            emit('refresh');
        } catch (error) {
            pendingMessageByThreadId.value[threadId] = {
                content,
                attachments
            };
            toast.error(error instanceof Error ? error.message : '消息发送失败。');
        } finally {
            sending.value = false;
        }
    }

    /**
     * 当前会话结束运行后消费待发送消息。
     * 流程：只处理当前 props.threadId，状态仍 running 或正在发送时跳过。
     * 参数：无。
     * 返回：消费完成 Promise。
     * 边界：不同会话的待发送正文按 threadId 隔离，不随左侧切换串线。
     */
    async function flushPendingMessage(): Promise<void> {
        const pending = pendingMessage.value;
        if (!props.threadId || props.threadRunning || sending.value || !pending) return;
        await sendMessage(props.threadId, pending.content, pending.attachments);
    }

    /**
     * 取消当前会话的待发送消息。
     * 流程：只清除当前 thread 的内存正文。
     * 参数：无。
     * 返回：无。
     * 边界：不会影响其它后台会话等待发送的正文。
     */
    function handleCancelPending(): void {
        if (!props.threadId || !pendingMessageByThreadId.value[props.threadId]) return;
        delete pendingMessageByThreadId.value[props.threadId];
        toast.info('已取消待发送消息。');
    }

    /**
     * 规范化运行态待发送消息。
     * 流程：兼容热更新前残留的纯字符串 pending，并把附件字段统一为数组。
     * 参数：pending 为当前 thread 内存队列中的原始值。
     * 返回：可安全发送的待发送消息；无正文且无附件时返回 null。
     * 边界：只处理当前页面内存态，不读取或修改其它会话历史。
     */
    function normalizePendingMessage(
        pending: ComposerPendingMessage | string | undefined
    ): ComposerPendingMessage | null {
        if (typeof pending === 'string') {
            const content = pending.trim();
            return content ? { content, attachments: [] } : null;
        }
        if (!pending) return null;
        const content = pending.content.trim();
        const attachments = normalizeAttachments(pending.attachments);
        if (!content && attachments.length === 0) return null;
        return { content, attachments };
    }

    /**
     * 规范化消息图片附件。
     * 流程：复制合法附件结构，避免 Vue 响应式代理、旧 undefined 或外部突变直接进入 HTTP 请求体。
     * 参数：attachments 为输入框当前附件数组。
     * 返回：普通 JSON 数组。
     * 边界：附件内容合法性仍由服务端和 Rust 进行权威校验。
     */
    function normalizeAttachments(attachments: SessionTaskAttachmentModel[] | undefined): SessionTaskAttachmentModel[] {
        return (attachments ?? []).map((attachment) => ({
            id: attachment.id,
            name: attachment.name,
            mimeType: attachment.mimeType,
            dataUrl: attachment.dataUrl
        }));
    }

    /**
     * 打开图片选择器。
     * 流程：代理点击隐藏 file input，使用浏览器原生选择器读取图片。
     * 参数：无。
     * 返回：无。
     * 边界：已达到数量上限时不打开，避免用户选择后再批量失败。
     */
    function handleChooseAttachment(): void {
        if (selectedAttachments.value.length >= ATTACHMENT_LIMIT) return;
        fileInputRef.value?.click();
    }

    /**
     * 处理图片选择结果。
     * 流程：读取 input.files 后统一进入附件添加逻辑，最后清空 input value 允许重复选择同名文件。
     * 参数：event 为文件选择变更事件。
     * 返回：无。
     * 边界：非 input 事件或空文件直接忽略。
     */
    function handleAttachmentInputChange(event: Event): void {
        const input = event.target instanceof HTMLInputElement ? event.target : null;
        if (!input?.files?.length) return;
        void addAttachmentFiles(Array.from(input.files)).finally(() => {
            input.value = '';
        });
    }

    /**
     * 处理输入框粘贴图片。
     * 流程：从剪贴板文件中筛选图片，存在图片时阻止默认粘贴并加入附件预览。
     * 参数：event 为粘贴事件。
     * 返回：无。
     * 边界：纯文本粘贴不拦截，保持浏览器默认输入行为。
     */
    function handlePaste(event: ClipboardEvent): void {
        const files = Array.from(event.clipboardData?.files ?? []).filter((file) =>
            SUPPORTED_ATTACHMENT_MIME_TYPES.includes(file.type as (typeof SUPPORTED_ATTACHMENT_MIME_TYPES)[number])
        );
        if (!files.length) return;
        event.preventDefault();
        void addAttachmentFiles(files);
    }

    /**
     * 添加图片附件。
     * 流程：校验数量、MIME 与 data URL 长度后写入本地预览数组。
     * 参数：files 为用户选择或粘贴的文件列表。
     * 返回：添加完成 Promise。
     * 异常边界：单个文件失败会提示并跳过，不影响其它合法图片。
     */
    async function addAttachmentFiles(files: File[]): Promise<void> {
        const remaining = ATTACHMENT_LIMIT - selectedAttachments.value.length;
        if (remaining <= 0) {
            toast.warning('最多只能添加 4 张图片。');
            return;
        }
        const acceptedFiles = files.slice(0, remaining);
        if (files.length > remaining) toast.warning('已达到 4 张图片上限，超出的图片未添加。');
        const nextAttachments: SessionTaskAttachmentModel[] = [];
        for (const file of acceptedFiles) {
            if (
                !SUPPORTED_ATTACHMENT_MIME_TYPES.includes(file.type as (typeof SUPPORTED_ATTACHMENT_MIME_TYPES)[number])
            ) {
                toast.warning('仅支持 PNG、JPEG 或 WebP 图片。');
                continue;
            }
            try {
                const dataUrl = await readFileAsDataUrl(file);
                nextAttachments.push({
                    id: createAttachmentId(),
                    name: file.name || 'codex-image.png',
                    mimeType: file.type as SessionTaskAttachmentModel['mimeType'],
                    dataUrl
                });
            } catch (error) {
                toast.error(error instanceof Error ? error.message : '图片读取失败。');
            }
        }
        if (!nextAttachments.length) return;
        selectedAttachments.value = [...selectedAttachments.value, ...nextAttachments];
    }

    /**
     * 移除已选择图片。
     * 流程：按附件 ID 从预览列表中删除。
     * 参数：attachmentId 为附件稳定 ID。
     * 返回：无。
     * 边界：ID 不存在时保持原列表。
     */
    function handleRemoveAttachment(attachmentId: string): void {
        selectedAttachments.value = selectedAttachments.value.filter((attachment) => attachment.id !== attachmentId);
    }

    /**
     * 读取图片为 data URL。
     * 流程：使用浏览器 FileReader 读取图片，并在前端提前执行 data URL 长度上限校验。
     * 参数：file 为待读取图片。
     * 返回：图片 data URL。
     * 异常：读取失败或超过上限时抛出明确错误。
     */
    function readFileAsDataUrl(file: File): Promise<string> {
        return new Promise((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => {
                if (typeof reader.result !== 'string') {
                    reject(new Error('图片读取失败。'));
                    return;
                }
                if (reader.result.length > ATTACHMENT_DATA_URL_MAX_LENGTH) {
                    reject(new Error('图片过大，请选择更小的图片。'));
                    return;
                }
                resolve(reader.result);
            };
            reader.onerror = () => reject(new Error('图片读取失败。'));
            reader.readAsDataURL(file);
        });
    }

    /**
     * 创建附件稳定 ID。
     * 流程：优先使用安全上下文 randomUUID，缺失时退化为时间戳加随机片段。
     * 参数：无。
     * 返回：前端渲染和排障使用的附件 ID。
     * 边界：ID 只用于本地渲染，不作为安全凭证。
     */
    function createAttachmentId(): string {
        if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
            return crypto.randomUUID();
        }
        return `attachment-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
    }

    /**
     * 在原生 CodeX 中打开当前会话。
     * 流程：复用公开打开接口；成功后由 CodeX Desktop 自行切换页面。
     * 参数：无。
     * 返回：无。
     * 边界：未选中会话时不发起请求。
     */
    function handleOpenNativeThread(): void {
        if (!props.threadId) return;
        void openSessionExternalThread(props.threadId)
            .then(() => toast.success('已请求打开 CodeX 会话。'))
            .catch((error: unknown) => {
                toast.error(error instanceof Error ? error.message : '打开 CodeX 会话失败。');
            });
    }

    /**
     * 根据内容重置输入框高度。
     * 流程：先恢复 auto 再使用 scrollHeight 限制到 180px。
     * 参数：无。
     * 返回：无。
     * 边界：DOM 尚未挂载时直接跳过。
     */
    function resizeTextarea(): void {
        const textarea = textareaRef.value;
        if (!textarea) return;
        textarea.style.height = 'auto';
        textarea.style.height = `${Math.min(textarea.scrollHeight, 180)}px`;
    }
</script>
