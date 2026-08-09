<script setup lang="ts">
    import type { Component, HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    interface PageStateProps {
        // 状态图标组件，用于表达空态、禁用态或异常态的视觉语义。
        icon?: Component;
        // 状态标题，用于告诉用户当前页面处于什么状态。
        title: string;
        // 状态说明，用于说明原因、后续记录出现方式或下一步操作。
        description: string;
        // 外层容器类名，用于业务页面在不改变组件结构的情况下调整间距。
        class?: HTMLAttributes['class'];
    }

    const props = defineProps<PageStateProps>();

    defineOptions({
        name: 'UiPageState'
    });
</script>

<template>
    <div
        :class="
            cn(
                'flex min-h-[280px] w-full flex-col items-center justify-center rounded-lg border border-dashed bg-muted/20 px-6 py-12 text-center',
                props.class
            )
        ">
        <div
            v-if="props.icon"
            class="mb-4 flex size-12 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <component
                :is="props.icon"
                theme="outline"
                size="24" />
        </div>
        <div class="text-[15px] font-medium text-foreground">{{ props.title }}</div>
        <p class="mt-2 max-w-[460px] text-[13px] leading-6 text-muted-foreground">
            {{ props.description }}
        </p>
        <div
            v-if="$slots.action"
            class="mt-5">
            <slot name="action" />
        </div>
    </div>
</template>
