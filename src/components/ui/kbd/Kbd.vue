<script setup lang="ts">
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    /** Kbd 按键组件属性，用于展示单个快捷键按键片段。 */
    const props = defineProps<{
        /** 额外类名，供业务在特殊密度或状态下微调展示。 */
        class?: HTMLAttributes['class'];
    }>();
</script>

<template>
    <kbd
        :class="
            cn(
                'pointer-events-none inline-flex h-7 min-w-7 items-center justify-center rounded-md border border-border bg-muted px-2 text-[12px] font-medium leading-none text-foreground shadow-sm',
                props.class
            )
        ">
        <slot />
    </kbd>
</template>
