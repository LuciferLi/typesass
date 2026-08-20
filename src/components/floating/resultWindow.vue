<template>
    <main class="h-full bg-transparent p-3">
        <ui-card class="flex h-full flex-col p-4">
            <div class="flex items-center justify-between">
                <div>
                    <div class="text-[15px] font-semibold text-foreground">转写结果</div>
                    <div
                        v-if="reason"
                        class="mt-1 rounded-md px-2 py-1 text-[12px]"
                        :class="reasonClass">
                        {{ reason }}
                    </div>
                </div>
                <ui-button
                    variant="ghost"
                    size="sm"
                    type="button"
                    @click="hideResult"
                    >关闭</ui-button
                >
            </div>
            <ui-textarea
                v-model="text"
                class="mt-3 min-h-0 flex-1 resize-none"
                readonly />
            <div class="mt-3 flex flex-wrap gap-2">
                <ui-button
                    type="button"
                    :disabled="!text.trim()"
                    @click="copyText">
                    复制结果
                </ui-button>
                <ui-button
                    variant="outline"
                    type="button"
                    :disabled="isRetrying || !canRetry"
                    @click="retryAiPolish">
                    {{ isRetrying ? '重试中' : '重试 AI 润色' }}
                </ui-button>
                <ui-button
                    v-if="requiresAccessibility"
                    variant="secondary"
                    type="button"
                    @click="openAccessibility">
                    打开辅助功能设置
                </ui-button>
            </div>
        </ui-card>
    </main>
</template>

