<template>
    <ui-dialog v-model:open="open">
        <ui-dialog-content>
            <ui-dialog-header>
                <ui-dialog-title>添加词条</ui-dialog-title>
                <ui-dialog-description class="sr-only">输入语音润色需要优先识别的词条。</ui-dialog-description>
            </ui-dialog-header>
            <form @submit.prevent="handleSubmit">
                <ui-field-group>
                    <ui-field>
                        <ui-field-label>词条</ui-field-label>
                        <ui-textarea
                            v-model="input"
                            class="min-h-[108px]"
                            placeholder="请输入词条，多个词条可换行输入。"
                            autofocus />
                        <ui-field-description
                            >用于语音转文字润色时优先保留专有名词、产品名或常用表达。</ui-field-description
                        >
                    </ui-field>
                </ui-field-group>
                <ui-dialog-footer class="mt-6">
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="open = false"
                        >取消</ui-button
                    >
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="handleSaveAndContinue"
                        >保存并添加下一条</ui-button
                    >
                    <ui-button type="submit">保存</ui-button>
                </ui-dialog-footer>
            </form>
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
    import { Textarea as UiTextarea } from '@/components/ui/textarea';

    defineOptions({
        name: 'VoicePolishDictionaryDialog'
    });

    const emit = defineEmits<{
        // 提交词条输入文本，调用方负责拆分和去重。
        submit: [value: string];
    }>();

    const open = defineModel<boolean>('open', { default: false });
    const input = ref('');

    /**
     * 保存当前词条输入。
     * 流程：先裁剪输入内容，再通过 submit 事件交给调用方拆分、去重并持久化。
     * 参数：无。
     * 返回：保存成功时返回 true，空输入时返回 false。
     * 边界：空输入不触发保存，避免写入无效词条。
     */
    function saveCurrentInput(): boolean {
        const normalizedInput = input.value.trim();
        if (!normalizedInput) return false;
        emit('submit', normalizedInput);
        input.value = '';
        return true;
    }

    /**
     * 保存词条并继续添加下一条。
     * 流程：复用当前保存逻辑，保存成功后保留弹窗打开，方便连续录入。
     * 参数：无。
     * 返回：无返回值。
     * 边界：空输入不会关闭弹窗，也不会触发保存。
     */
    function handleSaveAndContinue(): void {
        saveCurrentInput();
    }

    /**
     * 提交词条后关闭弹窗。
     * 流程：保存当前输入，保存成功后关闭弹窗，避免重复添加同一批文本。
     * 参数：无。
     * 返回：无返回值。
     * 边界：空输入不关闭弹窗，方便用户继续补充内容。
     */
    function handleSubmit(): void {
        if (saveCurrentInput()) {
            open.value = false;
        }
    }
</script>
