<template>
    <section class="grid gap-5">
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="grid gap-2">
                <ui-tabs
                    v-model="activeGroup"
                    class="w-fit justify-self-start">
                    <ui-tabs-list>
                        <ui-tabs-trigger
                            v-for="group in groups"
                            :key="group.key"
                            :value="group.key"
                            class="min-w-fit [&>span]:inline-flex [&>span]:items-center [&>span]:gap-1.5 [&>span]:overflow-visible [&>span]:whitespace-nowrap">
                            <component
                                :is="group.icon"
                                theme="outline"
                                size="14"
                                class="shrink-0" />
                            <span class="whitespace-nowrap">{{ group.label }}</span>
                            <ui-badge
                                variant="outline"
                                class="h-5 shrink-0 px-1.5"
                                >{{ store.groupModels(group.key).length }}</ui-badge
                            >
                        </ui-tabs-trigger>
                    </ui-tabs-list>
                </ui-tabs>
                <p class="text-[13px] leading-5 text-muted-foreground">{{ activeGroupDescription }}</p>
            </div>
            <ui-button
                v-if="activeModels.length"
                variant="outline"
                type="button"
                :disabled="store.saving"
                @click="handleOpenModelDialog">
                添加模型
            </ui-button>
        </div>

        <p
            v-if="store.message"
            class="text-[13px] leading-5 text-muted-foreground"
            role="status">
            {{ store.message }}
        </p>

        <ui-page-state
            v-if="!activeModels.length"
            :icon="activeGroupIcon"
            :title="emptyStateTitle"
            :description="emptyStateDescription">
            <template #action>
                <ui-button
                    type="button"
                    :disabled="store.saving"
                    @click="handleOpenModelDialog">
                    <plus
                        theme="outline"
                        size="16" />
                    <span>添加模型</span>
                </ui-button>
            </template>
        </ui-page-state>

        <div
            v-else
            class="overflow-hidden rounded-lg border border-border bg-card">
            <div
                class="grid grid-cols-[minmax(0,1fr)_140px_280px] gap-4 border-b border-border px-4 py-3 text-[12px] font-medium text-muted-foreground">
                <span>模型</span>
                <span>来源</span>
                <span class="text-right">操作</span>
            </div>
            <div
                v-if="activeModels.length"
                class="divide-y divide-border">
                <div
                    v-for="model in activeModels"
                    :key="model.id"
                    class="grid grid-cols-[minmax(0,1fr)_140px_280px] gap-4 px-4 py-4">
                    <div class="flex min-w-0 items-start gap-3">
                        <model-manage-vendor-mark
                            :vendor-key="model.provider"
                            :label="model.displayName" />
                        <div class="min-w-0">
                            <div class="truncate text-[14px] font-medium text-foreground">{{ model.displayName }}</div>
                            <div class="mt-1 truncate text-[12px] text-muted-foreground">{{ model.modelName }}</div>
                            <div class="mt-1 truncate text-[12px] text-muted-foreground">{{ model.baseUrl }}</div>
                        </div>
                    </div>
                    <div class="flex items-start">
                        <div class="grid gap-2">
                            <ui-badge variant="secondary">{{ model.provider || '自定义' }}</ui-badge>
                            <ui-badge
                                v-if="model.isDefault"
                                variant="outline">
                                默认
                            </ui-badge>
                        </div>
                    </div>
                    <div class="flex flex-wrap items-center justify-end gap-1">
                        <label class="flex items-center gap-2 text-[12px] text-muted-foreground">
                            <span>{{ model.enabled ? '已启用' : '已禁用' }}</span>
                            <ui-switch
                                :model-value="model.enabled"
                                :disabled="store.saving"
                                @update:model-value="handleToggleModel(model, $event)" />
                        </label>
                        <ui-button
                            v-if="!model.isDefault"
                            variant="ghost"
                            size="sm"
                            type="button"
                            :disabled="store.saving || !model.enabled"
                            @click="handleSetDefault(model)">
                            设为默认
                        </ui-button>
                        <ui-button
                            variant="ghost"
                            size="sm"
                            type="button"
                            :disabled="store.saving"
                            @click="handleOpenEditDialog(model)">
                            编辑
                        </ui-button>
                        <ui-button
                            variant="ghost"
                            size="sm"
                            type="button"
                            :disabled="store.saving"
                            @click="pendingRemoval = model">
                            删除
                        </ui-button>
                    </div>
                </div>
            </div>
        </div>

        <model-manage-model-form-dialog
            v-model:open="modelDialogOpen"
            :group="pendingEditing?.capability ?? activeGroup"
            :model="pendingEditing"
            :title="pendingEditing ? '编辑模型' : '添加模型'"
            :save-model="handleSaveModel" />
        <ui-dialog v-model:open="deleteDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>删除模型</ui-dialog-title>
                    <ui-dialog-description class="sr-only">确认删除当前选中的模型配置。</ui-dialog-description>
                </ui-dialog-header>
                <div class="text-sm text-muted-foreground">{{ pendingRemoval?.displayName }}</div>
                <ui-dialog-footer class="mt-5">
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="store.saving"
                        @click="pendingRemoval = null">
                        取消
                    </ui-button>
                    <ui-button
                        variant="destructive"
                        type="button"
                        :disabled="store.saving"
                        @click="handleRemoveModel">
                        {{ store.saving ? '删除中' : '删除' }}
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
    </section>
