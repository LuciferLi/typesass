/**
 * OpenAPI 根文档模型。
 * 业务含义：承载独立公共服务 `/openapi.json` 返回的接口文档，前端只按该文档渲染，不另行虚构接口。
 */
export interface HttpApiOpenApiDocumentModel {
    /** OpenAPI 版本号。 */
    openapi: string;
    /** 文档基础信息。 */
    info: HttpApiInfoModel;
    /** 服务地址列表。 */
    servers?: HttpApiServerModel[];
    /** 模块标签列表。 */
    tags?: HttpApiTagModel[];
    /** 接口路径映射。 */
    paths: Record<string, HttpApiPathItemModel>;
    /** 组件定义，主要用于 schema 和公共响应。 */
    components?: HttpApiComponentsModel;
}

/**
 * OpenAPI 信息模型。
 * 业务含义：展示文档标题、版本和说明。
 */
export interface HttpApiInfoModel {
    /** 文档标题。 */
    title: string;
    /** 文档版本。 */
    version: string;
    /** 文档描述。 */
    description?: string;
}

/**
 * OpenAPI 服务地址模型。
 * 业务含义：说明第三方 Web 应请求当前 App 托管的本机 HTTP 服务地址。
 */
export interface HttpApiServerModel {
    /** 服务地址。 */
    url: string;
    /** 服务地址说明。 */
    description?: string;
}

/**
 * OpenAPI 模块标签模型。
 * 业务含义：按模块组织公共 HTTP 接口。
 */
export interface HttpApiTagModel {
    /** 模块名称。 */
    name: string;
    /** 模块说明。 */
    description?: string;
}

/**
 * OpenAPI 路径项模型。
 * 业务含义：同一个 path 下可能包含 GET、POST、OPTIONS 等不同方法。
 */
export interface HttpApiPathItemModel {
    /** GET 方法定义。 */
    get?: HttpApiOperationModel;
    /** POST 方法定义。 */
    post?: HttpApiOperationModel;
    /** OPTIONS 方法定义。 */
    options?: HttpApiOperationModel;
}

/**
 * OpenAPI 操作模型。
 * 业务含义：单个 HTTP 方法的模块、用途、请求体和响应说明。
 */
export interface HttpApiOperationModel {
    /** 所属模块标签。 */
    tags?: string[];
    /** 接口摘要。 */
    summary?: string;
    /** 接口详细说明。 */
    description?: string;
    /** 请求体定义。 */
    requestBody?: HttpApiRequestBodyModel;
    /** 响应定义映射，key 通常为 HTTP 状态码。 */
    responses?: Record<string, HttpApiResponseModel | HttpApiReferenceModel>;
    /** 该接口采用的 OpenAPI security scheme。 */
    security?: Array<Record<string, string[]>>;
    /** Header、query 等请求参数。 */
    parameters?: HttpApiParameterModel[];
    /** 服务端按 HTTP 状态声明的稳定业务错误码和处理建议。 */
    'x-error-codes'?: Record<string, HttpApiErrorCodeModel[]>;
}

/** OpenAPI 扩展错误码说明。 */
export interface HttpApiErrorCodeModel {
    /** 稳定业务错误码。 */
    code: string;
    /** 相同请求在退避后是否允许重试。 */
    retryable: boolean;
    /** 第三方建议处理动作。 */
    action: string;
}

/** OpenAPI 请求参数模型。 */
export interface HttpApiParameterModel {
    /** 参数名称。 */
    name: string;
    /** 参数位置。 */
    in: string;
    /** 参数是否必填。 */
    required?: boolean;
    /** 参数说明。 */
    description?: string;
    /** 参数 schema。 */
    schema?: HttpApiSchemaModel | HttpApiReferenceModel;
}

/**
 * OpenAPI 请求体模型。
 * 业务含义：说明请求体是否必填以及 JSON schema。
 */
export interface HttpApiRequestBodyModel {
    /** 请求体是否必填。 */
    required?: boolean;
    /** 按 MIME 类型组织的内容定义。 */
    content?: Record<string, HttpApiMediaTypeModel>;
}

/**
 * OpenAPI 响应模型。
 * 业务含义：描述响应含义和响应 JSON schema。
 */
