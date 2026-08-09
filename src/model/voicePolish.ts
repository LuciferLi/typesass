// 语音润色历史项模型，用于保留本模块内部历史记录。
export type VoicePolishHistoryItemModel = {
    // 历史 ID，本地生成用于列表渲染和删除。
    id: string;
    // ASR 原文。
    sourceText: string;
    // 润色后的最终文本。
    outputText: string;
    // 触发时的前台应用。
    contextApp: string;
    // 创建时间 ISO 字符串。
    createdAt: string;
};

// 语音输入运行模式，用于区分仅 ASR 转文本和 ASR 后继续润色。
export type VoicePolishRunModeType = 'asr' | 'polish';

// 词典条目模型，用于语音转文字润色时保护专有名词。
export type DictionaryItemModel = {
    // 词条文本。
    word: string;
    // 创建时间 ISO 字符串。
    createdAt: string;
};

// 语音转写请求模型，映射 Tauri transcribe_audio 命令参数。
export type TranscribeRequestModel = {
    // API Key，空字符串时由原生侧读取会话、钥匙串或环境变量。
    apiKey: string;
    // OpenAI 兼容接口地址。
    baseUrl: string;
    // 语音识别模型名称。
    asrModel: string;
    // 识别语言，auto 表示自动识别。
    language: string;
    // 音频 MIME 类型。
    contentType: string;
    // 音频 base64 内容，不包含 Data URL 头。
    audioBase64: string;
};

// 语音转写响应模型，来源于 Tauri transcribe_audio 命令。
export type TranscribeResponseModel = {
    // 转写后的文本。
    text: string;
    // 服务端耗时，毫秒。
    elapsedMs: number;
    // 实际响应模型。
    model: string;
};

// 文本处理模式类型，复用原生侧已有模式枚举。
export type ProcessModeType = 'dictate' | 'polish';

// 文本处理请求模型，映射 Tauri process_text 命令参数。
export type ProcessTextRequestModel = {
    // API Key，空字符串时由原生侧读取会话、钥匙串或环境变量。
    apiKey: string;
    // OpenAI 兼容接口地址。
    baseUrl: string;
    // 文本模型名称。
    textModel: string;
    // 文本处理模式。
    mode: ProcessModeType;
    // 待处理文本。
    text: string;
    // 语音时长，非语音来源为 0。
    audioDurationMs: number;
    // 词典词条列表。
    dictionary: string[];
    // 目标语言列表，当前保留为空。
    targetLanguages: string[];
    // 触发时的前台应用。
    contextApp: string;
    // 本模块输出偏好。
    styleInstruction: string;
};

// 文本处理响应模型，来源于 Tauri process_text 命令。
export type ProcessTextResponseModel = {
    // 处理后的文本。
    processedText: string;
    // 服务端耗时，毫秒。
    elapsedMs: number;
    // 实际响应模型。
    model: string;
};
