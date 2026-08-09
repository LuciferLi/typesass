<template>
    <main class="h-full bg-transparent p-3">
        <ui-card class="flex h-full flex-col p-4">
            <div class="flex items-center justify-between">
                <div>
                    <div class="text-[15px] font-semibold text-foreground">字幕历史</div>
                    <div class="mt-1 text-[12px] text-muted-foreground">{{ status }}</div>
                </div>
                <ui-badge :variant="listening ? 'secondary' : 'outline'">
                    {{ listening ? '监听中' : '已停止' }}
                </ui-badge>
            </div>
            <div class="mt-3 min-h-0 flex-1 overflow-y-auto">
                <ui-alert
                    v-for="item in items"
                    :key="item.id"
                    class="border-x-0 border-t-0 bg-transparent px-0 shadow-none hover:bg-transparent">
                    <div class="text-[11px] text-muted-foreground">
                        {{ new Date(item.createdAt).toLocaleTimeString() }}
                    </div>
                    <div class="mt-1 text-[13px] leading-5 text-foreground">{{ item.text }}</div>
                </ui-alert>
                <ui-alert
                    v-if="!items.length"
                    class="border-0 pt-12"
                    >还没有字幕。</ui-alert
                >
            </div>
        </ui-card>
    </main>
</template>

<script setup lang="ts">
    import { Alert as UiAlert } from '@/components/ui/alert';
    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Card as UiCard } from '@/components/ui/card';
    import type { SubtitleHistoryItemModel, SubtitleHistoryUpdatePayloadModel } from '@/model/subtitle';
    import { listenEvent } from '@/service/tauri/command';

    defineOptions({
        name: 'SubtitleHistoryWindow'
    });

    const items = ref<SubtitleHistoryItemModel[]>([]);
    const status = ref('等待字幕');
    const listening = ref(false);

    onMounted(async () => {
        await listenEvent<SubtitleHistoryUpdatePayloadModel>('subtitle-history-updated', (payload) => {
            items.value = payload.items;
            status.value = payload.status;
            listening.value = payload.listening;
        });
    });
</script>
