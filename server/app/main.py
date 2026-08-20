"""FastAPI 应用入口与唯一 HTTP 路由定义。"""

import asyncio
from contextlib import asynccontextmanager
import hmac
import ipaddress
import logging
from typing import Annotated, AsyncIterator, Dict, List, Optional
from urllib.parse import urlparse

from fastapi import Depends, FastAPI, Header, Path, Request
from fastapi.exceptions import RequestValidationError
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
import httpx
from starlette.exceptions import HTTPException as StarletteHTTPException

from .config import Settings, load_settings
from .auth import AppAccessTokenService
from .errors import ApiError, is_retryable_error
from .logging_config import configure_logging
from .middleware import (
    BodyLimitMiddleware,
    RequestContextMiddleware,
)
from .models import (
    AudioTranscriptionRequest,
    AudioTranscriptionResponse,
    AccessTokenRequestResponse,
    AppAccessTokenApprovalRequest,
    AppAccessTokenApprovalResponse,
    AppAccessTokenResponse,
    AppAccessTokenVerifyResponse,
    AppAccessTokenWriteRequest,
    ErrorEnvelope,
    HealthResponse,
    ModelCatalogResponse,
    CodexConnectionResponse,
    CodexRestartAcceptedResponse,
    CodexThreadResponse,
    CodexThreadSearchRequest,
    CodexWorkspaceResponse,
    MyAppCreateRequest,
    MyAppOpenRequest,
    MyAppPortResponse,
    MyAppResponse,
    MyAppUpdateRequest,
    OperationResponse,
    ProjectWriteRequest,
    SafeBusinessId,
    TaskCreateRequest,
    TaskCreateResponse,
    TaskUpdateRequest,
    TextProcessRequest,
    TextProcessResponse,
    WorkspaceDataResponse,
    WorkspaceQueryRequest,
)
from .private_bridge import PrivateRpcClient, consume_private_rpc_bootstrap
from .service import ModelService
from .rate_limit import ClientRateLimiter


logger = logging.getLogger("aitool.app")
bearer_scheme = HTTPBearer(
    auto_error=False,
    scheme_name="AppAccessToken",
    description="公网来源携带 App 系统设置页维护的授权码；开发环境可使用固定开发授权码。",
)


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """管理应用级配置和 HTTP 客户端生命周期。

    用途：启动时一次性校验环境、配置日志和连接池，退出时可靠关闭连接。
    流程：加载配置，创建并发信号量与 httpx client，yield 后关闭 client。
    参数：``app`` 为 FastAPI 实例。
    返回：异步生命周期迭代器。
    异常边界：配置或日志初始化失败时阻止服务启动；关闭阶段不吞连接池异常。
    """

    settings = load_settings()
    configure_logging(settings)
    timeout = httpx.Timeout(settings.request_timeout_seconds)
    limits = httpx.Limits(
        max_connections=settings.concurrency_limit,
        max_keepalive_connections=settings.concurrency_limit,
    )
    client = httpx.AsyncClient(timeout=timeout, limits=limits, follow_redirects=False)
    app.state.settings = settings
    app.state.concurrency = asyncio.Semaphore(settings.concurrency_limit)
    app.state.app_access_tokens = AppAccessTokenService(
        settings.access_token_database_file
    )
    app.state.client_rate_limiter = ClientRateLimiter(
        settings.client_rate_limit_per_minute,
        settings.client_daily_quota,
        settings.quota_database_file,
    )
    app.state.model_service = ModelService(settings, client)
    app.state.private_rpc = PrivateRpcClient(consume_private_rpc_bootstrap())
    logger.info(
        "service_started",
        extra={"context": {"concurrencyLimit": settings.concurrency_limit}},
    )
    try:
        yield
    finally:
        await client.aclose()
        logger.info("service_stopped")


settings_for_middleware = load_settings()
app = FastAPI(
    title="CodexMan AI API",
    version="1.0.0",
    description=(
        "CodexMan 本机 HTTP sidecar，提供健康检查、App 授权码管理、安全模型目录、音频转写、文本处理、"
        "CodeX 会话查询以及任务项目完整管理。会话与任务接口统一交给桌面业务核心处理，"
        "HTTP 层不打开任务数据库、不启动 CodeX，也不复制任务状态机或暴露内部传输实现。"
        "健康检查和授权码申请可直接访问；内网来源业务请求可免授权码，公网来源业务请求必须携带 "
        "Authorization: Bearer <授权码>；缺失 Origin 的业务请求直接拦截。"
    ),
    servers=[
        {
            "url": settings_for_middleware.public_base_url,
            "description": "固定本机 sidecar",
        }
    ],
    lifespan=lifespan,
)
app.add_middleware(
    BodyLimitMiddleware, max_body_bytes=settings_for_middleware.max_body_bytes
)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
    expose_headers=["X-Request-ID", "Retry-After", "WWW-Authenticate"],
)
app.add_middleware(RequestContextMiddleware)


def build_error_responses(
    status_codes: Dict[int, tuple],
) -> Dict[int, Dict[str, object]]:
    """构建 OpenAPI 错误响应文档。

    用途：让所有路由复用同一错误 envelope、请求 ID Header 和重试 Header 定义。
    流程：自动加入正文中间件 413 与兜底 500，再按路由状态码生成文档；为 428/429/503 声明条件式重试等待。
    参数：``status_codes`` 为 HTTP 状态与该状态业务含义映射。
    返回：可直接传给 FastAPI 路由 ``responses`` 的定义。
    异常边界：只生成文档元数据，不参与运行时响应。
    """

    merged_status_codes = {
        413: ("请求声明或实际正文超过固定 12 MiB 限制。", "REQUEST_BODY_TOO_LARGE"),
        500: ("请求处理发生未预期服务错误。", "INTERNAL_ERROR"),
        **status_codes,
    }
    responses: Dict[int, Dict[str, object]] = {}
    for status_code, response_definition in merged_status_codes.items():
        description, example_code = response_definition
        headers = {
            "X-Request-ID": {
                "description": "服务最终采用的请求 ID；可用于日志排障。",
                "schema": {"type": "string"},
            }
        }
        if status_code in (428, 429, 503):
            retry_after_description = "再次尝试前至少等待的秒数。"
            if status_code == 503:
                retry_after_description = "仅当错误码表示过载或配额存储暂不可用时返回；MODEL_NOT_CONFIGURED 不返回此 Header。"
            headers["Retry-After"] = {
                "description": retry_after_description,
                "schema": {"type": "integer", "minimum": 1},
            }
        if status_code == 401:
            headers["WWW-Authenticate"] = {
                "description": "公网来源业务接口要求 Authorization: Bearer <App 授权码>。",
                "schema": {"type": "string"},
            }
        responses[status_code] = {
            "model": ErrorEnvelope,
            "description": description,
            "headers": headers,
            "content": {
                "application/json": {
                    "example": {
                        "error": {
                            "code": example_code,
                            "message": description,
                            "requestId": "partner-attempt-001",
                            "retryable": is_retryable_error(example_code),
                        }
                    }
                }
            },
        }
    return responses


def build_error_code_documentation(
    error_codes: Dict[str, List[Dict[str, object]]],
) -> Dict[str, List[Dict[str, object]]]:
    """补齐所有路由共有的中间件错误码说明。

    用途：保证 ``x-error-codes`` 与实际可由任意路由触发的 413、500 响应一致。
    流程：先建立固定公共错误策略，再由路由传入的同状态定义覆盖，以保留更精确的业务说明。
    参数：``error_codes`` 为当前路由特有的状态码、错误码和处理建议。
    返回：包含公共中间件错误的完整文档映射。
    异常边界：仅生成 OpenAPI 元数据，不影响运行时响应或重试判断。
    """

    documented_codes: Dict[str, List[Dict[str, object]]] = {
        "413": [
            {
                "code": "REQUEST_BODY_TOO_LARGE",
                "retryable": False,
                "action": "将完整 HTTP 请求正文缩减到 12 MiB 以内后重新发起新请求。",
            }
        ],
        "500": [
            {
                "code": "INTERNAL_ERROR",
                "retryable": False,
                "action": "携带响应 requestId 联系服务方排查，禁止自动重试。",
            }
        ],
    }
    for status_code, route_codes in error_codes.items():
        existing_codes = {
            str(entry.get("code", "")) for entry in documented_codes.get(status_code, [])
        }
        documented_codes.setdefault(status_code, []).extend(
            entry
            for entry in route_codes
            if str(entry.get("code", "")) not in existing_codes
        )
    return documented_codes


def request_id_openapi_parameter() -> Dict[str, object]:
    """构建 X-Request-ID OpenAPI Header 参数。

    用途：精确声明中间件实际接受和替换的请求追踪 ID 规则，同时避免 FastAPI 生成不可达的 Header 422。
    流程：返回可选 Header 参数，schema 限制为 ``^[A-Za-z0-9._-]{1,128}$``。
    参数：无。
    返回：可放入路由 ``openapi_extra.parameters`` 的参数对象。
    异常边界：缺失、空值、非 ASCII、超长或不匹配值不会返回 422，而会替换为 32 字符 UUID4 hex。
    """

    return {
        "name": "X-Request-ID",
        "in": "header",
        "required": False,
        "description": (
            "调用方单次尝试 ID，必须匹配 ^[A-Za-z0-9._-]{1,128}$；缺失、空值、非 ASCII、超长或不匹配时，"
            "服务替换为 32 字符 UUID4 hex，并在响应 X-Request-ID 与 error.requestId 中返回最终值。"
        ),
        "schema": {
            "type": "string",
            "pattern": "^[A-Za-z0-9._-]{1,128}$",
            "minLength": 1,
            "maxLength": 128,
        },
    }


def success_response_documentation(example: Dict[str, object]) -> Dict[str, object]:
    """构建成功响应的 wire contract 文档。

    用途：明确所有成功响应同样包含 X-Request-ID，并提供可复制的完整 JSON example。
    流程：返回 FastAPI responses 中单个 2xx 定义，由 response_model 自动合并 schema。
    参数：``example`` 为不含敏感信息的成功响应示例。
    返回：成功响应 OpenAPI 元数据。
    异常边界：只生成文档，不参与运行时序列化。
    """

    return {
        "description": "请求成功。",
        "headers": {
            "X-Request-ID": {
                "description": "服务最终采用的请求 ID；可用于日志排障。",
                "schema": {"type": "string"},
            }
        },
        "content": {"application/json": {"example": example}},
    }


def _request_id(request: Request) -> str:
    """从请求状态读取追踪 ID。

    用途：让路由、业务错误和异常处理器共享中间件生成的 requestId。
    流程：读取 ``request.state.request_id``，缺失时返回稳定兜底值。
    参数：``request`` 为当前 FastAPI 请求。
    返回：请求追踪 ID。
    异常边界：不会读取不可信查询参数或正文。
    """

    return getattr(request.state, "request_id", "unknown")


def _error_response(
    request: Request,
    status_code: int,
    code: str,
    message: str,
    headers: Optional[Dict[str, str]] = None,
) -> JSONResponse:
    """构造统一错误 JSON 响应。

    用途：集中保证错误 envelope 和 requestId 字段一致。
    流程：按稳定错误码映射重试语义，组装 ``error.code/message/requestId/retryable`` 并返回指定 HTTP 状态。
    参数：``request`` 为请求，``status_code`` 为状态，``code`` 与 ``message`` 为安全错误信息。
    返回：FastAPI ``JSONResponse``。
    异常边界：调用方必须传入已脱敏文案，不应包含异常堆栈或上游正文。
    """

    request.state.error_code = code
    return JSONResponse(
        status_code=status_code,
        content={
            "error": {
                "code": code,
                "message": message,
                "requestId": _request_id(request),
                "retryable": is_retryable_error(code),
            }
        },
        headers=headers,
    )


