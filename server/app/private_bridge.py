"""Rust 私有 Unix Domain Socket RPC 桥接。"""

import asyncio
from contextlib import suppress
from dataclasses import dataclass
import hmac
import json
import logging
import os
import struct
from typing import Dict, Optional

from .errors import ApiError


PRIVATE_RPC_REQUEST_MAX_BYTES = 1024 * 1024
PRIVATE_RPC_RESPONSE_MAX_BYTES = 8 * 1024 * 1024
PRIVATE_RPC_TIMEOUT_SECONDS = 70.0
_PRIVATE_RPC_BOOTSTRAP: Optional[object] = None
logger = logging.getLogger("aitool.private_rpc")


def _protocol_error(
    stage: str,
    method: str,
    request_id: str,
    declared_response_bytes: int,
    received_response_bytes: int,
) -> ApiError:
    """记录脱敏协议失败阶段并构造统一公开错误。

    用途：区分响应长度头、正文、大小、JSON 和 envelope 故障，使 requestId 可直接定位协议断点。
    流程：只记录固定阶段、allowlist 方法名、requestId 与字节计数，再返回不含内部细节的 502 ``ApiError``。
    参数：``stage`` 为固定诊断阶段；``method/request_id`` 为安全路由元数据；两个字节数未知时传 -1。
    返回：供调用位置抛出的统一协议错误。
    异常边界：禁止传入或记录响应正文、socket 路径、secret、业务参数、错误 message 或会话内容。
    """

    logger.warning(
        "private_rpc_protocol_error",
        extra={
            "context": {
                "stage": stage,
                "method": method,
                "requestId": request_id,
                "declaredResponseBytes": declared_response_bytes,
                "receivedResponseBytes": received_response_bytes,
            }
        },
    )
    return ApiError(502, "PRIVATE_SERVICE_PROTOCOL_ERROR", "桌面业务服务返回无效响应。")


@dataclass(frozen=True)
class PrivateRpcConfig:
    """私有 RPC 运行配置。

    用途：仅在 sidecar 进程内保存 Rust socket 地址和单次启动密钥。
    字段：``socket_path`` 为绝对 UDS 路径；``secret`` 为高熵共享密钥。
    边界：本类型不得序列化到 HTTP、OpenAPI、日志或异常消息。
    """

    socket_path: str
    secret: str


def set_private_rpc_bootstrap(payload: object) -> None:
    """暂存 Rust 经 stdin 注入的私有 RPC 配置。

    流程：只保存原始对象，应用 lifespan 消费时再严格校验并清空引用。
    参数：``payload`` 为 bootstrap 的 ``privateRpc`` 字段。
    返回：无。
    异常边界：不记录、复制或回显 socket 与 secret。
    """

    global _PRIVATE_RPC_BOOTSTRAP
    _PRIVATE_RPC_BOOTSTRAP = payload


def consume_private_rpc_bootstrap() -> Optional[PrivateRpcConfig]:
    """消费并严格校验一次性私有 RPC 配置。

    流程：先清空全局原始引用，再校验精确字段、绝对 socket 路径和密钥强度。
    返回：有效配置；直接 Uvicorn 启动且没有 bootstrap 时返回 ``None``。
    异常边界：非法配置阻止 sidecar 启动，错误不包含配置原值。
    """

    global _PRIVATE_RPC_BOOTSTRAP
    payload = _PRIVATE_RPC_BOOTSTRAP
    _PRIVATE_RPC_BOOTSTRAP = None
    if payload is None:
        return None
    if not isinstance(payload, dict) or set(payload) != {"socketPath", "secret"}:
        raise RuntimeError("sidecar bootstrap privateRpc 字段结构无效")
    socket_path = payload["socketPath"]
    secret = payload["secret"]
    if (
        not isinstance(socket_path, str)
        or not os.path.isabs(socket_path)
        or "\x00" in socket_path
        or len(socket_path.encode("utf-8")) > 1024
    ):
        raise RuntimeError("sidecar bootstrap privateRpc socketPath 无效")
    if not isinstance(secret, str) or len(secret) < 32 or len(secret) > 512:
        raise RuntimeError("sidecar bootstrap privateRpc secret 无效")
    return PrivateRpcConfig(socket_path=socket_path, secret=secret)


