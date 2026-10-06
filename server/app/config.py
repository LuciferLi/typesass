"""服务端配置读取与校验。"""

from dataclasses import dataclass
from functools import lru_cache
import json
import os
import re
from typing import Literal, Optional, Tuple
from urllib.parse import unquote, urlparse


PUBLIC_MAX_BODY_BYTES = 132 * 1024 * 1024
PUBLIC_MAX_AUDIO_BYTES = 96 * 1024 * 1024
PUBLIC_MAX_TEXT_CHARS = 20_000
PUBLIC_ACCESS_TOKEN_TTL_SECONDS = 8 * 60 * 60
PUBLIC_BASE_URL = "http://127.0.0.1:18080"
MODEL_ID_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
MODEL_CATALOG_KEYS = {
    "id",
    "displayName",
    "capability",
    "enabled",
    "isDefault",
    "provider",
    "baseUrl",
    "modelName",
    "apiKey",
}
_MODEL_CATALOG_BOOTSTRAP: Optional[object] = None


@dataclass(frozen=True)
class ModelCatalogItem:
    """受信 sidecar 注入的单个模型运行配置。

    用途：同时保存公开模型元数据和仅供上游调用使用的私有连接参数。
    流程：仅由 ``_model_catalog`` 校验并构造，HTTP 响应必须显式映射安全字段，禁止直接序列化本类型。
    字段：``id`` 为 opaque modelId；``display_name`` 为展示名；``capability`` 为单一能力；
    ``enabled/is_default`` 为目录状态；``provider/base_url/model_name/api_key`` 仅用于受控上游调用。
    边界：私有字段不得进入日志、异常消息、OpenAPI 示例或公开模型目录。
    """

    # 客户端唯一可见且可提交的 opaque 模型目录 ID。
    id: str
    # 面向用户展示的安全名称，不用于上游路由。
    display_name: str
    # 已验证的单一模型能力；一个目录项不能同时承担两类能力。
    capability: Literal["asr", "text"]
    # 是否允许新请求选择该模型；禁用项仍保留在公开目录供客户端识别过期选择。
    enabled: bool
    # 是否为该能力的桌面默认选择；服务端不会据此静默替换请求 ID。
    is_default: bool
    # 受控上游协议类型；当前只允许 OpenAI-compatible。
    provider: Literal["openai-compatible"]
    # 私有上游基础 URL，可含 API 前缀但不含固定 chat/completions endpoint。
    base_url: str
    # 实际提交给私有上游的模型名称，绝不返回客户端。
    model_name: str
    # 私有上游 Bearer 密钥，绝不进入公开响应、日志或异常消息。
    api_key: str


@dataclass(frozen=True)
class Settings:
    """服务运行配置。

    用途：集中承载鉴权、受信模型目录、超时、并发、日志与请求大小限制。
    流程：由 ``load_settings`` 合并一次性 stdin 模型 bootstrap 与非模型环境配置，业务模块不得直接读取二者。
    边界：鉴权启动配置缺失时失败；模型目录允许为空，上游地址、模型名和密钥不接受客户端覆盖。
    """

    api_tokens: Tuple[Tuple[str, str], ...]
    device_approver_client_ids: Tuple[str, ...]
    token_signing_key: str
    access_token_ttl_seconds: int
    client_rate_limit_per_minute: int
    client_daily_quota: int
    quota_database_file: str
    access_token_database_file: str
    public_base_url: str
    model_catalog: Tuple[ModelCatalogItem, ...]
    request_timeout_seconds: float
    concurrency_limit: int
    concurrency_wait_seconds: float
    max_body_bytes: int
    max_audio_bytes: int
    max_text_chars: int
    log_file: str
    log_max_bytes: int
    log_backup_count: int
    enable_dev_bearer_token: bool
    dev_bearer_token: str


def _required_environment(name: str) -> str:
    """读取必填环境变量。

    用途：避免服务在缺少鉴权密钥或上游密钥时带病启动。
    流程：读取并去除首尾空白，空值统一抛出配置错误。
    参数：``name`` 为环境变量名称。
    返回：非空环境变量值。
    异常边界：变量不存在或仅含空白时抛出 ``RuntimeError``，不会回退默认密钥。
    """

    value = os.getenv(name, "").strip()
    if not value:
        raise RuntimeError("缺少必填环境变量：{0}".format(name))
    return value


