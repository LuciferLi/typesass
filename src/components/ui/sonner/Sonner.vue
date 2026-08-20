<script setup lang="ts">
    import 'vue-sonner/style.css';

    import { Toaster, type ToasterProps } from 'vue-sonner';

    const props = withDefaults(defineProps<ToasterProps>(), {
        theme: 'system',
        position: 'top-right',
        richColors: true,
        closeButton: true
    });

    defineOptions({
        name: 'UiSonner'
    });
</script>

<template>
    <Toaster v-bind="props" />
</template>
