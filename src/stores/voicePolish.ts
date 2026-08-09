import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { DictionaryItemModel, VoicePolishHistoryItemModel, VoicePolishRunModeType } from '@/model/voicePolish';
import { blobToBase64, recordAudioOnce } from '@/service/speech/audioRecorder';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import {
    CLIENT_UNAVAILABLE_VOICE_MESSAGE,
    isTauriRuntime,
    pasteText,
    processText,
    transcribeAudio
} from '@/service/tauri/command';
import { useModelManageStore } from '@/stores/modelManage';
import { useSettingsStore } from '@/stores/settings';

interface VoicePolishState {
    // 当前选择的 ASR 模型 ID。
    selectedAsrModelId: string;
    // 当前选择的文本模型 ID。
    selectedTextModelId: string;
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
    // 非客户端提示弹窗是否打开。
    clientUnavailableDialogOpen: boolean;
}

// 语音润色模块需要持久化到本地的字段。
type VoicePolishPersistedState = Pick<
    VoicePolishState,
    'selectedAsrModelId' | 'selectedTextModelId' | 'dictionary' | 'history' | 'styleInstruction'
>;

const invalidPreviewVoiceText = '网页预览模式无法调用桌面端语音识别。';

const defaultState: VoicePolishPersistedState = {
    selectedAsrModelId: '',
    selectedTextModelId: '',
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
            message: '',
            clientUnavailableDialogOpen: false
        };
    },
    getters: {
        // 词典字符串列表，用于传给原生文本处理命令。
        dictionaryWords: (state): string[] => state.dictionary.map((item) => item.word)
    },
    actions: {
        /**
         * 从客户端 JSON 配置文件初始化语音润色状态。
         * 流程：读取语音润色分区，过滤历史预览占位数据后写入当前 store。
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
         * 流程：合并模型选择、词典、历史和输出偏好，并过滤历史预览占位数据。
         * 参数：state 为配置文件中的语音润色分区。
         * 返回：无返回值。
         * 边界：数组字段非法时回退为空数组，字符串字段非法时保持空字符串。
         */
        applyPersistedVoicePolish(state: unknown): void {
            if (!state || typeof state !== 'object') return;
            const nextState = state as Partial<VoicePolishPersistedState>;
            this.selectedAsrModelId =
                typeof nextState.selectedAsrModelId === 'string' ? nextState.selectedAsrModelId : '';
            this.selectedTextModelId =
                typeof nextState.selectedTextModelId === 'string' ? nextState.selectedTextModelId : '';
            this.dictionary = Array.isArray(nextState.dictionary) ? nextState.dictionary : [];
            this.history = Array.isArray(nextState.history)
                ? nextState.history.filter(
                      (item) =>
                          item.sourceText !== invalidPreviewVoiceText && item.outputText !== invalidPreviewVoiceText
                  )
                : [];
            this.styleInstruction = typeof nextState.styleInstruction === 'string' ? nextState.styleInstruction : '';
        },

        // 持久化语音润色模块状态到客户端 JSON 配置文件。
        persistVoicePolish(): void {
            void writeClientJson(StorageKey.voicePolish, {
                selectedAsrModelId: this.selectedAsrModelId,
                selectedTextModelId: this.selectedTextModelId,
                dictionary: this.dictionary,
                history: this.history,
                styleInstruction: this.styleInstruction
            });
        },

        // 添加词典词条。
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

        // 删除词典词条。
        removeDictionaryWord(word: string): void {
            this.dictionary = this.dictionary.filter((item) => item.word !== word);
            this.persistVoicePolish();
        },

        // 更新模型选择。
        updateModelSelection(asrModelId: string, textModelId: string): void {
            this.selectedAsrModelId = asrModelId;
            this.selectedTextModelId = textModelId;
            this.persistVoicePolish();
        },

        // 打开非客户端语音转换提示；由全局组件库 Dialog 读取该状态展示，不写入历史数据。
        showClientUnavailableDialog(): void {
            this.message = CLIENT_UNAVAILABLE_VOICE_MESSAGE;
            this.clientUnavailableDialogOpen = true;
        },

        /**
         * 执行一次语音输入。
         * 流程：先录音并通过 ASR 转成文本；asr 模式直接粘贴转写文本，polish 模式继续调用文本模型润色后粘贴。
         * 参数：targetApp 为触发时的前台应用，mode 为语音输入运行模式。
         * 返回：无返回值。
         * 边界：非客户端环境只展示提示；asr 模式只要求 ASR 模型可用，polish 模式同时要求 ASR 和润色模型可用。
         */
        async runVoicePolish(targetApp = '', mode: VoicePolishRunModeType = 'polish'): Promise<void> {
            if (!isTauriRuntime()) {
                this.showClientUnavailableDialog();
                return;
            }
            const modelStore = useModelManageStore();
            const asrModel = modelStore.modelById(this.selectedAsrModelId);
            const textModel = modelStore.modelById(this.selectedTextModelId);
            if (!asrModel || (mode === 'polish' && !textModel)) {
                this.message = mode === 'asr' ? '请先选择可用的 ASR 模型。' : '请先选择可用的 ASR 模型和润色模型。';
                return;
            }
            this.running = true;
            this.message = '正在录音。';
            try {
                const settingsStore = useSettingsStore();
                const audio = await recordAudioOnce(30000, {
                    enabled: settingsStore.settings.smartVoiceEnhancement
                });
                this.message = '正在识别语音。';
                const audioBase64 = await blobToBase64(audio.blob);
                const transcribed = await transcribeAudio({
                    apiKey: asrModel.apiKey,
                    baseUrl: asrModel.baseUrl,
                    asrModel: asrModel.model,
                    language: 'auto',
                    contentType: audio.contentType,
                    audioBase64
                });
                let outputText = transcribed.text;
                if (mode === 'polish' && textModel) {
                    this.message = '正在润色文本。';
                    const processed = await processText({
                        apiKey: textModel.apiKey,
                        baseUrl: textModel.baseUrl,
                        textModel: textModel.model,
                        mode: 'dictate',
                        text: transcribed.text,
                        audioDurationMs: audio.durationMs,
                        dictionary: this.dictionaryWords,
                        targetLanguages: [],
                        contextApp: targetApp,
                        styleInstruction: this.styleInstruction
                    });
                    outputText = processed.processedText;
                }
                this.latestOutput = outputText;
                this.history.unshift({
                    id: `voice-${Date.now()}`,
                    sourceText: transcribed.text,
                    outputText,
                    contextApp: targetApp,
                    createdAt: new Date().toISOString()
                });
                this.history = this.history.slice(0, 80);
                this.persistVoicePolish();
                await pasteText(outputText, targetApp);
                this.message = mode === 'asr' ? '语音转文字已完成。' : '语音润色已完成。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '语音润色失败。';
            } finally {
                this.running = false;
            }
        }
    }
});
