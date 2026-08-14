// 主题模式模型，用于控制 shadcn 主题变量在浅色和深色之间切换。
export type AppThemeMode = 'light' | 'dark';

// 任务执行并发数默认值；必须与 Rust `CODEX_TASK_DEFAULT_CONCURRENT_RUNNING` 协议保持一致。
export const SETTINGS_TASK_CONCURRENCY_DEFAULT = 3;

// 任务执行并发数最小值；必须与 Rust `CODEX_TASK_MIN_CONCURRENT_RUNNING` 协议保持一致。
export const SETTINGS_TASK_CONCURRENCY_MIN = 1;

// 任务执行并发数最大值；必须与 Rust `CODEX_TASK_MAX_CONCURRENT_RUNNING` 协议保持一致。
export const SETTINGS_TASK_CONCURRENCY_MAX = 10;

// 系统设置模型，用于保存应用级偏好。
export type SettingsModel = {
    // 是否开机自动启动，通过 Tauri 写入或删除用户级启动项。
    launchAtLogin: boolean;
    // 当前界面主题模式，写入 html class 后驱动 shadcn 颜色变量。
    themeMode: AppThemeMode;
    // 是否启用智能识音增强；开启后麦克风录音会优先请求降噪、回声消除、自动增益和单声道采集。
    smartVoiceEnhancement: boolean;
    // 任务管理同时提交到 Codex 的最大任务数，调度器会按该值补齐 running 槽位。
    taskConcurrencyLimit: number;
};
