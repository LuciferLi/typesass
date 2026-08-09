<script setup lang="ts">
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    /**
     * Item 媒体区属性。
     * 业务含义：展示列表项左侧图标、头像或自定义媒体内容。
     */
    interface ItemMediaProps {
        /** 媒体区视觉类型。 */
        variant?: 'default' | 'icon';
        /** 外部追加的 Tailwind 类名。 */
        class?: HTMLAttributes['class'];
    }

    const props = withDefaults(defineProps<ItemMediaProps>(), {
        variant: 'default'
    });
</script>

<template>
    <div
        data-slot="item-media"
        :class="
            cn(
                'flex shrink-0 items-center justify-center text-muted-foreground',
                variant === 'icon' && 'mt-0.5 size-8 rounded-md border border-border bg-muted/45',
                props.class
            )
        ">
        <slot />
    </div>
</template>
