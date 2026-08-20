<script setup lang="ts">
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    const props = defineProps<{
        /** 外部追加的 Tailwind 类名。 */
        class?: HTMLAttributes['class'];
    }>();
</script>

<template>
    <div
        data-slot="item-group"
        :class="cn('grid gap-2', props.class)">
        <slot />
    </div>
</template>
