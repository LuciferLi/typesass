<template>
    <ui-dialog v-model:open="open">
        <ui-dialog-content>
            <ui-dialog-header>
                <ui-dialog-title>{{ dialogTitle }}</ui-dialog-title>
                <ui-dialog-description>{{ dialogDescription }}</ui-dialog-description>
            </ui-dialog-header>
            <ui-field-group class="py-2">
                <ui-field v-if="mode === 'asr' || mode === 'all'">
                    <ui-field-label>ASR 模型</ui-field-label>
                    <ui-select-root v-model="asrModelId">
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
                    <ui-field-description>用于把语音内容识别成文字。</ui-field-description>
                </ui-field>
                <ui-field v-if="mode === 'text' || mode === 'all'">
                    <ui-field-label>润色模型</ui-field-label>
                    <ui-select-root v-model="textModelId">
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
                    <ui-field-description>用于整理和润色识别后的文字内容。</ui-field-description>
                </ui-field>
                <ui-field v-if="mode === 'text' || mode === 'all'">
                    <ui-field-label>输出偏好</ui-field-label>
                    <ui-textarea
                        v-model="store.styleInstruction"
                        class="min-h-[96px]"
                        placeholder="例如：保留我的语气，但去掉重复和明显口误。"
                        @blur="store.persistVoicePolish" />
                    <ui-field-description>描述希望保留或调整的表达风格，失焦后自动保存。</ui-field-description>
                </ui-field>
            </ui-field-group>
            <ui-dialog-footer>
                <ui-button
                    type="button"
                    @click="open = false"
                    >完成</ui-button
                >
            </ui-dialog-footer>
        </ui-dialog-content>
    </ui-dialog>
</template>

<script setup lang="ts">
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import {
        Field as UiField,
        FieldDescription as UiFieldDescription,
        FieldGroup as UiFieldGroup,
        FieldLabel as UiFieldLabel
    } from '@/components/ui/field';
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import { Textarea as UiTextarea } from '@/components/ui/textarea';
    import { useModelManageStore } from '@/stores/modelManage';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'VoicePolishModelSettingDialog'
    });

    /**
     * 语音润色模型设置弹窗模式。
     * 业务含义：用于控制弹窗当前展示 ASR 设置、润色模型设置，或完整模型配置。
     */
    type VoicePolishModelSettingMode = 'asr' | 'text' | 'all';

    const props = withDefaults(
        defineProps<{
            /**
             * 弹窗展示模式。
             * 业务含义：ASR 入口只维护识别模型，润色入口只维护文本模型和输出偏好，完整入口同时维护两者。
             */
            mode?: VoicePolishModelSettingMode;
        }>(),
        {
            mode: 'all'
        }
    );

    const open = defineModel<boolean>('open', { default: false });
    const store = useVoicePolishStore();
    const modelStore = useModelManageStore();
    const dialogTitle = computed(() => {
        if (props.mode === 'asr') return 'ASR 模型设置';
        if (props.mode === 'text') return '润色模型设置';
        return '设置模型';
    });
    const dialogDescription = computed(() => {
        if (props.mode === 'asr') return '选择语音转文字润色时用于识别语音的 ASR 模型。';
        if (props.mode === 'text') return '选择语音转文字润色时用于整理和润色文本的模型。';
        return '选择语音转文字润色使用的 ASR 模型和润色模型。';
    });
    const asrModelId = computed({
        get: () => store.selectedAsrModelId,
        set: (value: string) => store.updateModelSelection(value, store.selectedTextModelId)
    });
    const textModelId = computed({
        get: () => store.selectedTextModelId,
        set: (value: string) => store.updateModelSelection(store.selectedAsrModelId, value)
    });
</script>
