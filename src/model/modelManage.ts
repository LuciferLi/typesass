// 模型分组类型，用于区分语音识别模型和文本大模型。
export type ModelGroupType = 'asr' | 'text';

// 模型来源类型，用于区分厂商预设和自定义中转站。
export type ModelSourceType = 'vendor' | 'custom';

// 厂商预设键，用于在添加模型时快速填充请求地址和模型 ID。
export type ModelVendorKey =
    | 'xiaomi-asr'
    | 'xiaomi-text'
    | 'openai'
    | 'deepseek'
    | 'qwen'
    | 'qwen-asr'
    | 'gemini'
    | 'kimi'
    | 'zhipu'
    | 'volcengine';

// 模型配置项模型，用于模型管理页维护本地可选模型。
export type ModelItemModel = {
    // 模型本地唯一 ID，用于各业务模块保存选择关系。
    id: string;
    // 模型展示名称，面向用户显示。
    name: string;
    // 模型分组，asr 表示语音识别，text 表示文本大模型。
    group: ModelGroupType;
    // OpenAI 兼容接口地址，允许不同模型走不同中转。
    baseUrl: string;
    // 实际发送给接口的模型名称。
    model: string;
    // 调用该模型使用的 API Key，由添加模型时填写并保存在本地。
    apiKey: string;
    // 模型来源，vendor 表示厂商预设，custom 表示用户自定义中转站。
    source: ModelSourceType;
    // 厂商预设键，自定义中转站为空。
    vendorKey: ModelVendorKey | '';
    // 模型说明，用于备注供应商、用途或限制。
    remark: string;
    // 创建时间 ISO 字符串，用于排序和本地排查。
    createdAt: string;
};

// 模型表单模型，用于新增和编辑模型配置。
export type ModelFormModel = {
    // 模型展示名称，厂商预设会自动生成，自定义中转站由用户填写。
    name: string;
    // 模型分组。
    group: ModelGroupType;
    // OpenAI 兼容接口地址，厂商预设会自动填充，自定义中转站由用户填写。
    baseUrl: string;
    // 实际模型名称，厂商预设会自动填充，自定义中转站由用户填写。
    model: string;
    // 调用模型所需 API Key。
    apiKey: string;
    // 模型来源。
    source: ModelSourceType;
    // 厂商预设键。
    vendorKey: ModelVendorKey | '';
    // 模型备注。
    remark: string;
};

// 厂商预设模型，用于添加模型弹窗按模型类型展示可选厂商。
export type ModelVendorPresetModel = {
    // 厂商预设键。
    key: ModelVendorKey;
    // 厂商展示名称。
    label: string;
    // 厂商徽标文字，用于无外链图片时展示稳定的品牌识别。
    mark: string;
    // 适用模型分组。
    group: ModelGroupType;
    // 厂商默认请求地址。
    baseUrl: string;
    // 厂商默认模型 ID。
    model: string;
    // 添加后展示的模型名称。
    modelName: string;
    // API Key 输入框占位提示。
    apiKeyPlaceholder: string;
    // API Key 获取方式说明。
    apiKeyHelp: string;
    // API Key 获取地址，可为空。
    apiKeyUrl: string;
    // API Key 获取地址展示文案，可为空。
    apiKeyUrlLabel: string;
};
