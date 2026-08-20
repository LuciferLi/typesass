import type { ModelVendorOptionModel } from '@/model/modelManage';

// 厂商模型预设列表，用于把模型管理表单收敛为“厂商 -> 模型 -> 凭证”的产品化选择。
export const ModelVendorPresets: ModelVendorOptionModel[] = [
    {
        key: 'xiaomi',
        label: '小米 MiMo',
        mark: 'MI',
        group: 'text',
        apiKeyPlaceholder: '请输入小米 MiMo API Key',
        apiKeyHelp: '请先在小米 MiMo 平台开通文本模型套餐，再复制可用 API Key。',
        apiKeyUrl: 'https://token-plan-cn.xiaomimimo.com',
        apiKeyUrlLabel: '前往小米 MiMo',
        models: [
            {
                key: 'mimo-v25',
                label: 'MiMo V2.5',
                baseUrl: 'https://token-plan-cn.xiaomimimo.com/v1',
                model: 'mimo-v2.5',
                modelName: '小米 MiMo V2.5',
                description: '当前已接入的中文文本模型，适合文本润色和口述整理。',
                recommended: true
            }
        ]
    },
    {
        key: 'qwen',
        label: '阿里通义',
        mark: 'QW',
        group: 'text',
        apiKeyPlaceholder: '请输入阿里云百炼 API Key',
        apiKeyHelp: '请前往阿里云百炼控制台开通模型服务，并在 API Key 管理页复制密钥。',
        apiKeyUrl: 'https://bailian.console.aliyun.com/?tab=model#/api-key',
        apiKeyUrlLabel: '打开百炼 API Key',
        models: [
            {
                key: 'qwen-plus',
                label: 'Qwen Plus',
                baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
                model: 'qwen-plus',
                modelName: '通义千问 Plus',
                description: '中文质量和成本平衡，适合默认文本润色。',
                recommended: true
            },
            {
                key: 'qwen-turbo',
                label: 'Qwen Turbo',
                baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
                model: 'qwen-turbo',
                modelName: '通义千问 Turbo',
                description: '速度和成本优先，适合高频短文本处理。'
            }
        ]
    },
    {
        key: 'aliyun-realtime-asr',
        label: '阿里实时 ASR',
        mark: 'RT',
        group: 'asr',
        apiKeyPlaceholder: '请输入阿里云百炼 API Key',
        apiKeyHelp: '阿里实时 ASR 使用百炼 API Key，录音时由 CodexMan App 通过 WebSocket 边录边识别。',
        apiKeyUrl: 'https://bailian.console.aliyun.com/?tab=model#/api-key',
        apiKeyUrlLabel: '打开百炼 API Key',
        models: [
            {
                key: 'fun-asr-realtime',
                label: 'Fun-ASR Realtime',
                baseUrl: 'wss://dashscope.aliyuncs.com/api-ws/v1/inference',
                model: 'fun-asr-realtime',
                provider: 'aliyun-realtime-asr',
                modelName: '阿里 Fun-ASR Realtime',
                description: '主推实时 ASR，录音时通过阿里 WebSocket 边录边识别。',
                recommended: true
            },
            {
                key: 'paraformer-realtime-v2',
                label: 'Paraformer Realtime V2',
                baseUrl: 'wss://dashscope.aliyuncs.com/api-ws/v1/inference',
                model: 'paraformer-realtime-v2',
                provider: 'aliyun-realtime-asr',
                modelName: '阿里 Paraformer Realtime V2',
                description: '低成本实时 ASR 备选，录音时通过阿里 WebSocket 边录边识别。'
            }
        ]
    },
    {
        key: 'tencent-realtime-asr',
        label: '腾讯云实时 ASR',
        mark: 'TC',
        group: 'asr',
        apiKeyPlaceholder: '请输入 JSON：{"appId":"...","secretId":"...","secretKey":"..."}',
        apiKeyHelp: '腾讯云实时 ASR 需要 AppID、SecretId、SecretKey，录音时由 CodexMan App 通过 WebSocket 边录边识别。',
        apiKeyUrl: 'https://console.cloud.tencent.com/cam/capi',
        apiKeyUrlLabel: '打开腾讯云 API 密钥',
        models: [
            {
                key: 'tencent-16k-zh',
                label: '16k 普通话',
                baseUrl: 'wss://asr.cloud.tencent.com/asr/v2',
                model: '16k_zh',
                provider: 'tencent-realtime-asr',
                modelName: '腾讯云实时 ASR 16k 普通话',
                description: '腾讯云实时识别默认中文模型，录音时通过腾讯云 WebSocket 边录边识别。',
                recommended: true
            },
            {
                key: 'tencent-16k-zh-video',
                label: '16k 视频场景',
                baseUrl: 'wss://asr.cloud.tencent.com/asr/v2',
                model: '16k_zh_video',
                provider: 'tencent-realtime-asr',
                modelName: '腾讯云实时 ASR 视频场景',
                description: '更偏远场或视频音频场景，录音时通过腾讯云 WebSocket 边录边识别。'
            }
        ]
    },
    {
        key: 'iflytek-realtime-asr',
        label: '讯飞实时转写',
        mark: 'XF',
        group: 'asr',
        apiKeyPlaceholder: '请输入 JSON：{"appId":"...","apiKey":"...","apiSecret":"..."}',
        apiKeyHelp: '讯飞实时转写需要 APPID、APIKey、APISecret，录音时由 CodexMan App 通过 WebSocket 边录边识别。',
        apiKeyUrl: 'https://console.xfyun.cn/services/rtasr',
        apiKeyUrlLabel: '打开讯飞实时转写',
        models: [
            {
                key: 'iflytek-rtasr-standard',
                label: '实时转写标准版',
                baseUrl: 'wss://rtasr.xfyun.cn/v1/ws',
                model: 'rtasr-standard',
                provider: 'iflytek-realtime-asr',
                modelName: '讯飞实时转写标准版',
                description: '中文实时转写备选，录音时通过讯飞 WebSocket 边录边识别。',
                recommended: true
            }
        ]
    },
    {
        key: 'deepseek',
        label: 'DeepSeek',
        mark: 'DS',
        group: 'text',
        apiKeyPlaceholder: '请输入 DeepSeek API Key',
        apiKeyHelp: '请前往 DeepSeek 开放平台的 API keys 页面创建并复制 API Key。',
        apiKeyUrl: 'https://platform.deepseek.com/api_keys',
        apiKeyUrlLabel: '打开 DeepSeek API Keys',
        models: [
            {
                key: 'deepseek-chat',
                label: 'DeepSeek Chat',
                baseUrl: 'https://api.deepseek.com/v1',
                model: 'deepseek-chat',
                modelName: 'DeepSeek Chat',
                description: '通用文本润色和整理模型。',
                recommended: true
            },
            {
                key: 'deepseek-reasoner',
                label: 'DeepSeek Reasoner',
                baseUrl: 'https://api.deepseek.com/v1',
                model: 'deepseek-reasoner',
                modelName: 'DeepSeek Reasoner',
                description: '更偏复杂推理，不建议作为高频语音润色默认项。'
            }
        ]
    },
    {
        key: 'openai',
        label: 'OpenAI',
        mark: 'AI',
        group: 'text',
        apiKeyPlaceholder: '请输入 OpenAI API Key',
        apiKeyHelp: '请前往 OpenAI Platform 的 API keys 页面创建并复制 API Key。',
        apiKeyUrl: 'https://platform.openai.com/api-keys',
        apiKeyUrlLabel: '打开 OpenAI API Keys',
        models: [
            {
                key: 'gpt-4o-mini',
                label: 'GPT-4o mini',
                baseUrl: 'https://api.openai.com/v1',
                model: 'gpt-4o-mini',
                modelName: 'GPT-4o mini',
                description: '海外低成本文本处理模型。',
                recommended: true
            }
        ]
    },
    {
        key: 'gemini',
        label: 'Google Gemini',
        mark: 'G',
        group: 'text',
        apiKeyPlaceholder: '请输入 Google AI Studio API Key',
        apiKeyHelp: '请前往 Google AI Studio 创建 API Key，再用于 Gemini OpenAI 兼容接口。',
        apiKeyUrl: 'https://aistudio.google.com/app/apikey',
        apiKeyUrlLabel: '打开 Google AI Studio',
        models: [
            {
                key: 'gemini-25-flash',
                label: 'Gemini 2.5 Flash',
                baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
                model: 'gemini-2.5-flash',
                modelName: 'Gemini 2.5 Flash',
                description: '海外多模态模型，文本润色可用。',
                recommended: true
            }
        ]
    },
    {
        key: 'kimi',
        label: 'Moonshot Kimi',
        mark: 'KM',
        group: 'text',
        apiKeyPlaceholder: '请输入 Moonshot API Key',
        apiKeyHelp: '请前往 Moonshot 控制台创建并复制 API Key。',
        apiKeyUrl: 'https://platform.moonshot.cn/console/api-keys',
        apiKeyUrlLabel: '打开 Moonshot API Keys',
        models: [
            {
                key: 'kimi-k3',
                label: 'Kimi K3',
                baseUrl: 'https://api.moonshot.ai/v1',
                model: 'kimi-k3',
                modelName: 'Kimi K3',
                description: '长文本能力较强，适合较长文本润色。',
                recommended: true
            }
        ]
    },
    {
        key: 'zhipu',
        label: '智谱 GLM',
        mark: 'GL',
        group: 'text',
        apiKeyPlaceholder: '请输入智谱 API Key',
        apiKeyHelp: '请前往智谱开放平台的 API keys 页面创建并复制密钥。',
        apiKeyUrl: 'https://open.bigmodel.cn/usercenter/apikeys',
        apiKeyUrlLabel: '打开智谱 API Keys',
        models: [
            {
                key: 'glm-52',
                label: 'GLM 5.2',
                baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
                model: 'glm-5.2',
                modelName: '智谱 GLM 5.2',
                description: '国内文本模型备选。',
                recommended: true
            }
        ]
    },
    {
        key: 'volcengine',
        label: '火山方舟',
        mark: 'ARK',
        group: 'text',
        apiKeyPlaceholder: '请输入火山方舟 API Key',
        apiKeyHelp: '请前往火山方舟控制台开通模型并创建 API Key，模型使用推理接入点 ID。',
        apiKeyUrl: 'https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey',
        apiKeyUrlLabel: '打开方舟 API Key',
        models: [
            {
                key: 'volcengine-endpoint',
                label: '推理接入点',
                baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
                model: '请替换为方舟推理接入点 ID',
                modelName: '火山方舟',
                description: '需要在自定义模式中填写真实推理接入点 ID。',
                comingSoon: true
            }
        ]
    }
];
