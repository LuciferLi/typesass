import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { AppThemeMode, SettingsModel } from '@/model/settings';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import {
    getLoginLaunch,
    resetSessionTaskSchema as resetSessionTaskSchemaCommand,
    setLoginLaunch
} from '@/service/tauri/command';

interface SettingsState {
    // 系统设置表单。
    settings: SettingsModel;
    // 是否正在读取原生系统设置。
    initializing: boolean;
    // 设置保存状态。
    saving: boolean;
    // 设置页提示文案。
    message: string;
}

export const useSettingsStore = defineStore('settings', {
    state: (): SettingsState => {
        return {
            settings: {
                launchAtLogin: false,
                themeMode: 'dark',
                smartVoiceEnhancement: true
            },
            initializing: false,
            saving: false,
            message: ''
        };
    },
    actions: {
        /**
         * 从客户端 JSON 配置文件初始化系统设置。
         * 流程：读取设置分区并与默认值合并，然后立即应用主题 class。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：配置缺失时保持默认深色主题和智能识音增强开启。
         */
        async hydrateSettings(): Promise<void> {
            const savedSettings = await readClientJson<Partial<SettingsModel>>(StorageKey.settings, {});
            this.applyPersistedSettings(savedSettings);
        },

        /**
         * 应用客户端 JSON 配置变化中的系统设置。
         * 流程：按字段合并有效值，再同步页面主题 class。
         * 参数：settings 为配置文件中的设置分区。
         * 返回：无返回值。
         * 边界：外部手动写入非法主题时保留当前主题，避免 UI 状态抖动。
         */
        applyPersistedSettings(settings: unknown): void {
            if (!settings || typeof settings !== 'object') return;
            const nextSettings = settings as Partial<SettingsModel>;
            this.settings = {
                launchAtLogin: nextSettings.launchAtLogin ?? this.settings.launchAtLogin,
                themeMode:
                    nextSettings.themeMode === 'dark' || nextSettings.themeMode === 'light'
                        ? nextSettings.themeMode
                        : this.settings.themeMode,
                smartVoiceEnhancement: nextSettings.smartVoiceEnhancement ?? this.settings.smartVoiceEnhancement
            };
            if (typeof document !== 'undefined') {
                document.documentElement.classList.toggle('dark', this.settings.themeMode === 'dark');
            }
        },

        /**
         * 应用界面主题模式。
         * 流程：先根据传入模式更新 html.dark 类名，再保存本地设置，保证刷新后继续沿用用户选择。
         * 参数：themeMode 为目标主题模式。
         * 返回：无返回值。
         * 边界：运行在非浏览器环境时不访问 document，避免服务端或测试环境报错。
         */
        applyThemeMode(themeMode: AppThemeMode): void {
            this.settings.themeMode = themeMode;
            if (typeof document !== 'undefined') {
                document.documentElement.classList.toggle('dark', themeMode === 'dark');
            }
            void writeClientJson(StorageKey.settings, this.settings);
        },

        /**
         * 切换界面主题模式。
         * 流程：根据开关状态映射 dark/light，再复用 applyThemeMode 完成 DOM 类名和本地存储更新。
         * 参数：enabled 表示是否启用深色主题。
         * 返回：无返回值。
         * 边界：重复切换同一状态时仍会写入本地存储，确保缺失字段被补齐。
         */
        toggleThemeMode(enabled: boolean): void {
            this.applyThemeMode(enabled ? 'dark' : 'light');
        },

        /**
         * 切换智能识音增强。
         * 流程：根据开关状态更新应用级偏好并写入本地存储，后续所有共用录音入口都会读取该设置。
         * 参数：enabled 表示是否在麦克风录音时请求系统级音频增强能力。
         * 返回：无返回值。
         * 边界：浏览器或 WebView 不支持某些音频约束时，由录音入口自动退回普通录音。
         */
        toggleSmartVoiceEnhancement(enabled: boolean): void {
            this.settings.smartVoiceEnhancement = enabled;
            void writeClientJson(StorageKey.settings, this.settings);
            this.message = '声音设置已保存。';
        },

        // 初始化系统设置，优先读取原生开机启动真实状态。
        async initSettings(): Promise<void> {
            this.initializing = true;
            try {
                this.settings.launchAtLogin = await getLoginLaunch();
                void writeClientJson(StorageKey.settings, this.settings);
            } catch (error) {
                this.message = error instanceof Error ? error.message : '读取系统设置失败。';
            } finally {
                this.initializing = false;
            }
        },

        // 切换开机自动启动状态。
        async toggleLaunchAtLogin(enabled: boolean): Promise<void> {
            this.saving = true;
            try {
                await setLoginLaunch(enabled);
                this.settings.launchAtLogin = enabled;
                void writeClientJson(StorageKey.settings, this.settings);
                this.message = '系统设置已保存。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '保存系统设置失败。';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 恢复会话和任务管理最新表结构。
         * 流程：调用 Tauri 删除并重建任务管理 SQLite 业务表，同时清空项目、任务、会话和事件数据。
         * 参数：无。
         * 返回：恢复完成 Promise。
         * 边界：不会清理客户端 JSON 设置、主题、快捷键、模型配置或钥匙串 API Key。
         */
        async resetSessionTaskSchema(): Promise<void> {
            this.saving = true;
            try {
                await resetSessionTaskSchemaCommand();
                this.message = '会话和任务管理表结构已恢复，任务数据已清空。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '恢复表结构失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        }
    }
});
