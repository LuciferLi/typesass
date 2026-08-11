"""目录驱动上游服务单元测试，所有 HTTP 均使用 MockTransport。"""

import base64
from collections.abc import Callable
from typing import Any, Optional

import httpx
import pytest

from app.errors import ApiError
from app.models import AudioTranscriptionRequest, TextProcessRequest
from app.service import ModelService


def completion(
    content: object = " result ", model: Optional[str] = "returned-model"
) -> dict[str, object]:
    """构造 OpenAI 兼容的假成功响应。"""

    return {"choices": [{"message": {"content": content}}], "model": model}


@pytest.mark.parametrize(
    ("content_type", "audio_bytes"),
    [
        ("audio/wav", b"wav1"),
        ("audio/webm", b"webm"),
        ("audio/mpeg", b"mpeg"),
        ("audio/mp4", b"mp41"),
        ("audio/ogg", b"ogg1"),
    ],
)
@pytest.mark.asyncio
async def test_tc_svc_001_transcription_success_and_payload(
    settings_factory: object,
    content_type: str,
    audio_bytes: bytes,
) -> None:
    """验证五种正式音频 MIME 均按历史 Mimo data URL 协议发送。

    流程：分别编码五种允许的音频载荷，通过 MockTransport 捕获唯一上游请求，再完整核对模型、消息、语言选项和音频对象。
    参数：``settings_factory`` 提供隔离模型目录，``content_type`` 与 ``audio_bytes`` 来自正式 MIME 参数矩阵。
    返回：无；断言转写结果、固定上游地址、Bearer 鉴权和请求 JSON 全部符合契约。
    异常边界：``input_audio`` 只能包含 data URL，不得携带 format、额外文本、temperature、token 或 stream 等生成参数。
    """

    captured: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        captured.append(request)
        return httpx.Response(200, json=completion(" transcript ", None))

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        service = ModelService(settings_factory(), client)  # type: ignore[operator]
        audio_base64 = base64.b64encode(audio_bytes).decode()
        result = await service.transcribe(
            AudioTranscriptionRequest(
                modelId="fake-asr-id",
                audioBase64=audio_base64,
                contentType=content_type,
                language="zh",
            ),
            "req-1",
        )
    assert result[0] == "transcript"
    assert result[1] >= 0
    assert result[2] == "fake-asr-id"
    assert str(captured[0].url) == "https://upstream.invalid/v1/chat/completions"
    assert captured[0].headers["authorization"] == "Bearer fake-local-asr-key"
    body = __import__("json").loads(captured[0].content)
    assert body == {
        "model": "fake-asr-model",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": f"data:{content_type};base64,{audio_base64}"
                        },
                    }
                ],
            }
        ],
        "asr_options": {"language": "zh"},
    }
    input_audio = body["messages"][0]["content"][0]["input_audio"]
    assert set(input_audio) == {"data"}
    assert "format" not in input_audio
    assert not {
        "temperature",
        "max_tokens",
        "max_completion_tokens",
        "stream",
    }.intersection(body)


@pytest.mark.parametrize(
    ("payload", "code"),
    [
        (
            {"audioBase64": "MQ==", "contentType": "text/plain"},
            "UNSUPPORTED_AUDIO_TYPE",
        ),
        ({"audioBase64": "%%%", "contentType": "audio/wav"}, "INVALID_AUDIO_BASE64"),
        ({"audioBase64": "", "contentType": "audio/wav"}, "EMPTY_AUDIO"),
        (
            {
                "audioBase64": base64.b64encode(b"12345").decode(),
                "contentType": "audio/wav",
            },
            "AUDIO_TOO_LARGE",
        ),
    ],
)
@pytest.mark.asyncio
async def test_tc_svc_002_audio_boundaries(
    settings_factory: object, payload: dict[str, str], code: str
) -> None:
    """验证非法音频在调用上游前返回稳定错误码。

    流程：构造非法 MIME、base64、空内容和超限音频，通过记录型 MockTransport 执行转写并捕获业务异常。
    参数：``settings_factory`` 提供隔离配置，``payload`` 与 ``code`` 分别表示非法请求和期望错误码。
    返回：无；断言错误码精确匹配且 MockTransport 请求列表始终为空。
    异常边界：尤其保证 ``text/plain`` 不会被包装成 input_audio，也不会产生任何上游 HTTP 调用。
    """

    upstream_requests: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        upstream_requests.append(request)
        return httpx.Response(500)

    request = AudioTranscriptionRequest.model_construct(
        model_id="fake-asr-id",
        audio_base64=payload["audioBase64"],
        content_type=payload["contentType"],
        language="auto",
    )
    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        with pytest.raises(ApiError) as captured:
            await ModelService(settings_factory(), client).transcribe(request, "req")  # type: ignore[operator]
    assert captured.value.code == code
    assert upstream_requests == []


