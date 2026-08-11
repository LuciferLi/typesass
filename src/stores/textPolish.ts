import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { TextPolishHistoryItemModel } from '@/model/textPolish';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import { isTauriRuntime, pasteText, processText, readSelectedText, showResultWindow } from '@/service/tauri/command';
import { useModelManageStore } from '@/stores/modelManage';

interface TextPolishState {
    // 当前文本模型的不透明服务目录 ID。
    textModelId: string;
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
    textModelId: '',
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
            this.textModelId = typeof nextState.textModelId === 'string' ? nextState.textModelId : '';
            this.history = Array.isArray(nextState.history) ? nextState.history : [];
            this.styleInstruction = typeof nextState.styleInstruction === 'string' ? nextState.styleInstruction : '';
            this.inputText = typeof nextState.inputText === 'string' ? nextState.inputText : '';
        },

        /**
         * 持久化文字润色状态。
         * 流程：桌面端把历史、偏好和输入草稿写入客户端 JSON；普通 Web 保持会话内状态。
         * 返回：无。
         * 边界：异步写入失败由统一配置服务记录，不把未落盘状态伪报为跨窗口同步成功。
         */
        persistTextPolish(): void {
            if (!isTauriRuntime()) return;
            void writeClientJson(StorageKey.textPolish, {
                textModelId: this.textModelId,
                history: this.history,
                styleInstruction: this.styleInstruction,
                inputText: this.inputText
            });
        },

        /**
         * 润色页面输入框文本。
         * 流程：把当前输入和空目标应用交给统一 polishText 链路。
         * 返回：处理完成 Promise。
         * 边界：空输入由统一链路拒绝，不发 HTTP 请求。
         */
        async polishInputText(): Promise<void> {
            await this.polishText(this.inputText, '');
        },

        /**
         * 润色桌面端选中文本。
         * 流程：通过 Tauri IPC 读取选区和目标应用，再调用统一 HTTP 润色及粘贴链路。
         * 返回：处理完成 Promise。
         * 边界：普通 Web 不可调用；读取或粘贴失败会保留结果并展示可排障信息。
         */
        async polishSelectedText(): Promise<void> {
            this.running = true;
            try {
                const selected = await readSelectedText();
                await this.polishText(selected.text, selected.targetApp);
            } finally {
                this.running = false;
            }
        },

        /**
         * 执行文本润色主链路。
         * 流程：校验正文、调用 FastAPI、记录历史；桌面目标存在时尝试粘贴并核验插入结果。
         * 参数：text 为待处理正文，targetApp 为桌面目标应用；Web 传空字符串。
         * 返回：处理完成 Promise。
         * 边界：HTTP 错误包含稳定 code/requestId；未确认插入时打开结果窗口且不宣称成功。
         */
        async polishText(text: string, targetApp: string): Promise<void> {
            const normalizedText = text.trim();
            if (!normalizedText) {
                this.message = '请输入或选中需要润色的文字。';
                return;
            }
            this.running = true;
            let fallbackMessage = '';
            try {
                const modelManageStore = useModelManageStore();
                await modelManageStore.refreshServiceModels();
                const selection = modelManageStore.resolveSelection('text', this.textModelId, '文本润色');
                if (!selection.modelId) throw new Error(selection.message);
                this.textModelId = selection.modelId;
                this.persistTextPolish();
                fallbackMessage = selection.message;
                this.message = fallbackMessage || '正在润色文字。';
                const processed = await processText({
                    modelId: this.textModelId,
                    mode: 'polish',
                    text: normalizedText,
                    audioDurationMs: 0,
                    dictionary: [],
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
                if (targetApp) {
                    const pasteResult = await pasteText(processed.processedText, targetApp);
                    if (!pasteResult.insertionVerified) {
                        await showResultWindow(
                            processed.processedText,
                            pasteResult.message,
                            pasteResult.requiresAccessibility
                        );
                        this.message = '文字已生成，但未能确认已插入目标输入框，请在结果窗口复制。';
                        return;
                    }
                }
                this.message = fallbackMessage ? `${fallbackMessage} 文字润色已完成。` : '文字润色已完成。';
            } catch (error) {
                const errorMessage = error instanceof Error ? error.message : '文字润色失败。';
                this.message = fallbackMessage ? `${fallbackMessage} ${errorMessage}` : errorMessage;
            } finally {
                this.running = false;
            }
        }
    }
});
