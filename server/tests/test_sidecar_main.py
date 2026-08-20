"""PyInstaller sidecar 入口的监听边界与启动编排测试。"""

import io
import os
import time
from types import SimpleNamespace

import pytest

import sidecar_main


@pytest.mark.parametrize("value", ["localhost", "127.0.0.1", "127.0.0.2", "::1"])
def test_tc_sidecar_001_loopback_hosts(
    monkeypatch: pytest.MonkeyPatch, value: str
) -> None:
    """TC-SIDECAR-001 监听地址只接受 localhost 和 IPv4/IPv6 回环地址。

    参数：``monkeypatch`` 隔离环境，``value`` 为待验证回环地址。
    返回：无；通过返回值断言 Uvicorn 将收到原始合法地址。
    异常边界：非回环和主机名由 TC-SIDECAR-002 覆盖。
    """

    monkeypatch.setenv("AITOOL_SIDECAR_HOST", value)
    assert sidecar_main._sidecar_host() == value


@pytest.mark.parametrize("value", ["", "0.0.0.0", "192.168.1.2", "example.test"])
def test_tc_sidecar_002_non_loopback_hosts_rejected(
    monkeypatch: pytest.MonkeyPatch, value: str
) -> None:
    """TC-SIDECAR-002 空白、通配、局域网和普通主机名均不能绑定 sidecar。

    参数：``monkeypatch`` 隔离环境，``value`` 为不可信监听值。
    返回：无；以 ``RuntimeError`` 证明服务不会在非回环网络启动。
    异常边界：错误只描述配置名，不执行 DNS 解析或端口探测。
    """

    monkeypatch.setenv("AITOOL_SIDECAR_HOST", value)
    with pytest.raises(RuntimeError, match="必须是回环地址"):
        sidecar_main._sidecar_host()


@pytest.mark.parametrize("raw_value", ["", "bad", "0", "1023", "65536"])
def test_tc_sidecar_003_invalid_ports_rejected(
    monkeypatch: pytest.MonkeyPatch, raw_value: str
) -> None:
    """TC-SIDECAR-003 非整数、空白、特权和越界端口均阻止启动。

    参数：``monkeypatch`` 隔离环境，``raw_value`` 为待验证端口文本。
    返回：无；以 ``RuntimeError`` 证明入口不会自动换端口或使用危险端口。
    异常边界：错误不包含其它环境配置，合法默认与覆盖值由 TC-SIDECAR-004 覆盖。
    """

    monkeypatch.setenv("AITOOL_SIDECAR_PORT", raw_value)
    with pytest.raises(RuntimeError, match="必须"):
        sidecar_main._sidecar_port()