@pytest.mark.asyncio
async def test_tc_svc_003_transcription_language_branches(
    settings_factory: object,
) -> None:
    """TC-SVC-003 自动、空值和 None 语言均不发送上游语言选项。"""

    bodies: list[dict[str, Any]] = []

    def handler(request: httpx.Request) -> httpx.Response:
        bodies.append(__import__("json").loads(request.content))
        return httpx.Response(200, json=completion())

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        service = ModelService(settings_factory(), client)  # type: ignore[operator]
        for language in ("auto", " ", None):
            request = AudioTranscriptionRequest.model_construct(
                model_id="fake-asr-id",
                audio_base64="MQ==",
                content_type="audio/wav",
                language=language,
            )
            await service.transcribe(request, "req")
    assert all("asr_options" not in body for body in bodies)


@pytest.mark.asyncio
async def test_tc_svc_004_text_modes_context_and_limits(
    settings_factory: object,
) -> None:
    """TC-SVC-004 两种文本模式、可选上下文、最大 token 分支和模型回退均正确。"""

    bodies: list[dict[str, Any]] = []

    def handler(request: httpx.Request) -> httpx.Response:
        bodies.append(__import__("json").loads(request.content))
        return httpx.Response(200, json=completion(" polished ", None))

    settings = settings_factory(max_text_chars=3000)  # type: ignore[operator]
    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        service = ModelService(settings, client)
        dictate = TextProcessRequest(
            modelId="fake-text-id", mode="dictate", text=" short ", audioDurationMs=0
        )
        polish = TextProcessRequest(
            modelId="fake-text-id",
            mode="polish",
            text="x" * 3000,
            audioDurationMs=100,
            dictionary=["术语"],
            contextApp=" editor ",
            styleInstruction=" concise ",
        )
        first = await service.process_text(dictate, "req-1")
        second = await service.process_text(polish, "req-2")
    assert first[0] == second[0] == "polished"
    assert first[2] == second[2] == "fake-text-id"
    assert all(
        set(body) == {"model", "messages", "temperature", "max_completion_tokens"}
        for body in bodies
    )
    assert "整理听写内容" in bodies[0]["messages"][0]["content"]
    assert bodies[0]["max_completion_tokens"] == 256
    assert "润色文字" in bodies[1]["messages"][0]["content"]
    assert "词典：术语" in bodies[1]["messages"][1]["content"]
    assert "上下文应用：editor" in bodies[1]["messages"][1]["content"]
    assert "风格要求：concise" in bodies[1]["messages"][1]["content"]
    assert bodies[1]["max_completion_tokens"] == 4096


@pytest.mark.parametrize(
    ("payload", "code"),
    [
        (
            TextProcessRequest(
                modelId="fake-text-id",
                mode="dictate",
                text="123456789",
                audioDurationMs=0,
            ),
            "TEXT_TOO_LARGE",
        ),
        (
            TextProcessRequest.model_construct(
                mode="polish",
                model_id="fake-text-id",
                text="ok",
                audio_duration_ms=0,
                dictionary=["x" * 101],
                context_app="",
                style_instruction="",
            ),
            "INVALID_DICTIONARY",
        ),
    ],
)
@pytest.mark.asyncio
async def test_tc_svc_005_text_boundaries(
    settings_factory: object, payload: TextProcessRequest, code: str
) -> None:
    """TC-SVC-005 文本字符数和词典单项长度越界返回稳定错误码。"""

    async with httpx.AsyncClient(
        transport=httpx.MockTransport(lambda request: httpx.Response(500))
    ) as client:
        with pytest.raises(ApiError) as captured:
            await ModelService(settings_factory(), client).process_text(payload, "req")  # type: ignore[operator]
    assert captured.value.code == code


