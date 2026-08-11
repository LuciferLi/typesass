"""配置、错误对象和结构化日志测试。"""

import json
import logging
from pathlib import Path
from typing import Optional

import pytest

from app.config import (
    PUBLIC_MAX_AUDIO_BYTES,
    PUBLIC_ACCESS_TOKEN_TTL_SECONDS,
    PUBLIC_BASE_URL,
    PUBLIC_MAX_BODY_BYTES,
    PUBLIC_MAX_TEXT_CHARS,
    _api_tokens,
    _device_approver_client_ids,
    _high_entropy_secret,
    _model_base_url,
    _model_catalog,
    _positive_float,
    _positive_integer,
    _required_environment,
    load_settings,
    set_model_catalog_bootstrap,
)
from app.errors import ApiError, RequestBodyTooLarge
from app.logging_config import JsonFormatter, _redact, configure_logging


def test_tc_cfg_001_required_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """TC-CFG-001 必填环境变量接受去空白值并拒绝缺失值。"""

    monkeypatch.setenv("REQUIRED_VALUE", "  fake-value  ")
    assert _required_environment("REQUIRED_VALUE") == "fake-value"
    monkeypatch.setenv("REQUIRED_VALUE", "   ")
    with pytest.raises(RuntimeError, match="缺少必填环境变量"):
        _required_environment("REQUIRED_VALUE")


@pytest.mark.parametrize(
    ("reader", "name", "raw_value", "message"),
    [
        (_positive_integer, "POSITIVE_INT", "bad", "必须是正整数"),
        (_positive_integer, "POSITIVE_INT", "0", "必须大于零"),
        (_positive_float, "POSITIVE_FLOAT", "bad", "必须是正数"),
        (_positive_float, "POSITIVE_FLOAT", "-1", "必须大于零"),
    ],
)
def test_tc_cfg_002_invalid_positive_values(
    monkeypatch: pytest.MonkeyPatch,
    reader: object,
    name: str,
    raw_value: str,
    message: str,
) -> None:
    """TC-CFG-002 数值配置拒绝非法格式和非正数。"""

    monkeypatch.setenv(name, raw_value)
    with pytest.raises(RuntimeError, match=message):
        reader(name, 1)  # type: ignore[operator]


