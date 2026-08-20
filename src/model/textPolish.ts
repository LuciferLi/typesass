// 文字实时润色历史项模型，用于保留本模块内部处理记录。
export type TextPolishHistoryItemModel = {
    // 历史 ID，本地生成用于列表渲染和删除。
    id: string;
    // 原始选中文本或输入文本。
    sourceText: string;
    // 润色后的文本。
    outputText: string;
    // 创建时间 ISO 字符串。
    createdAt: string;
};

/** 文本润色需持久化的模型选择，只保存服务目录的不透明 ID。 */
export interface TextPolishModelSelectionModel {
    /** 当前选中的文本模型 ID；目录尚未加载或无可用模型时为空。 */
    textModelId: string;
}

// 读取选中文本响应模型，来源于 Tauri read_selected_text 命令。
export type SelectedTextResponseModel = {
    // 通过系统复制快捷键读取到的选中文本。
    text: string;
    // 触发读取前的前台应用名称。
    targetApp: string;
    // 是否具备辅助功能权限。
    accessibilityTrusted: boolean;
    // 是否恢复了原剪贴板。
    clipboardRestored: boolean;
    // 剪贴板恢复说明。
    clipboardRestoreMessage: string;
    // 本次读取使用的系统触发方式。
    copyMethod: string;
};

/**
 * 桌面端自动粘贴结果。
 * 业务含义：区分“系统粘贴命令已发送”和“目标输入框已确认插入”，防止把不可验证的系统操作误报为成功。
 */
export interface PasteResponseModel {
    /** 系统粘贴命令是否已经发出。 */
    commandSent: boolean;
    /** 是否通过辅助功能读取确认目标输入框已经包含本次文本。 */
    insertionVerified: boolean;
    /** 面向用户和问题排查的执行说明。 */
    message: string;
    /** 是否需要用户授予辅助功能权限。 */
    requiresAccessibility: boolean;
    /** 本次实际尝试粘贴的目标应用。 */
    targetApp: string;
}