@pytest.mark.parametrize(
    ("handler", "code"),
    [
        (
            lambda request: (_ for _ in ()).throw(
                httpx.ReadTimeout("late", request=request)
            ),
            "UPSTREAM_TIMEOUT",
        ),
        (
            lambda request: (_ for _ in ()).throw(
                httpx.ConnectError("down", request=request)
            ),
            "UPSTREAM_UNAVAILABLE",
        ),
        (
            lambda request: httpx.Response(429, text="fake sensitive upstream body"),
            "UPSTREAM_REJECTED",
        ),
        (
            lambda request: httpx.Response(200, text="not-json"),
            "UPSTREAM_INVALID_RESPONSE",
        ),
        (
            lambda request: httpx.Response(200, json=["not-object"]),
            "UPSTREAM_INVALID_RESPONSE",
        ),
    ],
)
@pytest.mark.asyncio
async def test_tc_svc_006_upstream_error_mapping(
    settings_factory: object,
    handler: Callable[[httpx.Request], httpx.Response],
    code: str,
) -> None:
    """TC-SVC-006 上游超时、网络、状态和格式错误映射为稳定脱敏错误码。"""

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        service = ModelService(settings_factory(), client)  # type: ignore[operator]
        model = service.list_models()[0]
        with pytest.raises(ApiError) as captured:
            await service._chat_completion(model, {"model": "fake"}, "req")
    assert captured.value.code == code
    assert "sensitive" not in captured.value.message


@pytest.mark.parametrize(
    ("catalog_override", "model_id", "capability", "code"),
    [
        ((), "missing", "text", "MODEL_NOT_CONFIGURED"),
        (None, "missing", "text", "MODEL_NOT_FOUND"),
        ("disabled", "disabled-id", "text", "MODEL_DISABLED"),
        (None, "fake-asr-id", "text", "MODEL_CAPABILITY_MISMATCH"),
    ],
)
def test_tc_svc_007_model_resolution_errors(
    settings_factory: object,
    catalog_override: object,
    model_id: str,
    capability: str,
    code: str,
) -> None:
    """TC-SVC-007 模型目录空、未知、禁用和能力不匹配返回稳定错误码。"""

    settings = settings_factory()  # type: ignore[operator]
    if catalog_override == ():
        settings = settings_factory(model_catalog=())  # type: ignore[operator]
    elif catalog_override == "disabled":
        disabled = settings.model_catalog[1]
        settings = settings_factory(  # type: ignore[operator]
            model_catalog=(
                disabled.__class__(
                    id="disabled-id",
                    display_name=disabled.display_name,
                    capability=disabled.capability,
                    enabled=False,
                    is_default=False,
                    provider=disabled.provider,
                    base_url=disabled.base_url,
                    model_name=disabled.model_name,
                    api_key=disabled.api_key,
                ),
            )
        )
    service = ModelService(settings, None)  # type: ignore[arg-type]
    with pytest.raises(ApiError) as captured:
        service._resolve_model(model_id, capability)  # type: ignore[arg-type]
    assert captured.value.code == code


@pytest.mark.parametrize(
    ("response", "code"),
    [
        ({}, "UPSTREAM_INVALID_RESPONSE"),
        ({"choices": "bad"}, "UPSTREAM_INVALID_RESPONSE"),
        ({"choices": []}, "UPSTREAM_INVALID_RESPONSE"),
        ({"choices": ["bad"]}, "UPSTREAM_INVALID_RESPONSE"),
        ({"choices": [{}]}, "UPSTREAM_EMPTY_RESULT"),
        ({"choices": [{"message": "bad"}]}, "UPSTREAM_EMPTY_RESULT"),
        ({"choices": [{"message": {"content": 1}}]}, "UPSTREAM_EMPTY_RESULT"),
        ({"choices": [{"message": {"content": " "}}]}, "UPSTREAM_EMPTY_RESULT"),
    ],
)
def test_tc_svc_008_message_contract(
    settings_factory: object, response: dict[str, object], code: str
) -> None:
    """TC-SVC-008 上游结果结构缺失或空结果均拒绝伪成功。"""

    service = ModelService(settings_factory(), None)  # type: ignore[arg-type,operator]
    with pytest.raises(ApiError) as captured:
        service._message_text(response)
    assert captured.value.code == code
