import { defineStore } from 'pinia';

import type {
    ModelCapabilityType,
    ModelCatalogItemModel,
    ModelFormModel,
    ModelTestResultModel,
    PrivateModelItemModel,
    SavePrivateModelRequestModel
} from '@/model/modelManage';
import {
    deletePrivateModel,
    isTauriRuntime,
    listPrivateModels,
    listPublicModels,
    savePrivateModel,
    testPrivateModel
} from '@/service/tauri/command';

/** 模型管理 Store 状态，分别保存公共安全目录和本机私有管理元数据。 */
interface ModelManageState {
    /** 公共服务目录，业务请求选择只能引用这里的不透明 ID。 */
    serviceModels: ModelCatalogItemModel[];
    /** 本机私有模型元数据，不包含 API Key。 */
    models: PrivateModelItemModel[];
    /** 是否正在读取任一模型目录。 */
    loading: boolean;
    /** 是否正在执行私有模型写操作。 */
    saving: boolean;
    /** 最近一次目录读取或模型操作提示。 */
    message: string;
}

/** 模型选择校正结果，用于把失效选择回退到服务端可用默认项并向用户解释。 */
export interface ResolvedModelSelectionModel {
    /** 校正后可发送给服务端的不透明模型 ID；无可用模型时为空。 */
    modelId: string;
    /** 发生回退或无可用模型时的明确提示；原选择有效时为空。 */
    message: string;
}

/**
 * 把页面表单转换为私有 IPC 请求。
 * 流程：映射展示名、能力、上游连接参数和临时密钥，默认启用但不擅自覆盖服务端默认项。
 * 参数：form 为仅存在于弹窗内存中的完整模型表单。
 * 返回：符合 Rust `save_private_model` 与 `test_private_model` 的请求。
 * 边界：不会写 JSON、localStorage 或日志；密钥只存在于返回对象直至 IPC 调用结束。
 */
function createPrivateModelRequest(form: ModelFormModel): SavePrivateModelRequestModel {
    return {
        id: form.id,
        displayName: form.name,
        capability: form.group,
        enabled: form.enabled,
        isDefault: form.isDefault,
        provider: form.provider,
        vendorKey: form.vendorKey || undefined,
        modelKey: form.modelKey || undefined,
        baseUrl: form.baseUrl,
        modelName: form.model,
        apiKey: form.apiKey || undefined
    };
}

