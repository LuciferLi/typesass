use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tauri::{AppHandle, Manager};

/// 桌面错误活动日志达到该大小后轮转，避免长期运行无限占用磁盘。
const DESKTOP_ERROR_LOG_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// 桌面错误备份日志最长保留时间。
const DESKTOP_ERROR_LOG_RETENTION: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// 桌面错误日志的进程内全局写锁；覆盖清理、轮转与追加，避免多个 Tauri 命令同时写入时互相重命名或丢失记录。
static DESKTOP_ERROR_LOG_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 记录可由用户诊断的桌面核心链路错误，并返回可直接展示的安全错误信息。
/// 流程：生成唯一诊断 ID，只保留白名单错误码、白名单操作名和固定摘要，轮转活动日志后追加单行 JSON。
/// 参数：app 用于定位应用数据目录，code/operation/context_id 标识失败位置，error 为待脱敏原始错误。
/// 返回：包含安全摘要、稳定错误码和诊断 ID 的用户文案。
/// 异常/边界：context_id 与 error 仅为兼容调用契约而接收，绝不进入日志或用户文案；日志写入失败不会覆盖原始业务错误。
pub fn record_desktop_error(
    app: &AppHandle,
    code: &str,
    operation: &str,
    _context_id: Option<&str>,
    _error: &str,
) -> String {
    let diagnostic_id = uuid::Uuid::new_v4().to_string();
    let (safe_code, safe_operation, safe_summary) = safe_error_metadata(code, operation);
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let log_dir = app_data_dir.join("logs");
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let payload = json!({
            "timestampMs": timestamp_ms,
            "diagnosticId": diagnostic_id,
            "code": safe_code,
            "operation": safe_operation,
            "errorSummary": safe_summary
        });
        append_desktop_error_log(&log_dir, &payload);
    }
    format!(
        "{}（错误码：{}，诊断 ID：{}）",
        safe_summary, safe_code, diagnostic_id
    )
}

