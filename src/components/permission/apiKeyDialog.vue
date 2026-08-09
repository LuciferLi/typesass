<template>
    <ui-dialog v-model:open="open">
        <ui-dialog-content>
            <ui-dialog-header>
                <ui-dialog-title>API Key</ui-dialog-title>
                <ui-dialog-description class="sr-only">保存 OpenAI 兼容接口使用的 API Key。</ui-dialog-description>
            </ui-dialog-header>
            <form @submit.prevent="handleSave">
                <ui-field-group>
                    <ui-field>
                        <ui-field-label>OpenAI 兼容接口 API Key</ui-field-label>
                        <ui-input
                            v-model="apiKey"
                            type="password"
                            placeholder="请输入 OpenAI 兼容接口 API Key"
                            autocomplete="off"
                            autofocus />
                        <ui-field-description
                            >用于调用全局默认 OpenAI 兼容接口。模型管理中新增的模型会优先使用各自保存的 API
                            Key。</ui-field-description
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
                        type="submit"
                        :disabled="saving"
                        >{{ saving ? '保存中' : '保存' }}</ui-button
                    >
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
    import { Input as UiInput } from '@/components/ui/input';

    defineOptions({
        name: 'PermissionApiKeyDialog'
    });

    const props = defineProps<{
        // 当前是否正在保存 API Key。
        saving: boolean;
    }>();

    const emit = defineEmits<{
        // 保存 API Key。
        save: [value: string];
    }>();

    const open = defineModel<boolean>('open', { default: false });
    const apiKey = defineModel<string>('apiKey', { default: '' });

    // 将输入的 API Key 交给权限模块保存。
    function handleSave(): void {
        if (props.saving) return;
        emit('save', apiKey.value);
    }
</script>
