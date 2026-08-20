import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { CodexConnectionRestartAcceptedModel, CodexConnectionStatusModel } from '@/model/codexConnection';
import type { HttpApiOpenApiDocumentModel } from '@/model/httpApiDoc';
import type { LocalConfigChangedPayloadModel, LocalConfigJsonValueModel } from '@/model/localConfig';
import type {
    ModelCatalogItemModel,
    ModelTestResultModel,
    PrivateModelItemModel,
    SavePrivateModelRequestModel,
    TestPrivateModelRequestModel
} from '@/model/modelManage';
import type { RuntimeDiagnosticsModel, ShortcutProfileModel } from '@/model/permission';
import type {
    CodexThreadListRequestModel,
    CodexThreadSummaryModel,
    CodexWorkspaceModel,
    CreateSessionProjectRequestModel,
    CreateSessionTaskRequestModel,
    CreateSessionTaskResponseModel,
    SessionWorkspaceDataModel,
    UpdateSessionProjectRequestModel,
    UpdateSessionTaskRequestModel
} from '@/model/sessionManage';
import type { ApplicationOptionModel } from '@/model/shortcutBinding';
import type { PasteResponseModel, SelectedTextResponseModel } from '@/model/textPolish';
import type {
    AppVoicePolishResponseModel,
    ProcessTextRequestModel,
    ProcessTextResponseModel,
    ResultWindowPayloadModel,
    TranscribeRequestModel,
    TranscribeResponseModel,
    VoicePolishRunModeType
} from '@/model/voicePolish';

/** 普通 Web 无法执行桌面系统操作时展示的统一提示。 */
export const CLIENT_UNAVAILABLE_VOICE_MESSAGE = '当前功能需要在 CodexMan 桌面端中使用。';

/** 公共 HTTP 服务不可达时展示的统一提示。 */
export const PUBLIC_API_UNAVAILABLE_MESSAGE = 'CodexMan HTTP 服务暂时不可用，请稍后重试。';

/** 公共 HTTP 服务的错误响应。 */
interface PublicApiErrorModel {
    /** 统一错误详情。 */
    error?: {
        /** 本次请求 ID，可提供给维护人员检索日志。 */
        requestId?: string;
        /** 稳定业务错误码。 */
        code?: string;
        /** 面向调用方的错误说明。 */
        message?: string;
        /** 服务端明确声明当前错误是否允许调用方自动重试。 */
        retryable?: boolean;
    };
}

/** 公开 HTTP API 的类型化业务错误，供调用方按稳定错误码执行精确交互。 */
export class PublicApiRequestError extends Error {
    /** 服务端返回的稳定业务错误码。 */
    readonly code: string;

    /** 服务端或客户端生成的请求追踪 ID。 */
    readonly requestId: string;

    /** 原始 HTTP 状态码。 */
    readonly status: number;

    /** 服务端是否明确允许自动重试。 */
    readonly retryable: boolean;

    /**
     * 创建公开 HTTP API 类型化错误。
     * 流程：保留用户可读完整文案，同时单独保存错误码、请求 ID、HTTP 状态和重试语义。
     * 参数：message 为完整安全文案，其余字段来自统一错误 envelope 与 HTTP 响应。
     * 返回：可被 instanceof 和稳定错误码守卫识别的 Error。
     * 边界：不保存响应正文、请求参数、Token 或其它内部诊断数据。
     */
    constructor(message: string, code: string, requestId: string, status: number, retryable: boolean) {
        super(message);
        this.name = 'PublicApiRequestError';
        this.code = code;
        this.requestId = requestId;
        this.status = status;
        this.retryable = retryable;
    }
}

/**
 * 判断未知异常是否为指定公开 API 稳定错误码。
 * 流程：先通过类型化 Error 校验，再比较服务端稳定业务码。
 * 参数：error 为 catch 捕获值，code 为调用方需要识别的业务错误码。
 * 返回：错误类型和业务码同时匹配时返回 true。
 * 边界：普通网络错误、旧版纯 Error 和伪造对象均返回 false。
 */
export function isPublicApiRequestErrorCode(error: unknown, code: string): error is PublicApiRequestError {
    return error instanceof PublicApiRequestError && error.code === code;
}

/** 公共 API 请求支持的 HTTP 方法，避免通过是否存在请求体隐式推断方法。 */
type PublicApiRequestMethod = 'GET' | 'POST';

/** 公共 API 单次业务调用配置。 */
interface PublicApiRequestOptions {
    /** 服务端路由要求的 HTTP 方法。 */
    method: PublicApiRequestMethod;
    /** 可选 JSON 请求体；未提供时不发送 Body，也不声明 Content-Type。 */
    payload?: unknown;
    /** 是否为允许按转换接口契约重试的 AI 请求。 */
    retryTransientErrors?: boolean;
    /** 非转换请求的单次超时毫秒数；转换请求始终受共享 60 秒预算约束。 */
    timeoutMs?: number;
}

/** 公共 API 一次 HTTP 尝试的完整结果。 */
interface PublicApiAttemptResult<ResponseModel> {
    /** 原始 Fetch 响应，供状态码与响应头判断使用。 */
    response: Response;
    /** 解析后的成功模型、错误 envelope 或空响应。 */
    responseJson: ResponseModel | PublicApiErrorModel | null;
    /** 当前尝试独立生成的请求 ID。 */
    requestId: string;
    /** 当前尝试实际携带的 App 授权码。 */
    publicApiToken: string;
}

/** 公共 API 授权码申请响应。 */
interface PublicApiAccessTokenRequestModel {
    /** 授权结果状态。 */
    status: 'approved' | 'rejected';
    /** 用户确认后返回的明文 App 授权码。 */
    accessToken: string | null;
    /** 授权码过期时间；永久有效时为空。 */
    expiresAt: string | null;
}

/** App 授权码状态，供系统设置页展示授权码生命周期。 */
export type PublicApiAccessTokenStatus = 'active' | 'expired' | 'revoked';

/** App 授权码记录，包含系统设置页可长期查看和复制的明文授权码。 */
export interface PublicApiAccessTokenModel {
    /** 授权码稳定 ID，用于撤销操作。 */
    id: string;
    /** 授权码名称，用于区分调用方。 */
    name: string;
    /** 明文授权码，系统设置页可复制和查看。 */
    token: string;
    /** 授权码到期时间；永久有效时为空。 */
    expiresAt: string | null;
    /** 授权码当前状态。 */
    status: PublicApiAccessTokenStatus;
    /** 授权码创建时间。 */
    createdAt: string;
    /** 授权码撤销时间；未撤销时为空。 */
    revokedAt: string | null;
    /** 授权码最近一次通过业务接口鉴权的时间。 */
    lastUsedAt: string | null;
}

