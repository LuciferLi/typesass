<script setup lang="ts">
    import type { DropdownMenuItemProps } from 'reka-ui';
    import { DropdownMenuItem } from 'reka-ui';
    import type { HTMLAttributes } from 'vue';

    import { cn } from '@/lib/utils';

    const props = defineProps<DropdownMenuItemProps & { class?: HTMLAttributes['class'] }>();
</script>

<template>
    <DropdownMenuItem
        v-bind="{ ...props, class: undefined }"
        :class="
            cn(
                'relative flex cursor-default select-none items-center gap-2 rounded-md px-2.5 py-2 text-[13px] outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&>svg]:size-4 [&>svg]:shrink-0',
                props.class
            )
        ">
        <slot />
    </DropdownMenuItem>
</template>
