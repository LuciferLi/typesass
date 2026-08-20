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
        data-slot="item-content"
        :class="cn('grid min-w-0 flex-1 gap-1', props.class)">
        <slot />
    </div>
</template>
