<script setup lang="ts">
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    /** KbdGroup 组合键容器属性，用于横向展示一组快捷键片段。 */
    const props = defineProps<{
        /** 额外类名，供业务调整组合键的间距或换行策略。 */
        class?: HTMLAttributes['class'];
    }>();
</script>

<template>
    <span :class="cn('inline-flex items-center gap-1', props.class)">
        <slot />
    </span>
</template>
