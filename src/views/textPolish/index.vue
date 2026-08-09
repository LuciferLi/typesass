<template>
    <div class="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <section class="grid gap-4">
            <div class="flex justify-end">
                <ui-button
                    variant="outline"
                    type="button"
                    :disabled="store.running"
                    @click="store.polishSelectedText">
                    选中文本
                </ui-button>
            </div>
            <ui-field>
                <ui-field-label>文本大模型</ui-field-label>
                <ui-select-root v-model="selectedTextModelId">
                    <ui-select-trigger>
                        <ui-select-value placeholder="选择模型" />
                    </ui-select-trigger>
                    <ui-select-content>
                        <ui-select-item
                            v-for="model in modelStore.groupModels('text')"
                            :key="model.id"
                            :value="model.id">
                            {{ model.name }} · {{ model.model }}
                        </ui-select-item>
                    </ui-select-content>
                </ui-select-root>
                <ui-field-description>选择用于润色文字的文本大模型。</ui-field-description>
            </ui-field>
            <ui-field>
                <ui-field-label>实时输入</ui-field-label>
                <ui-textarea
                    v-model="store.inputText"
                    class="min-h-[160px]"
                    placeholder="在这里输入文字，点击下方按钮润色。" />
                <ui-field-description>输入需要润色的文字内容。</ui-field-description>
            </ui-field>
            <ui-field>
                <ui-field-label>输出偏好</ui-field-label>
                <ui-textarea
                    v-model="store.styleInstruction"
                    class="min-h-[80px]"
                    placeholder="例如：更简洁、更适合发给同事，不改变事实。" />
                <ui-field-description>描述希望输出结果遵循的语气、长度或场景。</ui-field-description>
            </ui-field>
            <div class="flex items-center gap-3">
                <ui-button
                    variant="outline"
                    type="button"
                    :disabled="store.running"
                    @click="store.polishInputText">
                    {{ store.running ? '润色中' : '润色输入' }}
                </ui-button>
            </div>
            <ui-alert v-if="store.message">
                {{ store.message }}
            </ui-alert>
            <ui-alert v-if="store.running">
                <ui-skeleton class="h-4 w-[38%]" />
                <ui-skeleton class="mt-3 h-4 w-full" />
                <ui-skeleton class="mt-2 h-4 w-[66%]" />
            </ui-alert>
            <ui-alert v-else-if="store.outputText">
                <div class="mb-2 text-[13px] font-medium text-foreground">输出结果</div>
                <p class="whitespace-pre-wrap text-[14px] leading-6 text-foreground">{{ store.outputText }}</p>
            </ui-alert>
        </section>

        <section class="grid max-h-[520px] gap-3 overflow-y-auto">
            <ui-alert
                v-for="item in store.history"
                :key="item.id">
                <div class="text-[12px] text-muted-foreground">{{ new Date(item.createdAt).toLocaleString() }}</div>
                <div class="mt-2 line-clamp-3 text-[13px] text-muted-foreground">{{ item.sourceText }}</div>
                <div class="mt-2 line-clamp-4 text-[14px] leading-6 text-foreground">{{ item.outputText }}</div>
            </ui-alert>
            <ui-alert
                v-if="!store.history.length"
                class="py-4"
                >还没有润色历史。</ui-alert
            >
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
    import { Textarea as UiTextarea } from '@/components/ui/textarea';
    import { useModelManageStore } from '@/stores/modelManage';
    import { useTextPolishStore } from '@/stores/textPolish';

    defineOptions({
        name: 'TextPolishView'
    });

    const store = useTextPolishStore();
    const modelStore = useModelManageStore();
    const selectedTextModelId = computed({
        get: () => store.selectedTextModelId,
        set: (value: string) => store.updateTextModel(value)
    });
</script>