@app.exception_handler(ApiError)
async def handle_api_error(request: Request, error: ApiError) -> JSONResponse:
    """处理可公开业务异常。

    用途：把服务层 ``ApiError`` 转换为统一错误 envelope。
    流程：直接使用异常中的稳定状态、错误码和脱敏消息。
    参数：``request`` 为当前请求，``error`` 为业务异常。
    返回：统一 JSON 错误响应。
    异常边界：不输出堆栈到客户端。
    """

    request.state.error_code = error.code
    logger.warning(
        "api_error",
        extra={
            "context": {
                "requestId": _request_id(request),
                "clientId": getattr(request.state, "client_id", "anonymous"),
                "errorCode": error.code,
                "statusCode": error.status_code,
            }
        },
    )
    return _error_response(
        request, error.status_code, error.code, error.message, error.headers
    )


@app.exception_handler(RequestValidationError)
async def handle_validation_error(
    request: Request, error: RequestValidationError
) -> JSONResponse:
    """处理请求字段校验异常。

    用途：避免 FastAPI 默认 422 结构破坏统一错误契约。
    流程：记录错误数量，不记录输入值，再返回固定校验提示。
    参数：``request`` 为当前请求，``error`` 为 Pydantic 校验异常。
    返回：422 统一错误响应。
    异常边界：日志不写 ``error.body`` 或字段输入内容。
    """

    logger.info(
        "request_validation_failed",
        extra={
            "context": {
                "requestId": _request_id(request),
                "errorCount": len(error.errors()),
            }
        },
    )
    return _error_response(request, 422, "VALIDATION_ERROR", "请求字段校验失败。")


@app.exception_handler(StarletteHTTPException)
async def handle_http_error(
    request: Request, error: StarletteHTTPException
) -> JSONResponse:
    """处理框架 HTTP 异常。

    用途：统一路由不存在和方法不允许等框架错误结构。
    流程：按状态码映射稳定错误码，使用通用安全文案。
    参数：``request`` 为当前请求，``error`` 为 FastAPI HTTP 异常。
    返回：统一 JSON 错误响应。
    异常边界：不直接透传可能包含内部信息的 detail。
    """

    if error.status_code == 404:
        code, message = "NOT_FOUND", "接口不存在。"
    elif error.status_code == 405:
        code, message = "METHOD_NOT_ALLOWED", "请求方法不允许。"
    else:
        code, message = "HTTP_ERROR", "请求无法处理。"
    return _error_response(request, error.status_code, code, message)


@app.exception_handler(Exception)
async def handle_unexpected_error(request: Request, error: Exception) -> JSONResponse:
    """处理未预期服务异常。

    用途：避免内部堆栈或敏感信息泄露给调用方。
    流程：服务端按 requestId 记录异常堆栈，客户端仅收到固定 500 文案。
    参数：``request`` 为当前请求，``error`` 为未知异常。
    返回：500 统一错误响应。
    异常边界：日志 context 不含请求正文、Authorization 或 API Key。
    """

    logger.exception(
        "unexpected_error",
        extra={
            "context": {
                "requestId": _request_id(request),
                "errorType": type(error).__name__,
            }
        },
    )
    return _error_response(request, 500, "INTERNAL_ERROR", "服务内部错误。")


def _is_internal_origin(origin: str) -> bool:
    """判断请求 Origin 是否属于内网来源。

    参数：``origin`` 为浏览器提交的 Origin Header。
    流程：解析主机名，localhost 和私有网段 IP 视为内网，普通域名默认公网。
    返回：是否可免授权码访问业务接口。
    异常边界：非法或缺失 Origin 不在此处放行，由鉴权依赖统一拒绝。
    """

    try:
        parsed = urlparse(origin)
    except ValueError:
        return False
    hostname = parsed.hostname
    if not hostname:
        return False
    if hostname in ("localhost", "127.0.0.1", "::1", "tauri.localhost"):
        return True
    try:
        address = ipaddress.ip_address(hostname)
    except ValueError:
        return False
    return address.is_private or address.is_loopback


def _api_access_origin(request: Request) -> str:
    """读取业务接口来源标识。

    用途：兼容 Chrome 扩展后台 fetch 无法稳定携带标准 Origin 的场景，同时保留缺失来源拦截规则。
    流程：优先读取浏览器标准 ``Origin``；缺失时读取插件后台显式发送的 ``X-CodexMan-Client-Origin``。
    参数：``request`` 为当前 FastAPI 请求。
    返回：去除首尾空白后的来源字符串；两者均缺失时返回空字符串。
    异常边界：该来源只用于公网/内网判定，不能替代 Bearer 授权码校验。
    """

    origin = request.headers.get("origin", "").strip()
    if origin:
        return origin
    return request.headers.get("x-codexman-client-origin", "").strip()


async def require_api_access(
    request: Request,
    credentials: HTTPAuthorizationCredentials = Depends(bearer_scheme),
) -> str:
    """校验公开业务接口访问权限。

    用途：落实“内网来源免授权码，公网来源必须授权码，缺失 Origin 直接拦截”的统一门禁。
    流程：开发固定授权码优先放行；普通请求先检查 Origin，内网无 token 可放行，公网必须校验 App 明文授权码。
    参数：``request`` 用于读取配置，``credentials`` 为解析后的 Authorization。
    返回：验证通过的调用方 ID。
    异常边界：缺失 Origin、缺失授权码、错误授权码均返回稳定 401，不泄露授权码列表状态。
    """

    supplied_token = credentials.credentials if credentials is not None else ""
    settings: Settings = request.app.state.settings
    if settings.enable_dev_bearer_token and hmac.compare_digest(
        supplied_token, settings.dev_bearer_token
    ):
        request.state.client_id = "dev-access-token"
        return "dev-access-token"

    origin = _api_access_origin(request)
    if not origin:
        raise ApiError(
            401,
            "ORIGIN_REQUIRED",
            "业务请求缺少 Origin，已拒绝访问。",
            {"WWW-Authenticate": "Bearer"},
        )
    token_service: AppAccessTokenService = request.app.state.app_access_tokens
    if _is_internal_origin(origin):
        if supplied_token:
            client_id = token_service.verify(supplied_token)
            request.state.client_id = client_id
            return client_id
        request.state.client_id = "internal-origin"
        return "internal-origin"

    client_id = token_service.verify(supplied_token)
    request.state.client_id = client_id
    return client_id


async def require_internal_control_secret(
    request: Request,
    control_secret: str = Header(default="", alias="X-CodexMan-Internal-Secret"),
) -> None:
    """校验桌面 App 内部控制接口密钥。

    用途：保护 sidecar 运行时热更新等敏感内部接口，避免普通内网 Origin 或公网授权码触发控制操作。
    流程：读取 Rust 通过 Header 传入的当前启动代私有密钥，交给 private RPC 配置做常量时间比较。
    参数：``request`` 提供 sidecar 应用状态，``control_secret`` 为内部控制 Header。
    返回：校验通过无返回。
    异常边界：缺失、错误或未 bootstrap 的 sidecar 均返回 401，不泄露内部密钥状态。
    """

    client: PrivateRpcClient = request.app.state.private_rpc
    if not control_secret or not client.verify_secret(control_secret):
        raise ApiError(401, "UNAUTHORIZED", "内部控制密钥无效。")


async def limit_client_rate(
    request: Request,
    client_id: str = Depends(require_api_access),
) -> None:
    """执行按调用方限流与日配额。

    用途：避免任一第三方独占模型服务资源，并把额度失败与全局并发繁忙区分。
    流程：复用 App 授权码或开发固定授权码鉴权结果，再由 ClientRateLimiter 原子检查和扣减额度。
    参数：``request`` 提供额度器，``client_id`` 为已验签调用方。
    返回：无。
    异常边界：分钟或日额度耗尽分别返回 RATE_LIMIT 或 DAILY_QUOTA_EXCEEDED。
    """

    limiter: ClientRateLimiter = request.app.state.client_rate_limiter
    await limiter.check(client_id)


async def limit_concurrency(request: Request) -> AsyncIterator[None]:
    """限制模型接口并发执行数。

    用途：防止突发请求耗尽上游连接、内存或工作线程。
    流程：在配置的等待时间内获取应用级信号量，路由结束后必定释放。
    参数：``request`` 用于取得信号量和等待超时配置。
    返回：依赖生命周期迭代器，无业务值。
    异常边界：等待超时返回 429；取消或路由异常仍在 finally 释放已获取额度。
    """

    settings: Settings = request.app.state.settings
    semaphore: asyncio.Semaphore = request.app.state.concurrency
    try:
        await asyncio.wait_for(
            semaphore.acquire(), timeout=settings.concurrency_wait_seconds
        )
    except asyncio.TimeoutError as error:
        raise ApiError(
            429, "CONCURRENCY_LIMIT", "服务繁忙，请稍后重试。", {"Retry-After": "1"}
        ) from error
    try:
        yield
    finally:
        semaphore.release()


@app.get(
    "/health",
    response_model=HealthResponse,
    responses={
        200: success_response_documentation({"ok": True, "name": "codexman-ai-api"}),
        **build_error_responses({}),
    },
    tags=["基础"],
    summary="服务健康检查",
    description="无需鉴权，不访问上游模型。成功表示进程已加载有效配置并可接收请求。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation({}),
    },
)
async def health() -> HealthResponse:
    """读取服务健康状态。

    用途：供负载均衡和运维探针确认进程已完成配置初始化。
    流程：返回固定状态，不读取模型目录或调用上游，也不要求 Bearer Token。
    参数：无。
    返回：包含 ``ok`` 与服务名的健康响应。
    异常边界：不暴露版本、密钥、模型或上游地址。
    """

    return HealthResponse(ok=True, name="codexman-ai-api")


ACCESS_TOKEN_RECORD_EXAMPLE = {
    "id": "token_01J00000000000000000000000",
    "name": "Chrome 插件",
    "token": "typesass_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "expiresAt": None,
    "status": "active",
    "createdAt": "2026-08-12T00:00:00Z",
    "revokedAt": None,
    "lastUsedAt": None,
}
ACCESS_TOKEN_ERROR_CODES = {
    "401": [
        {
            "code": "UNAUTHORIZED",
            "retryable": False,
            "action": "检查 Authorization Bearer 授权码是否正确、过期或已撤销。",
        },
        {
            "code": "ORIGIN_REQUIRED",
            "retryable": False,
            "action": "浏览器业务请求必须携带 Origin；缺失时不按内网来源放行。",
        },
    ],
    "404": [
        {
            "code": "ACCESS_TOKEN_NOT_FOUND",
            "retryable": False,
            "action": "刷新授权码列表后重新选择。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正授权码名称、过期时间或额外字段。",
        }
    ],
    "500": [
        {
            "code": "ACCESS_TOKEN_STORE_FAILED",
            "retryable": False,
            "action": "携带 requestId 检查本机授权码存储。",
        }
    ],
    "503": [
        {
            "code": "PRIVATE_SERVICE_UNAVAILABLE",
            "retryable": True,
            "action": "确认 CodexMan 桌面 App 主窗口已启动并处于可响应状态后重试。",
        },
        {
            "code": "RPC_BUSY",
            "retryable": True,
            "action": "稍后重试授权申请，避免重复点击。",
        },
    ],
    "504": [
        {
            "code": "PRIVATE_SERVICE_TIMEOUT",
            "retryable": True,
            "action": "App 确认弹窗超时或桌面端暂未响应，请重新发起授权。",
        }
    ],
}


