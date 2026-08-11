import type { ShortcutProfileModel } from '@/model/permission';

// 默认快捷键配置，用于客户端配置缺失、网页预览和异常兜底。
export const DefaultShortcutProfile: ShortcutProfileModel = {
    asr: 'ctrl+shift+d',
    dictate: 'ctrl+p',
    polish: 'ctrl+shift+p'
};

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
    return [profile.asr, profile.dictate, profile.polish].some(
        (shortcut) => typeof shortcut === 'string' && shortcut.trim().length > 0
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
        polish: typeof profile.polish === 'string' ? profile.polish : fallback.polish
    };
}
