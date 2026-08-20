<script setup lang="ts">
    import type { PrimitiveProps } from 'reka-ui';
    import { Primitive } from 'reka-ui';
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    /**
     * Item 组件属性。
     * 业务含义：提供 shadcn 风格列表项容器，支持普通列表、描边列表和静音背景列表。
     */
    interface ItemProps extends PrimitiveProps {
        /** 列表项视觉样式。 */
        variant?: 'default' | 'outline' | 'muted';
        /** 列表项尺寸密度。 */
        size?: 'default' | 'sm';
        /** 外部追加的 Tailwind 类名。 */
        class?: HTMLAttributes['class'];
    }

    const props = withDefaults(defineProps<ItemProps>(), {
        as: 'div',
        variant: 'default',
        size: 'default'
    });
</script>

<template>
    <Primitive
        :as="as"
        :as-child="asChild"
        data-slot="item"
        :class="
            cn(
                'group/item flex w-full min-w-0 items-start gap-3 rounded-lg text-left outline-none transition-colors',
                'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background',
                variant === 'outline' && 'border border-border bg-card',
                variant === 'muted' && 'bg-muted/35',
                size === 'default' && 'p-3',
                size === 'sm' && 'p-2.5',
                props.class
            )
        ">
        <slot />
    </Primitive>
</template>