/// 把桌面错误收敛为白名单日志元数据。
/// 流程：错误码只接受当前桌面核心链路声明的固定值并映射固定中文摘要，操作名只接受内部命令白名单；未知值统一降级。
/// 参数：code 和 operation 来自内部调用点；返回安全错误码、安全操作名和固定用户摘要。
/// 异常/边界：不接收也不解析外部错误正文、prompt、路径、模型响应或密钥，因此这些内容无法通过关键词变形绕过脱敏进入日志。
fn safe_error_metadata(code: &str, operation: &str) -> (&'static str, &'static str, &'static str) {
    let (safe_code, safe_summary) = match code {
        "MODEL_CONFIG_INVALID" => ("MODEL_CONFIG_INVALID", "模型配置无效。"),
        "MODEL_API_KEY_REQUIRED" => ("MODEL_API_KEY_REQUIRED", "模型凭据缺失。"),
        "MODEL_CREDENTIAL_UNAVAILABLE" => ("MODEL_CREDENTIAL_UNAVAILABLE", "模型凭据不可用。"),
        "MODEL_UPSTREAM_AUTH_FAILED" => ("MODEL_UPSTREAM_AUTH_FAILED", "模型服务鉴权失败。"),
        "MODEL_CONNECTION_FAILED" => ("MODEL_CONNECTION_FAILED", "无法连接模型服务。"),
        "MODEL_TEST_CLIENT_UNAVAILABLE" => {
            ("MODEL_TEST_CLIENT_UNAVAILABLE", "模型测试服务不可用。")
        }
        "MODEL_UPSTREAM_RESPONSE_INVALID" => {
            ("MODEL_UPSTREAM_RESPONSE_INVALID", "模型服务响应无效。")
        }
        "MODEL_UPSTREAM_CONTRACT_INVALID" => {
            ("MODEL_UPSTREAM_CONTRACT_INVALID", "模型服务响应无效。")
        }
        "MODEL_LIST_FAILED" => ("MODEL_LIST_FAILED", "读取模型配置失败。"),
        "MODEL_CATALOG_LOAD_FAILED" => ("MODEL_CATALOG_LOAD_FAILED", "读取模型配置失败。"),
        "MODEL_SAVE_FAILED" => ("MODEL_SAVE_FAILED", "保存模型配置失败。"),
        "MODEL_DELETE_FAILED" => ("MODEL_DELETE_FAILED", "删除模型配置失败。"),
        "MODEL_TEST_INTERNAL_FAILED" => ("MODEL_TEST_INTERNAL_FAILED", "模型连通性测试失败。"),
        "SIDECAR_START_FAILED" => ("SIDECAR_START_FAILED", "本机 AI 服务启动失败。"),
        "SIDECAR_MODEL_RELOAD_FAILED" => (
            "SIDECAR_MODEL_RELOAD_FAILED",
            "本机 AI 服务模型目录刷新失败。",
        ),
        "SIDECAR_TOKEN_STORE_FAILED" => ("SIDECAR_TOKEN_STORE_FAILED", "本机 AI 服务初始化失败。"),
        "SIDECAR_SHUTDOWN_FAILED" => ("SIDECAR_SHUTDOWN_FAILED", "本机 AI 服务停止失败。"),
        "PRIVATE_RPC_START_FAILED" => ("PRIVATE_RPC_START_FAILED", "本机业务桥接启动失败。"),
        "PRIVATE_RPC_SHUTDOWN_FAILED" => ("PRIVATE_RPC_SHUTDOWN_FAILED", "本机业务桥接停止失败。"),
        "WEB_SERVER_START_FAILED" => ("WEB_SERVER_START_FAILED", "内置 Web 服务启动失败。"),
        "WEB_SERVER_SHUTDOWN_FAILED" => ("WEB_SERVER_SHUTDOWN_FAILED", "内置 Web 服务停止失败。"),
        "TASK_WORKSPACE_LOAD_FAILED" => ("TASK_WORKSPACE_LOAD_FAILED", "读取任务工作区失败。"),
        "TASK_PROJECT_CREATE_FAILED" => ("TASK_PROJECT_CREATE_FAILED", "创建任务项目失败。"),
        "TASK_PROJECT_UPDATE_FAILED" => ("TASK_PROJECT_UPDATE_FAILED", "更新任务项目失败。"),
        "TASK_PROJECT_DELETE_FAILED" => ("TASK_PROJECT_DELETE_FAILED", "删除任务项目失败。"),
        "TASK_CREATE_FAILED" => ("TASK_CREATE_FAILED", "创建任务失败。"),
        "TASK_QUEUE_FAILED" => ("TASK_QUEUE_FAILED", "任务入队失败。"),
        "TASK_ACCEPTANCE_FAILED" => ("TASK_ACCEPTANCE_FAILED", "更新任务验收状态失败。"),
        "TASK_DISPATCH_RUNNING_LIST_FAILED" => {
            ("TASK_DISPATCH_RUNNING_LIST_FAILED", "任务调度同步失败。")
        }
        "TASK_DISPATCH_QUEUE_LIST_FAILED" => {
            ("TASK_DISPATCH_QUEUE_LIST_FAILED", "任务调度同步失败。")
        }
        "TASK_EXECUTION_FAILED" => ("TASK_EXECUTION_FAILED", "任务执行失败。"),
        "TASK_DISPATCH_CAS_MISSED" => ("TASK_DISPATCH_CAS_MISSED", "任务领取暂未成功。"),
        "TASK_INCREMENTAL_READ_FAILED" => {
            ("TASK_INCREMENTAL_READ_FAILED", "读取任务增量状态失败。")
        }
        "TASK_FAILURE_PERSIST_FAILED" => ("TASK_FAILURE_PERSIST_FAILED", "保存任务失败状态失败。"),
        "CODEX_SEND_UNCERTAIN" => ("CODEX_SEND_UNCERTAIN", "任务发送结果无法确认。"),
        "CODEX_CDP_TARGET_CHECK_FAILED" => {
            ("CODEX_CDP_TARGET_CHECK_FAILED", "验证 Codex 主页面失败。")
        }
        "CODEX_NOT_CONNECTED" => ("CODEX_NOT_CONNECTED", "Codex 尚未建立本机连接。"),
        "CODEX_CDP_INPUT_INVALID" => ("CODEX_CDP_INPUT_INVALID", "Codex 任务输入无效。"),
        "CODEX_CDP_PROMPT_TOO_LARGE" => ("CODEX_CDP_PROMPT_TOO_LARGE", "Codex 任务内容超过上限。"),
        "CODEX_CDP_CONNECT_FAILED" => ("CODEX_CDP_CONNECT_FAILED", "连接 Codex 主页面失败。"),
        "CODEX_CDP_TARGET_INVALID" => ("CODEX_CDP_TARGET_INVALID", "Codex 主页面地址无效。"),
        "CODEX_CDP_PROTOCOL_FAILED" => ("CODEX_CDP_PROTOCOL_FAILED", "Codex 页面协议交互失败。"),
        "CODEX_CDP_WORKSPACE_SWITCH_FAILED" => (
            "CODEX_CDP_WORKSPACE_SWITCH_FAILED",
            "Codex 工作空间切换失败。",
        ),
        "CODEX_CDP_NEW_CHAT_FAILED" => ("CODEX_CDP_NEW_CHAT_FAILED", "Codex 新会话导航失败。"),
        "CODEX_CDP_COMPOSER_NOT_READY" => {
            ("CODEX_CDP_COMPOSER_NOT_READY", "Codex 输入框尚未就绪。")
        }
        "CODEX_CDP_COMPOSER_WRITE_FAILED" => {
            ("CODEX_CDP_COMPOSER_WRITE_FAILED", "Codex 输入框写入失败。")
        }
        "CODEX_CDP_ATTACHMENT_INPUT_MISSING" => (
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex 附件上传入口不可用。",
        ),
        "CODEX_CDP_ATTACHMENT_INVALID" => ("CODEX_CDP_ATTACHMENT_INVALID", "任务图片附件无效。"),
        "CODEX_CDP_ATTACHMENT_WRITE_FAILED" => (
            "CODEX_CDP_ATTACHMENT_WRITE_FAILED",
            "写入任务图片附件失败。",
        ),
        "CODEX_CDP_SUBMISSION_PERSIST_FAILED" => (
            "CODEX_CDP_SUBMISSION_PERSIST_FAILED",
            "保存 Codex 提交阶段失败。",
        ),
        "CDP_SUBMISSION_RECOVERY_LOAD_FAILED" => (
            "CDP_SUBMISSION_RECOVERY_LOAD_FAILED",
            "读取 Codex 提交恢复状态失败。",
        ),
        "CODEX_RESTART_FAILED" => ("CODEX_RESTART_FAILED", "Codex 重启失败。"),
        "CODEX_RESTART_TIMEOUT" => ("CODEX_RESTART_TIMEOUT", "等待 Codex 重启超时。"),
        "CODEX_CDP_PORT_IN_USE" => ("CODEX_CDP_PORT_IN_USE", "Codex 连接端口被占用。"),
        "CODEX_CONNECTION_STATE_FAILED" => (
            "CODEX_CONNECTION_STATE_FAILED",
            "无法确认 Codex 连接进程状态。",
        ),
        "RUNNING_RECOVERY_LIST_FAILED" => ("RUNNING_RECOVERY_LIST_FAILED", "恢复运行中任务失败。"),
        "RECONCILE_UNRECOVERABLE_PERSIST_FAILED" => (
            "RECONCILE_UNRECOVERABLE_PERSIST_FAILED",
            "保存任务同步状态失败。",
        ),
        "RECONCILE_TERMINAL_PERSIST_FAILED" => (
            "RECONCILE_TERMINAL_PERSIST_FAILED",
            "保存任务同步状态失败。",
        ),
        "RECONCILE_MISSING_PERSIST_FAILED" => {
            ("RECONCILE_MISSING_PERSIST_FAILED", "保存任务同步状态失败。")
        }
        "RECONCILE_CIRCUIT_BREAK_PERSIST_FAILED" => (
            "RECONCILE_CIRCUIT_BREAK_PERSIST_FAILED",
            "保存任务同步状态失败。",
        ),
        "RECONCILE_EMPTY_THREAD_CONFIRMED" => (
            "RECONCILE_EMPTY_THREAD_CONFIRMED",
            "任务线程为空，已确认无法恢复。",
        ),
        "RECONCILE_AMBIGUOUS_TURNS" => (
            "RECONCILE_AMBIGUOUS_TURNS",
            "任务状态存在多个候选，暂无法同步。",
        ),
        "TURN_START_RESPONSE_UNCERTAIN" => {
            ("TURN_START_RESPONSE_UNCERTAIN", "任务启动状态待确认。")
        }
        "TURN_START_ID_UNCERTAIN" => ("TURN_START_ID_UNCERTAIN", "任务启动状态待确认。"),
        "TURN_BIND_UNCERTAIN" => ("TURN_BIND_UNCERTAIN", "任务启动状态待确认。"),
        "TURN_NOTIFICATION_UNCERTAIN" => ("TURN_NOTIFICATION_UNCERTAIN", "任务运行状态待确认。"),
        "TURN_TERMINAL_PERSIST_RETRY" => {
            ("TURN_TERMINAL_PERSIST_RETRY", "保存任务终态失败，将重试。")
        }
        "RECONCILE_RETRY" => ("RECONCILE_RETRY", "任务状态同步失败，将重试。"),
        "RECONCILE_EMPTY_THREAD_PERSIST_FAILED" => (
            "RECONCILE_EMPTY_THREAD_PERSIST_FAILED",
            "保存空线程确认状态失败。",
        ),
        _ => ("DESKTOP_OPERATION_FAILED", "桌面操作失败，请稍后重试。"),
    };
    let safe_operation = match operation {
        "list_private_models" => "list_private_models",
        "save_private_model" => "save_private_model",
        "delete_private_model" => "delete_private_model",
        "sidecar_model_catalog_reload" => "sidecar_model_catalog_reload",
        "test_private_model" => "test_private_model",
        "load_session_workspace_data" => "load_session_workspace_data",
        "create_session_project" => "create_session_project",
        "update_session_project" => "update_session_project",
        "delete_session_project" => "delete_session_project",
        "create_session_task" => "create_session_task",
        "queue_session_task" => "queue_session_task",
        "complete_session_task" => "complete_session_task",
        "app_setup" => "app_setup",
        "app_exit" => "app_exit",
        "codex_task" => "codex_task",
        "restart_codex_desktop" => "restart_codex_desktop",
        "get_codex_connection" => "get_codex_connection",
        "restart_codex" => "restart_codex",
        _ => "desktop_operation",
    };
    (safe_code, safe_operation, safe_summary)
}

