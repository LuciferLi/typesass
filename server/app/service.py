"""目录驱动的 OpenAI-compatible 上游调用服务。"""

import asyncio
import base64
import binascii
import logging
import re
import time
from typing import Dict, Literal, Tuple

import httpx

from .config import ModelCatalogItem, Settings, parse_model_catalog_payload
from .errors import ApiError
from .models import AudioTranscriptionRequest, TextProcessRequest


logger = logging.getLogger("aitool.upstream")
ALLOWED_AUDIO_CONTENT_TYPES = {
    "audio/wav",
    "audio/webm",
    "audio/mpeg",
    "audio/mp4",
    "audio/ogg",
}
CRITICAL_TOKEN_PATTERN = re.compile(
    r"\d+(?:[.,:/-]\d+)*(?:%|％|ms|s|秒|分钟|小时|天|元|块|万|亿|GB|MB|KB)?",
    re.IGNORECASE,
)
TEXT_FIDELITY_FALLBACK_MIN_CHARS = 30
TEXT_FIDELITY_MIN_LENGTH_RATIO = 0.55


def _critical_text_tokens(text: str) -> set[str]:
    """提取文本保真兜底需要强制保留的关键数字类片段。

    用途：捕获版本号、时间、数量、金额、百分比和单位数字，避免润色模型漏掉硬信息。
    流程：使用固定正则匹配数字及紧邻单位，返回去重集合。
    参数：``text`` 为原文或模型输出。
    返回：关键数字类片段集合。
    异常边界：不解析自然语言语义，不记录正文，避免兜底逻辑引入隐私日志。
    """

    return {match.group(0) for match in CRITICAL_TOKEN_PATTERN.finditer(text)}


def _should_fallback_to_original_text(source_text: str, processed_text: str) -> bool:
    """判断文本处理结果是否疑似丢失原意。

    用途：在模型过度总结、删减或漏掉关键数字时回退 ASR 原文，优先保护用户原意。
    流程：先拒绝空输出，再检查关键数字是否缺失，最后对较长文本做长度缩水保护。
    参数：``source_text`` 为原始 ASR 或待润色文本，``processed_text`` 为模型输出。
    返回：需要回退原文返回 true。
    异常边界：只做保守启发式判断，不根据正文内容生成错误信息或日志。
    """

    normalized_source = source_text.strip()
    normalized_processed = processed_text.strip()
    if not normalized_processed:
        return True
    source_tokens = _critical_text_tokens(normalized_source)
    if source_tokens and not source_tokens.issubset(
        _critical_text_tokens(normalized_processed)
    ):
        return True
    if len(normalized_source) < TEXT_FIDELITY_FALLBACK_MIN_CHARS:
        return False
    return len(normalized_processed) < int(
        len(normalized_source) * TEXT_FIDELITY_MIN_LENGTH_RATIO
    )