@app.post(
    "/v1/access-tokens/request",
    response_model=AccessTokenRequestResponse,
    responses={
        200: success_response_documentation(
            {
                "status": "approved",
                "accessToken": "typesass_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                "expiresAt": None,
            }
        ),
        **build_error_responses(
            {
                422: ("请求字段校验失败。", "VALIDATION_ERROR"),
                503: ("桌面 App 暂不可确认授权。", "PRIVATE_SERVICE_UNAVAILABLE"),
                504: ("桌面 App 确认授权超时。", "PRIVATE_SERVICE_TIMEOUT"),
                500: ("授权码保存失败。", "ACCESS_TOKEN_STORE_FAILED"),
            }
        ),
    },
    tags=["鉴权"],
    summary="请求授权码",
    description="没有授权码的客户端请求 App 用户确认授权；桌面 App 确认后才创建授权码并返回。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "422": ACCESS_TOKEN_ERROR_CODES["422"],
                "503": ACCESS_TOKEN_ERROR_CODES["503"],
                "504": ACCESS_TOKEN_ERROR_CODES["504"],
                "500": ACCESS_TOKEN_ERROR_CODES["500"],
            }
        ),
    },
)
async def request_app_access_token(
    request: Request, payload: AppAccessTokenWriteRequest
) -> AccessTokenRequestResponse:
    """请求创建 App 授权码。

    用途：给没有授权码的客户端提供在线申请入口。
    流程：先通过私有 RPC 通知桌面 App 弹出确认框，用户同意后才创建授权码并返回明文。
    参数：``request`` 提供授权码服务，``payload`` 提供建议名称和有效期。
    返回：approved 状态和明文授权码。
    异常边界：用户拒绝时返回 rejected 且不创建授权码；桌面 App 不可用时返回稳定错误。
    """

    client: PrivateRpcClient = request.app.state.private_rpc
    approval_payload = AppAccessTokenApprovalRequest(
        requestId=request.state.request_id,
        name=payload.name,
        expiresAt=payload.expires_at,
    )
    approval_result = await client.call(
        "requestAccessTokenApproval",
        request.state.request_id,
        approval_payload.model_dump(by_alias=True),
    )
    approval = AppAccessTokenApprovalResponse.model_validate(approval_result)
    if not approval.approved:
        return AccessTokenRequestResponse(status="rejected")
    token_service: AppAccessTokenService = request.app.state.app_access_tokens
    record = token_service.create(payload.name, payload.expires_at)
    return AccessTokenRequestResponse(
        status="approved",
        accessToken=str(record["token"]),
        expiresAt=record["expiresAt"],
    )


@app.post(
    "/v1/access-tokens",
    response_model=AppAccessTokenResponse,
    responses={
        200: success_response_documentation(ACCESS_TOKEN_RECORD_EXAMPLE),
        **build_error_responses(
            {
                422: ("请求字段校验失败。", "VALIDATION_ERROR"),
                500: ("授权码保存失败。", "ACCESS_TOKEN_STORE_FAILED"),
            }
        ),
    },
    dependencies=[Depends(require_api_access)],
    tags=["鉴权"],
    summary="创建授权码",
    description="系统设置页手动创建明文授权码；创建后可长期查看和复制。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "401": ACCESS_TOKEN_ERROR_CODES["401"],
                "422": ACCESS_TOKEN_ERROR_CODES["422"],
                "500": ACCESS_TOKEN_ERROR_CODES["500"],
            }
        ),
    },
)
async def create_app_access_token(
    request: Request, payload: AppAccessTokenWriteRequest
) -> object:
    """手动创建 App 授权码。

    流程：访问门禁通过后把名称和有效期交给授权码服务落库。
    参数：``request`` 提供授权码服务；``payload`` 为创建字段。
    返回：包含明文 token 的授权码记录。
    异常边界：不支持权限范围和只展示一次，创建失败不返回伪 token。
    """

    token_service: AppAccessTokenService = request.app.state.app_access_tokens
    return token_service.create(payload.name, payload.expires_at)


@app.get(
    "/v1/access-tokens/verify",
    response_model=AppAccessTokenVerifyResponse,
    responses={
        200: success_response_documentation(
            {"ok": True, "clientId": "token_01J00000000000000000000000"}
        ),
        **build_error_responses({}),
    },
    tags=["鉴权"],
    summary="校验授权码",
    description="校验当前 Authorization Bearer 授权码是否可用；成功只表示授权码有效，不读取项目或模型业务数据。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {"401": ACCESS_TOKEN_ERROR_CODES["401"]}
        ),
    },
)
async def verify_app_access_token(
    client_id: str = Depends(require_api_access),
) -> AppAccessTokenVerifyResponse:
    """校验当前 App 授权码。

    流程：复用统一访问门禁确认 Origin 和 Bearer 授权码，校验通过后返回固定 ok。
    参数：``client_id`` 为鉴权依赖解析出的调用方标识。
    返回：当前授权码可用状态与调用方标识。
    异常边界：无效、撤销、过期或缺少 Origin/Bearer 时由统一门禁返回 401。
    """

    return AppAccessTokenVerifyResponse(ok=True, clientId=client_id)


@app.get(
    "/v1/access-tokens",
    response_model=List[AppAccessTokenResponse],
    responses={
        200: success_response_documentation([ACCESS_TOKEN_RECORD_EXAMPLE]),
        **build_error_responses(
            {500: ("授权码读取失败。", "ACCESS_TOKEN_STORE_FAILED")}
        ),
    },
    dependencies=[Depends(require_api_access)],
    tags=["鉴权"],
    summary="查询授权码列表",
    description="系统设置页查询所有明文授权码及状态；列表用于查看、复制和撤销。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "401": ACCESS_TOKEN_ERROR_CODES["401"],
                "500": ACCESS_TOKEN_ERROR_CODES["500"],
            }
        ),
    },
)
async def list_app_access_tokens(request: Request) -> object:
    """查询 App 授权码列表。

    流程：访问门禁通过后读取独立授权码存储，并计算 active/expired/revoked 状态。
    参数：``request`` 提供授权码服务。
    返回：包含明文 token 的授权码数组。
    异常边界：数据库不可用时返回统一错误 envelope。
    """

    token_service: AppAccessTokenService = request.app.state.app_access_tokens
    return token_service.list_tokens()


@app.post(
    "/v1/access-tokens/{tokenId}/revoke",
    response_model=AppAccessTokenResponse,
    responses={
        200: success_response_documentation(
            {**ACCESS_TOKEN_RECORD_EXAMPLE, "status": "revoked"}
        ),
        **build_error_responses(
            {
                404: ("授权码不存在。", "ACCESS_TOKEN_NOT_FOUND"),
                422: ("路径参数校验失败。", "VALIDATION_ERROR"),
                500: ("授权码撤销失败。", "ACCESS_TOKEN_STORE_FAILED"),
            }
        ),
    },
    dependencies=[Depends(require_api_access)],
    tags=["鉴权"],
    summary="撤销授权码",
    description="系统设置页撤销指定授权码；撤销后公网业务接口不得继续放行该授权码。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "401": ACCESS_TOKEN_ERROR_CODES["401"],
                "404": ACCESS_TOKEN_ERROR_CODES["404"],
                "422": ACCESS_TOKEN_ERROR_CODES["422"],
                "500": ACCESS_TOKEN_ERROR_CODES["500"],
            }
        ),
    },
)
async def revoke_app_access_token(
    request: Request,
    token_id: Annotated[
        SafeBusinessId,
        Path(
            alias="tokenId",
            description="待撤销授权码稳定 ID。",
            examples=["token_01J00000000000000000000000"],
        ),
    ],
) -> object:
    """撤销 App 授权码。

    流程：访问门禁通过后按 ID 撤销授权码；重复撤销保持幂等返回已撤销记录。
    参数：``request`` 提供授权码服务，``token_id`` 为授权码稳定 ID。
    返回：撤销后的授权码记录。
    异常边界：未知 ID 返回 404，不删除记录，便于系统设置页保留历史状态。
    """

    token_service: AppAccessTokenService = request.app.state.app_access_tokens
    return token_service.revoke(token_id)


@app.get(
    "/v1/models",
    response_model=List[ModelCatalogResponse],
    responses={
        200: success_response_documentation(
            [
                {
                    "id": "model_01J00000000000000000000000",
                    "displayName": "语音识别",
                    "capability": "asr",
                    "enabled": True,
                    "isDefault": True,
                }
            ]
        ),
        **build_error_responses(
            {
                401: ("短期 Bearer Token 缺失、错误、过期或已吊销。", "UNAUTHORIZED"),
            }
        ),
    },
    dependencies=[Depends(require_api_access)],
    tags=["AI"],
    summary="读取安全模型目录",
    description="返回可供选择的 opaque ID 和安全元数据；不返回 provider、baseUrl、modelName、apiKey。空目录返回空数组。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "401": [
                    {
                        "code": "UNAUTHORIZED",
                        "retryable": False,
                        "action": "检查来源 Origin 和 App 授权码，或在开发环境使用固定授权码。",
                    }
                ]
            }
        ),
    },
)
async def list_models(request: Request) -> List[ModelCatalogResponse]:
    """读取公开安全模型目录。

    用途：让桌面、浏览器和第三方调用方按 opaque modelId 选择模型，同时隔离所有私有上游配置。
    流程：Bearer 鉴权通过后，从模型服务取得不可变目录并逐项显式映射五个安全字段。
    参数：``request`` 提供已初始化的模型服务；不接收查询参数或请求正文。
    返回：模型安全元数据列表；无配置时返回空列表，服务仍保持可用。
    异常边界：绝不直接序列化私有配置类型，也不记录或返回 provider、URL、上游模型名和密钥。
    """

    service: ModelService = request.app.state.model_service
    return [
        ModelCatalogResponse(
            id=model.id,
            displayName=model.display_name,
            capability=model.capability,
            enabled=model.enabled,
            isDefault=model.is_default,
        )
        for model in service.list_models()
    ]


@app.post(
    "/internal/model-catalog/reload",
    response_model=OperationResponse,
    responses={
        200: success_response_documentation({"ok": True}),
        **build_error_responses(
            {
                401: ("内部控制密钥缺失或无效。", "UNAUTHORIZED"),
                422: ("模型目录结构无效。", "VALIDATION_ERROR"),
            }
        ),
    },
    dependencies=[Depends(require_internal_control_secret)],
    tags=["内部"],
    include_in_schema=False,
)
async def reload_model_catalog(request: Request) -> OperationResponse:
    """热更新 sidecar 运行时模型目录。

    用途：让桌面 App 保存、启停或删除模型后无需重启 PyInstaller/FastAPI 进程即可让业务请求使用新目录。
    流程：校验内部控制密钥后读取 ``modelCatalog`` envelope，复用配置层安全校验并原子替换模型服务目录。
    参数：``request`` 提供原始 JSON 和模型服务状态。
    返回：固定 ``ok=true``。
    异常边界：只接受 Rust 生成的目录数组；校验失败保持旧目录不变，不返回 URL、模型名或 API Key。
    """

    try:
        payload = await request.json()
    except ValueError as error:
        raise ApiError(422, "VALIDATION_ERROR", "模型目录结构无效。") from error
    if not isinstance(payload, dict) or set(payload) != {"modelCatalog"}:
        raise ApiError(422, "VALIDATION_ERROR", "模型目录结构无效。")
    service: ModelService = request.app.state.model_service
    try:
        service.reload_models(payload["modelCatalog"])
    except RuntimeError as error:
        raise ApiError(422, "VALIDATION_ERROR", "模型目录结构无效。") from error
    return OperationResponse(ok=True)


