<template>
    <main class="flex h-full items-center justify-center bg-transparent">
        <ui-card class="mx-3 w-full px-4 py-3">
            <div class="text-[13px] font-medium">CodexMan</div>
            <div
                v-if="message"
                class="mt-1 line-clamp-2 text-[13px] leading-5 text-muted-foreground">
                {{ message }}
            </div>
        </ui-card>
    </main>
</template>

<script setup lang="ts">
    import { Card as UiCard } from '@/components/ui/card';
    import { listenEvent } from '@/service/tauri/command';

    defineOptions({
        name: 'ToastWindow'
    });

    const message = ref('');

    onMounted(async () => {
        await listenEvent<{ message: string }>('toast-message', (payload) => {
            message.value = payload.message;
        });
    });
</script>
