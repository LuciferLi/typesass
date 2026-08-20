"""PyInstaller 打包使用的 CodexMan FastAPI sidecar 入口。"""

import ipaddress
import io
import json
import logging
import os
import select
import sys
import time
from typing import BinaryIO

import uvicorn

from app.config import load_settings, set_model_catalog_bootstrap
from app.logging_config import configure_logging
from app.private_bridge import set_private_rpc_bootstrap


DEFAULT_SIDECAR_HOST = "127.0.0.1"
DEFAULT_SIDECAR_PORT = 18080
MAX_BOOTSTRAP_FRAME_BYTES = 1024 * 1024
BOOTSTRAP_READ_TIMEOUT_SECONDS = 5.0
BOOTSTRAP_READ_CHUNK_BYTES = 64 * 1024
logger = logging.getLogger("aitool.sidecar")


def _sidecar_host() -> str:
    """读取并校验 sidecar 监听地址。

    用途：保证桌面自动启动的 HTTP 服务只能绑定本机回环接口，避免应用端口意外暴露到局域网或公网。
    流程：读取 ``AITOOL_SIDECAR_HOST``，缺失时使用 127.0.0.1；localhost 直接接受，其余值按 IP 解析并检查 loopback。
    参数：无。
    返回：可交给 Uvicorn 的 localhost 或回环 IP 字符串。
    异常边界：空白、主机名、通配地址和非回环 IP 抛出 ``RuntimeError``，错误不包含模型或鉴权配置。
    """

    value = os.getenv("AITOOL_SIDECAR_HOST", DEFAULT_SIDECAR_HOST).strip()
    if value == "localhost":
        return value
    try:
        address = ipaddress.ip_address(value)
    except ValueError as error:
        raise RuntimeError("AITOOL_SIDECAR_HOST 必须是回环地址") from error
    if not address.is_loopback:
        raise RuntimeError("AITOOL_SIDECAR_HOST 必须是回环地址")
    return value


def _sidecar_port() -> int:
    """读取并校验 sidecar 固定监听端口。

    用途：允许桌面运行环境显式注入端口，同时拒绝无效、特权或超出 TCP 范围的值。
    流程：读取 ``AITOOL_SIDECAR_PORT``，缺失时使用 18080，转换十进制整数并限制在 1024 到 65535。
    参数：无。
    返回：可交给 Uvicorn 的非特权 TCP 端口。
    异常边界：非整数、空白、特权端口和越界值抛出 ``RuntimeError``；不自动漂移或探测其它端口。
    """

    raw_value = os.getenv("AITOOL_SIDECAR_PORT", str(DEFAULT_SIDECAR_PORT)).strip()
    try:
        value = int(raw_value)
    except ValueError as error:
        raise RuntimeError("AITOOL_SIDECAR_PORT 必须是有效端口") from error
    if value < 1024 or value > 65535:
        raise RuntimeError("AITOOL_SIDECAR_PORT 必须是 1024 到 65535")
    return value


def _read_bootstrap_frame(stream: BinaryIO) -> bytearray:
    """在固定截止时间内读取完整 sidecar stdin 帧。

    用途：避免父进程遗漏关闭 stdin 时 PyInstaller sidecar 永久阻塞在启动阶段。
    流程：生产 stdin 使用 macOS/POSIX 支持的 ``select`` 等待匿名管道可读，再用 ``os.read`` 分块读取到 EOF；内存流仅供单元测试同步读取。
    参数：``stream`` 为 Rust 独占传入的二进制 stdin，或测试使用的 ``io.BytesIO``。
    返回：最多包含 1 MiB 完整 JSON+LF 帧和一个越界探针字节的可清零缓冲区。
    异常边界：5 秒内未收到数据或未等到 EOF、stdin 无文件描述符、系统读取失败、帧总长越界时抛出不含输入正文的 ``RuntimeError``。
    """

    if isinstance(stream, io.BytesIO):
        return bytearray(stream.read(MAX_BOOTSTRAP_FRAME_BYTES + 1))
    try:
        file_descriptor = stream.fileno()
    except (AttributeError, OSError) as error:
        raise RuntimeError("sidecar stdin bootstrap 管道不可用") from error

    deadline = time.monotonic() + BOOTSTRAP_READ_TIMEOUT_SECONDS
    raw_frame = bytearray()
    while True:
        remaining_seconds = deadline - time.monotonic()
        if remaining_seconds <= 0:
            raw_frame.clear()
            raise RuntimeError("sidecar stdin bootstrap 读取超时")
        try:
            ready_streams, _, _ = select.select(
                [file_descriptor],
                [],
                [],
                remaining_seconds,
            )
        except InterruptedError:
            continue
        except (OSError, ValueError) as error:
            raw_frame.clear()
            raise RuntimeError("sidecar stdin bootstrap 管道读取失败") from error
        if not ready_streams:
            raw_frame.clear()
            raise RuntimeError("sidecar stdin bootstrap 读取超时")
        try:
            chunk = os.read(
                file_descriptor,
                min(
                    BOOTSTRAP_READ_CHUNK_BYTES,
                    MAX_BOOTSTRAP_FRAME_BYTES + 1 - len(raw_frame),
                ),
            )
        except OSError as error:
            raw_frame.clear()
            raise RuntimeError("sidecar stdin bootstrap 管道读取失败") from error
        if not chunk:
            return raw_frame
        raw_frame.extend(chunk)
        if len(raw_frame) > MAX_BOOTSTRAP_FRAME_BYTES:
            raw_frame.clear()
            raise RuntimeError("sidecar stdin bootstrap 超过 1 MiB 限制")