<script setup lang="ts">
    import { Button as UiButton } from '@/components/ui/button';
    import { Card as UiCard } from '@/components/ui/card';
    import { Textarea as UiTextarea } from '@/components/ui/textarea';
    import type { ResultWindowPayloadModel } from '@/model/voicePolish';
    import {
        getLastResultWindowPayload,
        hideResultWindow,
        isTauriRuntime,
        listenEvent,
        openAccessibilitySettings,
        processText
    } from '@/service/tauri/command';
    import { useModelManageStore } from '@/stores/modelManage';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'ResultWindow'
    });

    const text = ref('');
    const reason = ref('');
    const requiresAccessibility = ref(false);
    const isRetrying = ref(false);
    const voicePolishStore = useVoicePolishStore();
    const modelManageStore = useModelManageStore();
    const canRetry = computed<boolean>(() => Boolean(voicePolishStore.history[0]?.sourceText.trim()));
    const reasonClass = computed<string>(() => {
        if (reason.value.includes('成功')) return 'bg-emerald-500/10 text-emerald-300';
        if (requiresAccessibility.value || reason.value.includes('失败') || reason.value.includes('未完成')) {
            return 'bg-amber-500/10 text-amber-300';
        }
        return 'bg-secondary text-muted-foreground';
    });

    onMounted(async () => {
        const nativeWindow = window as Window & { __AIToolRenderResult?: (payload: ResultWindowPayloadModel) => void };
        nativeWindow.__AIToolRenderResult = (payload) => {
            applyResultPayload(payload);
        };
        await listenEvent<ResultWindowPayloadModel>('result-message', (payload) => {
            applyResultPayload(payload);
        });
        if (!isTauriRuntime()) return;
        await voicePolishStore.hydrateVoicePolish();
        try {
            const payload = await getLastResultWindowPayload();
            if (payload) {
                applyResultPayload(payload);
            }
        } catch (error) {
            reason.value = error instanceof Error ? error.message : '读取结果窗口缓存失败。';
        }
    });

    /**
     * 复制当前结果到系统剪贴板。
     * 流程：调用浏览器 Clipboard API 写入当前结果文本。
     * 参数：无。
     * 返回：复制完成 Promise。
     * 异常：权限拒绝时由调用事件链保留原生异常，结果文本仍留在窗口中。
     */
    async function copyText(): Promise<void> {
        await navigator.clipboard.writeText(text.value);
        reason.value = '结果已复制到剪贴板。';
    }

    /**
     * 基于最近一次语音 ASR 原文重试 AI 润色。
     * 流程：读取语音历史首项，刷新模型目录后调用统一文本处理接口；成功后更新结果窗口和本地历史。
     * 参数：无。
     * 返回：无。
     * 异常：模型缺失、HTTP 失败或 AI 返回空时只更新结果窗口原因，不隐藏用户可复制的原文。
     */
    async function retryAiPolish(): Promise<void> {
        const latest = voicePolishStore.history[0];
        if (!latest?.sourceText.trim()) {
            reason.value = '没有可重试的 ASR 原文。';
            return;
        }
        isRetrying.value = true;
        reason.value = '正在基于已保存原文重试 AI 润色。';
        try {
            await modelManageStore.refreshServiceModels();
            const selection = modelManageStore.resolveSelection('text', voicePolishStore.textModelId, '语音转文字润色');
            if (!selection.modelId) throw new Error(selection.message);
            voicePolishStore.textModelId = selection.modelId;
            const processed = await processText({
                modelId: selection.modelId,
                mode: 'dictate',
                text: latest.sourceText,
                audioDurationMs: 0,
                dictionary: voicePolishStore.dictionaryWords,
                contextApp: latest.contextApp,
                styleInstruction: voicePolishStore.styleInstruction
            });
            const processedText = processed.processedText.trim();
            if (!processedText) throw new Error('AI 润色返回为空。');
            text.value = processedText;
            voicePolishStore.history[0] = { ...latest, outputText: processedText };
            voicePolishStore.latestOutput = processedText;
            voicePolishStore.persistVoicePolish();
            reason.value = selection.message
                ? `${selection.message} AI 润色重试成功，结果已更新。`
                : 'AI 润色重试成功，结果已更新。';
            requiresAccessibility.value = false;
        } catch (error) {
            reason.value = error instanceof Error ? `重试 AI 润色失败：${error.message}` : '重试 AI 润色失败。';
        } finally {
            isRetrying.value = false;
        }
    }

    /**
     * 打开 macOS 辅助功能设置。
     * 流程：复用权限页同一 Tauri IPC，结果窗口保持可见，方便用户授权后继续复制或重试。
     * 参数：无。
     * 返回：打开请求完成 Promise。
     * 异常：打开失败时在原因区反馈，不关闭结果窗口。
     */
    async function openAccessibility(): Promise<void> {
        try {
            await openAccessibilitySettings();
            reason.value = '已打开辅助功能设置，授权 CodexMan 后可重新触发语音输入。';
        } catch (error) {
            reason.value = error instanceof Error ? error.message : '打开辅助功能设置失败。';
        }
    }

    /**
     * 应用 Rust 发送的结果窗口载荷。
     * 流程：同步文本、原因和权限入口状态，并在下一帧选中文本，方便用户直接复制。
     * 参数：payload 为结果窗口展示载荷。
     * 返回：无。
     * 异常：空字段按空字符串处理，避免窗口初始化阶段展示 undefined。
     */
    function applyResultPayload(payload: ResultWindowPayloadModel): void {
        text.value = payload.text || '';
        reason.value = payload.reason || '';
        requiresAccessibility.value = Boolean(payload.requiresAccessibility);
        void nextTick(() => {
            const textarea = document.querySelector<HTMLTextAreaElement>('textarea');
            textarea?.focus();
            textarea?.select();
        });
    }

    /**
     * 隐藏结果窗口。
     * 流程：调用受限 Tauri IPC 隐藏当前窗口，App 继续后台运行。
     * 参数：无。
     * 返回：隐藏完成 Promise。
     * 异常：普通 Web 或窗口不存在时透传 IPC 错误。
     */
    async function hideResult(): Promise<void> {
        await hideResultWindow();
    }
</script>
