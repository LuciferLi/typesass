<script setup lang="ts">
    import { reactiveOmit } from '@vueuse/core';
    import { X } from 'lucide-vue-next';
    import type { DialogContentEmits, DialogContentProps } from 'reka-ui';
    import { DialogClose, DialogContent, DialogOverlay, DialogPortal, useForwardPropsEmits } from 'reka-ui';
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    const props = defineProps<DialogContentProps & { class?: HTMLAttributes['class'] }>();
    const emits = defineEmits<DialogContentEmits>();

    const delegatedProps = reactiveOmit(props, 'class');

    const forwarded = useForwardPropsEmits(delegatedProps, emits);

    /**
     * 阻止点击遮罩层关闭滚动弹窗。
     * 流程：Reka Dialog 触发外部点击事件时取消默认关闭行为，让长表单只能通过明确按钮关闭。
     * 参数：event 为 DialogContent 的外部指针事件。
     * 返回：无返回值；边界为滚动内容或表单内容较长时避免误触遮罩丢失填写内容。
     */
    function preventOutsidePointerClose(event: Event): void {
        event.preventDefault();
    }
</script>

<template>
    <DialogPortal>
        <DialogOverlay
            class="fixed inset-0 z-50 grid place-items-center overflow-y-auto bg-overlay/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0">
            <DialogContent
                :class="
                    cn(
                        'relative z-50 grid w-full max-w-lg my-8 gap-4 border border-border bg-popover p-6 text-popover-foreground shadow-sm duration-200 sm:rounded-lg md:w-full',
                        props.class
                    )
                "
                v-bind="forwarded"
                @pointer-down-outside="preventOutsidePointerClose">
                <slot />

                <DialogClose class="absolute top-4 right-4 p-0.5 transition-colors rounded-md hover:bg-secondary">
                    <X class="w-4 h-4" />
                    <span class="sr-only">Close</span>
                </DialogClose>
            </DialogContent>
        </DialogOverlay>
    </DialogPortal>
</template>
