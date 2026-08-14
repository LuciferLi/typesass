import type { ShortcutProfileModel } from '@/model/permission';

// 默认快捷键配置，用于客户端配置缺失、网页预览和异常兜底。
export const DefaultShortcutProfile: ShortcutProfileModel = {
    asr: 'ctrl+shift+d',
    dictate: 'ctrl+p',
    polish: 'ctrl+shift+p',
    appBindings: []
};

/**
 * 判断未知值是否为有效的打开应用快捷键绑定。
 * 流程：检查基础字段类型和固定动作类型，只有目标 App 名称、路径、快捷键都存在时才保留。
 * 参数：value 为客户端 JSON 或原生诊断返回的未知列表项。
 * 返回：字段完整且动作类型合法时返回 true。
 * 边界：非法旧数据会被过滤，避免原生注册不存在目标的快捷键。
 */
function isValidOpenAppBinding(value: unknown): value is ShortcutProfileModel['appBindings'][number] {
    if (!value || typeof value !== 'object') return false;
    const binding = value as Partial<ShortcutProfileModel['appBindings'][number]>;
    return (
        typeof binding.id === 'string' &&
        typeof binding.shortcut === 'string' &&
        binding.actionType === 'openApp' &&
        typeof binding.appName === 'string' &&
        typeof binding.appPath === 'string' &&
        typeof binding.createdAt === 'string' &&
        binding.id.trim().length > 0 &&
        binding.shortcut.trim().length > 0 &&
        binding.appName.trim().length > 0 &&
        binding.appPath.trim().length > 0
    );
}

/**
 * 判断未知值是否包含任意快捷键字段。
 * 流程：检查值是否为对象，并判断至少一个快捷键字段为字符串。
 * 参数：value 为客户端 JSON 配置读取出的未知值。
 * 返回：如果值可作为快捷键配置来源则返回 true。
 * 边界：空对象或非法类型不视为有效配置，避免启动时用空配置覆盖原生默认快捷键。
 */
export function hasShortcutProfileValue(value: unknown): boolean {
    if (!value || typeof value !== 'object') return false;
    const profile = value as Partial<ShortcutProfileModel>;
    const hasDefaultShortcut = [profile.asr, profile.dictate, profile.polish].some(
        (shortcut) => typeof shortcut === 'string' && shortcut.trim().length > 0
    );
    return (
        hasDefaultShortcut || (Array.isArray(profile.appBindings) && profile.appBindings.some(isValidOpenAppBinding))
    );
}

/**
 * 合并快捷键配置并过滤非法字段。
 * 流程：以传入 fallback 为基础，只接收字符串快捷键字段，缺失字段沿用 fallback。
 * 参数：value 为待合并的配置值，fallback 为默认或当前快捷键配置。
 * 返回：完整快捷键配置。
 * 边界：不会在前端判断组合键冲突，冲突仍交给原生注册命令统一处理。
 */
export function normalizeShortcutProfileValue(
    value: unknown,
    fallback: ShortcutProfileModel = DefaultShortcutProfile
): ShortcutProfileModel {
    if (!value || typeof value !== 'object') return { ...fallback };
    const profile = value as Partial<ShortcutProfileModel>;
    return {
        asr: typeof profile.asr === 'string' ? profile.asr : fallback.asr,
        dictate: typeof profile.dictate === 'string' ? profile.dictate : fallback.dictate,
        polish: typeof profile.polish === 'string' ? profile.polish : fallback.polish,
        appBindings: Array.isArray(profile.appBindings)
            ? profile.appBindings.filter(isValidOpenAppBinding)
            : fallback.appBindings
    };
}

/**
 * 把快捷键字符串拆成展示片段。
 * 流程：按加号拆分后统一修饰键和主键大小写。
 * 参数：shortcut 为原生侧格式的快捷键字符串。
 * 返回：用于 Kbd 组件展示的片段列表。
 * 边界：空值时返回占位文本，避免 UI 空白。
 */
export function splitShortcutParts(shortcut: string): string[] {
    const parts = shortcut
        .split('+')
        .map((part) => formatShortcutPart(part))
        .filter(Boolean);
    return parts.length ? parts : ['未设置'];
}

/**
 * 从键盘事件生成快捷键字符串。
 * 流程：按 Ctrl/Cmd/Alt/Shift 顺序收集修饰键，再追加主键。
 * 参数：event 为用户按下组合键的事件。
 * 返回：原生侧可规范化的快捷键字符串；只有修饰键时返回空字符串。
 * 边界：不允许把 Ctrl、Shift、Alt、Meta 单独作为主键。
 */
export function normalizeKeyboardEvent(event: KeyboardEvent): string {
    const key = normalizeEventKey(event);
    if (!key) return '';
    const parts: string[] = [];
    if (event.ctrlKey) parts.push('ctrl');
    if (event.metaKey) parts.push('cmd');
    if (event.altKey) parts.push('alt');
    if (event.shiftKey) parts.push('shift');
    parts.push(key);
    return parts.join('+');
}

/**
 * 格式化单个快捷键片段。
 * 流程：修饰键使用常见英文缩写，普通按键首字母大写。
 * 参数：part 为原始快捷键片段。
 * 返回：面向用户展示的按键文本。
 * 边界：未知按键保持原文本，避免误丢信息。
 */
function formatShortcutPart(part: string): string {
    const normalized = part.trim().toLowerCase();
    const labelByPart: Record<string, string> = {
        ctrl: 'Ctrl',
        control: 'Ctrl',
        cmd: 'Cmd',
        meta: 'Cmd',
        alt: 'Alt',
        option: 'Alt',
        shift: 'Shift',
        space: 'Space'
    };
    if (labelByPart[normalized]) return labelByPart[normalized];
    return normalized ? normalized.slice(0, 1).toUpperCase() + normalized.slice(1) : '';
}

/**
 * 规范化 KeyboardEvent 主键。
 * 流程：过滤修饰键，兼容空格、字母和常见符号。
 * 参数：event 为用户按键事件。
 * 返回：可用于快捷键配置的主键。
 * 边界：只按修饰键返回空字符串，避免无效组合。
 */
function normalizeEventKey(event: KeyboardEvent): string {
    const key = event.key.toLowerCase();
    if (['control', 'shift', 'alt', 'meta'].includes(key)) return '';
    if (key === ' ') return 'space';
    if (key.length === 1) return key;
    return key.replace(/\s+/g, '');
}
