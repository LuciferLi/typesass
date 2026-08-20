<script setup lang="ts">
    import { reactiveOmit } from '@vueuse/core';
    import type { DropdownMenuContentEmits, DropdownMenuContentProps } from 'reka-ui';
    import { DropdownMenuContent, DropdownMenuPortal, useForwardPropsEmits } from 'reka-ui';
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    defineOptions({
        inheritAttrs: false
    });

    const props = withDefaults(defineProps<DropdownMenuContentProps & { class?: HTMLAttributes['class'] }>(), {
        align: 'end',
        sideOffset: 8
    });
    const emits = defineEmits<DropdownMenuContentEmits>();
    const delegatedProps = reactiveOmit(props, 'class');
    const forwarded = useForwardPropsEmits(delegatedProps, emits);
</script>

<template>
    <DropdownMenuPortal>
        <DropdownMenuContent
            v-bind="{ ...forwarded, ...$attrs }"
            :class="
                cn(
                    'z-50 min-w-44 overflow-hidden rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-xl shadow-background/50 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2',
                    props.class
                )
            ">
            <slot />
        </DropdownMenuContent>
    </DropdownMenuPortal>
</template>
