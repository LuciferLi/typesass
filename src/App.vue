<template>
    <ui-tooltip-provider>
        <floatingWindow v-if="windowMode === 'main'" />
        <toastWindow v-else-if="windowMode === 'toast'" />
        <resultWindow v-else-if="windowMode === 'result'" />
        <mainLayout v-else />
        <ui-toaster />
    </ui-tooltip-provider>
</template>

<script setup lang="ts">
    import FloatingWindow from '@/components/floating/floatingWindow.vue';
    import ResultWindow from '@/components/floating/resultWindow.vue';
    import ToastWindow from '@/components/floating/toastWindow.vue';
    import MainLayout from '@/components/layout/mainLayout.vue';
    import { Toaster as UiToaster } from '@/components/ui/sonner';
    import { TooltipProvider as UiTooltipProvider } from '@/components/ui/tooltip';
    import type { AppWindowModeType } from '@/model/app';

    defineOptions({
        name: 'App'
    });

    const params = new URLSearchParams(window.location.search);
    const mode = params.get('mode');
    const initialWindowMode: AppWindowModeType =
        mode === 'main' || mode === 'toast' || mode === 'result' ? mode : 'hub';
    document.documentElement.dataset.windowMode = initialWindowMode;
    const windowMode = computed<AppWindowModeType>(() => {
        return initialWindowMode;
    });
</script>
