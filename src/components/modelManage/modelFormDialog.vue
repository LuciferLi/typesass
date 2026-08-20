<template>
    <ui-dialog v-model:open="open">
        <ui-dialog-content>
            <ui-dialog-header>
                <ui-dialog-title>{{ title }}</ui-dialog-title>
                <ui-dialog-description class="sr-only">{{ dialogDescription }}</ui-dialog-description>
            </ui-dialog-header>
            <form @submit.prevent="handleSubmit">
                <ui-alert
                    v-if="!canRunModelTest"
                    variant="muted"
                    class="mb-4">
                    <ui-alert-title>网页预览无法测试模型</ui-alert-title>
                    <ui-alert-description>
                        模型配置和密钥只允许通过 CodexMan 桌面端私有 IPC 保存。请在桌面客户端中完成测试和添加。
                    </ui-alert-description>
                </ui-alert>

                <ui-field-group>
                    <ui-field>
                        <ui-field-label>厂商</ui-field-label>
                        <ui-select-root
                            v-model="selectedVendorValue"
                            @update:model-value="handleVendorChanged">
                            <ui-select-trigger>
                                <ui-select-value :placeholder="vendorPlaceholder" />
                            </ui-select-trigger>
                            <ui-select-content>
                                <ui-select-item
                                    v-if="props.group === 'text'"
                                    value="custom"
                                    >自定义中转站</ui-select-item
                                >
                                <ui-select-item
                                    v-for="vendor in vendorOptions"
                                    :key="vendor.key"
                                    :value="vendor.key">
                                    <span class="flex items-center gap-2">
                                        <model-manage-vendor-mark
                                            :vendor-key="vendor.key"
                                            :label="vendor.label" />
                                        <span>{{ vendor.label }}</span>
                                    </span>
                                </ui-select-item>
                            </ui-select-content>
                        </ui-select-root>
                        <ui-field-description v-if="selectedVendor">
                            {{ selectedVendor.apiKeyHelp }}
                        </ui-field-description>
                    </ui-field>

                    <ui-field v-if="selectedVendor">
                        <ui-field-label>模型</ui-field-label>
                        <ui-select-root
                            v-model="selectedModelKey"
                            @update:model-value="handleFormChanged">
                            <ui-select-trigger>
                                <ui-select-value placeholder="选择模型" />
                            </ui-select-trigger>
                            <ui-select-content>
                                <ui-select-item
                                    v-for="modelOption in selectedVendor.models"
                                    :key="modelOption.key"
                                    :disabled="modelOption.comingSoon"
                                    :value="modelOption.key">
                                    {{ modelOption.label }}{{ modelOption.recommended ? ' · 推荐' : ''
                                    }}{{ modelOption.comingSoon ? ' · 即将支持' : '' }}
                                </ui-select-item>
                            </ui-select-content>
                        </ui-select-root>
                        <ui-field-description v-if="selectedModel">
                            {{ selectedModel.description }}
                        </ui-field-description>
                    </ui-field>

                    <ui-field>
                        <ui-field-label>API Key</ui-field-label>
                        <div class="relative">
                            <ui-input
                                v-model="apiKey"
                                class="pr-10"
                                :type="apiKeyVisible ? 'text' : 'password'"
                                :placeholder="apiKeyPlaceholder"
                                autocomplete="off"
                                autofocus
                                @update:model-value="handleFormChanged" />
                            <button
                                class="absolute right-2 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                                type="button"
                                :title="apiKeyVisible ? '隐藏 API Key' : '查看 API Key'"
                                @click="apiKeyVisible = !apiKeyVisible">
                                <preview-close
                                    v-if="apiKeyVisible"
                                    theme="outline"
                                    size="16" />
                                <preview-open
                                    v-else
                                    theme="outline"
                                    size="16" />
                                <span class="sr-only">{{ apiKeyVisible ? '隐藏 API Key' : '查看 API Key' }}</span>
                            </button>
                        </div>
                        <ui-field-description>
                            {{ apiKeyHelp }}
                            <a
                                v-if="apiKeyUrl"
                                class="whitespace-nowrap font-medium text-primary underline-offset-4 transition-colors hover:text-primary/80 hover:underline"
                                :href="apiKeyUrl"
                                rel="noreferrer"
                                target="_blank">
                                {{ apiKeyUrlLabel }}
                            </a>
                        </ui-field-description>
                    </ui-field>

                    <template v-if="selectedVendorValue === 'custom'">
                        <ui-field>
                            <ui-field-label>显示名称</ui-field-label>
                            <ui-input
                                v-model="customDisplayName"
                                placeholder="请输入用于模型选择器展示的名称"
                                @update:model-value="handleFormChanged" />
                            <ui-field-description>只用于本机管理页和业务模型选择器展示。</ui-field-description>
                        </ui-field>
                        <ui-field>
                            <ui-field-label>请求路径</ui-field-label>
                            <ui-input
                                v-model="customBaseUrl"
                                placeholder="请输入 OpenAI 兼容请求路径，例如 https://api.example.com/v1"
                                @update:model-value="handleFormChanged" />
                            <ui-field-description
                                >使用中转站时填写服务商提供的 OpenAI 兼容 Base URL。</ui-field-description
                            >
                        </ui-field>
                        <ui-field>
                            <ui-field-label>模型名称</ui-field-label>
                            <ui-input
                                v-model="customModelName"
                                placeholder="请输入模型名称，例如 gpt-4o-mini"
                                @update:model-value="handleFormChanged" />
                            <ui-field-description>填写中转站要求传给接口的 model 字段。</ui-field-description>
                        </ui-field>
                    </template>
                </ui-field-group>

                <p
                    v-if="testMessage"
                    :class="[
                        'mt-4 text-[13px] leading-5',
                        testStatus === 'error' ? 'text-destructive' : 'text-muted-foreground',
                        testStatus === 'success' ? 'text-primary' : ''
                    ]"
                    role="status">
                    {{ testMessage }}
                </p>

                <ui-dialog-footer class="mt-6 sm:justify-between">
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="operationRunning || !canRunModelTest"
                        @click="handleTestModel">
                        <loading-one
                            v-if="testing"
                            class="animate-spin"
                            theme="outline"
                            size="15" />
                        {{ testing ? '测试中' : '测试' }}
                        <check-small
                            v-if="testStatus === 'success' && !testing"
                            class="text-primary"
                            theme="outline"
                            size="15" />
                    </ui-button>
                    <div class="flex flex-col-reverse gap-2 sm:flex-row">
                        <ui-button
                            variant="outline"
                            type="button"
                            :disabled="operationRunning"
                            @click="open = false"
                            >取消</ui-button
                        >
                        <ui-button
                            type="submit"
                            :disabled="operationRunning || !canRunModelTest">
                            <loading-one
                                v-if="submitting"
                                class="animate-spin"
                                theme="outline"
                                size="15" />
                            {{ submitting ? '验证中' : props.model ? '保存' : '添加' }}
                        </ui-button>
                    </div>
                </ui-dialog-footer>
            </form>
        </ui-dialog-content>
    </ui-dialog>
