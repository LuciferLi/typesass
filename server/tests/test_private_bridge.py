"""Rust 私有 UDS RPC 长度帧、安全配置与错误映射测试。"""

import asyncio
import json
import os
from pathlib import Path
import struct
from typing import Callable, Optional

import pytest

from app.errors import ApiError
from app import private_bridge
from app.private_bridge import PrivateRpcClient, PrivateRpcConfig


def test_tc_bridge_001_bootstrap_validation() -> None:
    """私有配置只接受精确字段、绝对 UDS 路径和 32 到 512 字符密钥，且消费后清空。"""

    private_bridge.set_private_rpc_bootstrap(
        {"socketPath": "/tmp/aitool.sock", "secret": "s" * 32}
    )
    assert private_bridge.consume_private_rpc_bootstrap() == PrivateRpcConfig(
        "/tmp/aitool.sock", "s" * 32
    )
    assert private_bridge.consume_private_rpc_bootstrap() is None


@pytest.mark.parametrize(
    "payload",
    [
        [],
        {"socketPath": "/tmp/x", "secret": "s" * 32, "extra": True},
        {"socketPath": "relative", "secret": "s" * 32},
        {"socketPath": "/tmp/\x00x", "secret": "s" * 32},
        {"socketPath": "/" + "x" * 1025, "secret": "s" * 32},
        {"socketPath": "/tmp/x", "secret": "short"},
        {"socketPath": "/tmp/x", "secret": "s" * 513},
    ],
)
def test_tc_bridge_002_bootstrap_rejects_invalid_values(payload: object) -> None:
    """私有配置任一结构、安全路径或密钥边界非法时阻止启动且不回显原值。"""

    private_bridge.set_private_rpc_bootstrap(payload)
    with pytest.raises(RuntimeError, match="privateRpc"):
        private_bridge.consume_private_rpc_bootstrap()


