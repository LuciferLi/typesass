import { defineStore } from 'pinia';

import type { PermissionItemModel } from '@/model/permission';
import {
    checkPublicApiHealth,
    getRuntimeDiagnostics,
    hasPublicApiToken,
    isTauriRuntime,
    openAccessibilitySettings,
    openInputMonitoringSettings,
    openMicrophoneSettings,
    requestInputMonitoringAccess,
    requestMicrophoneAccess
} from '@/service/tauri/command';

interface PermissionState {
    // 权限列表。
    items: PermissionItemModel[];
    // 是否正在刷新权限。
    loading: boolean;
    // 权限页状态提示。
    message: string;
}

type PermissionRefreshOptions = {
    // 兼容旧调用签名；麦克风权限现在只以 App 原生诊断为准。
    probeMicrophoneAccess?: boolean;
};

/**
 * 生成公共 HTTP 服务的可判断状态说明。
 * 流程：先判断网络连通，再判断当前运行会话是否持有 Token，避免把健康检查伪报为业务已就绪。
 * 参数：connected 表示健康检查通过；authorized 表示当前会话存在 Token。
 * 返回：未连接、等待授权或已连接并授权三种稳定文案。
 * 边界：authorized 只有在 connected 为 true 时才有业务意义，异常组合仍按未连接显示。
 */
function describePublicApiStatus(connected: boolean, authorized: boolean): string {
    if (!connected) return '未连接';
    return authorized ? '已连接并授权' : '已连接，等待授权';
}

/**
 * 生成桌面 App 内部 HTTP 服务状态说明。
 * 流程：桌面端业务请求由 App 主进程和受保护 IPC 协同完成，内部页面只需要确认本机服务连通，外部访问授权仍由普通 Web 分支检查。
 * 参数：connected 表示健康检查通过。
 * 返回：未连接或 App 内部已连接的稳定文案。
 * 边界：该说明只用于 Tauri WebView，不放宽浏览器和局域网访问的设备授权要求。
 */
function describeDesktopPublicApiStatus(connected: boolean): string {
    return connected ? 'App 内部服务已连接' : '未连接';
}

export const usePermissionStore = defineStore('permission', {
    state: (): PermissionState => {
        return {
            items: [],
            loading: false,
            message: ''
        };
    },
    getters: {
        /**
         * 判断语音功能是否就绪。
         * 流程：同时检查真实麦克风授权和公共 HTTP 会话授权状态。
         * 返回：两项均就绪时返回 true。
         * 边界：任一诊断缺失时按未就绪处理，避免假成功。
         */
        voiceReady: (state): boolean => {
            const microphone = state.items.find((item) => item.key === 'microphone');
            const httpApi = state.items.find((item) => item.key === 'httpApi');
            return Boolean(microphone?.ready && httpApi?.ready);
        },
        /**
         * 判断桌面选区润色是否就绪。
         * 流程：同时检查辅助功能权限和公共 HTTP 会话授权状态。
         * 返回：两项均就绪时返回 true。
         * 边界：普通 Web 的辅助功能固定未就绪，但页面输入润色仍可直接调用 HTTP。
         */
        textPolishReady: (state): boolean => {
            const accessibility = state.items.find((item) => item.key === 'accessibility');
            const httpApi = state.items.find((item) => item.key === 'httpApi');
            return Boolean(accessibility?.ready && httpApi?.ready);
        }
    },
    actions: {
        /**
         * 刷新系统权限与 HTTP 服务状态。
         * 流程：Web 只检查公共服务；桌面读取 CodexMan App 的麦克风、辅助功能和快捷键真实诊断。
         * 返回：刷新完成 Promise。
         * 边界：不把 CORS 或健康检查当作业务鉴权；异常写入 message 供页面排障。
         */
        async refreshPermissions(options: PermissionRefreshOptions = {}): Promise<void> {
            void options;
            this.loading = true;
            try {
                if (!isTauriRuntime()) {
                    const publicApiConnected = await checkPublicApiHealth();
                    const publicApiAuthorized = publicApiConnected && (await hasPublicApiToken());
                    this.items = [
                        {
                            key: 'httpApi',
                            name: 'HTTP 服务',
                            description: '语音识别和文本润色通过独立 HTTP 服务执行。',
                            ready: publicApiAuthorized,
                            message: describePublicApiStatus(publicApiConnected, publicApiAuthorized)
                        },
                        {
                            key: 'microphone',
                            name: '麦克风',
                            description: '语音转文字和语音转文字润色由 CodexMan App 录音。',
                            ready: false,
                            message: '请在 CodexMan App 中授权'
                        },
                        {
                            key: 'accessibility',
                            name: '辅助功能',
                            description: '润色读取选中文本和自动粘贴需要。',
                            ready: false,
                            message: '仅桌面自动粘贴需要'
                        },
                        {
                            key: 'inputMonitoring',
                            name: '输入监控',
                            description: '在其它应用中响应语音快捷键需要。',
                            ready: false,
                            message: '请在 CodexMan App 中授权'
                        },
                        {
                            key: 'shortcut',
                            name: '全局快捷键',
                            description: '后台触发语音和润色动作需要。',
                            ready: false,
                            message: '仅桌面后台操作需要'
                        }
                    ];
                    return;
                }
                const diagnostics = await getRuntimeDiagnostics();
                const microphoneReady = Boolean(diagnostics?.microphoneAuthorized);
                const publicApiConnected = await checkPublicApiHealth();
                this.items = [
                    {
                        key: 'httpApi',
                        name: 'HTTP 服务',
                        description: '语音识别和文本润色通过独立 HTTP 服务执行。',
                        ready: publicApiConnected,
                        message: describeDesktopPublicApiStatus(publicApiConnected)
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
                        key: 'inputMonitoring',
                        name: '输入监控',
                        description: '在其它应用中响应语音快捷键需要。',
                        ready: Boolean(diagnostics?.inputMonitoringTrusted),
                        message: diagnostics?.inputMonitoringTrusted ? '已授权' : '未授权'
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

        /**
         * 打开指定系统权限设置。
         * 流程：麦克风先主动请求系统授权弹窗，拒绝或已被系统拒绝时再打开 macOS 设置；辅助功能直接打开设置。
         * 参数：key 为权限稳定键。
         * 返回：打开完成 Promise。
         * 边界：HTTP 服务与快捷键状态不映射系统设置，因此不会触发命令。
         */
        async openPermission(key: PermissionItemModel['key']): Promise<void> {
            if (key === 'microphone') {
                if (!isTauriRuntime()) {
                    this.message = '请在 CodexMan App 中授权麦克风。';
                    await this.refreshPermissions();
                    return;
                }
                const granted = await requestMicrophoneAccess();
                this.message = granted
                    ? '麦克风权限已开启。'
                    : '未获得麦克风权限，已打开系统设置，请在列表中允许 CodexMan。';
                if (!granted) await openMicrophoneSettings();
                await this.refreshPermissions();
            }
            if (key === 'accessibility') {
                await openAccessibilitySettings();
                await this.refreshPermissions();
            }
            if (key === 'inputMonitoring') {
                if (!isTauriRuntime()) {
                    this.message = '请在 CodexMan App 中开启输入监控。';
                    await this.refreshPermissions();
                    return;
                }
                const granted = await requestInputMonitoringAccess();
                this.message = granted
                    ? '输入监控权限已开启。'
                    : '未获得输入监控权限，已打开系统设置，请在列表中允许 CodexMan。';
                if (!granted) await openInputMonitoringSettings();
                await this.refreshPermissions();
            }
        }
    }
});
