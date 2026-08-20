"""纯 ASGI 请求上下文与正文限制中间件测试。"""

import json
from typing import Any, Optional

import pytest

from app.middleware import (
    BodyLimitMiddleware,
    RequestContextMiddleware,
    _request_id,
)


def http_scope(headers: Optional[list[tuple[bytes, bytes]]] = None) -> dict[str, Any]:
    """构造最小 HTTP ASGI scope。"""

    return {"type": "http", "method": "POST", "path": "/test", "headers": headers or []}


async def empty_receive() -> dict[str, Any]:
    """返回无正文请求帧。"""

    return {"type": "http.request", "body": b"", "more_body": False}


def collecting_send(messages: list[dict[str, Any]]) -> Any:
    """构造符合 ASGI 协议的异步响应收集函数。"""

    async def send(message: dict[str, Any]) -> None:
        messages.append(message)

    return send


@pytest.mark.parametrize(
    ("header_value", "accepted"),
    [
        (b" safe-ID_1.2 ", True),
        (b"", False),
        (b"unsafe id", False),
        (b"x" * 129, False),
        (b"safe\xff", False),
    ],
)
def test_tc_mid_001_request_id_validation(header_value: bytes, accepted: bool) -> None:
    """TC-MID-001 请求 ID 仅接受安全 ASCII 格式并为非法值生成新值。"""

    value = _request_id(http_scope([(b"X-Request-ID", header_value)]))
    expected = header_value.decode("ascii", errors="ignore").strip()
    assert value == expected if accepted else len(value) == 32