def _model_base_url(value: str, item_index: int) -> str:
    """校验模型目录中的 OpenAI-compatible 基础 URL。

    用途：阻止协议降级、URL 凭据泄漏、路径穿越和重复 endpoint 进入受控上游调用。
    流程：解析 sidecar 注入值，生产主机仅允许 HTTPS，本机回环地址可使用 HTTP，最后移除末尾斜杠。
    参数：``value`` 为目录项基础 URL，``item_index`` 为安全错误定位序号，不包含模型或密钥内容。
    返回：可安全追加 ``/chat/completions`` 的规范基础 URL。
    异常边界：主机、端口、凭据、查询、片段或路径非法时阻止启动，错误不回显原值。
    """

    normalized_value = value.strip()
    try:
        parsed = urlparse(normalized_value)
        hostname = parsed.hostname
        parsed.port
    except ValueError as error:
        raise RuntimeError(
            "模型目录第 {0} 项 baseUrl 非法".format(item_index)
        ) from error
    is_loopback_http = parsed.scheme == "http" and hostname in (
        "127.0.0.1",
        "localhost",
        "::1",
    )
    if (
        (parsed.scheme != "https" and not is_loopback_http)
        or not parsed.netloc
        or not hostname
        or any(character.isspace() for character in normalized_value)
    ):
        raise RuntimeError(
            "模型目录第 {0} 项 baseUrl 必须使用 HTTPS 或本机 HTTP".format(item_index)
        )
    if parsed.username is not None or parsed.password is not None:
        raise RuntimeError("模型目录第 {0} 项 baseUrl 禁止包含凭据".format(item_index))
    if (
        parsed.query
        or parsed.fragment
        or "?" in normalized_value
        or "#" in normalized_value
    ):
        raise RuntimeError(
            "模型目录第 {0} 项 baseUrl 禁止包含 query 或 fragment".format(item_index)
        )
    normalized_path = parsed.path.rstrip("/")
    decoded_segments = tuple(
        unquote(segment) for segment in normalized_path.split("/") if segment
    )
    if (
        parsed.params
        or "\\" in normalized_path
        or "//" in normalized_path
        or any(segment in (".", "..") for segment in decoded_segments)
    ):
        raise RuntimeError(
            "模型目录第 {0} 项 baseUrl 包含非法基础路径".format(item_index)
        )
    if decoded_segments[-2:] == ("chat", "completions"):
        raise RuntimeError(
            "模型目录第 {0} 项 baseUrl 不得包含固定 endpoint".format(item_index)
        )
    return normalized_value.rstrip("/")


def set_model_catalog_bootstrap(payload: object) -> None:
    """暂存 sidecar stdin 注入的一次性模型目录。

    用途：在导入 FastAPI 应用前，把 Rust 经 stdin 传入的私有目录交给集中配置层，避免模型 API Key 长期存在子进程环境。
    流程：仅保存待消费对象；随后 ``load_settings`` 调用 ``_model_catalog`` 时取得并立即清空本引用。
    参数：``payload`` 为 bootstrap envelope 中的 ``modelCatalog`` 值，结构校验统一由 ``_model_catalog`` 完成。
    返回：无。
    异常边界：本方法不记录、复制或序列化 payload；生产启动仅调用一次，测试须在清除配置缓存后重新注入。
    """

    global _MODEL_CATALOG_BOOTSTRAP
    _MODEL_CATALOG_BOOTSTRAP = payload


def _model_catalog() -> Tuple[ModelCatalogItem, ...]:
    """消费并校验受信 sidecar 注入的模型目录。

    用途：把模型元数据、上游路由和密钥集中转换为不可变运行时目录，业务请求只能按 opaque ID 选择。
    流程：取得一次性 stdin bootstrap 对象并立即清空原始引用，严格校验数组字段、类型、唯一 ID 和 provider 后构造目录项。
    参数：无。
    返回：按 sidecar 注入顺序排列的模型配置元组；直接 Uvicorn 启动且未 bootstrap 时返回空元组。
    异常边界：任一项结构或安全字段非法会阻止启动；错误只包含数组序号和字段名，不包含密钥或 URL 原值。
    """

    global _MODEL_CATALOG_BOOTSTRAP
    payload = _MODEL_CATALOG_BOOTSTRAP
    _MODEL_CATALOG_BOOTSTRAP = None
    if payload is None:
        return ()
    return parse_model_catalog_payload(payload)