def test_tc_sidecar_004_default_and_custom_port(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-SIDECAR-004 端口缺失使用固定 18080，合法覆盖值按原值返回。

    参数：``monkeypatch`` 隔离当前进程环境。
    返回：无；通过两个返回值断言固定默认和显式注入行为。
    异常边界：不会检测端口占用，绑定冲突由 Uvicorn 返回给 run。
    """

    monkeypatch.delenv("AITOOL_SIDECAR_PORT", raising=False)
    assert sidecar_main._sidecar_port() == 18080
    monkeypatch.setenv("AITOOL_SIDECAR_PORT", "19090")
    assert sidecar_main._sidecar_port() == 19090


def test_tc_sidecar_005_run_success(monkeypatch: pytest.MonkeyPatch) -> None:
    """TC-SIDECAR-005 入口按单 worker、无访问日志参数启动应用并正常退出。

    参数：``monkeypatch`` 替换配置、日志和 Uvicorn，避免测试真实监听端口。
    返回：无；断言启动返回码、日志初始化和 Uvicorn 参数。
    异常边界：本用例只覆盖正常返回，启动异常由 TC-SIDECAR-006 覆盖。
    """

    settings = SimpleNamespace()
    configured = []
    calls = []
    bootstraps = []
    monkeypatch.setenv("AITOOL_MODEL_CATALOG_JSON", "legacy-private-key")
    monkeypatch.setattr(
        sidecar_main,
        "_read_sidecar_bootstrap",
        lambda stream: (
            ["catalog"],
            {"socketPath": "/tmp/fake.sock", "secret": "x" * 32},
        ),
    )
    monkeypatch.setattr(
        sidecar_main,
        "set_model_catalog_bootstrap",
        lambda value: bootstraps.append(value),
    )
    monkeypatch.setattr(
        sidecar_main,
        "set_private_rpc_bootstrap",
        lambda value: bootstraps.append(value),
    )
    monkeypatch.setattr(sidecar_main, "load_settings", lambda: settings)
    monkeypatch.setattr(
        sidecar_main, "configure_logging", lambda value: configured.append(value)
    )
    monkeypatch.setattr(sidecar_main, "_sidecar_host", lambda: "127.0.0.1")
    monkeypatch.setattr(sidecar_main, "_sidecar_port", lambda: 18080)
    monkeypatch.setattr(
        sidecar_main.uvicorn,
        "run",
        lambda *args, **kwargs: calls.append((args, kwargs)),
    )
    assert sidecar_main.run() == 0
    assert "AITOOL_MODEL_CATALOG_JSON" not in sidecar_main.os.environ
    assert bootstraps == [
        ["catalog"],
        {"socketPath": "/tmp/fake.sock", "secret": "x" * 32},
    ]
    assert configured == [settings]
    assert calls == [
        (
            ("app.main:app",),
            {
                "host": "127.0.0.1",
                "port": 18080,
                "workers": 1,
                "log_config": None,
                "access_log": False,
            },
        )
    ]


def test_tc_sidecar_006_run_failure_is_sanitized(
    monkeypatch: pytest.MonkeyPatch, caplog: pytest.LogCaptureFixture
) -> None:
    """TC-SIDECAR-006 Uvicorn 启动异常返回失败码且日志不包含异常正文。

    参数：``monkeypatch`` 注入含敏感样例的异常，``caplog`` 捕获 sidecar 日志。
    返回：无；断言非零退出、稳定事件名和仅异常类型的安全上下文。
    异常边界：不记录异常消息或堆栈，避免第三方库把环境值拼进异常后泄漏。
    """

    monkeypatch.setattr(sidecar_main, "load_settings", lambda: SimpleNamespace())
    monkeypatch.setattr(
        sidecar_main, "_read_sidecar_bootstrap", lambda stream: ([], {})
    )
    monkeypatch.setattr(sidecar_main, "set_model_catalog_bootstrap", lambda value: None)
    monkeypatch.setattr(sidecar_main, "set_private_rpc_bootstrap", lambda value: None)
    monkeypatch.setattr(sidecar_main, "configure_logging", lambda settings: None)
    monkeypatch.setattr(sidecar_main, "_sidecar_host", lambda: "127.0.0.1")
    monkeypatch.setattr(sidecar_main, "_sidecar_port", lambda: 18080)

    def fail_run(*args: object, **kwargs: object) -> None:
        """模拟包含敏感正文的启动失败；参数只用于兼容 Uvicorn 调用签名。"""

        raise RuntimeError("fake-private-api-key")

    monkeypatch.setattr(sidecar_main.uvicorn, "run", fail_run)
    with caplog.at_level("ERROR", logger="aitool.sidecar"):
        assert sidecar_main.run() == 1
    assert caplog.records[-1].message == "sidecar_start_failed"
    assert caplog.records[-1].context == {"errorType": "RuntimeError"}
    assert "fake-private-api-key" not in caplog.text


def test_tc_sidecar_007_system_exit_is_failure(
    monkeypatch: pytest.MonkeyPatch, caplog: pytest.LogCaptureFixture
) -> None:
    """TC-SIDECAR-007 Uvicorn 以 SystemExit 报告绑定失败时仍转换为可诊断失败码。

    参数：``monkeypatch`` 模拟 Uvicorn 进程级退出，``caplog`` 捕获安全事件。
    返回：无；断言入口返回 1 且记录 SystemExit 类型。
    异常边界：不传播 ``SystemExit`` 到测试调用方，也不记录其可能携带的第三方正文。
    """

    monkeypatch.setattr(sidecar_main, "load_settings", lambda: SimpleNamespace())
    monkeypatch.setattr(
        sidecar_main, "_read_sidecar_bootstrap", lambda stream: ([], {})
    )
    monkeypatch.setattr(sidecar_main, "set_model_catalog_bootstrap", lambda value: None)
    monkeypatch.setattr(sidecar_main, "set_private_rpc_bootstrap", lambda value: None)
    monkeypatch.setattr(sidecar_main, "configure_logging", lambda settings: None)
    monkeypatch.setattr(sidecar_main, "_sidecar_host", lambda: "127.0.0.1")
    monkeypatch.setattr(sidecar_main, "_sidecar_port", lambda: 18080)

    def exit_run(*args: object, **kwargs: object) -> None:
        """模拟 Uvicorn 在端口绑定失败时触发的进程级退出。"""

        raise SystemExit("fake-sensitive-bind-detail")

    monkeypatch.setattr(sidecar_main.uvicorn, "run", exit_run)
    with caplog.at_level("ERROR", logger="aitool.sidecar"):
        assert sidecar_main.run() == 1
    assert caplog.records[-1].context == {"errorType": "SystemExit"}
    assert "fake-sensitive-bind-detail" not in caplog.text


def test_tc_sidecar_008_stdin_bootstrap_success() -> None:
    """TC-SIDECAR-008 stdin bootstrap 只接受严格 modelCatalog envelope。

    参数：无；使用内存二进制流模拟 Rust 写入并关闭匿名管道。
    返回：无；断言目录对象保持结构供配置层继续做字段和安全校验。
    异常边界：解析器只拆 envelope，不在此处复制、日志记录或公开 apiKey。
    """

    payload = (
        b'{"modelCatalog":[{"id":"opaque","apiKey":"fake-private"}],'
        b'"privateRpc":{"socketPath":"/tmp/fake.sock","secret":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}}\n'
    )
    assert sidecar_main._read_sidecar_bootstrap(io.BytesIO(payload)) == (
        [{"id": "opaque", "apiKey": "fake-private"}],
        {"socketPath": "/tmp/fake.sock", "secret": "x" * 32},
    )


@pytest.mark.parametrize(
    ("payload", "message"),
    [
        (b"", "不能为空"),
        (b"not-json\n", "必须是 UTF-8 JSON 对象"),
        (b"\xff\n", "必须是 UTF-8 JSON 对象"),
        (b"[]\n", "字段结构无效"),
        (b'{"modelCatalog":[],"extra":true}\n', "字段结构无效"),
        (b'{"modelCatalog":[]}', "必须以单个换行结束"),
        (b'{"modelCatalog":[]}\n{"modelCatalog":[]}\n', "必须以单个换行结束"),
        (b'{"modelCatalog":[]}\n ', "必须以单个换行结束"),
    ],
)
def test_tc_sidecar_009_stdin_bootstrap_rejections(
    payload: bytes, message: str
) -> None:
    """TC-SIDECAR-009 stdin bootstrap 拒绝空值、编码/JSON 错误和宽松 envelope。

    参数：``payload`` 为不可信管道字节，``message`` 为不含正文的预期安全错误摘要。
    返回：无；以 ``RuntimeError`` 证明应用导入前即停止。
    异常边界：错误消息不得拼接 bootstrap 内容，避免模型密钥进入进程日志。
    """

    with pytest.raises(RuntimeError, match=message) as captured:
        sidecar_main._read_sidecar_bootstrap(io.BytesIO(payload))
    assert "fake-private" not in str(captured.value)


def test_tc_sidecar_010_stdin_bootstrap_size_limit() -> None:
    """TC-SIDECAR-010 stdin bootstrap 超过 1 MiB 时通过探针字节拒绝。

    参数：无；构造限制值加一字节的内存流。
    返回：无；以 ``RuntimeError`` 断言入口不会无界读取 Rust 或被劫持父进程输入。
    异常边界：恰好限制内的 JSON 仍需通过正常 JSON 和 envelope 校验。
    """

    oversized = b"x" * (sidecar_main.MAX_BOOTSTRAP_FRAME_BYTES + 1)
    with pytest.raises(RuntimeError, match="超过 1 MiB 限制"):
        sidecar_main._read_sidecar_bootstrap(io.BytesIO(oversized))


def test_tc_sidecar_011_stdin_bootstrap_exact_size_boundary() -> None:
    """TC-SIDECAR-011 完整 JSON+LF 帧恰好 1 MiB 时允许，增加一个正文字节后拒绝。

    参数：无；使用可预测的 JSON 字符串值填满 envelope，避免测试依赖空白或编码差异。
    返回：无；断言 1 MiB 限制包含协议终止 LF，且越界一字节立即失败。
    异常边界：配置层稍后会拒绝字符串目录；本用例只验证 sidecar 传输帧的精确字节边界。
    """

    prefix = b'{"modelCatalog":"'
    suffix = b'","privateRpc":{}}'
    padding_size = (
        sidecar_main.MAX_BOOTSTRAP_FRAME_BYTES - len(prefix) - len(suffix) - 1
    )
    exact_payload = prefix + (b"x" * padding_size) + suffix
    exact_frame = exact_payload + b"\n"
    assert len(exact_frame) == sidecar_main.MAX_BOOTSTRAP_FRAME_BYTES
    assert (
        len(sidecar_main._read_sidecar_bootstrap(io.BytesIO(exact_frame))[0])
        == padding_size
    )

    oversized_payload = prefix + (b"x" * (padding_size + 1)) + suffix
    with pytest.raises(RuntimeError, match="超过 1 MiB 限制"):
        sidecar_main._read_sidecar_bootstrap(io.BytesIO(oversized_payload + b"\n"))


def test_tc_sidecar_012_stdin_bootstrap_read_deadline(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-SIDECAR-012 Rust 未写入也未关闭 stdin 时在截止时间内失败而不永久阻塞。

    参数：``monkeypatch`` 把生产 5 秒门限缩短到 10 毫秒，真实匿名管道写端保持打开以复现父进程异常。
    返回：无；断言读取在 1 秒内以稳定超时摘要结束。
    异常边界：测试始终在 finally 关闭写端，避免泄漏文件描述符或影响后续用例。
    """

    read_fd, write_fd = os.pipe()
    monkeypatch.setattr(sidecar_main, "BOOTSTRAP_READ_TIMEOUT_SECONDS", 0.01)
    started_at = time.monotonic()
    try:
        with os.fdopen(read_fd, "rb", buffering=0) as read_stream:
            with pytest.raises(RuntimeError, match="读取超时"):
                sidecar_main._read_sidecar_bootstrap(read_stream)
    finally:
        os.close(write_fd)
    assert time.monotonic() - started_at < 1


def test_tc_sidecar_013_production_pipe_reads_frame_to_eof() -> None:
    """TC-SIDECAR-013 生产匿名管道路径分块读取帧，并以 Rust 关闭写端形成的 EOF 结束。

    参数：无；使用真实 OS pipe 模拟 Rust 写入完整帧后关闭写端。
    返回：无；断言生产路径读取结果与写入字节完全一致。
    异常边界：不经过 ``BytesIO`` 测试捷径，确保 macOS/POSIX 的 select 与 os.read 组合真实可用。
    """

    read_fd, write_fd = os.pipe()
    expected = b'{"modelCatalog":[]}\n'
    os.write(write_fd, expected)
    os.close(write_fd)
    with os.fdopen(read_fd, "rb", buffering=0) as read_stream:
        assert sidecar_main._read_bootstrap_frame(read_stream) == expected


def test_tc_sidecar_014_pipe_without_file_descriptor_is_rejected() -> None:
    """TC-SIDECAR-014 非内存测试流缺少可等待文件描述符时拒绝阻塞降级读取。

    参数：无；使用基于 BytesIO 但自身不是 BytesIO 的 BufferedReader 触发不支持 fileno 的边界。
    返回：无；以稳定错误证明生产入口不会退回不可设置截止时间的阻塞 ``read``。
    异常边界：纯 ``BytesIO`` 仍是受控单元测试专用路径，由其它 bootstrap 用例覆盖。
    """

    stream = io.BufferedReader(io.BytesIO(b'{"modelCatalog":[]}\n'))
    with pytest.raises(RuntimeError, match="管道不可用"):
        sidecar_main._read_bootstrap_frame(stream)


def test_tc_sidecar_015_elapsed_deadline_fails_before_select(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-SIDECAR-015 已耗尽的读取预算在进入系统等待前立即失败。

    参数：``monkeypatch`` 把截止预算设为零，真实管道保持打开但不写入。
    返回：无；断言超时错误，不依赖 select 自身计时精度。
    异常边界：finally 始终关闭写端，避免文件描述符泄漏。
    """

    read_fd, write_fd = os.pipe()
    monkeypatch.setattr(sidecar_main, "BOOTSTRAP_READ_TIMEOUT_SECONDS", 0.0)
    try:
        with os.fdopen(read_fd, "rb", buffering=0) as read_stream:
            with pytest.raises(RuntimeError, match="读取超时"):
                sidecar_main._read_bootstrap_frame(read_stream)
    finally:
        os.close(write_fd)


@pytest.mark.parametrize(
    "select_error", [OSError("select failed"), ValueError("invalid fd")]
)
def test_tc_sidecar_016_select_failure_is_sanitized(
    monkeypatch: pytest.MonkeyPatch,
    select_error: Exception,
) -> None:
    """TC-SIDECAR-016 select 系统错误转换为不含底层正文的稳定管道失败。

    参数：``monkeypatch`` 注入系统等待错误，``select_error`` 覆盖 OSError 与 ValueError。
    返回：无；断言异常摘要固定且不包含注入正文。
    异常边界：文件描述符由测试 finally 完整关闭。
    """

    read_fd, write_fd = os.pipe()

    def fail_select(*args: object, **kwargs: object) -> object:
        """模拟 select 系统层失败；参数仅用于兼容标准库调用签名。"""

        raise select_error

    monkeypatch.setattr(sidecar_main.select, "select", fail_select)
    try:
        with os.fdopen(read_fd, "rb", buffering=0) as read_stream:
            with pytest.raises(RuntimeError, match="管道读取失败") as captured:
                sidecar_main._read_bootstrap_frame(read_stream)
    finally:
        os.close(write_fd)
    assert str(select_error) not in str(captured.value)


def test_tc_sidecar_017_interrupted_select_retries(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-SIDECAR-017 select 被信号中断后重试，并继续读取单帧直到 EOF。

    参数：``monkeypatch`` 让第一次 select 抛 InterruptedError，后续返回可读并依次模拟帧和 EOF。
    返回：无；断言中断不会消耗或放宽协议内容。
    异常边界：只重试 InterruptedError，其它系统异常由 TC-SIDECAR-016 覆盖。
    """

    read_fd, write_fd = os.pipe()
    select_calls = 0
    chunks = iter([b'{"modelCatalog":[]}\n', b""])

    def interrupted_then_ready(
        *args: object, **kwargs: object
    ) -> tuple[list[int], list[object], list[object]]:
        """首次模拟 EINTR，之后声明原文件描述符可读。"""

        nonlocal select_calls
        select_calls += 1
        if select_calls == 1:
            raise InterruptedError
        return [read_fd], [], []

    monkeypatch.setattr(sidecar_main.select, "select", interrupted_then_ready)
    monkeypatch.setattr(sidecar_main.os, "read", lambda *args: next(chunks))
    try:
        with os.fdopen(read_fd, "rb", buffering=0) as read_stream:
            assert (
                sidecar_main._read_bootstrap_frame(read_stream)
                == b'{"modelCatalog":[]}\n'
            )
    finally:
        os.close(write_fd)
    assert select_calls == 3


def test_tc_sidecar_018_pipe_read_failure_and_oversize_are_sanitized(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-SIDECAR-018 os.read 失败与生产管道越界都返回稳定摘要并清空临时缓冲。

    参数：``monkeypatch`` 控制 select 始终可读，并分别注入读取错误与 1 MiB 加二字节块。
    返回：无；断言两类异常不包含系统正文或输入内容。
    异常边界：用同一真实文件描述符覆盖读取分支，不执行实际大块管道写入以避免测试阻塞。
    """

    read_fd, write_fd = os.pipe()
    monkeypatch.setattr(
        sidecar_main.select, "select", lambda *args: ([read_fd], [], [])
    )
    try:
        with os.fdopen(read_fd, "rb", buffering=0) as read_stream:
            monkeypatch.setattr(
                sidecar_main.os,
                "read",
                lambda *args: (_ for _ in ()).throw(OSError("private-path")),
            )
            with pytest.raises(RuntimeError, match="管道读取失败") as captured:
                sidecar_main._read_bootstrap_frame(read_stream)
            assert "private-path" not in str(captured.value)

            monkeypatch.setattr(
                sidecar_main,
                "BOOTSTRAP_READ_CHUNK_BYTES",
                sidecar_main.MAX_BOOTSTRAP_FRAME_BYTES + 1,
            )
            monkeypatch.setattr(
                sidecar_main.os,
                "read",
                lambda *args: b"x" * (sidecar_main.MAX_BOOTSTRAP_FRAME_BYTES + 1),
            )
            with pytest.raises(RuntimeError, match="超过 1 MiB 限制"):
                sidecar_main._read_bootstrap_frame(read_stream)
    finally:
        os.close(write_fd)
