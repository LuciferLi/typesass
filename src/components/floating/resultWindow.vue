<template>
    <main class="h-full bg-transparent p-3">
        <ui-card class="flex h-full flex-col p-4">
            <div class="flex items-center justify-between">
                <div>
                    <div class="text-[15px] font-semibold text-foreground">转写结果</div>
                    <div
                        v-if="reason"
                        class="mt-1 text-[12px] text-muted-foreground">
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
            <ui-button
                class="mt-3"
                type="button"
                @click="copyText">
                复制结果
            </ui-button>
        </ui-card>
    </main>
</template>

<script setup lang="ts">
    import { Button as UiButton } from '@/components/ui/button';
    import { Card as UiCard } from '@/components/ui/card';
    import { Textarea as UiTextarea } from '@/components/ui/textarea';
    import { getLastResultWindowPayload, hideResultWindow, listenEvent } from '@/service/tauri/command';

    defineOptions({
        name: 'ResultWindow'
    });

    const text = ref('');
    const reason = ref('');

    onMounted(async () => {
        const nativeWindow = window as Window & {
            __AIToolRenderResult?: (payload: { text: string; reason: string }) => void;
        };
        nativeWindow.__AIToolRenderResult = (payload) => {
            text.value = payload.text;
            reason.value = payload.reason;
        };
        await listenEvent<{ text: string; reason: string }>('result-message', (payload) => {
            text.value = payload.text;
            reason.value = payload.reason;
        });
        try {
            const payload = await getLastResultWindowPayload();
            if (payload) {
                text.value = payload.text;
                reason.value = payload.reason;
            }
        } catch {
            // 独立网页预览没有客户端结果缓存，保持空态即可。
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
