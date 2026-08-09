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
                class="grid grid-cols-[minmax(0,1fr)_120px_120px] gap-4 border-b border-border px-4 py-3 text-[12px] font-medium text-muted-foreground">
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
                    class="grid grid-cols-[minmax(0,1fr)_120px_120px] gap-4 px-4 py-4">
                    <div class="flex min-w-0 items-start gap-3">
                        <model-manage-vendor-mark
                            :vendor-key="model.vendorKey"
                            :label="model.name" />
                        <div class="min-w-0">
                            <div class="truncate text-[14px] font-medium text-foreground">{{ model.name }}</div>
                            <div class="mt-1 truncate text-[12px] text-muted-foreground">{{ model.model }}</div>
                            <div class="mt-1 truncate text-[12px] text-muted-foreground">{{ model.baseUrl }}</div>
                        </div>
                    </div>
                    <div class="flex items-start">
                        <ui-badge variant="secondary">{{ model.source === 'vendor' ? '厂商' : '自定义' }}</ui-badge>
                    </div>
                    <div class="flex justify-end">
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
            :group="activeGroup"
            :save-model="handleAddModel" />
        <ui-dialog v-model:open="deleteDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>删除模型</ui-dialog-title>
                    <ui-dialog-description class="sr-only">确认删除当前选中的模型配置。</ui-dialog-description>
                </ui-dialog-header>
                <div class="text-sm text-muted-foreground">{{ pendingRemoval?.name }}</div>
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
    import { Tabs as UiTabs, TabsList as UiTabsList, TabsTrigger as UiTabsTrigger } from '@/components/ui/tabs';
    import type { ModelFormModel, ModelGroupType, ModelItemModel } from '@/model/modelManage';
    import { useModelManageStore } from '@/stores/modelManage';

    defineOptions({
        name: 'ModelManageView'
    });

    const store = useModelManageStore();
    const modelDialogOpen = ref(false);
    const activeGroup = ref<ModelGroupType>('text');
    const pendingRemoval = ref<ModelItemModel | null>(null);
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

    // 打开添加模型弹窗，并沿用当前 Tab 作为新增模型类型。
    function handleOpenModelDialog(): void {
        modelDialogOpen.value = true;
    }

    // 校验模型表单后通过客户端配置接口写入当前类型的模型列表。
    async function handleAddModel(form: ModelFormModel): Promise<void> {
        await store.addModel(form);
    }

    // 二次确认后通过客户端配置接口删除指定模型，避免误删当前业务正在使用的模型。
    async function handleRemoveModel(): Promise<void> {
        if (!pendingRemoval.value) return;
        try {
            await store.removeModel(pendingRemoval.value.id);
            pendingRemoval.value = null;
        } catch {
            // Store 已经恢复删除前列表并记录错误提示，弹窗保持打开便于用户重试。
        }
    }
</script>
