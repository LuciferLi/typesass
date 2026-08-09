<script lang="ts">
    export default {
        name: 'UiAccordionItem'
    };
</script>

<script setup lang="ts">
    import { reactiveOmit } from '@vueuse/core';
    import type { AccordionItemProps } from 'reka-ui';
    import { AccordionItem } from 'reka-ui';
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    const props = defineProps<AccordionItemProps & { class?: HTMLAttributes['class'] }>();

    const delegatedProps = reactiveOmit(props, 'class');
</script>

<template>
    <AccordionItem
        v-bind="delegatedProps"
        :class="cn('border-b border-border', props.class)">
        <slot />
    </AccordionItem>
</template>
