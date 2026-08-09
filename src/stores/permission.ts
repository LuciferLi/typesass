import { defineStore } from 'pinia';

import type { PermissionItemModel } from '@/model/permission';
import {
    getRuntimeDiagnostics,
    isTauriRuntime,
    openAccessibilitySettings,
    openMicrophoneSettings
} from '@/service/tauri/command';

interface PermissionState {
    // 权限列表。
    items: PermissionItemModel[];
    // 是否正在刷新权限。
    loading: boolean;
    // API Key 输入值。
    apiKey: string;
    // 权限页状态提示。
    message: string;
}

export const usePermissionStore = defineStore('permission', {
    state: (): PermissionState => {
        return {
            items: [],
            loading: false,
            apiKey: '',
            message: ''
        };
    },
    getters: {
        // 当前关键权限是否满足语音功能使用。
        voiceReady: (state): boolean => {
            const microphone = state.items.find((item) => item.key === 'microphone');
            const apiKey = state.items.find((item) => item.key === 'apiKey');
            return Boolean(microphone?.ready && apiKey?.ready);
        },
        // 当前关键权限是否满足文本润色。
        textPolishReady: (state): boolean => {
            const accessibility = state.items.find((item) => item.key === 'accessibility');
            const apiKey = state.items.find((item) => item.key === 'apiKey');
            return Boolean(accessibility?.ready && apiKey?.ready);
        }
    },
    actions: {
        // 刷新本机权限诊断。
        async refreshPermissions(): Promise<void> {
            this.loading = true;
            try {
                if (!isTauriRuntime()) {
                    this.items = [
                        {
                            key: 'apiKey',
                            name: 'API Key',
                            description: '语音识别和文本润色都会使用。',
                            ready: false,
                            message: '未授权'
                        },
                        {
                            key: 'microphone',
                            name: '麦克风',
                            description: '语音转文字和语音转文字润色需要。',
                            ready: false,
                            message: '未授权'
                        },
                        {
                            key: 'accessibility',
                            name: '辅助功能',
                            description: '润色读取选中文本和自动粘贴需要。',
                            ready: false,
                            message: '未授权'
                        },
                        {
                            key: 'shortcut',
                            name: '全局快捷键',
                            description: '后台触发语音和润色动作需要。',
                            ready: false,
                            message: '未授权'
                        }
                    ];
                    return;
                }
                const diagnostics = await getRuntimeDiagnostics();
                const microphoneReady = await navigator.permissions
                    ?.query({ name: 'microphone' as PermissionName })
                    .then((permission) => permission.state === 'granted')
                    .catch(() => false);
                const hasApiKey = Boolean(
                    diagnostics?.hasSessionApiKey || diagnostics?.hasKeychainApiKey || diagnostics?.hasEnvApiKey
                );
                this.items = [
                    {
                        key: 'apiKey',
                        name: 'API Key',
                        description: '语音识别和文本润色都会使用。',
                        ready: hasApiKey,
                        message: hasApiKey ? '已配置' : '未配置'
                    },
                    {
                        key: 'microphone',
                        name: '麦克风',
                        description: '语音转文字和语音转文字润色需要。',
                        ready: microphoneReady,
                        message: microphoneReady ? '已授权' : '未授权'
                    },
                    {
                        key: 'accessibility',
                        name: '辅助功能',
                        description: '润色读取选中文本和自动粘贴需要。',
                        ready: Boolean(diagnostics?.accessibilityTrusted),
                        message: diagnostics?.accessibilityTrusted ? '已授权' : '未授权'
                    },
                    {
                        key: 'shortcut',
                        name: '全局快捷键',
                        description: '后台触发语音和润色动作需要。',
                        ready: Boolean(diagnostics?.shortcutRegistrationReady),
                        message: diagnostics?.shortcutRegistrationMessage || '桌面端运行后检测'
                    }
                ];
            } catch (error) {
                this.message = error instanceof Error ? error.message : '刷新权限失败。';
            } finally {
                this.loading = false;
            }
        },

        // 打开指定权限的系统设置入口。
        async openPermission(key: PermissionItemModel['key']): Promise<void> {
            if (key === 'microphone') await openMicrophoneSettings();
            if (key === 'accessibility') await openAccessibilitySettings();
        }
    }
});