export const useModelManageStore = defineStore('modelManage', {
    state: (): ModelManageState => ({
        serviceModels: [],
        models: [],
        loading: false,
        saving: false,
        message: ''
    }),
    getters: {
        /** 按能力读取本机私有模型元数据，用于模型管理页分组展示。 */
        groupModels:
            (state) =>
            (capability: ModelCapabilityType): PrivateModelItemModel[] =>
                state.models.filter((model) => model.capability === capability),
        /** 按能力读取公共服务中当前启用的模型，用于业务页选择。 */
        enabledServiceModels:
            (state) =>
            (capability: ModelCapabilityType): ModelCatalogItemModel[] =>
                state.serviceModels.filter((model) => model.capability === capability && model.enabled)
    },
    actions: {
        /**
         * 初始化模型目录。
         * 流程：始终读取公共安全目录；桌面端再通过私有 IPC 读取可管理元数据，普通 Web 不尝试读取密钥配置。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：任一目录失败都会清空对应数据并展示错误，绝不回退到历史 JSON 中的密钥配置。
         */
        async hydrateModelManage(): Promise<void> {
            this.loading = true;
            const errors: string[] = [];
            try {
                this.serviceModels = await listPublicModels();
            } catch (error) {
                this.serviceModels = [];
                const reason = error instanceof Error ? error.message : '读取公共模型目录失败。';
                errors.push(`服务目录不可用：${reason}`);
            }
            try {
                this.models = isTauriRuntime() ? await listPrivateModels() : [];
            } catch (error) {
                this.models = [];
                const reason = error instanceof Error ? error.message : '读取本机私有模型失败。';
                errors.push(`本机私有模型不可用：${reason}`);
            } finally {
                this.message = errors.join(' ');
                this.loading = false;
            }
        },

        /**
         * 刷新公共服务模型目录。
         * 流程：重新读取 GET `/v1/models` 并原子替换目录，供业务执行前校验选择。
         * 参数：无。
         * 返回：刷新完成 Promise。
         * 异常：目录不可达时清空旧目录并向上抛出，避免继续使用已无法确认的模型 ID。
         */
        async refreshServiceModels(): Promise<void> {
            try {
                this.serviceModels = await listPublicModels();
            } catch (error) {
                this.serviceModels = [];
                throw error;
            }
        },

        /**
         * 校正业务模型选择。
         * 流程：保留仍启用且能力匹配的选择；否则优先回退服务端默认项，再回退首个启用项。
         * 参数：capability 为业务能力，selectedId 为已保存选择，usageLabel 为提示中的流程名称。
         * 返回：可用模型 ID 和明确回退说明；目录无可用项时 ID 为空。
         * 边界：只依赖当前服务目录，不根据私有表单或前端预设猜测模型。
         */
        resolveSelection(
            capability: ModelCapabilityType,
            selectedId: string,
            usageLabel: string
        ): ResolvedModelSelectionModel {
            const candidates = this.serviceModels.filter((model) => model.capability === capability && model.enabled);
            const selected = candidates.find((model) => model.id === selectedId);
            if (selected) return { modelId: selected.id, message: '' };
            const fallback = candidates.find((model) => model.isDefault) ?? candidates[0];
            if (!fallback) {
                return {
                    modelId: '',
                    message: `${usageLabel}没有可用模型，请在模型管理中添加并启用对应能力。`
                };
            }
            return {
                modelId: fallback.id,
                message: selectedId
                    ? `${usageLabel}原选择已失效，已回退到“${fallback.displayName}”。`
                    : `${usageLabel}已使用服务默认模型“${fallback.displayName}”。`
            };
        },

        /**
         * 测试未保存模型。
         * 流程：把内存表单转换成私有 IPC 请求，执行真实连通性测试但不写入配置或安全存储。
         * 参数：form 为弹窗当前表单。
         * 返回：原生端真实测试结果。
         * 异常：普通 Web、鉴权或上游异常时透传，调用方保留表单便于修正。
         */
        async testModel(form: ModelFormModel): Promise<ModelTestResultModel> {
            return testPrivateModel(createPrivateModelRequest(form));
        },

        /**
         * 新增私有模型。
         * 流程：通过私有 IPC 保存连接配置与安全密钥，再读取本地脱敏管理目录和安全模型目录；sidecar 运行时目录由原生端后台热更新。
         * 参数：form 为已经校验并测试通过的内存表单。
         * 返回：保存后的安全模型元数据。
         * 异常：保存失败时透传；目录刷新失败只影响当前页面回显，不缓存含密钥的表单对象。
         */
        async saveModel(form: ModelFormModel): Promise<PrivateModelItemModel> {
            this.saving = true;
            try {
                const savedModel = await savePrivateModel(createPrivateModelRequest(form));
                try {
                    this.models = await listPrivateModels();
                } catch {
                    this.models = [savedModel, ...this.models.filter((model) => model.id !== savedModel.id)];
                }
                try {
                    await this.refreshServiceModels();
                    this.message = form.id
                        ? '模型配置已更新，服务目录正在后台刷新。'
                        : '模型配置已安全保存，服务目录正在后台刷新。';
                } catch (error) {
                    const reason = error instanceof Error ? error.message : '公共模型目录刷新失败。';
                    this.message = `模型配置已安全保存，但服务目录暂未刷新：${reason}`;
                }
                return savedModel;
            } catch (error) {
                this.message = error instanceof Error ? error.message : '保存模型配置失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 更新模型启用态或默认态。
         * 流程：基于脱敏元数据构造不含 API Key 的编辑表单，复用 saveModel；原生端识别为纯状态变更并跳过上游探针，同时按 ID 保留 Keychain 密钥。
         * 参数：model 为当前安全元数据；changes 为需要修改的启用或默认字段。
         * 返回：更新后的安全模型元数据。
         * 异常：状态约束、原生持久化或目录刷新失败时按 saveModel 规则处理；sidecar 热更新不阻塞纯启停或设默认。
         */
        async updateModelStatus(
            model: PrivateModelItemModel,
            changes: Partial<Pick<PrivateModelItemModel, 'enabled' | 'isDefault'>>
        ): Promise<PrivateModelItemModel> {
            return this.saveModel({
                id: model.id,
                name: model.displayName,
                group: model.capability,
                baseUrl: model.baseUrl,
                model: model.modelName,
                provider: (model.provider || 'openai-compatible') as ModelFormModel['provider'],
                source: model.vendorKey ? 'vendor' : 'custom',
                vendorKey: (model.vendorKey || '') as ModelFormModel['vendorKey'],
                modelKey: (model.modelKey || '') as ModelFormModel['modelKey'],
                remark: model.provider,
                enabled: changes.enabled ?? model.enabled,
                isDefault: changes.isDefault ?? model.isDefault
            });
        },

        /**
         * 删除私有模型。
         * 流程：通过原生 IPC 删除本机持久化配置，成功后立即从管理目录和服务目录移除；公共服务目录稍后在后台刷新。
         * 参数：id 为不透明模型 ID。
         * 返回：本机配置删除完成 Promise。
         * 异常：只有原生端拒绝或持久化删除失败时向上抛出；sidecar 热更新与目录刷新不阻塞删除反馈。
         */
        async removeModel(id: string): Promise<void> {
            this.saving = true;
            try {
                await deletePrivateModel(id);
                this.models = this.models.filter((model) => model.id !== id);
                this.serviceModels = this.serviceModels.filter((model) => model.id !== id);
                this.message = '模型配置已删除，服务目录正在后台刷新。';
                window.setTimeout(() => {
                    void this.refreshServiceModels().catch((error: unknown) => {
                        const reason = error instanceof Error ? error.message : '公共模型目录刷新失败。';
                        this.message = `模型配置已删除，但服务目录暂未刷新：${reason}`;
                    });
                }, 2500);
            } catch (error) {
                this.message = error instanceof Error ? error.message : '删除模型配置失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        }
    }
});
