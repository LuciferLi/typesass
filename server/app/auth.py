"""长期调用凭据与短期访问 Token 服务。"""

import base64
import binascii
import hashlib
import hmac
import json
import secrets
import sqlite3
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from .config import Settings
from .errors import ApiError


class AppAccessTokenService:
    """管理 App 明文授权码。

    用途：为系统设置页、授权码申请接口和公网业务接口提供同一份授权码数据。
    流程：使用独立 SQLite 明文保存 token；创建时生成高熵授权码，校验时检查撤销与过期并更新最近使用时间。
    边界：本服务不接触任务 SQLite，不签发短期 session token，也不保存 pending 授权申请。
    """

    def __init__(self, database_file: str) -> None:
        """初始化授权码存储。

        参数：``database_file`` 为授权码独立 SQLite 文件路径。
        流程：保存路径并立即创建表结构，确保后续 HTTP 请求可直接读写。
        异常边界：数据库无法创建或迁移失败时抛出 ApiError，阻止伪成功。
        """

        self._database_file = database_file
        self._lock = threading.Lock()
        self._ensure_schema()

    def create(self, name: str, expires_at: Optional[str]) -> Dict[str, object]:
        """创建一条明文授权码。

        参数：``name`` 为用户可识别名称，``expires_at`` 为 UTC ISO 时间或 None。
        流程：生成唯一 token 和记录 ID，写入 SQLite 后返回完整记录。
        返回：包含明文 token、状态和时间字段的授权码记录。
        异常边界：过期时间非法或数据库写入失败时返回稳定错误。
        """

        normalized_expires_at = self._normalize_expires_at(expires_at)
        now = self._now()
        with self._lock:
            connection = self._connect()
            try:
                token_id = "token_{0}".format(secrets.token_urlsafe(16))
                token = "typesass_{0}".format(secrets.token_urlsafe(32))
                while self._record_exists(connection, token_id, token):
                    token_id = "token_{0}".format(secrets.token_urlsafe(16))
                    token = "typesass_{0}".format(secrets.token_urlsafe(32))
                connection.execute(
                    """
                    INSERT INTO access_token (
                        id, name, token, expires_at, created_at, revoked_at, last_used_at
                    ) VALUES (?, ?, ?, ?, ?, '', '')
                    """,
                    (token_id, name, token, normalized_expires_at or "", now),
                )
                connection.commit()
            except sqlite3.Error as error:
                raise ApiError(500, "ACCESS_TOKEN_STORE_FAILED", "授权码保存失败。") from error
            finally:
                connection.close()
        return self._public_record(
            {
                "id": token_id,
                "name": name,
                "token": token,
                "expires_at": normalized_expires_at or "",
                "created_at": now,
                "revoked_at": "",
                "last_used_at": "",
            }
        )

    def list_tokens(self) -> List[Dict[str, object]]:
        """查询授权码列表。

        流程：按创建时间倒序读取所有授权码，逐条计算有效、过期或已撤销状态。
        返回：包含明文授权码的列表。
        异常边界：读取失败返回 ACCESS_TOKEN_STORE_FAILED。
        """

        with self._lock:
            connection = self._connect()
            try:
                rows = connection.execute(
                    """
                    SELECT id, name, token, expires_at, created_at, revoked_at, last_used_at
                    FROM access_token
                    ORDER BY created_at DESC, id DESC
                    """
                ).fetchall()
            except sqlite3.Error as error:
                raise ApiError(500, "ACCESS_TOKEN_STORE_FAILED", "授权码读取失败。") from error
            finally:
                connection.close()
        return [
            self._public_record(
                {
                    "id": row[0],
                    "name": row[1],
                    "token": row[2],
                    "expires_at": row[3],
                    "created_at": row[4],
                    "revoked_at": row[5],
                    "last_used_at": row[6],
                }
            )
            for row in rows
        ]

    def revoke(self, token_id: str) -> Dict[str, object]:
        """撤销一条授权码。

        参数：``token_id`` 为授权码稳定 ID。
        流程：查找记录，已撤销保持幂等，未撤销则写入撤销时间。
        返回：撤销后的授权码记录。
        异常边界：未知 ID 返回 404，数据库错误返回 500。
        """

        now = self._now()
        with self._lock:
            connection = self._connect()
            try:
                row = connection.execute(
                    """
                    SELECT id, name, token, expires_at, created_at, revoked_at, last_used_at
                    FROM access_token
                    WHERE id = ?
                    """,
                    (token_id,),
                ).fetchone()
                if row is None:
                    raise ApiError(404, "ACCESS_TOKEN_NOT_FOUND", "授权码不存在。")
                revoked_at = row[5] or now
                if not row[5]:
                    connection.execute(
                        "UPDATE access_token SET revoked_at = ? WHERE id = ?",
                        (revoked_at, token_id),
                    )
                    connection.commit()
            except ApiError:
                raise
            except sqlite3.Error as error:
                raise ApiError(500, "ACCESS_TOKEN_STORE_FAILED", "授权码撤销失败。") from error
            finally:
                connection.close()
        return self._public_record(
            {
                "id": row[0],
                "name": row[1],
                "token": row[2],
                "expires_at": row[3],
                "created_at": row[4],
                "revoked_at": revoked_at,
                "last_used_at": row[6],
            }
        )

    def verify(self, token: str) -> str:
        """校验授权码并更新最近使用时间。

        参数：``token`` 为 Authorization Bearer 中的明文授权码。
        流程：常量时间匹配存量 token，拒绝撤销或过期记录，成功后更新 lastUsedAt。
        返回：授权码记录 ID，用于限流和访问日志 clientId。
        异常边界：缺失、未知、过期或撤销统一返回 401 UNAUTHORIZED。
        """

        if not token:
            raise ApiError(
                401, "UNAUTHORIZED", "授权码无效或已失效。", {"WWW-Authenticate": "Bearer"}
            )
        now = self._now()
        with self._lock:
            connection = self._connect()
            try:
                rows = connection.execute(
                    "SELECT id, token, expires_at, revoked_at FROM access_token"
                ).fetchall()
                matched_id = ""
                matched_expires_at = ""
                matched_revoked_at = ""
                for row in rows:
                    if hmac.compare_digest(token, row[1]):
                        matched_id = row[0]
                        matched_expires_at = row[2]
                        matched_revoked_at = row[3]
                        break
                if (
                    not matched_id
                    or matched_revoked_at
                    or self._is_expired(matched_expires_at)
                ):
                    raise ApiError(
                        401,
                        "UNAUTHORIZED",
                        "授权码无效或已失效。",
                        {"WWW-Authenticate": "Bearer"},
                    )
                connection.execute(
                    "UPDATE access_token SET last_used_at = ? WHERE id = ?",
                    (now, matched_id),
                )
                connection.commit()
            except ApiError:
                raise
            except sqlite3.Error as error:
                raise ApiError(500, "ACCESS_TOKEN_STORE_FAILED", "授权码校验失败。") from error
            finally:
                connection.close()
        return matched_id

    def _connect(self) -> sqlite3.Connection:
        """打开授权码数据库连接。

        返回：已配置 row 访问模式的 SQLite 连接。
        异常边界：目录创建或连接失败统一转换为 ApiError。
        """

        try:
            path = Path(self._database_file)
            path.parent.mkdir(parents=True, exist_ok=True)
            return sqlite3.connect(path, timeout=5)
        except OSError as error:
            raise ApiError(500, "ACCESS_TOKEN_STORE_FAILED", "授权码存储不可用。") from error

    def _ensure_schema(self) -> None:
        """创建授权码表结构。

        流程：使用 IF NOT EXISTS 初始化首版明文 token 表。
        异常边界：初始化失败阻止服务继续处理授权码请求。
        """

        connection = self._connect()
        try:
            connection.execute(
                """
                CREATE TABLE IF NOT EXISTS access_token (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    token TEXT NOT NULL UNIQUE,
                    expires_at TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    revoked_at TEXT NOT NULL DEFAULT '',
                    last_used_at TEXT NOT NULL DEFAULT ''
                )
                """
            )
            connection.commit()
        except sqlite3.Error as error:
            raise ApiError(500, "ACCESS_TOKEN_STORE_FAILED", "授权码存储初始化失败。") from error
        finally:
            connection.close()

    @staticmethod
    def _record_exists(connection: sqlite3.Connection, token_id: str, token: str) -> bool:
        """检查授权码 ID 或明文 token 是否已存在。"""

        row = connection.execute(
            "SELECT 1 FROM access_token WHERE id = ? OR token = ? LIMIT 1",
            (token_id, token),
        ).fetchone()
        return row is not None

    @classmethod
    def _normalize_expires_at(cls, expires_at: Optional[str]) -> Optional[str]:
        """规范化授权码过期时间。

        参数：``expires_at`` 为客户端传入的 UTC ISO 字符串或 None。
        返回：规范化 ISO 字符串；None 表示永久有效。
        异常边界：无法解析、非 UTC 或已过期返回 422。
        """

        if expires_at is None:
            return None
        normalized = expires_at.strip()
        if not normalized:
            return None
        try:
            parsed = datetime.fromisoformat(normalized.replace("Z", "+00:00"))
        except ValueError as error:
            raise ApiError(422, "VALIDATION_ERROR", "请求字段校验失败。") from error
        if parsed.tzinfo is None:
            raise ApiError(422, "VALIDATION_ERROR", "请求字段校验失败。")
        utc_value = parsed.astimezone(timezone.utc)
        if utc_value <= datetime.now(timezone.utc):
            raise ApiError(422, "VALIDATION_ERROR", "请求字段校验失败。")
        return utc_value.isoformat().replace("+00:00", "Z")

    @classmethod
    def _is_expired(cls, expires_at: str) -> bool:
        """判断授权码是否已过期。"""

        if not expires_at:
            return False
        try:
            parsed = datetime.fromisoformat(expires_at.replace("Z", "+00:00"))
        except ValueError:
            return True
        return parsed <= datetime.now(timezone.utc)

    @staticmethod
    def _now() -> str:
        """返回当前 UTC ISO 时间字符串。"""

        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

    @classmethod
    def _public_record(cls, row: Dict[str, object]) -> Dict[str, object]:
        """把数据库行转换为公开响应记录。"""

        revoked_at = str(row["revoked_at"] or "")
        expires_at = str(row["expires_at"] or "")
        status = "active"
        if revoked_at:
            status = "revoked"
        elif cls._is_expired(expires_at):
            status = "expired"
        return {
            "id": row["id"],
            "name": row["name"],
            "token": row["token"],
            "expiresAt": expires_at or None,
            "status": status,
            "createdAt": row["created_at"],
            "revokedAt": revoked_at or None,
            "lastUsedAt": str(row["last_used_at"] or "") or None,
        }


