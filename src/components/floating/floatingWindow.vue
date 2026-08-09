<template>
    <main
        class="flex h-full items-center justify-center bg-transparent"
        data-tauri-drag-region>
        <ui-card class="flex h-[46px] w-[132px] items-center justify-center px-3">
            <ui-button
                class="rounded-full"
                size="icon"
                variant="secondary"
                type="button"
                title="触发语音润色"
                @click="voiceStore.runVoicePolish(targetApp)">
                <microphone
                    theme="outline"
                    size="16" />
            </ui-button>
            <div class="mx-2 flex items-center gap-1">
                <span class="h-1.5 w-1.5 rounded-full bg-foreground/80"></span>
                <span class="h-2.5 w-1.5 rounded-full bg-muted-foreground"></span>
                <span class="h-1.5 w-1.5 rounded-full bg-foreground/80"></span>
            </div>
            <ui-button
                class="rounded-full"
                size="icon"
                variant="outline"
                type="button"
                title="确认"
                @click="voiceStore.runVoicePolish(targetApp)">
                <check-small
                    theme="outline"
                    size="16" />
            </ui-button>
        </ui-card>
    </main>
</template>

<script setup lang="ts">
    import { CheckSmall, Microphone } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import { Card as UiCard } from '@/components/ui/card';
    import { useTextPolishStore } from '@/stores/textPolish';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'FloatingWindow'
    });

    const voiceStore = useVoicePolishStore();
    const textStore = useTextPolishStore();
    const targetApp = ref('');

    onMounted(() => {
        const nativeWindow = window as Window & {
            __AIToolHandleShortcutMode?: (mode: string, app: string) => void;
            __AIToolPendingShortcutMode?: { mode: string; targetApp: string };
        };
        nativeWindow.__AIToolHandleShortcutMode = (mode: string, app: string) => {
            targetApp.value = app;
            if (mode === 'polish') void textStore.polishSelectedText();
            if (mode === 'asr') void voiceStore.runVoicePolish(app, 'asr');
            if (mode === 'dictate') void voiceStore.runVoicePolish(app);
        };
        if (nativeWindow.__AIToolPendingShortcutMode) {
            nativeWindow.__AIToolHandleShortcutMode(
                nativeWindow.__AIToolPendingShortcutMode.mode,
                nativeWindow.__AIToolPendingShortcutMode.targetApp
            );
        }
    });
</script>
