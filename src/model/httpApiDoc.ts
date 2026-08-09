/**
 * OpenAPI 根文档模型。
 * 业务含义：承载 App HTTP 桥 `/openapi.json` 返回的接口文档，前端只按该文档渲染，不另行虚构接口。
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
 * 业务含义：说明 Web 应请求哪个 App HTTP 桥地址。
 */
export interface HttpApiServerModel {
    /** 服务地址。 */
    url: string;
    /** 服务地址说明。 */
    description?: string;
}

/**
 * OpenAPI 模块标签模型。
 * 业务含义：按模块组织 HTTP 桥接口。
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
}

/**
 * OpenAPI 媒体类型模型。
 * 业务含义：承载 application/json 等响应或请求体 schema。
 */
export interface HttpApiMediaTypeModel {
    /** JSON schema 定义。 */
    schema?: HttpApiSchemaModel | HttpApiReferenceModel;
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
    /** 数值下限。 */
    minimum?: number;
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
