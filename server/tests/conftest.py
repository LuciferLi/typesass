"""服务端自动化测试共享夹具。"""

import os
from pathlib import Path
from typing import Callable, Iterator

import httpx
import pytest


os.environ.setdefault(
    "AITOOL_API_KEYS_JSON",
    '{"desktop-test":"fake-desktop-client-secret-0000000001"}',
)
os.environ.setdefault(
    "AITOOL_TOKEN_SIGNING_KEY", "fake-signing-key-for-tests-000000000001"
)
os.environ.setdefault("AITOOL_DEVICE_APPROVER_CLIENT_IDS", "desktop-test")
os.environ.setdefault("AITOOL_CLIENT_RATE_LIMIT_PER_MINUTE", "60")
os.environ.setdefault("AITOOL_CLIENT_DAILY_QUOTA", "10000")
os.environ.setdefault(
    "AITOOL_QUOTA_DATABASE_FILE",
    "/tmp/aitool-pytest-quota-{0}.sqlite3".format(os.getpid()),
)
os.environ.setdefault(
    "AITOOL_ACCESS_TOKEN_DATABASE_FILE",
    "/tmp/aitool-pytest-access-token-{0}.sqlite3".format(os.getpid()),
)
os.environ.setdefault("AITOOL_LOG_FILE", "/tmp/aitool-pytest.log")
os.environ.setdefault("AITOOL_MAX_BODY_BYTES", "512")
os.environ.setdefault("AITOOL_MAX_AUDIO_BYTES", "4")
os.environ.setdefault("AITOOL_MAX_TEXT_CHARS", "8")
os.environ.setdefault("AITOOL_CONCURRENCY_LIMIT", "1")
os.environ.setdefault("AITOOL_CONCURRENCY_WAIT_SECONDS", "0.01")

from app.config import (
    ModelCatalogItem,
    Settings,
    load_settings,
    set_model_catalog_bootstrap,
)  # noqa: E402


TEST_MODEL_CATALOG = [
    {
        "id": "fake-asr-id",
        "displayName": "Fake ASR",
        "capability": "asr",
        "enabled": True,
        "isDefault": True,
        "provider": "openai-compatible",
        "baseUrl": "https://upstream.invalid/v1",
        "modelName": "fake-asr-model",
        "apiKey": "fake-local-asr-key",
    },
    {
        "id": "fake-text-id",
        "displayName": "Fake Text",
        "capability": "text",
        "enabled": True,
        "isDefault": True,
        "provider": "openai-compatible",
        "baseUrl": "https://upstream.invalid/v1",
        "modelName": "fake-text-model",
        "apiKey": "fake-local-text-key",
    },
]
set_model_catalog_bootstrap(TEST_MODEL_CATALOG)


@pytest.fixture(autouse=True)
def restore_test_model_bootstrap() -> Iterator[None]:
    """在每个用例后恢复一次性测试目录和配置缓存。

    用途：生产 bootstrap 只消费一次，而配置测试会主动清除缓存；本夹具保证后续真实 lifespan 仍取得固定假模型。
    流程：用例运行期间不干预配置，结束后清除 ``load_settings`` 缓存并重新注入仅含假密钥的目录对象。
    参数：无。
    返回：pytest yield 夹具控制对象。
    异常边界：不会写模型环境变量，测试失败时 finally 阶段仍会恢复隔离状态。
    """

    yield
    load_settings.cache_clear()
    set_model_catalog_bootstrap(TEST_MODEL_CATALOG)


@pytest.fixture
def settings_factory(tmp_path: Path) -> Callable[..., Settings]:
    """构造只含假密钥且日志隔离到临时目录的测试配置。"""

    def factory(**overrides: object) -> Settings:
        values: dict[str, object] = {
            "api_tokens": (("desktop-test", "fake-desktop-client-secret-0000000001"),),
            "device_approver_client_ids": ("desktop-test",),
            "token_signing_key": "fake-signing-key-for-tests-000000000001",
            "access_token_ttl_seconds": 28800,
            "client_rate_limit_per_minute": 60,
            "client_daily_quota": 10000,
            "quota_database_file": str(tmp_path / "quota.sqlite3"),
            "access_token_database_file": str(tmp_path / "access-token.sqlite3"),
            "public_base_url": "http://127.0.0.1:18080",
            "model_catalog": (
                ModelCatalogItem(
                    id="fake-asr-id",
                    display_name="Fake ASR",
                    capability="asr",
                    enabled=True,
                    is_default=True,
                    provider="openai-compatible",
                    base_url="https://upstream.invalid/v1",
                    model_name="fake-asr-model",
                    api_key="fake-local-asr-key",
                ),
                ModelCatalogItem(
                    id="fake-text-id",
                    display_name="Fake Text",
                    capability="text",
                    enabled=True,
                    is_default=True,
                    provider="openai-compatible",
                    base_url="https://upstream.invalid/v1",
                    model_name="fake-text-model",
                    api_key="fake-local-text-key",
                ),
            ),
            "request_timeout_seconds": 0.05,
            "concurrency_limit": 1,
            "concurrency_wait_seconds": 0.01,
            "max_body_bytes": 1024,
            "max_audio_bytes": 4,
            "max_text_chars": 8,
            "log_file": str(tmp_path / "logs" / "server.log"),
            "log_max_bytes": 128,
            "log_backup_count": 2,
            "enable_dev_bearer_token": False,
            "dev_bearer_token": "",
        }
        values.update(overrides)
        return Settings(**values)  # type: ignore[arg-type]

    return factory


@pytest.fixture
def client_factory() -> Callable[[httpx.AsyncBaseTransport], httpx.AsyncClient]:
    """构造仅使用测试 Transport 的异步 HTTP 客户端。"""

    def factory(transport: httpx.AsyncBaseTransport) -> httpx.AsyncClient:
        return httpx.AsyncClient(transport=transport)

    return factory
