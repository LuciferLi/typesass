import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type { AppThemeMode, SettingsModel } from '@/model/settings';
import {
    SETTINGS_TASK_CONCURRENCY_DEFAULT,
    SETTINGS_TASK_CONCURRENCY_MAX,
    SETTINGS_TASK_CONCURRENCY_MIN
} from '@/model/settings';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import {
    getLoginLaunch,
    getPublicApiTunnelStatus,
    isTauriRuntime,
    setLoginLaunch,
    setPublicApiTunnelEnabled
} from '@/service/tauri/command';

/**
 * 设置页状态，用于聚合持久化偏好、原生设置读取状态和操作反馈。
 */
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
                userEnglishName: '',
                launchAtLogin: false,
                themeMode: 'dark',
                smartVoiceEnhancement: true,
                taskConcurrencyLimit: SETTINGS_TASK_CONCURRENCY_DEFAULT,
                publicApiExternalAccessEnabled: false,
                publicApiSubdomain: ''
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
         * 边界：外部手动写入非法主题或非法并发数时保留当前值，避免 UI 状态抖动。
         */
        applyPersistedSettings(settings: unknown): void {
            if (!settings || typeof settings !== 'object') return;
            const nextSettings = settings as Partial<SettingsModel>;
            this.settings = {
                userEnglishName: normalizeUserEnglishName(nextSettings.userEnglishName, this.settings.userEnglishName),
                launchAtLogin: nextSettings.launchAtLogin ?? this.settings.launchAtLogin,
                themeMode:
                    nextSettings.themeMode === 'dark' || nextSettings.themeMode === 'light'
                        ? nextSettings.themeMode
                        : this.settings.themeMode,
                smartVoiceEnhancement: nextSettings.smartVoiceEnhancement ?? this.settings.smartVoiceEnhancement,
                taskConcurrencyLimit: normalizeTaskConcurrencyLimit(
                    nextSettings.taskConcurrencyLimit,
                    this.settings.taskConcurrencyLimit
                ),
                publicApiExternalAccessEnabled:
                    nextSettings.publicApiExternalAccessEnabled ?? this.settings.publicApiExternalAccessEnabled,
                publicApiSubdomain: normalizePublicApiSubdomain(
                    nextSettings.publicApiSubdomain,
                    this.settings.publicApiSubdomain
                )
            };
            if (typeof document !== 'undefined') {
                document.documentElement.classList.toggle('dark', this.settings.themeMode === 'dark');
            }
        },

        /**
         * 应用界面主题模式。
         * 流程：先根据传入模式更新 html.dark 类名；桌面端再保存客户端设置，普通 Web 仅维护当前页面状态。
         * 参数：themeMode 为目标主题模式。
         * 返回：无返回值。
         * 边界：运行在非浏览器环境时不访问 document；普通 Web 不调用 Tauri IPC，避免产生无效错误日志。
         */
        applyThemeMode(themeMode: AppThemeMode): void {
            this.settings.themeMode = themeMode;
            if (typeof document !== 'undefined') {
                document.documentElement.classList.toggle('dark', themeMode === 'dark');
            }
            if (isTauriRuntime()) {
                void writeClientJson(StorageKey.settings, this.settings);
            }
        },

        /**
         * 切换界面主题模式。
         * 流程：根据开关状态映射 dark/light，再复用 applyThemeMode 完成 DOM 类名和桌面端配置更新。
         * 参数：enabled 表示是否启用深色主题。
         * 返回：无返回值。
         * 边界：桌面端重复切换同一状态时仍会写入配置；普通 Web 仅更新当前页面。
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
            if (isTauriRuntime()) {
                void writeClientJson(StorageKey.settings, this.settings);
            }
            this.message = '';
        },

        /**
         * 保存用户英文名。
         * 流程：按 DNS 前缀友好的字符规则规范化输入，再写入客户端配置文件；若 HTTP API 外网访问已开启，则立即按新英文名重启隧道。
         * 参数：name 为用户输入英文名。
         * 返回：最新 HTTP API 外网访问地址；未开启外网访问时返回空字符串。
         * 边界：英文名会影响 HTTP API 固定域名，不会覆盖“我的应用”的自定义二级域名。
         */
        async updateUserEnglishName(name: string): Promise<string> {
            this.settings.userEnglishName = normalizeUserEnglishName(name, '');
            if (isTauriRuntime()) {
                await writeClientJson(StorageKey.settings, this.settings);
                if (this.settings.publicApiExternalAccessEnabled) {
                    const status = await setPublicApiTunnelEnabled(true);
                    this.settings.publicApiExternalAccessEnabled = status.enabled;
                    this.settings.publicApiSubdomain = status.subdomain ?? '';
                    await writeClientJson(StorageKey.settings, this.settings);
                    this.message = '';
                    return status.publicUrl ?? '';
                }
            }
            this.message = '';
            return '';
        },

        /**
         * 刷新公共 HTTP API 外网访问状态。
         * 流程：桌面端读取 Rust 运行时状态并同步到设置分区字段；普通 Web 保留本地配置值。
         * 参数：无。
         * 返回：刷新完成 Promise。
         * 边界：IPC 失败时不清空已有域名，避免误导用户认为配置已丢失。
         */
        async refreshPublicApiTunnelStatus(): Promise<void> {
            if (!isTauriRuntime()) return;
            const status = await getPublicApiTunnelStatus();
            this.settings.publicApiExternalAccessEnabled = status.enabled;
            this.settings.publicApiSubdomain = status.subdomain ?? '';
        },

        /**
         * 切换公共 HTTP API 外网访问。
         * 流程：调用 Rust 写配置并启停 frpc，成功后同步固定域名到页面状态。
         * 参数：enabled 为目标开关状态。
         * 返回：远程公网访问地址；关闭或尚未生成时返回空字符串。
         * 边界：失败时不乐观修改开关，调用方负责展示错误。
         */
        async togglePublicApiExternalAccess(enabled: boolean): Promise<string> {
            this.saving = true;
            try {
                const status = await setPublicApiTunnelEnabled(enabled);
                this.settings.publicApiExternalAccessEnabled = status.enabled;
                this.settings.publicApiSubdomain = status.subdomain ?? '';
                this.message = '';
                return status.publicUrl ?? '';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '保存 HTTP API 外网访问设置失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 保存任务执行并发上限。
         * 流程：把输入值收敛到 1-10 的整数，再写入客户端配置文件；后端调度器下一轮读取配置时即时生效。
         * 参数：limit 为用户输入的并发上限。
         * 返回：无返回值。
         * 边界：浏览器预览只更新当前内存状态；桌面端写入失败会由调用方通过控制台或页面状态感知。
         */
        updateTaskConcurrencyLimit(limit: number): void {
            this.settings.taskConcurrencyLimit = normalizeTaskConcurrencyLimit(
                limit,
                SETTINGS_TASK_CONCURRENCY_DEFAULT
            );
            if (isTauriRuntime()) {
                void writeClientJson(StorageKey.settings, this.settings);
            }
            this.message = '';
        },

        /**
         * 初始化原生系统设置。
         * 流程：读取真实开机启动状态并同步当前客户端 JSON。
         * 返回：初始化完成 Promise。
         * 边界：IPC 失败时保留现有状态并展示错误，不伪报已开启。
         */
        async initSettings(): Promise<void> {
            this.initializing = true;
            try {
                this.settings.launchAtLogin = await getLoginLaunch();
                await this.refreshPublicApiTunnelStatus();
                void writeClientJson(StorageKey.settings, this.settings);
            } catch (error) {
                this.message = error instanceof Error ? error.message : '读取系统设置失败。';
            } finally {
                this.initializing = false;
            }
        },

        /**
         * 切换开机自动启动。
         * 流程：先调用 Tauri 修改系统状态，成功后更新页面并持久化。
         * 参数：enabled 表示目标开关状态。
         * 返回：保存完成 Promise。
         * 边界：系统调用失败时不修改本地显示状态。
         */
        async toggleLaunchAtLogin(enabled: boolean): Promise<void> {
            this.saving = true;
            try {
                await setLoginLaunch(enabled);
                this.settings.launchAtLogin = enabled;
                void writeClientJson(StorageKey.settings, this.settings);
                this.message = '';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '保存系统设置失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        }
    }
});

/**
 * 规范化任务并发上限。
 * 流程：只接受有限数字，先取整再收敛到设置页允许范围；参数为外部输入值和兜底值。
 * 返回：可写入设置并被 Rust 调度器识别的整数。
 * 边界：非法值返回兜底值，避免损坏配置把并发变成 0 或无限大。
 */
function normalizeTaskConcurrencyLimit(value: unknown, fallback: number): number {
    if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
    return Math.min(SETTINGS_TASK_CONCURRENCY_MAX, Math.max(SETTINGS_TASK_CONCURRENCY_MIN, Math.floor(value)));
}

/**
 * 规范化用户英文名。
 * 流程：转换为小写，保留字母、数字和短横线，并压缩连续短横线。
 * 参数：value 为外部输入值，fallback 为非法输入兜底值。
 * 返回：适合作为域名前缀参考的英文名。
 * 边界：空值允许保存；超过 32 位会截断，避免提示域名过长。
 */
function normalizeUserEnglishName(value: unknown, fallback: string): string {
    if (typeof value !== 'string') return fallback;
    return value
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9-]/g, '-')
        .replace(/-+/g, '-')
        .replace(/^-|-$/g, '')
        .slice(0, 32);
}

/**
 * 规范化公共 HTTP API 固定二级域名前缀。
 * 流程：接受用户英文名或随机兜底域名的 DNS label；非法值使用兜底值。
 * 参数：value 为外部输入值，fallback 为当前已有值。
 * 返回：可展示的固定前缀。
 * 边界：该字段不允许用户编辑，前端只做展示级校验。
 */
function normalizePublicApiSubdomain(value: unknown, fallback: string): string {
    if (typeof value !== 'string') return fallback;
    const normalized = value.trim().toLowerCase();
    if (!/^[a-z0-9-]{1,32}$/.test(normalized)) return fallback;
    return normalized.startsWith('-') || normalized.endsWith('-') ? fallback : normalized;
}