/** 浏览器插件 ZIP 下载响应。 */
export interface BrowserExtensionDownloadModel {
    /** ZIP 文件最终保存到本机的绝对路径。 */
    filePath: string;
}

/** App 授权确认弹窗事件。 */
export interface PublicApiAccessTokenApprovalEventModel {
    /** 本次 HTTP 请求追踪 ID，确认时需要原样带回。 */
    requestId: string;
    /** 申请方展示名称。 */
    name: string;
    /** 授权码过期时间；永久有效时为空。 */
    expiresAt: string | null;
}

/** 浏览器设备码授权启动响应。 */
export interface PublicApiDeviceAuthorizationModel {
    /** 仅当前浏览器轮询使用的高熵设备码。 */
    deviceCode: string;
    /** 交给管理员批准的短用户码。 */
    userCode: string;
    /** 固定批准方式；第三方 Web 不能调用桌面私有批准能力。 */
    approvalMethod: 'codexman-app';
    /** 可直接展示给最终用户的本机 App 批准步骤。 */
    approvalInstruction: string;
    /** 设备码有效期秒数。 */
    expiresIn: number;
    /** 最小轮询间隔秒数。 */
    interval: number;
}

/** CodexMan App 自动托管的本机 HTTP 服务固定地址。 */
const PUBLIC_API_BASE_URL = 'http://127.0.0.1:18080';
const PUBLIC_API_TIMEOUT_MS = 65_000;
const PUBLIC_API_HEALTH_TIMEOUT_MS = 1_200;
const PUBLIC_API_RETRY_BUDGET_MS = 60_000;
const PUBLIC_API_MAX_RETRY_COUNT = 2;
const PUBLIC_API_RETRY_JITTER_MS = 250;
const PUBLIC_API_RETRY_DELAYS_MS = [1_000, 2_000] as const;
const PUBLIC_API_RETRYABLE_ERROR_CODES = new Set([
    'RATE_LIMIT',
    'CONCURRENCY_LIMIT',
    'UPSTREAM_UNAVAILABLE',
    'QUOTA_STORE_UNAVAILABLE',
    'UPSTREAM_TIMEOUT'
]);
/** 普通浏览器当前页面持有的 App 授权码；外部授权页后续可改为持久保存。 */
let browserPublicApiToken = '';

/**
 * 保存当前浏览器会话使用的公共 API 授权码。
 * 流程：普通浏览器只写当前页面模块内存；Tauri 只写 Rust 进程内存，桌面 WebView 通常不需要保存。
 * 参数：token 为 App 签发给当前调用方的明文授权码。
 * 返回：无。
 * 边界：空值会清除会话凭据；上游模型密钥绝不能传入本方法。
 */
export async function setPublicApiToken(token: string): Promise<void> {
    const normalizedToken = token.trim();
    if (isTauriRuntime()) {
        await invokeDesktop<void>('set_public_api_token', { token: normalizedToken });
        return;
    }
    browserPublicApiToken = normalizedToken;
}

/**
 * 下载浏览器插件 ZIP 到本机下载目录。
 * 流程：桌面端使用受保护的 Tauri 命令写入固定插件包；普通 Web 返回明确不可用错误。
 * 参数：无。
 * 返回：最终保存路径。
 * 异常：普通 Web 或桌面命令失败时抛出用户可读错误。
 */
export async function downloadBrowserExtensionZip(): Promise<BrowserExtensionDownloadModel> {
    return invokeDesktop<BrowserExtensionDownloadModel>('download_browser_extension_zip');
}

/**
 * 响应 App 授权码申请。
 * 流程：Hub 主窗口弹出确认框后把 requestId 与用户选择交回 Rust，Rust 再唤醒等待中的 HTTP 请求。
 * 参数：requestId 为授权申请事件 ID，approved 表示是否同意创建授权码。
 * 返回：无返回值。
 * 异常：申请已过期、窗口无权限或桌面状态异常时抛出错误。
 */
export async function respondPublicApiAccessTokenRequest(requestId: string, approved: boolean): Promise<void> {
    await invokeDesktop<void>('respond_public_api_access_token_request', { requestId, approved });
}

/**
 * 读取当前运行会话的公共 API 授权码。
 * 流程：普通浏览器读取当前页面模块内存；Tauri 只读取 Rust 进程内存，不把敏感值复制进 Web Storage。
 * 参数：无。
 * 返回：当前授权码；尚未授权时返回空字符串。
 * 异常：桌面进程内状态读取失败时透传 IPC 错误，不回退伪造凭据。
 */
export async function getPublicApiToken(): Promise<string> {
    if (isTauriRuntime()) return (await invokeDesktop<string>('get_public_api_token')).trim();
    return browserPublicApiToken;
}

/**
 * 仅在凭据仍是请求使用值时清除公共 API Token。
 * 流程：普通浏览器比较当前页面内存值后清除；Tauri 只交给 Rust 在同一把锁内比较并清除。
 * 参数：expectedToken 为已收到 401 的请求实际携带的 Token。
 * 返回：成功清除返回 true；Token 已被其它窗口续签或本来为空时返回 false。
 * 异常：桌面原子清除 IPC 失败时透传错误，不覆盖可能已经续签的新凭据。
 */
async function clearPublicApiTokenIfCurrent(expectedToken: string): Promise<boolean> {
    if (!isTauriRuntime()) {
        if (browserPublicApiToken !== expectedToken) return false;
        browserPublicApiToken = '';
        return true;
    }
    return invokeDesktop<boolean>('clear_public_api_token_if_matches', { expectedToken });
}

/**
 * 判断当前运行会话是否具备公共 API 凭据。
 * 流程：复用跨 WebView 授权码读取逻辑，只返回是否存在，不向页面暴露授权码内容。
 * 参数：无。
 * 返回：存在非空授权码时返回 true。
 * 异常：桌面共享状态读取失败时透传，调用方应展示未就绪而不是假成功。
 */
export async function hasPublicApiToken(): Promise<boolean> {
    return Boolean(await getPublicApiToken());
}

/**
 * 判断当前页面是否运行在 Tauri WebView 中。
 * 流程：检查 Tauri 注入的内部标识，供桌面 IPC 与普通 Web 分流。
 * 参数：无。
 * 返回：处于 Tauri 运行时返回 true。
 * 边界：普通浏览器或注入尚未完成时返回 false。
 */
