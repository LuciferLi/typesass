/**
 * 快捷键绑定动作类型。
 * 业务含义：约束用户创建的快捷键当前只能执行打开应用动作，后续新增动作时从这里扩展。
 */
export type ShortcutBindingActionType = 'openApp';

/**
 * 可选择的本机应用模型。
 * 业务含义：由桌面端扫描本机应用目录后返回，供快捷键绑定表单选择目标 App。
 */
export interface ApplicationOptionModel {
    /** 应用展示名称，通常来源于 .app bundle 文件名。 */
    name: string;
    /** 应用 bundle 的绝对路径，用于触发快捷键时精确打开。 */
    path: string;
}

/**
 * 打开应用快捷键绑定模型。
 * 业务含义：用户创建的一条全局快捷键绑定，当前固定为打开某个本机 App。
 */
export interface OpenAppShortcutBindingModel {
    /** 绑定唯一 ID，用于列表渲染、删除和原生侧追踪。 */
    id: string;
    /** 用户录入的全局快捷键组合。 */
    shortcut: string;
    /** 动作类型，当前固定为 openApp。 */
    actionType: ShortcutBindingActionType;
    /** 目标应用展示名称。 */
    appName: string;
    /** 目标应用 bundle 绝对路径。 */
    appPath: string;
    /** 创建时间 ISO 字符串，用于稳定排序和后续审计。 */
    createdAt: string;
}