class ModelService:
    """目录驱动的 ASR 与文本处理服务。

    用途：隔离受信模型目录、上游密钥、超时和响应解析，路由层不接触外部协议细节。
    流程：按 opaque modelId 解析目录并校验能力，构造 OpenAI 兼容请求，通过共享客户端调用并映射稳定响应。
    边界：客户端不能覆盖 provider、URL、上游模型或 Key；无模型和选择错误使用稳定错误码，上游错误只返回脱敏摘要。
    """

    def __init__(self, settings: Settings, client: httpx.AsyncClient) -> None:
        """初始化目录驱动模型服务。

        用途：注入不可变配置和应用生命周期内共享的 HTTP 客户端。
        流程：保存引用，不建立额外连接池。
        参数：``settings`` 为服务配置，``client`` 为共享异步客户端。
        返回：无。
        异常边界：配置有效性由启动阶段负责。
        """

        self.settings = settings
        self.client = client
        self._model_catalog = settings.model_catalog

    def list_models(self) -> Tuple[ModelCatalogItem, ...]:
        """读取当前不可变模型目录。

        用途：供公开目录路由显式映射安全字段，不让路由访问环境变量或重新解析私有配置。
        流程：直接返回启动阶段完成校验的不可变元组，调用方不得序列化私有字段。
        参数：无。
        返回：受信模型目录元组，保留 sidecar 注入顺序。
        异常边界：空目录合法并返回空元组，不触发上游访问或配置错误。
        """

        return self._model_catalog

    def reload_models(self, payload: object) -> int:
        """热更新运行时模型目录。

        用途：让桌面 App 在新增、编辑、启停、删除模型后无需重启 sidecar 即可更新业务调用目录。
        流程：复用配置层严格校验受信 payload，成功后原子替换内存目录。
        参数：``payload`` 为 Rust 注入的模型目录数组。
        返回：热更新后的目录项数量。
        异常边界：校验失败时保持旧目录不变，错误不包含上游 URL、模型名或 API Key。
        """

        model_catalog = parse_model_catalog_payload(payload)
        self._model_catalog = model_catalog
        return len(model_catalog)

    def _resolve_model(
        self, model_id: str, capability: Literal["asr", "text"]
    ) -> ModelCatalogItem:
        """按 opaque ID 解析并校验模型。

        用途：为所有 AI 调用统一执行目录存在性、启用状态和单一能力校验，禁止静默回退其它模型。
        流程：先区分空目录，再精确匹配 ID，随后检查 enabled 和 capability，全部通过后返回私有运行配置。
        参数：``model_id`` 为请求提交的目录 ID；``capability`` 为当前接口要求的 asr 或 text。
        返回：可用于受控上游调用的模型配置。
        异常边界：依次稳定返回 MODEL_NOT_CONFIGURED、MODEL_NOT_FOUND、MODEL_DISABLED、MODEL_CAPABILITY_MISMATCH；
        错误消息和日志不包含 URL、上游模型名或密钥。
        """

        model_catalog = self._model_catalog
        if not model_catalog:
            raise ApiError(503, "MODEL_NOT_CONFIGURED", "服务尚未配置模型。")
        model = next(
            (item for item in model_catalog if item.id == model_id), None
        )
        if model is None:
            raise ApiError(404, "MODEL_NOT_FOUND", "模型不存在。")
        if not model.enabled:
            raise ApiError(409, "MODEL_DISABLED", "模型已禁用。")
        if model.capability != capability:
            raise ApiError(
                409, "MODEL_CAPABILITY_MISMATCH", "模型能力与当前接口不匹配。"
            )
        return model

    async def transcribe(
        self, request: AudioTranscriptionRequest, request_id: str
    ) -> Tuple[str, int, str]:
        """执行音频转写。

        用途：把合法 base64 音频提交给请求指定且已登记的 ASR 模型。
        流程：先解析并校验目录模型，再校验 MIME、严格解码和大小，构造 data URL 请求并解析首条文本。
        参数：``request`` 为音频契约，``request_id`` 用于结构化上游日志。
        返回：识别文本、耗时毫秒、实际模型三元组。
        异常边界：格式、大小、上游超时或空结果均抛出 ``ApiError``。
        """

        model = self._resolve_model(request.model_id, "asr")
        content_type = request.content_type.strip().lower()
        if content_type not in ALLOWED_AUDIO_CONTENT_TYPES:
            raise ApiError(400, "UNSUPPORTED_AUDIO_TYPE", "不支持的音频类型。")
        try:
            audio_bytes = base64.b64decode(request.audio_base64, validate=True)
        except (binascii.Error, ValueError) as error:
            raise ApiError(
                400, "INVALID_AUDIO_BASE64", "音频 base64 格式无效。"
            ) from error
        if not audio_bytes:
            raise ApiError(400, "EMPTY_AUDIO", "音频内容为空。")
        if len(audio_bytes) > self.settings.max_audio_bytes:
            raise ApiError(413, "AUDIO_TOO_LARGE", "音频超过服务限制。")
        body: Dict[str, object] = {
            "model": model.model_name,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": "data:{0};base64,{1}".format(
                                    content_type,
                                    request.audio_base64,
                                )
                            },
                        }
                    ],
                }
            ],
        }
        language = (request.language or "auto").strip()
        if language and language != "auto":
            body["asr_options"] = {"language": language}
        started_at = time.perf_counter()
        response = await self._chat_completion(model, body, request_id)
        text = self._message_text(response)
        elapsed_ms = int((time.perf_counter() - started_at) * 1000)
        return text, elapsed_ms, model.id

    async def process_text(
        self, request: TextProcessRequest, request_id: str
    ) -> Tuple[str, int, str]:
        """执行听写整理或文字润色。

        用途：按固定业务模式生成提示词并调用请求指定且已登记的文本模型。
        流程：先解析并校验目录模型，再校验文本及词典长度，组合系统和用户上下文，解析有效 assistant 文本。
        参数：``request`` 为文本契约，``request_id`` 用于上游日志追踪。
        返回：处理文本、耗时毫秒、实际模型三元组。
        异常边界：超长输入、非法词典、超时、上游失败或空输出均抛出 ``ApiError``。
        """

        model = self._resolve_model(request.model_id, "text")
        text = request.text.strip()
        if len(text) > self.settings.max_text_chars:
            raise ApiError(413, "TEXT_TOO_LARGE", "文本超过服务限制。")
        if any(len(item) > 100 for item in request.dictionary):
            raise ApiError(400, "INVALID_DICTIONARY", "词典单项长度超过限制。")
        if request.mode == "dictate":
            mode_rule = (
                "你正在处理语音转文字后的口述原文，目标是保真整理。"
                "只修正明显错别字、标点、断句和轻微口语连接，让文字更易读。"
                "必须完整保留原文中的事实、对象、动作、时间、地点、数字、数量、版本、否定、条件、因果、转折、限制和强调。"
                "禁止总结、概括、扩写、脑补、删减关键信息、改变立场或把不确定内容改成确定表达。"
                "如果某句话不够通顺但含义不明确，宁可保留原句。"
            )
        else:
            mode_rule = (
                "你正在润色用户已经写好的文本，目标是保真润色。"
                "只改善标点、错别字、语序和表达流畅度。"
                "必须完整保留原文中的事实、对象、动作、时间、地点、数字、数量、版本、否定、条件、因果、转折、限制和强调。"
                "禁止总结、概括、扩写、脑补、删减关键信息或改变原意。"
                "如果无法确认用户意图，宁可保留原句。"
            )
        system_prompt = (
            "你是中文文本保真处理助手。{0}"
            "输出前逐项自查：是否遗漏数字、否定、条件、因果、转折或关键对象；如有遗漏，必须恢复。"
            "只输出最终文本，不解释处理过程。".format(mode_rule)
        )
        context_lines = ["原文：{0}".format(text)]
        if request.dictionary:
            context_lines.append("词典：{0}".format("、".join(request.dictionary)))
        if request.context_app.strip():
            context_lines.append("上下文应用：{0}".format(request.context_app.strip()))
        if request.style_instruction.strip():
            context_lines.append(
                "风格要求：{0}".format(request.style_instruction.strip())
            )
        body = {
            "model": model.model_name,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": "\n".join(context_lines)},
            ],
            "temperature": 0.1,
            "max_completion_tokens": min(4096, max(256, len(text) * 3)),
        }
        started_at = time.perf_counter()
        if request.processing_timeout_ms is None:
            response = await self._chat_completion(model, body, request_id)
        else:
            try:
                response = await asyncio.wait_for(
                    self._chat_completion(model, body, request_id),
                    timeout=request.processing_timeout_ms / 1000,
                )
            except asyncio.TimeoutError as error:
                logger.warning(
                    "upstream_timeout",
                    extra={
                        "context": {
                            "requestId": request_id,
                            "timeoutMs": request.processing_timeout_ms,
                        }
                    },
                )
                raise ApiError(504, "UPSTREAM_TIMEOUT", "模型服务响应超时。") from error
        processed_text = self._message_text(response)
        if _should_fallback_to_original_text(text, processed_text):
            logger.warning(
                "text_fidelity_fallback",
                extra={
                    "context": {
                        "requestId": request_id,
                        "mode": request.mode,
                        "sourceChars": len(text),
                        "processedChars": len(processed_text),
                    }
                },
            )
            processed_text = text
        elapsed_ms = int((time.perf_counter() - started_at) * 1000)
        return processed_text, elapsed_ms, model.id

    async def _chat_completion(
        self,
        model: ModelCatalogItem,
        body: Dict[str, object],
        request_id: str,
    ) -> Dict[str, object]:
        """调用目录模型绑定的 chat/completions 上游。

        用途：统一 Bearer 鉴权、超时、上游状态映射和脱敏日志。
        流程：使用共享客户端 POST 受信目录地址，注入对应私有密钥，校验 JSON 对象后返回。
        参数：``model`` 为已校验私有目录项，``body`` 为服务端构造请求，``request_id`` 为追踪标识。
        返回：上游 JSON 对象。
        异常边界：连接或读取超时映射 504；其余网络错误和非 2xx 映射 502；
        不透传完整上游响应。
        """

        url = "{0}/chat/completions".format(model.base_url)
        try:
            response = await self.client.post(
                url,
                headers={"Authorization": "Bearer {0}".format(model.api_key)},
                json=body,
            )
        except httpx.TimeoutException as error:
            logger.warning(
                "upstream_timeout", extra={"context": {"requestId": request_id}}
            )
            raise ApiError(504, "UPSTREAM_TIMEOUT", "模型服务响应超时。") from error
        except httpx.HTTPError as error:
            logger.warning(
                "upstream_network_error",
                extra={
                    "context": {
                        "requestId": request_id,
                        "errorType": type(error).__name__,
                    }
                },
            )
            raise ApiError(502, "UPSTREAM_UNAVAILABLE", "模型服务暂不可用。") from error
        if not response.is_success:
            logger.warning(
                "upstream_rejected",
                extra={
                    "context": {
                        "requestId": request_id,
                        "statusCode": response.status_code,
                    }
                },
            )
            raise ApiError(502, "UPSTREAM_REJECTED", "模型服务请求失败。")
        try:
            payload = response.json()
        except ValueError as error:
            raise ApiError(
                502, "UPSTREAM_INVALID_RESPONSE", "模型服务返回格式无效。"
            ) from error
        if not isinstance(payload, dict):
            raise ApiError(502, "UPSTREAM_INVALID_RESPONSE", "模型服务返回格式无效。")
        return payload

    def _message_text(self, response: Dict[str, object]) -> str:
        """提取上游首条 assistant 文本。

        用途：将 OpenAI 兼容响应映射为稳定业务字符串。
        流程：逐层校验 choices、message、content 类型并 trim。
        参数：``response`` 为已解析的上游 JSON 对象。
        返回：非空 assistant 文本。
        异常边界：结构缺失或内容为空时抛出 502 业务异常。
        """

        choices = response.get("choices")
        if (
            not isinstance(choices, list)
            or not choices
            or not isinstance(choices[0], dict)
        ):
            raise ApiError(502, "UPSTREAM_INVALID_RESPONSE", "模型服务未返回有效结果。")
        message = choices[0].get("message")
        content = message.get("content") if isinstance(message, dict) else None
        if not isinstance(content, str) or not content.strip():
            raise ApiError(502, "UPSTREAM_EMPTY_RESULT", "模型服务返回空结果。")
        return content.strip()