@pytest.mark.asyncio
async def test_tc_mid_002_request_context_http_and_non_http(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """TC-MID-002 HTTP 响应注入 request ID 并记录状态，非 HTTP 直接透传。"""

    sent: list[dict[str, Any]] = []

    async def downstream(scope: dict[str, Any], receive: object, send: object) -> None:
        scope.setdefault("state", {})["client_id"] = "desktop-test"
        scope["state"]["error_code"] = "RATE_LIMIT"
        await send({"type": "http.response.start", "status": 201})  # type: ignore[operator]
        await send({"type": "http.response.body", "body": b"ok"})  # type: ignore[operator]

    middleware = RequestContextMiddleware(downstream)
    with caplog.at_level("INFO", logger="aitool.access"):
        scope = http_scope([(b"x-request-id", b"known-id")])
        await middleware(scope, empty_receive, collecting_send(sent))
    assert scope["state"]["request_id"] == "known-id"
    assert sent[0]["status"] == 201
    assert (b"x-request-id", b"known-id") in sent[0]["headers"]
    assert caplog.records[-1].context["statusCode"] == 201  # type: ignore[attr-defined]
    assert caplog.records[-1].context["clientId"] == "desktop-test"  # type: ignore[attr-defined]
    assert caplog.records[-1].context["errorCode"] == "RATE_LIMIT"  # type: ignore[attr-defined]

    async def framework_error(scope: object, receive: object, send: object) -> None:
        await send({"type": "http.response.start", "status": 404, "headers": []})  # type: ignore[operator]
        await send({"type": "http.response.body", "body": b"missing"})  # type: ignore[operator]

    framework_scope = http_scope()
    await RequestContextMiddleware(framework_error)(
        framework_scope,
        empty_receive,
        collecting_send([]),
    )
    assert framework_scope["state"]["error_code"] == "HTTP_404"

    called: list[str] = []

    async def websocket(scope: dict[str, Any], receive: object, send: object) -> None:
        called.append(scope["type"])

    await RequestContextMiddleware(websocket)(
        {"type": "websocket"}, empty_receive, collecting_send(sent)
    )
    assert called == ["websocket"]


@pytest.mark.asyncio
async def test_tc_mid_003_request_context_logs_500_on_failure(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """TC-MID-003 响应前异常统一为 500，响应开始后的异常继续抛出。"""

    async def failing(scope: object, receive: object, send: object) -> None:
        raise RuntimeError("fake downstream error")

    sent: list[dict[str, Any]] = []
    with caplog.at_level("INFO", logger="aitool.access"):
        await RequestContextMiddleware(failing)(
            http_scope(), empty_receive, collecting_send(sent)
        )
    assert sent[0]["status"] == 500
    assert len(dict(sent[0]["headers"])[b"x-request-id"]) == 32
    assert json.loads(sent[1]["body"])["error"]["code"] == "INTERNAL_ERROR"
    assert json.loads(sent[1]["body"])["error"]["retryable"] is False
    assert caplog.records[-1].context["statusCode"] == 500  # type: ignore[attr-defined]
    assert caplog.records[-1].context["errorCode"] == "INTERNAL_ERROR"  # type: ignore[attr-defined]

    async def started_then_failing(
        scope: object, receive: object, send: object
    ) -> None:
        await send({"type": "http.response.start", "status": 200})  # type: ignore[operator]
        raise RuntimeError("fake error after response")

    with pytest.raises(RuntimeError, match="after response"):
        await RequestContextMiddleware(started_then_failing)(
            http_scope(), empty_receive, collecting_send([])
        )


@pytest.mark.asyncio
async def test_tc_mid_004_body_limit_content_length_and_passthrough() -> None:
    """TC-MID-004 Content-Length 超限立即返回 413，合法和非 HTTP 请求透传。"""

    sent: list[dict[str, Any]] = []
    calls: list[str] = []

    async def downstream(scope: dict[str, Any], receive: object, send: object) -> None:
        calls.append(scope["type"])
        if scope["type"] == "http":
            await receive()  # type: ignore[operator]

    middleware = BodyLimitMiddleware(downstream, max_body_bytes=3)
    await middleware(
        http_scope([(b"content-length", b"4")]), empty_receive, collecting_send(sent)
    )
    assert sent[0]["status"] == 413
    payload = json.loads(sent[1]["body"])
    assert payload["error"]["code"] == "REQUEST_BODY_TOO_LARGE"
    assert len(payload["error"]["requestId"]) == 32
    assert payload["error"]["retryable"] is False
    assert calls == []

    await middleware(
        http_scope([(b"content-length", b"3")]), empty_receive, collecting_send(sent)
    )
    await middleware({"type": "lifespan"}, empty_receive, collecting_send(sent))
    assert calls == ["http", "lifespan"]


@pytest.mark.asyncio
async def test_tc_mid_005_chunked_body_limit_and_request_id() -> None:
    """TC-MID-005 无有效长度头时累计分帧正文并保留已有 request ID。"""

    frames = iter(
        [
            {"type": "http.request", "body": b"12", "more_body": True},
            {"type": "custom.event"},
            {"type": "http.request", "body": b"34", "more_body": False},
        ]
    )
    sent: list[dict[str, Any]] = []

    async def receive() -> dict[str, Any]:
        return next(frames)

    async def downstream(
        scope: dict[str, Any], receive_limited: object, send: object
    ) -> None:
        await receive_limited()  # type: ignore[operator]
        await receive_limited()  # type: ignore[operator]
        await receive_limited()  # type: ignore[operator]

    scope = http_scope([(b"content-length", b"invalid")])
    scope["state"] = {"request_id": "existing-id"}
    await BodyLimitMiddleware(downstream, max_body_bytes=3)(
        scope, receive, collecting_send(sent)
    )
    payload = json.loads(sent[1]["body"])
    assert payload["error"]["requestId"] == "existing-id"


@pytest.mark.parametrize(
    ("headers", "expected"),
    [
        ([], None),
        ([(b"Content-Length", b"2")], 2),
        ([(b"content-length", b"-1")], None),
        ([(b"content-length", b"bad")], None),
    ],
)
def test_tc_mid_006_content_length_parsing(
    headers: list[tuple[bytes, bytes]], expected: Optional[int]
) -> None:
    """TC-MID-006 长度头解析覆盖正常、负数、非法和缺失边界。"""

    middleware = BodyLimitMiddleware(lambda scope, receive, send: None, 3)  # type: ignore[arg-type]
    assert middleware._content_length(http_scope(headers)) == expected
