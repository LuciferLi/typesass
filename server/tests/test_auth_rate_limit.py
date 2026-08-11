"""短期 Token 与按调用方额度控制测试。"""

import asyncio
import hashlib
import hmac
import json
from datetime import datetime, timezone
from pathlib import Path
import sqlite3
import threading
from typing import Any

import pytest

from app.auth import AccessTokenService, DeviceAuthorizationService
from app.errors import ApiError
from app.rate_limit import ClientRateLimiter


def signed_token(service: AccessTokenService, payload: bytes) -> str:
    """使用测试服务的假签名密钥构造指定载荷 Token。"""

    encoded = service._encode(payload)
    signature = hmac.new(
        service._signing_key, encoded.encode("ascii"), hashlib.sha256
    ).digest()
    return "{0}.{1}".format(encoded, service._encode(signature))


def valid_claims(**overrides: object) -> bytes:
    """构造带固定生产声明的已编码测试载荷。"""

    payload: dict[str, object] = {
        "clientId": "desktop-test",
        "iss": "http://127.0.0.1:18080",
        "aud": "codexman-ai-api",
        "iat": 1000,
        "exp": 1900,
        "ver": 1,
        "nonce": "fake-nonce",
    }
    payload.update(overrides)
    return json.dumps(payload).encode()


def assert_api_error(captured: Any, code: str, authenticate: str) -> None:
    """断言鉴权错误的稳定码和挑战头。"""

    assert captured.value.code == code
    assert captured.value.headers["WWW-Authenticate"] == authenticate


