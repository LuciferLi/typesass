<template>
    <section class="grid min-h-0 place-items-center border-l border-border bg-transparent p-8">
        <div class="grid max-w-sm justify-items-center gap-3 text-center">
            <div
                class="grid h-12 w-12 place-items-center rounded-lg border border-border bg-muted/40 text-muted-foreground">
                <doc-detail
                    theme="outline"
                    size="20" />
            </div>
            <div class="grid gap-1">
                <h2 class="text-[14px] font-medium text-foreground">会话内容区域</h2>
                <p class="text-[13px] leading-5 text-muted-foreground">这里先预留为空，后续再接入会话详情内容。</p>
            </div>
        </div>
    </section>
</template>

<script setup lang="ts">
    import { DocDetail } from '@icon-park/vue-next';

    defineOptions({
        name: 'SessionManageSessionContentPlaceholder'
    });
</script>
