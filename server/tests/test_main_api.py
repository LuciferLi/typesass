"""FastAPI 路由、鉴权、安全边界和 OpenAPI 契约测试。"""

import asyncio
import base64
from collections.abc import AsyncIterator, Callable
from contextlib import asynccontextmanager
import json
import logging
from typing import Any, Optional

from fastapi import Request
from fastapi.exceptions import RequestValidationError
import httpx
import pytest
from starlette.exceptions import HTTPException as StarletteHTTPException

from app import main
from app.auth import AppAccessTokenService
from app.config import (
    ModelCatalogItem,
    PUBLIC_MAX_AUDIO_BYTES,
    PUBLIC_MAX_BODY_BYTES,
    PUBLIC_MAX_TEXT_CHARS,
    set_model_catalog_bootstrap,
)
from app.errors import ApiError, is_retryable_error
from app.rate_limit import ClientRateLimiter
from app.service import ModelService


CLIENT_ID = "desktop-test"
CLIENT_SECRET = "fake-desktop-client-secret-0000000001"
SECONDARY_ID = "secondary-approver"
SECONDARY_SECRET = "fake-secondary-client-secret-0000000001"
ORDINARY_ID = "ordinary-client"
ORDINARY_SECRET = "fake-ordinary-client-secret-000000000001"


class FakeAccessTokenApprovalRpcClient:
    """记录授权申请私有 RPC 调用并返回固定审批结果。"""

    def __init__(
        self,
        approved: bool,
        control_secret: str = "test-control-secret-000000000000000000",
    ) -> None:
        """初始化审批结果和调用列表。"""

        self.approved = approved
        self.control_secret = control_secret
        self.calls: list[tuple[str, str, dict[str, object]]] = []

    def verify_secret(self, secret: str) -> bool:
        """校验测试用内部控制密钥。"""

        return secret == self.control_secret

    async def call(
        self, method: str, request_id: str, params: dict[str, object]
    ) -> object:
        """记录授权申请 RPC 参数并返回 App 审批结论。"""

        self.calls.append((method, request_id, params))
        return {"approved": self.approved, "message": None}


def success_payload(
    text: str = "mock result", model: str = "mock-model"
) -> dict[str, object]:
    """构造假上游成功结果。"""

    return {"choices": [{"message": {"content": text}}], "model": model}


@asynccontextmanager
async def api_client(
    handler: Optional[Callable[[httpx.Request], httpx.Response]] = None,
    *,
    raise_app_exceptions: bool = False,
) -> AsyncIterator[httpx.AsyncClient]:
    """启动真实应用生命周期，并用 MockTransport 替换唯一上游连接。"""

    async with main.lifespan(main.app):
        if handler is not None:
            upstream = httpx.AsyncClient(transport=httpx.MockTransport(handler))
            original = main.app.state.model_service
            await original.client.aclose()
            main.app.state.model_service = ModelService(
                main.app.state.settings, upstream
            )
        else:
            upstream = None
        transport = httpx.ASGITransport(
            app=main.app, raise_app_exceptions=raise_app_exceptions
        )
        async with httpx.AsyncClient(
            transport=transport, base_url="http://testserver"
        ) as client:
            yield client
        if upstream is not None:
            await upstream.aclose()


def assert_error(response: httpx.Response, status: int, code: str) -> None:
    """断言统一错误 envelope、request ID 与稳定重试语义。"""

    assert response.status_code == status
    assert response.json()["error"]["code"] == code
    assert response.json()["error"]["requestId"] == response.headers["x-request-id"]
    assert response.json()["error"]["retryable"] is is_retryable_error(code)


async def session_headers(
    client: httpx.AsyncClient, request_id: Optional[str] = None
) -> dict[str, str]:
    """通过真实授权码申请接口取得业务接口 Bearer Header。"""

    main.app.state.private_rpc = FakeAccessTokenApprovalRpcClient(True)
    response = await client.post(
        "/v1/access-tokens/request", json={"name": "pytest", "expiresAt": None}
    )
    assert response.status_code == 200
    headers = {"Authorization": "Bearer {0}".format(response.json()["accessToken"])}
    headers["Origin"] = "https://public.example"
    if request_id is not None:
        headers["X-Request-ID"] = request_id
    return headers


@pytest.mark.asyncio
async def test_tc_api_001_health_request_id_and_cors() -> None:
    """TC-API-001 健康检查免鉴权、请求 ID 透传且浏览器来源不参与门禁。"""

    async with api_client() as client:
        response = await client.get(
            "/health",
            headers={
                "X-Request-ID": "client-request-1",
                "Origin": "https://allowed.example",
            },
        )
        denied = await client.get(
            "/health", headers={"Origin": "https://denied.example"}
        )
        replaced = await client.get("/health", headers={"X-Request-ID": "x" * 129})
        preflight = await client.options(
            "/v1/text/process",
            headers={
                "Origin": "https://allowed.example",
                "Access-Control-Request-Method": "POST",
                "Access-Control-Request-Headers": "authorization,content-type,x-request-id",
            },
        )
        bad_preflight = await client.options(
            "/v1/text/process",
            headers={
                "Origin": "https://allowed.example",
                "Access-Control-Request-Method": "DELETE",
                "X-Request-ID": "cors-method-denied",
            },
        )
        denied_origin_preflight = await client.options(
            "/v1/text/process",
            headers={
                "Origin": "https://denied.example",
                "Access-Control-Request-Method": "POST",
            },
        )
        denied_header_preflight = await client.options(
            "/v1/text/process",
            headers={
                "Origin": "https://allowed.example",
                "Access-Control-Request-Method": "POST",
                "Access-Control-Request-Headers": "x-private-header",
            },
        )
    assert response.json() == {"ok": True, "name": "codexman-ai-api"}
    assert response.headers["x-request-id"] == "client-request-1"
    assert len(replaced.headers["x-request-id"]) == 32
    assert replaced.headers["x-request-id"] != "x" * 129
    assert response.headers["access-control-allow-origin"] == "*"
    assert denied.headers["access-control-allow-origin"] == "*"
    assert preflight.status_code == 200
    assert preflight.headers["access-control-allow-origin"] == "*"
    assert bad_preflight.status_code == 200
    assert bad_preflight.headers["access-control-allow-origin"] == "*"
    assert denied_origin_preflight.status_code == 200
    assert denied_origin_preflight.headers["access-control-allow-origin"] == "*"
    assert denied_header_preflight.status_code == 200
    assert denied_header_preflight.headers["access-control-allow-origin"] == "*"


