<script lang="ts">
    export default {
        name: 'UiAccordionTrigger'
    };
</script>

<script setup lang="ts">
    import { Down } from '@icon-park/vue-next';
    import { reactiveOmit } from '@vueuse/core';
    import type { AccordionTriggerProps } from 'reka-ui';
    import { AccordionHeader, AccordionTrigger, useForwardProps } from 'reka-ui';
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    const props = defineProps<AccordionTriggerProps & { class?: HTMLAttributes['class'] }>();

    const delegatedProps = reactiveOmit(props, 'class');
    const forwardedProps = useForwardProps(delegatedProps);
</script>

<template>
    <AccordionHeader class="flex">
        <AccordionTrigger
            v-bind="forwardedProps"
            :class="
                cn(
                    'group flex flex-1 items-center justify-between gap-4 py-4 text-left text-sm font-medium transition-all hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50',
                    props.class
                )
            ">
            <slot />
            <Down
                class="h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200 group-data-[state=open]:rotate-180" />
        </AccordionTrigger>
    </AccordionHeader>
</template>