class AccessTokenService:
    """签发并校验短期访问 Token。

    用途：避免 Web 或第三方业务请求长期携带管理端调用凭据。
    流程：交换时校验每调用方独立 secret，使用 HMAC-SHA256 签名 clientId、过期时间和随机数；调用时验签和验期。
    边界：移除调用方或轮换签名密钥可立即吊销 Token；不记录或返回长期 secret。
    """

    def __init__(self, settings: Settings) -> None:
        """初始化 Token 服务。

        参数：``settings`` 提供调用方凭据、签名密钥和 TTL。
        流程：复制为内存映射并保存签名字节，不执行外部 IO。
        边界：格式与最小强度已由配置层校验。
        """

        self._client_secrets: Dict[str, str] = dict(settings.api_tokens)
        self._device_approver_client_ids = frozenset(
            settings.device_approver_client_ids
        )
        self._signing_key = settings.token_signing_key.encode("utf-8")
        self._ttl_seconds = settings.access_token_ttl_seconds
        self._issuer = settings.public_base_url
        self._audience = "codexman-ai-api"

    def exchange(self, client_id: str, client_secret: str) -> str:
        """用长期调用凭据换取短期 Token。

        参数：``client_id`` 和 ``client_secret`` 为管理员分配的独立凭据。
        返回：签名后的短期 Token。
        异常边界：未知调用方与错误 secret 返回相同 401，避免枚举账号。
        """

        self.verify_client(client_id, client_secret)
        return self.issue_for_client(client_id)

    def verify_client(self, client_id: str, client_secret: str) -> None:
        """校验长期调用凭据。

        参数：调用方 ID 与 secret。
        流程：查找独立 secret 并常量时间比较，供服务端换 Token 和设备码批准复用。
        返回：无。
        异常边界：未知调用方和错误 secret 使用相同 401。
        """

        expected_secret = self._client_secrets.get(client_id, "")
        if not expected_secret or not secrets.compare_digest(
            client_secret, expected_secret
        ):
            raise ApiError(
                401, "INVALID_CLIENT", "调用凭据无效。", {"WWW-Authenticate": "Basic"}
            )

    def verify_device_approver(self, client_id: str, client_secret: str) -> None:
        """校验设备码批准方凭据与授权范围。

        参数：调用方 ID 与长期 secret。
        流程：先复用常量时间凭据校验，再检查调用方是否在部署配置的批准白名单。
        返回：无。
        异常边界：凭据错误返回 401；有效但无批准权限返回 403，不泄露其它调用方信息。
        """

        self.verify_client(client_id, client_secret)
        if client_id not in self._device_approver_client_ids:
            raise ApiError(
                403, "DEVICE_APPROVAL_FORBIDDEN", "当前调用方无权批准设备码。"
            )

    def issue_for_client(self, client_id: str) -> str:
        """为已验证调用方签发短期 Token。

        参数：``client_id`` 必须已通过长期凭据或设备授权验证。
        流程：写入 issuer、audience、签发/过期时间、版本和随机数后 HMAC 签名。
        返回：短期 Bearer Token。
        边界：调用方已从配置移除时拒绝签发。
        """

        if client_id not in self._client_secrets:
            raise ApiError(
                401, "UNAUTHORIZED", "鉴权失败。", {"WWW-Authenticate": "Bearer"}
            )
        issued_at = int(time.time())
        payload = {
            "clientId": client_id,
            "iss": self._issuer,
            "aud": self._audience,
            "iat": issued_at,
            "exp": issued_at + self._ttl_seconds,
            "ver": 1,
            "nonce": secrets.token_hex(12),
        }
        encoded_payload = self._encode(
            json.dumps(payload, separators=(",", ":")).encode("utf-8")
        )
        signature = hmac.new(
            self._signing_key, encoded_payload.encode("ascii"), hashlib.sha256
        ).digest()
        return "{0}.{1}".format(encoded_payload, self._encode(signature))

    def verify(self, token: str) -> str:
        """校验短期 Token 并返回调用方 ID。

        参数：``token`` 为 Bearer 凭据。
        流程：拆分载荷与签名、常量时间验签、解析 JSON、检查过期和调用方仍有效。
        返回：Token 绑定的 clientId。
        异常边界：任何格式、签名、过期或已吊销错误统一返回 401。
        """

        try:
            encoded_payload, encoded_signature = token.split(".", 1)
            expected_signature = hmac.new(
                self._signing_key, encoded_payload.encode("ascii"), hashlib.sha256
            ).digest()
            supplied_signature = self._decode(encoded_signature)
            if not hmac.compare_digest(supplied_signature, expected_signature):
                raise ValueError("signature")
            payload = json.loads(self._decode(encoded_payload).decode("utf-8"))
            if not isinstance(payload, dict):
                raise ValueError("payload")
            client_id = payload.get("clientId")
            issuer = payload.get("iss")
            audience = payload.get("aud")
            issued_at = payload.get("iat")
            expires_at = payload.get("exp")
            version = payload.get("ver")
            if not isinstance(client_id, str) or client_id not in self._client_secrets:
                raise ValueError("client")
            if issuer != self._issuer or audience != self._audience or version != 1:
                raise ValueError("claims")
            if not isinstance(issued_at, int) or issued_at > int(time.time()) + 30:
                raise ValueError("issued_at")
            if not isinstance(expires_at, int) or expires_at <= int(time.time()):
                raise ValueError("expired")
            return client_id
        except (
            ValueError,
            UnicodeDecodeError,
            json.JSONDecodeError,
            TypeError,
            binascii.Error,
        ) as error:
            raise ApiError(
                401, "UNAUTHORIZED", "鉴权失败。", {"WWW-Authenticate": "Bearer"}
            ) from error

    @property
    def ttl_seconds(self) -> int:
        """读取签发 TTL。

        返回：配置的有效期秒数。
        边界：只读属性，不允许调用方延长单个 Token。
        """

        return self._ttl_seconds

    @staticmethod
    def _encode(value: bytes) -> str:
        """编码无填充 URL-safe base64。

        参数：``value`` 为原始字节。
        返回：可放入 HTTP Token 的 ASCII 字符串。
        """

        return base64.urlsafe_b64encode(value).rstrip(b"=").decode("ascii")

    @staticmethod
    def _decode(value: str) -> bytes:
        """解码无填充 URL-safe base64。

        参数：``value`` 为 Token 片段。
        返回：原始字节。
        异常边界：非法字符或填充由调用方统一映射为 401。
        """

        padding = "=" * (-len(value) % 4)
        return base64.b64decode(value + padding, altchars=b"-_", validate=True)