async def _run_rpc_server(
    socket_path: Path, response_factory: Callable[[dict], bytes]
) -> asyncio.AbstractServer:
    """启动测试专用 UDS 服务，按生产长度帧读取请求并写入指定响应。"""

    async def handler(
        reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        """读取并验证单请求帧，再返回测试响应帧。"""

        request_size = struct.unpack(">I", await reader.readexactly(4))[0]
        request = json.loads(await reader.readexactly(request_size))
        response = response_factory(request)
        writer.write(struct.pack(">I", len(response)) + response)
        await writer.drain()
        writer.close()
        await writer.wait_closed()

    return await asyncio.start_unix_server(handler, str(socket_path))


@pytest.mark.asyncio
async def test_tc_bridge_003_success_framing_and_auth(tmp_path: Path) -> None:
    """客户端使用大端长度帧并携带 secret/method/requestId/params，成功只返回 result。"""

    observed = []

    def response_factory(request: dict) -> bytes:
        """记录内部 envelope 并返回成功 JSON。"""

        observed.append(request)
        return b'{"ok":true,"result":{"value":1}}'

    socket_path = Path(
        "/tmp/aitool-rpc-{0}-{1}.sock".format(os.getpid(), id(response_factory))
    )
    server = await _run_rpc_server(socket_path, response_factory)
    try:
        client = PrivateRpcClient(PrivateRpcConfig(str(socket_path), "s" * 32))
        assert await client.call("listCodexWorkspaces", "request-1", {"limit": 1}) == {
            "value": 1
        }
    finally:
        server.close()
        await server.wait_closed()
    assert observed == [
        {
            "secret": "s" * 32,
            "method": "listCodexWorkspaces",
            "requestId": "request-1",
            "params": {"limit": 1},
        }
    ]


@pytest.mark.asyncio
async def test_tc_bridge_004_unavailable_timeout_and_request_limit(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """未配置、连接失败、整体超时与请求超限分别映射稳定 503/504/413。"""

    with pytest.raises(ApiError) as missing:
        await PrivateRpcClient(None).call("method", "request", {})
    assert (missing.value.status_code, missing.value.code) == (
        503,
        "PRIVATE_SERVICE_UNAVAILABLE",
    )

    unavailable_client = PrivateRpcClient(
        PrivateRpcConfig("/tmp/does-not-exist-aitool.sock", "s" * 32)
    )
    with pytest.raises(ApiError) as unavailable:
        await unavailable_client.call("method", "request", {})
    assert (unavailable.value.status_code, unavailable.value.code) == (
        503,
        "PRIVATE_SERVICE_UNAVAILABLE",
    )

    async def timeout_exchange(
        request_bytes: bytes, method: str, request_id: str
    ) -> object:
        """超过测试 deadline 的内部交换。"""

        await asyncio.sleep(0.02)
        return None

    monkeypatch.setattr(private_bridge, "PRIVATE_RPC_TIMEOUT_SECONDS", 0.001)
    monkeypatch.setattr(unavailable_client, "_exchange", timeout_exchange)
    with pytest.raises(ApiError) as timeout:
        await unavailable_client.call("method", "request", {})
    assert (timeout.value.status_code, timeout.value.code) == (
        504,
        "PRIVATE_SERVICE_TIMEOUT",
    )

    monkeypatch.setattr(private_bridge, "PRIVATE_RPC_REQUEST_MAX_BYTES", 1)
    with pytest.raises(ApiError) as oversized:
        await unavailable_client.call("method", "request", {})
    assert (oversized.value.status_code, oversized.value.code) == (
        413,
        "PRIVATE_REQUEST_TOO_LARGE",
    )


@pytest.mark.parametrize(
    ("response", "status", "code", "protocol_stage"),
    [
        (b"not-json", 502, "PRIVATE_SERVICE_PROTOCOL_ERROR", "response_json_invalid"),
        (b"[]", 502, "PRIVATE_SERVICE_PROTOCOL_ERROR", "response_envelope_invalid"),
        (
            b'{"ok":true,"result":null,"extra":1}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_success_envelope_invalid",
        ),
        (
            b'{"ok":false}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_error_envelope_invalid",
        ),
        (
            b'{"ok":false,"error":{"code":1,"message":"sensitive-response-text"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_error_fields_invalid",
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_NOT_FOUND","message":"missing"}}',
            404,
            "TASK_NOT_FOUND",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"CODEX_THREAD_NOT_FOUND","message":"missing"}}',
            404,
            "CODEX_THREAD_NOT_FOUND",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"INVALID_TASK","message":"bad"}}',
            400,
            "INVALID_TASK",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_INVALID_PARAMS","message":"sensitive-params"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_INVALID_REQUEST","message":"sensitive-request"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_PROJECT_NAME_TOO_LONG","message":"bad"}}',
            400,
            "TASK_PROJECT_NAME_TOO_LONG",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_PROMPT_REQUIRED","message":"bad"}}',
            400,
            "TASK_PROMPT_REQUIRED",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_PROMPT_TOO_LONG","message":"bad"}}',
            400,
            "TASK_PROMPT_TOO_LONG",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_TITLE_REQUIRED","message":"bad"}}',
            400,
            "TASK_TITLE_REQUIRED",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_TITLE_TOO_LONG","message":"bad"}}',
            400,
            "TASK_TITLE_TOO_LONG",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_UNAUTHORIZED","message":"sensitive-auth"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_METHOD_NOT_ALLOWED","message":"sensitive-method"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_RESPONSE_TOO_LARGE","message":"sensitive-size"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_REQUEST_TOO_LARGE","message":"sensitive-request-size"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_SERIALIZATION_FAILED","message":"sensitive-serialization"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_INTERNAL_ERROR","message":"sensitive-internal"}}',
            502,
            "PRIVATE_SERVICE_PROTOCOL_ERROR",
            "response_internal_error",
        ),
        (
            b'{"ok":false,"error":{"code":"RPC_BUSY","message":"busy"}}',
            503,
            "RPC_BUSY",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_STORE_UNAVAILABLE","message":"down"}}',
            503,
            "TASK_STORE_UNAVAILABLE",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"CODEX_DESKTOP_NOT_CONNECTED","message":"disconnected"}}',
            503,
            "CODEX_DESKTOP_NOT_CONNECTED",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"CODEX_PLATFORM_UNSUPPORTED","message":"unsupported"}}',
            501,
            "CODEX_PLATFORM_UNSUPPORTED",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"CODEX_CONNECTION_STATE_FAILED","message":"state"}}',
            500,
            "CODEX_CONNECTION_STATE_FAILED",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"CODEX_PROCESS_CHECK_FAILED","message":"process"}}',
            500,
            "CODEX_CONNECTION_STATE_FAILED",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_PROJECT_CAPACITY_INVALID","message":"bad"}}',
            500,
            "TASK_PROJECT_CAPACITY_INVALID",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_PROJECT_TASK_CAPACITY_INVALID","message":"bad"}}',
            500,
            "TASK_PROJECT_TASK_CAPACITY_INVALID",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_PROJECT_SESSION_CAPACITY_INVALID","message":"bad"}}',
            500,
            "TASK_PROJECT_SESSION_CAPACITY_INVALID",
            None,
        ),
        (
            b'{"ok":false,"error":{"code":"TASK_STATE_CONFLICT","message":"conflict"}}',
            409,
            "TASK_STATE_CONFLICT",
            None,
        ),
    ],
)
@pytest.mark.asyncio
async def test_tc_bridge_005_protocol_and_business_error_mapping(
    tmp_path: Path,
    response: bytes,
    status: int,
    code: str,
    protocol_stage: Optional[str],
    caplog: pytest.LogCaptureFixture,
) -> None:
    """验证私有 RPC 响应校验、公开业务错误映射与旧连接错误归一化。

    流程：通过真实 UDS 长度帧返回参数化错误 envelope，调用桥接客户端并核对公开 HTTP
    状态码和错误码；协议错误额外验证脱敏日志。旧 CODEX_PROCESS_CHECK_FAILED 即使由旧桌面端
    返回，也必须收敛为 500 CODEX_CONNECTION_STATE_FAILED，禁止泄露内部进程探测语义。
    """

    def response_factory(request: dict) -> bytes:
        """返回当前参数化响应。"""

        return response

    socket_path = Path(
        "/tmp/aitool-rpc-{0}-{1}.sock".format(os.getpid(), id(response_factory))
    )
    server = await _run_rpc_server(socket_path, response_factory)
    try:
        client = PrivateRpcClient(PrivateRpcConfig(str(socket_path), "s" * 32))
        with pytest.raises(ApiError) as captured:
            await client.call("method", "request", {})
    finally:
        server.close()
        await server.wait_closed()
    assert (captured.value.status_code, captured.value.code) == (status, code)
    if protocol_stage == "response_internal_error":
        internal_error = json.loads(response)["error"]
        assert internal_error["code"] not in str(captured.value)
        assert internal_error["message"] not in str(captured.value)
        assert internal_error["code"] not in caplog.text
        assert internal_error["message"] not in caplog.text
    protocol_records = [
        record
        for record in caplog.records
        if record.message == "private_rpc_protocol_error"
    ]
    if protocol_stage is None:
        assert protocol_records == []
    else:
        assert len(protocol_records) == 1
        assert protocol_records[0].context == {
            "stage": protocol_stage,
            "method": "method",
            "requestId": "request",
            "declaredResponseBytes": len(response),
            "receivedResponseBytes": len(response),
        }
        assert "sensitive-response-text" not in caplog.text
        assert str(socket_path) not in caplog.text
        assert "s" * 32 not in caplog.text


