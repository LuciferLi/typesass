import { defineStore } from 'pinia';

import { DefaultModels } from '@/config/defaultModel';
import { StorageKey } from '@/config/storageKey';
import type { ModelFormModel, ModelGroupType, ModelItemModel } from '@/model/modelManage';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';

interface ModelManageState {
    // 本地维护的模型列表，作为各业务模块的模型仓库。
    models: ModelItemModel[];
    // 是否正在从客户端配置文件初始化模型列表。
    loading: boolean;
    // 是否正在把模型列表保存到客户端配置文件。
    saving: boolean;
    // 模型管理模块最近一次保存或读取提示。
    message: string;
}

const LegacyDefaultModelIds = new Set(['asr-mimo-default', 'text-mimo-default', 'text-deepseek-default']);

/**
 * 归一化本地模型列表。
 * 流程：过滤历史内置默认模型，再补齐旧数据缺失的新字段，避免历史数据在新页面中继续显示成用户模型。
 * 参数：models 为本地存储读取出的模型列表。
 * 返回：可在当前模型仓库展示和调用的模型列表。
 * 边界：旧数据缺少 apiKey/source/vendorKey 时按自定义模型兼容，但历史默认 ID 会直接过滤。
 */
function normalizeStoredModels(models: ModelItemModel[]): ModelItemModel[] {
    return models
        .filter((model) => !LegacyDefaultModelIds.has(model.id))
        .map((model) => ({
            ...model,
            apiKey: model.apiKey ?? '',
            source: model.source ?? 'custom',
            vendorKey: model.vendorKey ?? ''
        }));
}

export const useModelManageStore = defineStore('modelManage', {
    state: (): ModelManageState => {
        return {
            models: normalizeStoredModels(DefaultModels),
            loading: false,
            saving: false,
            message: ''
        };
    },
    getters: {
        // 按分组读取模型列表。
        groupModels:
            (state) =>
            (group: ModelGroupType): ModelItemModel[] => {
                return state.models.filter((model) => model.group === group);
            },
        // 按 ID 读取单个模型。
        modelById:
            (state) =>
            (id: string): ModelItemModel | null => {
                return state.models.find((model) => model.id === id) || null;
            }
    },
    actions: {
        /**
         * 从客户端 JSON 配置文件初始化模型列表。
         * 流程：读取模型管理分区，归一化历史数据后覆盖当前列表，供页面和语音模块使用。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：配置缺失或损坏时保持内置默认模型，避免页面启动空白。
         */
        async hydrateModelManage(): Promise<void> {
            this.loading = true;
            try {
                const storedModels = await readClientJson<ModelItemModel[]>(StorageKey.modelManage, DefaultModels);
                this.models = normalizeStoredModels(storedModels);
                this.message = '';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '读取模型配置失败。';
            } finally {
                this.loading = false;
            }
        },

        /**
         * 应用客户端 JSON 配置变化中的模型列表。
         * 流程：收到文件变化事件后读取对应分区并归一化，保证多个窗口或外部改文件后页面实时刷新。
         * 参数：models 为配置文件中的模型列表。
         * 返回：无返回值。
         * 边界：非数组输入会被忽略，避免外部手动改坏 JSON 后覆盖当前可用状态。
         */
        applyPersistedModels(models: unknown): void {
            if (!Array.isArray(models)) return;
            this.models = normalizeStoredModels(models as ModelItemModel[]);
        },

        /**
         * 保存模型列表到客户端 JSON 配置文件。
         * 流程：通过客户端本地 HTTP 桥接写入用户本机应用数据目录，不访问浏览器 localStorage。
         * 参数：无。
         * 返回：保存完成 Promise。
         * 边界：客户端未启动或写入失败会向上抛出，避免把未落盘状态误当作保存成功。
         */
        async persistModels(): Promise<void> {
            await writeClientJson(StorageKey.modelManage, this.models);
        },

        /**
         * 新增模型配置。
         * 流程：根据表单生成本地模型项，写入列表头部并持久化，最后返回新模型供调用方立即选中。
         * 参数：form 为已经通过校验和连通性测试的模型表单。
         * 返回：新写入的模型配置项。
         * 边界：不会校验重复模型名或请求路径，保持与既有模型管理页添加行为一致。
         */
        async addModel(form: ModelFormModel): Promise<ModelItemModel> {
            const previousModels = [...this.models];
            const model: ModelItemModel = {
                id: `model-${Date.now()}`,
                ...form,
                createdAt: new Date().toISOString()
            };
            this.saving = true;
            try {
                this.models = [model, ...this.models];
                await this.persistModels();
                this.message = '模型配置已保存。';
                return model;
            } catch (error) {
                this.models = previousModels;
                this.message = error instanceof Error ? error.message : '保存模型配置失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 删除指定模型配置。
         * 流程：先从当前列表移除，再通过客户端配置接口保存；保存失败时恢复删除前列表。
         * 参数：id 为需要删除的模型 ID。
         * 返回：删除保存完成 Promise。
         * 边界：目标 ID 不存在时仍会写入一次当前列表，保证客户端配置与页面状态一致。
         */
        async removeModel(id: string): Promise<void> {
            const previousModels = [...this.models];
            this.saving = true;
            try {
                this.models = this.models.filter((model) => model.id !== id);
                await this.persistModels();
                this.message = '模型配置已删除。';
            } catch (error) {
                this.models = previousModels;
                this.message = error instanceof Error ? error.message : '删除模型配置失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        }
    }
});
