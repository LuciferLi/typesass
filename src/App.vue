<template>
    <ui-tooltip-provider>
        <floatingWindow v-if="windowMode === 'main'" />
        <toastWindow v-else-if="windowMode === 'toast'" />
        <resultWindow v-else-if="windowMode === 'result'" />
        <subtitleOverlay v-else-if="windowMode === 'subtitle'" />
        <subtitleHistoryWindow v-else-if="windowMode === 'subtitleHistory'" />
        <mainLayout v-else />
        <ui-toaster />
        <voice-polish-client-unavailable-dialog />
    </ui-tooltip-provider>
</template>

<script setup lang="ts">
    import FloatingWindow from '@/components/floating/floatingWindow.vue';
    import ResultWindow from '@/components/floating/resultWindow.vue';
    import ToastWindow from '@/components/floating/toastWindow.vue';
    import MainLayout from '@/components/layout/mainLayout.vue';
    import SubtitleHistoryWindow from '@/components/subtitle/subtitleHistoryWindow.vue';
    import SubtitleOverlay from '@/components/subtitle/subtitleOverlay.vue';
    import { Toaster as UiToaster } from '@/components/ui/sonner';
    import { TooltipProvider as UiTooltipProvider } from '@/components/ui/tooltip';
    import VoicePolishClientUnavailableDialog from '@/components/voicePolish/clientUnavailableDialog.vue';
    import type { AppWindowModeType } from '@/model/app';

    defineOptions({
        name: 'App'
    });

    const params = new URLSearchParams(window.location.search);
    const mode = params.get('mode');
    const windowMode = computed<AppWindowModeType>(() => {
        if (
            mode === 'main' ||
            mode === 'toast' ||
            mode === 'result' ||
            mode === 'subtitle' ||
            mode === 'subtitleHistory'
        ) {
            return mode;
        }
        return 'hub';
    });
</script>
