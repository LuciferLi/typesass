/** 模型能力类型，用于区分语音识别模型和文本处理模型。 */
export type ModelCapabilityType = 'asr' | 'text';

/** 模型分组类型，与服务目录能力字段保持一致。 */
export type ModelGroupType = ModelCapabilityType;

/** 模型来源类型，用于区分厂商预设和自定义中转站。 */
export type ModelSourceType = 'vendor' | 'custom';

/** 厂商预设键，用于添加模型时快速填充私有连接参数。 */
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

/**
 * 安全模型目录项。
 * 业务含义：前端只接收选择模型所需的稳定 ID 和展示字段，不接收密钥、上游地址或厂商真实模型名。
 */
export interface ModelCatalogItemModel {
    /** 不透明稳定 ID，业务请求只能发送该字段。 */
    id: string;
    /** 面向用户的模型名称。 */
    displayName: string;
    /** 模型支持的业务能力。 */
    capability: ModelCapabilityType;
    /** 当前是否允许业务调用。 */
    enabled: boolean;
    /** 是否为该能力的服务端默认模型。 */
    isDefault: boolean;
}

/** 本机私有模型配置元数据，允许管理页回显连接信息但绝不包含 API Key。 */
export interface PrivateModelItemModel extends ModelCatalogItemModel {
    /** 供应商标识，用于管理页展示来源。 */
    provider: string;
    /** 上游服务地址；仅由本机私有 IPC 返回。 */
    baseUrl: string;
    /** 上游模型名；仅由本机私有 IPC 返回。 */
    modelName: string;
    /** 本地配置中是否已有密钥。 */
    hasApiKey: boolean;
}

/** 保存私有模型的 IPC 请求，密钥会由原生端写入本地模型 JSON 配置。 */
export interface SavePrivateModelRequestModel {
    /** 编辑时携带现有模型 ID；新增时省略并由原生端生成。 */
    id?: string;
    /** 面向用户的模型名称。 */
    displayName: string;
    /** 模型能力。 */
    capability: ModelCapabilityType;
    /** 保存后是否启用。 */
    enabled: boolean;
    /** 是否设为该能力默认模型。 */
    isDefault: boolean;
    /** 供应商标识。 */
    provider: string;
    /** 上游服务地址。 */
    baseUrl: string;
    /** 上游真实模型名。 */
    modelName: string;
    /** 新增或轮换时提供的密钥；编辑其它字段时可省略。 */
    apiKey?: string;
}

/** 测试私有模型的 IPC 请求，与保存请求相同但不会落盘。 */
export type TestPrivateModelRequestModel = SavePrivateModelRequestModel;

/** 私有模型表单，通过 Tauri IPC 交给原生端写入本地模型配置。 */
export interface ModelFormModel {
    /** 编辑时携带的模型 ID；新增时省略。 */
    id?: string;
    /** 模型展示名称。 */
    name: string;
    /** 模型能力分组。 */
    group: ModelGroupType;
    /** OpenAI 兼容服务地址。 */
    baseUrl: string;
    /** 上游真实模型名。 */
    model: string;
    /** 上游 API Key；保存后写入本地模型 JSON 配置。 */
    apiKey?: string;
    /** 模型来源。 */
    source: ModelSourceType;
    /** 厂商预设键，自定义中转站为空。 */
    vendorKey: ModelVendorKey | '';
    /** 用户备注。 */
    remark: string;
    /** 保存后是否启用。 */
    enabled: boolean;
    /** 是否为该能力的默认模型。 */
    isDefault: boolean;
}

/** 私有模型连通性测试结果，由原生端基于真实上游请求返回。 */
export interface ModelTestResultModel {
    /** 连通性测试是否通过。 */
    success: boolean;
    /** 失败时返回的稳定诊断码；成功时为空。 */
    errorCode?: string;
    /** 原生端返回的可展示说明。 */
    message: string;
}

/** 厂商预设模型，用于添加模型弹窗填充私有连接参数。 */
export interface ModelVendorPresetModel {
    /** 厂商预设键。 */
    key: ModelVendorKey;
    /** 厂商展示名称。 */
    label: string;
    /** 厂商徽标文字。 */
    mark: string;
    /** 适用模型分组。 */
    group: ModelGroupType;
    /** 厂商默认请求地址。 */
    baseUrl: string;
    /** 厂商默认上游模型名。 */
    model: string;
    /** 添加后展示名称。 */
    modelName: string;
    /** API Key 输入提示。 */
    apiKeyPlaceholder: string;
    /** API Key 获取说明。 */
    apiKeyHelp: string;
    /** API Key 获取地址。 */
    apiKeyUrl: string;
    /** API Key 地址展示文案。 */
    apiKeyUrlLabel: string;
}
