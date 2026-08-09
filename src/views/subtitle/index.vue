<template>
    <div class="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <section class="grid gap-4">
            <div class="flex justify-end">
                <ui-button
                    :variant="store.listening ? 'destructive' : 'outline'"
                    type="button"
                    @click="store.toggleSubtitle">
                    {{ store.listening ? '停止字幕' : '开始字幕' }}
                </ui-button>
            </div>
            <ui-field>
                <ui-field-label>ASR 模型</ui-field-label>
                <ui-select-root v-model="selectedAsrModelId">
                    <ui-select-trigger>
                        <ui-select-value placeholder="选择模型" />
                    </ui-select-trigger>
                    <ui-select-content>
                        <ui-select-item
                            v-for="model in modelStore.groupModels('asr')"
                            :key="model.id"
                            :value="model.id">
                            {{ model.name }} · {{ model.model }}
                        </ui-select-item>
                    </ui-select-content>
                </ui-select-root>
                <ui-field-description>选择用于实时字幕识别的语音模型。</ui-field-description>
            </ui-field>
            <ui-alert class="p-5">
                <div class="text-[13px] font-medium text-muted-foreground">当前状态</div>
                <div class="mt-2 text-[24px] font-semibold text-foreground">{{ statusText }}</div>
                <div
                    v-if="store.runtimeState === 'starting'"
                    class="mt-4">
                    <ui-skeleton class="h-5 w-[80%]" />
                    <ui-skeleton class="mt-3 h-5 w-[46%]" />
                </div>
                <p
                    v-else
                    class="mt-3 min-h-[56px] whitespace-pre-wrap text-[16px] leading-7 text-muted-foreground">
                    {{ store.currentText || '等待字幕文本。' }}
                </p>
            </ui-alert>
        </section>

        <section class="grid gap-3">
            <div class="flex justify-end">
                <ui-button
                    variant="ghost"
                    size="sm"
                    type="button"
                    @click="store.clearHistory"
                    >清空</ui-button
                >
            </div>
            <div class="grid max-h-[520px] gap-3 overflow-y-auto">
                <ui-alert
                    v-for="item in store.history"
                    :key="item.id">
                    <div class="text-[12px] text-muted-foreground">{{ new Date(item.createdAt).toLocaleString() }}</div>
                    <div class="mt-2 text-[14px] leading-6 text-foreground">{{ item.text }}</div>
                </ui-alert>
                <ui-alert
                    v-if="!store.history.length"
                    class="py-4"
                    >还没有字幕历史。</ui-alert
                >
            </div>
        </section>
    </div>
</template>

<script setup lang="ts">
    import { Alert as UiAlert } from '@/components/ui/alert';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Field as UiField,
        FieldDescription as UiFieldDescription,
        FieldLabel as UiFieldLabel
    } from '@/components/ui/field';
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import { Skeleton as UiSkeleton } from '@/components/ui/skeleton';
    import { useModelManageStore } from '@/stores/modelManage';
    import { useSubtitleStore } from '@/stores/subtitle';

    defineOptions({
        name: 'SubtitleView'
    });

    const store = useSubtitleStore();
    const modelStore = useModelManageStore();
    const selectedAsrModelId = computed({
        get: () => store.selectedAsrModelId,
        set: (value: string) => store.updateAsrModel(value)
    });
    const statusText = computed(() => {
        if (store.runtimeState === 'starting') return '启动中';
        if (store.runtimeState === 'listening') return '监听中';
        if (store.runtimeState === 'error') return '异常';
        return '未开启';
    });
</script>
