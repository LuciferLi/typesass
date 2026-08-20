"""请求 ID、body 限制和访问日志中间件。"""

import json
import logging
import re
import time
from typing import Optional
from uuid import uuid4

from starlette.types import ASGIApp, Message, Receive, Scope, Send

from .errors import RequestBodyTooLarge, is_retryable_error


REQUEST_ID_HEADER = b"x-request-id"
REQUEST_ID_PATTERN = re.compile(r"^[A-Za-z0-9._-]{1,128}$")
logger = logging.getLogger("aitool.access")
app_logger = logging.getLogger("aitool.app")


def _request_id(scope: Scope) -> str:
    """读取或生成请求 ID。

    用途：让日志、错误响应和 HTTP Header 使用同一追踪标识。
    流程：仅接受严格匹配 ``^[A-Za-z0-9._-]{1,128}$`` 的 ASCII 请求头，否则生成 UUID4 hex。
    参数：``scope`` 为当前 ASGI 请求上下文。
    返回：安全请求 ID 字符串。
    异常边界：非 HTTP scope 或非法字节统一生成新 ID。
    """

    for name, value in scope.get("headers", []):
        if name.lower() == REQUEST_ID_HEADER:
            try:
                candidate = value.decode("ascii").strip()
            except UnicodeDecodeError:
                break
            if REQUEST_ID_PATTERN.fullmatch(candidate):
                return candidate
    return uuid4().hex


class RequestContextMiddleware:
    """请求追踪与结构化访问日志中间件。

    用途：为每个 HTTP 请求建立 requestId，并记录方法、路径、状态与耗时。
    流程：写入 scope state，包装 response.start 增加响应头，结束后输出脱敏访问日志。
    边界：不记录查询内容、Authorization 或请求正文；非 HTTP 流量直接透传。
    """

    def __init__(self, app: ASGIApp) -> None:
        """初始化请求上下文中间件。

        用途：保存下游 ASGI 应用引用。
        流程：由 Starlette 中间件栈构造一次。
        参数：``app`` 为下游应用。
        返回：无。
        异常边界：不在初始化阶段读取环境或执行 IO。
        """

        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        """处理单次 ASGI 调用。

        用途：注入请求 ID、响应 Header 和访问日志。
        流程：包装 send 捕获状态码；响应前异常统一生成可追踪 500，最后记录总耗时与错误码。
        参数：``scope``、``receive``、``send`` 为 ASGI 标准对象。
        返回：无。
        异常边界：响应已开始后的连接/流式异常继续抛出，避免重复响应；finally 始终记录最终状态。
        """

        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return
        request_id = _request_id(scope)
        scope.setdefault("state", {})["request_id"] = request_id
        started_at = time.perf_counter()
        status_code = 500
        response_started = False

        async def send_with_request_id(message: Message) -> None:
            """为响应首帧增加 requestId，并捕获状态码。

            用途：统一响应追踪头并为访问日志保存最终状态。
            流程：在 response.start 捕获状态，为没有业务错误码的框架错误补 HTTP 状态码，再追加请求 ID。
            参数：``message`` 为下游发送的 ASGI 响应消息。
            返回：无。
            异常边界：已有业务错误码不覆盖；下游发送异常继续向上抛出，不吞掉连接错误。
            """

            nonlocal response_started, status_code
            if message["type"] == "http.response.start":
                response_started = True
                status_code = int(message["status"])
                if status_code >= 400 and not scope.get("state", {}).get("error_code"):
                    scope.setdefault("state", {})["error_code"] = "HTTP_{0}".format(
                        status_code
                    )
                headers = list(message.get("headers", []))
                headers.append((REQUEST_ID_HEADER, request_id.encode("ascii")))
                message["headers"] = headers
            await send(message)

        try:
            await self.app(scope, receive, send_with_request_id)
        except Exception as error:
            if response_started:
                raise
            scope.setdefault("state", {})["error_code"] = "INTERNAL_ERROR"
            app_logger.exception(
                "unexpected_error",
                extra={
                    "context": {
                        "requestId": request_id,
                        "errorType": type(error).__name__,
                    }
                },
            )
            body = json.dumps(
                {
                    "error": {
                        "code": "INTERNAL_ERROR",
                        "message": "服务内部错误。",
                        "requestId": request_id,
                        "retryable": is_retryable_error("INTERNAL_ERROR"),
                    }
                },
                ensure_ascii=False,
            ).encode("utf-8")
            await send_with_request_id(
                {
                    "type": "http.response.start",
                    "status": 500,
                    "headers": [(b"content-type", b"application/json; charset=utf-8")],
                }
            )
            await send_with_request_id({"type": "http.response.body", "body": body})
        finally:
            logger.info(
                "http_request",
                extra={
                    "context": {
                        "requestId": request_id,
                        "method": scope.get("method", ""),
                        "path": scope.get("path", ""),
                        "statusCode": status_code,
                        "elapsedMs": int((time.perf_counter() - started_at) * 1000),
                        "clientId": scope.get("state", {}).get(
                            "client_id", "anonymous"
                        ),
                        "errorCode": scope.get("state", {}).get("error_code", ""),
                    }
                },
            )