def test_tc_cfg_003_defaults_and_overrides(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """TC-CFG-003 配置加载覆盖默认值、自定义值与缓存。"""

    load_settings.cache_clear()
    for name in (
        "AITOOL_REQUEST_TIMEOUT_SECONDS",
        "AITOOL_CONCURRENCY_LIMIT",
        "AITOOL_CONCURRENCY_WAIT_SECONDS",
        "AITOOL_ACCESS_TOKEN_TTL_SECONDS",
        "AITOOL_CLIENT_RATE_LIMIT_PER_MINUTE",
        "AITOOL_CLIENT_DAILY_QUOTA",
        "AITOOL_LOG_MAX_BYTES",
        "AITOOL_LOG_BACKUP_COUNT",
    ):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv(
        "AITOOL_API_KEYS_JSON", '{"client-one":"fake-client-secret-000000000000001"}'
    )
    monkeypatch.setenv("AITOOL_DEVICE_APPROVER_CLIENT_IDS", "client-one")
    monkeypatch.setenv("AITOOL_LOG_FILE", str(tmp_path / "default.log"))
    monkeypatch.setenv(
        "AITOOL_MODEL_CATALOG_JSON", "legacy-sensitive-value-must-be-ignored"
    )
    set_model_catalog_bootstrap(None)
    settings = load_settings()
    assert settings.api_tokens == (
        ("client-one", "fake-client-secret-000000000000001"),
    )
    assert settings.device_approver_client_ids == ("client-one",)
    assert settings.token_signing_key == "fake-signing-key-for-tests-000000000001"
    assert settings.access_token_ttl_seconds == PUBLIC_ACCESS_TOKEN_TTL_SECONDS
    assert settings.client_rate_limit_per_minute == 60
    assert settings.client_daily_quota == 10000
    assert settings.quota_database_file.endswith(".sqlite3")
    assert settings.public_base_url == PUBLIC_BASE_URL
    assert settings.model_catalog == ()
    assert settings.max_body_bytes == PUBLIC_MAX_BODY_BYTES
    assert settings.max_audio_bytes == PUBLIC_MAX_AUDIO_BYTES
    assert settings.max_text_chars == PUBLIC_MAX_TEXT_CHARS
    assert settings.enable_dev_bearer_token is False
    assert settings.dev_bearer_token == ""
    assert settings is load_settings()
    assert _positive_integer("UNSET_INT", 7) == 7
    assert _positive_float("UNSET_FLOAT", 1.5) == 1.5

    load_settings.cache_clear()
    monkeypatch.setenv("AITOOL_ENABLE_DEV_BEARER_TOKEN", "1")
    monkeypatch.setenv(
        "AITOOL_DEV_BEARER_TOKEN", "codexman-dev-bearer-token-000000000001"
    )
    set_model_catalog_bootstrap(None)
    dev_settings = load_settings()
    assert dev_settings.enable_dev_bearer_token is True
    assert dev_settings.dev_bearer_token == "codexman-dev-bearer-token-000000000001"

    load_settings.cache_clear()
    monkeypatch.delenv("AITOOL_ENABLE_DEV_BEARER_TOKEN", raising=False)
    monkeypatch.delenv("AITOOL_DEV_BEARER_TOKEN", raising=False)
    set_model_catalog_bootstrap(
        [
            {
                "id": "opaque-1",
                "displayName": " Text ",
                "capability": "text",
                "enabled": True,
                "isDefault": True,
                "provider": "openai-compatible",
                "baseUrl": "https://custom.invalid/v2/",
                "modelName": " model-private ",
                "apiKey": " secret-private ",
            }
        ]
    )
    monkeypatch.setenv("AITOOL_REQUEST_TIMEOUT_SECONDS", "2.5")
    monkeypatch.setenv("AITOOL_CONCURRENCY_LIMIT", "3")
    monkeypatch.setenv("AITOOL_CONCURRENCY_WAIT_SECONDS", "0.5")
    monkeypatch.setenv("AITOOL_ACCESS_TOKEN_TTL_SECONDS", "120")
    monkeypatch.setenv("AITOOL_CLIENT_RATE_LIMIT_PER_MINUTE", "7")
    monkeypatch.setenv("AITOOL_CLIENT_DAILY_QUOTA", "8")
    monkeypatch.setenv("AITOOL_MAX_BODY_BYTES", "10")
    monkeypatch.setenv("AITOOL_MAX_AUDIO_BYTES", "11")
    monkeypatch.setenv("AITOOL_MAX_TEXT_CHARS", "12")
    monkeypatch.setenv("AITOOL_LOG_MAX_BYTES", "13")
    monkeypatch.setenv("AITOOL_LOG_BACKUP_COUNT", "14")
    custom = load_settings()
    assert len(custom.model_catalog) == 1
    assert custom.model_catalog[0].id == "opaque-1"
    assert custom.model_catalog[0].display_name == "Text"
    assert custom.model_catalog[0].base_url == "https://custom.invalid/v2"
    assert custom.model_catalog[0].model_name == "model-private"
    assert custom.model_catalog[0].api_key == "secret-private"
    assert custom.request_timeout_seconds == 2.5
    assert custom.concurrency_limit == 3
    assert custom.concurrency_wait_seconds == 0.5
    assert custom.access_token_ttl_seconds == PUBLIC_ACCESS_TOKEN_TTL_SECONDS
    assert custom.client_rate_limit_per_minute == 7
    assert custom.client_daily_quota == 8
    assert custom.max_body_bytes == PUBLIC_MAX_BODY_BYTES
    assert custom.max_audio_bytes == PUBLIC_MAX_AUDIO_BYTES
    assert custom.max_text_chars == PUBLIC_MAX_TEXT_CHARS
    assert custom.log_max_bytes == 13
    assert custom.log_backup_count == 14
    assert _model_catalog() == ()
    load_settings.cache_clear()


@pytest.mark.parametrize(
    ("serialized_payload", "message"),
    [
        ("{}", "必须是 JSON 数组"),
        ("[1]", "字段结构无效"),
        ('[{"id":"missing"}]', "字段结构无效"),
        (
            '[{"id":"bad id","displayName":"Name","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":"secret"}]',
            "id 格式无效或重复",
        ),
        (
            '[{"id":"same","displayName":"One","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":"secret"},{"id":"same","displayName":"Two",'
            '"capability":"asr","enabled":true,"isDefault":false,"provider":"openai-compatible",'
            '"baseUrl":"https://upstream.invalid/v1","modelName":"private","apiKey":"secret"}]',
            "id 格式无效或重复",
        ),
        (
            '[{"id":"valid","displayName":" ","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":"secret"}]',
            "displayName 无效",
        ),
        (
            '[{"id":"valid","displayName":"Name","capability":"vision","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":"secret"}]',
            "capability 无效",
        ),
        (
            '[{"id":"valid","displayName":"Name","capability":"text","enabled":1,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":"secret"}]',
            "状态字段必须是布尔值",
        ),
        (
            '[{"id":"valid","displayName":"Name","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"private-provider","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":"secret"}]',
            "provider 不受支持",
        ),
        (
            '[{"id":"valid","displayName":"Name","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":" ",'
            '"modelName":"private","apiKey":"secret"}]',
            "baseUrl 无效",
        ),
        (
            '[{"id":"valid","displayName":"Name","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":" ","apiKey":"secret"}]',
            "modelName 无效",
        ),
        (
            '[{"id":"valid","displayName":"Name","capability":"text","enabled":true,'
            '"isDefault":false,"provider":"openai-compatible","baseUrl":"https://upstream.invalid/v1",'
            '"modelName":"private","apiKey":" "}]',
            "apiKey 无效",
        ),
    ],
)
def test_tc_cfg_003a_model_catalog_rejections(
    serialized_payload: str,
    message: str,
) -> None:
    """TC-CFG-003A 模型目录拒绝格式、字段、类型和私有连接参数错误。

    参数：``serialized_payload`` 为便于参数化表达的目录 JSON，``message`` 为安全错误摘要。
    返回：无；通过 ``RuntimeError`` 断言启动门禁。
    异常边界：错误不得回显 apiKey、modelName 或 URL 原值。
    """

    set_model_catalog_bootstrap(json.loads(serialized_payload))
    with pytest.raises(RuntimeError, match=message) as captured:
        _model_catalog()
    assert "secret" not in str(captured.value)


@pytest.mark.parametrize(
    ("raw_value", "expected"),
    [
        (" https://model.example.test/ ", "https://model.example.test"),
        (
            "https://model.example.test/api/openai/v1/",
            "https://model.example.test/api/openai/v1",
        ),
        ("http://127.0.0.1:9000/v1", "http://127.0.0.1:9000/v1"),
        ("http://localhost:9000/v1", "http://localhost:9000/v1"),
        ("http://[::1]:9000/v1", "http://[::1]:9000/v1"),
    ],
)
def test_tc_cfg_003b_model_base_url_allowed(raw_value: str, expected: str) -> None:
    """TC-CFG-003B 模型基础 URL 接受 HTTPS 和回环 HTTP，并完成末尾斜杠规范化。

    参数：``raw_value`` 为 sidecar 注入地址；``expected`` 为规范地址。
    返回：无；通过读取结果断言验证固定 endpoint 追加前的基础地址。
    异常边界：公网 HTTP 不在成功矩阵，非法输入由 TC-CFG-003C 覆盖。
    """

    assert _model_base_url(raw_value, 0) == expected


@pytest.mark.parametrize(
    ("raw_value", "message"),
    [
        ("   ", "必须使用 HTTPS 或本机 HTTP"),
        ("http://model.example.test/v1", "必须使用 HTTPS 或本机 HTTP"),
        ("ftp://model.example.test/v1", "必须使用 HTTPS 或本机 HTTP"),
        ("https:///v1", "必须使用 HTTPS 或本机 HTTP"),
        ("https://model.example.test:bad/v1", "baseUrl 非法"),
        ("https://model example.test/v1", "必须使用 HTTPS 或本机 HTTP"),
        ("https://user:secret@model.example.test/v1", "禁止包含凭据"),
        ("https://model.example.test/v1?region=cn", "禁止包含 query 或 fragment"),
        ("https://model.example.test/v1#primary", "禁止包含 query 或 fragment"),
        ("https://model.example.test/v1;region=cn", "包含非法基础路径"),
        ("https://model.example.test/v1\\backup", "包含非法基础路径"),
        ("https://model.example.test/api//v1", "包含非法基础路径"),
        ("https://model.example.test/api/../v1", "包含非法基础路径"),
        ("https://model.example.test/api/%2E/v1", "包含非法基础路径"),
        ("https://model.example.test/v1/chat/completions", "不得包含固定 endpoint"),
    ],
)
def test_tc_cfg_003c_model_base_url_rejections(raw_value: str, message: str) -> None:
    """TC-CFG-003C 模型基础 URL 拒绝不安全地址和无法追加固定 endpoint 的路径。

    参数：``raw_value`` 为非法部署值；``message`` 为预期错误摘要。
    返回：无；以 ``RuntimeError`` 证明非法配置在应用构造前触发启动门禁。
    异常边界：覆盖协议/主机/端口、凭据、查询/片段、路径和重复 endpoint 分支。
    """

    with pytest.raises(RuntimeError, match=message):
        _model_base_url(raw_value, 2)


@pytest.mark.parametrize(
    ("raw_value", "message"),
    [
        ("not-json", "必须是 JSON 对象"),
        ("[]", "必须是 JSON 对象"),
        ("{}", "必须包含至少一个调用方"),
        ('{"":"fake-client-secret-000000000000001"}', "调用方和 Token 格式无效"),
        ('{"client":1}', "调用方和 Token 格式无效"),
        ('{"client":"short"}', "调用方和 Token 格式无效"),
        (
            '{"client-one":"fake-shared-secret-000000000000001","client-two":"fake-shared-secret-000000000000001"}',
            "不允许多个调用方共享 Token",
        ),
    ],
)
def test_tc_cfg_005_api_token_validation(
    monkeypatch: pytest.MonkeyPatch, raw_value: str, message: str
) -> None:
    """TC-CFG-005 兼容旧调用方 Token 配置时仍拒绝格式、短值和重复值。"""

    monkeypatch.setenv("AITOOL_API_KEYS_JSON", raw_value)
    if raw_value == "{}":
        assert _api_tokens() == ()
        return
    with pytest.raises(RuntimeError, match=message):
        _api_tokens()


def test_tc_cfg_006_api_token_normalization(monkeypatch: pytest.MonkeyPatch) -> None:
    """TC-CFG-006 调用方 ID 和 Token 去空白并保持独立映射。"""

    monkeypatch.setenv(
        "AITOOL_API_KEYS_JSON",
        '{" client-one ":" fake-client-secret-000000000000001 ","client-two":"fake-client-secret-000000000000002"}',
    )
    assert _api_tokens() == (
        ("client-one", "fake-client-secret-000000000000001"),
        ("client-two", "fake-client-secret-000000000000002"),
    )


def test_tc_cfg_006a_device_approver_normalization(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-CFG-006A 设备批准方配置去空白、去重并保持登记顺序。"""

    api_tokens = (
        ("client-one", "fake-client-secret-000000000000001"),
        ("client-two", "fake-client-secret-000000000000002"),
    )
    monkeypatch.setenv(
        "AITOOL_DEVICE_APPROVER_CLIENT_IDS", " client-two,client-one,client-two "
    )
    assert _device_approver_client_ids(api_tokens) == ("client-two", "client-one")


@pytest.mark.parametrize(
    ("raw_value", "message"),
    [
        (None, "缺少必填环境变量"),
        ("   ", "缺少必填环境变量"),
        (", ,", "必须引用已登记调用方"),
        ("unknown-client", "必须引用已登记调用方"),
    ],
)
def test_tc_cfg_006b_device_approver_rejections(
    monkeypatch: pytest.MonkeyPatch, raw_value: Optional[str], message: str
) -> None:
    """TC-CFG-006B 兼容旧设备批准方配置时仍拒绝空列表和未知调用方。"""

    if raw_value is None:
        monkeypatch.delenv("AITOOL_DEVICE_APPROVER_CLIENT_IDS", raising=False)
    else:
        monkeypatch.setenv("AITOOL_DEVICE_APPROVER_CLIENT_IDS", raw_value)
    if raw_value is None or raw_value.strip() == "":
        assert (
            _device_approver_client_ids(
                (("client-one", "fake-client-secret-000000000000001"),)
            )
            == ()
        )
        return
    with pytest.raises(RuntimeError, match=message):
        _device_approver_client_ids(
            (("client-one", "fake-client-secret-000000000000001"),)
        )


def test_tc_cfg_011_high_entropy_secret(monkeypatch: pytest.MonkeyPatch) -> None:
    """TC-CFG-011 签名密钥必须存在且至少 32 字符。"""

    monkeypatch.setenv("SIGNING_KEY", "short-fake-key")
    with pytest.raises(RuntimeError, match="至少需要 32 个字符"):
        _high_entropy_secret("SIGNING_KEY")
    monkeypatch.setenv("SIGNING_KEY", "fake-high-entropy-secret-000000001")
    assert _high_entropy_secret("SIGNING_KEY") == "fake-high-entropy-secret-000000001"


def test_tc_cfg_007_public_base_url_is_fixed_localhost(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-CFG-007 公开地址固定为本机 sidecar，并完全忽略遗留远程部署环境变量。"""

    monkeypatch.setenv("AITOOL_PUBLIC_BASE_URL", "https://api.example.test")
    load_settings.cache_clear()
    set_model_catalog_bootstrap(None)
    assert load_settings().public_base_url == PUBLIC_BASE_URL


def test_tc_log_001_recursive_redaction() -> None:
    """TC-LOG-001 日志字段递归脱敏且未知对象安全字符串化。"""

    class Opaque:
        def __str__(self) -> str:
            return "opaque"

    value = {
        "Authorization": "fake-secret",
        "nested": [{"apiKey": "fake-key", "ok": True}, (None, 2.5)],
        "object": Opaque(),
    }
    assert _redact(value) == {
        "Authorization": "[REDACTED]",
        "nested": [{"apiKey": "[REDACTED]", "ok": True}, [None, 2.5]],
        "object": "opaque",
    }


def test_tc_log_002_json_formatter_with_context_and_exception() -> None:
    """TC-LOG-002 JSON 日志包含上下文和异常栈且不泄漏敏感值。"""

    formatter = JsonFormatter()
    try:
        raise ValueError("fake failure")
    except ValueError:
        record = logging.LogRecord(
            "test",
            logging.ERROR,
            __file__,
            1,
            "failed %s",
            ("once",),
            __import__("sys").exc_info(),
        )
    record.context = {"token": "fake-token", "plain": "visible"}  # type: ignore[attr-defined]
    payload = json.loads(formatter.format(record))
    assert payload["message"] == "failed once"
    assert payload["context"] == {"token": "[REDACTED]", "plain": "visible"}
    assert "ValueError: fake failure" in payload["exception"]

    plain = logging.LogRecord("test", logging.INFO, __file__, 1, "ok", (), None)
    assert "context" not in json.loads(formatter.format(plain))


def test_tc_log_003_rotation_and_api_errors(settings_factory: object) -> None:
    """TC-LOG-003 文件日志按上限轮转，错误对象仅保存稳定字段。"""

    settings = settings_factory(log_max_bytes=80, log_backup_count=2)  # type: ignore[operator]
    configure_logging(settings)
    logger = logging.getLogger("rotation-test")
    for index in range(12):
        logger.info(
            "line-%s-xxxxxxxxxxxxxxxxxxxxxxxx",
            index,
            extra={"context": {"audioBase64": "fake-audio"}},
        )
    for handler in logging.getLogger().handlers:
        handler.flush()
    log_path = Path(settings.log_file)
    assert log_path.exists()
    assert list(log_path.parent.glob("server.log.*"))
    combined = "".join(
        path.read_text(encoding="utf-8") for path in log_path.parent.glob("server.log*")
    )
    assert "fake-audio" not in combined
    assert "[REDACTED]" in combined

    error = ApiError(418, "STABLE_CODE", "safe", {"Retry-After": "1"})
    assert (
        error.status_code,
        error.code,
        error.message,
        error.headers,
        str(error),
    ) == (
        418,
        "STABLE_CODE",
        "safe",
        {"Retry-After": "1"},
        "safe",
    )
    assert isinstance(RequestBodyTooLarge(), Exception)
