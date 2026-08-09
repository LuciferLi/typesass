import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { SubtitleHistoryItemModel, SubtitleRuntimeStateType } from '@/model/subtitle';
import { blobToBase64, recordAudioOnce } from '@/service/speech/audioRecorder';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import {
    emitSubtitleHistory,
    emitSubtitleMessage,
    hideSubtitleWindows,
    showSubtitleWindows,
    transcribeAudio
} from '@/service/tauri/command';
import { useModelManageStore } from '@/stores/modelManage';
import { useSettingsStore } from '@/stores/settings';

interface SubtitleState {
    // 当前选择的 ASR 模型 ID。
    selectedAsrModelId: string;
    // 字幕历史，仅归属于实时字幕模块。
    history: SubtitleHistoryItemModel[];
    // 字幕运行状态。
    runtimeState: SubtitleRuntimeStateType;
    // 当前字幕文本。
    currentText: string;
    // 状态提示。
    message: string;
    // 当前是否正在监听。
    listening: boolean;
}

const defaultState = {
    selectedAsrModelId: '',
    history: []
};

// 字幕模块需要持久化到客户端 JSON 配置文件的字段。
type SubtitlePersistedState = typeof defaultState;

let subtitleLoopActive = false;

export const useSubtitleStore = defineStore('subtitle', {
    state: (): SubtitleState => {
        return {
            ...defaultState,
            runtimeState: 'idle',
            currentText: '',
            message: '等待开始实时字幕。',
            listening: false
        };
    },
    actions: {
        /**
         * 从客户端 JSON 配置文件初始化字幕模块状态。
         * 流程：读取字幕分区并应用模型选择和历史列表，运行态仍保持当前会话默认值。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：配置缺失时使用未选择模型和空历史。
         */
        async hydrateSubtitle(): Promise<void> {
            const saved = await readClientJson<SubtitlePersistedState>(StorageKey.subtitle, defaultState);
            this.applyPersistedSubtitle(saved);
        },

        /**
         * 应用客户端 JSON 配置变化中的字幕模块状态。
         * 流程：只刷新可持久化字段，不触碰当前监听状态和窗口可见状态。
         * 参数：state 为配置文件中的字幕分区。
         * 返回：无返回值。
         * 边界：历史字段非法时回退为空数组。
         */
        applyPersistedSubtitle(state: unknown): void {
            if (!state || typeof state !== 'object') return;
            const nextState = state as Partial<SubtitlePersistedState>;
            this.selectedAsrModelId =
                typeof nextState.selectedAsrModelId === 'string' ? nextState.selectedAsrModelId : '';
            this.history = Array.isArray(nextState.history) ? nextState.history : [];
        },

        // 持久化字幕模块状态到客户端 JSON 配置文件。
        persistSubtitle(): void {
            void writeClientJson(StorageKey.subtitle, {
                selectedAsrModelId: this.selectedAsrModelId,
                history: this.history
            });
        },

        // 更新 ASR 模型选择。
        updateAsrModel(modelId: string): void {
            this.selectedAsrModelId = modelId;
            this.persistSubtitle();
        },

        // 切换实时字幕监听。
        async toggleSubtitle(): Promise<void> {
            if (this.listening) {
                await this.stopSubtitle();
                return;
            }
            await this.startSubtitle();
        },

        // 开始实时字幕监听。
        async startSubtitle(): Promise<void> {
            const modelStore = useModelManageStore();
            const asrModel = modelStore.modelById(this.selectedAsrModelId);
            if (!asrModel) {
                this.message = '请先选择可用的 ASR 模型。';
                return;
            }
            subtitleLoopActive = true;
            this.listening = true;
            this.runtimeState = 'starting';
            this.message = '正在启动实时字幕。';
            await showSubtitleWindows();
            await this.syncSubtitleWindows();
            void this.runSubtitleLoop();
        },

        // 停止实时字幕监听。
        async stopSubtitle(): Promise<void> {
            subtitleLoopActive = false;
            this.listening = false;
            this.runtimeState = 'idle';
            this.currentText = '';
            this.message = '实时字幕已停止。';
            await emitSubtitleMessage({ state: 'idle', text: '', visible: false });
            await this.syncSubtitleHistory();
            await hideSubtitleWindows();
        },

        // 执行字幕循环，按短音频片段连续识别。
        async runSubtitleLoop(): Promise<void> {
            while (subtitleLoopActive) {
                const modelStore = useModelManageStore();
                const asrModel = modelStore.modelById(this.selectedAsrModelId);
                if (!asrModel) {
                    this.runtimeState = 'error';
                    this.message = 'ASR 模型不可用。';
                    break;
                }
                try {
                    this.runtimeState = 'listening';
                    this.message = '正在监听声音。';
                    await emitSubtitleMessage({
                        state: 'listening',
                        text: this.currentText || '等待声音',
                        visible: true
                    });
                    const settingsStore = useSettingsStore();
                    const audio = await recordAudioOnce(3200, {
                        enabled: settingsStore.settings.smartVoiceEnhancement
                    });
                    const audioBase64 = await blobToBase64(audio.blob);
                    const transcribed = await transcribeAudio({
                        apiKey: asrModel.apiKey,
                        baseUrl: asrModel.baseUrl,
                        asrModel: asrModel.model,
                        language: 'auto',
                        contentType: audio.contentType,
                        audioBase64
                    });
                    const text = transcribed.text.trim();
                    if (text) {
                        this.currentText = text;
                        this.history.unshift({
                            id: `subtitle-${Date.now()}`,
                            text,
                            createdAt: new Date().toISOString()
                        });
                        this.history = this.history.slice(0, 120);
                        this.persistSubtitle();
                        await this.syncSubtitleWindows();
                    }
                } catch (error) {
                    this.runtimeState = 'error';
                    this.message = error instanceof Error ? error.message : '实时字幕识别失败。';
                    await emitSubtitleMessage({ state: 'error', text: this.message, visible: true });
                    subtitleLoopActive = false;
                    this.listening = false;
                }
            }
        },

        // 同步字幕文本和历史窗口。
        async syncSubtitleWindows(): Promise<void> {
            await emitSubtitleMessage({ state: this.runtimeState, text: this.currentText, visible: this.listening });
            await this.syncSubtitleHistory();
        },

        // 同步字幕历史窗口。
        async syncSubtitleHistory(): Promise<void> {
            await emitSubtitleHistory({ items: this.history, status: this.message, listening: this.listening });
        },

        // 清空字幕历史。
        async clearHistory(): Promise<void> {
            this.history = [];
            this.persistSubtitle();
            await this.syncSubtitleHistory();
        }
    }
});