export function isTauriRuntime(): boolean {
    return Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

/**
 * 调用桌面端 Tauri command。
 * 流程：先验证运行环境，再通过受 Tauri capability 保护的 IPC 调用原生命令。
 * 参数：command 为 Rust command 名；args 为序列化参数。
 * 返回：原生命令的类型化响应。
 * 异常：普通 Web 调用或原生命令失败时抛出明确错误。
 */
async function invokeDesktop<ResponseModel>(command: string, args?: Record<string, unknown>): Promise<ResponseModel> {
    if (!isTauriRuntime()) throw new Error(CLIENT_UNAVAILABLE_VOICE_MESSAGE);
    return invoke<ResponseModel>(command, args);
}

/**
 * 调用模型管理专用桌面命令并归一化 IPC 错误。
 * 流程：复用受保护的 Tauri invoke；保留标准 Error，把 Rust 的非空字符串 rejection 转成 Error 供模型页面展示。
 * 参数：command 为模型管理命令名；args 为不包含持久化副本的临时 IPC 参数。
 * 返回：原生命令的类型化响应。
 * 异常：未知 rejection 使用固定兜底文案；禁止把对象直接字符串化，避免展示内部对象或无意义内容。
 */
async function invokeModelDesktop<ResponseModel>(
    command: string,
    args?: Record<string, unknown>
): Promise<ResponseModel> {
    try {
        return await invokeDesktop<ResponseModel>(command, args);
    } catch (error) {
        if (error instanceof Error) throw error;
        if (typeof error === 'string' && error.trim()) throw new Error(error.trim());
        throw new Error('模型管理操作失败，请查看本机日志。');
    }
}

/**
 * 解析服务端 Retry-After 响应头。
 * 流程：优先按秒数解析，否则按 HTTP 日期计算从当前时刻起的等待时间。
 * 参数：retryAfter 为响应头原值，currentTimeMs 为当前 Unix 毫秒时间。
 * 返回：合法且未过期时返回非负等待毫秒数；缺失、非法或已过期时返回 null。
 * 边界：本方法不截断超长等待，调用方必须再用业务请求总时限判断是否可继续。
 */
function parseRetryAfterMs(retryAfter: string | null, currentTimeMs: number): number | null {
    if (!retryAfter) return null;
    const seconds = Number(retryAfter);
    if (Number.isFinite(seconds) && seconds >= 0) return seconds * 1_000;
    const retryTimeMs = Date.parse(retryAfter);
    if (!Number.isFinite(retryTimeMs) || retryTimeMs < currentTimeMs) return null;
    return retryTimeMs - currentTimeMs;
}

/**
 * 等待下一次公共 API 尝试。
 * 流程：创建一次性浏览器定时器，到期后完成 Promise，不保留额外状态。
 * 参数：delayMs 为等待毫秒数。
 * 返回：等待结束后完成的 Promise。
 * 边界：调用方已保证等待不超过共享总时限；页面销毁不会主动取消该短定时器。
 */
function waitForPublicApiRetry(delayMs: number): Promise<void> {
    return new Promise((resolve) => {
        window.setTimeout(resolve, delayMs);
    });
}

/**
 * 创建公共 API 请求追踪 ID。
 * 流程：优先使用安全上下文提供的 randomUUID；不可用时改用随机字节；极端旧环境用时间戳随机片段兜底。
 * 参数：无。
 * 返回：可放入 X-Request-Id 的前端追踪字符串。
 * 边界：HTTP 内网访问可能没有 randomUUID，本方法必须保证文档和公共 API 页面不因此中断。
 */
function createPublicApiRequestId(): string {
    if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
        return crypto.randomUUID();
    }

    if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
        const randomValues = new Uint8Array(16);
        crypto.getRandomValues(randomValues);
        return Array.from(randomValues)
            .map((value) => value.toString(16).padStart(2, '0'))
            .join('');
    }

    return `request-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

/**
 * 执行一次公共 API HTTP 尝试。
 * 流程：读取最新 Token、生成独立请求 ID、按显式方法和可选 JSON Body 请求，并解析统一响应。
 * 参数：path 为接口路径，options 为方法与请求体配置，timeoutMs 为当前尝试可使用的最长时间。
 * 返回：原始响应、解析结果、请求 ID 与本次实际 Token。
 * 异常：超时或网络不可达时转换为携带当前 requestId 的稳定错误，其它异常原样透传。
 */
async function performPublicApiAttempt<ResponseModel>(
    path: string,
    options: PublicApiRequestOptions,
    timeoutMs: number
): Promise<PublicApiAttemptResult<ResponseModel>> {
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), timeoutMs);
    const requestId = createPublicApiRequestId();
    try {
        const publicApiToken = await getPublicApiToken();
        const headers: Record<string, string> = { Accept: 'application/json', 'X-Request-Id': requestId };
        if (options.payload !== undefined) headers['Content-Type'] = 'application/json';
        if (publicApiToken) headers.Authorization = `Bearer ${publicApiToken}`;
        const response = await fetch(`${PUBLIC_API_BASE_URL}${path}`, {
            method: options.method,
            headers,
            body: options.payload === undefined ? undefined : JSON.stringify(options.payload),
            cache: 'no-store',
            signal: controller.signal
        });
        const responseText = await response.text();
        let responseJson: ResponseModel | PublicApiErrorModel | null = null;
        try {
            responseJson = responseText ? (JSON.parse(responseText) as ResponseModel | PublicApiErrorModel) : null;
        } catch {
            responseJson = null;
        }
        return { response, responseJson, requestId, publicApiToken };
    } catch (error) {
        if (error instanceof Error && error.name === 'AbortError') {
            throw new Error(`CodexMan HTTP 服务请求超时（请求 ID：${requestId}）`);
        }
        if (error instanceof TypeError) throw new Error(`${PUBLIC_API_UNAVAILABLE_MESSAGE}（请求 ID：${requestId}）`);
        throw error;
    } finally {
        window.clearTimeout(timer);
    }
}

/**
 * 请求独立的 CodexMan 公共 HTTP 服务。
 * 流程：每次尝试使用新请求 ID；AI 瞬时错误最多额外重试两次并共享 60 秒。
 * 参数：path 为公共 API 路径；options 显式声明 HTTP 方法、可选 JSON Body 和是否启用转换重试。
 * 返回：类型化业务响应。
 * 异常：响应非法、共享时限耗尽、鉴权失败或非可重试业务错误时抛出含 requestId 的错误。
 */
async function requestPublicApi<ResponseModel>(path: string, options: PublicApiRequestOptions): Promise<ResponseModel> {
    const requestTimeoutMs = options.retryTransientErrors
        ? PUBLIC_API_RETRY_BUDGET_MS
        : (options.timeoutMs ?? PUBLIC_API_TIMEOUT_MS);
    const deadlineMs = Date.now() + requestTimeoutMs;
    let retryCount = 0;
    while (Date.now() < deadlineMs) {
        const attempt = await performPublicApiAttempt<ResponseModel>(path, options, deadlineMs - Date.now());
        if (attempt.response.ok && attempt.responseJson !== null) return attempt.responseJson as ResponseModel;
        if (attempt.response.ok) {
            throw new Error(
                `CodexMan HTTP 服务响应格式无效（错误码：INVALID_RESPONSE，请求 ID：${attempt.requestId}）`
            );
        }
        const errorDetail = ((attempt.responseJson || {}) as PublicApiErrorModel).error || {};
        if (attempt.response.status === 401 && attempt.publicApiToken) {
            await clearPublicApiTokenIfCurrent(attempt.publicApiToken);
        }
        const traceId = errorDetail.requestId || attempt.response.headers.get('X-Request-Id') || attempt.requestId;
        const responseCode = errorDetail.code || String(attempt.response.status);
        const responseError = new PublicApiRequestError(
            `${errorDetail.message || PUBLIC_API_UNAVAILABLE_MESSAGE}（错误码：${responseCode}，请求 ID：${traceId}）`,
            responseCode,
            traceId,
            attempt.response.status,
            errorDetail.retryable === true
        );
        const canRetry =
            options.retryTransientErrors === true &&
            errorDetail.retryable === true &&
            Boolean(errorDetail.code && PUBLIC_API_RETRYABLE_ERROR_CODES.has(errorDetail.code)) &&
            retryCount < PUBLIC_API_MAX_RETRY_COUNT;
        if (!canRetry) throw responseError;
        const retryAfterMs = parseRetryAfterMs(attempt.response.headers.get('Retry-After'), Date.now());
        const retryDelayMs =
            retryAfterMs ?? PUBLIC_API_RETRY_DELAYS_MS[retryCount] + Math.random() * PUBLIC_API_RETRY_JITTER_MS;
        if (Date.now() + retryDelayMs >= deadlineMs) throw responseError;
        retryCount += 1;
        await waitForPublicApiRetry(retryDelayMs);
    }
    throw new Error('CodexMan HTTP 服务请求超过 60 秒总时限。');
}

/**
 * 启动浏览器设备码授权。
 * 流程：旧设备码流程已下线，保留函数是为了旧页面调用时给出明确提示。
 * 参数：无。
 * 返回：设备码、用户码、有效期和轮询间隔。
 * 异常：服务不可达或响应非法时抛出带 requestId 的错误，前端不接触长期 secret。
 */
export async function createPublicApiDeviceAuthorization(): Promise<PublicApiDeviceAuthorizationModel> {
    throw new Error('设备码授权已下线，请在系统设置中创建或申请 App 授权码。');
}

/**
 * 由本机桌面端批准第三方浏览器展示的设备授权码。
 * 流程：仅通过 Tauri 私有 IPC 提交 userCode，Rust 使用进程内批准方凭据调用本机 HTTP 批准接口。
 * 参数：userCode 为浏览器展示的 XXXX-XXXX 人工核对码。
 * 返回：不包含 clientId、secret 或 Token 的成功说明。
 * 异常：普通 Web、码无效/过期/重复或 sidecar 不可用时抛出可诊断错误。
 */
export async function approvePublicApiDevice(userCode: string): Promise<string> {
    try {
        return await invokeDesktop<string>('approve_public_api_device', { userCode });
    } catch (error) {
        if (error instanceof Error) throw error;
        if (typeof error === 'string' && error.trim()) throw new Error(error.trim());
        throw new Error('设备授权批准失败，请查看本机日志。');
    }
}

/**
 * 轮询浏览器设备码授权结果。
 * 流程：旧设备码流程已下线，保留函数是为了旧页面调用时给出明确提示。
 * 参数：deviceCode 为设备授权启动响应中的高熵轮询码。
 * 返回：短期 Token 有效期秒数。
 * 异常：待批准、过期或服务错误均保留稳定 code/requestId，调用方按 interval 决定是否继续。
 */
export async function pollPublicApiDeviceToken(deviceCode: string): Promise<number> {
    void deviceCode;
    throw new Error('设备码授权已下线，请在系统设置中创建或申请 App 授权码。');
}

/**
 * 请求 App 创建一条公共 API 授权码。
 * 流程：调用无需凭证的授权码申请接口；当前后端在 App 用户确认时直接创建并返回明文授权码。
 * 参数：name 为授权码展示名称，expiresAt 为可选过期时间。
 * 返回：授权结果、明文授权码和过期时间。
 * 异常：服务不可达、用户拒绝或响应非法时抛出带 requestId 的错误。
 */
export async function requestPublicApiAccessToken(
    name: string,
    expiresAt: string | null = null
): Promise<PublicApiAccessTokenRequestModel> {
    const response = await requestPublicApi<PublicApiAccessTokenRequestModel>('/v1/access-tokens/request', {
        method: 'POST',
        payload: { name, expiresAt },
        timeoutMs: PUBLIC_API_TIMEOUT_MS
    });
    if (response.status !== 'approved' || !response.accessToken) {
        throw new Error('授权码申请未通过。');
    }
    await setPublicApiToken(response.accessToken);
    return response;
}

/**
 * 手动创建 App 授权码。
 * 流程：调用系统设置页专用授权码创建接口，成功后返回包含明文 token 的完整记录。
 * 参数：name 为授权码名称，expiresAt 为可选到期时间。
 * 返回：可在系统设置页展示的授权码记录。
 * 异常：鉴权失败、字段校验失败或授权码存储不可用时抛出带 requestId 的错误。
 */
export async function createPublicApiAccessToken(
    name: string,
    expiresAt: string | null
): Promise<PublicApiAccessTokenModel> {
    return requestPublicApi<PublicApiAccessTokenModel>('/v1/access-tokens', {
        method: 'POST',
        payload: { name, expiresAt },
        timeoutMs: PUBLIC_API_TIMEOUT_MS
    });
}

/**
 * 查询 App 授权码列表。
 * 流程：读取系统设置页授权码管理接口，返回所有明文授权码及当前状态。
 * 参数：无。
 * 返回：按服务端创建时间倒序排列的授权码列表。
 * 异常：鉴权失败或授权码存储不可用时抛出带 requestId 的错误。
 */
export async function listPublicApiAccessTokens(): Promise<PublicApiAccessTokenModel[]> {
    return requestPublicApi<PublicApiAccessTokenModel[]>('/v1/access-tokens', {
        method: 'GET',
        timeoutMs: PUBLIC_API_TIMEOUT_MS
    });
}

/**
 * 撤销 App 授权码。
 * 流程：按授权码稳定 ID 调用撤销接口，服务端保留历史记录并返回撤销后的状态。
 * 参数：tokenId 为授权码稳定 ID。
 * 返回：撤销后的授权码记录。
 * 异常：未知授权码、鉴权失败或授权码存储不可用时抛出带 requestId 的错误。
 */
export async function revokePublicApiAccessToken(tokenId: string): Promise<PublicApiAccessTokenModel> {
    return requestPublicApi<PublicApiAccessTokenModel>(`/v1/access-tokens/${encodeURIComponent(tokenId)}/revoke`, {
        method: 'POST',
        timeoutMs: PUBLIC_API_TIMEOUT_MS
    });
}

/**
 * 检查公共 HTTP 服务健康状态。
 * 流程：读取独立 FastAPI `/health`，只校验服务标识和健康状态。
 * 参数：无。
 * 返回：服务可达且健康时返回 true。
 * 边界：超时、非 2xx 或响应不匹配时返回 false，不抛出到页面轮询。
 */
export async function checkPublicApiHealth(): Promise<boolean> {
    try {
        const response = await requestPublicApi<{ ok?: unknown; name?: unknown }>('/health', {
            method: 'GET',
            timeoutMs: PUBLIC_API_HEALTH_TIMEOUT_MS
        });
        return response.ok === true && response.name === 'codexman-ai-api';
    } catch {
        return false;
    }
}

/**
 * 读取 Codex Desktop 公开连接状态。
 * 流程：通过统一 HTTP client 请求 `/v1/codex/connection`，由服务端返回脱敏连接快照。
 * 参数：无。
 * 返回：连接状态、桌面运行状态、可重启能力、原因码和探测时间。
 * 异常：HTTP、鉴权、私有服务或响应错误时抛出类型化异常，调用方必须展示状态未知而非伪报断连。
 */
export async function getCodexConnectionStatus(): Promise<CodexConnectionStatusModel> {
    return requestPublicApi<CodexConnectionStatusModel>('/v1/codex/connection', {
        method: 'GET',
        timeoutMs: 1_500
    });
}

/**
 * 请求 HTTP 服务异步重启 Codex Desktop。
 * 流程：POST 到 `/v1/codex/connection/restart`；202 只表示接受，最终状态由连接接口轮询确认。
 * 参数：无。
 * 返回：服务端是否接受以及接受后的连接状态。
 * 异常：不可重启、已有重启或服务错误时抛出带稳定错误码和 requestId 的类型化异常。
 */
export async function restartCodexConnection(): Promise<CodexConnectionRestartAcceptedModel> {
    return requestPublicApi<CodexConnectionRestartAcceptedModel>('/v1/codex/connection/restart', {
        method: 'POST',
        timeoutMs: 5_000
    });
}

/**
 * 读取公共服务自动生成的 OpenAPI 文档。
 * 流程：请求 FastAPI `/openapi.json`，页面据此渲染真实第三方契约。
 * 参数：无。
 * 返回：OpenAPI 文档模型。
 * 异常：服务不可达或文档无效时抛出公共 API 错误。
 */
export async function readPublicApiOpenApi(): Promise<HttpApiOpenApiDocumentModel> {
    return requestPublicApi<HttpApiOpenApiDocumentModel>('/openapi.json', { method: 'GET' });
}

/**
 * 读取公共服务模型目录。
 * 流程：桌面端通过受保护 IPC 读取本机安全目录，普通 Web 通过 GET `/v1/models` 读取 HTTP 授权后的安全目录。
 * 参数：无。
 * 返回：当前启用状态、默认项及能力列表，均不包含上游地址、真实模型名或密钥。
 * 异常：桌面 IPC、鉴权、网络或响应错误时透传明确错误。
 */
export async function listPublicModels(): Promise<ModelCatalogItemModel[]> {
    if (isTauriRuntime()) return invokeModelDesktop<ModelCatalogItemModel[]>('list_public_model_catalog');
    return requestPublicApi<ModelCatalogItemModel[]>('/v1/models', { method: 'GET' });
}

/**
 * 读取桌面权限和快捷键诊断。
 * 流程：通过 Tauri IPC 查询原生辅助功能状态和快捷键注册结果。
 * 参数：无。
 * 返回：桌面运行诊断；普通 Web 调用会被拒绝。
 * 异常：IPC 或系统查询失败时透传明确错误，不伪造已授权状态。
 */
export async function getRuntimeDiagnostics(): Promise<RuntimeDiagnosticsModel | null> {
    return invokeDesktop<RuntimeDiagnosticsModel>('get_runtime_diagnostics');
}

/**
 * 注册桌面全局快捷键。
 * 流程：把完整快捷键配置交给 Rust 原子替换，失败时由原生侧恢复旧配置。
 * 参数：shortcuts 为三种业务模式的快捷键配置。
 * 返回：原生端确认实际生效的配置。
 * 异常：冲突或系统注册失败时透传原生错误。
 */
export async function registerShortcuts(shortcuts: ShortcutProfileModel): Promise<ShortcutProfileModel> {
    return invokeDesktop<ShortcutProfileModel>('register_shortcuts', { shortcuts });
}

/**
 * 暂停桌面全局快捷键。
 * 流程：快捷键录制前通知 Rust 注销当前配置，避免录制按键触发业务。
 * 参数：无。
 * 返回：暂停完成 Promise。
 * 异常：普通 Web 或注销失败时透传 IPC 错误。
 */
export async function suspendShortcutsForRecording(): Promise<void> {
    await invokeDesktop<void>('suspend_shortcuts_for_recording');
}

/**
 * 读取本机可打开的应用列表。
 * 流程：通过 Tauri IPC 扫描 macOS 常见应用目录，并返回按名称排序的 .app 列表。
 * 参数：无。
 * 返回：可用于快捷键绑定选择的应用选项。
 * 异常：普通 Web 或系统扫描失败时透传 IPC 错误。
 */
export async function listInstalledApplications(): Promise<ApplicationOptionModel[]> {
    return invokeDesktop<ApplicationOptionModel[]>('list_installed_applications');
}

/**
 * 主动请求 CodexMan App 麦克风权限。
 * 流程：通过 AVFoundation requestAccess 触发 macOS 系统授权弹窗，让 CodexMan 出现在麦克风隐私列表。
 * 参数：无。
 * 返回：用户允许时 true；已拒绝、受限制或用户本次拒绝时 false。
 * 异常：普通 Web 或系统调用失败时透传 IPC 错误。
 */
export async function requestMicrophoneAccess(): Promise<boolean> {
    return invokeDesktop<boolean>('request_microphone_access');
}

/**
 * 打开 macOS 麦克风权限设置。
 * 流程：通过受限桌面 IPC 打开系统对应设置页。
 * 参数：无。
 * 返回：打开请求完成 Promise。
 * 异常：普通 Web 或系统调用失败时透传 IPC 错误。
 */
export async function openMicrophoneSettings(): Promise<void> {
    await invokeDesktop<void>('open_microphone_settings');
}

/**
 * 主动请求 CodexMan App 输入监控权限。
 * 流程：通过 IOKit 触发 macOS 输入监控授权记录，用于全局键盘快捷键监听。
 * 参数：无。
 * 返回：系统当前允许时 true；未允许或需要用户手动开启时 false。
 * 异常：普通 Web 或系统调用失败时透传 IPC 错误。
 */
export async function requestInputMonitoringAccess(): Promise<boolean> {
    return invokeDesktop<boolean>('request_input_monitoring_access');
}

/**
 * 打开 macOS 输入监控权限设置。
 * 流程：通过受限桌面 IPC 打开系统对应设置页。
 * 参数：无。
 * 返回：打开请求完成 Promise。
 * 异常：普通 Web 或系统调用失败时透传 IPC 错误。
 */
export async function openInputMonitoringSettings(): Promise<void> {
    await invokeDesktop<void>('open_input_monitoring_settings');
}

/**
 * 打开 macOS 辅助功能设置。
 * 流程：通过受限桌面 IPC 打开系统对应设置页。
 * 参数：无。
 * 返回：打开请求完成 Promise。
 * 异常：普通 Web 或系统调用失败时透传 IPC 错误。
 */
export async function openAccessibilitySettings(): Promise<void> {
    await invokeDesktop<void>('open_accessibility_settings');
}

/**
 * 设置桌面应用开机启动状态。
 * 流程：把目标布尔值交给 Rust 安装或移除用户级启动项。
 * 参数：enabled 表示是否启用开机启动。
 * 返回：设置完成 Promise。
 * 异常：文件或系统设置失败时透传 IPC 错误。
 */
export async function setLoginLaunch(enabled: boolean): Promise<void> {
    await invokeDesktop<void>('set_login_launch', { enabled });
}

/**
 * 读取桌面应用开机启动状态。
 * 流程：通过 Rust 检查当前用户启动项是否真实存在。
 * 参数：无。
 * 返回：已启用返回 true。
 * 异常：普通 Web 或状态读取失败时透传 IPC 错误。
 */
export async function getLoginLaunch(): Promise<boolean> {
    return invokeDesktop<boolean>('get_login_launch');
}

/**
 * 通过公共 HTTP 服务执行语音转写。
 * 流程：只发送音频、MIME 和语言；上游地址、模型与密钥由服务端配置，阻止前端外送服务端密钥。
 * 参数：request 为前端录音结果和显示用模型配置。
 * 返回：转写文本、耗时和实际模型。
 * 异常：非法音频、超限、鉴权或上游异常时抛出带错误码的信息。
 */
export async function transcribeAudio(request: TranscribeRequestModel): Promise<TranscribeResponseModel> {
    const contentType = request.contentType.split(';', 1)[0]?.trim().toLowerCase() || 'audio/webm';
    return requestPublicApi<TranscribeResponseModel>('/v1/audio/transcriptions', {
        method: 'POST',
        payload: {
            modelId: request.modelId,
            audioBase64: request.audioBase64,
            contentType,
            language: request.language
        },
        retryTransientErrors: true
    });
}

/**
 * 通过公共 HTTP 服务执行文本润色或语音整理。
 * 流程：只发送业务文本与处理偏好；模型地址和密钥由服务端可信配置提供。
 * 参数：request 为文本处理模式、正文、词典和场景偏好。
 * 返回：处理后文本、耗时和实际模型。
 * 异常：文本为空、超限、鉴权或上游异常时抛出带错误码的信息。
 */
export async function processText(request: ProcessTextRequestModel): Promise<ProcessTextResponseModel> {
    return requestPublicApi<ProcessTextResponseModel>('/v1/text/process', {
        method: 'POST',
        payload: {
            modelId: request.modelId,
            mode: request.mode,
            text: request.text,
            audioDurationMs: request.audioDurationMs,
            dictionary: request.dictionary,
            contextApp: request.contextApp,
            styleInstruction: request.styleInstruction
        },
        retryTransientErrors: true
    });
}

/**
 * 通过 CodexMan App 主进程执行语音输入。
 * 流程：前端只发起桌面 IPC；录音、ASR、文本整理、历史写入和自动粘贴均由 Rust 侧完成。
 * 参数：mode 为 asr 或 polish，targetApp 为可选目标应用。
 * 返回：本次输出和用户提示。
 * 异常：普通 Web、麦克风权限、模型配置、HTTP 服务或粘贴失败时透传原生错误。
 */
export async function runAppVoicePolish(
    mode: VoicePolishRunModeType,
    targetApp: string
): Promise<AppVoicePolishResponseModel> {
    return invokeDesktop<AppVoicePolishResponseModel>('run_app_voice_polish', { mode, targetApp });
}

/**
 * 停止当前 App 原生录音并进入转写。
 * 流程：悬浮窗确认按钮通过桌面 IPC 设置录音停止信号，Rust 侧继续执行 ASR 和文本处理。
 * 参数：无。
 * 返回：原生侧当前状态文案。
 * 异常：普通 Web、非语音窗口或没有可停止录音时透传原生错误。
 */
export async function stopAppVoiceRecording(): Promise<string> {
    return invokeDesktop<string>('stop_app_voice_recording');
}

/**
 * 取消当前 App 原生语音任务。
 * 流程：悬浮窗取消按钮通过桌面 IPC 设置整链路取消信号，后台请求返回后会丢弃结果。
 * 参数：无。
 * 返回：原生侧当前状态文案。
 * 异常：普通 Web、非语音窗口或取消失败时透传原生错误。
 */
export async function cancelAppVoiceTask(): Promise<string> {
    return invokeDesktop<string>('cancel_app_voice_task');
}

/**
 * 读取本机私有模型安全元数据。
 * 流程：通过受限 Tauri IPC 读取配置；原生端只返回 hasApiKey，不返回密钥正文。
 * 参数：无。
 * 返回：可供模型管理页展示的安全元数据列表。
 * 异常：普通 Web 或原生安全存储读取失败时透传明确错误。
 */
export async function listPrivateModels(): Promise<PrivateModelItemModel[]> {
    return invokeModelDesktop<PrivateModelItemModel[]>('list_private_models');
}

/**
 * 保存本机私有模型。
 * 流程：表单通过单次 IPC 交给原生端，原生端负责安全保存 API Key 并返回脱敏元数据。
 * 参数：request 为新增或编辑模型参数。
 * 返回：保存后的安全元数据。
 * 异常：字段校验、密钥写入或配置落盘失败时透传原生错误。
 */
export async function savePrivateModel(request: SavePrivateModelRequestModel): Promise<PrivateModelItemModel> {
    return invokeModelDesktop<PrivateModelItemModel>('save_private_model', { request });
}

/**
 * 删除本机私有模型。
 * 流程：把不透明 ID 交给原生端同时清理配置与安全密钥。
 * 参数：modelId 为待删除模型 ID。
 * 返回：删除完成 Promise。
 * 异常：模型不存在、仍被保护或原生删除失败时透传错误。
 */
export async function deletePrivateModel(modelId: string): Promise<void> {
    await invokeModelDesktop<void>('delete_private_model', { modelId });
}

/**
 * 测试未保存的私有模型表单。
 * 流程：通过单次 IPC 执行真实上游连通性测试，原生端不得持久化请求中的 API Key。
 * 参数：request 为待测试的完整连接参数。
 * 返回：真实测试结果与说明。
 * 异常：普通 Web、上游连接或鉴权失败时透传原生错误。
 */
export async function testPrivateModel(request: TestPrivateModelRequestModel): Promise<ModelTestResultModel> {
    return invokeModelDesktop<ModelTestResultModel>('test_private_model', { request });
}

/**
 * 读取系统当前选中文本。
 * 流程：通过桌面 IPC 使用原生辅助功能和剪贴板链路读取选区。
 * 参数：无。
 * 返回：选中文本和目标 App 诊断。
 * 异常：权限、焦点或读取失败时透传原生错误。
 */
export async function readSelectedText(): Promise<SelectedTextResponseModel> {
    return invokeDesktop<SelectedTextResponseModel>('read_selected_text');
}

/**
 * 将文本粘贴到目标应用。
 * 流程：由 Rust 恢复焦点、写剪贴板、触发粘贴并核验插入结果。
 * 参数：text 为最终文本，targetApp 为录音或选区来源应用。
 * 返回：命令发送、插入核验和权限诊断。
 * 异常：原生操作无法执行时透传 IPC 错误，不返回伪成功。
 */
export async function pasteText(text: string, targetApp: string): Promise<PasteResponseModel> {
    return invokeDesktop<PasteResponseModel>('paste_text', { text, targetApp });
}

/**
 * 显示结果兜底窗口。
 * 流程：把无法确认自动插入的文本和原因交给 Rust 保存并展示结果窗口。
 * 参数：text 为结果，reason 为失败说明，requiresAccessibility 表示是否建议授权。
 * 返回：展示完成 Promise。
 * 异常：窗口创建或 IPC 失败时透传错误。
 */
export async function showResultWindow(text: string, reason: string, requiresAccessibility: boolean): Promise<void> {
    await invokeDesktop<void>('show_result_window', { text, reason, requiresAccessibility });
}

/**
 * 读取最近一次结果兜底窗口内容。
 * 流程：从 Rust 进程内缓存读取，避免窗口首次加载错过事件。
 * 参数：无。
 * 返回：最近结果；没有缓存时返回 null。
 * 异常：状态锁或 IPC 失败时透传错误。
 */
export async function getLastResultWindowPayload(): Promise<ResultWindowPayloadModel | null> {
    return invokeDesktop<ResultWindowPayloadModel | null>('get_last_result_window_payload');
}

/**
 * 隐藏结果兜底窗口。
 * 流程：通过 Rust 定位结果窗口并隐藏，App 继续后台运行。
 * 参数：无。
 * 返回：隐藏完成 Promise。
 * 异常：窗口不存在或 IPC 失败时透传错误。
 */
export async function hideResultWindow(): Promise<void> {
    await invokeDesktop<void>('hide_result_window');
}

/**
 * 打开真实 CodeX 会话。
 * 流程：通过 HTTP 把 threadId 交给服务端，由 Rust 校验并打开 CodeX Desktop deeplink。
 * 参数：threadId 为 CodeX 返回的真实会话 ID。
 * 返回：已打开的 deeplink。
 * 异常：ID 非法、会话不存在、CodeX 不可用或 HTTP/私有桥接失败时透传错误。
 */
export async function openSessionExternalThread(threadId: string): Promise<void> {
    await requestPublicApi<{ ok: true }>(`/v1/codex/threads/${encodeURIComponent(threadId)}/open`, {
        method: 'POST'
    });
}

/**
 * 读取 CodeX 当前可见工作空间。
 * 流程：请求 HTTP 服务，再由私有桥接调用 Rust 只读本地状态或 app-server。
 * 参数：无。
 * 返回：按最近活跃时间排序的工作空间。
 * 异常：本地状态和 app-server 均不可用时透传错误。
 */
export async function listCodexWorkspaces(): Promise<CodexWorkspaceModel[]> {
    return requestPublicApi<CodexWorkspaceModel[]>('/v1/codex/workspaces', { method: 'GET' });
}

/**
 * 读取指定工作空间下已有 CodeX 会话。
 * 流程：通过 HTTP 提交有界搜索条件，服务端委托 Rust 读取真实会话。
 * 参数：request 为工作空间、分页和搜索条件。
 * 返回：真实会话摘要列表。
 * 异常：路径、分页或底层读取失败时透传错误。
 */
export async function listCodexThreads(request: CodexThreadListRequestModel): Promise<CodexThreadSummaryModel[]> {
    return requestPublicApi<CodexThreadSummaryModel[]>('/v1/codex/threads/search', {
        method: 'POST',
        payload: request
    });
}

/**
 * 读取任务管理真实工作区数据。
 * 流程：通过 HTTP 从 Rust 权威任务库读取项目、任务和会话，前端不推断状态。
 * 参数：projectId 为可选项目 ID。
 * 返回：HTTP 服务从 Rust 权威任务库取得的聚合数据。
 * 异常：HTTP 服务、私有桥接或任务库不可用时透传稳定错误码和 requestId。
 */
export async function loadSessionWorkspaceData(projectId?: string): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>('/v1/task-workspace/query', {
        method: 'POST',
        payload: projectId ? { projectId } : {}
    });
}

/**
 * 创建真实任务项目。
 * 流程：把项目名称、工作空间和项目基础提示词提交给 HTTP，Rust 事务落盘后返回最新聚合数据。
 * 参数：request 为项目展示名称、真实工作空间路径和任务执行基础提示词。
 * 返回：HTTP 服务确认的项目、任务和会话聚合数据。
 * 异常：路径无效、项目重复或任务库写入失败时透传稳定错误码和 requestId。
 */
export async function createSessionProject(
    request: CreateSessionProjectRequestModel
): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>('/v1/projects', { method: 'POST', payload: request });
}

/**
 * 编辑真实任务项目。
 * 流程：把稳定 ID、名称、后续工作空间和项目基础提示词提交给 HTTP，在 Rust 事务确认后返回聚合数据。
 * 参数：request 为完整项目编辑字段。
 * 返回：HTTP 服务确认的项目、任务和会话聚合数据。
 * 异常：项目不存在、路径非法或事务失败时透传，不修改前端列表。
 */
export async function updateSessionProject(
    request: UpdateSessionProjectRequestModel
): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>(`/v1/projects/${encodeURIComponent(request.id)}/update`, {
        method: 'POST',
        payload: { name: request.name, workspacePath: request.workspacePath, basePrompt: request.basePrompt }
    });
}

/**
 * 软删除任务项目。
 * 流程：只通过 HTTP 路径发送项目稳定 ID，由 Rust 事务标记项目已删除并刷新当前聚合。
 * 参数：projectId 为待删除项目 ID。
 * 返回：HTTP 服务确认删除后的聚合数据。
 * 异常：项目不存在、已删除或事务失败时透传，绝不级联删除任务和会话历史。
 */
export async function deleteSessionProject(projectId: string): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>(`/v1/projects/${encodeURIComponent(projectId)}/delete`, {
        method: 'POST'
    });
}

/**
 * 创建真实任务卡片。
 * 流程：把项目、标题和提示词提交给 HTTP，初始状态完全采用 Rust 任务库返回值。
 * 参数：request 为任务创建字段。
 * 返回：HTTP 服务确认的本次任务 ID 和最新聚合数据。
 * 异常：项目不存在、字段非法或任务库写入失败时透传稳定错误码和 requestId。
 */
export async function createSessionTask(
    request: CreateSessionTaskRequestModel
): Promise<CreateSessionTaskResponseModel> {
    return requestPublicApi<CreateSessionTaskResponseModel>('/v1/tasks', { method: 'POST', payload: request });
}

/**
 * 更新真实任务卡片。
 * 流程：把任务 ID、标题和提示词提交给 HTTP，由 Rust 状态机确认只有已创建或等待中任务可修改。
 * 参数：request 为任务更新字段。
 * 返回：HTTP 服务确认的最新聚合数据。
 * 异常：任务不存在、状态不允许或任务库写入失败时透传稳定错误码和 requestId。
 */
export async function updateSessionTask(request: UpdateSessionTaskRequestModel): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>(`/v1/tasks/${encodeURIComponent(request.id)}/update`, {
        method: 'POST',
        payload: { title: request.title, prompt: request.prompt }
    });
}

/**
 * 删除真实任务卡片。
 * 流程：只通过 HTTP 路径提交任务 ID，由 Rust 事务拒绝进行中任务并删除其它状态任务。
 * 参数：taskId 为真实任务 ID。
 * 返回：HTTP 服务确认的最新聚合数据。
 * 异常：任务不存在、状态不允许或任务库写入失败时透传稳定错误码和 requestId。
 */
export async function deleteSessionTask(taskId: string): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>(`/v1/tasks/${encodeURIComponent(taskId)}/delete`, {
        method: 'POST'
    });
}

/**
 * 请求真实任务进入排队。
 * 流程：只通过 HTTP 路径提交任务 ID，前端等待 Rust CAS 确认后再更新看板。
 * 参数：taskId 为真实任务 ID。
 * 返回：HTTP 服务确认的最新聚合数据。
 * 异常：任务不存在、状态不允许或调度失败时透传稳定错误码和 requestId。
 */
export async function queueSessionTask(taskId: string): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>(`/v1/tasks/${encodeURIComponent(taskId)}/queue`, {
        method: 'POST'
    });
}

/**
 * 验收真实待验收任务。
 * 流程：只通过 HTTP 路径提交任务 ID，前端等待 Rust 落库结果后再展示完成状态。
 * 参数：taskId 为真实任务 ID。
 * 返回：HTTP 服务确认的最新聚合数据。
 * 异常：任务不存在、状态不允许或验收落库失败时透传稳定错误码和 requestId。
 */
export async function completeSessionTask(taskId: string): Promise<SessionWorkspaceDataModel> {
    return requestPublicApi<SessionWorkspaceDataModel>(`/v1/tasks/${encodeURIComponent(taskId)}/complete`, {
        method: 'POST'
    });
}

/**
 * 读取本地 JSON 配置分区。
 * 流程：通过 Rust 校验 key 后读取当前配置文件对应值。
 * 参数：key 为 StorageKey 中登记的分区键。
 * 返回：分区 JSON；不存在时返回 null。
 * 异常：文件损坏、键非法或 IPC 失败时透传错误。
 */
export async function readLocalConfigValue(key: string): Promise<LocalConfigJsonValueModel | null> {
    return invokeDesktop<LocalConfigJsonValueModel | null>('read_local_config_value', { key });
}

/**
 * 写入本地 JSON 配置分区。
 * 流程：通过 Rust 原子更新指定 key，不覆盖其它模块分区。
 * 参数：key 为配置分区，value 为可序列化 JSON 值。
 * 返回：写入完成 Promise。
 * 异常：值非法、磁盘写入或 IPC 失败时透传错误。
 */
export async function writeLocalConfigValue(key: string, value: LocalConfigJsonValueModel): Promise<void> {
    await invokeDesktop<void>('write_local_config_value', { key, value });
}

/**
 * 删除本地 JSON 配置分区。
 * 流程：通过 Rust 只移除指定 key 并原子保存剩余配置。
 * 参数：key 为待删除分区。
 * 返回：删除完成 Promise。
 * 异常：键非法、磁盘写入或 IPC 失败时透传错误。
 */
export async function removeLocalConfigValue(key: string): Promise<void> {
    await invokeDesktop<void>('remove_local_config_value', { key });
}

/**
 * 读取本地 JSON 配置完整快照。
 * 流程：通过 Rust 一次读取当前版本和全部分区，供跨窗口同步初始化。
 * 参数：无。
 * 返回：配置变化载荷模型。
 * 异常：文件解析或 IPC 失败时透传错误。
 */
export async function readLocalConfigSnapshot(): Promise<LocalConfigChangedPayloadModel> {
    return invokeDesktop<LocalConfigChangedPayloadModel>('read_local_config_snapshot');
}

/**
 * 启动配置文件变化监听。
 * 流程：通知 Rust 启动单例轮询并向受信 WebView 广播真实变更。
 * 参数：无。
 * 返回：监听启动完成 Promise。
 * 异常：监听初始化或 IPC 失败时透传错误；重复调用由 Rust 保持幂等。
 */
export async function startLocalConfigWatch(): Promise<void> {
    await invokeDesktop<void>('start_local_config_watch');
}

/**
 * 监听桌面或普通 Web 事件。
 * 流程：Tauri 使用原生 event.listen 接收 Rust/跨 WebView 事件；普通 Web 使用当前窗口 CustomEvent。
 * 参数：event 为事件名；handler 为业务载荷处理函数。
 * 返回：取消监听函数。
 * 边界：普通 Web 的 DOM 事件不能跨浏览器窗口。
 */
export async function listenEvent<PayloadModel>(
    event: string,
    handler: (payload: PayloadModel) => void
): Promise<() => void> {
    if (isTauriRuntime()) {
        return listen<PayloadModel>(event, (message) => handler(message.payload));
    }
    const listener = (message: Event) => handler((message as CustomEvent<PayloadModel>).detail);
    window.addEventListener(event, listener);
    return () => window.removeEventListener(event, listener);
}
