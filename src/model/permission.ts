import type { OpenAppShortcutBindingModel } from '@/model/shortcutBinding';

// 权限状态项模型，用于权限管理页展示每类能力是否可用。
export type PermissionItemModel = {
    // 权限稳定键，用于页面渲染和操作分发。
    key: PermissionKeyType;
    // 权限名称，面向用户展示。
    name: string;
    // 权限说明，解释该权限影响哪些功能。
    description: string;
    // 是否已授权或已准备好。
    ready: boolean;
    // 当前状态说明，来自系统诊断或浏览器能力检测。
    message: string;
};

// 权限键类型，用于区分当前本地能力。
export type PermissionKeyType = 'microphone' | 'accessibility' | 'shortcut' | 'httpApi';

// Tauri 运行诊断模型，来源于原生 get_runtime_diagnostics 命令。
export type RuntimeDiagnosticsModel = {
    // 辅助功能权限是否已授权。
    accessibilityTrusted: boolean;
    // 当前桌面端实际保存的全局快捷键配置。
    shortcuts: ShortcutProfileModel;
    // 当前快捷键是否注册成功。
    shortcutRegistrationReady: boolean;
    // 快捷键注册状态说明。
    shortcutRegistrationMessage: string;
};

// 全局快捷键配置模型，来源于原生运行时并可提交给 register_shortcuts。
export type ShortcutProfileModel = {
    // ASR 仅转文本模式快捷键。
    asr: string;
    // 语音转文字并润色模式快捷键。
    dictate: string;
    // 选中文本润色模式快捷键。
    polish: string;
    // 用户创建的打开应用快捷键绑定。
    appBindings: OpenAppShortcutBindingModel[];
};