def test_tc_auth_001_exchange_verify_and_ttl(
    settings_factory: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """TC-AUTH-001 正确长期凭据签发绑定调用方、随机数、有效期的短期 Token。"""

    monkeypatch.setattr("app.auth.time.time", lambda: 1000.0)
    monkeypatch.setattr("app.auth.secrets.token_hex", lambda length: "ab" * length)
    service = AccessTokenService(settings_factory(access_token_ttl_seconds=900))  # type: ignore[operator]
    token = service.exchange("desktop-test", "fake-desktop-client-secret-0000000001")
    payload_segment = token.split(".", 1)[0]
    payload = json.loads(service._decode(payload_segment))
    assert payload == {
        "clientId": "desktop-test",
        "iss": "http://127.0.0.1:18080",
        "aud": "codexman-ai-api",
        "iat": 1000,
        "exp": 1900,
        "ver": 1,
        "nonce": "ab" * 12,
    }
    assert service.verify(token) == "desktop-test"
    assert service.ttl_seconds == 900
    with pytest.raises(ApiError) as captured:
        service.issue_for_client("removed-client")
    assert_api_error(captured, "UNAUTHORIZED", "Bearer")


@pytest.mark.parametrize(
    ("client_id", "client_secret"),
    [
        ("unknown-client", "fake-desktop-client-secret-0000000001"),
        ("desktop-test", "fake-wrong-client-secret-000000000001"),
    ],
)
def test_tc_auth_002_invalid_exchange(
    settings_factory: object, client_id: str, client_secret: str
) -> None:
    """TC-AUTH-002 未知调用方和错误长期 secret 返回相同 INVALID_CLIENT。"""

    service = AccessTokenService(settings_factory())  # type: ignore[operator]
    with pytest.raises(ApiError) as captured:
        service.exchange(client_id, client_secret)
    assert_api_error(captured, "INVALID_CLIENT", "Basic")


def test_tc_auth_003_tampered_expired_and_revoked(
    settings_factory: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """TC-AUTH-003 篡改、过期和移除调用方后的 Token 均统一吊销。"""

    monkeypatch.setattr("app.auth.time.time", lambda: 1000.0)
    service = AccessTokenService(settings_factory(access_token_ttl_seconds=900))  # type: ignore[operator]
    token = service.exchange("desktop-test", "fake-desktop-client-secret-0000000001")
    payload, signature = token.split(".", 1)
    cases = ["malformed", "{0}.{1}".format(payload + "A", signature)]
    monkeypatch.setattr("app.auth.time.time", lambda: 1900.0)
    cases.append(token)
    revoked = AccessTokenService(settings_factory(api_tokens=()))  # type: ignore[operator]
    for candidate, verifier in [
        (cases[0], service),
        (cases[1], service),
        (cases[2], service),
        (token, revoked),
    ]:
        with pytest.raises(ApiError) as captured:
            verifier.verify(candidate)
        assert_api_error(captured, "UNAUTHORIZED", "Bearer")


@pytest.mark.parametrize(
    "payload",
    [
        b"{",
        b"\xff",
        b"[]",
        valid_claims(clientId=1),
        valid_claims(clientId="removed-client"),
        valid_claims(iss="https://wrong.example"),
        valid_claims(iat="bad"),
        valid_claims(iat=1031),
        valid_claims(exp="later"),
        valid_claims(exp=1000),
    ],
)
def test_tc_auth_004_invalid_signed_payload(
    settings_factory: object, payload: bytes, monkeypatch: pytest.MonkeyPatch
) -> None:
    """TC-AUTH-004 即使签名正确，非法 JSON、编码、调用方或过期字段也拒绝。"""

    monkeypatch.setattr("app.auth.time.time", lambda: 1000.0)
    service = AccessTokenService(settings_factory())  # type: ignore[operator]
    with pytest.raises(ApiError) as captured:
        service.verify(signed_token(service, payload))
    assert_api_error(captured, "UNAUTHORIZED", "Bearer")


def test_tc_device_001_create_pending_approve_and_single_use(
    settings_factory: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """TC-DEVICE-001 设备码创建、待批准、Basic 批准和一次领取形成完整闭环。"""

    clock = [1000.0]
    monkeypatch.setattr("app.auth.time.time", lambda: clock[0])
    monkeypatch.setattr("app.auth.secrets.token_urlsafe", lambda length: "d" * 43)
    codes = iter(["a1b2", "c3d4"])
    monkeypatch.setattr(
        "app.auth.secrets.token_hex",
        lambda length: next(codes) if length == 2 else "ab" * length,
    )
    token_service = AccessTokenService(settings_factory())  # type: ignore[operator]
    service = DeviceAuthorizationService(token_service)
    device_code, user_code = service.create()
    assert (device_code, user_code) == ("d" * 43, "A1B2-C3D4")
    with pytest.raises(ApiError) as pending:
        service.poll(device_code)
    assert pending.value.code == "AUTHORIZATION_PENDING"
    assert pending.value.headers == {"Retry-After": "2"}
    with pytest.raises(ApiError) as too_fast:
        service.poll(device_code)
    assert too_fast.value.status_code == 429
    assert too_fast.value.code == "DEVICE_POLLING_TOO_FAST"
    assert too_fast.value.headers == {"Retry-After": "2"}
    clock[0] += service.interval
    with pytest.raises(ApiError) as pending_again:
        service.poll(device_code)
    assert pending_again.value.code == "AUTHORIZATION_PENDING"
    service.approve(user_code, "desktop-test", "fake-desktop-client-secret-0000000001")
    clock[0] += service.interval
    access_token, client_id, expires_in = service.poll(device_code)
    assert client_id == "desktop-test"
    assert expires_in == 28800
    assert token_service.verify(access_token) == "desktop-test"
    with pytest.raises(ApiError) as reused:
        service.poll(device_code)
    assert reused.value.code == "INVALID_DEVICE_CODE"


def test_tc_device_001a_approver_scope_capacity_and_binding_conflict(
    settings_factory: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """TC-DEVICE-001A 批准权限、1000 条容量、过期清理和跨批准方覆盖均受控。"""

    primary_secret = "fake-desktop-client-secret-0000000001"
    secondary_secret = "fake-secondary-client-secret-0000000001"
    ordinary_secret = "fake-ordinary-client-secret-000000000001"
    settings = settings_factory(  # type: ignore[operator]
        api_tokens=(
            ("desktop-test", primary_secret),
            ("secondary-approver", secondary_secret),
            ("ordinary-client", ordinary_secret),
        ),
        device_approver_client_ids=("desktop-test", "secondary-approver"),
    )
    token_service = AccessTokenService(settings)
    service = DeviceAuthorizationService(token_service)
    clock = [1000.0]
    monkeypatch.setattr("app.auth.time.time", lambda: clock[0])

    device_code, user_code = service.create()
    with pytest.raises(ApiError) as forbidden:
        service.approve(user_code, "ordinary-client", ordinary_secret)
    assert forbidden.value.status_code == 403
    assert forbidden.value.code == "DEVICE_APPROVAL_FORBIDDEN"

    service.approve(user_code, "desktop-test", primary_secret)
    service.approve(user_code, "desktop-test", primary_secret)
    with pytest.raises(ApiError) as conflict:
        service.approve(user_code, "secondary-approver", secondary_secret)
    assert conflict.value.status_code == 409
    assert conflict.value.code == "DEVICE_ALREADY_APPROVED"
    assert device_code in service._pending

    collision_service = DeviceAuthorizationService(token_service)
    collision_service._pending["duplicate-device"] = {
        "expiresAt": 2000.0,
        "clientId": None,
        "lastPolledAt": 0.0,
        "userCode": "DUPL-CATE",
    }
    device_codes = iter(["duplicate-device", "unique-device"])
    user_code_parts = iter(["dupl", "cate", "new1", "code"])
    monkeypatch.setattr(
        "app.auth.secrets.token_urlsafe", lambda length: next(device_codes)
    )
    monkeypatch.setattr(
        "app.auth.secrets.token_hex", lambda length: next(user_code_parts)
    )
    unique_device, unique_user = collision_service.create()
    assert (unique_device, unique_user) == ("unique-device", "NEW1-CODE")

    service._pending = {
        "pending-{0}".format(index): {
            "expiresAt": 2000.0,
            "clientId": None,
            "lastPolledAt": 0.0,
            "userCode": "CODE-{0}".format(index),
        }
        for index in range(service.max_pending)
    }
    with pytest.raises(ApiError) as capacity:
        service.create()
    assert capacity.value.status_code == 429
    assert capacity.value.code == "DEVICE_AUTHORIZATION_CAPACITY"
    assert capacity.value.headers == {"Retry-After": "2"}

    for entry in service._pending.values():
        entry["expiresAt"] = clock[0]
    monkeypatch.setattr(
        "app.auth.secrets.token_urlsafe", lambda length: "replacement-device"
    )
    replacement_parts = iter(["next", "code"])
    monkeypatch.setattr(
        "app.auth.secrets.token_hex", lambda length: next(replacement_parts)
    )
    replacement, unused_code = service.create()
    assert list(service._pending) == [replacement]


def test_tc_device_002_invalid_and_expired_codes(
    settings_factory: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """TC-DEVICE-002 错误用户码、过期用户码和过期设备码均被清理并拒绝。"""

    clock = [1000.0]
    monkeypatch.setattr("app.auth.time.time", lambda: clock[0])
    token_service = AccessTokenService(settings_factory())  # type: ignore[operator]
    service = DeviceAuthorizationService(token_service)
    device_code, user_code = service.create()
    with pytest.raises(ApiError) as unknown:
        service.approve(
            "FFFF-FFFF", "desktop-test", "fake-desktop-client-secret-0000000001"
        )
    assert unknown.value.code == "INVALID_DEVICE_CODE"
    clock[0] = 1600.0
    with pytest.raises(ApiError) as expired_approval:
        service.approve(
            user_code, "desktop-test", "fake-desktop-client-secret-0000000001"
        )
    assert expired_approval.value.code == "INVALID_DEVICE_CODE"
    second_device, unused_code = service.create()
    clock[0] = 2200.0
    with pytest.raises(ApiError) as expired_poll:
        service.poll(second_device)
    assert expired_poll.value.code == "INVALID_DEVICE_CODE"


@pytest.mark.asyncio
async def test_tc_rate_001_minute_limit_and_client_isolation(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """TC-RATE-001 分钟额度按调用方隔离并返回可执行 Retry-After。"""

    monkeypatch.setattr("app.rate_limit.time.monotonic", lambda: 100.0)
    limiter = ClientRateLimiter(
        per_minute=1, daily_quota=10, database_file=str(tmp_path / "rate.sqlite3")
    )
    await limiter.check("client-a")
    await limiter.check("client-b")
    with pytest.raises(ApiError) as captured:
        await limiter.check("client-a")
    assert captured.value.code == "RATE_LIMIT"
    assert captured.value.headers == {"Retry-After": "60"}


@pytest.mark.asyncio
async def test_tc_rate_002_stale_window_cleanup(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """TC-RATE-002 超过一分钟的历史命中被全部清理后允许新请求。"""

    monkeypatch.setattr("app.rate_limit.time.monotonic", lambda: 200.0)
    limiter = ClientRateLimiter(
        per_minute=1, daily_quota=10, database_file=str(tmp_path / "rate.sqlite3")
    )
    limiter._minute_hits["client-a"].extend([100.0, 139.0])
    await limiter.check("client-a")
    assert list(limiter._minute_hits["client-a"]) == [200.0]


@pytest.mark.asyncio
async def test_tc_rate_003_daily_quota_persists_across_restart(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """TC-RATE-003 日额度跨实例重启保持，并精确等待到下一 UTC 日。"""

    class FakeDateTime:
        current = datetime(2026, 8, 10, 23, 59, 59, 200000, tzinfo=timezone.utc)
        min = datetime.min

        @classmethod
        def now(cls, zone: object) -> datetime:
            return cls.current

        @classmethod
        def combine(
            cls, value: object, clock: object, tzinfo: object = None
        ) -> datetime:
            return datetime.combine(value, clock, tzinfo=tzinfo)  # type: ignore[arg-type]

    monkeypatch.setattr("app.rate_limit.datetime", FakeDateTime)
    database = str(tmp_path / "persistent.sqlite3")
    first_process = ClientRateLimiter(
        per_minute=5, daily_quota=1, database_file=database
    )
    await first_process.check("client-a")
    restarted_process = ClientRateLimiter(
        per_minute=5, daily_quota=1, database_file=database
    )
    with pytest.raises(ApiError) as captured:
        await restarted_process.check("client-a")
    assert captured.value.code == "DAILY_QUOTA_EXCEEDED"
    assert captured.value.headers == {"Retry-After": "1"}
    FakeDateTime.current = datetime(2026, 8, 11, 0, 0, 0, tzinfo=timezone.utc)
    await restarted_process.check("client-a")


@pytest.mark.asyncio
async def test_tc_rate_004_retry_after_floor(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """TC-RATE-004 时钟边界下 Retry-After 最低保持一秒。"""

    monkeypatch.setattr("app.rate_limit.time.monotonic", lambda: 100.0)
    limiter = ClientRateLimiter(
        per_minute=1, daily_quota=10, database_file=str(tmp_path / "rate.sqlite3")
    )
    limiter._minute_hits["client-a"].append(40.9)
    with pytest.raises(ApiError) as captured:
        await limiter.check("client-a")
    assert captured.value.headers == {"Retry-After": "1"}


@pytest.mark.asyncio
async def test_tc_rate_005_store_failure_is_fail_closed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """TC-RATE-005 SQLite 扣额失败返回 503，且不写入分钟放行记录。"""

    limiter = ClientRateLimiter(
        per_minute=10, daily_quota=10, database_file=str(tmp_path / "rate.sqlite3")
    )

    def fail_connect() -> object:
        raise sqlite3.OperationalError("fake quota database failure")

    monkeypatch.setattr(limiter, "_connect", fail_connect)
    with pytest.raises(ApiError) as captured:
        await limiter.check("client-a")
    assert captured.value.status_code == 503
    assert captured.value.code == "QUOTA_STORE_UNAVAILABLE"
    assert captured.value.headers == {"Retry-After": "5"}
    assert list(limiter._minute_hits["client-a"]) == []


@pytest.mark.asyncio
async def test_tc_rate_006_sqlite_io_does_not_block_event_loop_or_other_client(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """验证同步额度 IO 被卸载且不同调用方互不阻塞。

    用途：防止 SQLite 慢 IO 占用事件循环或通过全局异步锁阻塞其他调用方。
    流程：在工作线程阻塞 client-a 的扣额，同时要求 client-b 在释放前完成，并记录实际线程身份。
    参数：monkeypatch 替换额度事务，tmp_path 隔离额度数据库。
    返回：无；通过异步断言验证事件循环可调度和按 clientId 隔离。
    异常边界：等待均设置超时，失败时仍释放工作线程，避免测试进程悬挂。
    """

    limiter = ClientRateLimiter(
        per_minute=10, daily_quota=10, database_file=str(tmp_path / "rate.sqlite3")
    )
    main_thread_id = threading.get_ident()
    client_a_started = threading.Event()
    release_client_a = threading.Event()
    worker_thread_ids: list[int] = []

    def consume_daily_quota(client_id: str, unused_now_utc: datetime) -> None:
        """模拟可能阻塞的 SQLite 扣额并记录执行线程。

        用途：确认 check 没有在事件循环线程直接执行同步 IO。
        流程：记录线程；client-a 等待释放，client-b 立即返回。
        参数：client_id 标识隔离调用方，unused_now_utc 保持被替换方法签名。
        返回：无。
        异常边界：client-a 最多等待一秒，避免断言失败导致测试永久阻塞。
        """

        _ = unused_now_utc
        worker_thread_ids.append(threading.get_ident())
        if client_id == "client-a":
            client_a_started.set()
            assert release_client_a.wait(timeout=1)

    monkeypatch.setattr(limiter, "_consume_daily_quota", consume_daily_quota)
    client_a_task = asyncio.create_task(limiter.check("client-a"))
    try:
        assert await asyncio.to_thread(client_a_started.wait, 1)
        await asyncio.wait_for(limiter.check("client-b"), timeout=0.5)
    finally:
        release_client_a.set()
        await client_a_task

    assert worker_thread_ids
    assert all(thread_id != main_thread_id for thread_id in worker_thread_ids)
    assert list(limiter._minute_hits["client-a"])
    assert list(limiter._minute_hits["client-b"])


@pytest.mark.asyncio
async def test_tc_rate_007_concurrent_daily_quota_is_atomic(tmp_path: Path) -> None:
    """验证并发请求只能原子取得一个 UTC 日额度。

    用途：防止线程卸载后两个 SQLite 连接同时读到旧计数并超发日额度。
    流程：两个独立额度器并发扣减同一数据库、同一 clientId 和单次日额度，再核对成功与拒绝各一次。
    参数：tmp_path 提供当前用例独占的 SQLite 文件。
    返回：无；通过结果集合验证事务原子性。
    异常边界：允许任一请求先获得写锁，但拒绝结果必须稳定为 DAILY_QUOTA_EXCEEDED。
    """

    database = str(tmp_path / "atomic.sqlite3")
    first_limiter = ClientRateLimiter(
        per_minute=10, daily_quota=1, database_file=database
    )
    second_limiter = ClientRateLimiter(
        per_minute=10, daily_quota=1, database_file=database
    )
    results = await asyncio.gather(
        first_limiter.check("client-a"),
        second_limiter.check("client-a"),
        return_exceptions=True,
    )

    successes = [result for result in results if result is None]
    errors = [result for result in results if isinstance(result, ApiError)]
    assert len(successes) == 1
    assert len(errors) == 1
    assert errors[0].code == "DAILY_QUOTA_EXCEEDED"
