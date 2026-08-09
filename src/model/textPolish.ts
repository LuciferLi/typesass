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
