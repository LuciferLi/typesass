<script setup lang="ts">
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    const props = defineProps<{
        /** 外部追加的 Tailwind 类名。 */
        class?: HTMLAttributes['class'];
    }>();
</script>

<template>
    <p
        data-slot="item-description"
        :class="cn('text-[12px] leading-5 text-muted-foreground', props.class)">
        <slot />
    </p>
</template>