@pytest.mark.asyncio
async def test_tc_bridge_006_truncated_and_oversized_response(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    caplog: pytest.LogCaptureFixture,
) -> None:
    """响应头或正文截断及声明超过 8 MiB 时返回协议错误，不进行无界读取。"""

    socket_path = Path("/tmp/aitool-rpc-{0}-truncated.sock".format(os.getpid()))

    async def truncated_handler(
        reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        """读取请求后只写入不完整响应头。"""

        size = struct.unpack(">I", await reader.readexactly(4))[0]
        await reader.readexactly(size)
        writer.write(b"\x00\x01")
        await writer.drain()
        writer.close()

    server = await asyncio.start_unix_server(truncated_handler, str(socket_path))
    client = PrivateRpcClient(PrivateRpcConfig(str(socket_path), "s" * 32))
    try:
        with pytest.raises(ApiError) as truncated:
            await client.call("method", "request", {})
    finally:
        server.close()
        await server.wait_closed()
    assert truncated.value.code == "PRIVATE_SERVICE_PROTOCOL_ERROR"
    header_record = [
        record
        for record in caplog.records
        if record.message == "private_rpc_protocol_error"
    ][-1]
    assert header_record.context == {
        "stage": "response_header_truncated",
        "method": "method",
        "requestId": "request",
        "declaredResponseBytes": 4,
        "receivedResponseBytes": 2,
    }

    async def body_truncated_handler(
        reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        """读取请求后声明四字节响应正文但只写一个字节。"""

        size = struct.unpack(">I", await reader.readexactly(4))[0]
        await reader.readexactly(size)
        writer.write(struct.pack(">I", 4) + b"{")
        await writer.drain()
        writer.close()

    server = await asyncio.start_unix_server(body_truncated_handler, str(socket_path))
    try:
        with pytest.raises(ApiError) as body_truncated:
            await client.call("method", "request", {})
    finally:
        server.close()
        await server.wait_closed()
    assert body_truncated.value.code == "PRIVATE_SERVICE_PROTOCOL_ERROR"
    body_record = [
        record
        for record in caplog.records
        if record.message == "private_rpc_protocol_error"
    ][-1]
    assert body_record.context == {
        "stage": "response_body_truncated",
        "method": "method",
        "requestId": "request",
        "declaredResponseBytes": 4,
        "receivedResponseBytes": 1,
    }

    monkeypatch.setattr(private_bridge, "PRIVATE_RPC_RESPONSE_MAX_BYTES", 1)
    server = await _run_rpc_server(socket_path, lambda request: b"{}")
    try:
        with pytest.raises(ApiError) as oversized:
            await client.call("method", "request", {})
    finally:
        server.close()
        await server.wait_closed()
    assert oversized.value.code == "PRIVATE_SERVICE_PROTOCOL_ERROR"
    size_record = [
        record
        for record in caplog.records
        if record.message == "private_rpc_protocol_error"
    ][-1]
    assert size_record.context == {
        "stage": "response_size_exceeded",
        "method": "method",
        "requestId": "request",
        "declaredResponseBytes": 2,
        "receivedResponseBytes": 0,
    }
