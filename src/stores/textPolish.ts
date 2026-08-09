import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { TextPolishHistoryItemModel } from '@/model/textPolish';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import { pasteText, processText, readSelectedText } from '@/service/tauri/command';
import { useModelManageStore } from '@/stores/modelManage';

interface TextPolishState {
    // 当前选择的文本模型 ID。
    selectedTextModelId: string;
    // 模块内历史记录。
    history: TextPolishHistoryItemModel[];
    // 本模块输出偏好。
    styleInstruction: string;
    // 当前输入文本，可用于页面直接润色。
    inputText: string;
    // 最近输出文本。
    outputText: string;
    // 当前是否正在处理。
    running: boolean;
    // 模块状态提示。
    message: string;
}

const defaultState = {
    selectedTextModelId: '',
    history: [],
    styleInstruction: '',
    inputText: ''
};

// 文字润色模块需要持久化到客户端 JSON 配置文件的字段。
type TextPolishPersistedState = typeof defaultState;

export const useTextPolishStore = defineStore('textPolish', {
    state: (): TextPolishState => {
        return {
            ...defaultState,
            outputText: '',
            running: false,
            message: ''
        };
    },
    actions: {
        /**
         * 从客户端 JSON 配置文件初始化文字润色模块状态。
         * 流程：读取文字润色分区并应用模型选择、历史、输出偏好和页面输入草稿。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：配置缺失时使用空历史、空输入和未选择模型。
         */
        async hydrateTextPolish(): Promise<void> {
            const saved = await readClientJson<TextPolishPersistedState>(StorageKey.textPolish, defaultState);
            this.applyPersistedTextPolish(saved);
        },

        /**
         * 应用客户端 JSON 配置变化中的文字润色状态。
         * 流程：只刷新持久化字段，不覆盖本轮正在处理的输出态和 loading 态。
         * 参数：state 为配置文件中的文字润色分区。
         * 返回：无返回值。
         * 边界：数组字段非法时回退为空数组，字符串字段非法时回退为空字符串。
         */
        applyPersistedTextPolish(state: unknown): void {
            if (!state || typeof state !== 'object') return;
            const nextState = state as Partial<TextPolishPersistedState>;
            this.selectedTextModelId =
                typeof nextState.selectedTextModelId === 'string' ? nextState.selectedTextModelId : '';
            this.history = Array.isArray(nextState.history) ? nextState.history : [];
            this.styleInstruction = typeof nextState.styleInstruction === 'string' ? nextState.styleInstruction : '';
            this.inputText = typeof nextState.inputText === 'string' ? nextState.inputText : '';
        },

        // 持久化文字润色模块状态到客户端 JSON 配置文件。
        persistTextPolish(): void {
            void writeClientJson(StorageKey.textPolish, {
                selectedTextModelId: this.selectedTextModelId,
                history: this.history,
                styleInstruction: this.styleInstruction,
                inputText: this.inputText
            });
        },

        // 更新文本模型选择。
        updateTextModel(modelId: string): void {
            this.selectedTextModelId = modelId;
            this.persistTextPolish();
        },

        // 润色页面输入框中的文本。
        async polishInputText(): Promise<void> {
            await this.polishText(this.inputText, '');
        },

        // 从外部应用读取选中文本并润色后粘贴回去。
        async polishSelectedText(): Promise<void> {
            this.running = true;
            try {
                const selected = await readSelectedText();
                await this.polishText(selected.text, selected.targetApp);
            } finally {
                this.running = false;
            }
        },

        // 执行文本润色。
        async polishText(text: string, targetApp: string): Promise<void> {
            const modelStore = useModelManageStore();
            const textModel = modelStore.modelById(this.selectedTextModelId);
            if (!textModel) {
                this.message = '请先选择可用的文本模型。';
                return;
            }
            const normalizedText = text.trim();
            if (!normalizedText) {
                this.message = '请输入或选中需要润色的文字。';
                return;
            }
            this.running = true;
            this.message = '正在润色文字。';
            try {
                const processed = await processText({
                    apiKey: textModel.apiKey,
                    baseUrl: textModel.baseUrl,
                    textModel: textModel.model,
                    mode: 'polish',
                    text: normalizedText,
                    audioDurationMs: 0,
                    dictionary: [],
                    targetLanguages: [],
                    contextApp: targetApp,
                    styleInstruction: this.styleInstruction
                });
                this.outputText = processed.processedText;
                this.history.unshift({
                    id: `text-${Date.now()}`,
                    sourceText: normalizedText,
                    outputText: processed.processedText,
                    createdAt: new Date().toISOString()
                });
                this.history = this.history.slice(0, 80);
                this.persistTextPolish();
                if (targetApp) await pasteText(processed.processedText, targetApp);
                this.message = '文字润色已完成。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '文字润色失败。';
            } finally {
                this.running = false;
            }
        }
    }
});