def parse_model_catalog_payload(payload: object) -> Tuple[ModelCatalogItem, ...]:
    """校验并转换受信模型目录 payload。

    用途：复用启动 bootstrap 和运行时 reload 的同一套模型目录校验规则，避免热更新路径绕过安全边界。
    流程：校验数组、字段全集、ID 唯一性、能力、状态、provider、baseUrl、模型名和密钥后构造不可变目录。
    参数：``payload`` 为 Rust 注入的模型目录原始 JSON 对象。
    返回：可原子替换到运行时服务的不可变模型目录元组。
    异常边界：错误只包含数组序号和字段名，不包含 URL、模型名或 API Key 原文。
    """

    if not isinstance(payload, list):
        raise RuntimeError("sidecar bootstrap modelCatalog 必须是 JSON 数组")
    items = []
    seen_ids = set()
    for item_index, item in enumerate(payload):
        if not isinstance(item, dict) or set(item) != MODEL_CATALOG_KEYS:
            raise RuntimeError("模型目录第 {0} 项字段结构无效".format(item_index))
        model_id = item["id"]
        display_name = item["displayName"]
        capability = item["capability"]
        provider = item["provider"]
        model_name = item["modelName"]
        api_key = item["apiKey"]
        if (
            not isinstance(model_id, str)
            or not MODEL_ID_PATTERN.fullmatch(model_id)
            or model_id in seen_ids
        ):
            raise RuntimeError("模型目录第 {0} 项 id 格式无效或重复".format(item_index))
        if (
            not isinstance(display_name, str)
            or not display_name.strip()
            or len(display_name.strip()) > 100
        ):
            raise RuntimeError("模型目录第 {0} 项 displayName 无效".format(item_index))
        if capability not in ("asr", "text"):
            raise RuntimeError("模型目录第 {0} 项 capability 无效".format(item_index))
        if type(item["enabled"]) is not bool or type(item["isDefault"]) is not bool:
            raise RuntimeError(
                "模型目录第 {0} 项状态字段必须是布尔值".format(item_index)
            )
        if provider != "openai-compatible":
            raise RuntimeError("模型目录第 {0} 项 provider 不受支持".format(item_index))
        if not isinstance(item["baseUrl"], str) or not item["baseUrl"].strip():
            raise RuntimeError("模型目录第 {0} 项 baseUrl 无效".format(item_index))
        if not isinstance(model_name, str) or not model_name.strip():
            raise RuntimeError("模型目录第 {0} 项 modelName 无效".format(item_index))
        if not isinstance(api_key, str) or not api_key.strip():
            raise RuntimeError("模型目录第 {0} 项 apiKey 无效".format(item_index))
        seen_ids.add(model_id)
        items.append(
            ModelCatalogItem(
                id=model_id,
                display_name=display_name.strip(),
                capability=capability,
                enabled=item["enabled"],
                is_default=item["isDefault"],
                provider=provider,
                base_url=_model_base_url(item["baseUrl"], item_index),
                model_name=model_name.strip(),
                api_key=api_key.strip(),
            )
        )
    return tuple(items)


def _high_entropy_secret(name: str) -> str:
    """读取至少 32 字符的高熵服务密钥。

    用途：阻止短弱密钥进入长期调用凭据签名链路。
    流程：复用必填读取，再检查最小长度。
    参数：``name`` 为环境变量名。
    返回：通过最小强度门禁的原值。
    异常边界：无法判断真实随机性，部署方仍必须使用密码学安全随机生成器。
    """

    value = _required_environment(name)
    if len(value) < 32:
        raise RuntimeError("环境变量 {0} 至少需要 32 个字符".format(name))
    return value