export interface HttpApiResponseModel {
    /** 响应说明。 */
    description?: string;
    /** 按 MIME 类型组织的内容定义。 */
    content?: Record<string, HttpApiMediaTypeModel>;
    /** 响应 Header 定义。 */
    headers?: Record<string, HttpApiHeaderModel | HttpApiReferenceModel>;
}

/** OpenAPI 响应 Header 定义。 */
export interface HttpApiHeaderModel {
    /** Header 业务说明。 */
    description?: string;
    /** Header 值 schema。 */
    schema?: HttpApiSchemaModel | HttpApiReferenceModel;
}

/**
 * OpenAPI 媒体类型模型。
 * 业务含义：承载 application/json 等响应或请求体 schema。
 */
export interface HttpApiMediaTypeModel {
    /** JSON schema 定义。 */
    schema?: HttpApiSchemaModel | HttpApiReferenceModel;
    /** 单个媒体类型示例。 */
    example?: unknown;
    /** 命名示例集合。 */
    examples?: Record<string, { summary?: string; value?: unknown }>;
}

/**
 * OpenAPI 引用模型。
 * 业务含义：通过 `$ref` 复用 components 内的 schema 或响应。
 */
export interface HttpApiReferenceModel {
    /** 组件引用路径，例如 #/components/schemas/HealthResponse。 */
    $ref: string;
}

/**
 * OpenAPI Schema 模型。
 * 业务含义：描述请求或响应字段类型、必填规则、枚举、说明和嵌套结构。
 */
export interface HttpApiSchemaModel {
    /** schema 类型。 */
    type?: string;
    /** schema 说明。 */
    description?: string;
    /** 常量值。 */
    const?: unknown;
    /** 最小字符串长度。 */
    minLength?: number;
    /** 最大字符串或数组长度。 */
    maxLength?: number;
    /** 数值下限。 */
    minimum?: number;
    /** 数值上限。 */
    maximum?: number;
    /** 数组最少元素数。 */
    minItems?: number;
    /** 数组最多元素数。 */
    maxItems?: number;
    /** 字符串正则规则。 */
    pattern?: string;
    /** 枚举值列表。 */
    enum?: unknown[];
    /** 必填字段名列表。 */
    required?: string[];
    /** 对象字段定义。 */
    properties?: Record<string, HttpApiSchemaModel | HttpApiReferenceModel>;
    /** 数组元素定义。 */
    items?: HttpApiSchemaModel | HttpApiReferenceModel;
    /** 额外字段规则。 */
    additionalProperties?: boolean | HttpApiSchemaModel | HttpApiReferenceModel;
    /** 多类型其一。 */
    oneOf?: Array<HttpApiSchemaModel | HttpApiReferenceModel>;
    /** 任一匹配类型，FastAPI 可空字段通常使用该结构。 */
    anyOf?: Array<HttpApiSchemaModel | HttpApiReferenceModel>;
    /** 字段默认值。 */
    default?: unknown;
}

/**
 * OpenAPI 组件模型。
 * 业务含义：保存公共 schema 和公共响应定义。
 */
export interface HttpApiComponentsModel {
    /** 公共 schema 映射。 */
    schemas?: Record<string, HttpApiSchemaModel>;
    /** 公共响应映射。 */
    responses?: Record<string, HttpApiResponseModel>;
    /** Bearer 等鉴权方案。 */
    securitySchemes?: Record<string, HttpApiSecuritySchemeModel>;
}

/** OpenAPI 鉴权方案模型。 */
export interface HttpApiSecuritySchemeModel {
    /** security scheme 类型。 */
    type: string;
    /** HTTP 鉴权 scheme。 */
    scheme?: string;
    /** Bearer token 格式说明。 */
    bearerFormat?: string;
    /** 鉴权说明。 */
    description?: string;
}

/**
 * 页面渲染用接口行模型。
 * 业务含义：把 paths 映射拍平成可按模块列表展示的接口。
 */
export interface HttpApiEndpointModel {
    /** 接口路径。 */
    path: string;
    /** HTTP 方法。 */
    method: string;
    /** 接口所属模块。 */
    tag: string;
    /** 接口摘要。 */
    summary: string;
    /** 接口说明。 */
    description: string;
    /** 操作原始定义。 */
    operation: HttpApiOperationModel;
}