@app.post(
    "/v1/audio/transcriptions",
    response_model=AudioTranscriptionResponse,
    responses={
        200: success_response_documentation(
            {
                "text": "今天下午三点开会。",
                "elapsedMs": 240,
                "modelId": "model_01J00000000000000000000000",
            }
        ),
        **build_error_responses(
            {
                400: ("音频内容或类型无效。", "INVALID_AUDIO_BASE64"),
                401: ("短期 Bearer Token 缺失、错误、过期或已吊销。", "UNAUTHORIZED"),
                404: ("modelId 不存在。", "MODEL_NOT_FOUND"),
                409: ("模型已禁用或能力不匹配。", "MODEL_DISABLED"),
                413: ("请求体或解码后音频超过限制。", "REQUEST_BODY_TOO_LARGE"),
                422: ("JSON、字段、类型或额外字段校验失败。", "VALIDATION_ERROR"),
                429: ("调用方额度或服务并发额度耗尽。", "RATE_LIMIT"),
                500: ("未预期服务错误。", "INTERNAL_ERROR"),
                502: ("上游拒绝、不可达或返回格式无效。", "UPSTREAM_UNAVAILABLE"),
                503: ("调用方额度存储暂不可用。", "QUOTA_STORE_UNAVAILABLE"),
                504: ("上游响应超时。", "UPSTREAM_TIMEOUT"),
            }
        ),
    },
    dependencies=[Depends(limit_client_rate), Depends(limit_concurrency)],
    tags=["AI"],
    summary="语音转文字",
    description="提交 opaque modelId 和 base64 音频。客户端不能传上游地址、上游模型名或模型密钥。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "400": [
                    {
                        "code": code,
                        "retryable": False,
                        "action": "修正音频内容或 MIME 声明后发起新请求。",
                    }
                    for code in [
                        "UNSUPPORTED_AUDIO_TYPE",
                        "INVALID_AUDIO_BASE64",
                        "EMPTY_AUDIO",
                    ]
                ],
                "401": [
                    {
                        "code": "UNAUTHORIZED",
                        "retryable": False,
                        "action": "检查来源 Origin 和 App 授权码，或在开发环境使用固定授权码。",
                    }
                ],
                "404": [
                    {
                        "code": "MODEL_NOT_FOUND",
                        "retryable": False,
                        "action": "刷新模型目录并重新选择。",
                    }
                ],
                "409": [
                    {
                        "code": "MODEL_DISABLED",
                        "retryable": False,
                        "action": "刷新模型目录并选择已启用模型。",
                    },
                    {
                        "code": "MODEL_CAPABILITY_MISMATCH",
                        "retryable": False,
                        "action": "选择 capability=asr 的模型。",
                    },
                ],
                "413": [
                    {
                        "code": code,
                        "retryable": False,
                        "action": "压缩或拆分数据后发起新请求，不自动重试原请求。",
                    }
                    for code in ["REQUEST_BODY_TOO_LARGE", "AUDIO_TOO_LARGE"]
                ],
                "422": [
                    {
                        "code": "VALIDATION_ERROR",
                        "retryable": False,
                        "action": "按 OpenAPI 修正字段。",
                    }
                ],
                "429": [
                    {
                        "code": "RATE_LIMIT",
                        "retryable": True,
                        "action": "按 Retry-After 退避；计入转换请求最多 2 次重试。",
                    },
                    {
                        "code": "DAILY_QUOTA_EXCEEDED",
                        "retryable": False,
                        "action": "停止自动重试，等待 Retry-After 指定的下一 UTC 自然日后再发起新请求。",
                    },
                    {
                        "code": "CONCURRENCY_LIMIT",
                        "retryable": True,
                        "action": "按 Retry-After 退避；计入转换请求最多 2 次重试。",
                    },
                ],
                "500": [
                    {
                        "code": "INTERNAL_ERROR",
                        "retryable": False,
                        "action": "携带 requestId 联系服务方排查，禁止自动重试。",
                    }
                ],
                "502": [
                    {
                        "code": "UPSTREAM_UNAVAILABLE",
                        "retryable": True,
                        "action": "退避后重试；计入转换请求最多 2 次重试。",
                    },
                    {
                        "code": "UPSTREAM_REJECTED",
                        "retryable": False,
                        "action": "携带 requestId 排查上游拒绝原因，禁止自动重试。",
                    },
                    {
                        "code": "UPSTREAM_INVALID_RESPONSE",
                        "retryable": False,
                        "action": "携带 requestId 排查上游响应契约，禁止自动重试。",
                    },
                    {
                        "code": "UPSTREAM_EMPTY_RESULT",
                        "retryable": False,
                        "action": "携带 requestId 排查上游空结果，禁止自动重试。",
                    },
                ],
                "503": [
                    {
                        "code": "MODEL_NOT_CONFIGURED",
                        "retryable": False,
                        "action": "先在桌面端配置 ASR 模型。",
                    },
                    {
                        "code": "QUOTA_STORE_UNAVAILABLE",
                        "retryable": True,
                        "action": "按 Retry-After 重试；计入转换请求最多 2 次重试。",
                    },
                ],
                "504": [
                    {
                        "code": "UPSTREAM_TIMEOUT",
                        "retryable": True,
                        "action": "退避后重试；计入转换请求最多 2 次重试。",
                    }
                ],
            }
        ),
    },
)
async def transcribe_audio(
    request: Request, payload: AudioTranscriptionRequest
) -> AudioTranscriptionResponse:
    """执行音频转写。

    用途：按固定前端契约把 base64 音频提交给服务端固定 ASR 模型。
    流程：鉴权和并发门禁通过后调用 ``ModelService.transcribe`` 并映射响应。
    参数：``request`` 提供应用服务和 requestId，``payload`` 为音频字段。
    返回：``text/elapsedMs/modelId`` 成功响应，modelId 为实际使用的目录 ID。
    异常边界：业务、上游和限制异常交由全局 handler 返回统一 envelope。
    """

    service: ModelService = request.app.state.model_service
    text, elapsed_ms, model_id = await service.transcribe(payload, _request_id(request))
    return AudioTranscriptionResponse(text=text, elapsedMs=elapsed_ms, modelId=model_id)


@app.post(
    "/v1/text/process",
    response_model=TextProcessResponse,
    responses={
        200: success_response_documentation(
            {
                "processedText": "请于今天下午三点参会。",
                "elapsedMs": 180,
                "modelId": "model_01J00000000000000000000001",
            }
        ),
        **build_error_responses(
            {
                400: ("文本或词典业务校验失败。", "INVALID_DICTIONARY"),
                401: ("短期 Bearer Token 缺失、错误、过期或已吊销。", "UNAUTHORIZED"),
                404: ("modelId 不存在。", "MODEL_NOT_FOUND"),
                409: ("模型已禁用或能力不匹配。", "MODEL_DISABLED"),
                413: ("请求体或文本超过限制。", "REQUEST_BODY_TOO_LARGE"),
                422: ("JSON、字段、类型或额外字段校验失败。", "VALIDATION_ERROR"),
                429: ("调用方额度或服务并发额度耗尽。", "RATE_LIMIT"),
                500: ("未预期服务错误。", "INTERNAL_ERROR"),
                502: ("上游拒绝、不可达或返回格式无效。", "UPSTREAM_UNAVAILABLE"),
                503: ("调用方额度存储暂不可用。", "QUOTA_STORE_UNAVAILABLE"),
                504: ("上游响应超时。", "UPSTREAM_TIMEOUT"),
            }
        ),
    },
    dependencies=[Depends(limit_client_rate), Depends(limit_concurrency)],
    tags=["AI"],
    summary="文本整理或润色",
    description="按 opaque modelId 和 dictate/polish 模式处理文本。客户端不能传上游地址、上游模型名或模型密钥。",
    openapi_extra={
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": build_error_code_documentation(
            {
                "400": [
                    {
                        "code": "INVALID_DICTIONARY",
                        "retryable": False,
                        "action": "修正词典字段后发起新请求。",
                    }
                ],
                "401": [
                    {
                        "code": "UNAUTHORIZED",
                        "retryable": False,
                        "action": "检查来源 Origin 和 App 授权码，或在开发环境使用固定授权码。",
                    }
                ],
                "404": [
                    {
                        "code": "MODEL_NOT_FOUND",
                        "retryable": False,
                        "action": "刷新模型目录并重新选择。",
                    }
                ],
                "409": [
                    {
                        "code": "MODEL_DISABLED",
                        "retryable": False,
                        "action": "刷新模型目录并选择已启用模型。",
                    },
                    {
                        "code": "MODEL_CAPABILITY_MISMATCH",
                        "retryable": False,
                        "action": "选择 capability=text 的模型。",
                    },
                ],
                "413": [
                    {
                        "code": code,
                        "retryable": False,
                        "action": "缩短正文或请求体后发起新请求，不自动重试原请求。",
                    }
                    for code in ["REQUEST_BODY_TOO_LARGE", "TEXT_TOO_LARGE"]
                ],
                "422": [
                    {
                        "code": "VALIDATION_ERROR",
                        "retryable": False,
                        "action": "按 OpenAPI 修正字段。",
                    }
                ],
                "429": [
                    {
                        "code": "RATE_LIMIT",
                        "retryable": True,
                        "action": "按 Retry-After 退避；计入转换请求最多 2 次重试。",
                    },
                    {
                        "code": "DAILY_QUOTA_EXCEEDED",
                        "retryable": False,
                        "action": "停止自动重试，等待 Retry-After 指定的下一 UTC 自然日后再发起新请求。",
                    },
                    {
                        "code": "CONCURRENCY_LIMIT",
                        "retryable": True,
                        "action": "按 Retry-After 退避；计入转换请求最多 2 次重试。",
                    },
                ],
                "500": [
                    {
                        "code": "INTERNAL_ERROR",
                        "retryable": False,
                        "action": "携带 requestId 联系服务方排查，禁止自动重试。",
                    }
                ],
                "502": [
                    {
                        "code": "UPSTREAM_UNAVAILABLE",
                        "retryable": True,
                        "action": "退避后重试；计入转换请求最多 2 次重试。",
                    },
                    {
                        "code": "UPSTREAM_REJECTED",
                        "retryable": False,
                        "action": "携带 requestId 排查上游拒绝原因，禁止自动重试。",
                    },
                    {
                        "code": "UPSTREAM_INVALID_RESPONSE",
                        "retryable": False,
                        "action": "携带 requestId 排查上游响应契约，禁止自动重试。",
                    },
                    {
                        "code": "UPSTREAM_EMPTY_RESULT",
                        "retryable": False,
                        "action": "携带 requestId 排查上游空结果，禁止自动重试。",
                    },
                ],
                "503": [
                    {
                        "code": "MODEL_NOT_CONFIGURED",
                        "retryable": False,
                        "action": "先在桌面端配置文本模型。",
                    },
                    {
                        "code": "QUOTA_STORE_UNAVAILABLE",
                        "retryable": True,
                        "action": "按 Retry-After 重试；计入转换请求最多 2 次重试。",
                    },
                ],
                "504": [
                    {
                        "code": "UPSTREAM_TIMEOUT",
                        "retryable": True,
                        "action": "退避后重试；计入转换请求最多 2 次重试。",
                    }
                ],
            }
        ),
    },
)
async def process_text(
    request: Request, payload: TextProcessRequest
) -> TextProcessResponse:
    """执行听写整理或文字润色。

    用途：按固定前端契约处理 dictate/polish 文本。
    流程：鉴权和并发门禁通过后调用 ``ModelService.process_text`` 并映射响应。
    参数：``request`` 提供应用服务和 requestId，``payload`` 为文本处理字段。
    返回：``processedText/elapsedMs/modelId`` 成功响应，modelId 为实际使用的目录 ID。
    异常边界：不接受客户端 URL、模型或 Key，所有失败使用统一错误 envelope。
    """

    service: ModelService = request.app.state.model_service
    processed_text, elapsed_ms, model_id = await service.process_text(
        payload, _request_id(request)
    )
    return TextProcessResponse(
        processedText=processed_text, elapsedMs=elapsed_ms, modelId=model_id
    )


PRIVATE_COMMON_ERROR_CODES = {
    "401": [
        {
            "code": "UNAUTHORIZED",
            "retryable": False,
            "action": "内网来源可免授权码；公网来源需携带 App 授权码。",
        }
    ],
    "502": [
        {
            "code": "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "retryable": False,
            "action": "携带 requestId 检查桌面 App 与 sidecar 版本，禁止自动重试。",
        },
    ],
    "503": [
        {
            "code": "PRIVATE_SERVICE_UNAVAILABLE",
            "retryable": True,
            "action": "确认桌面 App 和 HTTP 服务均已启动，退避后重试读取请求；写请求先查询聚合状态。",
        },
        {
            "code": "RPC_BUSY",
            "retryable": True,
            "action": "本机业务服务当前过载；退避后重试读取请求，写请求先查询状态。",
        },
    ],
    "504": [
        {
            "code": "PRIVATE_SERVICE_TIMEOUT",
            "retryable": True,
            "action": "读取请求可退避重试；写请求先查询工作区确认是否已提交，禁止盲目重放。",
        }
    ],
}