def _api_tokens() -> Tuple[Tuple[str, str], ...]:
    """读取每调用方独立 API Token 映射。

    用途：支持按调用方签发、轮换和吊销凭据，禁止所有接入方共享一个全局 Token。
    流程：解析 ``AITOOL_API_KEYS_JSON`` JSON 对象，校验调用方 ID、Token 长度和唯一性后转为不可变元组。
    参数：无。
    返回：``(client_id, token)`` 元组列表。
    异常边界：空对象、非字符串字段、短 Token 或重复 Token 会阻止服务启动，原值不会写入错误信息。
    """

    raw_value = os.getenv("AITOOL_API_KEYS_JSON", "").strip()
    if not raw_value:
        return ()
    try:
        payload = json.loads(raw_value)
    except json.JSONDecodeError as error:
        raise RuntimeError("AITOOL_API_KEYS_JSON 必须是 JSON 对象") from error
    if not isinstance(payload, dict):
        raise RuntimeError("AITOOL_API_KEYS_JSON 必须是 JSON 对象")
    entries = []
    seen_tokens = set()
    for client_id, token in payload.items():
        if (
            not isinstance(client_id, str)
            or not client_id.strip()
            or ":" in client_id
            or not client_id.isascii()
            or not isinstance(token, str)
            or len(token.strip()) < 32
            or not token.isascii()
        ):
            raise RuntimeError("AITOOL_API_KEYS_JSON 的调用方和 Token 格式无效")
        normalized_token = token.strip()
        if normalized_token in seen_tokens:
            raise RuntimeError("AITOOL_API_KEYS_JSON 不允许多个调用方共享 Token")
        seen_tokens.add(normalized_token)
        entries.append((client_id.strip(), normalized_token))
    return tuple(entries)


def _device_approver_client_ids(
    api_tokens: Tuple[Tuple[str, str], ...],
) -> Tuple[str, ...]:
    """读取允许批准设备码的机密客户端白名单。

    用途：兼容旧设备码服务单元测试；公开 HTTP 主链路不再使用设备码批准。
    流程：配置缺失时返回空元组；存在时解析逗号分隔的 ``AITOOL_DEVICE_APPROVER_CLIENT_IDS`` 并校验引用。
    参数：``api_tokens`` 为已完成格式校验的调用方凭据列表。
    返回：至少包含一个调用方 ID 的不可变元组。
    异常边界：缺失、空白或引用未知调用方时阻止服务启动，不隐式选择第一个调用方。
    """

    raw_value = os.getenv("AITOOL_DEVICE_APPROVER_CLIENT_IDS", "").strip()
    if not raw_value:
        return ()
    client_ids = tuple(
        dict.fromkeys(
            client_id.strip() for client_id in raw_value.split(",")
            if client_id.strip()
        )
    )
    known_client_ids = {client_id for client_id, _unused_secret in api_tokens}
    if not client_ids or any(
        client_id not in known_client_ids for client_id in client_ids
    ):
        raise RuntimeError("AITOOL_DEVICE_APPROVER_CLIENT_IDS 必须引用已登记调用方")
    return client_ids


def _positive_integer(name: str, default: int) -> int:
    """读取正整数配置。

    用途：统一校验并发数、请求大小和日志轮转阈值。
    流程：读取环境变量并转换整数，未配置时使用默认值。
    参数：``name`` 为环境变量名，``default`` 为默认正整数。
    返回：大于零的整数配置。
    异常边界：格式非法或小于等于零时抛出 ``RuntimeError``。
    """

    raw_value = os.getenv(name, str(default)).strip()
    try:
        value = int(raw_value)
    except ValueError as error:
        raise RuntimeError("环境变量 {0} 必须是正整数".format(name)) from error
    if value <= 0:
        raise RuntimeError("环境变量 {0} 必须大于零".format(name))
    return value


def _positive_float(name: str, default: float) -> float:
    """读取正浮点配置。

    用途：统一校验上游请求和并发等待超时秒数。
    流程：读取环境变量并转换浮点数，未配置时使用默认值。
    参数：``name`` 为环境变量名，``default`` 为默认正数。
    返回：大于零的浮点配置。
    异常边界：格式非法或小于等于零时抛出 ``RuntimeError``。
    """

    raw_value = os.getenv(name, str(default)).strip()
    try:
        value = float(raw_value)
    except ValueError as error:
        raise RuntimeError("环境变量 {0} 必须是正数".format(name)) from error
    if value <= 0:
        raise RuntimeError("环境变量 {0} 必须大于零".format(name))
    return value