</template>

<script setup lang="ts">
    import { Magic, Microphone, Plus } from '@icon-park/vue-next';
    import type { Component } from 'vue';
    import { toast } from 'vue-sonner';

    import ModelManageModelFormDialog from '@/components/modelManage/modelFormDialog.vue';
    import ModelManageVendorMark from '@/components/modelManage/vendorMark.vue';
    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import { PageState as UiPageState } from '@/components/ui/pageState';
    import { Switch as UiSwitch } from '@/components/ui/switch';
    import { Tabs as UiTabs, TabsList as UiTabsList, TabsTrigger as UiTabsTrigger } from '@/components/ui/tabs';
    import type { ModelFormModel, ModelGroupType, PrivateModelItemModel } from '@/model/modelManage';
    import { useModelManageStore } from '@/stores/modelManage';

    defineOptions({
        name: 'ModelManageView'
    });

    const store = useModelManageStore();
    const modelDialogOpen = ref(false);
    const activeGroup = ref<ModelGroupType>('text');
    const pendingEditing = ref<PrivateModelItemModel | null>(null);
    const pendingRemoval = ref<PrivateModelItemModel | null>(null);
    const deleteDialogOpen = computed({
        get: () => Boolean(pendingRemoval.value),
        set: (value: boolean) => {
            if (!value) pendingRemoval.value = null;
        }
    });
    const groups: { key: ModelGroupType; label: string; icon: Component }[] = [
        { key: 'text', label: '文本大模型', icon: Magic },
        { key: 'asr', label: 'ASR', icon: Microphone }
    ];
    const activeModels = computed(() => store.groupModels(activeGroup.value));
    const activeGroupIcon = computed(() => {
        if (activeGroup.value === 'text') return Magic;
        return Microphone;
    });
    const activeGroupDescription = computed(() => {
        if (activeGroup.value === 'text') {
            return '文本大模型用于润色，以及语音转文字后的内容整理和润色。';
        }
        return 'ASR 用于语音转文字和语音转文字润色中的语音识别。';
    });
    const emptyStateTitle = computed(() => {
        if (activeGroup.value === 'text') return '还没有文本大模型';
        return '还没有 ASR 模型';
    });
    const emptyStateDescription = computed(() => {
        if (activeGroup.value === 'text') {
            return '添加文本大模型后，润色和语音转文字后的内容整理会使用这里的模型。';
        }
        return '添加 ASR 模型后，语音转文字和语音转文字润色会使用这里的模型识别声音。';
    });

    /**
     * 弹出模型管理操作失败提示。
     * 流程：优先展示 Error 中的安全错误说明；未知异常使用兜底文案。
     * 参数：title 为短提示标题，error 为捕获异常，fallbackDescription 为兜底说明。
     * 返回：无返回值。
     * 边界：只处理用户主动操作失败，目录加载失败仍保留页面状态说明。
     */
    function showModelOperationError(title: string, error: unknown, fallbackDescription: string): void {
        toast.error(title, {
            description: error instanceof Error ? error.message : fallbackDescription
        });
    }

    /**
     * 打开添加模型弹窗。
     * 流程：保留当前能力 Tab 并打开表单，弹窗据此展示对应厂商预设。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不会创建模型或初始化密钥字段。
     */
    function handleOpenModelDialog(): void {
        pendingEditing.value = null;
        modelDialogOpen.value = true;
    }

    /**
     * 打开模型编辑弹窗。
     * 流程：保存脱敏元数据作为编辑上下文，弹窗只回显非敏感字段且 API Key 保持为空。
     * 参数：model 为待编辑的本机安全模型元数据。
     * 返回：无返回值。
     * 边界：不会读取或回填本地配置中的密钥正文。
     */
    function handleOpenEditDialog(model: PrivateModelItemModel): void {
        pendingEditing.value = model;
        modelDialogOpen.value = true;
    }

    /**
     * 保存新增模型。
     * 流程：把已测试的内存表单交给 Store，通过私有 Tauri IPC 写入本地配置和密钥。
     * 参数：form 为弹窗校验后的模型表单。
     * 返回：保存完成 Promise。
     * 异常：原生端保存失败时向弹窗透传，表单保持打开以便重试。
     */
    async function handleSaveModel(form: ModelFormModel): Promise<void> {
        try {
            await store.saveModel(form);
            toast.success(form.id ? '模型配置已更新' : '模型配置已保存', {
                description: store.message || undefined
            });
            store.message = '';
            pendingEditing.value = null;
        } catch (error) {
            store.message = '';
            showModelOperationError('保存模型配置失败', error, '模型配置保存失败。');
            throw error;
        }
    }

    /**
     * 切换模型启用状态。
     * 流程：通过统一 save_private_model IPC 保存状态，原生端按 ID 保留现有本地密钥。
     * 参数：model 为目标模型，enabled 为用户选择的目标状态。
     * 返回：操作完成 Promise。
     * 边界：失败时 Store 保留原列表并展示错误，不做乐观状态切换。
     */
    async function handleToggleModel(model: PrivateModelItemModel, enabled: boolean): Promise<void> {
        try {
            await store.updateModelStatus(model, { enabled });
            toast.success(enabled ? '模型已启用' : '模型已禁用', {
                description: store.message || undefined
            });
            store.message = '';
        } catch (error) {
            store.message = '';
            showModelOperationError('更新模型状态失败', error, '模型状态更新失败。');
        }
    }

    /**
     * 把模型设为能力默认项。
     * 流程：通过统一 save_private_model IPC 设置 isDefault，原生端负责取消同能力旧默认项。
     * 参数：model 为目标已启用模型。
     * 返回：操作完成 Promise。
     * 边界：禁用模型按钮不可用；失败时不修改前端默认标记。
     */
    async function handleSetDefault(model: PrivateModelItemModel): Promise<void> {
        try {
            await store.updateModelStatus(model, { isDefault: true });
            toast.success('默认模型已更新', {
                description: store.message || undefined
            });
            store.message = '';
        } catch (error) {
            store.message = '';
            showModelOperationError('设置默认模型失败', error, '默认模型设置失败。');
        }
    }

    /**
     * 删除当前确认模型。
     * 流程：通过私有 Tauri IPC 删除模型配置和本地密钥，成功后关闭确认弹窗。
     * 参数：无，目标来自 pendingRemoval。
     * 返回：删除完成 Promise。
     * 边界：没有目标时直接返回；失败时保留弹窗并由 Store 展示错误。
     */
    async function handleRemoveModel(): Promise<void> {
        if (!pendingRemoval.value) return;
        try {
            await store.removeModel(pendingRemoval.value.id);
            pendingRemoval.value = null;
            toast.success('模型配置已删除', {
                description: store.message || undefined
            });
            store.message = '';
        } catch (error) {
            store.message = '';
            showModelOperationError('删除模型配置失败', error, '模型配置删除失败。');
        }
    }

    onMounted(() => {
        void store.hydrateModelManage();
    });
</script>