@pytest.mark.asyncio
async def test_tc_api_001a_lifespan_starts_without_models(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-API-001A 模型目录缺失时应用生命周期仍能完成启动和退出。

    参数：``monkeypatch`` 临时移除 sidecar 模型目录环境变量并隔离配置缓存。
    返回：无；通过 lifespan 内的空目录和已初始化服务断言证明 HTTP 进程可用。
    异常边界：只豁免模型目录，鉴权、签名、公开地址和日志等最小可信启动配置仍必须有效。
    """

    monkeypatch.delenv("AITOOL_MODEL_CATALOG_JSON", raising=False)
    main.load_settings.cache_clear()
    set_model_catalog_bootstrap(None)
    try:
        async with main.lifespan(main.app):
            assert main.app.state.settings.model_catalog == ()
            assert main.app.state.model_service.list_models() == ()
    finally:
        main.load_settings.cache_clear()


@pytest.mark.parametrize(
    ("headers", "code"),
    [
        ({}, "ORIGIN_REQUIRED"),
        ({"Origin": "https://public.example"}, "UNAUTHORIZED"),
        (
            {"Origin": "https://public.example", "Authorization": "Bearer wrong-fake-token-0000"},
            "UNAUTHORIZED",
        ),
        ({"Origin": "https://public.example", "Authorization": "Basic fake"}, "UNAUTHORIZED"),
    ],
)
@pytest.mark.asyncio
async def test_tc_api_002_authentication_failures(
    headers: dict[str, str], code: str
) -> None:
    """TC-API-002 缺失 Origin、缺失授权码、错误授权码或非 Bearer 鉴权均返回 401。"""

    async with api_client() as client:
        response = await client.post(
            "/v1/text/process",
            headers=headers,
            json={
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
            },
        )
    assert_error(response, 401, code)
    assert response.headers["www-authenticate"] == "Bearer"


@pytest.mark.asyncio
async def test_tc_api_002a_access_token_create_list_revoke_and_origin_rules() -> None:
    """TC-API-002A 授权码创建、列表、撤销、内网免授权和公网鉴权形成接口闭环。"""

    async with api_client() as client:
        approval_rpc = FakeAccessTokenApprovalRpcClient(True)
        main.app.state.private_rpc = approval_rpc
        requested = await client.post(
            "/v1/access-tokens/request",
            headers={"X-Request-ID": "token-request-002a"},
            json={"name": "Chrome 插件", "expiresAt": None},
        )
        access_token = requested.json()["accessToken"]
        auth_headers = {
            "Origin": "https://public.example",
            "Authorization": "Bearer {0}".format(access_token),
        }
        listed = await client.get("/v1/access-tokens", headers=auth_headers)
        verified = await client.get("/v1/access-tokens/verify", headers=auth_headers)
        extension_verified = await client.get(
            "/v1/access-tokens/verify",
            headers={
                "X-CodexMan-Client-Origin": "chrome-extension://codexman",
                "Authorization": "Bearer {0}".format(access_token),
            },
        )
        extension_missing_token = await client.get(
            "/v1/access-tokens/verify",
            headers={"X-CodexMan-Client-Origin": "chrome-extension://codexman"},
        )
        internal = await client.get("/v1/models", headers={"Origin": "http://127.0.0.1:4006"})
        public_success = await client.get("/v1/models", headers=auth_headers)
        revoked = await client.post(
            "/v1/access-tokens/{0}/revoke".format(listed.json()[0]["id"]),
            headers=auth_headers,
        )
        after_revoke = await client.get("/v1/models", headers=auth_headers)
        verify_after_revoke = await client.get(
            "/v1/access-tokens/verify", headers=auth_headers
        )
        missing_origin = await client.get(
            "/v1/models", headers={"Authorization": "Bearer {0}".format(access_token)}
        )
    assert requested.status_code == 200
    assert requested.json()["status"] == "approved"
    assert approval_rpc.calls == [
        (
            "requestAccessTokenApproval",
            "token-request-002a",
            {"requestId": "token-request-002a", "name": "Chrome 插件", "expiresAt": None},
        )
    ]
    assert access_token.startswith("typesass_")
    assert listed.status_code == 200
    assert listed.json()[0]["name"] == "Chrome 插件"
    assert listed.json()[0]["token"] == access_token
    assert verified.status_code == 200
    assert verified.json()["ok"] is True
    assert verified.json()["clientId"] == listed.json()[0]["id"]
    assert extension_verified.status_code == 200
    assert extension_verified.json()["ok"] is True
    assert_error(extension_missing_token, 401, "UNAUTHORIZED")
    assert internal.status_code == 200
    assert public_success.status_code == 200
    assert revoked.status_code == 200
    assert revoked.json()["status"] == "revoked"
    assert_error(after_revoke, 401, "UNAUTHORIZED")
    assert_error(verify_after_revoke, 401, "UNAUTHORIZED")
    assert_error(missing_origin, 401, "ORIGIN_REQUIRED")


@pytest.mark.asyncio
async def test_tc_api_002b_access_token_request_rejected_does_not_create_token() -> None:
    """TC-API-002B App 拒绝授权申请时不创建授权码，业务接口仍需有效 Bearer。"""

    async with api_client() as client:
        created = await client.post(
            "/v1/access-tokens",
            headers={"Origin": "http://127.0.0.1:4006"},
            json={"name": "人工授权码", "expiresAt": None},
        )
        auth_headers = {
            "Origin": "https://public.example",
            "Authorization": "Bearer {0}".format(created.json()["token"]),
        }
        before_reject = await client.get("/v1/access-tokens", headers=auth_headers)
        approval_rpc = FakeAccessTokenApprovalRpcClient(False)
        main.app.state.private_rpc = approval_rpc
        rejected = await client.post(
            "/v1/access-tokens/request",
            headers={"X-Request-ID": "token-request-rejected"},
            json={"name": "Chrome 插件", "expiresAt": None},
        )
        after_reject = await client.get("/v1/access-tokens", headers=auth_headers)
    assert rejected.status_code == 200
    assert rejected.json() == {"status": "rejected", "accessToken": None, "expiresAt": None}
    assert approval_rpc.calls == [
        (
            "requestAccessTokenApproval",
            "token-request-rejected",
            {"requestId": "token-request-rejected", "name": "Chrome 插件", "expiresAt": None},
        )
    ]
    assert "人工授权码" in [item["name"] for item in after_reject.json()]
    assert len(after_reject.json()) == len(before_reject.json())


@pytest.mark.asyncio
async def test_tc_api_003_audio_and_text_success_contracts() -> None:
    """TC-API-003 两个受保护接口成功映射固定 camelCase 响应契约。"""

    calls: list[dict[str, Any]] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(json.loads(request.content))
        return httpx.Response(200, json=success_payload(" mock output "))

    async with api_client(handler) as client:
        audio_headers = await session_headers(client, "audio-id")
        text_headers = await session_headers(client, "text-id")
        audio = await client.post(
            "/v1/audio/transcriptions",
            headers=audio_headers,
            json={
                "modelId": "fake-asr-id",
                "audioBase64": base64.b64encode(b"1234").decode(),
                "contentType": "audio/wav",
            },
        )
        text = await client.post(
            "/v1/text/process",
            headers=text_headers,
            json={
                "modelId": "fake-text-id",
                "mode": "polish",
                "text": "abcdefgh",
                "audioDurationMs": 0,
            },
        )
    assert audio.status_code == 200
    assert audio.json()["text"] == "mock output"
    assert set(audio.json()) == {"text", "elapsedMs", "modelId"}
    assert audio.json()["modelId"] == "fake-asr-id"
    assert text.status_code == 200
    assert text.json()["processedText"] == "mock output"
    assert set(text.json()) == {"processedText", "elapsedMs", "modelId"}
    assert text.json()["modelId"] == "fake-text-id"
    assert [call["model"] for call in calls] == ["fake-asr-model", "fake-text-model"]


@pytest.mark.asyncio
async def test_tc_api_003a_safe_model_catalog_and_empty_catalog(
    settings_factory: object,
) -> None:
    """TC-API-003A 模型目录要求短 Token、仅返回安全字段，且空目录返回空数组。

    参数：``settings_factory`` 构造不含真实密钥的隔离配置。
    返回：无；通过真实 ASGI 请求断言鉴权、字段白名单和空目录契约。
    异常边界：响应正文不得包含 provider、baseUrl、modelName、apiKey 或测试私有值。
    """

    async with api_client() as client:
        unauthorized = await client.get("/v1/models")
        headers = await session_headers(client)
        configured = await client.get("/v1/models", headers=headers)
        current_service = main.app.state.model_service
        main.app.state.model_service = ModelService(
            settings_factory(model_catalog=()), current_service.client
        )  # type: ignore[operator]
        empty = await client.get("/v1/models", headers=headers)
    assert_error(unauthorized, 401, "ORIGIN_REQUIRED")
    assert configured.status_code == 200
    assert configured.json() == [
        {
            "id": "fake-asr-id",
            "displayName": "Fake ASR",
            "capability": "asr",
            "enabled": True,
            "isDefault": True,
        },
        {
            "id": "fake-text-id",
            "displayName": "Fake Text",
            "capability": "text",
            "enabled": True,
            "isDefault": True,
        },
    ]
    assert all(
        private_name not in configured.text
        for private_name in ("provider", "baseUrl", "modelName", "apiKey")
    )
    assert "fake-local" not in configured.text
    assert empty.json() == []


@pytest.mark.asyncio
async def test_tc_api_003a2_internal_model_catalog_reload_requires_secret() -> None:
    """TC-API-003A2 内部模型目录热更新必须校验私有密钥，成功后公开目录读取新内存状态。"""

    async with api_client() as client:
        main.app.state.private_rpc = FakeAccessTokenApprovalRpcClient(True)
        missing_secret = await client.post(
            "/internal/model-catalog/reload",
            json={"modelCatalog": []},
        )
        wrong_secret = await client.post(
            "/internal/model-catalog/reload",
            headers={"X-CodexMan-Internal-Secret": "wrong-secret"},
            json={"modelCatalog": []},
        )
        reloaded = await client.post(
            "/internal/model-catalog/reload",
            headers={
                "X-CodexMan-Internal-Secret": "test-control-secret-000000000000000000"
            },
            json={
                "modelCatalog": [
                    {
                        "id": "hot-text-id",
                        "displayName": "Hot Text",
                        "capability": "text",
                        "enabled": True,
                        "isDefault": True,
                        "provider": "openai-compatible",
                        "baseUrl": "https://example.com/v1",
                        "modelName": "hot-text-model",
                        "apiKey": "hot-key",
                    }
                ]
            },
        )
        headers = await session_headers(client)
        models = await client.get("/v1/models", headers=headers)
    assert_error(missing_secret, 401, "UNAUTHORIZED")
    assert_error(wrong_secret, 401, "UNAUTHORIZED")
    assert reloaded.status_code == 200
    assert reloaded.json() == {"ok": True}
    assert models.status_code == 200
    assert models.json() == [
        {
            "id": "hot-text-id",
            "displayName": "Hot Text",
            "capability": "text",
            "enabled": True,
            "isDefault": True,
        }
    ]
    assert "hot-key" not in models.text


@pytest.mark.parametrize(
    ("catalog_mode", "model_id", "expected_status", "expected_code"),
    [
        ("empty", "missing-id", 503, "MODEL_NOT_CONFIGURED"),
        ("normal", "missing-id", 404, "MODEL_NOT_FOUND"),
        ("disabled", "disabled-text-id", 409, "MODEL_DISABLED"),
        ("normal", "fake-asr-id", 409, "MODEL_CAPABILITY_MISMATCH"),
    ],
)
@pytest.mark.asyncio
async def test_tc_api_003b_model_selection_errors(
    settings_factory: object,
    catalog_mode: str,
    model_id: str,
    expected_status: int,
    expected_code: str,
) -> None:
    """TC-API-003B 文本接口稳定区分空目录、未知、禁用和能力不匹配。

    参数：目录模式和 modelId 构造四类选择失败，预期状态与错误码用于逐项断言。
    返回：无；通过真实鉴权、限额和路由链路验证统一错误 envelope。
    异常边界：所有分支都在上游调用前失败，错误正文不得包含私有目录配置。
    """

    settings = settings_factory()  # type: ignore[operator]
    if catalog_mode == "empty":
        settings = settings_factory(model_catalog=())  # type: ignore[operator]
    elif catalog_mode == "disabled":
        settings = settings_factory(  # type: ignore[operator]
            model_catalog=(
                ModelCatalogItem(
                    id="disabled-text-id",
                    display_name="Disabled Text",
                    capability="text",
                    enabled=False,
                    is_default=False,
                    provider="openai-compatible",
                    base_url="https://private-upstream.invalid/v1",
                    model_name="private-model",
                    api_key="private-secret",
                ),
            )
        )
    async with api_client() as client:
        current_service = main.app.state.model_service
        main.app.state.model_service = ModelService(settings, current_service.client)
        headers = await session_headers(client)
        response = await client.post(
            "/v1/text/process",
            headers=headers,
            json={
                "modelId": model_id,
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
            },
        )
    assert_error(response, expected_status, expected_code)
    assert "private" not in response.text
    if expected_code == "MODEL_NOT_CONFIGURED":
        assert "retry-after" not in response.headers


@pytest.mark.parametrize(
    ("path", "payload", "status", "code"),
    [
        (
            "/v1/audio/transcriptions",
            {
                "modelId": "fake-asr-id",
                "audioBase64": "MQ==",
                "contentType": "text/plain",
            },
            400,
            "UNSUPPORTED_AUDIO_TYPE",
        ),
        (
            "/v1/audio/transcriptions",
            {
                "modelId": "fake-asr-id",
                "audioBase64": "%%%",
                "contentType": "audio/wav",
            },
            400,
            "INVALID_AUDIO_BASE64",
        ),
        (
            "/v1/audio/transcriptions",
            {"modelId": "fake-asr-id", "audioBase64": "", "contentType": "audio/wav"},
            400,
            "EMPTY_AUDIO",
        ),
        (
            "/v1/audio/transcriptions",
            {
                "modelId": "fake-asr-id",
                "audioBase64": base64.b64encode(
                    b"x" * (PUBLIC_MAX_AUDIO_BYTES + 1)
                ).decode(),
                "contentType": "audio/wav",
            },
            413,
            "AUDIO_TOO_LARGE",
        ),
        (
            "/v1/text/process",
            {
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "x" * (PUBLIC_MAX_TEXT_CHARS + 1),
                "audioDurationMs": 0,
            },
            413,
            "TEXT_TOO_LARGE",
        ),
        (
            "/v1/text/process",
            {
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "",
                "audioDurationMs": 0,
            },
            422,
            "VALIDATION_ERROR",
        ),
        (
            "/v1/text/process",
            {
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
                "dictionary": ["x" * 101],
            },
            400,
            "INVALID_DICTIONARY",
        ),
        (
            "/v1/text/process",
            {
                "modelId": "fake-text-id",
                "mode": "invalid",
                "text": "ok",
                "audioDurationMs": 0,
            },
            422,
            "VALIDATION_ERROR",
        ),
        (
            "/v1/text/process",
            {
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
                "model": "client-model",
            },
            422,
            "VALIDATION_ERROR",
        ),
        (
            "/v1/audio/transcriptions",
            {
                "modelId": "fake-asr-id",
                "audioBase64": "MQ==",
                "contentType": "audio/wav",
                "apiKey": "fake-client-key",
            },
            422,
            "VALIDATION_ERROR",
        ),
    ],
)
@pytest.mark.asyncio
async def test_tc_api_004_boundary_and_validation_errors(
    path: str, payload: dict[str, object], status: int, code: str
) -> None:
    """TC-API-004 音频、文本极限和字段校验保持稳定错误码。"""

    async with api_client(lambda request: httpx.Response(500)) as client:
        response = await client.post(
            path, headers=await session_headers(client), json=payload
        )
    assert_error(response, status, code)


@pytest.mark.asyncio
async def test_tc_api_005_body_limit_declared() -> None:
    """TC-API-005 声明长度超过上限时在 JSON 解析前返回 413。"""

    oversized = b"x" * (PUBLIC_MAX_BODY_BYTES + 1)
    async with api_client() as client:
        declared = await client.post(
            "/v1/text/process",
            headers={
                "Content-Type": "application/json",
                "X-Request-ID": "body-declared",
            },
            content=oversized,
        )

    assert_error(declared, 413, "REQUEST_BODY_TOO_LARGE")
    assert declared.json()["error"]["requestId"] == "body-declared"


@pytest.mark.asyncio
async def test_tc_api_006_concurrency_wait_timeout() -> None:
    """TC-API-006 并发额度被占用超过等待窗口时第二个请求稳定返回 429。"""

    entered = asyncio.Event()
    release = asyncio.Event()

    async def handler(request: httpx.Request) -> httpx.Response:
        entered.set()
        await release.wait()
        return httpx.Response(200, json=success_payload())

    async with api_client(handler) as client:
        auth_headers = await session_headers(client)
        first = asyncio.create_task(
            client.post(
                "/v1/text/process",
                headers=auth_headers,
                json={
                    "modelId": "fake-text-id",
                    "mode": "dictate",
                    "text": "ok",
                    "audioDurationMs": 0,
                },
            )
        )
        await entered.wait()
        second = await client.post(
            "/v1/text/process",
            headers=auth_headers,
            json={
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
            },
        )
        release.set()
        first_response = await first
    assert first_response.status_code == 200
    assert_error(second, 429, "CONCURRENCY_LIMIT")
    assert second.headers["retry-after"] == "1"


@pytest.mark.asyncio
async def test_tc_api_006a_client_rate_limit(tmp_path: object) -> None:
    """TC-API-006A 同一短期 Token 超过分钟额度返回 RATE_LIMIT 和 Retry-After。"""

    async with api_client(
        lambda request: httpx.Response(200, json=success_payload())
    ) as client:
        auth_headers = await session_headers(client)
        main.app.state.client_rate_limiter = ClientRateLimiter(
            per_minute=1,
            daily_quota=10,
            database_file=str(tmp_path / "api-rate.sqlite3"),  # type: ignore[operator]
        )
        first = await client.post(
            "/v1/text/process",
            headers=auth_headers,
            json={
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
            },
        )
        second = await client.post(
            "/v1/text/process",
            headers=auth_headers,
            json={
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
            },
        )
    assert first.status_code == 200
    assert_error(second, 429, "RATE_LIMIT")
    assert int(second.headers["retry-after"]) >= 1


@pytest.mark.parametrize(
    ("handler", "status", "code"),
    [
        (
            lambda request: httpx.Response(503, text="fake private detail"),
            502,
            "UPSTREAM_REJECTED",
        ),
        (
            lambda request: (_ for _ in ()).throw(
                httpx.ReadTimeout("fake timeout detail", request=request)
            ),
            504,
            "UPSTREAM_TIMEOUT",
        ),
    ],
)
@pytest.mark.asyncio
async def test_tc_api_007_upstream_errors_are_stable_and_redacted(
    handler: Callable[[httpx.Request], httpx.Response], status: int, code: str
) -> None:
    """TC-API-007 上游错误和超时不透传响应正文或异常详情。"""

    async with api_client(handler) as client:
        auth_headers = await session_headers(client)
        response = await client.post(
            "/v1/text/process",
            headers=auth_headers,
            json={
                "modelId": "fake-text-id",
                "mode": "dictate",
                "text": "ok",
                "audioDurationMs": 0,
            },
        )
    assert_error(response, status, code)
    assert "fake" not in response.text
    assert "private" not in response.text


@pytest.mark.asyncio
async def test_tc_api_008_framework_and_unexpected_errors(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-API-008 404、405、其他 HTTP 与未知异常均使用统一脱敏 envelope。"""

    async with api_client() as client:
        missing = await client.get("/missing")
        wrong_method = await client.get("/v1/text/process")
    assert_error(missing, 404, "NOT_FOUND")
    assert_error(wrong_method, 405, "METHOD_NOT_ALLOWED")

    scope = {"type": "http", "app": main.app, "state": {}}
    request = Request(scope)
    other_http = await main.handle_http_error(
        request, StarletteHTTPException(418, "fake detail")
    )
    assert json.loads(other_http.body)["error"]["code"] == "HTTP_ERROR"
    unexpected = await main.handle_unexpected_error(
        request, RuntimeError("fake secret failure")
    )
    assert unexpected.status_code == 500
    assert json.loads(unexpected.body)["error"] == {
        "code": "INTERNAL_ERROR",
        "message": "服务内部错误。",
        "requestId": "unknown",
        "retryable": False,
    }


@pytest.mark.asyncio
async def test_tc_api_008a_access_token_expiry_revoke_and_invalid_id() -> None:
    """TC-API-008A 授权码过期、撤销幂等和未知 ID 返回稳定接口结果。"""

    async with api_client() as client:
        main.app.state.private_rpc = FakeAccessTokenApprovalRpcClient(True)
        expired = await client.post(
            "/v1/access-tokens/request",
            json={"name": "过期授权码", "expiresAt": "2020-01-01T00:00:00Z"},
        )
        permanent = await client.post(
            "/v1/access-tokens/request",
            json={"name": "永久授权码", "expiresAt": None},
        )
        headers = {
            "Origin": "https://public.example",
            "Authorization": "Bearer {0}".format(permanent.json()["accessToken"]),
        }
        missing = await client.post(
            "/v1/access-tokens/missing-token/revoke", headers=headers
        )
        created = await client.post(
            "/v1/access-tokens",
            headers=headers,
            json={"name": "手动授权码", "expiresAt": None},
        )
        first_revoke = await client.post(
            "/v1/access-tokens/{0}/revoke".format(created.json()["id"]),
            headers=headers,
        )
        second_revoke = await client.post(
            "/v1/access-tokens/{0}/revoke".format(created.json()["id"]),
            headers=headers,
        )
    assert_error(expired, 422, "VALIDATION_ERROR")
    assert permanent.status_code == 200
    assert_error(missing, 404, "ACCESS_TOKEN_NOT_FOUND")
    assert created.status_code == 200
    assert created.json()["token"].startswith("typesass_")
    assert first_revoke.status_code == 200
    assert second_revoke.status_code == 200
    assert first_revoke.json()["revokedAt"] == second_revoke.json()["revokedAt"]


@pytest.mark.asyncio
async def test_tc_api_008c_access_logs_real_error_codes(
    caplog: pytest.LogCaptureFixture,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """TC-API-008C 真实 422、413、404、500 请求访问日志记录统一 errorCode。"""

    async def fail_process(*args: object, **kwargs: object) -> tuple[str, int, str]:
        raise RuntimeError("fake internal failure")

    async with api_client(raise_app_exceptions=False) as client:
        root_logger = logging.getLogger()
        root_logger.addHandler(caplog.handler)
        try:
            with caplog.at_level("INFO", logger="aitool.access"):
                validation = await client.post("/v1/access-tokens/request", json={})
                too_large = await client.post(
                    "/v1/text/process",
                    headers={"Content-Type": "application/json"},
                    content=b"x" * (PUBLIC_MAX_BODY_BYTES + 1),
                )
                missing = await client.get("/missing-log-target")
                auth_headers = await session_headers(client)
                monkeypatch.setattr(
                    main.app.state.model_service, "process_text", fail_process
                )
                internal = await client.post(
                    "/v1/text/process",
                    headers=auth_headers,
                    json={
                        "modelId": "fake-text-id",
                        "mode": "dictate",
                        "text": "ok",
                        "audioDurationMs": 0,
                    },
                )
        finally:
            root_logger.removeHandler(caplog.handler)
    assert_error(validation, 422, "VALIDATION_ERROR")
    assert_error(too_large, 413, "REQUEST_BODY_TOO_LARGE")
    assert_error(missing, 404, "NOT_FOUND")
    assert_error(internal, 500, "INTERNAL_ERROR")
    access_contexts = [
        record.context  # type: ignore[attr-defined]
        for record in caplog.records
        if record.name == "aitool.access"
    ]
    observed = {
        (context["statusCode"], context["errorCode"]) for context in access_contexts
    }
    assert {
        (422, "VALIDATION_ERROR"),
        (413, "REQUEST_BODY_TOO_LARGE"),
        (404, "NOT_FOUND"),
        (500, "INTERNAL_ERROR"),
    } <= observed


@pytest.mark.asyncio
async def test_tc_api_009_direct_handlers_and_dependency_release(
    settings_factory: object, tmp_path: object
) -> None:
    """TC-API-009 业务/校验 handler 和并发依赖成功路径均覆盖稳定行为。"""

    scope = {
        "type": "http",
        "app": main.app,
        "state": {"request_id": "direct-id"},
        "headers": [(b"origin", b"https://public.example")],
    }
    request = Request(scope)
    api_response = await main.handle_api_error(
        request,
        ApiError(400, "DIRECT", "safe", {"X-Test-Contract": "present"}),
    )
    assert json.loads(api_response.body)["error"]["code"] == "DIRECT"
    assert api_response.headers["x-test-contract"] == "present"
    validation = RequestValidationError(
        [{"type": "missing", "loc": ("body", "text"), "msg": "required", "input": {}}]
    )
    validation_response = await main.handle_validation_error(request, validation)
    assert validation_response.status_code == 422

    token_service = AppAccessTokenService(str(tmp_path / "direct-token.sqlite3"))  # type: ignore[operator]
    token = str(token_service.create("direct", None)["token"])

    class Credential:
        credentials = token

    main.app.state.app_access_tokens = token_service
    assert await main.require_api_access(request, Credential())  # type: ignore[arg-type]
    assert str(request.state.client_id).startswith("token_")
    main.app.state.settings = settings_factory(  # type: ignore[operator]
        enable_dev_bearer_token=True,
        dev_bearer_token="codexman-dev-bearer-token-000000000001",
    )

    class DevCredential:
        credentials = "codexman-dev-bearer-token-000000000001"

    assert await main.require_api_access(request, DevCredential()) == "dev-access-token"  # type: ignore[arg-type]
    assert request.state.client_id == "dev-access-token"
    main.app.state.client_rate_limiter = ClientRateLimiter(
        10,
        10,
        str(tmp_path / "direct.sqlite3"),  # type: ignore[operator]
    )
    await main.limit_client_rate(request, CLIENT_ID)
    generator = main.limit_concurrency(request)
    await generator.__anext__()
    assert main.app.state.concurrency.locked()
    await generator.aclose()
    assert not main.app.state.concurrency.locked()


@pytest.mark.contract
@pytest.mark.asyncio
async def test_tc_api_010_openapi_contract() -> None:
    """TC-API-010 OpenAPI 暴露鉴权、模型目录与 AI 路径，并声明完整错误契约。"""

    async with api_client() as client:
        response = await client.get("/openapi.json")
    assert response.status_code == 200
    schema = response.json()
    assert schema["info"]["title"] == "CodexMan AI API"
    assert schema["info"]["version"] == "1.0.0"
    assert "App 授权码管理" in schema["info"]["description"]
    assert "缺失 Origin 的业务请求直接拦截" in schema["info"]["description"]
    assert schema["servers"] == [
        {"url": "http://127.0.0.1:18080", "description": "固定本机 sidecar"}
    ]
    assert set(schema["paths"]) == {
        "/health",
        "/v1/access-tokens",
        "/v1/access-tokens/verify",
        "/v1/access-tokens/request",
        "/v1/access-tokens/{tokenId}/revoke",
        "/v1/models",
        "/v1/audio/transcriptions",
        "/v1/text/process",
        "/v1/codex/connection",
        "/v1/codex/connection/restart",
        "/v1/codex/workspaces",
        "/v1/codex/threads/search",
        "/v1/codex/threads/{threadId}/open",
        "/v1/task-workspace/query",
        "/v1/projects",
        "/v1/projects/{projectId}/update",
        "/v1/projects/{projectId}/delete",
        "/v1/tasks",
        "/v1/tasks/{taskId}/queue",
        "/v1/tasks/{taskId}/complete",
        "/v1/tasks/{taskId}/update",
        "/v1/tasks/{taskId}/delete",
    }
    assert set(schema["paths"]["/health"]) == {"get"}
    health_parameters = schema["paths"]["/health"]["get"]["parameters"]
    assert any(
        parameter["name"] == "X-Request-ID" and parameter["in"] == "header"
        for parameter in health_parameters
    )
    request_token_operation = schema["paths"]["/v1/access-tokens/request"]["post"]
    assert "security" not in request_token_operation
    assert request_token_operation["x-error-codes"]["422"][0]["code"] == "VALIDATION_ERROR"
    access_token_operation = schema["paths"]["/v1/access-tokens"]["post"]
    assert access_token_operation["security"] == [{"AppAccessToken": []}]
    assert schema["paths"]["/v1/access-tokens"]["get"]["security"] == [
        {"AppAccessToken": []}
    ]
    assert schema["paths"]["/v1/access-tokens/verify"]["get"]["security"] == [
        {"AppAccessToken": []}
    ]
    assert schema["paths"]["/v1/access-tokens/{tokenId}/revoke"]["post"][
        "security"
    ] == [{"AppAccessToken": []}]
    models_operation = schema["paths"]["/v1/models"]["get"]
    assert models_operation["security"] == [{"AppAccessToken": []}]
    assert set(models_operation["responses"]) == {"200", "401", "413", "500"}
    expected_common_codes = {
        "UNAUTHORIZED",
        "VALIDATION_ERROR",
        "CONCURRENCY_LIMIT",
        "INTERNAL_ERROR",
        "UPSTREAM_TIMEOUT",
        "UPSTREAM_UNAVAILABLE",
        "UPSTREAM_REJECTED",
        "UPSTREAM_INVALID_RESPONSE",
        "UPSTREAM_EMPTY_RESULT",
        "MODEL_NOT_CONFIGURED",
        "MODEL_NOT_FOUND",
        "MODEL_DISABLED",
        "MODEL_CAPABILITY_MISMATCH",
    }
    for path in ("/v1/audio/transcriptions", "/v1/text/process"):
        operation = schema["paths"][path]["post"]
        assert operation["security"] == [{"AppAccessToken": []}]
        assert {"200", "400", "401", "413", "422", "429", "500", "502", "504"} <= set(
            operation["responses"]
        )
        documented_codes = {
            entry["code"]
            for entries in operation["x-error-codes"].values()
            for entry in entries
        }
        assert expected_common_codes <= documented_codes
        assert all(
            set(entry) == {"code", "retryable", "action"}
            for entries in operation["x-error-codes"].values()
            for entry in entries
        )
        assert "X-Request-ID" in operation["responses"]["401"]["headers"]
        assert "WWW-Authenticate" in operation["responses"]["401"]["headers"]
        assert "Retry-After" in operation["responses"]["429"]["headers"]
        assert "Retry-After" in operation["responses"]["503"]["headers"]
        assert operation["responses"]["503"]["headers"]["Retry-After"][
            "description"
        ] == (
            "仅当错误码表示过载或配额存储暂不可用时返回；MODEL_NOT_CONFIGURED 不返回此 Header。"
        )
        assert operation["responses"]["429"]["headers"]["Retry-After"]["schema"] == {
            "type": "integer",
            "minimum": 1,
        }
        assert any(
            parameter["name"] == "X-Request-ID" and parameter["in"] == "header"
            for parameter in operation["parameters"]
        )
    assert {
        "UNSUPPORTED_AUDIO_TYPE",
        "INVALID_AUDIO_BASE64",
        "EMPTY_AUDIO",
        "AUDIO_TOO_LARGE",
    } <= {
        entry["code"]
        for entries in schema["paths"]["/v1/audio/transcriptions"]["post"][
            "x-error-codes"
        ].values()
        for entry in entries
    }
    assert {"TEXT_TOO_LARGE", "INVALID_DICTIONARY"} <= {
        entry["code"]
        for entries in schema["paths"]["/v1/text/process"]["post"][
            "x-error-codes"
        ].values()
        for entry in entries
    }
    components = schema["components"]["schemas"]
    assert components["AudioTranscriptionRequest"]["required"] == [
        "modelId",
        "audioBase64",
        "contentType",
    ]
    assert {"modelId", "mode", "text", "audioDurationMs"} <= set(
        components["TextProcessRequest"]["required"]
    )
    assert components["ErrorDetail"]["required"] == [
        "code",
        "message",
        "requestId",
        "retryable",
    ]
    assert components["AudioTranscriptionRequest"]["additionalProperties"] is False
    assert components["TextProcessRequest"]["additionalProperties"] is False
    assert components["AccessTokenRequestResponse"]["required"] == ["status"]
    assert components["AppAccessTokenResponse"]["required"] == [
        "id",
        "name",
        "token",
        "status",
        "createdAt",
    ]
    assert components["ModelCatalogResponse"]["required"] == [
        "id",
        "displayName",
        "capability",
        "enabled",
        "isDefault",
    ]
    assert set(components["ModelCatalogResponse"]["properties"]) == {
        "id",
        "displayName",
        "capability",
        "enabled",
        "isDefault",
    }
    for path_item in schema["paths"].values():
        for operation in path_item.values():
            success_status = "200" if "200" in operation["responses"] else "202"
            success = operation["responses"][success_status]
            assert "X-Request-ID" in success["headers"]
            assert "example" in success["content"]["application/json"]
            assert {"413", "500"} <= set(operation["responses"])
            assert (
                operation["x-error-codes"]["413"][0]["code"] == "REQUEST_BODY_TOO_LARGE"
            )
            assert operation["x-error-codes"]["500"][0]["code"] == "INTERNAL_ERROR"
            assert all(
                entry["retryable"] is is_retryable_error(entry["code"])
                for entries in operation["x-error-codes"].values()
                for entry in entries
            )
            request_id_parameters = [
                parameter
                for parameter in operation["parameters"]
                if parameter["name"] == "X-Request-ID" and parameter["in"] == "header"
            ]
            assert request_id_parameters == [
                {
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
            ]
    for path in ("/health", "/v1/models"):
        operation = next(iter(schema["paths"][path].values()))
        assert "422" not in operation["responses"]
        assert "422" not in operation["x-error-codes"]
    for path in (
        "/v1/access-tokens",
        "/v1/access-tokens/request",
        "/v1/audio/transcriptions",
        "/v1/text/process",
    ):
        operation = schema["paths"][path]["post"]
        assert "422" in operation["responses"]
        assert operation["x-error-codes"]["422"][0]["code"] == "VALIDATION_ERROR"

    for path in ("/v1/audio/transcriptions", "/v1/text/process"):
        retry_codes = {
            entry["code"]: entry["retryable"]
            for entries in schema["paths"][path]["post"]["x-error-codes"].values()
            for entry in entries
        }
        assert retry_codes["DAILY_QUOTA_EXCEEDED"] is False
        assert retry_codes["RATE_LIMIT"] is True
        assert retry_codes["CONCURRENCY_LIMIT"] is True
        assert retry_codes["UPSTREAM_UNAVAILABLE"] is True
        assert retry_codes["UPSTREAM_REJECTED"] is False


def test_tc_api_011_model_serialization_contracts() -> None:
    """TC-API-011 请求响应和错误模型使用约定别名并保留默认字段。"""

    from app.models import (
        AudioTranscriptionResponse,
        ErrorDetail,
        ErrorEnvelope,
        TextProcessRequest,
        TextProcessResponse,
    )

    request = TextProcessRequest(
        modelId="fake-text-id", mode="dictate", text="ok", audioDurationMs=0
    )
    assert request.dictionary == []
    assert request.context_app == ""
    assert request.style_instruction == ""
    assert (
        AudioTranscriptionResponse(text="ok", elapsedMs=0, modelId="m").model_dump(
            by_alias=True
        )["modelId"]
        == "m"
    )
    assert (
        TextProcessResponse(processedText="ok", elapsedMs=0, modelId="m").model_dump(
            by_alias=True
        )["modelId"]
        == "m"
    )
    envelope = ErrorEnvelope(
        error=ErrorDetail(code="E", message="safe", requestId="id", retryable=False)
    )
    assert envelope.model_dump(by_alias=True)["error"]["requestId"] == "id"
    assert envelope.model_dump(by_alias=True)["error"]["retryable"] is False