class BodyLimitMiddleware:
    """流式请求体大小限制中间件。

    用途：在 JSON/base64 解析前阻断超大 body，降低内存耗尽风险。
    流程：先检查 Content-Length，再包装 receive 累计实际数据帧字节数。
    边界：同时覆盖 Content-Length 和 chunked 请求；非 HTTP 请求直接透传。
    """

    def __init__(self, app: ASGIApp, max_body_bytes: int) -> None:
        """初始化 body 限制。

        用途：保存下游应用及进程级请求上限。
        流程：由应用启动时注入已校验的正整数配置。
        参数：``app`` 为下游应用，``max_body_bytes`` 为最大字节数。
        返回：无。
        异常边界：非法上限由配置层提前阻止。
        """

        self.app = app
        self.max_body_bytes = max_body_bytes

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        """限制单次 HTTP 请求正文。

        用途：拒绝声明或实际超过阈值的请求。
        流程：校验头部，累计 receive body；超限后直接写 413 JSON envelope。
        参数：``scope``、``receive``、``send`` 为 ASGI 标准对象。
        返回：无。
        异常边界：不读取或记录超限正文；下游已开始响应时不会发生该异常。
        """

        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return
        content_length = self._content_length(scope)
        request_id = scope.get("state", {}).get("request_id", uuid4().hex)
        if content_length is not None and content_length > self.max_body_bytes:
            scope.setdefault("state", {})["error_code"] = "REQUEST_BODY_TOO_LARGE"
            await self._send_too_large(send, request_id)
            return
        received_bytes = 0

        async def limited_receive() -> Message:
            """累计当前请求的 ASGI body 帧并在超限时中止。

            用途：覆盖无 Content-Length 或使用分块传输的请求。
            流程：读取一帧、累计 body 字节数，超过上限时抛出内部异常。
            参数：无，闭包使用当前请求的 ``receive``。
            返回：未超限的原始 ASGI 消息。
            异常边界：超限时不记录或继续缓存请求正文。
            """

            nonlocal received_bytes
            message = await receive()
            if message["type"] == "http.request":
                received_bytes += len(message.get("body", b""))
                if received_bytes > self.max_body_bytes:
                    raise RequestBodyTooLarge()
            return message

        try:
            await self.app(scope, limited_receive, send)
        except RequestBodyTooLarge:
            scope.setdefault("state", {})["error_code"] = "REQUEST_BODY_TOO_LARGE"
            await self._send_too_large(send, request_id)

    def _content_length(self, scope: Scope) -> Optional[int]:
        """解析 Content-Length。

        用途：在读取正文前快速拒绝明确超限请求。
        流程：查找对应头并转换非负整数。
        参数：``scope`` 为 HTTP ASGI scope。
        返回：有效长度或 ``None``。
        异常边界：非法值按未知长度处理，仍由流式累计兜底。
        """

        for name, value in scope.get("headers", []):
            if name.lower() == b"content-length":
                try:
                    length = int(value)
                    return length if length >= 0 else None
                except ValueError:
                    return None
        return None

    async def _send_too_large(self, send: Send, request_id: str) -> None:
        """发送统一 413 错误。

        用途：在框架尚未构建 Request 对象时仍保持错误 envelope 一致。
        流程：序列化固定错误码和 requestId，发送 response.start 与 response.body。
        参数：``send`` 为 ASGI 响应函数，``request_id`` 为追踪 ID。
        返回：无。
        异常边界：不包含请求大小或正文内容，避免信息泄露。
        """

        body = json.dumps(
            {
                "error": {
                    "code": "REQUEST_BODY_TOO_LARGE",
                    "message": "请求体超过服务限制。",
                    "requestId": request_id,
                    "retryable": is_retryable_error("REQUEST_BODY_TOO_LARGE"),
                }
            },
            ensure_ascii=False,
        ).encode("utf-8")
        await send(
            {
                "type": "http.response.start",
                "status": 413,
                "headers": [(b"content-type", b"application/json; charset=utf-8")],
            }
        )
        await send({"type": "http.response.body", "body": body})