class PrivateRpcClient:
    """通过有界长度帧调用 Rust 单一业务服务。

    用途：FastAPI 只承担公开 HTTP 网关职责，不访问任务 SQLite、不启动 Codex、不复制状态机。
    流程：每次调用新建 UDS 连接，发送 4 字节大端长度与 JSON envelope，读取并校验一帧响应后关闭。
    边界：请求 1 MiB、响应 8 MiB、整体 10 秒；内部地址、密钥和协议正文不进入公开错误。
    """

    def __init__(self, config: Optional[PrivateRpcConfig]) -> None:
        """初始化私有桥接客户端，不执行连接或复制配置到环境变量。"""

        self._config = config

    def verify_secret(self, secret: str) -> bool:
        """校验调用方是否持有当前 sidecar 启动代私有密钥。

        用途：为 Rust -> sidecar 的内部控制接口复用一次性 bootstrap 密钥，避免公开 HTTP Origin 规则放行敏感操作。
        流程：仅在存在私有 RPC 配置时使用常量时间比较，调用方只得到布尔结果。
        参数：``secret`` 为 HTTP 内部控制 Header 提供的密钥。
        返回：密钥是否匹配当前启动代。
        异常边界：不记录、不回显、不派生密钥；未 bootstrap 的开发直启 sidecar 默认拒绝内部控制调用。
        """

        if self._config is None:
            return False
        return hmac.compare_digest(secret, self._config.secret)

    async def call(
        self, method: str, request_id: str, params: Dict[str, object]
    ) -> object:
        """调用一个 Rust 私有业务方法。

        流程：序列化严格 envelope，校验请求长度，在统一 deadline 内连接、写入、读取和解析响应。
        参数：``method`` 为内部固定方法名；``request_id`` 为公开请求追踪 ID；``params`` 为已校验字段。
        返回：Rust ``result`` JSON 值。
        异常边界：不可达映射 503，协议/超限映射 502，超时映射 504；业务错误保留稳定 code。
        """

        if self._config is None:
            raise ApiError(503, "PRIVATE_SERVICE_UNAVAILABLE", "桌面业务服务尚未就绪。")
        request_bytes = json.dumps(
            {
                "secret": self._config.secret,
                "method": method,
                "requestId": request_id,
                "params": params,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        ).encode("utf-8")
        if len(request_bytes) > PRIVATE_RPC_REQUEST_MAX_BYTES:
            raise ApiError(
                413, "PRIVATE_REQUEST_TOO_LARGE", "业务请求超过内部转发限制。"
            )
        try:
            return await asyncio.wait_for(
                self._exchange(request_bytes, method, request_id),
                timeout=PRIVATE_RPC_TIMEOUT_SECONDS,
            )
        except asyncio.TimeoutError as error:
            raise ApiError(
                504, "PRIVATE_SERVICE_TIMEOUT", "桌面业务服务响应超时。"
            ) from error
        except (
            FileNotFoundError,
            ConnectionRefusedError,
            ConnectionResetError,
            BrokenPipeError,
            OSError,
        ) as error:
            raise ApiError(
                503, "PRIVATE_SERVICE_UNAVAILABLE", "桌面业务服务暂不可用。"
            ) from error

    async def _exchange(
        self, request_bytes: bytes, method: str, request_id: str
    ) -> object:
        """执行一次 UDS 长度帧交换并校验响应。

        参数：``request_bytes`` 为已完成大小检查的 UTF-8 JSON；``method/request_id`` 仅用于脱敏阶段日志。
        返回：成功响应中的 JSON result。
        异常边界：截断、超限、非法 JSON/envelope 及除 RPC_BUSY 外的内部 RPC 错误统一映射为脱敏 502；
        RPC_BUSY 过载映射为可退避 503；进程探测内部码归一为 500 ``CODEX_CONNECTION_STATE_FAILED``，
        其余稳定业务错误按公开状态映射，内部协议正文不进入 HTTP 或日志。
        """

        assert self._config is not None
        reader, writer = await asyncio.open_unix_connection(self._config.socket_path)
        try:
            writer.write(struct.pack(">I", len(request_bytes)) + request_bytes)
            await writer.drain()
            try:
                response_header = await reader.readexactly(4)
            except asyncio.IncompleteReadError as error:
                raise _protocol_error(
                    "response_header_truncated",
                    method,
                    request_id,
                    4,
                    len(error.partial),
                ) from error
            response_size = struct.unpack(">I", response_header)[0]
            if response_size > PRIVATE_RPC_RESPONSE_MAX_BYTES:
                raise _protocol_error(
                    "response_size_exceeded",
                    method,
                    request_id,
                    response_size,
                    0,
                )
            try:
                response_bytes = await reader.readexactly(response_size)
            except asyncio.IncompleteReadError as error:
                raise _protocol_error(
                    "response_body_truncated",
                    method,
                    request_id,
                    response_size,
                    len(error.partial),
                ) from error
        finally:
            writer.close()
            with suppress(OSError):
                await writer.wait_closed()
        try:
            response = json.loads(response_bytes)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise _protocol_error(
                "response_json_invalid",
                method,
                request_id,
                response_size,
                len(response_bytes),
            ) from error
        if not isinstance(response, dict) or response.get("ok") not in (True, False):
            raise _protocol_error(
                "response_envelope_invalid",
                method,
                request_id,
                response_size,
                len(response_bytes),
            )
        if response["ok"] is True:
            if set(response) != {"ok", "result"}:
                raise _protocol_error(
                    "response_success_envelope_invalid",
                    method,
                    request_id,
                    response_size,
                    len(response_bytes),
                )
            return response["result"]
        error_payload = response.get("error")
        if set(response) != {"ok", "error"} or not isinstance(error_payload, dict):
            raise _protocol_error(
                "response_error_envelope_invalid",
                method,
                request_id,
                response_size,
                len(response_bytes),
            )
        code = error_payload.get("code")
        message = error_payload.get("message")
        if not isinstance(code, str) or not isinstance(message, str):
            raise _protocol_error(
                "response_error_fields_invalid",
                method,
                request_id,
                response_size,
                len(response_bytes),
            )
        # 进程探测属于连接状态计算细节，HTTP 边界只暴露统一连接状态错误。
        if code == "CODEX_PROCESS_CHECK_FAILED":
            code = "CODEX_CONNECTION_STATE_FAILED"
        if code.startswith("RPC_") and code != "RPC_BUSY":
            raise _protocol_error(
                "response_internal_error",
                method,
                request_id,
                response_size,
                len(response_bytes),
            )
        status_code = 409
        if code in {
            "TASK_PROJECT_NAME_TOO_LONG",
            "TASK_PROMPT_REQUIRED",
            "TASK_PROMPT_TOO_LONG",
            "TASK_TITLE_REQUIRED",
            "TASK_TITLE_TOO_LONG",
        } or code.startswith("INVALID_"):
            status_code = 400
        elif code.endswith("_NOT_FOUND"):
            status_code = 404
        elif code in {
            "TASK_STORE_UNAVAILABLE",
            "CODEX_DESKTOP_NOT_CONNECTED",
            "CODEX_UNAVAILABLE",
            "RPC_BUSY",
        }:
            status_code = 503
        elif code == "CODEX_PLATFORM_UNSUPPORTED":
            status_code = 501
        elif code in {
            "CODEX_CONNECTION_STATE_FAILED",
            "TASK_PROJECT_CAPACITY_INVALID",
            "TASK_WORKSPACE_RESPONSE_TOO_LARGE",
            "TASK_WORKSPACE_SERIALIZATION_FAILED",
        }:
            status_code = 500
        raise ApiError(status_code, code[:128], message[:500])