class DeviceAuthorizationService:
    """为无密钥浏览器提供一次性设备码授权。

    用途：让 Web/Tauri 在不接触长期 client secret 的情况下获得短期 Token。
    流程：浏览器创建设备码；管理员从机密环境用 Basic 批准；浏览器轮询后一次性领取 Token。
    边界：设备码保存在单 worker 内存且十分钟过期；发布会使未完成授权失效，但不影响已签发 Token。
    """

    def __init__(self, token_service: AccessTokenService) -> None:
        """初始化设备授权服务。

        参数：短期 Token 服务。
        流程：创建受线程锁保护的待授权映射。
        边界：不持久化长期 secret 或设备码。
        """

        self._token_service = token_service
        self._pending: Dict[str, Dict[str, object]] = {}
        self._lock = threading.Lock()
        self.expires_in = 600
        self.interval = 2
        self.max_pending = 1000

    def create(self) -> Tuple[str, str]:
        """创建设备授权请求。

        流程：锁内清理过期记录、检查容量，再生成当前集合内唯一的高熵 deviceCode 和人工 userCode。
        返回：deviceCode、userCode。
        边界：尚未绑定 clientId，不能直接换 Token；容量达到 1000 条时返回带 Retry-After 的 429。
        """

        now = time.time()
        with self._lock:
            self._remove_expired(now)
            if len(self._pending) >= self.max_pending:
                raise ApiError(
                    429,
                    "DEVICE_AUTHORIZATION_CAPACITY",
                    "待授权设备过多，请稍后重试。",
                    {"Retry-After": str(self.interval)},
                )
            device_code = secrets.token_urlsafe(32)
            while device_code in self._pending:
                device_code = secrets.token_urlsafe(32)
            existing_user_codes = {
                entry.get("userCode") for entry in self._pending.values()
            }
            user_code = "{0}-{1}".format(
                secrets.token_hex(2).upper(), secrets.token_hex(2).upper()
            )
            while user_code in existing_user_codes:
                user_code = "{0}-{1}".format(
                    secrets.token_hex(2).upper(), secrets.token_hex(2).upper()
                )
            self._pending[device_code] = {
                "expiresAt": now + self.expires_in,
                "clientId": None,
                "lastPolledAt": 0.0,
                "userCode": user_code,
            }
        return device_code, user_code

    def approve(self, user_code: str, client_id: str, client_secret: str) -> None:
        """批准设备码并绑定调用方。

        参数：用户展示码及管理员长期调用凭据。
        流程：先校验 Basic secret，再在锁内查找有效用户码并写入 clientId。
        返回：无。
        异常边界：未知或过期设备码返回 INVALID_DEVICE_CODE；浏览器永远接触不到 secret。
        """

        self._token_service.verify_device_approver(client_id, client_secret)
        now = time.time()
        with self._lock:
            self._remove_expired(now)
            matched = next(
                (
                    (device_code, entry)
                    for device_code, entry in self._pending.items()
                    if entry.get("userCode") == user_code
                ),
                None,
            )
            if not matched:
                raise ApiError(400, "INVALID_DEVICE_CODE", "设备码无效或已过期。")
            approved_client_id = matched[1].get("clientId")
            if isinstance(approved_client_id, str) and approved_client_id != client_id:
                raise ApiError(
                    409, "DEVICE_ALREADY_APPROVED", "设备码已由其他批准方绑定。"
                )
            matched[1]["clientId"] = client_id

    def poll(self, device_code: str) -> Tuple[str, str, int]:
        """轮询并一次性领取短期 Token。

        参数：浏览器持有的高熵 deviceCode。
        流程：检查存在、过期和批准状态；批准后删除记录并签发 Token。
        返回：accessToken、clientId、expiresIn。
        异常边界：未批准返回 428 与 Retry-After；同一设备码不能重复领取。
        """

        client_id: Optional[str]
        now = time.time()
        with self._lock:
            self._remove_expired(now)
            entry = self._pending.get(device_code)
            if not entry:
                raise ApiError(400, "INVALID_DEVICE_CODE", "设备码无效或已过期。")
            last_polled_at = float(entry.get("lastPolledAt", 0.0))
            if last_polled_at and now - last_polled_at < self.interval:
                raise ApiError(
                    429,
                    "DEVICE_POLLING_TOO_FAST",
                    "设备码轮询过于频繁。",
                    {"Retry-After": str(self.interval)},
                )
            entry["lastPolledAt"] = now
            raw_client_id = entry.get("clientId")
            client_id = raw_client_id if isinstance(raw_client_id, str) else None
            if not client_id:
                raise ApiError(
                    428,
                    "AUTHORIZATION_PENDING",
                    "等待批准方批准设备码。",
                    {"Retry-After": str(self.interval)},
                )
            self._pending.pop(device_code, None)
        return (
            self._token_service.issue_for_client(client_id),
            client_id,
            self._token_service.ttl_seconds,
        )

    def _remove_expired(self, now: float) -> None:
        """删除所有已过期的待授权记录。

        用途：限制公开设备码创建接口的长期内存占用。
        流程：在调用方已持有服务锁时筛选过期 deviceCode，并从映射中逐项删除。
        参数：``now`` 为当前 Unix 秒时间，保证单次清理使用一致时间点。
        返回：无。
        异常边界：仅由锁内的 create/approve/poll 调用，禁止在无锁状态单独执行。
        """

        expired_codes = [
            device_code
            for device_code, entry in self._pending.items()
            if float(entry["expiresAt"]) <= now
        ]
        for device_code in expired_codes:
            self._pending.pop(device_code, None)