CODEX_WORKSPACE_EXAMPLE = [
    {
        "cwd": "/Users/demo/Documents/project-a",
        "title": "project-a",
        "threadCount": 12,
        "updatedAt": "1786406400000",
    }
]
CODEX_THREAD_EXAMPLE = [
    {
        "id": "0198f25a-1111-7000-8000-000000000001",
        "title": "完善 HTTP 接口文档",
        "parentThreadId": "",
        "depth": 0,
        "agentNickname": "",
        "agentRole": "",
        "updatedAt": "1786406400000",
    }
]
WORKSPACE_DATA_EXAMPLE = {
    "projects": [
        {
            "id": "proj_01J00000000000000000000000",
            "name": "AI 工具接口接入",
            "workspacePath": "/Users/demo/Documents/project-a",
            "basePrompt": "所有任务都优先遵循项目规则。",
            "taskCount": 1,
            "sessionCount": 1,
            "createdAt": "2026-08-11 09:30:00",
            "updatedAt": "2026-08-11 10:15:00",
        }
    ],
    "tasks": [
        {
            "id": "task_01J00000000000000000000000",
            "projectId": "proj_01J00000000000000000000000",
            "title": "完善 HTTP 接口文档",
            "prompt": "检查现有接口契约，补齐请求示例、响应示例和稳定错误码。",
            "attachments": [],
            "status": "waiting_acceptance",
            "currentSessionId": "session_01J00000000000000000000000",
            "externalThreadId": "0198f25a-1111-7000-8000-000000000001",
            "lastError": "",
            "resultJson": '{"summary":"接口文档已补齐","filesChanged":3}',
            "createdAt": "2026-08-11 09:30:00",
            "updatedAt": "2026-08-11 10:15:00",
        }
    ],
    "sessions": [
        {
            "id": "session_01J00000000000000000000000",
            "projectId": "proj_01J00000000000000000000000",
            "taskId": "task_01J00000000000000000000000",
            "provider": "codex",
            "workspacePath": "/Users/demo/Documents/project-a",
            "title": "完善 HTTP 接口文档",
            "status": "waiting_acceptance",
            "externalThreadId": "0198f25a-1111-7000-8000-000000000001",
            "createdAt": "2026-08-11 09:31:00",
            "updatedAt": "2026-08-11 10:14:00",
        }
    ],
}
TASK_CREATE_DATA_EXAMPLE = {
    "createdTaskId": "task_01J00000000000000000000000",
    **WORKSPACE_DATA_EXAMPLE,
}