def _boolean_flag(name: str) -> bool:
    """读取显式启用型布尔开关。

    用途：只接受环境变量值 ``1`` 开启开发专用能力，避免拼写宽松导致生产误启。
    流程：读取并去除首尾空白，仅当值严格等于 ``1`` 时返回 true。
    参数：``name`` 为环境变量名。
    返回：是否显式开启。
    异常边界：其它值一律视为关闭，不抛错也不降级鉴权。
    """

    return os.getenv(name, "").strip() == "1"


def _dev_bearer_token(enabled: bool) -> str:
    """读取开发期万能 Bearer Token。

    用途：为本机开发 curl 联调提供固定授权码入口，默认关闭且由独立开关保护。
    流程：未启用时返回空字符串；启用后优先读取 ``AITOOL_DEV_ACCESS_TOKEN``，并兼容旧 ``AITOOL_DEV_BEARER_TOKEN``。
    参数：``enabled`` 表示开发 Token 开关是否已显式开启。
    返回：可用于常量时间比较的固定 token。
    异常边界：启用但 token 缺失、过短或含非 ASCII 时阻止服务启动。
    """

    if not enabled:
        return ""
    value = os.getenv("AITOOL_DEV_ACCESS_TOKEN", "").strip() or os.getenv(
        "AITOOL_DEV_BEARER_TOKEN", ""
    ).strip()
    if not value:
        raise RuntimeError("缺少必填环境变量：AITOOL_DEV_ACCESS_TOKEN")
    if len(value) < 32 or not value.isascii():
        raise RuntimeError("AITOOL_DEV_ACCESS_TOKEN 至少需要 32 个 ASCII 字符")
    return value


@lru_cache(maxsize=1)
def load_settings() -> Settings:
    """加载并缓存服务配置。

    用途：向应用和业务服务提供单一、不可变的配置来源。
    流程：消费一次性模型 bootstrap，校验必填启动密钥，解析数值限制，再构造 ``Settings``。
    参数：无。
    返回：当前进程唯一的配置对象。
    异常边界：任何必填或数值配置非法时立即抛错，阻止应用启动。
    """

    api_tokens = _api_tokens()
    enable_dev_bearer_token = _boolean_flag("AITOOL_ENABLE_DEV_BEARER_TOKEN")
    return Settings(
        api_tokens=api_tokens,
        device_approver_client_ids=_device_approver_client_ids(api_tokens),
        token_signing_key=os.getenv(
            "AITOOL_TOKEN_SIGNING_KEY", "unused-signing-key-for-removed-session-token"
        ).strip(),
        access_token_ttl_seconds=PUBLIC_ACCESS_TOKEN_TTL_SECONDS,
        client_rate_limit_per_minute=_positive_integer(
            "AITOOL_CLIENT_RATE_LIMIT_PER_MINUTE", 60
        ),
        client_daily_quota=_positive_integer("AITOOL_CLIENT_DAILY_QUOTA", 10_000),
        quota_database_file=os.getenv(
            "AITOOL_QUOTA_DATABASE_FILE", "data/aitool-quota.sqlite3"
        ).strip(),
        access_token_database_file=os.getenv(
            "AITOOL_ACCESS_TOKEN_DATABASE_FILE", "data/aitool-access-tokens.sqlite3"
        ).strip(),
        public_base_url=PUBLIC_BASE_URL,
        model_catalog=_model_catalog(),
        request_timeout_seconds=_positive_float("AITOOL_REQUEST_TIMEOUT_SECONDS", 30.0),
        concurrency_limit=_positive_integer("AITOOL_CONCURRENCY_LIMIT", 8),
        concurrency_wait_seconds=_positive_float(
            "AITOOL_CONCURRENCY_WAIT_SECONDS", 1.0
        ),
        max_body_bytes=PUBLIC_MAX_BODY_BYTES,
        max_audio_bytes=PUBLIC_MAX_AUDIO_BYTES,
        max_text_chars=PUBLIC_MAX_TEXT_CHARS,
        log_file=os.getenv("AITOOL_LOG_FILE", "logs/aitool-server.log").strip(),
        log_max_bytes=_positive_integer("AITOOL_LOG_MAX_BYTES", 10 * 1024 * 1024),
        log_backup_count=_positive_integer("AITOOL_LOG_BACKUP_COUNT", 5),
        enable_dev_bearer_token=enable_dev_bearer_token,
        dev_bearer_token=_dev_bearer_token(enable_dev_bearer_token),
    )
