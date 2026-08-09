<script setup lang="ts">
    import { toast } from 'vue-sonner';

    import { CLIENT_UNAVAILABLE_VOICE_MESSAGE } from '@/service/tauri/command';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'VoicePolishClientUnavailableDialog'
    });

    const store = useVoicePolishStore();

    watch(
        () => store.clientUnavailableDialogOpen,
        (open) => {
            if (!open) return;
            toast.warning('无法转换语音', {
                description: CLIENT_UNAVAILABLE_VOICE_MESSAGE
            });
            store.clientUnavailableDialogOpen = false;
        }
    );
</script>

<template></template>