CODEX_LIST_ERROR_CODES = {
    "503": [
        {
            "code": "CODEX_UNAVAILABLE",
            "retryable": False,
            "action": "确认本机 CodeX 可用并查看 requestId 对应桌面日志；修复依赖后重新查询。",
        }
    ]
}
CODEX_CONNECTION_ERROR_CODES = {
    "500": [
        {
            "code": "CODEX_CONNECTION_STATE_FAILED",
            "retryable": False,
            "action": "携带 requestId 检查桌面日志；禁止根据缓存或进程存在性猜测已连接。",
        }
    ]
}
CODEX_RESTART_ERROR_CODES = {
    "409": [
        {
            "code": "CODEX_RESTART_IN_PROGRESS",
            "retryable": False,
            "action": "停止重复提交并轮询连接状态，等待当前唯一重启流程结束。",
        },
        {
            "code": "CODEX_CDP_PORT_IN_USE",
            "retryable": False,
            "action": "固定本机连接资源被其它服务占用；人工处理冲突，禁止结束未知进程。",
        },
        {
            "code": "CODEX_RESTART_TASK_ACTIVE",
            "retryable": False,
            "action": "仍有 CodexMan 任务正在执行；等待任务进入终态后，再由用户重新确认是否重启。",
        },
        {
            "code": "CODEX_RESTART_FAILED",
            "retryable": False,
            "action": "携带 requestId 检查桌面日志，确认 CodeX 状态后再由用户决定是否重试。",
        },
    ],
    "500": [
        {
            "code": "CODEX_CONNECTION_STATE_FAILED",
            "retryable": False,
            "action": "运行时连接状态不可用；携带 requestId 排障，禁止假报已接受。",
        }
    ],
    "501": [
        {
            "code": "CODEX_PLATFORM_UNSUPPORTED",
            "retryable": False,
            "action": "当前平台不支持 CodeX Desktop 重启；不要继续提交重启请求。",
        }
    ],
}
CODEX_SEARCH_ERROR_CODES = {
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正路径、limit、offset、keyword 或额外字段。",
        }
    ],
    **CODEX_LIST_ERROR_CODES,
}
CODEX_OPEN_ERROR_CODES = {
    "400": [
        {
            "code": "INVALID_THREAD_ID",
            "retryable": False,
            "action": "只使用搜索接口返回的 thread ID，禁止自行拼接或修改。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正路径中的 threadId 格式和长度。",
        }
    ],
    "404": [
        {
            "code": "CODEX_THREAD_NOT_FOUND",
            "retryable": False,
            "action": "会话不存在或已归档；刷新 thread 搜索结果，禁止为未知 ID 构造打开请求。",
        }
    ],
    **CODEX_LIST_ERROR_CODES,
}
TASK_AGGREGATE_INVARIANT_ERROR_CODES = {
    "500": [
        {
            "code": "TASK_PROJECT_CAPACITY_INVALID",
            "retryable": False,
            "action": "项目数据超过首发 200 条不变量；携带 requestId 排查本地任务库，禁止当作空数据。",
        },
        {
            "code": "TASK_WORKSPACE_SERIALIZATION_FAILED",
            "retryable": False,
            "action": "聚合响应序列化失败；携带 requestId 排查本地任务库和版本。",
        },
        {
            "code": "TASK_WORKSPACE_RESPONSE_TOO_LARGE",
            "retryable": False,
            "action": "聚合响应超过 7 MiB 业务预算；携带 requestId 排查容量不变量。",
        },
    ],
}
TASK_WORKSPACE_ERROR_CODES = {
    "404": [
        {
            "code": "TASK_PROJECT_NOT_FOUND",
            "retryable": False,
            "action": "显式 projectId 不存在；刷新聚合项目列表并让用户重新选择，禁止回退其它项目。",
        }
    ],
    "409": [
        {
            "code": "TASK_WORKSPACE_LOAD_FAILED",
            "retryable": False,
            "action": "携带 requestId 检查任务库日志；禁止把数据库或容量故障当作空列表。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正 projectId 格式或移除额外字段。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
PROJECT_CREATE_ERROR_CODES = {
    "400": [
        {
            "code": "TASK_PROJECT_NAME_TOO_LONG",
            "retryable": False,
            "action": "将项目名缩减到 100 个 Unicode 字符以内后创建新请求。",
        }
    ],
    "409": [
        {
            "code": "TASK_PROJECT_LIMIT_REACHED",
            "retryable": False,
            "action": "项目已达到 200 个首发容量上限；不要自动重试或创建重复项目。",
        },
        {
            "code": "TASK_PROJECT_CREATE_FAILED",
            "retryable": False,
            "action": "检查路径存在性、访问权限、名称与路径唯一性，并携带 requestId 排障后重新提交。",
        },
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正名称 100 字符上限、绝对路径长度或额外字段。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
PROJECT_UPDATE_ERROR_CODES = {
    "400": [
        {
            "code": "TASK_PROJECT_NAME_TOO_LONG",
            "retryable": False,
            "action": "将项目名缩减到 100 个 Unicode 字符以内后创建新请求。",
        }
    ],
    "409": [
        {
            "code": "TASK_PROJECT_UPDATE_FAILED",
            "retryable": False,
            "action": "检查路径、权限和唯一性；已有会话路径快照不会被追溯修改。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正 projectId、名称 100 字符上限、路径或额外字段。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
PROJECT_DELETE_ERROR_CODES = {
    "409": [
        {
            "code": "TASK_PROJECT_DELETE_FAILED",
            "retryable": False,
            "action": "确认 projectId 存在且未删除；删除仅标记项目已删除，不级联清理任务或会话历史。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正路径中的 projectId 格式和长度。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
TASK_CREATE_ERROR_CODES = {
    "404": [
        {
            "code": "TASK_PROJECT_NOT_FOUND",
            "retryable": False,
            "action": "刷新项目列表并重新选择，禁止回退到默认项目。",
        }
    ],
    "400": [
        {
            "code": "TASK_TITLE_REQUIRED",
            "retryable": False,
            "action": "提供去除首尾空白后非空的任务标题。",
        },
        {
            "code": "TASK_TITLE_TOO_LONG",
            "retryable": False,
            "action": "将任务标题缩减到 200 个 Unicode 字符以内。",
        },
        {
            "code": "TASK_PROMPT_REQUIRED",
            "retryable": False,
            "action": "提供去除首尾空白后非空的任务提示词。",
        },
        {
            "code": "TASK_PROMPT_TOO_LONG",
            "retryable": False,
            "action": "将提示词缩减到 50000 个 Unicode 字符以内。",
        },
    ],
    "409": [
        {
            "code": "TASK_CREATE_FAILED",
            "retryable": False,
            "action": "携带 requestId 排查事务失败；先查询聚合结果再决定是否重新创建。",
        },
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "按请求 schema 修正 projectId、title、prompt 或额外字段。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
TASK_UPDATE_ERROR_CODES = {
    "400": [
        {
            "code": "TASK_TITLE_REQUIRED",
            "retryable": False,
            "action": "提供去除首尾空白后非空的任务标题。",
        },
        {
            "code": "TASK_TITLE_TOO_LONG",
            "retryable": False,
            "action": "将任务标题缩减到 200 个 Unicode 字符以内。",
        },
        {
            "code": "TASK_PROMPT_REQUIRED",
            "retryable": False,
            "action": "提供去除首尾空白后非空的任务描述。",
        },
        {
            "code": "TASK_PROMPT_TOO_LONG",
            "retryable": False,
            "action": "将任务描述缩减到 50000 个 Unicode 字符以内。",
        },
    ],
    "404": [
        {
            "code": "TASK_NOT_FOUND",
            "retryable": False,
            "action": "任务不存在或已删除；刷新聚合数据后重新选择。",
        }
    ],
    "409": [
        {
            "code": "TASK_UPDATE_STATUS_FORBIDDEN",
            "retryable": False,
            "action": "只有 created 或 queued 任务可以修改；已执行过的任务必须保留历史内容。",
        },
        {
            "code": "TASK_UPDATE_FAILED",
            "retryable": False,
            "action": "刷新聚合数据后重试；禁止对已执行任务自动覆盖。",
        },
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正 taskId、title、prompt 或额外字段。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
TASK_DELETE_ERROR_CODES = {
    "404": [
        {
            "code": "TASK_NOT_FOUND",
            "retryable": False,
            "action": "任务不存在或已删除；刷新聚合数据后重新选择。",
        }
    ],
    "409": [
        {
            "code": "TASK_DELETE_STATUS_FORBIDDEN",
            "retryable": False,
            "action": "running 任务不能删除；等待任务进入终态后由用户重新发起删除。",
        },
        {
            "code": "TASK_DELETE_FAILED",
            "retryable": False,
            "action": "刷新聚合数据后检查任务状态；禁止删除正在执行的任务。",
        },
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正路径中的 taskId 格式和长度。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
TASK_QUEUE_ERROR_CODES = {
    "409": [
        {
            "code": "TASK_QUEUE_FAILED",
            "retryable": False,
            "action": "刷新聚合数据；只对 created 或 failed 任务由用户再次发起排队，禁止自动重放。",
        },
        {
            "code": "CODEX_SEND_UNCERTAIN",
            "retryable": False,
            "action": (
                "该只读预检优先于 Codex Desktop 连接检查；即使当前断连也稳定返回 409。"
                "任务内部 externalStatus=sendUncertain 时，禁止重新排队、自动重试或重放 prompt，"
                "应保留任务与 requestId 供人工核对。"
            ),
        },
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正路径中的 taskId 格式和长度。",
        }
    ],
    "503": [
        {
            "code": "CODEX_DESKTOP_NOT_CONNECTED",
            "retryable": False,
            "action": (
                "仅在 CODEX_SEND_UNCERTAIN 只读预检通过后检查连接；先读取连接状态并由用户确认重启，"
                "连接恢复后再重新发起排队，禁止自动重放 prompt。"
            ),
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
TASK_COMPLETE_ERROR_CODES = {
    "409": [
        {
            "code": "TASK_ACCEPTANCE_FAILED",
            "retryable": False,
            "action": "刷新聚合数据；仅 waiting_acceptance 可人工完成，禁止自动重放。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "修正路径中的 taskId 格式和长度。",
        }
    ],
    **TASK_AGGREGATE_INVARIANT_ERROR_CODES,
}
MY_APP_COMMON_ERROR_CODES = {
    "409": [
        {
            "code": "MY_APP_OPERATION_FAILED",
            "retryable": False,
            "action": "刷新我的应用列表并检查端口、zip 包或 URL；禁止结束非 CodexMan 持有的系统进程。",
        }
    ],
    "422": [
        {
            "code": "VALIDATION_ERROR",
            "retryable": False,
            "action": "按请求 schema 修正名称、端口、URL、zipDataUrl、打开目标或额外字段。",
        }
    ],
}
MY_APP_LIST_ERROR_CODES = {
    "500": [
        {
            "code": "MY_APP_LIST_FAILED",
            "retryable": False,
            "action": "携带 requestId 检查桌面日志和我的应用配置文件，禁止把读取失败当作空列表。",
        }
    ]
}
MY_APP_PORT_ERROR_CODES = {
    "409": [
        {
            "code": "MY_APP_PORT_ALLOCATE_FAILED",
            "retryable": False,
            "action": "当前端口段没有可用端口；让用户手动填写并在保存时再次校验。",
        }
    ]
}
MY_APP_CREATE_ERROR_CODES = {
    "400": [
        {
            "code": "MY_APP_CREATE_FAILED",
            "retryable": False,
            "action": "检查名称、端口、远程 URL 或静态站点 zip；本地托管必须包含 index.html。",
        }
    ],
    **MY_APP_COMMON_ERROR_CODES,
}
MY_APP_UPDATE_ERROR_CODES = {
    "400": [
        {
            "code": "MY_APP_UPDATE_FAILED",
            "retryable": False,
            "action": "检查名称、端口、远程 URL 或 zip；端口变化只会重启 CodexMan 当前持有的服务。",
        }
    ],
    "404": [
        {
            "code": "MY_APP_NOT_FOUND",
            "retryable": False,
            "action": "应用不存在或已删除；刷新列表后重新选择。",
        }
    ],
    **MY_APP_COMMON_ERROR_CODES,
}
MY_APP_DELETE_ERROR_CODES = {
    "404": [
        {
            "code": "MY_APP_NOT_FOUND",
            "retryable": False,
            "action": "应用不存在或已删除；刷新列表后重新选择。",
        }
    ],
    **MY_APP_COMMON_ERROR_CODES,
}
MY_APP_RESTART_ERROR_CODES = {
    "404": [
        {
            "code": "MY_APP_NOT_FOUND",
            "retryable": False,
            "action": "应用不存在或已删除；刷新列表后重新选择。",
        }
    ],
    "409": [
        {
            "code": "MY_APP_RESTART_FAILED",
            "retryable": False,
            "action": "端口被占用、站点目录缺失或 zip 内容无效；不要结束未知进程，修改端口或重新上传后再启动。",
        }
    ],
    "422": MY_APP_COMMON_ERROR_CODES["422"],
}
MY_APP_OPEN_ERROR_CODES = {
    "404": [
        {
            "code": "MY_APP_NOT_FOUND",
            "retryable": False,
            "action": "应用不存在或已删除；刷新列表后重新选择。",
        }
    ],
    "409": [
        {
            "code": "MY_APP_OPEN_FAILED",
            "retryable": False,
            "action": "本地服务启动失败或目标 URL 无效；修复后由用户重新打开。",
        }
    ],
    "422": MY_APP_COMMON_ERROR_CODES["422"],
}

MY_APP_EXAMPLE = {
    "id": "app_01J00000000000000000000000",
    "name": "数据看板",
    "logoDataUrl": "",
    "accessType": "local",
    "port": 18123,
    "remoteUrl": "",
    "localUrl": "http://127.0.0.1:18123",
    "lanUrl": "http://192.168.1.23:18123",
    "openUrl": "http://127.0.0.1:18123",
    "serviceStatus": "running",
    "serviceMessage": "服务已启动。",
    "createdAt": "2026-08-20T00:00:00Z",
    "updatedAt": "2026-08-20T00:00:00Z",
}


def _private_route_error_codes(
    endpoint_error_codes: Dict[str, List[Dict[str, object]]],
) -> Dict[str, List[Dict[str, object]]]:
    """合并公开网关公共故障与单接口真实业务错误码。

    用途：避免十条路由重复声明鉴权和桥接错误，同时禁止用泛化业务码掩盖真实 TASK/CODEX/RPC 契约。
    流程：复制公共错误列表，再把端点同状态码条目追加进去，最后补上全局 413/500 中间件错误。
    参数：``endpoint_error_codes`` 为当前路由真实可返回的业务错误码列表。
    返回：按 HTTP 状态字符串分组的完整错误码、重试语义和调用方动作。
    异常边界：只处理 OpenAPI 元数据；调用方不得传入 socket、secret 或内部请求正文。
    """

    merged = build_error_code_documentation({})
    for status, items in PRIVATE_COMMON_ERROR_CODES.items():
        merged.setdefault(status, []).extend(items)
    for status, items in endpoint_error_codes.items():
        merged.setdefault(status, []).extend(items)
    return merged


def private_route_responses(
    success_example: object,
    endpoint_error_codes: Dict[str, List[Dict[str, object]]],
    success_status_code: int = 200,
) -> Dict[int, Dict[str, object]]:
    """构建单条会话或任务路由的完整响应文档。

    用途：为 200 响应提供可复制 JSON，并让每个非成功 HTTP 状态展示该端点实际错误码。
    流程：合并公共与端点错误，选取每个状态首个稳定码生成 error envelope example，再复用统一 Header 定义。
    参数：``success_example`` 为完整成功 JSON；``endpoint_error_codes`` 为端点特有错误策略；``success_status_code`` 为成功状态码。
    返回：可直接传给 FastAPI ``responses`` 的响应映射。
    异常边界：同一状态存在多个错误码时，完整清单位于 ``x-error-codes``，响应 example 只展示第一个。
    """

    documented_codes = _private_route_error_codes(endpoint_error_codes)
    status_definitions = {
        int(status): (
            "可能错误码：" + "、".join(str(item["code"]) for item in items) + "。",
            str(items[0]["code"]),
        )
        for status, items in documented_codes.items()
        if status not in {"413", "500"}
    }
    return {
        success_status_code: success_response_documentation(success_example),
        **build_error_responses(status_definitions),
    }


def private_route_openapi(
    endpoint_error_codes: Dict[str, List[Dict[str, object]]],
    request_example: Optional[Dict[str, object]] = None,
) -> Dict[str, object]:
    """构建单条会话或任务路由的 OpenAPI 扩展。

    用途：向第三方展示 requestId 规则、完整请求 example 和该端点真实错误码处置方式。
    流程：合并公共与端点错误；有 JSON 正文时把 example 合并进 FastAPI 生成的 requestBody。
    参数：``endpoint_error_codes`` 为端点错误策略；``request_example`` 为可复制请求 JSON，无正文时省略。
    返回：可直接传入 ``openapi_extra`` 的字典。
    异常边界：仅生成公开文档，不包含私有 RPC 方法名、socket 地址、密钥或未实现接口。
    """

    result: Dict[str, object] = {
        "parameters": [request_id_openapi_parameter()],
        "x-error-codes": _private_route_error_codes(endpoint_error_codes),
    }
    if request_example is not None:
        result["requestBody"] = {
            "content": {"application/json": {"example": request_example}}
        }
    return result


async def _call_private(
    request: Request, method: str, params: Dict[str, object]
) -> object:
    """调用当前应用唯一私有 Rust RPC 客户端。

    流程：读取 lifespan 初始化的客户端，携带公开 requestId 和已校验参数执行一次调用。
    参数：``request`` 提供应用状态，``method`` 为内部固定方法名，``params`` 为业务参数。
    返回：Rust 原子业务结果，随后由具体路由响应模型严格验证。
    异常边界：不记录参数或结果，桥接错误交给统一 ``ApiError`` 处理器。
    """

    client: PrivateRpcClient = request.app.state.private_rpc
    return await client.call(method, _request_id(request), params)


@app.get(
    "/v1/my-apps",
    response_model=List[MyAppResponse],
    responses=private_route_responses([MY_APP_EXAMPLE], MY_APP_LIST_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="读取我的应用列表",
    description=(
        "读取本机保存的本地托管和远程 URL 应用，并返回静态服务状态、本机地址和局域网地址。"
        "HTTP 层不读取站点目录、不启动端口服务，所有运行时状态均来自桌面业务核心。"
    ),
    openapi_extra=private_route_openapi(MY_APP_LIST_ERROR_CODES),
)
async def list_my_apps(request: Request) -> object:
    """读取我的应用列表。

    流程：Bearer 依赖先完成鉴权，再以空参数调用 Rust ``listMyApps``。
    参数：``request`` 提供 requestId 和私有 RPC 客户端。
    返回：我的应用列表；无应用时为空数组。
    异常边界：读取配置失败返回统一错误，不伪装为空列表。
    """

    return await _call_private(request, "listMyApps", {})


@app.post(
    "/v1/my-apps/allocate-port",
    response_model=MyAppPortResponse,
    responses=private_route_responses({"port": 18123}, MY_APP_PORT_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="自动分配我的应用本地端口",
    description="请求桌面业务核心在固定端口段内寻找当前可绑定端口；返回值不预占端口，保存时仍会再次校验。",
    openapi_extra=private_route_openapi(MY_APP_PORT_ERROR_CODES),
)
async def allocate_my_app_port(request: Request) -> object:
    """自动分配可用端口。

    流程：调用 Rust ``allocateMyAppPort``，由 Rust 避开已配置端口并尝试绑定检测。
    参数：``request`` 提供 requestId 和私有 RPC 客户端。
    返回：当前检测可用端口。
    异常边界：HTTP 不自行扫描端口，也不保留端口占用。
    """

    return await _call_private(request, "allocateMyAppPort", {})


@app.post(
    "/v1/my-apps",
    response_model=MyAppResponse,
    responses=private_route_responses(MY_APP_EXAMPLE, MY_APP_CREATE_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="创建我的应用",
    description=(
        "创建本地托管或远程 URL 应用。本地托管需要 zipDataUrl 和端口，Rust 会安全解压到 App 数据目录，"
        "随后绑定 0.0.0.0:<port> 以允许局域网访问。"
    ),
    openapi_extra=private_route_openapi(
        MY_APP_CREATE_ERROR_CODES,
        {
            "name": "数据看板",
            "logoDataUrl": "",
            "accessType": "local",
            "port": 18123,
            "remoteUrl": "",
            "zipDataUrl": "data:application/zip;base64,UEsDBBQAAAA...",
        },
    ),
)
async def create_my_app(request: Request, payload: MyAppCreateRequest) -> object:
    """创建我的应用。

    流程：Pydantic 严格校验请求体后调用 Rust ``createMyApp``；Rust 负责 zip 解压、持久化和本地服务启动。
    参数：``request`` 提供 RPC 上下文；``payload`` 为应用配置和可选 zip。
    返回：创建后的应用列表项。
    异常边界：HTTP 不保存 zip、不触碰文件系统，也不启动端口服务。
    """

    return await _call_private(request, "createMyApp", payload.model_dump(by_alias=True))


@app.post(
    "/v1/my-apps/{appId}/update",
    response_model=MyAppResponse,
    responses=private_route_responses(MY_APP_EXAMPLE, MY_APP_UPDATE_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="修改我的应用",
    description=(
        "修改名称、logo、访问方式、端口或远程 URL。端口变化时 Rust 只停止 CodexMan 当前持有的旧服务并重启新端口；"
        "zipDataUrl 为空时复用现有站点目录。"
    ),
    openapi_extra=private_route_openapi(
        MY_APP_UPDATE_ERROR_CODES,
        {
            "name": "数据看板 v2",
            "logoDataUrl": "",
            "accessType": "local",
            "port": 18124,
            "remoteUrl": "",
            "zipDataUrl": "",
        },
    ),
)
async def update_my_app(
    request: Request,
    app_id: Annotated[
        SafeBusinessId,
        Path(alias="appId", description="待修改应用稳定 ID。", examples=["app_01J00000000000000000000000"]),
    ],
    payload: MyAppUpdateRequest,
) -> object:
    """修改我的应用。

    流程：把路径 appId 与严格正文合并后调用 Rust ``updateMyApp``。
    参数：``request`` 提供 RPC 上下文；``app_id`` 为应用 ID；``payload`` 为新配置。
    返回：更新后的应用列表项。
    异常边界：HTTP 不杀端口进程；Rust 只管理当前 App 持有的静态服务线程。
    """

    params = payload.model_dump(by_alias=True, exclude_none=True)
    params["id"] = app_id
    return await _call_private(request, "updateMyApp", params)


@app.post(
    "/v1/my-apps/{appId}/delete",
    response_model=OperationResponse,
    responses=private_route_responses({"ok": True}, MY_APP_DELETE_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="删除我的应用",
    description="删除应用记录；本地托管应用会停止 CodexMan 受管静态服务并删除解压目录，不会结束未知系统进程。",
    openapi_extra=private_route_openapi(MY_APP_DELETE_ERROR_CODES),
)
async def delete_my_app(
    request: Request,
    app_id: Annotated[
        SafeBusinessId,
        Path(alias="appId", description="待删除应用稳定 ID。", examples=["app_01J00000000000000000000000"]),
    ],
) -> OperationResponse:
    """删除我的应用。

    流程：校验 appId 后调用 Rust ``deleteMyApp``，由 Rust 停止受管服务、删除配置和站点目录。
    参数：``request`` 提供 RPC 上下文；``app_id`` 为应用 ID。
    返回：固定成功对象。
    异常边界：HTTP 不直接删除文件，也不结束任何端口进程。
    """

    await _call_private(request, "deleteMyApp", {"appId": app_id})
    return OperationResponse()


@app.post(
    "/v1/my-apps/{appId}/start",
    response_model=MyAppResponse,
    responses=private_route_responses(MY_APP_EXAMPLE, MY_APP_RESTART_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="启动或重启我的应用本地服务",
    description="仅本地托管应用可调用；Rust 停止当前 App 持有的旧服务后重新绑定记录中的固定端口。",
    openapi_extra=private_route_openapi(MY_APP_RESTART_ERROR_CODES),
)
async def restart_my_app(
    request: Request,
    app_id: Annotated[
        SafeBusinessId,
        Path(alias="appId", description="待启动或重启应用稳定 ID。", examples=["app_01J00000000000000000000000"]),
    ],
) -> object:
    """启动或重启我的应用本地服务。

    流程：校验 appId 后调用 Rust ``restartMyApp``，由 Rust 停止受管线程并重新启动静态 HTTP 服务。
    参数：``request`` 提供 RPC 上下文；``app_id`` 为应用 ID。
    返回：最新应用列表项和服务状态。
    异常边界：远程 URL 应用没有本地服务，端口被其它进程占用时返回失败而不杀未知进程。
    """

    return await _call_private(request, "restartMyApp", {"appId": app_id})


@app.post(
    "/v1/my-apps/{appId}/open",
    response_model=OperationResponse,
    responses=private_route_responses({"ok": True}, MY_APP_OPEN_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["我的应用"],
    summary="打开我的应用",
    description="按 target 使用 CodexMan 新窗口或默认浏览器打开；本地托管应用会先确保静态服务启动成功。",
    openapi_extra=private_route_openapi(MY_APP_OPEN_ERROR_CODES, {"target": "codexman"}),
)
async def open_my_app(
    request: Request,
    app_id: Annotated[
        SafeBusinessId,
        Path(alias="appId", description="待打开应用稳定 ID。", examples=["app_01J00000000000000000000000"]),
    ],
    payload: MyAppOpenRequest,
) -> OperationResponse:
    """打开我的应用。

    流程：校验 appId 和打开目标后调用 Rust ``openMyApp``，本地应用由 Rust 先启动静态服务。
    参数：``request`` 提供 RPC 上下文；``app_id`` 为应用 ID；``payload`` 为打开目标。
    返回：固定成功对象。
    异常边界：服务启动失败或 URL 无效时不打开空窗口。
    """

    params = payload.model_dump(by_alias=True)
    params["appId"] = app_id
    await _call_private(request, "openMyApp", params)
    return OperationResponse()


@app.get(
    "/v1/codex/connection",
    response_model=CodexConnectionResponse,
    responses=private_route_responses(
        {
            "state": "connected",
            "connected": True,
            "desktopRunning": True,
            "canRestart": False,
            "reasonCode": "CODEX_CONNECTED",
            "message": "Codex 已连接，可以由 Desktop 原生创建新会话并发送首次任务。",
            "checkedAt": "1786406400000",
        },
        CODEX_CONNECTION_ERROR_CODES,
    ),
    dependencies=[Depends(require_api_access)],
    tags=["会话管理"],
    summary="读取 CodeX Desktop 连接状态",
    description=(
        "请求桌面 Rust 服务探测真实 CodeX Desktop renderer，并返回可轮询的脱敏连接快照。"
        "响应不公开端口、PID、WebSocket 地址、DOM、工作目录或其它内部探针信息；"
        "connected=false 是可判定业务状态，不会伪装成空成功或 HTTP 故障。"
    ),
    openapi_extra=private_route_openapi(CODEX_CONNECTION_ERROR_CODES),
)
async def get_codex_connection(request: Request) -> object:
    """读取真实 CodeX Desktop 连接快照。

    流程：Bearer 依赖先完成鉴权，再以空参数调用 Rust ``getCodexConnection``，由响应模型过滤私有实现字段。
    参数：``request`` 提供 requestId 和应用级私有 RPC 客户端。
    返回：包含稳定状态、原因码、重启能力和探针时间的脱敏连接快照。
    异常边界：Python 不探测进程、不缓存状态、不记录 RPC 参数或响应；私有桥接故障进入统一错误 envelope。
    """

    return await _call_private(request, "getCodexConnection", {})


@app.post(
    "/v1/codex/connection/restart",
    response_model=CodexRestartAcceptedResponse,
    status_code=202,
    responses=private_route_responses(
        {"accepted": True, "state": "restarting"},
        CODEX_RESTART_ERROR_CODES,
        202,
    ),
    dependencies=[Depends(require_api_access)],
    tags=["会话管理"],
    summary="请求异步重启 CodeX Desktop 连接",
    description=(
        "仅把用户明确确认的重启请求交给桌面 Rust 单飞流程；HTTP 202 表示请求已接受，不表示后台重启完成。"
        "明确重启即使当前已连接也会真正退出旧 CodeX，确认受信监听进程和固定端口释放后再启动新实例；"
        "请求接受后通过 GET /v1/codex/connection 继续轮询。"
        "HTTP 层不会直接退出、启动或探测任何本机进程。"
    ),
    openapi_extra=private_route_openapi(CODEX_RESTART_ERROR_CODES),
)
async def restart_codex_connection(request: Request) -> object:
    """接受一次 CodeX Desktop 异步重启请求。

    流程：Bearer 依赖先完成鉴权，再以空参数调用 Rust ``restartCodex``；Rust 负责单飞、任务状态门禁、旧进程退出和端口释放后重启。
    参数：``request`` 提供 requestId 和应用级私有 RPC 客户端。
    返回：HTTP 202 响应正文，说明请求已接受且进入 restarting 状态。
    异常边界：Python 不真实重启 CodeX、不等待后台完成、不自动重试副作用请求，也不记录本机进程信息。
    """

    return await _call_private(request, "restartCodex", {})


@app.get(
    "/v1/codex/workspaces",
    response_model=List[CodexWorkspaceResponse],
    responses=private_route_responses(CODEX_WORKSPACE_EXAMPLE, CODEX_LIST_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["会话管理"],
    summary="读取 CodeX 工作空间",
    description="读取本机 CodeX 已索引工作空间并按最近活跃度返回。HTTP 服务不直接扫描文件，由 Rust 执行路径和索引校验。",
    openapi_extra=private_route_openapi(CODEX_LIST_ERROR_CODES),
)
async def list_codex_workspaces(request: Request) -> object:
    """通过 HTTP 读取真实 CodeX 工作空间。

    流程：Bearer 依赖先完成鉴权，再以空参数调用 Rust ``listCodexWorkspaces``。
    参数：``request`` 提供 requestId 和应用级私有 RPC 客户端。
    返回：按最近活跃度排列的工作空间摘要；无记录时为空数组。
    异常边界：底层索引或私有服务不可用时返回统一错误，不由 Python 扫描 CodeX 文件。
    """

    return await _call_private(request, "listCodexWorkspaces", {})


@app.post(
    "/v1/codex/threads/search",
    response_model=List[CodexThreadResponse],
    responses=private_route_responses(CODEX_THREAD_EXAMPLE, CODEX_SEARCH_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["会话管理"],
    summary="分页搜索 CodeX 会话",
    description="按工作空间、分页和可选关键词搜索真实 CodeX thread；每页 1 到 60 条，不提供未实现的详情或流式接口。",
    openapi_extra=private_route_openapi(
        CODEX_SEARCH_ERROR_CODES,
        {
            "workspaceCwd": "/Users/demo/Documents/project-a",
            "limit": 20,
            "offset": 0,
            "keyword": "接口文档",
        },
    ),
)
async def search_codex_threads(
    request: Request, payload: CodexThreadSearchRequest
) -> object:
    """分页搜索指定工作空间的真实 CodeX 会话。

    流程：Pydantic 先限制工作空间、分页和关键词，再按 camelCase 转发 Rust 查询。
    参数：``request`` 提供 RPC 上下文；``payload`` 为已校验搜索条件。
    返回：当前页会话摘要数组，不包含消息详情或未实现的流式状态。
    异常边界：路径最终由 Rust 校验；关键词和会话内容不会写入 HTTP 日志。
    """

    return await _call_private(
        request, "listCodexThreads", payload.model_dump(by_alias=True)
    )


@app.post(
    "/v1/codex/threads/{threadId}/open",
    response_model=OperationResponse,
    responses=private_route_responses({"ok": True}, CODEX_OPEN_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["会话管理"],
    summary="在 CodeX 中打开会话",
    description=(
        "请求桌面 Rust 服务打开指定 thread。成功表示 Rust 已确认 thread 存在并向操作系统提交打开请求；"
        "不保证 CodeX UI 已完成切换，也不返回伪造会话详情。"
    ),
    openapi_extra=private_route_openapi(CODEX_OPEN_ERROR_CODES),
)
async def open_codex_thread(
    request: Request,
    thread_id: Annotated[
        SafeBusinessId,
        Path(
            alias="threadId",
            description="从会话搜索结果取得的 CodeX thread 稳定 ID。",
            examples=["0198f25a-1111-7000-8000-000000000001"],
        ),
    ],
) -> OperationResponse:
    """通过 HTTP 请求桌面端打开 CodeX 会话。

    流程：校验路径中的安全 ID，调用 Rust ``openCodexThread``，确认完成后返回固定成功对象。
    参数：``request`` 提供 RPC 上下文；``thread_id`` 为 CodeX thread 稳定 ID。
    返回：``{"ok": true}``，仅表示 Rust 已确认会话存在并向 OS 提交打开请求，不保证 CodeX UI 已切换完成。
    异常边界：非法或不存在 ID 返回 4xx，Python 不自行构造 deeplink 或伪造会话详情。
    """

    await _call_private(request, "openCodexThread", {"threadId": thread_id})
    return OperationResponse()


@app.post(
    "/v1/task-workspace/query",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(
        WORKSPACE_DATA_EXAMPLE, TASK_WORKSPACE_ERROR_CODES
    ),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="读取任务工作区聚合数据",
    description="一次读取项目列表以及选中项目的任务和会话，避免多次请求造成状态撕裂；projectId 可省略。",
    openapi_extra=private_route_openapi(
        TASK_WORKSPACE_ERROR_CODES,
        {"projectId": "proj_01J00000000000000000000000"},
    ),
)
async def query_task_workspace(
    request: Request, payload: WorkspaceQueryRequest
) -> object:
    """读取任务与会话的原子聚合快照。

    流程：可选 projectId 校验后调用 Rust ``loadWorkspaceData``，由同一任务库查询项目、任务和会话。
    参数：``request`` 提供 RPC 上下文；``payload`` 提供可选项目筛选。
    返回：项目全集及当前项目的任务、会话数组。
    异常边界：HTTP 层不缓存、不补默认记录，也不推导任何任务或会话状态。
    """

    return await _call_private(
        request,
        "loadWorkspaceData",
        payload.model_dump(by_alias=True, exclude_none=True),
    )


@app.post(
    "/v1/projects",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(
        WORKSPACE_DATA_EXAMPLE, PROJECT_CREATE_ERROR_CODES
    ),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="创建任务项目",
    description="创建并绑定真实工作空间。Rust 校验目录、重复项并在事务提交后返回最新聚合数据。",
    openapi_extra=private_route_openapi(
        PROJECT_CREATE_ERROR_CODES,
        {
            "name": "AI 工具接口接入",
            "workspacePath": "/Users/demo/Documents/project-a",
            "basePrompt": "所有任务都优先遵循项目规则。",
        },
    ),
)
async def create_project(request: Request, payload: ProjectWriteRequest) -> object:
    """创建绑定真实工作空间的任务项目。

    流程：校验名称和路径长度后调用 Rust ``createProject``，等待事务提交并返回聚合快照。
    参数：``request`` 提供 RPC 上下文；``payload`` 为项目名称、工作空间路径和基础提示词。
    返回：创建完成后的完整工作区聚合数据。
    异常边界：路径存在性、重复项和权限由 Rust 最终校验；失败时不生成临时项目。
    """

    return await _call_private(
        request, "createProject", payload.model_dump(by_alias=True)
    )


@app.post(
    "/v1/projects/{projectId}/update",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(
        WORKSPACE_DATA_EXAMPLE, PROJECT_UPDATE_ERROR_CODES
    ),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="更新任务项目",
    description="更新项目名称和后续任务工作空间；已有会话仍保留执行时路径快照。",
    openapi_extra=private_route_openapi(
        PROJECT_UPDATE_ERROR_CODES,
        {
            "name": "AI 工具接口接入 v2",
            "workspacePath": "/Users/demo/Documents/project-a-v2",
            "basePrompt": "所有任务都优先遵循项目规则。",
        },
    ),
)
async def update_project(
    request: Request,
    project_id: Annotated[
        SafeBusinessId,
        Path(
            alias="projectId",
            description="待更新项目稳定 ID。",
            examples=["proj_01J00000000000000000000000"],
        ),
    ],
    payload: ProjectWriteRequest,
) -> object:
    """更新任务项目名称和后续任务工作空间。

    流程：把安全路径 projectId 与严格正文合并，调用 Rust ``updateProject`` 原子更新。
    参数：``request`` 提供 RPC 上下文；``project_id`` 为稳定 ID；``payload`` 为新名称、路径和基础提示词。
    返回：提交完成后的完整工作区聚合数据。
    异常边界：已有会话路径快照不被改写；项目不存在或路径非法时保持原数据。
    """

    params = payload.model_dump(by_alias=True)
    params["id"] = project_id
    return await _call_private(request, "updateProject", params)


@app.post(
    "/v1/projects/{projectId}/delete",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(
        WORKSPACE_DATA_EXAMPLE, PROJECT_DELETE_ERROR_CODES
    ),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="删除任务项目",
    description="软删除任务项目；Rust 仅标记项目已删除，不级联清理任务或会话历史，并在事务后返回最新聚合数据。",
    openapi_extra=private_route_openapi(PROJECT_DELETE_ERROR_CODES),
)
async def delete_project(
    request: Request,
    project_id: Annotated[
        SafeBusinessId,
        Path(
            alias="projectId",
            description="待删除项目稳定 ID。",
            examples=["proj_01J00000000000000000000000"],
        ),
    ],
) -> object:
    """软删除任务项目。

    流程：校验 projectId 后调用 Rust ``deleteProject``，由任务库在事务内标记项目已删除。
    参数：``request`` 提供 RPC 上下文；``project_id`` 为待删除项目稳定 ID。
    返回：删除提交后的完整工作区聚合数据。
    异常边界：未知或已删除项目返回状态冲突，禁止级联删除任务或会话。
    """

    return await _call_private(request, "deleteProject", {"projectId": project_id})


@app.post(
    "/v1/tasks",
    response_model=TaskCreateResponse,
    responses=private_route_responses(
        TASK_CREATE_DATA_EXAMPLE, TASK_CREATE_ERROR_CODES
    ),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="创建任务",
    description="在指定项目创建 created 状态任务。HTTP 不预创建会话，实际落库和初始状态由 Rust 保证。",
    openapi_extra=private_route_openapi(
        TASK_CREATE_ERROR_CODES,
        {
            "projectId": "proj_01J00000000000000000000000",
            "title": "完善 HTTP 接口文档",
            "prompt": "检查现有接口契约，补齐请求示例、响应示例和稳定错误码。",
            "attachments": [],
        },
    ),
)
async def create_task(request: Request, payload: TaskCreateRequest) -> object:
    """在指定项目中创建真实任务记录。

    流程：校验项目 ID、标题和提示词边界后调用 Rust ``createTask`` 完成事务。
    参数：``request`` 提供 RPC 上下文；``payload`` 为项目 ID、标题和完整提示词。
    返回：``createdTaskId`` 和包含该 created 任务的工作区聚合数据；后续操作必须使用该唯一 ID。
    异常边界：HTTP 不预创建会话、不推进状态，提示词和响应正文不会写入日志。
    """

    return await _call_private(request, "createTask", payload.model_dump(by_alias=True))


@app.post(
    "/v1/tasks/{taskId}/update",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(WORKSPACE_DATA_EXAMPLE, TASK_UPDATE_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="修改任务",
    description="修改任务名称和描述；仅 created 或 queued 状态允许更新，已执行过的任务由 Rust 状态机拒绝覆盖。",
    openapi_extra=private_route_openapi(
        TASK_UPDATE_ERROR_CODES,
        {
            "title": "完善任务管理接口文档",
            "prompt": "补充修改任务和删除任务接口说明，并同步界面操作。",
            "attachments": [],
        },
    ),
)
async def update_task(
    request: Request,
    task_id: Annotated[
        SafeBusinessId,
        Path(
            alias="taskId",
            description="待修改任务稳定 ID；仅 created 或 queued 状态允许修改。",
            examples=["task_01J00000000000000000000000"],
        ),
    ],
    payload: TaskUpdateRequest,
) -> object:
    """修改未执行任务的名称和描述。

    流程：把路径 taskId 与严格正文合并后调用 Rust ``updateTask``，由 TaskStore 在事务内校验状态。
    参数：``request`` 提供 RPC 上下文；``task_id`` 为任务 ID；``payload`` 为新标题和描述。
    返回：更新后的完整工作区聚合数据。
    异常边界：只有 created 和 queued 可修改，running 及之后状态不会被覆盖。
    """

    params = payload.model_dump(by_alias=True)
    params["id"] = task_id
    return await _call_private(request, "updateTask", params)


@app.post(
    "/v1/tasks/{taskId}/delete",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(WORKSPACE_DATA_EXAMPLE, TASK_DELETE_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="删除任务",
    description="删除指定任务；除 running 状态外都允许删除，Rust 在事务内校验状态并清理关联本地记录。",
    openapi_extra=private_route_openapi(TASK_DELETE_ERROR_CODES),
)
async def delete_task(
    request: Request,
    task_id: Annotated[
        SafeBusinessId,
        Path(
            alias="taskId",
            description="待删除任务稳定 ID；running 状态不能删除。",
            examples=["task_01J00000000000000000000000"],
        ),
    ],
) -> object:
    """删除非 running 状态的真实任务。

    流程：校验 taskId 后调用 Rust ``deleteTask``，由 TaskStore 在事务内拒绝 running 并删除关联本地记录。
    参数：``request`` 提供 RPC 上下文；``task_id`` 为待删除任务稳定 ID。
    返回：删除后的完整工作区聚合数据。
    异常边界：running 任务正在执行或等待回写，必须保留到终态后再由用户删除。
    """

    return await _call_private(request, "deleteTask", {"taskId": task_id})


@app.post(
    "/v1/tasks/{taskId}/queue",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(WORKSPACE_DATA_EXAMPLE, TASK_QUEUE_ERROR_CODES),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="任务进入执行队列",
    description=(
        "请求桌面业务核心先执行只读 CODEX_SEND_UNCERTAIN 预检，再检查 Codex Desktop 连接，最后以原子状态转换"
        "把 created 或可安全重试的 failed 任务加入执行队列。externalStatus=sendUncertain 的 failed 任务优先返回"
        " 409 CODEX_SEND_UNCERTAIN；即使当前断连也不会降级为 503，且禁止自动重排或重放 prompt。"
        "仅在该预检通过后，未连接才返回 503 CODEX_DESKTOP_NOT_CONNECTED，任务和事件表不会发生变化。"
        "连接检查通过后，任务状态检查与 queue CAS 位于同一事务，"
        "在任何任务、session 或 event 写入前同步拒绝并保证零写入。"
    ),
    openapi_extra=private_route_openapi(TASK_QUEUE_ERROR_CODES),
)
async def queue_task(
    request: Request,
    task_id: Annotated[
        SafeBusinessId,
        Path(
            alias="taskId",
            description="待排队任务稳定 ID。",
            examples=["task_01J00000000000000000000000"],
        ),
    ],
) -> object:
    """请求真实任务进入桌面业务核心执行队列。

    流程：校验 taskId 后调用 Rust ``queueTask``；Rust 先只读检查 ``CODEX_SEND_UNCERTAIN``，再检查 Codex Desktop 连接，
    最后在同一事务内检查任务状态并执行 queue CAS。
    参数：``request`` 提供 RPC 上下文；``task_id`` 为待排队任务稳定 ID。
    返回：包含最新任务状态的工作区聚合数据。
    异常边界：发送结果不确定优先于断连返回稳定 409；仅预检通过后断连才返回 503；
    状态冲突或发送结果不确定时均不重放、不乐观更新；
    状态拒绝发生在任何任务/session/event 写入前，事务保证零写入，HTTP 层也不直接启动 Codex 执行器。
    """

    return await _call_private(request, "queueTask", {"taskId": task_id})


@app.post(
    "/v1/tasks/{taskId}/complete",
    response_model=WorkspaceDataResponse,
    responses=private_route_responses(
        WORKSPACE_DATA_EXAMPLE, TASK_COMPLETE_ERROR_CODES
    ),
    dependencies=[Depends(require_api_access)],
    tags=["任务管理"],
    summary="验收并完成任务",
    description="仅允许 Rust 状态机把 waiting_acceptance 任务转换为 completed，并返回提交后的聚合快照。",
    openapi_extra=private_route_openapi(TASK_COMPLETE_ERROR_CODES),
)
async def complete_task(
    request: Request,
    task_id: Annotated[
        SafeBusinessId,
        Path(
            alias="taskId",
            description="待验收任务稳定 ID；仅 waiting_acceptance 状态允许完成。",
            examples=["task_01J00000000000000000000000"],
        ),
    ],
) -> object:
    """验收 waiting_acceptance 状态的真实任务。

    流程：校验 taskId 后调用 Rust ``completeTask``，由状态机执行 CAS 并提交完成状态。
    参数：``request`` 提供 RPC 上下文；``task_id`` 为待验收任务稳定 ID。
    返回：包含 completed 任务的最新工作区聚合数据。
    异常边界：状态不匹配或任务不存在时返回 4xx，不伪造完成结果或覆盖并发更新。
    """

    return await _call_private(request, "completeTask", {"taskId": task_id})
