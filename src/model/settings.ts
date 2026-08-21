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
    // 用户英文名，用于生成 HTTP API 外网访问域名前缀，并作为“我的应用”二级域名建议参考。
    userEnglishName: string;
    // 是否开机自动启动，通过 Tauri 写入或删除用户级启动项。
    launchAtLogin: boolean;
    // 当前界面主题模式，写入 html class 后驱动 shadcn 颜色变量。
    themeMode: AppThemeMode;
    // 是否启用智能识音增强；开启后麦克风录音会优先请求降噪、回声消除、自动增益和单声道采集。
    smartVoiceEnhancement: boolean;
    // 任务管理同时提交到 Codex 的最大任务数，调度器会按该值补齐 running 槽位。
    taskConcurrencyLimit: number;
    // 公共 HTTP API 是否允许通过二级域名外网访问，默认关闭。
    publicApiExternalAccessEnabled: boolean;
    // 公共 HTTP API 固定二级域名前缀；优先使用用户英文名，未设置时由系统生成随机兜底值。
    publicApiSubdomain: string;
};

// 公共 HTTP API 外网访问状态，用于设置页持久化和文档页展示远程访问地址。
export type PublicApiTunnelStatusModel = {
    // 是否允许外网访问。
    enabled: boolean;
    // 固定二级域名前缀；关闭且从未生成时为空。
    subdomain: string | null;
    // 完整远程访问地址；未生成时为空。
    publicUrl: string | null;
    // 当前 frpc 隧道是否正在运行。
    running: boolean;
};
