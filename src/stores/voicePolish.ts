import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { DictionaryItemModel, VoicePolishHistoryItemModel, VoicePolishRunModeType } from '@/model/voicePolish';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import { CLIENT_UNAVAILABLE_VOICE_MESSAGE, isTauriRuntime, runAppVoicePolish } from '@/service/tauri/command';

interface VoicePolishState {
    // 当前语音识别模型的不透明服务目录 ID。
    asrModelId: string;
    // 当前语音整理模型的不透明服务目录 ID。
    textModelId: string;
    // 本模块词典。
    dictionary: DictionaryItemModel[];
    // 本模块历史记录。
    history: VoicePolishHistoryItemModel[];
    // 输出风格偏好。
    styleInstruction: string;
    // 当前是否正在处理。
    running: boolean;
    // 最近一次输出。
    latestOutput: string;
    // 模块状态提示。
    message: string;
}

// 语音润色模块需要持久化到本地的字段。
type VoicePolishPersistedState = Pick<
    VoicePolishState,
    'asrModelId' | 'textModelId' | 'dictionary' | 'history' | 'styleInstruction'
>;

const defaultState: VoicePolishPersistedState = {
    asrModelId: '',
    textModelId: '',
    dictionary: [],
    history: [],
    styleInstruction: ''
};

export const useVoicePolishStore = defineStore('voicePolish', {
    state: (): VoicePolishState => {
        return {
            ...defaultState,
            running: false,
            latestOutput: '',
            message: ''
        };
    },
    getters: {
        // 词典字符串列表，用于传给原生文本处理命令。
        dictionaryWords: (state): string[] => state.dictionary.map((item) => item.word)
    },
    actions: {
        /**
         * 从客户端 JSON 配置文件初始化语音润色状态。
         * 流程：读取语音润色分区后写入当前 store。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：配置缺失时使用空词典、空历史和未选择模型的默认状态。
         */
        async hydrateVoicePolish(): Promise<void> {
            const saved = await readClientJson<typeof defaultState>(StorageKey.voicePolish, defaultState);
            this.applyPersistedVoicePolish(saved);
        },

        /**
         * 应用客户端 JSON 配置变化中的语音润色状态。
         * 流程：合并模型选择、词典、历史和输出偏好。
         * 参数：state 为配置文件中的语音润色分区。
         * 返回：无返回值。
         * 边界：数组字段非法时回退为空数组，字符串字段非法时保持空字符串。
         */
        applyPersistedVoicePolish(state: unknown): void {
            if (!state || typeof state !== 'object') return;
            const nextState = state as Partial<VoicePolishPersistedState>;
            this.asrModelId = typeof nextState.asrModelId === 'string' ? nextState.asrModelId : '';
            this.textModelId = typeof nextState.textModelId === 'string' ? nextState.textModelId : '';
            this.dictionary = Array.isArray(nextState.dictionary) ? nextState.dictionary : [];
            this.history = Array.isArray(nextState.history) ? nextState.history : [];
            this.styleInstruction = typeof nextState.styleInstruction === 'string' ? nextState.styleInstruction : '';
        },

        /**
         * 持久化语音模块状态。
         * 流程：桌面端写入词典、历史和输出偏好；Web 保持当前标签页内状态。
         * 返回：无。
         * 边界：不保存音频、Token、上游地址或模型密钥。
         */
        persistVoicePolish(): void {
            if (!isTauriRuntime()) return;
            void writeClientJson(StorageKey.voicePolish, {
                asrModelId: this.asrModelId,
                textModelId: this.textModelId,
                dictionary: this.dictionary,
                history: this.history,
                styleInstruction: this.styleInstruction
            });
        },

        /**
         * 批量添加词典词条。
         * 流程：按换行和常用分隔符拆分、去空白、去重后插入列表头并持久化。
         * 参数：input 为用户输入的一个或多个术语。
         * 返回：无。
         * 边界：空项和已存在项被忽略，不创建重复术语。
         */
        addDictionaryWords(input: string): void {
            const words = input
                .split(/[\n,，、]/)
                .map((word) => word.trim())
                .filter(Boolean);
            const existed = new Set(this.dictionary.map((item) => item.word));
            words.forEach((word) => {
                if (!existed.has(word)) {
                    this.dictionary.unshift({ word, createdAt: new Date().toISOString() });
                    existed.add(word);
                }
            });
            this.persistVoicePolish();
        },

        /**
         * 删除词典词条。
         * 流程：按完整文本过滤目标项并持久化剩余列表。
         * 参数：word 为待删除术语。
         * 返回：无。
         * 边界：目标不存在时保持列表不变。
         */
        removeDictionaryWord(word: string): void {
            this.dictionary = this.dictionary.filter((item) => item.word !== word);
            this.persistVoicePolish();
        },

        /**
         * 执行一次语音输入。
         * 流程：桌面端交给 Rust 主进程录音、识别、润色和粘贴；普通 Web 只展示产品提示，不调用浏览器麦克风。
         * 参数：targetApp 为触发时的前台应用，mode 为语音输入运行模式。
         * 返回：无返回值。
         * 边界：外部浏览器页面永不请求麦克风权限，避免权限归属变成浏览器。
         */
        async runVoicePolish(targetApp = '', mode: VoicePolishRunModeType = 'polish'): Promise<void> {
            if (!isTauriRuntime()) {
                this.message = `${CLIENT_UNAVAILABLE_VOICE_MESSAGE} 请使用 CodexMan App 的全局快捷键进行语音输入。`;
                return;
            }
            this.running = true;
            try {
                this.message = '正在录音，请稍后。';
                const result = await runAppVoicePolish(mode, targetApp);
                this.latestOutput = result.outputText;
                await this.hydrateVoicePolish();
                this.message = result.message;
            } catch (error) {
                const errorMessage = error instanceof Error ? error.message : '语音润色失败。';
                this.message = errorMessage;
            } finally {
                this.running = false;
            }
        }
    }
});
