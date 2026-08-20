// 本地存储键名配置，统一管理避免各模块散落硬编码。
export const StorageKey = {
    settings: 'codexman.settings.v1',
    voicePolish: 'codexman.voicePolish.v1',
    textPolish: 'codexman.textPolish.v1',
    shortcuts: 'codexman.shortcuts.v1',
    sessionManage: 'codexman.sessionManage.v1'
} as const;