def _read_sidecar_bootstrap(stream: BinaryIO) -> tuple:
    """从 sidecar stdin 读取一次性私有模型目录。

    用途：接收 Rust 通过匿名管道传入的含 API Key bootstrap，避免模型密钥出现在可继承、可枚举的长期子进程环境。
    流程：在固定截止时间内读取到 Rust 关闭写端形成的 EOF，要求唯一 LF 终止符，再解析严格 ``modelCatalog`` envelope 并清空原始字节缓冲。
    参数：``stream`` 为 sidecar 启动时独占的二进制 stdin，Rust 写完后必须关闭管道以结束读取。
    返回：待 ``config.set_model_catalog_bootstrap`` 消费和校验的目录对象。
    异常边界：空输入、未以 LF 终止、多帧/额外字节、超限、超时、非 UTF-8、非法 JSON、非对象或额外字段均阻止启动；错误不包含输入正文。
    """

    raw_frame = _read_bootstrap_frame(stream)
    try:
        if not raw_frame:
            raise RuntimeError("sidecar stdin bootstrap 不能为空")
        if len(raw_frame) > MAX_BOOTSTRAP_FRAME_BYTES:
            raise RuntimeError("sidecar stdin bootstrap 超过 1 MiB 限制")
        if raw_frame[-1:] != b"\n" or raw_frame.count(b"\n") != 1:
            raise RuntimeError("sidecar stdin bootstrap 必须以单个换行结束")
        del raw_frame[-1]
        try:
            envelope = json.loads(raw_frame)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise RuntimeError("sidecar stdin bootstrap 必须是 UTF-8 JSON 对象") from error
        if not isinstance(envelope, dict) or set(envelope) != {"modelCatalog", "privateRpc"}:
            raise RuntimeError("sidecar stdin bootstrap 字段结构无效")
        return envelope["modelCatalog"], envelope["privateRpc"]
    finally:
        raw_frame.clear()


def run() -> int:
    """启动并托管 FastAPI sidecar 进程。

    用途：作为 PyInstaller 唯一入口，先完成可信配置与日志初始化，再以前台单 worker 方式运行 Uvicorn。
    流程：加载服务配置、启用结构化日志、校验回环 host/port，记录安全启动元数据后调用 ``uvicorn.run``，退出时记录状态。
    参数：无；Rust 通过子进程环境注入非模型启动配置，并通过标准输入一次性注入模型目录。
    返回：正常退出返回 0；参数校验或 Uvicorn 启动异常返回 1，供 Rust 判断 sidecar 启动失败。
    异常边界：不捕获或记录配置原值、密钥和请求正文；失败日志只包含异常类型，端口冲突不杀进程也不自动换端口。
    """

    try:
        os.environ.pop("AITOOL_MODEL_CATALOG_JSON", None)
        model_catalog, private_rpc = _read_sidecar_bootstrap(sys.stdin.buffer)
        set_model_catalog_bootstrap(model_catalog)
        set_private_rpc_bootstrap(private_rpc)
        del model_catalog
        del private_rpc
        settings = load_settings()
        configure_logging(settings)
        host = _sidecar_host()
        port = _sidecar_port()
        logger.info("sidecar_starting", extra={"context": {"host": host, "port": port}})
        uvicorn.run(
            "app.main:app",
            host=host,
            port=port,
            workers=1,
            log_config=None,
            access_log=False,
        )
        logger.info("sidecar_stopped", extra={"context": {"exitCode": 0}})
        return 0
    except SystemExit as error:
        logger.error(
            "sidecar_start_failed",
            extra={"context": {"errorType": type(error).__name__}},
        )
        return 1
    except Exception as error:
        logger.error(
            "sidecar_start_failed",
            extra={"context": {"errorType": type(error).__name__}},
        )
        return 1


if __name__ == "__main__":  # pragma: no cover - PyInstaller 通过该分支进入已完整单测的 run。
    raise SystemExit(run())
