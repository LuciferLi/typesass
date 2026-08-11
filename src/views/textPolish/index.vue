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
                <ui-field-label>文本模型</ui-field-label>
                <ui-select-root v-model="selectedTextModelId">
                    <ui-select-trigger>
                        <ui-select-value placeholder="暂无可用文本模型" />
                    </ui-select-trigger>
                    <ui-select-content>
                        <ui-select-item
                            v-for="model in textModels"
                            :key="model.id"
                            :value="model.id">
                            {{ model.displayName }}
                        </ui-select-item>
                    </ui-select-content>
                </ui-select-root>
                <ui-field-description>选择服务目录中已启用的模型，业务请求只发送不透明模型 ID。</ui-field-description>
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
    const modelManageStore = useModelManageStore();
    const textModels = computed(() => modelManageStore.enabledServiceModels('text'));
    const selectedTextModelId = computed({
        get: () => store.textModelId,
        set: (modelId: string) => {
            store.textModelId = modelId;
            store.persistTextPolish();
        }
    });

    /**
     * 初始化文本润色模型选择。
     * 流程：读取公共安全目录，校正已保存 ID 并持久化服务端默认回退结果。
     * 参数：无。
     * 返回：初始化完成 Promise。
     * 边界：目录不可达或无文本能力时保留空 ID并展示明确提示，润色动作会阻止请求。
     */
    async function initializeTextModelSelection(): Promise<void> {
        await modelManageStore.hydrateModelManage();
        const selection = modelManageStore.resolveSelection('text', store.textModelId, '文本润色');
        store.textModelId = selection.modelId;
        store.persistTextPolish();
        store.message = modelManageStore.message || selection.message;
    }

    onMounted(() => {
        void initializeTextModelSelection();
    });
</script>
