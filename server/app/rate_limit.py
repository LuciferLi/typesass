"""按调用方执行分钟限流和持久化日额度。"""

import asyncio
from collections import defaultdict, deque
from datetime import datetime, timedelta, timezone
import math
from pathlib import Path
import sqlite3
import time
from typing import Deque, Dict

from .errors import ApiError


class ClientRateLimiter:
    """按 clientId 控制分钟请求数和持久化 UTC 日额度。

    用途：阻止单个第三方独占上游资源，并确保发布、崩溃或重启不会重置成本额度。
    流程：内存滚动窗口负责突发保护；SQLite ``BEGIN IMMEDIATE`` 原子更新 UTC 自然日计数。
    边界：当前支持单 worker；横向扩容必须把分钟窗口迁移到共享网关，SQLite 日额度仍可共享同一持久卷。
    """

    def __init__(self, per_minute: int, daily_quota: int, database_file: str) -> None:
        """初始化额度器和 SQLite 表。

        参数：分钟上限、UTC 日额度和数据库文件路径。
        流程：创建父目录与计数表，再初始化调用方分钟窗口。
        边界：数据库不可创建时直接阻止应用启动，避免无成本门禁运行。
        """

        self._per_minute = per_minute
        self._daily_quota = daily_quota
        self._database_file = database_file
        self._minute_hits: Dict[str, Deque[float]] = defaultdict(deque)
        self._client_locks: Dict[str, asyncio.Lock] = defaultdict(asyncio.Lock)
        parent = Path(database_file).expanduser().parent
        parent.mkdir(parents=True, exist_ok=True)
        with self._connect() as connection:
            connection.execute(
                """CREATE TABLE IF NOT EXISTS client_daily_usage (
                    client_id TEXT NOT NULL,
                    quota_date TEXT NOT NULL,
                    request_count INTEGER NOT NULL,
                    PRIMARY KEY (client_id, quota_date)
                )"""
            )

    def _connect(self) -> sqlite3.Connection:
        """打开短生命周期 SQLite 连接。

        流程：启用超时和 WAL，供重启及多进程管理工具安全读取同一持久卷。
        返回：配置完成的连接。
        异常边界：IO/锁错误由 check 转换为 fail-closed 服务错误。
        """

        connection = sqlite3.connect(self._database_file, timeout=5)
        connection.execute("PRAGMA journal_mode=WAL")
        return connection

    async def check(self, client_id: str) -> None:
        """校验并扣减调用方额度。

        参数：``client_id`` 为已验签身份。
        流程：按调用方串行检查滚动分钟窗口，再把 SQLite 原子扣额卸载到工作线程，成功后记录分钟命中。
        返回：无。
        异常边界：同一调用方的并发请求不会突破分钟窗口；额度库异常时失败关闭且不记录分钟命中。
        """

        async with self._client_locks[client_id]:
            now_monotonic = time.monotonic()
            now_utc = datetime.now(timezone.utc)
            minute_hits = self._minute_hits[client_id]
            while minute_hits and minute_hits[0] <= now_monotonic - 60:
                minute_hits.popleft()
            if len(minute_hits) >= self._per_minute:
                retry_after = max(1, math.ceil(60 - (now_monotonic - minute_hits[0])))
                raise ApiError(
                    429,
                    "RATE_LIMIT",
                    "调用频率超过限制。",
                    {"Retry-After": str(retry_after)},
                )
            try:
                await asyncio.to_thread(self._consume_daily_quota, client_id, now_utc)
            except ApiError:
                raise
            except sqlite3.Error as error:
                raise ApiError(
                    503,
                    "QUOTA_STORE_UNAVAILABLE",
                    "额度服务暂不可用。",
                    {"Retry-After": "5"},
                ) from error
            minute_hits.append(now_monotonic)

    def _consume_daily_quota(self, client_id: str, now_utc: datetime) -> None:
        """原子扣减 UTC 日额度。

        参数：调用方 ID 与带时区 UTC 当前时间。
        流程：立即事务读取当日计数，达到上限则回滚，否则 upsert 加一并提交。
        返回：无。
        异常边界：超限 Retry-After 精确计算到下一 UTC 自然日，不固定返回 86400。
        """

        quota_date = now_utc.date().isoformat()
        next_day = datetime.combine(
            now_utc.date() + timedelta(days=1), datetime.min.time(), tzinfo=timezone.utc
        )
        retry_after = max(1, math.ceil((next_day - now_utc).total_seconds()))
        with self._connect() as connection:
            connection.execute("BEGIN IMMEDIATE")
            row = connection.execute(
                "SELECT request_count FROM client_daily_usage WHERE client_id = ? AND quota_date = ?",
                (client_id, quota_date),
            ).fetchone()
            request_count = int(row[0]) if row else 0
            if request_count >= self._daily_quota:
                connection.rollback()
                raise ApiError(
                    429,
                    "DAILY_QUOTA_EXCEEDED",
                    "UTC 当日调用额度已耗尽。",
                    {"Retry-After": str(retry_after)},
                )
            connection.execute(
                """INSERT INTO client_daily_usage (client_id, quota_date, request_count)
                   VALUES (?, ?, 1)
                   ON CONFLICT(client_id, quota_date)
                   DO UPDATE SET request_count = request_count + 1""",
                (client_id, quota_date),
            )
            connection.commit()
