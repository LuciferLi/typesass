// 字幕运行状态类型，用于区分监听、处理和错误态。
export type SubtitleRuntimeStateType = 'idle' | 'starting' | 'listening' | 'error';

// 字幕历史项模型，用于实时字幕模块内部历史记录。
export type SubtitleHistoryItemModel = {
    // 历史 ID，本地生成用于列表渲染。
    id: string;
    // 字幕文本。
    text: string;
    // 创建时间 ISO 字符串。
    createdAt: string;
};

// 字幕窗口消息模型，用于 Hub 与字幕窗口同步展示。
export type SubtitleMessagePayloadModel = {
    // 字幕窗口状态。
    state: SubtitleRuntimeStateType;
    // 当前展示文本。
    text: string;
    // 是否显示字幕气泡。
    visible: boolean;
};

// 字幕历史同步模型，用于独立字幕历史窗口刷新列表。
export type SubtitleHistoryUpdatePayloadModel = {
    // 字幕历史列表。
    items: SubtitleHistoryItemModel[];
    // 当前状态说明。
    status: string;
    // 是否正在监听。
    listening: boolean;
};
