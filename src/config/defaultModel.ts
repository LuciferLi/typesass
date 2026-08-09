import type { ModelItemModel, ModelVendorPresetModel } from '@/model/modelManage';

// 默认模型列表，用户未主动添加模型前保持为空，避免把厂商预设误展示成用户模型。
export const DefaultModels: ModelItemModel[] = [];

// 厂商预设列表，用于添加模型时快速生成请求地址和模型 ID。
export const ModelVendorPresets: ModelVendorPresetModel[] = [
    {
        key: 'xiaomi-asr',
        label: '小米 ASR',
        mark: 'MI',
        group: 'asr',
        baseUrl: 'https://token-plan-cn.xiaomimimo.com/v1',
        model: 'mimo-v2.5-asr',
        modelName: '小米 ASR',
        apiKeyPlaceholder: '请输入小米 MIMO API Key',
        apiKeyHelp: '请先在小米 MIMO 平台开通 ASR 对应套餐，再复制可用 API Key。',
        apiKeyUrl: 'https://token-plan-cn.xiaomimimo.com',
        apiKeyUrlLabel: '前往小米 MIMO'
    },
    {
        key: 'xiaomi-text',
        label: '小米文本模型',
        mark: 'MI',
        group: 'text',
        baseUrl: 'https://token-plan-cn.xiaomimimo.com/v1',
        model: 'mimo-v2.5',
        modelName: '小米文本模型',
        apiKeyPlaceholder: '请输入小米 MIMO API Key',
        apiKeyHelp: '请先在小米 MIMO 平台开通文本模型对应套餐，再复制可用 API Key。',
        apiKeyUrl: 'https://token-plan-cn.xiaomimimo.com',
        apiKeyUrlLabel: '前往小米 MIMO'
    },
    {
        key: 'openai',
        label: 'OpenAI / ChatGPT',
        mark: 'AI',
        group: 'text',
        baseUrl: 'https://api.openai.com/v1',
        model: 'gpt-4o-mini',
        modelName: 'ChatGPT',
        apiKeyPlaceholder: '请输入 OpenAI API Key',
        apiKeyHelp: '请前往 OpenAI Platform 的 API keys 页面创建并复制 API Key。',
        apiKeyUrl: 'https://platform.openai.com/api-keys',
        apiKeyUrlLabel: '打开 OpenAI API Keys'
    },
    {
        key: 'deepseek',
        label: 'DeepSeek',
        mark: 'DS',
        group: 'text',
        baseUrl: 'https://api.deepseek.com/v1',
        model: 'deepseek-v4-flash',
        modelName: 'DeepSeek',
        apiKeyPlaceholder: '请输入 DeepSeek API Key',
        apiKeyHelp: '请前往 DeepSeek 开放平台的 API keys 页面创建并复制 API Key。',
        apiKeyUrl: 'https://platform.deepseek.com/api_keys',
        apiKeyUrlLabel: '打开 DeepSeek API Keys'
    },
    {
        key: 'qwen',
        label: '通义千问',
        mark: 'QW',
        group: 'text',
        baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        model: 'qwen-plus',
        modelName: '通义千问',
        apiKeyPlaceholder: '请输入阿里云百炼 API Key',
        apiKeyHelp: '请前往阿里云百炼控制台开通模型服务，并在 API Key 管理页复制密钥。',
        apiKeyUrl: 'https://bailian.console.aliyun.com/?tab=model#/api-key',
        apiKeyUrlLabel: '打开百炼 API Key'
    },
    {
        key: 'qwen-asr',
        label: '通义 ASR',
        mark: 'QA',
        group: 'asr',
        baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        model: 'qwen3-asr-flash',
        modelName: '通义 ASR',
        apiKeyPlaceholder: '请输入阿里云百炼 API Key',
        apiKeyHelp: '请前往阿里云百炼控制台开通 ASR 服务，并在 API Key 管理页复制密钥。',
        apiKeyUrl: 'https://bailian.console.aliyun.com/?tab=model#/api-key',
        apiKeyUrlLabel: '打开百炼 API Key'
    },
    {
        key: 'gemini',
        label: 'Google Gemini',
        mark: 'G',
        group: 'text',
        baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
        model: 'gemini-2.5-flash',
        modelName: 'Gemini',
        apiKeyPlaceholder: '请输入 Google AI Studio API Key',
        apiKeyHelp: '请前往 Google AI Studio 创建 API Key，再用于 Gemini OpenAI 兼容接口。',
        apiKeyUrl: 'https://aistudio.google.com/app/apikey',
        apiKeyUrlLabel: '打开 Google AI Studio'
    },
    {
        key: 'kimi',
        label: 'Moonshot Kimi',
        mark: 'KM',
        group: 'text',
        baseUrl: 'https://api.moonshot.ai/v1',
        model: 'kimi-k3',
        modelName: 'Kimi',
        apiKeyPlaceholder: '请输入 Moonshot API Key',
        apiKeyHelp: '请前往 Moonshot 控制台创建并复制 API Key。',
        apiKeyUrl: 'https://platform.moonshot.cn/console/api-keys',
        apiKeyUrlLabel: '打开 Moonshot API Keys'
    },
    {
        key: 'zhipu',
        label: '智谱 GLM',
        mark: 'GL',
        group: 'text',
        baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
        model: 'glm-5.2',
        modelName: '智谱 GLM',
        apiKeyPlaceholder: '请输入智谱 API Key',
        apiKeyHelp: '请前往智谱开放平台的 API keys 页面创建并复制密钥。',
        apiKeyUrl: 'https://open.bigmodel.cn/usercenter/apikeys',
        apiKeyUrlLabel: '打开智谱 API Keys'
    },
    {
        key: 'volcengine',
        label: '火山方舟',
        mark: 'ARK',
        group: 'text',
        baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
        model: '请替换为方舟推理接入点 ID',
        modelName: '火山方舟',
        apiKeyPlaceholder: '请输入火山方舟 API Key',
        apiKeyHelp: '请前往火山方舟控制台开通模型并创建 API Key，模型名称使用推理接入点 ID。',
        apiKeyUrl: 'https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey',
        apiKeyUrlLabel: '打开方舟 API Key'
    }
];
