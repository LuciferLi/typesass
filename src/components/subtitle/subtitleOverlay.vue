<template>
    <main class="flex h-full items-end justify-center bg-transparent p-4">
        <ui-card
            class="w-full max-w-[960px] px-6 py-4 text-center"
            :class="payload.state === 'error' ? 'border-destructive bg-destructive text-destructive-foreground' : ''">
            <div class="text-[12px] font-medium text-muted-foreground">{{ statusText }}</div>
            <div class="mt-2 min-h-[40px] text-[24px] font-semibold leading-tight">
                {{ payload.text || '等待字幕' }}
            </div>
        </ui-card>
    </main>
</template>

<script setup lang="ts">
    import { Card as UiCard } from '@/components/ui/card';
    import type { SubtitleMessagePayloadModel } from '@/model/subtitle';
    import { listenEvent } from '@/service/tauri/command';

    defineOptions({
        name: 'SubtitleOverlay'
    });

    const payload = ref<SubtitleMessagePayloadModel>({ state: 'idle', text: '', visible: false });
    const statusText = computed(() => {
        if (payload.value.state === 'error') return '字幕异常';
        if (payload.value.state === 'listening') return '实时字幕';
        if (payload.value.state === 'starting') return '启动中';
        return '未监听';
    });

    onMounted(async () => {
        await listenEvent<SubtitleMessagePayloadModel>('subtitle-message', (message) => {
            payload.value = message;
        });
    });
</script>
