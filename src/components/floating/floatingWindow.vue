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
                @click="voiceStore.runVoicePolish()">
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
                @click="voiceStore.runVoicePolish()">
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
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'FloatingWindow'
    });

    const voiceStore = useVoicePolishStore();
</script>