/// 在全局临界区内轮转并追加一条桌面错误日志。
/// 流程：取得进程级写锁，创建日志目录，清理过期备份，检查活动文件上限并完成单备份轮转，最后追加单行 JSON。
/// 参数：log_dir 为 App 数据目录下的日志目录，payload 为已完成脱敏且不含业务正文的 JSON 对象；返回无。
/// 异常/边界：锁中毒时恢复其内部状态继续记录；目录、元数据、轮转或追加失败均静默，不反向覆盖原业务错误。
fn append_desktop_error_log(log_dir: &Path, payload: &serde_json::Value) {
    let _write_guard = DESKTOP_ERROR_LOG_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let log_path = log_dir.join("desktop-errors.log");
    let backup_path = log_dir.join("desktop-errors.log.1");
    if fs::create_dir_all(log_dir).is_err() {
        return;
    }
    remove_expired_backup(&backup_path);
    if fs::metadata(&log_path)
        .map(|metadata| metadata.len() >= DESKTOP_ERROR_LOG_MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&backup_path);
        let _ = fs::rename(&log_path, &backup_path);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{}", payload);
    }
}

/// 清理超过保留期的单个轮转备份。
/// 流程：读取修改时间并与当前时间比较，过期时删除；参数为固定 `.1` 备份路径；返回无。
/// 异常/边界：文件不存在、时间异常或删除失败均保持静默，不影响业务错误返回和活动日志写入。
fn remove_expired_backup(backup_path: &std::path::Path) {
    let expired = fs::metadata(backup_path)
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| {
            SystemTime::now()
                .duration_since(modified)
                .map_err(std::io::Error::other)
        })
        .map(|age| age > DESKTOP_ERROR_LOG_RETENTION)
        .unwrap_or(false);
    if expired {
        let _ = fs::remove_file(backup_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    /// 白名单错误码必须映射固定摘要，且摘要不依赖任何外部错误正文。
    #[test]
    fn desktop_error_metadata_uses_stable_whitelist_summary() {
        assert_eq!(
            safe_error_metadata("MODEL_UPSTREAM_AUTH_FAILED", "test_private_model"),
            (
                "MODEL_UPSTREAM_AUTH_FAILED",
                "test_private_model",
                "模型服务鉴权失败。"
            )
        );
        for task_code in [
            "RECONCILE_EMPTY_THREAD_CONFIRMED",
            "RECONCILE_AMBIGUOUS_TURNS",
            "TURN_START_RESPONSE_UNCERTAIN",
            "TURN_START_ID_UNCERTAIN",
            "TURN_BIND_UNCERTAIN",
            "TURN_NOTIFICATION_UNCERTAIN",
            "TURN_TERMINAL_PERSIST_RETRY",
            "RECONCILE_RETRY",
            "RECONCILE_EMPTY_THREAD_PERSIST_FAILED",
            "CODEX_SEND_UNCERTAIN",
            "CODEX_CDP_WORKSPACE_SWITCH_FAILED",
            "CODEX_CDP_COMPOSER_WRITE_FAILED",
            "CODEX_CDP_SUBMISSION_PERSIST_FAILED",
            "CDP_SUBMISSION_RECOVERY_LOAD_FAILED",
            "CODEX_CONNECTION_STATE_FAILED",
        ] {
            let metadata = safe_error_metadata(task_code, "codex_task");
            assert_eq!(metadata.0, task_code);
            assert_eq!(metadata.1, "codex_task");
            assert_ne!(metadata.2, "桌面操作失败，请稍后重试。");
        }
        let restart = safe_error_metadata("CODEX_RESTART_FAILED", "restart_codex_desktop");
        assert_eq!(restart.0, "CODEX_RESTART_FAILED");
        assert_eq!(restart.1, "restart_codex_desktop");
    }

    /// 未知错误码和操作名必须整体降级，避免 prompt、路径或非标准密钥伪装成诊断字段落盘。
    #[test]
    fn desktop_error_metadata_rejects_arbitrary_fields() {
        let metadata = safe_error_metadata(
            "sk-nonstandard-secret",
            "/Users/private/project prompt=do-secret-work",
        );
        let serialized = serde_json::to_string(&metadata).expect("安全元数据应能序列化");
        assert_eq!(
            metadata,
            (
                "DESKTOP_OPERATION_FAILED",
                "desktop_operation",
                "桌面操作失败，请稍后重试。"
            )
        );
        assert!(!serialized.contains("nonstandard"));
        assert!(!serialized.contains("/Users"));
        assert!(!serialized.contains("prompt"));
    }

    /// 并发写入遇到轮转阈值时必须只轮转一次，并完整保留阈值后的每条 JSON 记录。
    #[test]
    fn desktop_error_rotation_and_append_are_serialized() {
        let log_dir = std::env::temp_dir().join(format!(
            "aitool-desktop-error-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&log_dir).expect("应能创建测试日志目录");
        fs::write(
            log_dir.join("desktop-errors.log"),
            vec![b'x'; DESKTOP_ERROR_LOG_MAX_BYTES as usize],
        )
        .expect("应能写入达到轮转阈值的测试日志");

        let mut workers = Vec::new();
        for worker_index in 0..8 {
            let worker_log_dir = log_dir.clone();
            workers.push(thread::spawn(move || {
                for entry_index in 0..10 {
                    append_desktop_error_log(
                        &worker_log_dir,
                        &json!({"worker": worker_index, "entry": entry_index}),
                    );
                }
            }));
        }
        for worker in workers {
            worker.join().expect("并发日志线程不应失败");
        }

        let active_log =
            fs::read_to_string(log_dir.join("desktop-errors.log")).expect("轮转后活动日志应存在");
        assert_eq!(active_log.lines().count(), 80);
        assert!(active_log
            .lines()
            .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok()));
        assert_eq!(
            fs::metadata(log_dir.join("desktop-errors.log.1"))
                .expect("达到阈值的旧日志应成为备份")
                .len(),
            DESKTOP_ERROR_LOG_MAX_BYTES
        );
        fs::remove_dir_all(log_dir).expect("应能清理测试日志目录");
    }
}