</template>

<script setup lang="ts">
    import { CheckSmall, LoadingOne, PreviewClose, PreviewOpen } from '@icon-park/vue-next';

    import ModelManageVendorMark from '@/components/modelManage/vendorMark.vue';
    import {
        Alert as UiAlert,
        AlertDescription as UiAlertDescription,
        AlertTitle as UiAlertTitle
    } from '@/components/ui/alert';
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
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import { ModelVendorPresets } from '@/config/defaultModel';
    import type {
        ModelFormModel,
        ModelGroupType,
        ModelPresetKey,
        ModelVendorKey,
        ModelVendorOptionModel,
        PrivateModelItemModel
    } from '@/model/modelManage';
    import { isTauriRuntime } from '@/service/tauri/command';
    import { useModelManageStore } from '@/stores/modelManage';

    defineOptions({
        name: 'ModelManageModelFormDialog'
    });

    const props = withDefaults(
        defineProps<{
            // 当前添加的模型类型，由模型管理页顶部切换栏决定。
            group: ModelGroupType;
            // 弹窗标题，用于模型管理页和业务引导流程展示不同入口文案。
            title?: string;
            // 保存模型配置的业务回调，调用方应在这里完成客户端配置接口写入。
            saveModel?: (value: ModelFormModel) => Promise<void>;
            // 待编辑的安全模型元数据；为空时表示新增。
            model?: PrivateModelItemModel | null;
        }>(),
        {
            title: '添加模型'
        }
    );

    const emit = defineEmits<{
        // 提交新增模型表单。
        submit: [value: ModelFormModel];
    }>();

    const open = defineModel<boolean>('open', { default: false });
    const modelManageStore = useModelManageStore();
    const selectedVendorValue = ref<ModelVendorKey | 'custom'>('custom');
    const selectedModelKey = ref<ModelPresetKey | ''>('');
    const apiKey = ref('');
    const apiKeyVisible = ref(false);
    const customBaseUrl = ref('');
    const customDisplayName = ref('');
    const customModelName = ref('');
    const testing = ref(false);
    const submitting = ref(false);
    const testStatus = ref<'idle' | 'success' | 'error'>('idle');
    const testMessage = ref('');
    const vendorOptions = computed<ModelVendorOptionModel[]>(() =>
        ModelVendorPresets.filter((vendor) => vendor.group === props.group)
    );
    const selectedVendor = computed<ModelVendorOptionModel | null>(() => {
        if (selectedVendorValue.value === 'custom') return null;
        return vendorOptions.value.find((vendor) => vendor.key === selectedVendorValue.value) || null;
    });
    const selectedModel = computed(() => {
        if (!selectedVendor.value) return null;
        return selectedVendor.value.models.find((model) => model.key === selectedModelKey.value) || null;
    });
    const apiKeyPlaceholder = computed(() => selectedVendor.value?.apiKeyPlaceholder || '请输入中转站 API Key');
    const apiKeyHelp = computed(() => {
        if (props.model?.hasApiKey) return '密钥已保存在本地配置中；留空会保留原密钥，填写则轮换密钥。';
        return selectedVendor.value?.apiKeyHelp || '请填写中转站或代理服务提供的 API Key。';
    });
    const apiKeyUrl = computed(() => selectedVendor.value?.apiKeyUrl || '');
    const apiKeyUrlLabel = computed(() => selectedVendor.value?.apiKeyUrlLabel || '');
    const operationRunning = computed(() => testing.value || submitting.value);
    const canRunModelTest = computed(() => isTauriRuntime());
    const vendorPlaceholder = computed(() => {
        if (props.group === 'asr') return '选择实时语音识别厂商';
        return '选择厂商或自定义中转站';
    });
    const dialogDescription = computed(() => {
        if (props.group === 'asr') return '选择实时语音识别厂商和模型配置。';
        return '选择厂商或填写自定义中转站模型配置。';
    });

    /**
     * 重置添加模型表单。
     * 流程：清空密钥和自定义字段；文本模型默认自定义中转站，ASR 默认首个实时 provider。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不关闭弹窗，由调用处决定弹窗状态。
     */
    function resetForm(): void {
        const editingVendor = props.model?.vendorKey as ModelVendorKey | undefined;
        const editingModel = props.model?.modelKey as ModelPresetKey | undefined;
        const matchedVendor = editingVendor
            ? vendorOptions.value.find((vendor) => vendor.key === editingVendor)
            : vendorOptions.value.find((vendor) =>
                  vendor.models.some(
                      (model) => model.model === props.model?.modelName && model.baseUrl === props.model?.baseUrl
                  )
              );
        selectedVendorValue.value = matchedVendor?.key ?? firstVendorValue();
        selectedModelKey.value =
            matchedVendor?.models.find((model) => model.key === editingModel)?.key ??
            matchedVendor?.models.find(
                (model) => model.model === props.model?.modelName && model.baseUrl === props.model?.baseUrl
            )?.key ??
            matchedVendor?.models.find((model) => model.recommended && !model.comingSoon)?.key ??
            matchedVendor?.models.find((model) => !model.comingSoon)?.key ??
            '';
        apiKey.value = '';
        apiKeyVisible.value = false;
        customBaseUrl.value = props.model?.baseUrl ?? '';
        customDisplayName.value = props.model?.displayName ?? '';
        customModelName.value = props.model?.modelName ?? '';
        testStatus.value = 'idle';
        testMessage.value = '';
    }

    watch(open, (visible) => {
        if (visible) resetForm();
    });

    /**
     * 厂商切换后自动选择该厂商推荐模型。
     * 流程：先选推荐且当前已接入的模型，再退回首个可用模型；自定义中转站清空模型选择。
     * 参数：无。
     * 返回：无返回值。
     * 边界：暂未接入的实时 ASR 模型不会被默认选中，避免用户误以为已经可以保存使用。
     */
    function handleVendorChanged(): void {
        if (selectedVendor.value) {
            selectedModelKey.value =
                selectedVendor.value.models.find((model) => model.recommended && !model.comingSoon)?.key ??
                selectedVendor.value.models.find((model) => !model.comingSoon)?.key ??
                '';
        } else {
            selectedModelKey.value = '';
        }
        handleFormChanged();
    }

    /**
     * 表单内容变化后清理上一次测试状态。
     * 流程：用户修改厂商、密钥、请求路径或模型名称时，清空成功对勾和错误提示，避免旧测试结果继续代表新配置。
     * 参数：无。
     * 返回：无返回值。
     * 边界：测试或提交进行中不清理状态，避免 loading 反馈被输入事件打断。
     */
    function handleFormChanged(): void {
        if (operationRunning.value) return;
        testStatus.value = 'idle';
        testMessage.value = '';
    }

    /**
     * 构建当前表单模型。
     * 流程：先校验 API Key，再根据厂商预设或自定义中转站补齐请求路径、模型名和来源字段。
     * 参数：无。
     * 返回：字段完整时返回模型表单；字段缺失时返回 null 并写入错误提示。
     * 边界：不会发起网络测试，也不会关闭弹窗。
     */
    function createFormModel(): ModelFormModel | null {
        const normalizedApiKey = apiKey.value.trim();
        if (!normalizedApiKey && !props.model?.hasApiKey) {
            setTestError('请先填写 API Key。');
            return null;
        }
        if (selectedVendor.value) {
            if (!selectedModel.value) {
                setTestError('请先选择模型。');
                return null;
            }
            if (selectedModel.value.comingSoon) {
                setTestError('该实时 ASR 模型将在下一阶段接入 WebSocket 音频流，当前版本暂不能保存使用。');
                return null;
            }
            return {
                name: selectedModel.value.modelName,
                group: props.group,
                baseUrl: selectedModel.value.baseUrl,
                model: selectedModel.value.model,
                provider: selectedModel.value.provider ?? 'openai-compatible',
                apiKey: normalizedApiKey || undefined,
                source: 'vendor',
                vendorKey: selectedVendor.value.key,
                modelKey: selectedModel.value.key,
                remark: selectedVendor.value.label,
                enabled: true,
                isDefault: false
            };
        }
        if (props.group === 'asr') {
            setTestError('ASR 只支持内置实时语音识别厂商，请选择阿里实时 ASR、腾讯云实时 ASR 或讯飞实时转写。');
            return null;
        }
        const normalizedBaseUrl = customBaseUrl.value.trim();
        const normalizedDisplayName = customDisplayName.value.trim();
        const normalizedModelName = customModelName.value.trim();
        if (!normalizedBaseUrl || !normalizedDisplayName || !normalizedModelName) {
            setTestError('请填写显示名称、请求路径和模型名称。');
            return null;
        }
        return {
            id: props.model?.id,
            name: normalizedDisplayName,
            group: props.model?.capability ?? props.group,
            baseUrl: normalizedBaseUrl,
            model: normalizedModelName,
            provider: 'openai-compatible',
            apiKey: normalizedApiKey || undefined,
            source: 'custom',
            vendorKey: '',
            modelKey: '',
            remark: props.model?.provider ?? '自定义中转站',
            enabled: props.model?.enabled ?? true,
            isDefault: props.model?.isDefault ?? false
        };
    }

    /**
     * 获取当前能力的默认厂商选项。
     * 流程：文本模型保留自定义中转站；ASR 新增入口强制选择首个实时 provider。
     * 参数：无。
     * 返回：厂商键或 custom。
     * 边界：如果 ASR 预设异常为空，则仍返回 custom 并由提交校验给出明确错误。
     */
    function firstVendorValue(): ModelVendorKey | 'custom' {
        if (props.group === 'text') return 'custom';
        return vendorOptions.value[0]?.key ?? 'custom';
    }

    /**
     * 写入模型测试失败提示。
     * 流程：把状态切换为 error，并用统一区域展示失败原因。
     * 参数：message 为失败原因文案。
     * 返回：无返回值。
     * 边界：不会抛出异常，供表单校验和接口异常共用。
     */
    function setTestError(message: string): void {
        testStatus.value = 'error';
        testMessage.value = message;
    }

    /**
     * 写入模型测试成功提示。
     * 流程：把状态切换为 success，并用统一区域展示成功原因。
     * 参数：message 为成功文案。
     * 返回：无返回值。
     * 边界：不会保存模型，只反馈本次测试结果。
     */
    function setTestSuccess(message: string): void {
        testStatus.value = 'success';
        testMessage.value = message;
    }

    /**
     * 确认当前环境能执行真实模型测试。
     * 流程：先判断桌面运行环境，不能使用私有 IPC 时直接写入提示并阻断字段校验。
     * 参数：无。
     * 返回：可真实测试返回 true，不可测试返回 false。
     * 边界：该方法不抛出异常，避免网页预览环境先出现 API Key 等次级字段错误。
     */
    function ensureClientRuntimeForModelTest(): boolean {
        if (isTauriRuntime()) return true;
        setTestError('网页预览无法真实测试模型连通性，请先打开 CodexMan 客户端。');
        return false;
    }

    /**
     * 点击测试按钮后验证模型可用性。
     * 流程：归一化表单，进入测试 loading，按模型类型调用对应测试接口，并展示成功或失败结果。
     * 参数：无。
     * 返回：无返回值。
     * 边界：测试失败只展示错误，不会关闭弹窗或保存模型。
     */
    async function handleTestModel(): Promise<void> {
        if (operationRunning.value) return;
        if (!ensureClientRuntimeForModelTest()) return;
        const form = createFormModel();
        if (!form) return;
        testing.value = true;
        testStatus.value = 'idle';
        testMessage.value = form.group === 'asr' ? '正在测试 ASR 模型。' : '正在测试文本模型。';
        try {
            const result = await modelManageStore.testModel(form);
            if (!result.success) {
                const errorCode = result.errorCode || 'MODEL_CONNECTION_TEST_FAILED';
                setTestError(`${result.message || '模型测试失败。'}（错误码：${errorCode}）`);
                return;
            }
            setTestSuccess(
                result.message || (form.group === 'asr' ? 'ASR 模型真实请求通过。' : '文本模型真实请求通过。')
            );
        } catch (error) {
            setTestError(error instanceof Error ? error.message : '模型测试失败。');
        } finally {
            testing.value = false;
        }
    }

    /**
     * 提交新增模型配置。
     * 流程：归一化表单后提交私有 IPC，由原生端强制执行真实能力探测并在通过后原子保存，成功才关闭弹窗。
     * 参数：无。
     * 返回：无返回值。
     * 边界：必要字段为空或模型测试失败时直接中断，不写入模型列表。
     */
    async function handleSubmit(): Promise<void> {
        if (operationRunning.value) return;
        if (!ensureClientRuntimeForModelTest()) return;
        const form = createFormModel();
        if (!form) return;
        submitting.value = true;
        testStatus.value = 'idle';
        testMessage.value = form.group === 'asr' ? '正在验证并保存 ASR 模型。' : '正在验证并保存文本模型。';
        try {
            if (props.saveModel) {
                await props.saveModel(form);
            } else {
                emit('submit', form);
            }
            setTestSuccess(props.model ? '模型验证通过，已保存。' : '模型验证通过，已添加。');
            resetForm();
            open.value = false;
        } catch (error) {
            const fallbackMessage = props.model ? '模型验证或保存失败，配置未更新。' : '模型验证失败，暂未添加。';
            setTestError(error instanceof Error ? error.message : fallbackMessage);
        } finally {
            submitting.value = false;
        }
    }
</script>
