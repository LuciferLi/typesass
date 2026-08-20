"""业务错误定义。"""

from typing import Dict, FrozenSet, Optional


RETRYABLE_ERROR_CODES: FrozenSet[str] = frozenset(
    {
        "AUTHORIZATION_PENDING",
        "CONCURRENCY_LIMIT",
        "DEVICE_AUTHORIZATION_CAPACITY",
        "DEVICE_POLLING_TOO_FAST",
        "QUOTA_STORE_UNAVAILABLE",
        "RATE_LIMIT",
        "PRIVATE_SERVICE_TIMEOUT",
        "PRIVATE_SERVICE_UNAVAILABLE",
        "RPC_BUSY",
        "UPSTREAM_TIMEOUT",
        "UPSTREAM_UNAVAILABLE",
    }
)


def is_retryable_error(code: str) -> bool:
    """判断稳定业务错误码是否允许调用方重试。

    用途：让运行时错误 envelope 与 OpenAPI ``x-error-codes`` 使用同一重试语义。
    流程：仅对白名单内的瞬时错误或设备授权等待状态返回 ``True``。
    参数：``code`` 为服务端稳定错误码。
    返回：调用方可按文档约束重试时为 ``True``，否则为 ``False``。
    异常边界：未知错误码默认不可重试，避免 4xx、内部错误或新错误被误重放。
    """

    return code in RETRYABLE_ERROR_CODES


class ApiError(Exception):
    """可安全返回给调用方的业务异常。

    用途：携带稳定 HTTP 状态、业务错误码和脱敏提示。
    流程：业务层抛出后由 FastAPI 全局异常处理器统一生成错误 envelope。
    参数：``status_code`` 为 HTTP 状态码，``code`` 为稳定错误码，``message`` 为安全提示。
    异常边界：不得把密钥、请求正文或完整上游响应传入该类型。
    """

    def __init__(
        self,
        status_code: int,
        code: str,
        message: str,
        headers: Optional[Dict[str, str]] = None,
    ) -> None:
        """初始化业务异常。

        用途：保存异常处理器生成响应所需的最小信息。
        流程：调用父类保存 message，再记录状态码与错误码。
        参数：``status_code``、``code``、``message`` 分别表示 HTTP 状态、协议码和安全提示。
        返回：无。
        异常边界：本方法不执行日志记录或外部 IO。
        """

        super().__init__(message)
        self.status_code = status_code
        self.code = code
        self.message = message
        self.headers = headers or {}


class RequestBodyTooLarge(Exception):
    """请求体超过服务端上限异常。

    用途：让纯 ASGI body 限制中间件在解析 JSON 前中止超大请求。
    流程：接收字节累计超过阈值时抛出，并由中间件直接返回 413 envelope。
    边界：该异常不携带请求内容，避免大正文进入日志。
    """
