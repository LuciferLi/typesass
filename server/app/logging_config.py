"""结构化脱敏日志配置。"""

import json
import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path
import sys
from typing import Dict

from .config import Settings


REDACTED_KEYS = {
    "authorization",
    "api_key",
    "apikey",
    "audio_base64",
    "audiobase64",
    "token",
}


def _redact(value: object) -> object:
    """递归脱敏日志扩展字段。

    用途：阻止 Token、API Key 和音频正文进入控制台或轮转日志。
    流程：字典按小写键匹配敏感集合，列表递归处理，其余值保持不变。
    参数：``value`` 为待写日志的结构化值。
    返回：已脱敏且可 JSON 序列化的值。
    异常边界：未知对象转为字符串，不调用其外部资源。
    """

    if isinstance(value, dict):
        return {
            str(key): "[REDACTED]"
            if str(key).lower() in REDACTED_KEYS
            else _redact(item)
            for key, item in value.items()
        }
    if isinstance(value, (list, tuple)):
        return [_redact(item) for item in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


class JsonFormatter(logging.Formatter):
    """单行 JSON 日志格式器。

    用途：统一控制台和文件日志结构，便于按请求 ID 检索。
    流程：输出时间、级别、logger、消息和可选 context，并对 context 递归脱敏。
    边界：异常堆栈仅写服务端内部日志，不进入 HTTP 响应。
    """

    def format(self, record: logging.LogRecord) -> str:
        """格式化一条日志记录。

        用途：把标准 ``LogRecord`` 转成稳定 JSON 字符串。
        流程：收集基础字段、脱敏 context，并按需附加异常堆栈。
        参数：``record`` 为 Python logging 记录。
        返回：UTF-8 友好的单行 JSON 文本。
        异常边界：context 不可序列化时由 ``_redact`` 转为字符串。
        """

        payload: Dict[str, object] = {
            "timestamp": self.formatTime(record, "%Y-%m-%dT%H:%M:%S%z"),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }
        context = getattr(record, "context", None)
        if context is not None:
            payload["context"] = _redact(context)
        if record.exc_info:
            payload["exception"] = self.formatException(record.exc_info)
        return json.dumps(payload, ensure_ascii=False, separators=(",", ":"))


def configure_logging(settings: Settings) -> None:
    """配置控制台和轮转文件日志。

    用途：在应用启动时建立唯一日志出口并限制磁盘占用。
    流程：创建日志目录，添加 stdout 与 ``RotatingFileHandler``，替换根 logger handler。
    参数：``settings`` 提供日志路径、单文件大小和保留数量。
    返回：无。
    异常边界：目录或文件不可写时直接抛错，避免无日志运行。
    """

    log_path = Path(settings.log_file).expanduser()
    log_path.parent.mkdir(parents=True, exist_ok=True)
    formatter = JsonFormatter()
    stream_handler = logging.StreamHandler(sys.stdout)
    stream_handler.setFormatter(formatter)
    file_handler = RotatingFileHandler(
        log_path,
        maxBytes=settings.log_max_bytes,
        backupCount=settings.log_backup_count,
        encoding="utf-8",
    )
    file_handler.setFormatter(formatter)
    root_logger = logging.getLogger()
    root_logger.handlers = [stream_handler, file_handler]
    root_logger.setLevel(logging.INFO)
