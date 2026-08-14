#![recursion_limit = "256"]

mod codex_cdp;
mod codex_desktop;
mod desktop_error;
mod private_models;
mod private_rpc;
mod sidecar;
mod task_store;
mod web_server;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, Once};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Position, State};

use codex_desktop::{CodexConnectionStatus, CodexRestartAccepted, RuntimeCodexDesktop};
use private_models::{PrivateModelRecord, SavePrivateModelRequest};
use private_rpc::RuntimePrivateRpc;
use sidecar::RuntimeSidecar;
use task_store::{
    CreateProjectRequest, CreateTaskRequest, CreateTaskResponse, RunningTaskRecord,
    UpdateProjectRequest, WorkspaceDataResponse,
};
use web_server::RuntimeWebServer;

#[cfg(target_os = "macos")]
use objc2::rc::{autoreleasepool, Retained};
#[cfg(target_os = "macos")]
use objc2::runtime::ProtocolObject;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSArray, NSData, NSString};

const DEFAULT_ASR_TEXT_SHORTCUT: &str = "ctrl+shift+d";
const DEFAULT_DICTATE_SHORTCUT: &str = "ctrl+p";
const DEFAULT_POLISH_SHORTCUT: &str = "ctrl+shift+p";
const LOGIN_AGENT_LABEL: &str = "asia.aijob.aitool.login";
const FLOAT_WINDOW_WIDTH: f64 = 132.0;
const FLOAT_WINDOW_TOP: f64 = 60.0;
const TOAST_WINDOW_WIDTH: f64 = 460.0;
const TOAST_WINDOW_HEIGHT: f64 = 86.0;
const TOAST_WINDOW_TOP: f64 = 42.0;
const RESULT_WINDOW_WIDTH: f64 = 520.0;
const RESULT_WINDOW_HEIGHT: f64 = 320.0;
const RESULT_WINDOW_TOP: f64 = 76.0;
const RESULT_TOAST_GAP: f64 = 12.0;
const CLIPBOARD_VERIFY_INITIAL_DELAY_MS: u64 = 30;
const CLIPBOARD_VERIFY_RETRY_STEP_MS: u64 = 80;
const CLIPBOARD_RESTORE_DELAY_MS: u64 = 45;
const PASTE_DIAGNOSTIC_SETTLE_DELAY_MS: u64 = 0;
const PASTE_WINDOW_SETTLE_DELAY_MS: u64 = 40;
const PASTE_TARGET_REFOCUS_DELAY_MS: u64 = 160;
const PASTE_FOCUS_RETRY_COUNT: usize = 4;
const PASTE_FOCUS_RETRY_DELAY_MS: u64 = 75;
const LOCAL_CONFIG_FILE_NAME: &str = "codexman-config.json";
/// 系统设置分区 key；必须与前端 `StorageKey.settings` 保持一致。
const LOCAL_CONFIG_SETTINGS_KEY: &str = "codexman.settings.v1";
/// 首发客户端配置格式版本；未知版本必须拒绝，禁止静默兼容历史结构。
const LOCAL_CONFIG_VERSION: u32 = 1;
const LOCAL_CONFIG_WATCH_INTERVAL_MS: u64 = 500;
const CODEX_THREAD_LIST_LIMIT: usize = 60;
const CODEX_THREAD_MESSAGE_LIMIT: usize = 80;
const CODEX_MESSAGE_CONTENT_MAX_CHARS: usize = 6000;
/// 任务结果中最终助手文本字符上限；按最坏 JSON 转义后仍低于 task_store 的 32 KiB 结果上限。
const CODEX_TASK_RESULT_TEXT_MAX_CHARS: usize = 4000;
const CODEX_SESSION_SCAN_LIMIT: usize = 180;
/// 单次会话索引最多保留的有效记录数；超出部分按更新时间择新，避免索引长期增长导致内存无界。
const CODEX_SESSION_INDEX_ENTRY_LIMIT: usize = 500;
/// 递归枚举 sessions 时允许访问的目录数上限；超限立即失败，防止异常目录树拖垮桌面进程。
const CODEX_SESSION_DIRECTORY_LIMIT: usize = 1024;
/// 递归枚举 sessions 时允许发现的 JSONL 文件数上限；这是首发版可管理的本机会话总量边界。
const CODEX_SESSION_FILE_ENUM_LIMIT: usize = 500;
/// 按 thread ID 精确定位详情时允许检查的目录条目数；独立于 supplemental 500 文件列表预算。
const CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT: usize = 4096;
/// 单个 session JSONL 文件允许的最大字节数；打开句柄后同时校验元数据并限制实际读取量。
const CODEX_SESSION_FILE_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// 一次递归枚举允许覆盖的 session JSONL 总字节数；超限停止补充扫描并保留已收集候选。
const CODEX_SESSION_TOTAL_BYTES_LIMIT: u64 = 512 * 1024 * 1024;
/// Codex 官方 session_index.jsonl 最大字节数；打开句柄后最多读取上限加一字节。
const CODEX_SESSION_INDEX_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// 单条本地 session JSONL 事件最大字节数；超限拒绝整个文件且不回显事件正文。
const CODEX_SESSION_FRAME_MAX_BYTES: usize = 4 * 1024 * 1024;
const CODEX_SESSION_SUMMARY_MAX_LINES: usize = 120;
const CODEX_DESKTOP_BIN: &str = "/Applications/ChatGPT.app/Contents/Resources/codex";
/// app-server stdout 消息通道容量；读线程达到上限后阻塞，向子进程施加背压而不是无限堆内存。
const CODEX_APP_SERVER_CHANNEL_CAPACITY: usize = 128;
/// 等待 JSON-RPC 响应期间允许暂存的通知上限；超限转入只读对账，避免 Vec 无界增长。
const CODEX_PENDING_NOTIFICATION_CAPACITY: usize = 128;
/// app-server 单条 JSONL 帧最大字节数；超限即停止当前读线程且不记录帧正文。
const CODEX_APP_SERVER_FRAME_MAX_BYTES: usize = 4 * 1024 * 1024;
/// Codex app-server stderr 单条记录消费上限；超出后继续丢弃到换行，只记录固定脱敏诊断。
const CODEX_APP_SERVER_STDERR_RECORD_MAX_BYTES: usize = 64 * 1024;
/// Codex app-server 活动诊断日志最大字节数；每次写入前实时检查并轮转。
const CODEX_APP_SERVER_DIAGNOSTIC_LOG_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Codex app-server 诊断日志写锁；串行化 stderr 与 stdout 协议错误的轮转和追加。
static CODEX_APP_SERVER_DIAGNOSTIC_LOG_LOCK: Mutex<()> = Mutex::new(());
/// 当前进程只执行一次的诊断日志初始化，确保旧版可能写入的原始 stderr 不会继续留在磁盘。
static CODEX_APP_SERVER_DIAGNOSTIC_LOG_INITIALIZE: Once = Once::new();
/// 活跃 turn 的只读对账间隔；短任务完成后最多约一秒刷新到待验收。
const CODEX_RECONCILE_ACTIVE_DELAY: Duration = Duration::from_secs(1);
/// 重启对账轮询初始间隔，未发现 turn 或瞬时故障时按指数退避。
const CODEX_RECONCILE_INITIAL_DELAY: Duration = Duration::from_secs(2);
/// 重启对账最大退避间隔，避免故障期间高频启动 app-server。
const CODEX_RECONCILE_MAX_DELAY: Duration = Duration::from_secs(30);
/// 专用 thread 连续读取为空的确认次数；配合指数退避提供约 30 秒持久化一致性窗口。
const CODEX_EMPTY_THREAD_CONFIRMATIONS: usize = 5;
/// 专用 thread 出现多个 turn 时的稳定协议诊断，调用方据此区分不可自动归属的状态。
const CODEX_AMBIGUOUS_THREAD_TURNS_ERROR: &str =
    "Codex 任务专用 thread 出现多个 turn，无法可靠恢复 turnId";
/// app-server 普通 JSON-RPC 请求最大等待时间。
const CODEX_APP_SERVER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(45);
/// 任务调度器空闲或等待可用并发槽位时的轮询间隔。
const CODEX_TASK_DISPATCH_INTERVAL: Duration = Duration::from_secs(1);
/// Codex 任务默认并发数，允许多个已提交任务同时等待终态，但避免首次启动时一次性压垮 Desktop 和本机资源。
const CODEX_TASK_DEFAULT_CONCURRENT_RUNNING: usize = 3;
/// Codex 任务最小并发数；设置页非法值会回落默认值，调度器仍以该值兜底防止 0 并发。
const CODEX_TASK_MIN_CONCURRENT_RUNNING: usize = 1;
/// Codex 任务最大并发数；限制用户配置上限，避免一次性打开过多任务导致 Desktop 或本机资源不可控。
const CODEX_TASK_MAX_CONCURRENT_RUNNING: usize = 10;
/// Enter 后等待 Codex session JSONL 持久化并恢复唯一 thread 的最长时间。
const CODEX_CDP_THREAD_RECOVERY_TIMEOUT: Duration = Duration::from_secs(30);
/// CDP thread 恢复轮询间隔，避免高频扫描本地 session 文件。
const CODEX_CDP_THREAD_RECOVERY_INTERVAL: Duration = Duration::from_millis(250);
/// Token 续签和清除 IPC 的稳定桌面错误码；统一入口会附加唯一诊断 ID，且不记录 Token 正文。
const PUBLIC_API_TOKEN_IPC_ERROR_CODE: &str = "DESKTOP_OPERATION_FAILED";
const BROWSER_EXTENSION_ZIP_BYTES: &[u8] =
    include_bytes!("../../public/downloads/typesass-extension.zip");
const BROWSER_EXTENSION_ZIP_FILE_NAME: &str = "typesass-extension.zip";
const ACCESS_TOKEN_APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// 浏览器插件 ZIP 下载结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserExtensionDownloadResponse {
    /// ZIP 文件最终保存到本机的绝对路径。
    file_path: String,
}

/// 本机可打开的应用选项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationOption {
    /// 应用展示名称，通常取自 .app bundle 文件名。
    name: String,
    /// 应用 bundle 的绝对路径，用于通过 open 命令精确打开。
    path: String,
}

/// 浏览器插件发起的 App 授权确认事件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccessTokenApprovalEvent {
    /// 本次 HTTP 请求追踪 ID，前端确认时原样带回。
    request_id: String,
    /// 申请方展示名称。
    name: String,
    /// 授权码到期时间；空值表示永久有效。
    expires_at: Option<String>,
}

/// 前端确认授权后返回给 HTTP sidecar 的结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccessTokenApprovalResponse {
    /// 用户是否确认授权。
    approved: bool,
    /// 拒绝、超时或界面不可用时的安全说明。
    message: Option<String>,
}

/// 待用户确认的授权申请。
struct PendingAccessTokenApproval {
    /// 本次申请展示给前端的结构化事件。
    event: AccessTokenApprovalEvent,
    /// 等待确认的同步响应通道，只能消费一次。
    responder: mpsc::Sender<AccessTokenApprovalResponse>,
}

/// 运行期间保存当前唯一一条 App 授权确认请求。
#[derive(Default)]
struct RuntimeAccessTokenApproval {
    /// 当前待确认申请；避免多个插件请求同时弹出多个授权框。
    pending: Mutex<Option<PendingAccessTokenApproval>>,
}

/// 运行期间保存的全局快捷键映射。
struct RuntimeShortcuts {
    /// 各模式当前实际注册的快捷键。
    profile: Mutex<ShortcutProfile>,
    /// 最近一次系统快捷键注册结果，用于设置页展示真实诊断。
    registration_status: Mutex<ShortcutRegistrationStatus>,
}

impl Default for RuntimeShortcuts {
    fn default() -> Self {
        Self {
            profile: Mutex::new(ShortcutProfile::default()),
            registration_status: Mutex::new(ShortcutRegistrationStatus::default()),
        }
    }
}

/// 全局快捷键触发录音时的窗口和目标 App 决策。
#[derive(Debug, Clone, PartialEq, Eq)]
struct VoiceTriggerContext {
    /// 本次录音触发时观测到的外部 App，仅用于日志展示。
    target_app: String,
    /// 是否需要展示顶部悬浮胶囊。
    show_floating_window: bool,
    /// 是否必须保持 Hub 主界面不受录音影响。
    keep_hub_visible: bool,
}

/// 自动粘贴前的当前焦点 App 与主界面保留决策。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PasteTargetDecision {
    /// 本次粘贴时系统当前前台 App；只用于日志和是否允许发送粘贴。
    target_app: String,
    /// 是否允许隐藏 Hub；默认保留，避免粘贴时主界面被收起。
    should_hide_hub: bool,
}

/// 粘贴前通过辅助功能读取到的当前焦点状态。
struct PasteFocusStatus {
    /// 当前焦点是否像可输入文本控件。
    ready: bool,
    /// 焦点元素的可读摘要，用于诊断日志和结果兜底提示。
    summary: String,
}

/// 粘贴前可恢复的输入焦点快照。
#[derive(Debug, Clone)]
struct PasteFocusSnapshot {
    /// 录音开始时的外部目标 App。
    target_app: String,
    /// 录音开始时的输入控件摘要。
    summary: String,
    /// 输入控件中心点横坐标，用于 AX 焦点丢失后恢复同一输入区域。
    center_x: i64,
    /// 输入控件中心点纵坐标，用于 AX 焦点丢失后恢复同一输入区域。
    center_y: i64,
}

/// 运行期间保存最近一次口述触发时的外部输入焦点，只用于本次自动粘贴前恢复焦点。
#[derive(Default)]
struct RuntimePasteFocusSnapshot {
    /// 最近一次快捷键触发时捕获到的可输入控件位置。
    snapshot: Mutex<Option<PasteFocusSnapshot>>,
}

/// 全局快捷键注册结果，避免系统冲突时只表现为按键无响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutRegistrationStatus {
    /// 各模式快捷键是否已成功注册到系统。
    ready: bool,
    /// 最近一次注册结果说明；失败时包含系统返回原因。
    message: String,
}

impl Default for ShortcutRegistrationStatus {
    fn default() -> Self {
        Self {
            ready: false,
            message: "快捷键尚未注册".to_string(),
        }
    }
}

/// 运行期间保存最近一次需要手动处理的转写结果，避免结果窗口首次加载时错过事件。
#[derive(Default)]
struct RuntimeResult {
    /// 最近一次结果兜底窗口应展示的内容。
    payload: Mutex<Option<ResultWindowPayload>>,
}

/// 运行期间在受信 WebView 之间共享公共 HTTP 短期 Token。
#[derive(Default)]
struct RuntimePublicApiToken {
    /// 当前 App 进程内的短期 Token；不写磁盘、日志或系统剪贴板，退出 App 后自动清除。
    token: Mutex<String>,
}

/// 私有模型连通性测试 IPC 响应。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivateModelTestResponse {
    /// 真实上游请求是否通过。
    success: bool,
    /// 失败时返回稳定诊断码；成功时不返回该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    /// 可向用户展示且不含密钥或响应正文的结果说明。
    message: String,
}

/// 读取本机私有模型安全元数据。
/// 流程：先验证调用窗口，再读取公开元数据；参数为 Tauri 注入的窗口；返回模型列表。
/// 异常/边界：API Key 只返回存在性，非 hub 默认拒绝，磁盘错误统一转换为脱敏诊断。
#[tauri::command]
fn list_private_models(window: tauri::WebviewWindow) -> Result<Vec<PrivateModelRecord>, String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    private_models::list_private_models(&app).map_err(|error| {
        desktop_error::record_desktop_error(
            &app,
            "MODEL_LIST_FAILED",
            "list_private_models",
            None,
            &error,
        )
    })
}

/// 保存私有模型并重启 sidecar 使注册表立即生效。
/// 流程：新增或上游关键参数变化时先执行真实探针；纯启停、设默认或改显示名直接写入本地配置，再重启 sidecar 并通过健康检查。
/// 参数：window 为可信调用窗口，request 为模型表单；返回保存项；异常时显式返回，绝不把未生效配置伪装成功。
/// 异常/边界：完整探针和最长 45 秒的 sidecar 健康检查均在线程池执行，不阻塞 Tauri 异步调度线程。
#[tauri::command]
async fn save_private_model(
    window: tauri::WebviewWindow,
    request: SavePrivateModelRequest,
) -> Result<PrivateModelRecord, String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    let diagnostic_context = request.id.clone();
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let sidecar = worker_app.state::<RuntimeSidecar>();
        let token_state = worker_app.state::<RuntimePublicApiToken>();
        let existing = if let Some(request_id) = request.id.as_deref() {
            let existing = private_models::list_private_models(&worker_app)?
                .into_iter()
                .find(|record| record.id == request_id)
                .ok_or_else(|| "待编辑的私有模型不存在".to_string())?;
            if existing.capability != request.capability {
                return Err("模型能力创建后不可修改；请新增并重新测试对应能力模型".to_string());
            }
            Some(existing)
        } else {
            None
        };
        if private_model_save_requires_probe(existing.as_ref(), &request) {
            private_models::test_private_model(Some(&worker_app), request.clone())
                .map_err(|failure| failure.message)?;
        }
        let snapshot = private_models::capture_snapshot(&worker_app)?;
        let requested_id = request.id.clone();
        let records = private_models::save_private_model(&worker_app, request)?;
        let catalog = match private_models::sidecar_catalog_json(&worker_app) {
            Ok(catalog) => catalog,
            Err(error) => {
                private_models::restore_snapshot(&worker_app, snapshot)?;
                return Err(format!(
                    "生成 sidecar 模型注册表失败，配置已回滚：{}",
                    error
                ));
            }
        };
        coordinate_sidecar_apply(
            "模型保存",
            || sidecar.restart(&worker_app, &catalog),
            || private_models::restore_snapshot(&worker_app, snapshot),
            || {
                let rollback_catalog = private_models::sidecar_catalog_json(&worker_app)?;
                sidecar.restart(&worker_app, &rollback_catalog)
            },
            |token| store_public_api_token(&token_state, token),
        )?;
        requested_id
            .as_deref()
            .and_then(|id| records.iter().find(|record| record.id == id))
            .cloned()
            .or_else(|| records.last().cloned())
            .ok_or_else(|| "保存后未找到私有模型记录".to_string())
    })
    .await
    .map_err(|_| "模型保存后台任务异常退出".to_string())?;
    result.map_err(|error| {
        desktop_error::record_desktop_error(
            &app,
            "MODEL_SAVE_FAILED",
            "save_private_model",
            diagnostic_context.as_deref(),
            &error,
        )
    })
}

/// 判断模型保存前是否必须执行真实上游探针。
/// 流程：新增模型始终测试；编辑时比较能力、协议、地址、上游模型名及是否提交新密钥，只有上游关键参数变化才测试。
/// 参数：existing 为已保存脱敏记录，request 为本次保存请求；返回是否需要探针。
/// 异常/边界：显示名、启用态和默认态不影响上游连接，因此不会因网络不可达阻止这些管理操作；API Key 无法读取比较，只要显式提交就视为轮换并强制测试。
fn private_model_save_requires_probe(
    existing: Option<&PrivateModelRecord>,
    request: &SavePrivateModelRequest,
) -> bool {
    let Some(existing) = existing else {
        return true;
    };
    request.api_key.is_some()
        || existing.capability != request.capability
        || existing.provider != request.provider.trim()
        || existing.base_url.trim_end_matches('/') != request.base_url.trim_end_matches('/')
        || existing.model_name != request.model_name.trim()
}

/// 删除私有模型并重启 sidecar 使注册表立即生效。
/// 流程：校验 hub 后在线程池删除本地模型配置，生成新目录，重启 sidecar 并更新短 Token。
/// 参数：window 为可信调用窗口，model_id 为模型 ID；成功返回空值。
/// 异常/边界：sidecar 未通过健康检查会补偿配置和旧进程；最长 45 秒检查不阻塞异步调度线程。
#[tauri::command]
async fn delete_private_model(
    window: tauri::WebviewWindow,
    model_id: String,
) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    let diagnostic_context = model_id.clone();
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let sidecar = worker_app.state::<RuntimeSidecar>();
        let token_state = worker_app.state::<RuntimePublicApiToken>();
        let snapshot = private_models::capture_snapshot(&worker_app)?;
        private_models::delete_private_model(&worker_app, &model_id)?;
        let catalog = match private_models::sidecar_catalog_json(&worker_app) {
            Ok(catalog) => catalog,
            Err(error) => {
                private_models::restore_snapshot(&worker_app, snapshot)?;
                return Err(format!(
                    "生成 sidecar 模型注册表失败，配置已回滚：{}",
                    error
                ));
            }
        };
        coordinate_sidecar_apply(
            "模型删除",
            || sidecar.restart(&worker_app, &catalog),
            || private_models::restore_snapshot(&worker_app, snapshot),
            || {
                let rollback_catalog = private_models::sidecar_catalog_json(&worker_app)?;
                sidecar.restart(&worker_app, &rollback_catalog)
            },
            |token| store_public_api_token(&token_state, token),
        )?;
        Ok(())
    })
    .await
    .map_err(|_| "模型删除后台任务异常退出".to_string())?;
    result.map_err(|error| {
        desktop_error::record_desktop_error(
            &app,
            "MODEL_DELETE_FAILED",
            "delete_private_model",
            Some(&diagnostic_context),
            &error,
        )
    })
}

/// 对未保存模型表单执行真实上游测试且不落盘。
/// 流程：校验 hub 后在线程池调用私有模型探针；成功返回说明，校验、网络、鉴权或协议失败返回稳定错误码和脱敏原因。
/// 参数：window 为可信调用窗口，request 为仅在本次 IPC 内存中存在的模型连接配置；返回结构化测试结果。
/// 异常/边界：可预期探测失败使用 `success=false`，避免 Tauri 字符串 rejection 被前端吞掉；本方法不保存配置或密钥。
#[tauri::command]
async fn test_private_model(
    window: tauri::WebviewWindow,
    request: SavePrivateModelRequest,
) -> Result<PrivateModelTestResponse, String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    let worker_app = app.clone();
    let diagnostic_context = request.id.clone();
    let response = match tauri::async_runtime::spawn_blocking(move || {
        private_models::test_private_model(Some(&worker_app), request)
    })
    .await
    .map_err(|_| "模型测试后台任务异常退出".to_string())?
    {
        Ok(message) => PrivateModelTestResponse {
            success: true,
            error_code: None,
            message,
        },
        Err(failure) if !failure.is_internal => {
            let message = desktop_error::record_desktop_error(
                &app,
                failure.code,
                "test_private_model",
                diagnostic_context.as_deref(),
                &failure.message,
            );
            PrivateModelTestResponse {
                success: false,
                error_code: Some(failure.code.to_string()),
                message,
            }
        }
        Err(failure) => {
            return Err(desktop_error::record_desktop_error(
                &app,
                "MODEL_TEST_INTERNAL_FAILED",
                "test_private_model",
                diagnostic_context.as_deref(),
                &failure.message,
            ));
        }
    };
    Ok(response)
}

/// 统一协调 sidecar 配置应用与补偿，保证磁盘状态、进程配置和短 Token 同步提交或回滚。
/// 流程：执行新配置重启并保存新 Token；任一步失败时先恢复持久化状态，再按旧状态重启 sidecar 并保存回滚 Token。
/// 参数：action 为稳定业务动作名，apply/restore_state/restore_sidecar/store_token 分别封装新进程启动、配置恢复、旧进程恢复和 Token 写入；成功返回空值。
/// 异常/边界：闭包底层错误不会进入 IPC 文案；配置或 sidecar 任一补偿失败均返回固定失败阶段，避免泄漏路径、凭据或上游响应。
fn coordinate_sidecar_apply<Apply, RestoreState, RestoreSidecar, StoreToken>(
    action: &str,
    apply: Apply,
    restore_state: RestoreState,
    restore_sidecar: RestoreSidecar,
    mut store_token: StoreToken,
) -> Result<(), String>
where
    Apply: FnOnce() -> Result<String, String>,
    RestoreState: FnOnce() -> Result<(), String>,
    RestoreSidecar: FnOnce() -> Result<String, String>,
    StoreToken: FnMut(String) -> Result<(), String>,
{
    if let Ok(token) = apply() {
        if store_token(token).is_ok() {
            return Ok(());
        }
    }
    if restore_state().is_err() {
        return Err(format!("{}失败，配置回滚未完成", action));
    }
    let rollback_token =
        restore_sidecar().map_err(|_| format!("{}失败，sidecar 回滚未完成", action))?;
    store_token(rollback_token).map_err(|_| format!("{}失败，回滚 Token 未更新", action))?;
    Err(format!("{}未生效，配置已回滚", action))
}

/// 原子替换运行时公共 API 短 Token。
/// 流程：取得单一 Token 锁后覆盖旧值；参数为运行状态和只存在于内存的新 Token；成功返回空值。
/// 异常/边界：锁中毒时返回稳定错误，不记录或回显 Token。
fn store_public_api_token(
    token_state: &RuntimePublicApiToken,
    token: String,
) -> Result<(), String> {
    *token_state
        .token
        .lock()
        .map_err(|_| "公共 API Token 状态不可用".to_string())? = token;
    Ok(())
}

/// 限制模型、配置和系统设置等敏感管理 IPC 只能由 hub 主窗口调用。
/// 流程：精确比较 Tauri 运行时注入的窗口标签；参数为不可由 IPC Body 伪造的 label；hub 返回成功。
/// 异常/边界：其它窗口、未知窗口和空标签均默认拒绝，固定错误不暴露配置或系统状态。
fn ensure_sensitive_management_window(window_label: &str) -> Result<(), String> {
    if window_label == "hub" {
        Ok(())
    } else {
        Err("当前窗口无权执行敏感管理操作（错误码：SENSITIVE_MANAGEMENT_FORBIDDEN）".to_string())
    }
}

/// 请求 Hub 主窗口确认是否允许创建 App 授权码。
/// 流程：私有 RPC worker 创建一次性 pending 通道，投递前端事件后等待用户确认、拒绝或超时。
/// 参数：app 为桌面端上下文，name/expires_at 为插件申请展示信息。
/// 返回：approved 表示用户确认，false 表示拒绝、超时或窗口不可用。
/// 异常/边界：同一时间只允许一个 pending，避免连续点击生成多条长期授权码。
pub(crate) fn request_access_token_approval_core(
    app: &AppHandle,
    request_id: String,
    name: String,
    expires_at: Option<String>,
) -> Result<AccessTokenApprovalResponse, String> {
    let event = AccessTokenApprovalEvent {
        request_id,
        name,
        expires_at,
    };
    let (sender, receiver) = mpsc::channel::<AccessTokenApprovalResponse>();
    {
        let approval_state = app.state::<RuntimeAccessTokenApproval>();
        let mut pending = approval_state
            .pending
            .lock()
            .map_err(|_| "授权确认状态不可用".to_string())?;
        if pending.is_some() {
            return Ok(AccessTokenApprovalResponse {
                approved: false,
                message: Some("已有一条授权申请正在等待确认。".to_string()),
            });
        }
        *pending = Some(PendingAccessTokenApproval {
            event: event.clone(),
            responder: sender,
        });
    }
    let emit_result = app.emit_to("hub", "public-api-access-token-requested", event.clone());
    if emit_result.is_err() {
        let _ = take_pending_access_token_approval(app, &event.request_id);
        return Ok(AccessTokenApprovalResponse {
            approved: false,
            message: Some("未找到可确认授权的主窗口。".to_string()),
        });
    }
    match receiver.recv_timeout(ACCESS_TOKEN_APPROVAL_TIMEOUT) {
        Ok(response) => Ok(response),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = take_pending_access_token_approval(app, &event.request_id);
            Ok(AccessTokenApprovalResponse {
                approved: false,
                message: Some("授权确认已超时。".to_string()),
            })
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Ok(AccessTokenApprovalResponse {
            approved: false,
            message: Some("授权确认通道已关闭。".to_string()),
        }),
    }
}

/// 取走指定 requestId 的 pending 授权申请。
/// 流程：在同一把锁内校验 requestId，匹配时移除并返回 pending；不匹配时保持原状态。
/// 参数：app 为桌面端上下文，request_id 为前端或超时路径传回的申请 ID。
/// 返回：匹配到的 pending；无匹配时返回 None。
/// 异常/边界：状态锁损坏时返回 None，调用方按拒绝或无效请求处理。
fn take_pending_access_token_approval(
    app: &AppHandle,
    request_id: &str,
) -> Option<PendingAccessTokenApproval> {
    let approval_state = app.state::<RuntimeAccessTokenApproval>();
    let mut pending = approval_state.pending.lock().ok()?;
    let should_take = pending
        .as_ref()
        .is_some_and(|approval| approval.event.request_id == request_id);
    if should_take {
        pending.take()
    } else {
        None
    }
}

/// 响应当前 App 授权码申请。
/// 流程：仅允许 Hub 主窗口调用，按 requestId 取走 pending 并把确认结果发送给等待中的 HTTP 请求。
/// 参数：window 用于校验调用窗口，request_id 为授权申请 ID，approved 为用户确认结论。
/// 返回：发送成功时无返回。
/// 异常/边界：未知或已超时的 requestId 返回稳定错误，不会创建授权码。
#[tauri::command]
fn respond_public_api_access_token_request(
    window: tauri::WebviewWindow,
    request_id: String,
    approved: bool,
) -> Result<(), String> {
    if window.label() != "hub" {
        return Err(
            "当前窗口无权确认 App 授权（错误码：ACCESS_TOKEN_APPROVAL_FORBIDDEN）".to_string(),
        );
    }
    let app = window.app_handle().clone();
    let Some(pending) = take_pending_access_token_approval(&app, &request_id) else {
        return Err("授权申请已失效，请重新发起。".to_string());
    };
    pending
        .responder
        .send(AccessTokenApprovalResponse {
            approved,
            message: if approved {
                None
            } else {
                Some("用户已拒绝授权。".to_string())
            },
        })
        .map_err(|_| "授权申请等待通道已关闭，请重新发起。".to_string())
}

/// 通过桌面端进程内批准方凭据批准浏览器设备码。
/// 流程：先限制为 hub 窗口，再把阻塞式本机 HTTP 批准放入后台线程；后台从 App 状态读取本次临时 Basic 凭据。
/// 参数：window 用于窗口权限和取得 AppHandle，user_code 为浏览器展示码；返回不含凭据的批准说明。
/// 异常/边界：WebView 无法读取 clientId/secret；无效、过期或重复批准透传稳定错误码和 requestId；后台任务异常返回诊断错误。
#[tauri::command]
async fn approve_public_api_device(
    window: tauri::WebviewWindow,
    user_code: String,
) -> Result<String, String> {
    if window.label() != "hub" {
        return Err("当前窗口无权批准设备码（错误码：DEVICE_APPROVAL_FORBIDDEN）".to_string());
    }
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<RuntimeSidecar>()
            .approve_device_authorization(&user_code)
    })
    .await
    .map_err(|error| format!("设备授权批准后台任务失败：{}", error))?
}

/// 读取任务管理工作区聚合数据的共享业务入口。
/// 流程：由私有 UDS RPC 调用唯一 TaskStore 聚合查询，失败时写入统一桌面日志。
/// 参数：app 用于定位同一份 SQLite 与日志，project_id 为可选当前项目；返回有限项目、任务和会话聚合。
/// 异常/边界：显式项目不存在时稳定报错，只有省略 ID 才选择首个项目；数据库错误不伪装为空数据。
pub(crate) fn load_session_workspace_data_core(
    app: &AppHandle,
    project_id: Option<String>,
) -> Result<WorkspaceDataResponse, String> {
    let diagnostic_context = project_id.clone();
    task_store::load_workspace_data(app, project_id).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_WORKSPACE_LOAD_FAILED",
            "load_session_workspace_data",
            diagnostic_context.as_deref(),
            &error,
        )
    })
}

/// 创建任务项目的共享业务入口。
/// 流程：由私有 UDS RPC 把请求交给唯一 TaskStore 完成目录校验和事务写入，失败时记录脱敏诊断。
/// 参数：app 定位同一份业务库，request 包含名称和工作空间；返回创建后工作区聚合。
/// 异常/边界：目录不可访问或名称冲突时整笔失败；同一工作目录允许绑定多个任务项目，不记录用户任务正文。
pub(crate) fn create_session_project_core(
    app: &AppHandle,
    request: CreateProjectRequest,
) -> Result<WorkspaceDataResponse, String> {
    task_store::create_project(app, request).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_PROJECT_CREATE_FAILED",
            "create_session_project",
            None,
            &error,
        )
    })
}

/// 更新任务项目的共享业务入口。
/// 流程：保留项目 ID 作为脱敏诊断上下文，再由唯一 TaskStore 事务更新名称和后续工作空间。
/// 参数：app 定位同一份业务库，request 为完整编辑表单；返回更新后工作区聚合。
/// 异常/边界：并发删除或重复名称时拒绝覆盖；工作目录允许被多个任务项目复用，已有会话路径快照保持不变。
pub(crate) fn update_session_project_core(
    app: &AppHandle,
    request: UpdateProjectRequest,
) -> Result<WorkspaceDataResponse, String> {
    let diagnostic_context = request.id.clone();
    task_store::update_project(app, request).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_PROJECT_UPDATE_FAILED",
            "update_session_project",
            Some(&diagnostic_context),
            &error,
        )
    })
}

/// 软删除任务项目的共享业务入口。
/// 流程：由唯一 TaskStore 标记项目已删除，失败时记录项目 ID 对应的脱敏诊断。
/// 参数：app 定位同一份业务库，project_id 为目标项目；返回删除后聚合。
/// 异常/边界：任务和会话历史不级联删除；未知或已删除项目返回稳定业务错误。
pub(crate) fn delete_session_project_core(
    app: &AppHandle,
    project_id: String,
) -> Result<WorkspaceDataResponse, String> {
    task_store::delete_project(app, &project_id).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_PROJECT_DELETE_FAILED",
            "delete_session_project",
            Some(&project_id),
            &error,
        )
    })
}

/// 创建任务的共享业务入口。
/// 流程：由唯一 TaskStore 原子写入 created 任务和创建事件，再返回事务生成 ID 与扁平工作区聚合；保留项目 ID 作为失败诊断上下文。
/// 参数：app 定位同一份业务库，request 为项目、标题和提示词；返回仅 createTask 使用的 createdTaskId + projects/tasks/sessions。
/// 异常/边界：输入校验错误原样返回以便 HTTP 映射 4xx；其它错误写脱敏日志，日志绝不包含 prompt。
pub(crate) fn create_session_task_core(
    app: &AppHandle,
    request: CreateTaskRequest,
) -> Result<CreateTaskResponse, String> {
    let diagnostic_context = request.project_id.clone();
    task_store::create_task(app, request).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_CREATE_FAILED",
            "create_session_task",
            Some(&diagnostic_context),
            &error,
        )
    })
}

/// 更新任务名称和描述的共享业务入口。
/// 流程：由唯一 TaskStore 校验任务状态并原子更新 title/prompt，失败时记录任务 ID 对应的脱敏诊断。
/// 参数：app 定位同一份业务库，request 为任务 ID 与新内容；返回更新后工作区聚合。
/// 异常/边界：仅 created 和 queued 可修改；其它已执行过状态拒绝，日志绝不包含 prompt。
pub(crate) fn update_session_task_core(
    app: &AppHandle,
    request: task_store::UpdateTaskRequest,
) -> Result<WorkspaceDataResponse, String> {
    let diagnostic_context = request.id.clone();
    task_store::update_task(app, request).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_UPDATE_FAILED",
            "update_session_task",
            Some(&diagnostic_context),
            &error,
        )
    })
}

/// 删除任务的共享业务入口。
/// 流程：由唯一 TaskStore 事务校验任务不在 running，再物理删除任务及其关联本地记录。
/// 参数：app 定位同一份业务库，task_id 为目标任务；返回删除后工作区聚合。
/// 异常/边界：running 任务拒绝删除，避免破坏调度器执行和重启对账。
pub(crate) fn delete_session_task_core(
    app: &AppHandle,
    task_id: String,
) -> Result<WorkspaceDataResponse, String> {
    task_store::delete_task(app, &task_id).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_DELETE_FAILED",
            "delete_session_task",
            Some(&task_id),
            &error,
        )
    })
}

/// 将任务排入唯一调度队列的共享业务入口。
/// 流程：使用 TaskStore CAS 把任务置为 queued，再读取同项目聚合；后台唯一调度器按既有顺序领取。
/// 参数：app 定位同一份业务库和调度生命周期，task_id 为目标任务；返回排队后聚合。
/// 异常/边界：不创建第二个调度器、不提前写 running；非法状态或并发冲突显式失败并记录脱敏诊断。
pub(crate) fn queue_session_task_core(
    app: &AppHandle,
    task_id: String,
) -> Result<WorkspaceDataResponse, String> {
    let result = (|| {
        task_store::ensure_task_queue_retry_allowed(app, &task_id)?;
        codex_desktop::with_execution_start_gate(app, || {
            let task = task_store::queue_task(app, &task_id)?;
            let project_id = task.project_id.clone();
            task_store::load_workspace_data(app, Some(project_id))
        })
    })();
    result.map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_QUEUE_FAILED",
            "queue_session_task",
            Some(&task_id),
            &error,
        )
    })
}

/// 读取 Codex Desktop 本机连接快照的共享业务入口。
/// 流程：调用唯一 Rust 探针生成公开字段，再用 TaskStore 活动状态收紧重启能力；参数为 AppHandle；返回可由私有 RPC 序列化的快照。
/// 异常/边界：探针或数据库未知状态显式失败；不返回 CLI 版本、端口、PID、WebSocket、DOM、路径、prompt 或登录态。
pub(crate) fn get_codex_connection_core(app: &AppHandle) -> Result<CodexConnectionStatus, String> {
    let mut status = codex_desktop::connection_status(&app.state::<RuntimeCodexDesktop>())
        .map_err(|error| {
            if error.contains("CODEX_CONNECTION_STATE_FAILED") {
                desktop_error::record_desktop_error(
                    app,
                    "CODEX_CONNECTION_STATE_FAILED",
                    "get_codex_connection",
                    None,
                    &error,
                )
            } else {
                error
            }
        })?;
    if status.can_restart && task_store::has_running_task(app)? {
        status.can_restart = false;
        status.reason_code = "CODEX_RESTART_TASK_ACTIVE".to_string();
        status.message = "存在执行中的任务，当前不能重启 Codex。".to_string();
    }
    Ok(status)
}

/// 接受用户确认后的 Codex Desktop 异步重启请求。
/// 流程：委托 RuntimeCodexDesktop 执行活动任务、监听者身份和单飞门禁，再于后台退出受信旧进程、确认端口释放并启动新实例；参数为 AppHandle；返回 accepted/state。
/// 异常/边界：不接收端口、路径、命令或 flags；用户明确请求时即使已连接也真正重启，但执行中任务、未知监听者和状态探测失败均在产生进程副作用前拒绝。
pub(crate) fn restart_codex_core(app: &AppHandle) -> Result<CodexRestartAccepted, String> {
    codex_desktop::begin_restart(app).map_err(|error| {
        if error.contains("CODEX_CONNECTION_STATE_FAILED") {
            desktop_error::record_desktop_error(
                app,
                "CODEX_CONNECTION_STATE_FAILED",
                "restart_codex",
                None,
                &error,
            )
        } else {
            error
        }
    })
}

/// 完成任务人工验收的共享业务入口。
/// 流程：由唯一 TaskStore 事务提交 completed 状态，随后广播真实数据库快照供桌面页面增量刷新。
/// 参数：app 定位同一份业务库并发事件，task_id 为待验收任务；返回完成后聚合。
/// 异常/边界：仅 waiting_acceptance 可完成；事务失败不发送成功事件，错误附带稳定诊断信息。
pub(crate) fn complete_session_task_core(
    app: &AppHandle,
    task_id: String,
) -> Result<WorkspaceDataResponse, String> {
    let data = task_store::complete_task(app, &task_id).map_err(|error| {
        record_task_ipc_error(
            app,
            "TASK_ACCEPTANCE_FAILED",
            "complete_session_task",
            Some(&task_id),
            &error,
        )
    })?;
    if let Some(task) = data.tasks.iter().find(|task| task.id == task_id) {
        emit_session_task_updated(app, &task.id, &task.project_id);
    }
    Ok(data)
}

/// 运行期间同步给托盘菜单的口述历史记录。
#[derive(Default)]
struct RuntimeDictationHistory {
    /// 最近的口述历史，菜单事件按下标读取，不持久化到 Rust 侧。
    items: Mutex<Vec<TrayHistoryItem>>,
}

/// 运行期间保存客户端 JSON 配置文件监听状态，避免重复启动轮询线程。
#[derive(Default)]
struct RuntimeLocalConfigWatcher {
    /// 是否已经启动本机配置文件监听。
    started: Mutex<bool>,
}

/// 客户端 JSON 配置文件结构，按前端 StorageKey 分区保存可持久化配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalConfigDocument {
    /// 首发配置格式标识；读取到未知版本时显式拒绝，不执行历史兼容迁移。
    version: u32,
    /// 最近一次客户端写入时间；外部手动编辑文件时保留原值。
    updated_at: String,
    /// 各模块配置分区，key 来自前端 StorageKey。
    items: HashMap<String, Value>,
}

impl Default for LocalConfigDocument {
    fn default() -> Self {
        Self {
            version: LOCAL_CONFIG_VERSION,
            updated_at: String::new(),
            items: HashMap::new(),
        }
    }
}

/// 读取客户端 JSON 配置文件中的单个分区。
/// 流程：校验 hub 和分区键后读取完整文档并复制目标值；参数为调用窗口和稳定分区键；返回可选 JSON 值。
/// 异常/边界：非 hub、非法键、文件损坏或读取失败均显式返回，不用空值掩盖错误。
#[tauri::command]
fn read_local_config_value(
    window: tauri::WebviewWindow,
    key: String,
) -> Result<Option<Value>, String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    validate_local_config_key(&key)?;
    let document = read_local_config_document(&app)?;
    Ok(document.items.get(&key).cloned())
}

/// 写入客户端 JSON 配置文件中的单个分区，并通知所有 WebView 刷新。
/// 流程：校验 hub 和分区键，原子更新文档后广播快照；参数为窗口、稳定键和 JSON 值；成功返回空值。
/// 异常/边界：非 hub 或原子写入失败时不广播成功事件，避免其它窗口读取半完成状态。
#[tauri::command]
fn write_local_config_value(
    window: tauri::WebviewWindow,
    key: String,
    value: Value,
) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    validate_local_config_key(&key)?;
    let mut document = read_local_config_document(&app)?;
    document.version = LOCAL_CONFIG_VERSION;
    document.updated_at = local_config_updated_at();
    document.items.insert(key, value);
    write_local_config_document(&app, &document)?;
    emit_local_config_changed(&app, &document);
    Ok(())
}

/// 删除客户端 JSON 配置文件中的单个分区，并通知所有 WebView 刷新。
/// 流程：校验 hub 和分区键，原子删除目标项后广播快照；参数为窗口和稳定键；成功返回空值。
/// 异常/边界：目标键不存在时保持幂等，非 hub 或写入失败时不发送变更事件。
#[tauri::command]
fn remove_local_config_value(window: tauri::WebviewWindow, key: String) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    validate_local_config_key(&key)?;
    let mut document = read_local_config_document(&app)?;
    document.updated_at = local_config_updated_at();
    document.items.remove(&key);
    write_local_config_document(&app, &document)?;
    emit_local_config_changed(&app, &document);
    Ok(())
}

/// 读取客户端 JSON 配置文件的完整快照，供前端启动时诊断或主动刷新。
/// 流程：校验 hub 后读取并解析完整文档；参数为调用窗口；返回带版本和更新时间的配置快照。
/// 异常/边界：非 hub 或文件损坏时显式失败，禁止向临时窗口暴露本地配置。
#[tauri::command]
fn read_local_config_snapshot(window: tauri::WebviewWindow) -> Result<LocalConfigDocument, String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    read_local_config_document(&app)
}

/// 启动客户端 JSON 配置文件变化监听；内部通过轻量轮询捕捉外部改文件场景。
/// 流程：校验 hub 并以运行时锁保证只启动一次，再由后台线程监测修改时间并广播；参数为窗口和监听状态。
/// 异常/边界：重复调用幂等成功，非 hub 默认拒绝，单次文件读取失败由后续轮询自行恢复。
#[tauri::command]
fn start_local_config_watch(
    window: tauri::WebviewWindow,
    watcher: State<'_, RuntimeLocalConfigWatcher>,
) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    {
        let mut started = watcher
            .started
            .lock()
            .map_err(|_| "启动配置监听失败：状态锁已损坏".to_string())?;
        if *started {
            return Ok(());
        }
        *started = true;
    }
    let watch_app = app.clone();
    thread::spawn(move || {
        let mut last_modified = local_config_modified_millis(&watch_app).unwrap_or(0);
        loop {
            thread::sleep(Duration::from_millis(LOCAL_CONFIG_WATCH_INTERVAL_MS));
            let current_modified = local_config_modified_millis(&watch_app).unwrap_or(0);
            if current_modified == last_modified {
                continue;
            }
            last_modified = current_modified;
            if let Ok(document) = read_local_config_document(&watch_app) {
                emit_local_config_changed(&watch_app, &document);
            }
        }
    });
    Ok(())
}

/// 校验客户端配置 key，避免 Web 端传入空 key 或路径类字符串污染 JSON 结构。
fn validate_local_config_key(key: &str) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("配置 key 不能为空".to_string());
    }
    if !trimmed.starts_with("codexman.") {
        return Err("配置 key 不在允许的 CodexMan 命名空间内".to_string());
    }
    Ok(())
}

/// 读取客户端 JSON 配置文件；文件不存在时返回空默认结构。
fn read_local_config_document(app: &AppHandle) -> Result<LocalConfigDocument, String> {
    let path = local_config_file_path(app)?;
    if !path.exists() {
        return Ok(LocalConfigDocument::default());
    }
    let content =
        fs::read_to_string(&path).map_err(|error| format!("读取本地配置文件失败：{}", error))?;
    if content.trim().is_empty() {
        return Ok(LocalConfigDocument::default());
    }
    parse_local_config_document(&content)
}

/// 解析并校验首发客户端配置文档。
/// 流程：先反序列化固定结构，再校验版本必须等于首发版本；参数为 JSON 正文；返回可信配置文档。
/// 异常/边界：JSON 损坏或未知版本均显式失败，不尝试迁移、降级或用默认值覆盖问题现场。
fn parse_local_config_document(content: &str) -> Result<LocalConfigDocument, String> {
    let document = serde_json::from_str::<LocalConfigDocument>(content)
        .map_err(|error| format!("解析本地配置文件失败：{}", error))?;
    if document.version != LOCAL_CONFIG_VERSION {
        return Err(format!(
            "本地配置版本不受支持：期望 {}，实际 {}",
            LOCAL_CONFIG_VERSION, document.version
        ));
    }
    Ok(document)
}

/// 写入客户端 JSON 配置文件；目录不存在时自动创建。
fn write_local_config_document(
    app: &AppHandle,
    document: &LocalConfigDocument,
) -> Result<(), String> {
    let path = local_config_file_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建本地配置目录失败：{}", error))?;
    }
    let content = serde_json::to_string_pretty(document)
        .map_err(|error| format!("序列化本地配置失败：{}", error))?;
    fs::write(&path, content).map_err(|error| format!("写入本地配置文件失败：{}", error))
}

/// 读取客户端 JSON 配置文件路径，集中约束配置只能落在应用数据目录下。
fn local_config_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(LOCAL_CONFIG_FILE_NAME))
        .map_err(|error| format!("读取应用数据目录失败：{}", error))
}

/// 读取客户端 JSON 配置文件修改时间，用于轮询监听外部编辑。
fn local_config_modified_millis(app: &AppHandle) -> Result<u128, String> {
    let path = local_config_file_path(app)?;
    if !path.exists() {
        return Ok(0);
    }
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("读取本地配置修改时间失败：{}", error))?;
    system_time_to_millis(modified)
}

/// 生成客户端配置更新时间戳字符串，前端只用于判断快照新旧和排查。
fn local_config_updated_at() -> String {
    system_time_to_millis(SystemTime::now())
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "0".to_string())
}

/// 将系统时间转换为 Unix 毫秒时间戳。
fn system_time_to_millis(value: SystemTime) -> Result<u128, String> {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("转换系统时间失败：{}", error))
}

/// 向所有 WebView 广播客户端 JSON 配置文件快照。
fn emit_local_config_changed(app: &AppHandle, document: &LocalConfigDocument) {
    let _ = app.emit("local-config-changed", document.clone());
}

/// 前端同步给原生托盘菜单的历史记录摘要。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrayHistoryItem {
    /// 历史记录 ID，用于去重和调试。
    id: String,
    /// 托盘菜单展示的短标题。
    title: String,
    /// 点击菜单后复制到系统剪贴板的完整文本。
    text: String,
}

/// 前端提交的全局模式快捷键。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct ShortcutProfile {
    /// ASR 仅转文本模式快捷键。
    asr: String,
    /// 听写模式快捷键。
    dictate: String,
    /// 文本润色模式快捷键。
    polish: String,
    /// 用户创建的打开应用快捷键绑定。
    app_bindings: Vec<AppShortcutBinding>,
}

/// 用户创建的打开应用快捷键绑定。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppShortcutBinding {
    /// 绑定唯一 ID，用于前端渲染和后续删除。
    id: String,
    /// 触发打开应用动作的全局快捷键。
    shortcut: String,
    /// 动作类型，当前只允许 openApp。
    action_type: String,
    /// 目标应用展示名称。
    app_name: String,
    /// 目标应用 bundle 绝对路径。
    app_path: String,
    /// 创建时间 ISO 字符串。
    created_at: String,
}

impl Default for ShortcutProfile {
    fn default() -> Self {
        Self {
            asr: DEFAULT_ASR_TEXT_SHORTCUT.to_string(),
            dictate: DEFAULT_DICTATE_SHORTCUT.to_string(),
            polish: DEFAULT_POLISH_SHORTCUT.to_string(),
            app_bindings: Vec::new(),
        }
    }
}

/// 自动粘贴命令的执行结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PasteResponse {
    /// 是否已成功发出系统粘贴指令；该字段不代表目标输入框已插入文字。
    command_sent: bool,
    /// 是否通过辅助功能读取确认目标输入框已插入本次文字。
    insertion_verified: bool,
    /// 给前端展示的执行说明。
    message: String,
    /// 是否需要用户授予辅助功能权限。
    requires_accessibility: bool,
    /// 本次尝试粘贴前恢复的目标应用。
    target_app: String,
    /// 是否已成功写入系统剪贴板。
    clipboard_written: bool,
    /// 剪贴板读回内容是否与本次输出一致。
    clipboard_matches_expected: bool,
    /// 是否尝试恢复用户原本的系统剪贴板。
    clipboard_restore_attempted: bool,
    /// 用户原本的系统剪贴板是否已恢复。
    clipboard_restored: bool,
    /// 剪贴板恢复状态说明，不包含剪贴板正文。
    clipboard_restore_message: String,
    /// 触发粘贴前检测到的辅助功能授权状态。
    accessibility_trusted: bool,
    /// 本次粘贴指令实际使用的触发方式。
    paste_method: String,
    /// 隐藏 CodexMan 窗口前的系统前台应用。
    frontmost_before_paste: String,
    /// 尝试激活目标应用后的系统前台应用。
    frontmost_after_activate: String,
    /// 发送粘贴指令后的系统前台应用。
    frontmost_after_paste: String,
    /// 粘贴校验的说明，不包含目标输入框正文。
    verification_status: String,
    /// 发送粘贴指令前目标 App 内的系统焦点元素。
    focused_element_before_paste: String,
    /// 激活目标 App 后的系统焦点元素。
    focused_element_after_activate: String,
    /// 发送粘贴指令后的系统焦点元素。
    focused_element_after_paste: String,
}

/// 读取系统当前选中文本后的结果，供文本润色模式使用。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectedTextResponse {
    /// 通过系统复制快捷键读到的选中文本。
    text: String,
    /// 触发读取前的前台 App 名称，用于后续粘贴回原 App。
    target_app: String,
    /// 是否检测到辅助功能授权。
    accessibility_trusted: bool,
    /// 读取完成后是否恢复了用户原剪贴板。
    clipboard_restored: bool,
    /// 剪贴板恢复状态说明，不包含剪贴板正文。
    clipboard_restore_message: String,
    /// 本次读取使用的系统触发方式。
    copy_method: String,
}

/// Codex 会话索引中的单条会话摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexThreadSummary {
    /// Codex 会话 ID。
    id: String,
    /// Codex 会话标题。
    title: String,
    /// 父级 Codex 会话 ID；普通用户会话为空，子 Agent 会话来自 source.subagent.thread_spawn.parent_thread_id。
    parent_thread_id: String,
    /// 子任务深度；普通会话为 0，子 Agent 会话使用 CodeX 记录的 depth。
    depth: i64,
    /// 子 Agent 昵称；普通会话为空。
    agent_nickname: String,
    /// 子 Agent 角色；普通会话为空。
    agent_role: String,
    /// 最近更新时间，保持 ISO 字符串供前端本地化展示。
    updated_at: String,
}

/// Codex 已有任务归属的工作空间摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexWorkspaceSummary {
    /// 工作空间绝对路径，对应 Codex Thread.cwd。
    cwd: String,
    /// 下拉框展示名称，优先使用目录名。
    title: String,
    /// 最近任务数量，用于辅助判断工作空间活跃度。
    thread_count: usize,
    /// 最近更新时间，保持字符串交给前端格式化。
    updated_at: String,
}

/// Codex 会话详情中的可展示消息。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexThreadMessage {
    /// 消息角色，MVP 只返回 user 和 assistant。
    role: String,
    /// 消息正文，已经做最大长度保护。
    content: String,
    /// 消息创建时间，保持 ISO 字符串供前端本地化展示。
    created_at: String,
}

/// Codex 本地会话详情。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexThreadDetail {
    /// Codex 会话 ID。
    id: String,
    /// Codex 会话标题。
    title: String,
    /// 最近更新时间，保持 ISO 字符串供前端本地化展示。
    updated_at: String,
    /// 从 JSONL 中抽取出的最近用户和助手消息。
    messages: Vec<CodexThreadMessage>,
}

/// 前端读取 CodeX 会话列表时提交的分页筛选参数。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexThreadListRequest {
    /// CodeX 工作空间绝对路径。
    pub(crate) workspace_cwd: String,
    /// 本次读取的最大会话数量。
    pub(crate) limit: i64,
    /// 跳过的会话数量，用于加载更多分页。
    pub(crate) offset: i64,
    /// 搜索关键词，可匹配标题、预览或 thread ID。
    pub(crate) keyword: String,
}

/// 系统级粘贴触发结果，记录具体路径以便前端诊断。
#[derive(Debug)]
struct PasteTriggerResult {
    /// 触发粘贴时辅助功能权限是否可信。
    accessibility_ready: bool,
    /// 实际使用的系统粘贴触发方式。
    method: String,
}

/// 系统剪贴板中单个数据类型的二进制快照。
#[derive(Debug, Clone)]
struct ClipboardRepresentationSnapshot {
    /// macOS Pasteboard 数据类型名称。
    type_name: String,
    /// 对应数据类型的原始二进制内容。
    data: Vec<u8>,
}

/// 系统剪贴板中单个条目的快照，保留文件、图片、富文本等多类型数据。
#[derive(Debug, Clone)]
struct ClipboardItemSnapshot {
    /// 当前条目下可恢复的数据表示列表。
    representations: Vec<ClipboardRepresentationSnapshot>,
}

/// 系统剪贴板完整快照，用于自动粘贴后恢复用户原本剪切或复制的内容。
#[derive(Debug, Clone)]
struct ClipboardSnapshot {
    /// 剪贴板条目列表；为空代表原剪贴板为空。
    items: Vec<ClipboardItemSnapshot>,
}

/// 自动粘贴结束后的原剪贴板恢复状态。
#[derive(Debug, Clone)]
struct ClipboardRestoreStatus {
    /// 是否执行过恢复动作。
    attempted: bool,
    /// 恢复动作是否成功。
    restored: bool,
    /// 面向诊断日志的恢复说明。
    message: String,
}

/// 返回给前端的桌面运行状态诊断。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeDiagnostics {
    /// macOS 辅助功能权限是否已授权。
    accessibility_trusted: bool,
    /// 当前 Rust 侧实际保存的快捷键配置。
    shortcuts: ShortcutProfile,
    /// 当前全局快捷键是否已成功注册。
    shortcut_registration_ready: bool,
    /// 最近一次全局快捷键注册结果说明。
    shortcut_registration_message: String,
}

/// 顶部错误气泡的前端展示内容。
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ErrorBubblePayload {
    /// 错误原因文本。
    message: String,
}

/// 粘贴失败或没有输入焦点时展示的结果窗口内容。
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ResultWindowPayload {
    /// 本次识别和处理后的最终文字。
    text: String,
    /// 展示给用户的兜底原因。
    reason: String,
    /// 是否建议用户打开辅助功能设置。
    requires_accessibility: bool,
}

/// Tauri 入口，注册桌面端命令和插件。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeShortcuts::default())
        .manage(RuntimeResult::default())
        .manage(RuntimePublicApiToken::default())
        .manage(RuntimeAccessTokenApproval::default())
        .manage(RuntimeDictationHistory::default())
        .manage(RuntimePasteFocusSnapshot::default())
        .manage(RuntimeLocalConfigWatcher::default())
        .manage(RuntimePrivateRpc::default())
        .manage(RuntimeSidecar::default())
        .manage(RuntimeWebServer::default())
        .manage(RuntimeCodexDesktop::default())
        .setup(|app| {
            let catalog = private_models::sidecar_catalog_json(app.handle()).map_err(|error| {
                std::io::Error::other(desktop_error::record_desktop_error(
                    app.handle(),
                    "MODEL_CATALOG_LOAD_FAILED",
                    "app_setup",
                    None,
                    &error,
                ))
            })?;
            app.state::<RuntimePrivateRpc>()
                .start(app.handle())
                .map_err(|error| {
                    std::io::Error::other(desktop_error::record_desktop_error(
                        app.handle(),
                        "PRIVATE_RPC_START_FAILED",
                        "app_setup",
                        None,
                        &error,
                    ))
                })?;
            let token = match app.state::<RuntimeSidecar>().start(app.handle(), &catalog) {
                Ok(token) => token,
                Err(error) => {
                    let _ = app.state::<RuntimePrivateRpc>().shutdown();
                    return Err(std::io::Error::other(desktop_error::record_desktop_error(
                        app.handle(),
                        "SIDECAR_START_FAILED",
                        "app_setup",
                        None,
                        &error,
                    ))
                    .into());
                }
            };
            let public_api_token_state = app.state::<RuntimePublicApiToken>();
            let mut public_api_token = match public_api_token_state.token.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    let _ = app.state::<RuntimeSidecar>().shutdown();
                    let _ = app.state::<RuntimePrivateRpc>().shutdown();
                    return Err(std::io::Error::other(desktop_error::record_desktop_error(
                        app.handle(),
                        "SIDECAR_TOKEN_STORE_FAILED",
                        "app_setup",
                        None,
                        "保存 sidecar 短 Token 失败：状态锁已损坏",
                    ))
                    .into());
                }
            };
            *public_api_token = token;
            drop(public_api_token);
            if let Err(error) = app.state::<RuntimeWebServer>().start(app.handle()) {
                let _ = app.state::<RuntimeSidecar>().shutdown();
                let _ = app.state::<RuntimePrivateRpc>().shutdown();
                return Err(std::io::Error::other(desktop_error::record_desktop_error(
                    app.handle(),
                    "WEB_SERVER_START_FAILED",
                    "app_setup",
                    None,
                    &error,
                ))
                .into());
            }
            let dispatcher_app = app.handle().clone();
            thread::spawn(move || run_codex_task_dispatcher(&dispatcher_app));
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::ShortcutState;

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(|app, shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                trigger_voice_shortcut(app.clone(), shortcut.to_string());
                            }
                        })
                        .build(),
                )?;
                let default_profile = ShortcutProfile::default();
                let shortcut_result = register_shortcut_profile(app.handle(), &default_profile);
                let shortcut_state = app.state::<RuntimeShortcuts>();
                let _ =
                    set_shortcut_runtime(&shortcut_state, default_profile, shortcut_result.err());
                configure_tray(app)?;
            }

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if window.label() == "hub" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            read_selected_text,
            paste_text,
            show_main_window,
            hide_main_window,
            show_hub_window,
            hide_hub_window,
            show_error_bubble,
            hide_toast_window,
            show_result_window,
            hide_result_window,
            get_last_result_window_payload,
            download_browser_extension_zip,
            respond_public_api_access_token_request,
            set_public_api_token,
            get_public_api_token,
            refresh_public_api_token_if_matches,
            clear_public_api_token_if_matches,
            register_shortcuts,
            suspend_shortcuts_for_recording,
            list_installed_applications,
            get_runtime_diagnostics,
            open_accessibility_settings,
            open_microphone_settings,
            set_login_launch,
            get_login_launch,
            set_dock_visible,
            get_frontmost_app,
            set_system_output_muted,
            play_native_interaction_sound,
            sync_tray_dictation_history,
            read_local_config_value,
            write_local_config_value,
            remove_local_config_value,
            read_local_config_snapshot,
            start_local_config_watch,
            list_private_models,
            save_private_model,
            delete_private_model,
            test_private_model,
            approve_public_api_device
        ])
        .build(tauri::generate_context!())
        .expect("启动 CodexMan 失败")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                let _ = present_window(app, "hub", false);
            }
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                if let Err(error) = app.state::<RuntimeSidecar>().shutdown() {
                    let _ = desktop_error::record_desktop_error(
                        app,
                        "SIDECAR_SHUTDOWN_FAILED",
                        "app_exit",
                        None,
                        &error,
                    );
                }
                if let Err(error) = app.state::<RuntimePrivateRpc>().shutdown() {
                    let _ = desktop_error::record_desktop_error(
                        app,
                        "PRIVATE_RPC_SHUTDOWN_FAILED",
                        "app_exit",
                        None,
                        &error,
                    );
                }
                if let Err(error) = app.state::<RuntimeWebServer>().shutdown() {
                    let _ = desktop_error::record_desktop_error(
                        app,
                        "WEB_SERVER_SHUTDOWN_FAILED",
                        "app_exit",
                        None,
                        &error,
                    );
                }
                if let Ok(mut token) = app.state::<RuntimePublicApiToken>().token.lock() {
                    token.clear();
                }
            }
        });
}

/// 创建系统托盘菜单，并把常用动作接到桌面端命令。
#[cfg(desktop)]
fn configure_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::image::Image;
    use tauri::tray::TrayIconBuilder;

    let menu = build_tray_menu(app, &app.package_info().version.to_string(), &[])?;
    let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(false)
        .tooltip("CodexMan")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_voice_polish" => present_hub_view(app, "voicePolish"),
            "open_text_polish" => present_hub_view(app, "textPolish"),
            "open_settings" => present_hub_view(app, "settings"),
            "microphone_default" | "microphone_settings" => present_hub_view(app, "permission"),
            "microphone_refresh" => {
                present_hub_view(app, "permission");
                emit_hub_event(app.clone(), "hub-refresh-microphones", String::new());
            }
            "check_updates" => show_update_status(app),
            "quit" => app.exit(0),
            id if id.starts_with("copy_dictation_history_") => copy_tray_dictation_history(app, id),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// 构造系统托盘菜单；历史子菜单由前端同步的最近口述记录动态生成。
#[cfg(desktop)]
fn build_tray_menu<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
    version: &str,
    history_items: &[TrayHistoryItem],
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

    let menu = Menu::new(manager)?;
    let open_voice_polish = MenuItem::with_id(
        manager,
        "open_voice_polish",
        "语音转文字润色",
        true,
        None::<&str>,
    )?;
    let open_text_polish =
        MenuItem::with_id(manager, "open_text_polish", "润色", true, None::<&str>)?;
    let dictation_history_menu =
        Submenu::with_id(manager, "dictation_history_menu", "口述历史记录", true)?;
    if history_items.is_empty() {
        let empty_history = MenuItem::with_id(
            manager,
            "dictation_history_empty",
            "暂无口述历史记录",
            false,
            None::<&str>,
        )?;
        dictation_history_menu.append(&empty_history)?;
    } else {
        for (index, item) in history_items.iter().take(8).enumerate() {
            let label = format_tray_history_label(item);
            let menu_item = MenuItem::with_id(
                manager,
                format!("copy_dictation_history_{}", index),
                label,
                true,
                None::<&str>,
            )?;
            dictation_history_menu.append(&menu_item)?;
        }
    }
    let open_settings =
        MenuItem::with_id(manager, "open_settings", "设置...", true, Some("Cmd+,"))?;
    let microphone_default = MenuItem::with_id(
        manager,
        "microphone_default",
        "系统默认麦克风",
        true,
        None::<&str>,
    )?;
    let microphone_settings = MenuItem::with_id(
        manager,
        "microphone_settings",
        "打开麦克风设置",
        true,
        None::<&str>,
    )?;
    let microphone_refresh = MenuItem::with_id(
        manager,
        "microphone_refresh",
        "刷新麦克风列表",
        true,
        None::<&str>,
    )?;
    let microphone_separator = PredefinedMenuItem::separator(manager)?;
    let microphone_menu = Submenu::with_id_and_items(
        manager,
        "microphone_menu",
        "选择麦克风",
        true,
        &[
            &microphone_default,
            &microphone_separator,
            &microphone_settings,
            &microphone_refresh,
        ],
    )?;
    let version_item = MenuItem::with_id(
        manager,
        "version",
        format!("版本 {}", version),
        false,
        None::<&str>,
    )?;
    let check_updates =
        MenuItem::with_id(manager, "check_updates", "检查更新...", true, None::<&str>)?;
    let quit = MenuItem::with_id(manager, "quit", "退出 CodexMan", true, Some("Cmd+Q"))?;
    let first_separator = PredefinedMenuItem::separator(manager)?;
    let second_separator = PredefinedMenuItem::separator(manager)?;
    menu.append(&open_voice_polish)?;
    menu.append(&open_text_polish)?;
    menu.append(&dictation_history_menu)?;
    menu.append(&first_separator)?;
    menu.append(&open_settings)?;
    menu.append(&microphone_menu)?;
    menu.append(&second_separator)?;
    menu.append(&version_item)?;
    menu.append(&check_updates)?;
    menu.append(&quit)?;
    Ok(menu)
}

/// 打开 Hub 并切到指定页面，供托盘菜单复用。
#[cfg(desktop)]
fn present_hub_view(app: &AppHandle, view: &str) {
    let _ = present_window(app, "hub", false);
    emit_hub_event(app.clone(), "hub-switch-view", view.to_string());
}

/// 延迟向 Hub 发送事件，避免窗口刚显示时前端监听还没恢复。
#[cfg(desktop)]
fn emit_hub_event(app: AppHandle, event: &'static str, payload: String) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        let _ = app.emit_to("hub", event, payload);
    });
}

/// 根据当前前台 App 计算快捷键录音行为；不主动恢复目标 App，避免录音流程切换焦点。
fn resolve_voice_trigger_context(frontmost_app: &str) -> VoiceTriggerContext {
    let normalized_frontmost_app = normalize_target_app_name(frontmost_app);
    if normalized_frontmost_app.is_empty() {
        return VoiceTriggerContext {
            target_app: String::new(),
            show_floating_window: false,
            keep_hub_visible: true,
        };
    }
    VoiceTriggerContext {
        target_app: normalized_frontmost_app,
        show_floating_window: true,
        keep_hub_visible: false,
    }
}

/// 根据当前前台 App 计算粘贴行为；有请求目标时必须与当前前台 App 一致，避免处理期间焦点漂移后误粘。
fn resolve_paste_target(requested_target_app: &str, frontmost_app: &str) -> PasteTargetDecision {
    let normalized_requested_app = normalize_target_app_name(requested_target_app);
    let normalized_frontmost_app = normalize_target_app_name(frontmost_app);
    if !normalized_requested_app.is_empty() && normalized_requested_app != normalized_frontmost_app
    {
        return PasteTargetDecision {
            target_app: String::new(),
            should_hide_hub: false,
        };
    }
    if !normalized_frontmost_app.is_empty() {
        return PasteTargetDecision {
            target_app: normalized_frontmost_app,
            should_hide_hub: false,
        };
    }
    PasteTargetDecision {
        target_app: String::new(),
        should_hide_hub: false,
    }
}

/// 只有口述开始时记录了明确外部 App，才在粘贴前主动恢复该 App 的焦点。
/// 这样可以修正 CodexMan 悬浮窗在 ASR/AI 等待期间让 Web 输入框短暂失焦的问题，
/// 同时不放宽后续“必须存在可输入焦点”的粘贴门禁。
fn should_refocus_requested_paste_target(
    requested_target_app: &str,
    resolved_target_app: &str,
) -> bool {
    let normalized_requested_app = normalize_target_app_name(requested_target_app);
    !normalized_requested_app.is_empty() && normalized_requested_app == resolved_target_app
}

/// 判断是否允许在显式目标 App 未变化时绕过 AX 文本控件误判。
/// ChatGPT、浏览器和部分 Electron WebView 的 DOM 焦点不会稳定映射为 AXTextArea，
/// 此时只要目标 App 与录音开始时一致，就应继续发送系统粘贴，而不是误弹结果窗口。
fn should_trust_explicit_paste_target(
    requested_target_app: &str,
    resolved_target_app: &str,
) -> bool {
    should_refocus_requested_paste_target(requested_target_app, resolved_target_app)
}

/// 展示当前版本的更新状态；在线更新通道接入前不给用户虚假的升级入口。
#[cfg(desktop)]
fn show_update_status(app: &AppHandle) {
    present_hub_view(app, "settings");
    emit_hub_notice(
        app,
        &format!(
            "当前版本为 {}，暂无在线更新通道。",
            app.package_info().version
        ),
        "idle",
    );
}

/// 向 Hub 展示托盘菜单触发的状态反馈。
#[cfg(desktop)]
fn emit_hub_notice(app: &AppHandle, message: &str, state: &str) {
    let payload = json!({
        "message": message,
        "state": state
    });
    let app_handle = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(180));
        let _ = app_handle.emit_to("hub", "hub-show-notice", payload);
    });
}

/// 同步最近口述历史到原生托盘菜单，让托盘子菜单可以直接复制历史输出。
#[tauri::command]
fn sync_tray_dictation_history(
    app: tauri::AppHandle,
    state: State<'_, RuntimeDictationHistory>,
    items: Vec<TrayHistoryItem>,
) -> Result<(), String> {
    let normalized_items = items
        .into_iter()
        .filter_map(normalize_tray_history_item)
        .take(8)
        .collect::<Vec<_>>();
    {
        let mut history_items = state
            .items
            .lock()
            .map_err(|_| "同步托盘口述历史失败：状态锁已损坏".to_string())?;
        *history_items = normalized_items.clone();
    }
    refresh_tray_menu(&app, &normalized_items)
}

/// 清理托盘历史条目，避免空文本或超长标题影响原生菜单。
fn normalize_tray_history_item(item: TrayHistoryItem) -> Option<TrayHistoryItem> {
    let text = item.text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let title = item.title.trim();
    Some(TrayHistoryItem {
        id: item.id.trim().to_string(),
        title: if title.is_empty() {
            make_tray_history_title(&text, 32)
        } else {
            make_tray_history_title(title, 32)
        },
        text,
    })
}

/// 刷新系统托盘菜单；失败时只返回错误，不影响前端历史保存。
#[cfg(desktop)]
fn refresh_tray_menu(app: &AppHandle, history_items: &[TrayHistoryItem]) -> Result<(), String> {
    let tray = app
        .tray_by_id("main")
        .ok_or_else(|| "未找到系统托盘图标".to_string())?;
    let menu = build_tray_menu(app, &app.package_info().version.to_string(), history_items)
        .map_err(|error| format!("刷新托盘菜单失败：{}", error))?;
    tray.set_menu(Some(menu))
        .map_err(|error| format!("设置托盘菜单失败：{}", error))
}

/// 非桌面平台不刷新托盘菜单。
#[cfg(not(desktop))]
fn refresh_tray_menu(_app: &AppHandle, _history_items: &[TrayHistoryItem]) -> Result<(), String> {
    Ok(())
}

/// 点击托盘口述历史子菜单后复制完整内容，并展示顶部提示。
#[cfg(desktop)]
fn copy_tray_dictation_history(app: &AppHandle, menu_id: &str) {
    let index = menu_id
        .trim_start_matches("copy_dictation_history_")
        .parse::<usize>()
        .ok();
    let Some(index) = index else {
        let _ = show_error_bubble(app.clone(), "复制历史记录失败：菜单项无效".to_string());
        return;
    };
    let history_state = app.state::<RuntimeDictationHistory>();
    let item = history_state
        .items
        .lock()
        .ok()
        .and_then(|items| items.get(index).cloned());
    let Some(item) = item else {
        let _ = show_error_bubble(app.clone(), "复制历史记录失败：记录已失效".to_string());
        return;
    };
    match write_clipboard_text_verified(&item.text) {
        Ok(true) => {
            let _ = show_error_bubble(app.clone(), "已复制到剪切板".to_string());
        }
        Ok(false) => {
            let _ = show_error_bubble(
                app.clone(),
                "复制历史记录失败：剪贴板读回不一致".to_string(),
            );
        }
        Err(error) => {
            let _ = show_error_bubble(
                app.clone(),
                format!("复制历史记录失败：{}", trim_error_message(&error)),
            );
        }
    }
}

/// 生成托盘历史菜单展示文案，避免长文本撑开系统菜单。
fn format_tray_history_label(item: &TrayHistoryItem) -> String {
    make_tray_history_title(&item.title, 32)
}

/// 按字符数截断托盘菜单文案，保留完整文本用于点击复制。
fn make_tray_history_title(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = normalized.chars().take(max_chars).collect::<String>();
    if normalized.chars().count() > max_chars {
        title.push('…');
    }
    title
}

/// 判断指定窗口当前是否可见；读取失败时按不可见处理，避免阻断快捷键主链路。
fn is_window_visible(app: &tauri::AppHandle, label: &str) -> bool {
    app.get_webview_window(label)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// 根据全局快捷键字符串判断目标模式，并通知悬浮窗开始或停止。
fn trigger_voice_shortcut(app: tauri::AppHandle, shortcut: String) {
    if trigger_app_shortcut(app.clone(), &shortcut) {
        return;
    }
    let mode = shortcut_to_mode(&app, &shortcut);
    trigger_voice_mode(app, &mode);
}

/// 触发用户创建的打开应用快捷键。
/// 流程：按规范化快捷键查找自定义绑定，命中后通过系统 open 命令打开目标 App。
/// 参数：app 为 Tauri AppHandle，shortcut 为插件回调传入的快捷键字符串。
/// 返回：命中自定义绑定并已尝试打开时返回 true。
/// 异常/边界：打开失败只记录日志，不继续落入语音模式，避免同一个快捷键触发双动作。
fn trigger_app_shortcut(app: tauri::AppHandle, shortcut: &str) -> bool {
    let normalized = normalize_shortcut(shortcut);
    let binding = app
        .state::<RuntimeShortcuts>()
        .profile
        .lock()
        .ok()
        .and_then(|profile| {
            profile
                .app_bindings
                .iter()
                .find(|binding| normalize_shortcut(&binding.shortcut) == normalized)
                .cloned()
        });
    if let Some(binding) = binding {
        if let Err(error) = open_application_bundle(&binding.app_path) {
            eprintln!(
                "打开快捷键绑定应用失败：{}，目标：{}",
                trim_error_message(&error),
                binding.app_name
            );
        }
        return true;
    }
    false
}

/// 按指定模式通知悬浮录音条开始或停止。
fn trigger_voice_mode(app: tauri::AppHandle, mode: &str) {
    if mode.trim().is_empty() {
        return;
    }
    let frontmost_app = get_frontmost_app().unwrap_or_default();
    let context = resolve_voice_trigger_context(&frontmost_app);
    let main_is_visible = is_window_visible(&app, "main");
    if !main_is_visible {
        let focus_snapshot = read_current_paste_focus_snapshot(&context.target_app);
        if let Ok(mut stored_snapshot) = app.state::<RuntimePasteFocusSnapshot>().snapshot.lock() {
            *stored_snapshot = focus_snapshot;
        }
    }
    if let Some(result) = app.get_webview_window("result") {
        let _ = result.hide();
    }
    if context.show_floating_window && !main_is_visible {
        let _ = present_window(&app, "main", true);
    } else if let Some(main) = app.get_webview_window("main") {
        if !main_is_visible {
            let _ = main.hide();
        }
    }
    let mode = mode.to_string();
    let target_app = context.target_app;
    let keep_hub_visible = context.keep_hub_visible;
    thread::spawn(move || {
        if !main_is_visible {
            thread::sleep(Duration::from_millis(180));
        }
        if let Some(window) = app.get_webview_window("main") {
            let mode_json =
                serde_json::to_string(&mode).unwrap_or_else(|_| "\"dictate\"".to_string());
            let target_app_json =
                serde_json::to_string(&target_app).unwrap_or_else(|_| "\"\"".to_string());
            let keep_hub_visible_json = keep_hub_visible.to_string();
            let script = format!(
                r#"if (window.__AIToolHandleShortcutMode) {{
                    window.__AIToolHandleShortcutMode({mode_json}, {target_app_json}, {keep_hub_visible_json});
                }} else {{
                    window.__AIToolPendingShortcutMode = {{ mode: {mode_json}, targetApp: {target_app_json}, keepHubVisible: {keep_hub_visible_json} }};
                }}"#
            );
            let _ = window.eval(&script);
        }
    });
}

/// 把快捷键字符串转换成 CodexMan 的语音模式。
fn shortcut_to_mode(app: &tauri::AppHandle, shortcut: &str) -> String {
    let normalized = normalize_shortcut(shortcut);
    let profile = app
        .state::<RuntimeShortcuts>()
        .profile
        .lock()
        .map(|profile| profile.clone())
        .unwrap_or_default();
    if normalized == normalize_shortcut(&profile.asr) {
        "asr".to_string()
    } else if normalized == normalize_shortcut(&profile.polish) {
        "polish".to_string()
    } else if normalized == normalize_shortcut(&profile.dictate) {
        "dictate".to_string()
    } else {
        String::new()
    }
}

/// 把窗口显示到前台；置顶仅用于悬浮录音条。
fn present_window(app: &tauri::AppHandle, label: &str, always_on_top: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("未找到窗口：{}", label))?;
    window
        .set_always_on_top(always_on_top)
        .map_err(|error| format!("设置窗口置顶失败：{}", error))?;
    if label == "main" {
        position_top_center_window(app, &window, FLOAT_WINDOW_WIDTH, FLOAT_WINDOW_TOP)?;
    }
    window
        .show()
        .map_err(|error| format!("显示窗口失败：{}", error))?;
    if label == "main" {
        return Ok(());
    }
    window
        .set_focus()
        .map_err(|error| format!("聚焦窗口失败：{}", error))
}

/// 注册前端提交的三种全局快捷键，保存后立即生效。
/// 流程：校验 hub、规范化配置并注册；失败时恢复旧快捷键；参数为窗口、快捷键配置和运行状态；返回实际配置。
/// 异常/边界：非 hub 默认拒绝，新旧配置均注册失败时返回明确错误并保留运行诊断。
#[tauri::command]
fn register_shortcuts(
    window: tauri::WebviewWindow,
    shortcuts: ShortcutProfile,
    state: State<'_, RuntimeShortcuts>,
) -> Result<ShortcutProfile, String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    let normalized = normalize_shortcut_profile(shortcuts)?;
    let previous = read_shortcut_runtime_profile(&state)?;
    match register_shortcut_profile(&app, &normalized) {
        Ok(()) => {
            set_shortcut_runtime(&state, normalized.clone(), None)?;
            Ok(normalized)
        }
        Err(error) => {
            match register_shortcut_profile(&app, &previous) {
                Ok(()) => {
                    let message = format!(
                        "新快捷键注册失败，已保留原快捷键：{}",
                        trim_error_message(&error)
                    );
                    let _ = set_shortcut_runtime_status(&state, previous, true, message);
                }
                Err(restore_error) => {
                    let message = format!(
                        "{}；恢复原快捷键失败：{}",
                        trim_error_message(&error),
                        trim_error_message(&restore_error)
                    );
                    let _ = set_shortcut_runtime(&state, normalized, Some(message));
                }
            }
            Err(error)
        }
    }
}

/// 进入快捷键录制态前临时注销全局快捷键，避免当前快捷键拦截 WebView 的按键回显。
/// 流程：校验 hub 后注销当前全局快捷键；参数为调用窗口；成功返回空值。
/// 异常/边界：非 hub 默认拒绝，系统注销失败时保持错误可见，禁止伪报已进入录制态。
#[tauri::command]
fn suspend_shortcuts_for_recording(window: tauri::WebviewWindow) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    suspend_shortcut_profile(&app)
}

/// 读取本机可绑定的应用列表。
/// 流程：校验 hub 窗口后扫描系统和用户 Applications 目录，返回去重排序后的 .app bundle。
/// 参数：window 为调用窗口，用于限制只有主界面能读取本机应用目录。
/// 返回：可供前端选择的应用列表。
/// 异常/边界：非 macOS 返回当前平台不支持；目录不存在时跳过，不把空目录视为错误。
#[tauri::command]
fn list_installed_applications(
    window: tauri::WebviewWindow,
) -> Result<Vec<ApplicationOption>, String> {
    ensure_sensitive_management_window(window.label())?;
    list_installed_applications_core()
}

/// 读取当前桌面端能力状态，供设置页展示真实诊断结果。
/// 流程：校验 hub 后汇总系统权限和快捷键状态；参数为窗口和运行状态；返回脱敏诊断结构。
/// 异常/边界：非 hub 默认拒绝，状态锁损坏时显式失败，不向临时窗口暴露系统权限状态。
#[tauri::command]
fn get_runtime_diagnostics(
    window: tauri::WebviewWindow,
    shortcuts: State<'_, RuntimeShortcuts>,
) -> Result<RuntimeDiagnostics, String> {
    ensure_sensitive_management_window(window.label())?;
    let profile = shortcuts
        .profile
        .lock()
        .map_err(|_| "读取快捷键状态失败：状态锁已损坏".to_string())?
        .clone();
    let shortcut_registration_status = shortcuts
        .registration_status
        .lock()
        .map_err(|_| "读取快捷键注册结果失败：状态锁已损坏".to_string())?
        .clone();

    Ok(RuntimeDiagnostics {
        accessibility_trusted: is_accessibility_trusted(),
        shortcuts: profile,
        shortcut_registration_ready: shortcut_registration_status.ready,
        shortcut_registration_message: shortcut_registration_status.message,
    })
}

/// 读取 Codex 工作空间列表的共享业务入口。
/// 流程：由私有 UDS RPC 优先只读本地状态库，失败时回退真实 app-server。
/// 参数：无；返回按活跃度整理的工作空间摘要。
/// 异常/边界：两条真实读取链路均失败时返回错误，不构造假数据或空成功。
pub(crate) fn list_codex_workspaces_core() -> Result<Vec<CodexWorkspaceSummary>, String> {
    read_codex_state_workspaces().or_else(|_| run_codex_app_server_workspaces())
}

/// 读取 Codex 会话列表的共享业务入口。
/// 流程：规范化目录、分页和关键词后优先查询只读状态库，失败时回退真实 app-server 并在内存中分页。
/// 参数：request 包含工作空间、limit、offset 和 keyword；返回有限会话摘要。
/// 异常/边界：limit 强制受既有上限约束，负 offset 归零；两条读取链路均失败时显式返回错误。
pub(crate) fn list_codex_threads_core(
    request: CodexThreadListRequest,
) -> Result<Vec<CodexThreadSummary>, String> {
    let cwd = normalize_codex_workspace_cwd(Some(&request.workspace_cwd));
    let limit = normalize_codex_thread_page_limit(request.limit);
    let offset = request.offset.max(0) as usize;
    let keyword = request.keyword.trim().to_string();
    read_codex_state_threads(&cwd, limit, offset, &keyword).or_else(|_| {
        run_codex_app_server_list(limit + offset, Some(&cwd)).map(|threads| {
            filter_codex_threads_by_keyword(threads, &keyword)
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect()
        })
    })
}

/// 按 MVP 的方式从 CodeX 本地 state_5.sqlite 读取工作空间列表。
/// 流程：打开 ~/.codex/state_5.sqlite，只统计未归档且有标题或预览的 threads.cwd。
/// 参数：无。
/// 返回：按最近活跃时间倒序排列的工作空间摘要。
/// 边界：状态库不存在或结构变化时返回错误，由调用方回退 app-server。
fn read_codex_state_workspaces() -> Result<Vec<CodexWorkspaceSummary>, String> {
    let connection = open_codex_state_database()?;
    let mut statement = connection
        .prepare(
            "
            SELECT cwd,
                   COUNT(*) AS thread_count,
                   MAX(COALESCE(NULLIF(recency_at_ms, 0), updated_at * 1000)) AS latest_at
              FROM threads
             WHERE archived = 0
               AND cwd <> ''
               AND (title <> '' OR preview <> '')
          GROUP BY cwd
          ORDER BY latest_at DESC, cwd COLLATE NOCASE ASC
            ",
        )
        .map_err(|error| format!("读取 CodeX 工作空间 SQL 准备失败：{}", error))?;
    let rows = statement
        .query_map([], |row| {
            let cwd: String = row.get(0)?;
            let latest_at: i64 = row.get(2)?;
            Ok(CodexWorkspaceSummary {
                title: codex_workspace_title(&cwd),
                cwd,
                thread_count: row.get::<_, i64>(1)?.max(0) as usize,
                updated_at: latest_at.to_string(),
            })
        })
        .map_err(|error| format!("读取 CodeX 工作空间失败：{}", error))?;
    collect_sqlite_rows(rows, "读取 CodeX 工作空间失败")
}

/// 按 MVP 的方式从 CodeX 本地 state_5.sqlite 读取指定工作空间会话。
/// 流程：按 cwd 过滤未归档 threads，并按置顶和最近活跃时间排序。
/// 参数：workspace_cwd 为工作空间绝对路径，limit 为最大返回数量，offset 为分页起点，keyword 为搜索关键词。
/// 返回：CodeX 会话摘要列表。
/// 边界：状态库不存在或结构变化时返回错误，由调用方回退 app-server。
fn read_codex_state_threads(
    workspace_cwd: &str,
    limit: usize,
    offset: usize,
    keyword: &str,
) -> Result<Vec<CodexThreadSummary>, String> {
    let connection = open_codex_state_database()?;
    let keyword_pattern = codex_thread_keyword_pattern(keyword);
    let mut statement = connection
        .prepare(
            "
            SELECT id,
                   SUBSTR(COALESCE(NULLIF(name, ''), NULLIF(title, ''), '未命名任务'), 1, 240) AS title,
                   CASE
                       WHEN json_valid(source) = 1
                       THEN COALESCE(json_extract(source, '$.subagent.thread_spawn.parent_thread_id'), '')
                       ELSE ''
                   END AS parent_thread_id,
                   CASE
                       WHEN json_valid(source) = 1
                       THEN COALESCE(json_extract(source, '$.subagent.thread_spawn.depth'), 0)
                       ELSE 0
                   END AS depth,
                   COALESCE(agent_nickname, '') AS agent_nickname,
                   COALESCE(agent_role, '') AS agent_role,
                   COALESCE(NULLIF(recency_at_ms, 0), updated_at * 1000) AS updated_at_ms
              FROM threads
             WHERE archived = 0
               AND cwd = ?1
               AND (title <> '' OR preview <> '')
               AND (?4 = '' OR id LIKE ?4 OR title LIKE ?4 OR name LIKE ?4 OR preview LIKE ?4)
          ORDER BY is_pinned DESC, updated_at_ms DESC, id DESC
             LIMIT ?2
            OFFSET ?3
            ",
        )
        .map_err(|error| format!("读取 CodeX 会话 SQL 准备失败：{}", error))?;
    let rows = statement
        .query_map(
            params![workspace_cwd, limit as i64, offset as i64, keyword_pattern],
            |row| {
                Ok(CodexThreadSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    parent_thread_id: row.get(2)?,
                    depth: row.get::<_, i64>(3)?.max(0),
                    agent_nickname: row.get(4)?,
                    agent_role: row.get(5)?,
                    updated_at: row.get::<_, i64>(6)?.to_string(),
                })
            },
        )
        .map_err(|error| format!("读取 CodeX 会话失败：{}", error))?;
    collect_sqlite_rows(rows, "读取 CodeX 会话失败")
}

/// 归一化 CodeX 会话分页数量，避免前端异常参数一次读取过多本地状态库记录。
fn normalize_codex_thread_page_limit(limit: i64) -> usize {
    if limit <= 0 {
        return 30;
    }
    (limit as usize).min(CODEX_THREAD_LIST_LIMIT)
}

/// 构造 CodeX 会话搜索 LIKE 参数。
fn codex_thread_keyword_pattern(keyword: &str) -> String {
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    format!("%{}%", trimmed)
}

/// 在 app-server 兜底数据中按关键词过滤会话。
fn filter_codex_threads_by_keyword(
    threads: Vec<CodexThreadSummary>,
    keyword: &str,
) -> Vec<CodexThreadSummary> {
    let normalized_keyword = keyword.trim().to_lowercase();
    if normalized_keyword.is_empty() {
        return threads;
    }
    threads
        .into_iter()
        .filter(|thread| {
            thread.id.to_lowercase().contains(&normalized_keyword)
                || thread.title.to_lowercase().contains(&normalized_keyword)
        })
        .collect()
}

/// 打开 CodeX 本地状态库，只读访问避免影响 CodeX Desktop 自身写入。
fn open_codex_state_database() -> Result<Connection, String> {
    let path = codex_home_dir()?.join("state_5.sqlite");
    if !path.exists() {
        return Err(format!("未找到 CodeX 状态库：{}", path.display()));
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("打开 CodeX 状态库失败：{}", error))
}

/// 在现有 Codex 状态库连接中确认会话主键是否存在。
/// 流程：使用精确主键和未归档条件执行 EXISTS 查询；参数为只读连接与已通过格式校验的 thread ID；返回是否存在。
/// 异常/边界：数据库结构变化或读取失败显式返回，由上层尝试其它真实读取能力；不接受前缀或模糊匹配。
fn codex_thread_exists_in_state_connection(
    connection: &Connection,
    thread_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM threads WHERE id = ?1 AND archived = 0)",
            params![thread_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("校验 CodeX 会话存在性失败：{}", error))
}

/// 通过现有权威读取链确认目标 Codex 会话真实存在。
/// 流程：优先精确查询只读 state_5.sqlite；状态库不可用时读取 app-server 详情，再以本地索引和精确 JSONL 文件读取共同确认。
/// 参数：thread_id 为已完成格式校验的稳定 ID；存在返回空值。
/// 异常/边界：权威状态库明确返回不存在时立即报 CODEX_THREAD_NOT_FOUND；仅状态库无法读取时才切换真实后备源，不把任意合法形状 ID 当作存在。
fn ensure_codex_thread_exists(thread_id: &str) -> Result<(), String> {
    if let Ok(connection) = open_codex_state_database() {
        match codex_thread_exists_in_state_connection(&connection, thread_id) {
            Ok(true) => return Ok(()),
            Ok(false) => {
                return Err("Codex 会话不存在或已归档（错误码：CODEX_THREAD_NOT_FOUND）".to_string())
            }
            Err(_) => {}
        }
    }
    if run_codex_app_server_thread_detail(thread_id).is_ok()
        || (read_codex_thread_index()
            .map(|threads| threads.iter().any(|thread| thread.id == thread_id))
            .unwrap_or(false)
            && find_codex_session_file(thread_id)
                .and_then(|path| read_codex_session_messages(&path).map(|_| ()))
                .is_ok())
    {
        return Ok(());
    }
    Err("Codex 会话不存在或已归档（错误码：CODEX_THREAD_NOT_FOUND）".to_string())
}

/// 收集 rusqlite 查询行并统一转换错误文案。
fn collect_sqlite_rows<T, F>(
    rows: rusqlite::MappedRows<'_, F>,
    context: &str,
) -> Result<Vec<T>, String>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("{}：{}", context, error))?);
    }
    Ok(items)
}

/// 打开 Codex Desktop 会话的共享业务入口。
/// 流程：规范化并校验 thread ID，通过 Codex 权威状态库确认记录存在后，才向系统提交 deeplink 打开请求。
/// 参数：thread_id 为已绑定的 Codex 会话 ID；返回已提交给系统的 deeplink URL，不代表 Codex 已完成页面切换。
/// 异常/边界：空值、非法 ID 或权威索引不存在均在启动系统进程前拒绝，禁止为任意合法形状 ID 构造成功。
pub(crate) fn open_session_external_thread_core(thread_id: String) -> Result<String, String> {
    let normalized_id = thread_id.trim();
    if normalized_id.is_empty() {
        return Err("当前任务还没有绑定 CodeX 会话".to_string());
    }
    validate_codex_thread_id(normalized_id)?;
    ensure_codex_thread_exists(normalized_id)?;
    open_codex_desktop_thread(normalized_id)
}

/// 通过 Codex app-server stdio 读取已有任务中的工作空间列表。
fn run_codex_app_server_workspaces() -> Result<Vec<CodexWorkspaceSummary>, String> {
    let mut session = CodexAppServerSession::start()?;
    session.initialize()?;
    let response = session.request(
        2,
        "thread/list",
        json!({
            "limit": 120,
            "archived": false,
            "sortKey": "updated_at",
            "sortDirection": "desc"
        }),
    )?;
    let threads = response
        .get("result")
        .and_then(|result| result.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex 工作空间响应缺少 data".to_string())?;
    Ok(parse_codex_workspaces(threads))
}

/// 通过 Codex app-server stdio 读取桌面任务列表，确保 CodexMan 与 Codex 侧边栏使用同一套任务数据。
fn run_codex_app_server_list(
    limit: usize,
    workspace_cwd: Option<&str>,
) -> Result<Vec<CodexThreadSummary>, String> {
    let mut session = CodexAppServerSession::start()?;
    session.initialize()?;
    let mut params = json!({
        "limit": limit,
        "archived": false,
        "sortKey": "updated_at",
        "sortDirection": "desc"
    });
    if let Some(cwd) = workspace_cwd {
        if let Value::Object(map) = &mut params {
            map.insert("cwd".to_string(), json!(cwd));
        }
    }
    let response = session.request(2, "thread/list", params)?;
    let threads = response
        .get("result")
        .and_then(|result| result.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex 桌面任务列表响应缺少 data".to_string())?;
    Ok(threads
        .iter()
        .filter_map(parse_codex_app_thread_summary)
        .collect())
}

/// 通过 Codex app-server 读取任务详情，确保 CodexMan 和 Codex Desktop 数据层保持一致。
fn run_codex_app_server_thread_detail(thread_id: &str) -> Result<CodexThreadDetail, String> {
    let mut session = CodexAppServerSession::start()?;
    session.initialize()?;
    let response = session.request(
        2,
        "thread/read",
        json!({
            "threadId": thread_id,
            "includeTurns": true
        }),
    )?;
    let thread = response
        .get("result")
        .and_then(|result| result.get("thread"))
        .ok_or_else(|| "Codex 任务详情响应缺少 thread".to_string())?;
    parse_codex_app_thread_detail(thread)
}

/// 使用 Codex Desktop deeplink 打开指定任务，让真实 Desktop 侧边栏立即选中该任务。
fn open_codex_desktop_thread(thread_id: &str) -> Result<String, String> {
    let url = format!("codex://threads/{}", thread_id);
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "打开 Codex Desktop 任务失败：{}",
                    trim_error_message(&error.to_string())
                )
            })?;
    }
    Ok(url)
}

/// 将 app-server 返回的 Thread 对象转成前端列表需要的摘要。
fn parse_codex_app_thread_summary(value: &Value) -> Option<CodexThreadSummary> {
    let id = value.get("id").and_then(Value::as_str)?.to_string();
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))
        .or_else(|| value.get("threadName").and_then(Value::as_str))
        .or_else(|| value.get("preview").and_then(Value::as_str))
        .unwrap_or("未命名任务")
        .trim();
    let updated_at = value
        .get("updatedAt")
        .and_then(Value::as_i64)
        .map(|timestamp| (timestamp * 1000).to_string())
        .unwrap_or_default();
    Some(CodexThreadSummary {
        id,
        title: if title.is_empty() {
            "未命名任务".to_string()
        } else {
            limit_chars(title, 60)
        },
        parent_thread_id: value
            .get("source")
            .and_then(|source| source.get("subagent"))
            .and_then(|subagent| subagent.get("thread_spawn"))
            .and_then(|thread_spawn| thread_spawn.get("parent_thread_id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        depth: value
            .get("source")
            .and_then(|source| source.get("subagent"))
            .and_then(|subagent| subagent.get("thread_spawn"))
            .and_then(|thread_spawn| thread_spawn.get("depth"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0),
        agent_nickname: value
            .get("agentNickname")
            .or_else(|| value.get("agent_nickname"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        agent_role: value
            .get("agentRole")
            .or_else(|| value.get("agent_role"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        updated_at,
    })
}

/// 将 app-server 的 Thread 详情转成前端可展示的消息列表。
fn parse_codex_app_thread_detail(value: &Value) -> Result<CodexThreadDetail, String> {
    let thread_id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Codex 任务详情缺少 id".to_string())?;
    let title = value
        .get("name")
        .or_else(|| value.get("title"))
        .or_else(|| value.get("preview"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("未命名会话");
    let updated_at = value
        .get("updatedAt")
        .and_then(Value::as_i64)
        .map(|timestamp| (timestamp * 1000).to_string())
        .unwrap_or_default();
    let mut messages = Vec::new();
    if let Some(turns) = value.get("turns").and_then(Value::as_array) {
        for turn in turns {
            let created_at = turn
                .get("startedAt")
                .and_then(Value::as_i64)
                .map(|timestamp| (timestamp * 1000).to_string())
                .unwrap_or_else(|| updated_at.clone());
            if let Some(items) = turn.get("items").and_then(Value::as_array) {
                for item in items {
                    if let Some(message) = parse_codex_app_thread_item(item, &created_at) {
                        messages.push(message);
                    }
                }
            }
        }
    }
    let skip_count = messages.len().saturating_sub(CODEX_THREAD_MESSAGE_LIMIT);
    Ok(CodexThreadDetail {
        id: thread_id.to_string(),
        title: title.to_string(),
        updated_at,
        messages: messages.into_iter().skip(skip_count).collect(),
    })
}

/// 从 app-server 的单个 ThreadItem 中抽取用户或助手消息。
fn parse_codex_app_thread_item(value: &Value, created_at: &str) -> Option<CodexThreadMessage> {
    match value.get("type").and_then(Value::as_str)? {
        "userMessage" => Some(CodexThreadMessage {
            role: "user".to_string(),
            content: limit_chars(
                &extract_codex_app_user_message(value),
                CODEX_MESSAGE_CONTENT_MAX_CHARS,
            ),
            created_at: created_at.to_string(),
        }),
        "agentMessage" => Some(CodexThreadMessage {
            role: "assistant".to_string(),
            content: limit_chars(
                value.get("text").and_then(Value::as_str).unwrap_or(""),
                CODEX_MESSAGE_CONTENT_MAX_CHARS,
            ),
            created_at: created_at.to_string(),
        }),
        _ => None,
    }
}

/// 从 app-server 用户消息内容数组中拼出纯文本，兼容未来多段文本结构。
fn extract_codex_app_user_message(value: &Value) -> String {
    value
        .get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_default()
}

/// 将 app-server 返回的近期任务按 cwd 合并成工作空间列表。
fn parse_codex_workspaces(threads: &[Value]) -> Vec<CodexWorkspaceSummary> {
    let mut workspaces = HashMap::<String, CodexWorkspaceSummary>::new();
    for thread in threads {
        let Some(cwd) = thread.get("cwd").and_then(Value::as_str) else {
            continue;
        };
        let updated_at = thread
            .get("updatedAt")
            .and_then(Value::as_i64)
            .map(|timestamp| (timestamp * 1000).to_string())
            .unwrap_or_default();
        let entry = workspaces
            .entry(cwd.to_string())
            .or_insert_with(|| CodexWorkspaceSummary {
                cwd: cwd.to_string(),
                title: codex_workspace_title(cwd),
                thread_count: 0,
                updated_at: updated_at.clone(),
            });
        entry.thread_count += 1;
        if updated_at > entry.updated_at {
            entry.updated_at = updated_at;
        }
    }
    let mut values = workspaces.into_values().collect::<Vec<_>>();
    if values.is_empty() {
        values.push(CodexWorkspaceSummary {
            cwd: default_codex_workspace_cwd(),
            title: codex_workspace_title(&default_codex_workspace_cwd()),
            thread_count: 0,
            updated_at: String::new(),
        });
    }
    values.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    values
}

/// 使用工作空间目录名作为下拉主标题，路径仍在辅助信息中完整展示。
fn codex_workspace_title(cwd: &str) -> String {
    PathBuf::from(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(cwd)
        .to_string()
}

/// 归一化前端传入的工作空间路径，空值回落到默认 monorepo。
fn normalize_codex_workspace_cwd(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(str::to_string)
        .unwrap_or_else(default_codex_workspace_cwd)
}

/// CodexMan 默认管理当前 monorepo 下的 aitool/Codex 任务，环境变量可覆盖。
fn default_codex_workspace_cwd() -> String {
    env::var("CODEXMAN_CODEX_CWD")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/Users/lucifer/Documents/source/t/monorepo".to_string())
}

/// Codex app-server stdio 短连接，封装初始化、请求发送和响应读取。
struct CodexAppServerSession {
    /// 当前 app-server 子进程。
    child: std::process::Child,
    /// app-server stdin，用于写入 JSON-RPC 请求。
    stdin: std::process::ChildStdin,
    /// app-server stdout 有界解析通道；Err 只携带脱敏协议诊断，支持带截止时间等待并向读线程施加背压。
    messages: mpsc::Receiver<Result<Value, String>>,
    /// 等待请求响应期间提前到达的通知，避免极快 turn 的 completed 事件被丢弃。
    pending_notifications: Vec<Value>,
}

impl CodexAppServerSession {
    /// 启动一个 app-server stdio 子进程，短连接结束时由 Drop 清理。
    fn start() -> Result<Self, String> {
        let mut command = if fs::metadata(CODEX_DESKTOP_BIN).is_ok() {
            let mut command = Command::new(CODEX_DESKTOP_BIN);
            command.args(["app-server", "--stdio"]);
            command
        } else {
            let mut command = Command::new("zsh");
            command.args([
                "-lc",
                "source ~/.zshrc >/dev/null 2>&1 || true; codex app-server --stdio",
            ]);
            command
        };
        let diagnostic_path = env::temp_dir().join("codexman-codex-app-server.log");
        initialize_codex_diagnostic_log(&diagnostic_path);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                format!(
                    "启动 Codex app-server 失败：{}",
                    trim_error_message(&error.to_string())
                )
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Codex app-server stdin 不可用".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Codex app-server stdout 不可用".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Codex app-server stderr 不可用".to_string())?;
        let (sender, messages) = mpsc::sync_channel(CODEX_APP_SERVER_CHANNEL_CAPACITY);
        let stdout_diagnostic_path = diagnostic_path.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_bounded_codex_jsonl_frame(&mut reader, CODEX_APP_SERVER_FRAME_MAX_BYTES)
                {
                    Ok(None) => return,
                    Ok(Some(frame)) => {
                        if let Ok(value) = serde_json::from_slice::<Value>(&frame) {
                            if sender.send(Ok(value)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        append_codex_stdout_diagnostic(&stdout_diagnostic_path, &error);
                        let _ = sender.send(Err(error));
                        return;
                    }
                }
            }
        });
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            loop {
                match consume_codex_stderr_record(
                    &mut reader,
                    CODEX_APP_SERVER_STDERR_RECORD_MAX_BYTES,
                ) {
                    Ok(None) => return,
                    Ok(Some(was_truncated)) => append_codex_diagnostic(
                        &diagnostic_path,
                        if was_truncated {
                            "CODEX_APP_SERVER_STDERR_RECORD_TRUNCATED"
                        } else {
                            "CODEX_APP_SERVER_STDERR_REPORTED"
                        },
                    ),
                    Err(()) => {
                        append_codex_diagnostic(
                            &diagnostic_path,
                            "CODEX_APP_SERVER_STDERR_READ_FAILED",
                        );
                        return;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            pending_notifications: Vec::new(),
        })
    }

    /// 完成 app-server 初始化握手，开启 experimental v2 API。
    fn initialize(&mut self) -> Result<(), String> {
        let _ = self.request(
            1,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "codexman",
                    "title": "CodexMan",
                    "version": "0.0.2"
                },
                "capabilities": {
                    "experimentalApi": true,
                    "requestAttestation": false,
                    "optOutNotificationMethods": [
                        "item/agentMessage/delta",
                        "command/exec/outputDelta",
                        "rawResponseItem/completed"
                    ]
                }
            }),
        )?;
        self.write_value(&json!({ "method": "initialized" }))
    }

    /// 写入一个 JSON-RPC 请求并等待对应 id 的成功响应。
    fn request(&mut self, id: i64, method: &str, params: Value) -> Result<Value, String> {
        self.write_value(&json!({
            "id": id,
            "method": method,
            "params": params
        }))?;
        self.read_response(id)
    }

    /// 向 app-server 写入一行 JSON。
    fn write_value(&mut self, value: &Value) -> Result<(), String> {
        let line = serde_json::to_string(value)
            .map_err(|error| format!("序列化 Codex app-server 请求失败：{}", error))?;
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|error| {
                format!(
                    "写入 Codex app-server 请求失败：{}",
                    trim_error_message(&error.to_string())
                )
            })
    }

    /// 读取 app-server 输出，跳过通知，只返回指定 id 的 JSON-RPC 响应。
    fn read_response(&mut self, id: i64) -> Result<Value, String> {
        let deadline = Instant::now() + CODEX_APP_SERVER_RESPONSE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("等待 Codex app-server 响应超过总截止时间".to_string());
            }
            let value = self.messages.recv_timeout(remaining).map_err(|error| {
                format!("等待 Codex app-server 响应超时或通道关闭：{}", error)
            })??;
            if value.get("id").and_then(Value::as_i64) != Some(id) {
                if value.get("method").is_some() {
                    if self.pending_notifications.len() >= CODEX_PENDING_NOTIFICATION_CAPACITY {
                        return Err(format!(
                            "Codex app-server 待处理通知超过 {} 条，已停止当前会话（错误码：CODEX_NOTIFICATION_BUFFER_FULL）",
                            CODEX_PENDING_NOTIFICATION_CAPACITY
                        ));
                    }
                    self.pending_notifications.push(value);
                }
                continue;
            }
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex app-server 返回错误");
                return Err(message.to_string());
            }
            return Ok(value);
        }
    }
}

/// 从 app-server stdout 读取一条有界 JSONL 帧。
/// 流程：通过 `Read::take` 最多读取上限加一字节，识别 LF/CRLF 后返回不含换行的原始 JSON 字节；参数为缓冲读取器和字节上限。
/// 返回：EOF 返回 None，合法帧返回 Some；异常/边界：帧超过上限或达到上限仍无换行立即返回固定脱敏错误，禁止继续分片解析同一 JSON。
fn read_bounded_codex_jsonl_frame<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::with_capacity(max_bytes.min(8 * 1024));
    let bytes_read = reader
        .take((max_bytes.saturating_add(2)) as u64)
        .read_until(b'\n', &mut frame)
        .map_err(|_| {
            "读取 Codex app-server stdout 失败（错误码：CODEX_STDOUT_READ_FAILED）".to_string()
        })?;
    if bytes_read == 0 {
        return Ok(None);
    }
    let has_newline = frame.last() == Some(&b'\n');
    if has_newline {
        frame.pop();
        if frame.last() == Some(&b'\r') {
            frame.pop();
        }
    }
    if frame.len() > max_bytes || (!has_newline && bytes_read > max_bytes) {
        return Err(format!(
            "Codex app-server stdout 单帧超过 {} 字节，已停止读取（错误码：CODEX_STDOUT_FRAME_TOO_LARGE）",
            max_bytes
        ));
    }
    Ok(Some(frame))
}

/// 向 app-server 诊断日志追加固定 stdout 协议错误。
/// 流程：以 append 模式写入单行脱敏摘要；参数为既有诊断日志路径和内部固定错误；返回无。
/// 异常/边界：不接收或记录 stdout 帧正文、prompt、结果或密钥；日志写入失败不覆盖通道返回的主错误。
fn append_codex_stdout_diagnostic(path: &Path, error: &str) {
    let code = if error.contains("CODEX_STDOUT_FRAME_TOO_LARGE") {
        "CODEX_STDOUT_FRAME_TOO_LARGE"
    } else {
        "CODEX_STDOUT_READ_FAILED"
    };
    append_codex_diagnostic(path, code);
}

/// 消费一条 app-server stderr 记录而不保留原始正文。
/// 流程：直接检查 BufRead 内部缓冲区并持续 consume 到 LF 或 EOF，只累计字节数和是否超限；参数为 stderr 读取器与单条上限。
/// 返回：EOF 且无数据返回 None，读到记录返回其是否被截断；异常/边界：超长无换行记录不分配同等内存、不回显正文，读取失败返回固定空错误。
fn consume_codex_stderr_record<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<bool>, ()> {
    let mut consumed_bytes = 0usize;
    let mut has_data = false;
    loop {
        let buffer = reader.fill_buf().map_err(|_| ())?;
        if buffer.is_empty() {
            return Ok(has_data.then_some(consumed_bytes > max_bytes));
        }
        has_data = true;
        let consumed = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(buffer.len());
        let has_newline = buffer.get(consumed.saturating_sub(1)) == Some(&b'\n');
        consumed_bytes = consumed_bytes.saturating_add(consumed);
        reader.consume(consumed);
        if has_newline {
            return Ok(Some(consumed_bytes > max_bytes));
        }
    }
}

/// 向 Codex app-server 诊断日志追加一条固定代码，并在每次写入前实时执行单备份轮转。
/// 流程：获取进程内写锁，若活动日志达到上限则替换 `.1` 备份，随后只写固定 ASCII 诊断代码；参数为固定日志路径和内部代码。
/// 返回：无；异常/边界：不接受 stderr/stdout 正文、prompt、结果或路径作为日志内容，目录、轮转和写入失败均不影响主业务错误。
fn append_codex_diagnostic(path: &Path, code: &'static str) {
    let _guard = CODEX_APP_SERVER_DIAGNOSTIC_LOG_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let backup_path = path.with_extension("log.1");
    let next_record_bytes = code.len().saturating_add(1) as u64;
    if fs::metadata(path)
        .map(|metadata| {
            metadata.len().saturating_add(next_record_bytes)
                > CODEX_APP_SERVER_DIAGNOSTIC_LOG_MAX_BYTES
        })
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&backup_path);
        let _ = fs::rename(path, &backup_path);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", code);
    }
}

/// 初始化当前进程的 Codex app-server 脱敏诊断日志。
/// 流程：首次启动 app-server 时在写锁内删除活动文件和单个备份，后续短连接复用同一日志；参数为固定临时日志路径。
/// 返回：无；异常/边界：清理失败不阻止 app-server 启动，后续所有新增记录仍只允许固定诊断代码，禁止继承旧版可能保存的原始 stderr。
fn initialize_codex_diagnostic_log(path: &Path) {
    CODEX_APP_SERVER_DIAGNOSTIC_LOG_INITIALIZE.call_once(|| {
        let _guard = CODEX_APP_SERVER_DIAGNOSTIC_LOG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("log.1"));
    });
}

/// 匹配指定 thread/turn 的可靠完成通知。
/// 流程：忽略其它方法、thread 和 turn，只解析目标 `turn/completed`；参数为通知与目标标识；返回可选终态。
/// 异常/边界：仅目标 thread 的畸形通知报协议错误，避免无关任务的异常消息中断当前等待。
#[cfg(test)]
fn parse_matching_terminal_notification(
    value: &Value,
    thread_id: &str,
    turn_id: &str,
) -> Result<Option<(String, String, String)>, String> {
    if value.get("method").and_then(Value::as_str) != Some("turn/completed") {
        return Ok(None);
    }
    let params = value
        .get("params")
        .ok_or_else(|| "turn/completed 通知缺少 params".to_string())?;
    if params.get("threadId").and_then(Value::as_str) != Some(thread_id) {
        return Ok(None);
    }
    let turn = params
        .get("turn")
        .ok_or_else(|| "turn/completed 通知缺少 turn".to_string())?;
    if turn.get("id").and_then(Value::as_str) != Some(turn_id) {
        return Ok(None);
    }
    parse_codex_terminal_turn(turn).map(Some)
}

/// 通过 Codex Desktop 原生 composer 提交一个真实任务，并启动后台终态对账。
/// 流程：记录 Enter 前已知 thread，CDP 精确切换工作区和新会话，回调事务持久化提交水位后只按一次 Enter，再以 canonical cwd、水位和首条用户消息从 JSONL 唯一恢复 thread 并绑定。
/// 参数：AppHandle、队列任务和本地 session ID；返回无；只有 Enter 前的确定失败向调度线程传播，绑定后交给 thread/read 监控。
/// 异常/边界：Enter 后无论传输是否成功都绝不重放 prompt；零或多候选、UI 与 JSONL 冲突均原子标记 sendUncertain 并返回成功，避免调度器覆盖为普通 failed。
fn execute_codex_task(
    app: &AppHandle,
    task: &task_store::QueuedTaskRecord,
    session_id: &str,
) -> Result<(), String> {
    let canonical_cwd = fs::canonicalize(&task.workspace_path)
        .map_err(|error| format!("任务工作目录已失效：{}", error))?;
    if !canonical_cwd.is_dir() {
        return Err("任务工作目录不是已存在目录".to_string());
    }
    let cwd = canonical_cwd.to_string_lossy().to_string();
    let known_thread_ids = capture_codex_thread_ids(&cwd)?;
    let client_user_message_id = uuid::Uuid::new_v4().to_string();
    let receipt = codex_cdp::submit_new_chat(&cwd, &task.prompt, &task.attachments, |watermark| {
        task_store::mark_task_submission_started(
            app,
            &task.id,
            session_id,
            &client_user_message_id,
            watermark,
            &known_thread_ids.iter().cloned().collect::<Vec<_>>(),
        )?;
        Ok(())
    });
    let (watermark, ui_thread_id) = match receipt {
        Ok(receipt) => (receipt.submitted_at_ms, receipt.thread_id),
        Err(failure) if failure.submission_uncertain => {
            let diagnostic =
                record_desktop_task_error(app, &task.id, failure.code, &failure.message);
            if let Err(error) =
                task_store::mark_task_send_uncertain(app, &task.id, session_id, &diagnostic)
            {
                let _ =
                    record_desktop_task_error(app, &task.id, "TASK_FAILURE_PERSIST_FAILED", &error);
            } else {
                emit_session_task_updated(app, &task.id, &task.project_id);
            }
            return Ok(());
        }
        Err(failure) => return Err(format!("{}（错误码：{}）", failure.message, failure.code)),
    };
    let thread_id = match recover_cdp_thread_from_jsonl(
        &cwd,
        &task.prompt,
        watermark,
        &known_thread_ids,
        ui_thread_id.as_deref(),
    ) {
        Ok(thread_id) => thread_id,
        Err(error) => {
            let diagnostic =
                record_desktop_task_error(app, &task.id, "CODEX_SEND_UNCERTAIN", &error);
            task_store::mark_task_send_uncertain(app, &task.id, session_id, &diagnostic)?;
            emit_session_task_updated(app, &task.id, &task.project_id);
            return Ok(());
        }
    };
    task_store::bind_task_thread(
        app,
        &task.id,
        session_id,
        &thread_id,
        &client_user_message_id,
    )?;
    let monitor_app = app.clone();
    let running = RunningTaskRecord {
        task_id: task.id.clone(),
        project_id: task.project_id.clone(),
        session_id: session_id.to_string(),
        thread_id,
        turn_id: String::new(),
    };
    thread::spawn(move || monitor_reconciled_task(&monitor_app, running));
    Ok(())
}

/// 捕获 Enter 前指定 canonical cwd 已存在的 thread ID 集合。
/// 流程：从权威 state_5.sqlite 精确按 cwd 查询全部 thread ID；参数为 canonical cwd；返回去重集合。
/// 异常/边界：状态库不可用时显式失败且不发送；禁止用全局最新 thread 或标题近似值替代快照。
fn capture_codex_thread_ids(canonical_cwd: &str) -> Result<HashSet<String>, String> {
    let connection = open_codex_state_database()?;
    let mut statement = connection
        .prepare("SELECT id FROM threads WHERE cwd = ?1")
        .map_err(|_| "准备 Codex thread 水位查询失败".to_string())?;
    let rows = statement
        .query_map(params![canonical_cwd], |row| row.get::<_, String>(0))
        .map_err(|_| "读取 Codex thread 水位失败".to_string())?;
    let mut ids = HashSet::new();
    for row in rows {
        ids.insert(row.map_err(|_| "读取 Codex thread 水位失败".to_string())?);
    }
    Ok(ids)
}

/// 从 Enter 后新增的 session JSONL 中恢复唯一真实 thread ID。
/// 流程：在截止时间内扫描水位后修改且不在旧快照的文件，逐个精确匹配 canonical cwd 与第一条用户消息；唯一候选还必须与可用 UI thread ID 一致。
/// 参数：cwd、原始 prompt、Unix 毫秒水位、旧 ID 集合和可选 UI ID；返回唯一真实 ID。
/// 异常/边界：零候选持续等待，多个候选或 UI 冲突立即 fail closed；超时只返回稳定诊断，绝不选择最新 thread 或重放 prompt。
fn recover_cdp_thread_from_jsonl(
    canonical_cwd: &str,
    prompt: &str,
    submitted_at_ms: i64,
    known_thread_ids: &HashSet<String>,
    ui_thread_id: Option<&str>,
) -> Result<String, String> {
    if submitted_at_ms <= 0 {
        return Err("Codex Desktop 提交恢复时间水位无效。".to_string());
    }
    let deadline = Instant::now() + CODEX_CDP_THREAD_RECOVERY_TIMEOUT;
    let sessions_dir = codex_home_dir()?.join("sessions");
    while Instant::now() < deadline {
        let files = collect_codex_session_files(&sessions_dir)?;
        let mut candidates = HashSet::new();
        for (path, modified_ms) in files {
            let Some(identity) = read_codex_submission_identity(&path, submitted_at_ms)? else {
                continue;
            };
            if submission_identity_matches(
                &identity,
                modified_ms,
                submitted_at_ms,
                known_thread_ids,
                canonical_cwd,
                prompt,
            ) && (codex_thread_created_after_watermark(
                &identity.thread_id,
                canonical_cwd,
                submitted_at_ms,
            )? || identity.first_user_message_at_ms >= submitted_at_ms)
            {
                candidates.insert(identity.thread_id);
            }
        }
        match candidates.len() {
            0 => thread::sleep(CODEX_CDP_THREAD_RECOVERY_INTERVAL),
            _ => {
                let candidate = select_unique_recovered_thread(&candidates, ui_thread_id)?;
                validate_codex_thread_id(&candidate)?;
                return Ok(candidate);
            }
        }
    }
    Err("Codex Desktop 提交后未在限定时间内恢复唯一 thread。".to_string())
}

/// 使用权威状态库确认候选 thread 确实在提交水位后创建。
/// 流程：按 thread ID、canonical cwd 和 created_at_ms 精确毫秒下界做单条 EXISTS；参数为候选身份和提交水位；返回是否为本次新建记录。
/// 异常/边界：状态库不可用或结构异常显式失败并进入 sendUncertain；不允许崩溃恢复因旧 ID 快照不在内存而放宽到旧 thread。
fn codex_thread_created_after_watermark(
    thread_id: &str,
    canonical_cwd: &str,
    submitted_at_ms: i64,
) -> Result<bool, String> {
    let connection = open_codex_state_database()?;
    codex_thread_created_after_watermark_with_connection(
        &connection,
        thread_id,
        canonical_cwd,
        submitted_at_ms,
    )
}

/// 在已打开的权威状态库上执行精确 thread 创建水位判断。
/// 流程：要求正水位，再按 thread ID、cwd 和 `COALESCE(created_at_ms, created_at * 1000) >= submittedAtMs` 查询；参数为连接及提交身份；返回是否命中。
/// 异常/边界：零/负水位直接返回 false，绝不进入 JSONL 恢复；本方法供生产查询和内存数据库调用链测试共用。
fn codex_thread_created_after_watermark_with_connection(
    connection: &Connection,
    thread_id: &str,
    canonical_cwd: &str,
    submitted_at_ms: i64,
) -> Result<bool, String> {
    if submitted_at_ms <= 0 {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM threads WHERE id = ?1 AND cwd = ?2 AND COALESCE(created_at_ms, created_at * 1000) >= ?3 LIMIT 1)",
            params![thread_id, canonical_cwd, submitted_at_ms],
            |row| row.get(0),
        )
        .map_err(|_| "校验 Codex thread 创建水位失败".to_string())
}

/// 判断单个 session 身份是否属于本次 CDP 提交。
/// 流程：依次验证文件修改水位、旧 thread 快照、canonical cwd 和完整首条用户消息；参数为候选及本次提交边界；返回严格匹配结果。
/// 异常/边界：使用精确相等，不接受标题、preview、basename、包含关系或大小写折叠；文件修改时间不得早于提交水位，旧 ID 始终优先排除。
fn submission_identity_matches(
    identity: &CodexSubmissionIdentity,
    modified_ms: u128,
    submitted_at_ms: i64,
    known_thread_ids: &HashSet<String>,
    canonical_cwd: &str,
    prompt: &str,
) -> bool {
    submitted_at_ms > 0
        && modified_ms >= submitted_at_ms as u128
        && (!known_thread_ids.contains(&identity.thread_id)
            || identity.first_user_message_at_ms >= submitted_at_ms)
        && identity.first_user_message_at_ms >= submitted_at_ms
        && identity.canonical_cwd == canonical_cwd
        && codex_user_message_matches_prompt(&identity.first_user_message, prompt)
}

/// 判断 Codex JSONL 用户消息是否对应本次提交的原始 prompt。
/// 流程：先做裸文本精确匹配；若 Codex Desktop 自动追加 ambient UI state，则提取 `## My request:` 后的正文再匹配。
/// 参数：message 为 JSONL 用户消息，prompt 为任务原始 prompt；返回是否为同一提交。
/// 异常/边界：只允许去除 Codex Desktop 固定包裹和末尾换行，不接受包含、大小写折叠或普通空白 trim。
fn codex_user_message_matches_prompt(message: &str, prompt: &str) -> bool {
    let normalized_prompt = normalize_codex_user_message(prompt);
    normalize_codex_user_message(message) == normalized_prompt
        || extract_codex_wrapped_request_message(message)
            .is_some_and(|request| request == normalized_prompt)
}

/// 规范化 Codex JSONL 用户消息用于提交恢复精确比对。
/// 流程：仅移除末尾 CR/LF，因为 composer 按 Enter 后 Codex 会把用户消息持久化为带换行文本；其它空白、大小写和内容必须保持精确。
/// 参数：``message`` 为任务 prompt 或 JSONL 中的首条用户消息。
/// 返回：只去掉末尾换行符的比较视图。
/// 异常/边界：不会 trim 普通空格、tab 或中间换行，避免把不同 prompt 误判为同一任务。
fn normalize_codex_user_message(message: &str) -> &str {
    message.trim_end_matches(['\r', '\n'])
}

/// 提取 Codex Desktop 自动包裹消息中的真实用户请求正文。
/// 流程：允许开头存在 in-app browser ambient context 或文件附件清单，随后必须包含固定 `## My request:` 标记；返回标记后的正文。
/// 参数：message 为 JSONL 用户消息；返回去除末尾 CR/LF 后的正文切片。
/// 异常/边界：不移除普通空格或正文内部换行；缺少固定标记时返回 None，避免把任意长文本误当作任务 prompt。
fn extract_codex_wrapped_request_message(message: &str) -> Option<&str> {
    let mut remaining = message.trim_start_matches(['\r', '\n']);
    if remaining.starts_with("<in-app-browser-context ") {
        let context_end = remaining.find("</in-app-browser-context>")?;
        remaining = &remaining[context_end + "</in-app-browser-context>".len()..];
        remaining = remaining.trim_start_matches(['\r', '\n']);
    }
    if remaining.starts_with("# Files mentioned by the user:") {
        let request_start = remaining.find("## My request:")?;
        remaining = &remaining[request_start..];
    }
    let request = remaining.strip_prefix("## My request:")?;
    Some(normalize_codex_user_message(
        request.trim_start_matches(['\r', '\n']),
    ))
}

/// 解析 Codex JSONL UTC 时间为 Unix 毫秒。
/// 流程：只接受 ``YYYY-MM-DDTHH:MM:SS(.mmm)Z`` 这类固定 UTC 文本，使用纯整数 civil-date 算法换算天数。
/// 参数：``value`` 为 JSONL ``timestamp`` 字段。
/// 返回：合法时返回 Unix epoch 毫秒；非法或非 UTC 时返回 None。
/// 异常/边界：不接受本地时区、offset、闰秒或缺字段，失败只让恢复继续等待或进入 sendUncertain。
fn parse_codex_jsonl_timestamp_ms(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
        || bytes.last() != Some(&b'Z')
    {
        return None;
    }
    let year = parse_fixed_digits(value, 0, 4)? as i32;
    let month = parse_fixed_digits(value, 5, 2)? as i32;
    let day = parse_fixed_digits(value, 8, 2)? as i32;
    let hour = parse_fixed_digits(value, 11, 2)? as i64;
    let minute = parse_fixed_digits(value, 14, 2)? as i64;
    let second = parse_fixed_digits(value, 17, 2)? as i64;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let millisecond = if bytes.get(19) == Some(&b'.') {
        let fraction_end = value.len().checked_sub(1)?;
        let fraction = value.get(20..fraction_end)?;
        if fraction.is_empty()
            || fraction.len() > 9
            || !fraction.bytes().all(|digit| digit.is_ascii_digit())
        {
            return None;
        }
        let mut padded = fraction.chars().take(3).collect::<String>();
        while padded.len() < 3 {
            padded.push('0');
        }
        padded.parse::<i64>().ok()?
    } else if bytes.get(19) == Some(&b'Z') {
        0
    } else {
        return None;
    };
    let days = days_from_civil(year, month, day)?;
    Some(
        days.checked_mul(86_400_000)?
            .checked_add(hour.checked_mul(3_600_000)?)?
            .checked_add(minute.checked_mul(60_000)?)?
            .checked_add(second.checked_mul(1_000)?)?
            .checked_add(millisecond)?,
    )
}

/// 解析固定宽度 ASCII 数字。
fn parse_fixed_digits(value: &str, start: usize, length: usize) -> Option<i64> {
    let slice = value.get(start..start.checked_add(length)?)?;
    if slice.bytes().all(|digit| digit.is_ascii_digit()) {
        slice.parse::<i64>().ok()
    } else {
        None
    }
}

/// 把公历日期转换为 Unix epoch 天数。
fn days_from_civil(year: i32, month: i32, day: i32) -> Option<i64> {
    let leap = month == 2 && (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
    let month_lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day > month_lengths.get((month - 1) as usize).copied()? {
        return None;
    }
    let year_adjusted = year - if month <= 2 { 1 } else { 0 };
    let era = if year_adjusted >= 0 {
        year_adjusted
    } else {
        year_adjusted - 399
    } / 400;
    let year_of_era = year_adjusted - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some((era * 146_097 + day_of_era - 719_468) as i64)
}

/// 从 JSONL 匹配集合和可选 UI thread 中选择唯一结果。
/// 流程：要求 JSONL 恰好一个候选；UI ID 只作辅助观测，滞后或仍指向旧页面时不覆盖本地会话文件证据。
/// 参数：candidates 为通过 cwd、prompt 和提交水位严格匹配的候选集合，ui_thread_id 为页面观测值；返回唯一 ID。
/// 异常/边界：零或多候选均 fail closed，禁止按最新更新时间猜测；唯一 JSONL 候选比 UI 活跃态更可靠。
fn select_unique_recovered_thread(
    candidates: &HashSet<String>,
    _ui_thread_id: Option<&str>,
) -> Result<String, String> {
    if candidates.len() != 1 {
        return Err(if candidates.is_empty() {
            "Codex Desktop 提交后没有匹配 thread。".to_string()
        } else {
            "Codex Desktop 提交后出现多个匹配 thread，已拒绝猜测。".to_string()
        });
    }
    let candidate = candidates.iter().next().cloned().unwrap_or_default();
    Ok(candidate)
}

/// session JSONL 中用于提交恢复的最小权威身份。
struct CodexSubmissionIdentity {
    /// session_meta 中的真实 thread ID。
    thread_id: String,
    /// session_meta 中的 canonical cwd，必须与任务快照精确相等。
    canonical_cwd: String,
    /// 提交水位后的第一条完整用户消息，不做截断或模糊匹配。
    first_user_message: String,
    /// 提交水位后第一条用户消息的 JSONL UTC 毫秒时间；用于支持复用已有 thread 后发送新消息。
    first_user_message_at_ms: i64,
}

/// 从单个有界 session JSONL 读取恢复身份。
/// 流程：复用普通文件句柄和单帧上限，读取 session_meta 的 id/cwd 与提交水位后的首个 event_msg.user_message；参数为受限枚举路径和提交水位；返回字段齐全的身份。
/// 异常/边界：超限、符号链接或读取错误显式失败；字段不全返回 None 等待持久化，不把标题、preview 或提交水位前的历史消息当作本次用户消息。
fn read_codex_submission_identity(
    path: &Path,
    submitted_at_ms: i64,
) -> Result<Option<CodexSubmissionIdentity>, String> {
    let file = open_bounded_codex_session_file(path, "提交恢复")?;
    let mut reader = BufReader::new(file.take(CODEX_SESSION_FILE_MAX_BYTES + 1));
    let mut thread_id = String::new();
    let mut canonical_cwd = String::new();
    let mut first_user_message: Option<(String, i64)> = None;
    while let Some(frame) = read_bounded_codex_session_frame(&mut reader)? {
        let Ok(value) = serde_json::from_slice::<Value>(&frame) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            if let Some(payload) = value.get("payload") {
                thread_id = payload
                    .get("id")
                    .or_else(|| payload.get("session_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                canonical_cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
        } else if first_user_message.is_none()
            && value.get("type").and_then(Value::as_str) == Some("event_msg")
            && value.pointer("/payload/type").and_then(Value::as_str) == Some("user_message")
        {
            let message_at_ms = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_codex_jsonl_timestamp_ms)
                .unwrap_or(0);
            if submitted_at_ms <= 0 || message_at_ms >= submitted_at_ms {
                first_user_message = value
                    .pointer("/payload/message")
                    .and_then(Value::as_str)
                    .map(|message| (message.to_string(), message_at_ms));
            }
        }
        if !thread_id.is_empty() && !canonical_cwd.is_empty() && first_user_message.is_some() {
            break;
        }
    }
    if reader.get_ref().limit() == 0 {
        return Err(
            "Codex 会话文件超过读取上限（错误码：CODEX_SESSION_FILE_TOO_LARGE）".to_string(),
        );
    }
    Ok(
        match (
            thread_id.is_empty(),
            canonical_cwd.is_empty(),
            first_user_message,
        ) {
            (false, false, Some((first_user_message, first_user_message_at_ms))) => {
                Some(CodexSubmissionIdentity {
                    thread_id,
                    canonical_cwd,
                    first_user_message,
                    first_user_message_at_ms,
                })
            }
            _ => None,
        },
    )
}

/// 持续运行 Codex 任务调度器。
/// 流程：先恢复并监控重启前 running 任务；每轮按 running 数量计算剩余并发槽位，再按排队顺序通过 CAS 领取 queued 任务并启动独立执行线程。
/// 参数：AppHandle 用于访问任务库、启动 Codex 和广播页面刷新；本方法运行至 App 进程退出，无返回值。
/// 异常/边界：数据库读取或 CAS 失败只记录诊断并重试；queued 不会被错误标失败；并发上限只限制提交/监控中的任务数，不重放 running prompt。
fn run_codex_task_dispatcher(app: &AppHandle) {
    reconcile_codex_tasks(app);
    loop {
        let running_count = match task_store::list_running_tasks(app) {
            Ok(tasks) => tasks.len(),
            Err(error) => {
                let _ =
                    record_desktop_task_error(app, "", "TASK_DISPATCH_RUNNING_LIST_FAILED", &error);
                thread::sleep(CODEX_TASK_DISPATCH_INTERVAL);
                continue;
            }
        };
        let max_running = read_codex_task_concurrency_limit(app);
        let available_slots = max_running.saturating_sub(running_count);
        if available_slots == 0 {
            thread::sleep(CODEX_TASK_DISPATCH_INTERVAL);
            continue;
        }
        let tasks = match task_store::list_queued_tasks(app) {
            Ok(tasks) => tasks,
            Err(error) => {
                let _ =
                    record_desktop_task_error(app, "", "TASK_DISPATCH_QUEUE_LIST_FAILED", &error);
                thread::sleep(CODEX_TASK_DISPATCH_INTERVAL);
                continue;
            }
        };
        if tasks.is_empty() {
            thread::sleep(CODEX_TASK_DISPATCH_INTERVAL);
            continue;
        }
        for task in tasks.into_iter().take(available_slots) {
            let worker_app = app.clone();
            thread::spawn(move || execute_dispatched_codex_task(&worker_app, task));
        }
        thread::sleep(CODEX_TASK_DISPATCH_INTERVAL);
    }
}

/// 执行调度器领取到的单个 queued 任务。
/// 流程：在 Codex 执行启动门禁内完成 DB 领取和 composer 提交，提交成功后由任务自身监控逻辑等待终态；参数为 AppHandle 和队列任务。
/// 返回：无返回值，所有失败都会写入任务诊断并广播刷新。
/// 异常/边界：CAS 未命中代表其它调度线程已领取；提交前失败会把任务置为 failed，提交不确定则由 execute_codex_task 标记 sendUncertain，避免重放 prompt。
fn execute_dispatched_codex_task(app: &AppHandle, task: task_store::QueuedTaskRecord) {
    let mut claimed_session_id = String::new();
    let result = codex_desktop::with_execution_start_gate(app, || {
        let session_id = task_store::mark_task_running(app, &task)?;
        claimed_session_id = session_id.clone();
        emit_session_task_updated(app, &task.id, &task.project_id);
        execute_codex_task(app, &task, &session_id)?;
        Ok(())
    });
    if let Err(error) = result {
        let diagnostic_code = task_execution_diagnostic_code(&error);
        let diagnostic_error = record_desktop_task_error(app, &task.id, diagnostic_code, &error);
        if !claimed_session_id.is_empty() {
            if let Err(persist_error) =
                task_store::mark_task_failed(app, &task.id, &claimed_session_id, &diagnostic_error)
            {
                let _ = record_desktop_task_error(
                    app,
                    &task.id,
                    "TASK_FAILURE_PERSIST_FAILED",
                    &persist_error,
                );
            } else {
                emit_session_task_updated(app, &task.id, &task.project_id);
            }
        }
    }
}

/// 读取系统设置中的 Codex 任务并发上限。
/// 流程：从客户端 JSON 的 settings 分区读取 `taskConcurrencyLimit`，只接受 1-10 的整数；参数为 AppHandle。
/// 返回：当前有效并发上限。
/// 异常/边界：配置文件不存在、损坏或字段非法时回落默认 3，调度器继续工作且不把坏配置写回覆盖现场。
fn read_codex_task_concurrency_limit(app: &AppHandle) -> usize {
    read_local_config_document(app)
        .ok()
        .and_then(|document| document.items.get(LOCAL_CONFIG_SETTINGS_KEY).cloned())
        .and_then(|settings| {
            settings
                .get("taskConcurrencyLimit")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
        })
        .filter(|value| {
            (CODEX_TASK_MIN_CONCURRENT_RUNNING..=CODEX_TASK_MAX_CONCURRENT_RUNNING).contains(value)
        })
        .unwrap_or(CODEX_TASK_DEFAULT_CONCURRENT_RUNNING)
}

/// 从任务执行失败中提取允许写入诊断日志的稳定错误码。
/// 流程：只识别内部 CDP 失败生成的完整固定标记；其它工作目录、数据库或未知错误统一收敛为任务执行失败。
/// 参数：error 为 `execute_codex_task` 返回的内部失败说明；返回桌面错误白名单中的稳定错误码。
/// 异常/边界：不做前缀、模糊或任意括号内容解析，避免未知正文伪造诊断元数据。
fn task_execution_diagnostic_code(error: &str) -> &'static str {
    for (marker, code) in [
        (
            "（错误码：CODEX_CDP_TARGET_CHECK_FAILED）",
            "CODEX_CDP_TARGET_CHECK_FAILED",
        ),
        ("（错误码：CODEX_NOT_CONNECTED）", "CODEX_NOT_CONNECTED"),
        (
            "（错误码：CODEX_CDP_INPUT_INVALID）",
            "CODEX_CDP_INPUT_INVALID",
        ),
        (
            "（错误码：CODEX_CDP_PROMPT_TOO_LARGE）",
            "CODEX_CDP_PROMPT_TOO_LARGE",
        ),
        (
            "（错误码：CODEX_CDP_WORKSPACE_SWITCH_FAILED）",
            "CODEX_CDP_WORKSPACE_SWITCH_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_NEW_CHAT_FAILED）",
            "CODEX_CDP_NEW_CHAT_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_COMPOSER_NOT_READY）",
            "CODEX_CDP_COMPOSER_NOT_READY",
        ),
        (
            "（错误码：CODEX_CDP_COMPOSER_WRITE_FAILED）",
            "CODEX_CDP_COMPOSER_WRITE_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_ATTACHMENT_INPUT_MISSING）",
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
        ),
        (
            "（错误码：CODEX_CDP_ATTACHMENT_INVALID）",
            "CODEX_CDP_ATTACHMENT_INVALID",
        ),
        (
            "（错误码：CODEX_CDP_ATTACHMENT_WRITE_FAILED）",
            "CODEX_CDP_ATTACHMENT_WRITE_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_SUBMISSION_PERSIST_FAILED）",
            "CODEX_CDP_SUBMISSION_PERSIST_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_CONNECT_FAILED）",
            "CODEX_CDP_CONNECT_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_PROTOCOL_FAILED）",
            "CODEX_CDP_PROTOCOL_FAILED",
        ),
        (
            "（错误码：CODEX_CDP_TARGET_INVALID）",
            "CODEX_CDP_TARGET_INVALID",
        ),
    ] {
        if error.contains(marker) {
            return code;
        }
    }
    "TASK_EXECUTION_FAILED"
}

/// App 重启时对账本地 running 任务与 Codex 持久化 turn 状态。
/// 流程：只读取重启前 running 任务，逐项启动监控线程并用 thread/read(includeTurns) 对账；queued 由唯一调度器领取。
/// 参数：AppHandle；返回无，后台执行；无 thread 时先检查持久化 CDP 提交阶段，Enter 后按水位恢复，Enter 前才可确定失败。
fn reconcile_codex_tasks(app: &AppHandle) {
    let preexisting_running = match task_store::list_running_tasks(app) {
        Ok(tasks) => tasks,
        Err(error) => {
            let _ = record_desktop_task_error(app, "", "RUNNING_RECOVERY_LIST_FAILED", &error);
            Vec::new()
        }
    };
    for running in preexisting_running {
        if running.thread_id.is_empty() {
            match task_store::load_pending_submission(app, &running.task_id, &running.session_id) {
                Ok(Some(pending)) => {
                    let recovered = recover_cdp_thread_from_jsonl(
                        &pending.workspace_path,
                        &pending.prompt,
                        pending.submitted_at_ms,
                        &pending.known_thread_ids.iter().cloned().collect(),
                        None,
                    );
                    match recovered.and_then(|thread_id| {
                        task_store::bind_task_thread(
                            app,
                            &pending.task_id,
                            &pending.session_id,
                            &thread_id,
                            &pending.client_user_message_id,
                        )?;
                        Ok(thread_id)
                    }) {
                        Ok(thread_id) => {
                            let monitor_app = app.clone();
                            let recovered_running = RunningTaskRecord {
                                task_id: pending.task_id,
                                project_id: pending.project_id,
                                session_id: pending.session_id,
                                thread_id,
                                turn_id: String::new(),
                            };
                            thread::spawn(move || {
                                monitor_reconciled_task(&monitor_app, recovered_running)
                            });
                        }
                        Err(error) => {
                            let diagnostic = record_desktop_task_error(
                                app,
                                &running.task_id,
                                "CODEX_SEND_UNCERTAIN",
                                &error,
                            );
                            if task_store::mark_task_send_uncertain(
                                app,
                                &running.task_id,
                                &running.session_id,
                                &diagnostic,
                            )
                            .is_ok()
                            {
                                emit_session_task_updated(
                                    app,
                                    &running.task_id,
                                    &running.project_id,
                                );
                            }
                        }
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = record_desktop_task_error(
                        app,
                        &running.task_id,
                        "CDP_SUBMISSION_RECOVERY_LOAD_FAILED",
                        &error,
                    );
                }
            }
            let error = "本地运行任务缺少 threadId，说明 prompt 尚未进入 turn/start；任务可确定失败且不会重放 prompt";
            let diagnostic_error = record_desktop_task_error(
                app,
                &running.task_id,
                "RECONCILE_IDENTIFIERS_MISSING",
                error,
            );
            if let Err(persist_error) = task_store::mark_task_failed(
                app,
                &running.task_id,
                &running.session_id,
                &diagnostic_error,
            ) {
                let _ = record_desktop_task_error(
                    app,
                    &running.task_id,
                    "RECONCILE_UNRECOVERABLE_PERSIST_FAILED",
                    &persist_error,
                );
            } else {
                emit_session_task_updated(app, &running.task_id, &running.project_id);
            }
            continue;
        }
        let monitor_app = app.clone();
        thread::spawn(move || monitor_reconciled_task(&monitor_app, running));
    }
}

/// 持续轮询 running 任务，直到 thread/read 返回可靠 turn 终态，或确认 turn/start 未产生任何持久化 turn。
/// 流程：按 threadId 读取持久化 turns；已知 turnId 精确匹配，缺失 turnId 时仅允许绑定任务专用 thread 中的唯一 turn；连续空读取经过一致性窗口后安全失败。
/// 参数：AppHandle 与运行快照；返回无；App 退出时进程结束，轮询不持有外部子进程。
/// 异常/边界：已存在或可能存在 turn 时，通道、协议、超时、not-found 和歧义都不能据此标记 failed；只有专用 thread 连续确认无 turn 才可释放 worker，本方法绝不重放 prompt。
fn monitor_reconciled_task(app: &AppHandle, mut running: RunningTaskRecord) {
    let mut delay = CODEX_RECONCILE_INITIAL_DELAY;
    let mut empty_thread_confirmations = 0_usize;
    loop {
        let outcome = reconcile_codex_task(app, &mut running);
        match outcome {
            Ok(Some((status, result, error))) => {
                if let Err(persist_error) =
                    task_store::finish_task_execution(app, &running, &status, &result, &error)
                {
                    let _ = record_desktop_task_error(
                        app,
                        &running.task_id,
                        "RECONCILE_TERMINAL_PERSIST_FAILED",
                        &persist_error,
                    );
                } else {
                    emit_session_task_updated(app, &running.task_id, &running.project_id);
                    return;
                }
                delay = (delay * 2).min(CODEX_RECONCILE_MAX_DELAY);
            }
            Ok(None) => {
                let is_empty_dedicated_thread = running.turn_id.is_empty();
                if !is_empty_dedicated_thread {
                    empty_thread_confirmations = 0;
                    delay = CODEX_RECONCILE_ACTIVE_DELAY;
                    thread::sleep(delay);
                    continue;
                }
                let (next_confirmations, is_confirmed_absent) = advance_empty_thread_confirmation(
                    empty_thread_confirmations,
                    is_empty_dedicated_thread,
                );
                empty_thread_confirmations = next_confirmations;
                if is_confirmed_absent {
                    let diagnostic_error = record_desktop_task_error(
                        app,
                        &running.task_id,
                        "RECONCILE_EMPTY_THREAD_CONFIRMED",
                        "Codex 任务专用 thread 连续五次只读确认无 turn，判定 turn/start 未产生持久化执行；不会重放 prompt",
                    );
                    if let Err(persist_error) = task_store::mark_task_failed(
                        app,
                        &running.task_id,
                        &running.session_id,
                        &diagnostic_error,
                    ) {
                        let _ = record_desktop_task_error(
                            app,
                            &running.task_id,
                            "RECONCILE_EMPTY_THREAD_PERSIST_FAILED",
                            &persist_error,
                        );
                        delay = CODEX_RECONCILE_MAX_DELAY;
                    } else {
                        emit_session_task_updated(app, &running.task_id, &running.project_id);
                        return;
                    }
                } else if is_empty_dedicated_thread && empty_thread_confirmations > 1 {
                    delay = (delay * 2).min(CODEX_RECONCILE_MAX_DELAY);
                } else {
                    delay = CODEX_RECONCILE_INITIAL_DELAY;
                }
            }
            Err(error) => {
                empty_thread_confirmations = 0;
                let code = if error == CODEX_AMBIGUOUS_THREAD_TURNS_ERROR {
                    "RECONCILE_AMBIGUOUS_TURNS"
                } else {
                    "RECONCILE_RETRY"
                };
                let _ = record_desktop_task_error(app, &running.task_id, code, &error);
                delay = (delay * 2).min(CODEX_RECONCILE_MAX_DELAY);
            }
        }
        thread::sleep(delay);
    }
}

/// 推进专用 thread 的连续空读取确认状态。
/// 流程：本次成功读取仍为空时计数加一，否则清零；参数为旧计数和本次是否为空；返回新计数与是否达到一致性窗口。
/// 异常/边界：使用饱和加法避免理论上的长时间运行溢出；任何 inProgress、终态或读取错误都必须由调用方传入 false 以打断连续确认。
fn advance_empty_thread_confirmation(current: usize, is_confirmed_empty: bool) -> (usize, bool) {
    let next = if is_confirmed_empty {
        current.saturating_add(1)
    } else {
        0
    };
    (next, next >= CODEX_EMPTY_THREAD_CONFIRMATIONS)
}

/// 查询单个运行中任务的 Codex 持久化 turn 状态。
/// 流程：启动短生命周期 app-server，按 threadId 读取 turns；缺失 turnId 时从任务专用 thread 的唯一 turn 恢复并原子绑定。
/// 参数：app 用于提交恢复出的 turnId，running 为可更新的运行快照；返回可选可靠终态。
/// 异常/边界：零 turn 或 inProgress 返回 None；多 turn 歧义、对象不存在或协议畸形返回可重试错误，不创建 turn、不重放 prompt。
fn reconcile_codex_task(
    app: &AppHandle,
    running: &mut RunningTaskRecord,
) -> Result<Option<(String, String, String)>, String> {
    if running.thread_id.is_empty() {
        return Err("本地运行任务缺少 threadId，无法只读对账".to_string());
    }
    let mut session = CodexAppServerSession::start()?;
    session.initialize()?;
    let response = session.request(
        2,
        "thread/read",
        json!({"threadId": running.thread_id, "includeTurns": true}),
    )?;
    let turns = response
        .pointer("/result/thread/turns")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "thread/read 响应缺少 turns".to_string())?;
    let Some(turn) = select_codex_turn_for_reconciliation(&turns, &running.turn_id)? else {
        return Ok(None);
    };
    if running.turn_id.is_empty() {
        let recovered_turn_id = turn
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Codex 专用 thread 的唯一 turn 缺少 id".to_string())?
            .to_string();
        task_store::bind_task_execution(
            app,
            &running.task_id,
            &running.session_id,
            &running.thread_id,
            &recovered_turn_id,
        )?;
        running.turn_id = recovered_turn_id;
    }
    if turn.get("status").and_then(Value::as_str) == Some("inProgress") {
        return Ok(None);
    }
    let (status, result, error) = parse_codex_terminal_turn(&turn)?;
    match resolve_interrupted_turn_from_session(&status, &result, running)? {
        InterruptedTurnResolution::Recovered(recovered_result) => {
            Ok(Some((status, recovered_result, error)))
        }
        InterruptedTurnResolution::Aborted => Ok(Some((status, result, error))),
        InterruptedTurnResolution::Pending => Ok(None),
        InterruptedTurnResolution::Unchanged => Ok(Some((status, result, error))),
    }
}

/// 从 thread/read 返回值中选择当前任务可证明归属的 turn。
/// 流程：已有 turnId 时精确匹配；缺失 turnId 时依赖“每个任务新建专用 thread 且最多发送一次 turn/start”的不变量，只接受唯一 turn。
/// 参数：turns 为 thread/read(includeTurns=true) 的完整持久化历史，turn_id 为本地已绑定标识或空值；返回可选 turn 副本。
/// 异常/边界：零 turn 表示尚无可恢复对象；多 turn 无法证明归属，必须报错并由调用方继续只读重试，禁止猜测或重放 prompt。
fn select_codex_turn_for_reconciliation(
    turns: &[Value],
    turn_id: &str,
) -> Result<Option<Value>, String> {
    if turn_id.is_empty() {
        return match turns {
            [] => Ok(None),
            [turn] => Ok(Some(turn.clone())),
            _ => Err(CODEX_AMBIGUOUS_THREAD_TURNS_ERROR.to_string()),
        };
    }
    turns
        .iter()
        .find(|turn| turn.get("id").and_then(Value::as_str) == Some(turn_id))
        .cloned()
        .map(Some)
        .ok_or_else(|| "Codex thread 中未找到已绑定 turn".to_string())
}

/// 解析 schema 定义的 Codex Turn 可靠终态并生成精简持久化结果。
fn parse_codex_terminal_turn(turn: &Value) -> Result<(String, String, String), String> {
    let status = turn
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "Codex turn 缺少 status".to_string())?;
    if !matches!(status, "completed" | "failed" | "interrupted") {
        return Err(format!("Codex turn 尚未进入可靠终态：{}", status));
    }
    let final_text = turn
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().rev().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("agentMessage"))
                    .then(|| item.get("text").and_then(Value::as_str))
                    .flatten()
            })
        })
        .map(|text| limit_chars(text, CODEX_TASK_RESULT_TEXT_MAX_CHARS))
        .unwrap_or_default();
    let error = turn
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(|message| limit_chars(message, 1_000))
        .unwrap_or_default();
    let result = json!({
        "turnId": turn.get("id").and_then(Value::as_str).unwrap_or(""),
        "status": status,
        "completedAt": turn.get("completedAt").cloned().unwrap_or(Value::Null),
        "finalText": final_text
    })
    .to_string();
    Ok((status.to_string(), result, error))
}

/// 当 app-server 只返回 interrupted 且 items 缺少最终文本时，从 Codex JSONL 的同 turn task_complete 事件恢复回复。
/// 流程：先确认终态结果没有 finalText，再按 threadId 精确定位本地 session 文件，并只接受 turnId 完全一致的 last_agent_message。
/// 参数：status/result_json 来自 thread/read，running 提供当前任务绑定的 thread/turn；返回可落库结果 JSON。
/// 异常/边界：找不到会话文件、没有 task_complete 或消息为空时保留原结果，后续仍按 interrupted 失败处理；不会读取其它 turn 的回复。
fn resolve_interrupted_turn_from_session(
    status: &str,
    result_json: &str,
    running: &RunningTaskRecord,
) -> Result<InterruptedTurnResolution, String> {
    if status != "interrupted" {
        return Ok(InterruptedTurnResolution::Unchanged);
    }
    let Ok(session_path) = find_codex_session_file(&running.thread_id) else {
        return Ok(InterruptedTurnResolution::Pending);
    };
    match read_codex_turn_completion_state(&session_path, &running.turn_id)? {
        CodexTurnCompletionState::Completed(message) if !message.trim().is_empty() => {
            Ok(InterruptedTurnResolution::Recovered(
                with_codex_terminal_result_final_text(result_json, &message)?,
            ))
        }
        CodexTurnCompletionState::Aborted => Ok(InterruptedTurnResolution::Aborted),
        CodexTurnCompletionState::Completed(_) | CodexTurnCompletionState::Pending => {
            Ok(InterruptedTurnResolution::Pending)
        }
    }
}

/// interrupted turn 的本地 JSONL 对账结果。
enum InterruptedTurnResolution {
    /// 已从 task_complete 补回最终回复。
    Recovered(String),
    /// 已确认用户或系统中止，可按 interrupted 失败落库。
    Aborted,
    /// JSONL 还没有写入 task_complete/turn_aborted，继续轮询。
    Pending,
    /// 非需要兜底的状态，沿用 thread/read 结果。
    Unchanged,
}

/// 指定 turn 在 Codex JSONL 中的完成状态。
enum CodexTurnCompletionState {
    /// 同 turn 已写入 task_complete，并携带最终助手消息。
    Completed(String),
    /// 同 turn 已写入 turn_aborted，说明不会再产出可验收结果。
    Aborted,
    /// 尚未观察到 task_complete 或 turn_aborted，调用方应继续等待。
    Pending,
}

/// 返回替换 finalText 后的 Codex 终态结果 JSON，保持其它字段不变。
fn with_codex_terminal_result_final_text(
    result_json: &str,
    final_text: &str,
) -> Result<String, String> {
    let mut value = serde_json::from_str::<Value>(result_json)
        .map_err(|_| "Codex turn 结果不是有效 JSON".to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Codex turn 结果不是对象 JSON".to_string())?;
    object.insert(
        "finalText".to_string(),
        Value::String(limit_chars(final_text, CODEX_TASK_RESULT_TEXT_MAX_CHARS)),
    );
    object.insert(
        "finalTextSource".to_string(),
        Value::String("sessionTaskComplete".to_string()),
    );
    Ok(value.to_string())
}

/// 通过统一桌面错误入口记录任务核心故障并返回可排障文案。
/// 流程：把稳定错误码、操作名和可选业务 ID 写入 desktop-errors.log；白名单业务契约错误保留 TaskStore 原始稳定码，其它错误只返回脱敏诊断文案。
/// 参数：app、code、operation、context_id 和 error 描述失败上下文；返回给私有 UDS RPC 的安全错误。
/// 异常/边界：调用方不得传入 prompt 或结果正文；只有 TaskStore 明确登记的固定错误文案可透传，数据库和进程详情始终隐藏。
fn record_task_ipc_error(
    app: &AppHandle,
    code: &str,
    operation: &str,
    context_id: Option<&str>,
    error: &str,
) -> String {
    let diagnostic = desktop_error::record_desktop_error(app, code, operation, context_id, error);
    if task_store::is_public_task_contract_error(error) {
        error.to_string()
    } else {
        diagnostic
    }
}

/// 通过统一桌面错误入口记录后台任务故障并广播诊断事件。
/// 流程：生成稳定错误码和唯一诊断 ID，写入统一轮转日志，再向任务页面广播安全用户文案。
/// 参数：app、task_id、code 和 error 标识失败任务及原因；返回带错误码与诊断 ID 的安全文案，供状态持久化复用。
/// 异常/边界：统一入口负责截断和脱敏；日志写入失败不影响事件广播，不记录 prompt、结果正文或密钥。
fn record_desktop_task_error(app: &AppHandle, task_id: &str, code: &str, error: &str) -> String {
    let safe_task_id = limit_chars(task_id, 100).replace(['\n', '\r', '\t'], " ");
    let safe_code = limit_chars(code, 80).replace(['\n', '\r', '\t'], " ");
    let safe_error = desktop_error::record_desktop_error(
        app,
        &safe_code,
        "codex_task",
        Some(&safe_task_id),
        &trim_error_message(error),
    );
    let display_error =
        append_cdp_protocol_stage(&safe_code, &safe_error, error).unwrap_or(safe_error);
    let _ = app.emit(
        "session-task-reconcile-error",
        json!({"taskId": safe_task_id, "code": safe_code, "message": display_error.clone()}),
    );
    display_error
}

/// 为 CDP 协议失败补充固定阶段名，便于本机开发排障。
/// 流程：只处理内部固定错误码和 ``阶段：`` 标记，从错误文本中提取 ASCII 阶段名并裁剪；其它错误保持统一诊断文案。
/// 参数：``code`` 为稳定错误码，``safe_error`` 为统一桌面错误文案，``raw_error`` 为已由调用方构造的安全错误。
/// 返回：带阶段名的展示错误；无法安全提取时返回 None。
/// 异常/边界：不会回显 prompt、DOM、路径或 CDP 原始响应。
fn append_cdp_protocol_stage(code: &str, safe_error: &str, raw_error: &str) -> Option<String> {
    if code != "CODEX_CDP_PROTOCOL_FAILED" {
        return None;
    }
    let stage_start = raw_error.find("阶段：")? + "阶段：".len();
    let stage_end = raw_error[stage_start..]
        .find(['）', ')', '。'])
        .map(|offset| stage_start + offset)
        .unwrap_or(raw_error.len());
    let stage = raw_error[stage_start..stage_end].trim();
    if stage.is_empty()
        || stage.len() > 80
        || !stage
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == ' ')
    {
        return None;
    }
    Some(format!("{}（阶段：{}）", safe_error, stage))
}

/// 广播任务真实状态已提交事件，并携带任务、项目、当前会话数据库快照供前端增量替换。
/// 流程：按 taskId/projectId 读取有限增量字段，成功时把快照随事件发送；读取失败时记录脱敏诊断并发送 null 快照。
/// 参数：app 用于读取任务库与发事件，task_id/project_id 标识变更任务；返回无。
/// 异常/边界：事件不携带未落库的推测状态；单条读取失败不回退全量聚合，也不阻塞后台调度器。
fn emit_session_task_updated(app: &AppHandle, task_id: &str, project_id: &str) {
    let snapshot = match task_store::load_task_update_snapshot(app, task_id, project_id) {
        Ok(snapshot) => Some(snapshot),
        Err(error) => {
            let _ = record_desktop_task_error(app, task_id, "TASK_INCREMENTAL_READ_FAILED", &error);
            None
        }
    };
    let _ = app.emit(
        "session-task-updated",
        json!({"taskId": task_id, "projectId": project_id, "snapshot": snapshot}),
    );
}

impl Drop for CodexAppServerSession {
    /// 结束短连接时终止 app-server 子进程，避免后台残留。
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 读取 Codex 会话索引并合并最近会话文件，避免 exec 新建会话尚未写入索引时列表缺项。
fn read_codex_thread_index() -> Result<Vec<CodexThreadSummary>, String> {
    let threads = read_codex_session_index_file()?;
    Ok(merge_codex_thread_summaries(
        threads,
        read_recent_codex_session_summaries(),
    ))
}

/// 合并官方 index 与可选 supplemental 扫描结果，并始终以合法 index 为可用基线。
/// 流程：supplemental 成功时按 ID 去重追加，失败时视为空补充；随后按更新时间倒序并截断列表上限。
/// 参数：threads 为已合法读取的官方 index，supplemental 为受限目录扫描结果；返回最终会话摘要。
/// 异常/边界：文件数、总字节、目录权限或候选解析错误不得使合法 index 整次失败；supplemental 永远不能覆盖同 ID 的官方记录。
fn merge_codex_thread_summaries(
    mut threads: Vec<CodexThreadSummary>,
    supplemental: Result<Vec<CodexThreadSummary>, String>,
) -> Vec<CodexThreadSummary> {
    let mut seen_ids = threads
        .iter()
        .map(|thread| (thread.id.clone(), true))
        .collect::<HashMap<_, _>>();
    if let Ok(summaries) = supplemental {
        for thread in summaries {
            if seen_ids.contains_key(&thread.id) {
                continue;
            }
            seen_ids.insert(thread.id.clone(), true);
            threads.push(thread);
        }
    }
    threads.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    threads.dedup_by(|left, right| left.id == right.id);
    threads.into_iter().take(CODEX_THREAD_LIST_LIMIT).collect()
}

/// 读取 Codex 官方会话索引文件，作为列表标题和排序的主要来源。
fn read_codex_session_index_file() -> Result<Vec<CodexThreadSummary>, String> {
    let path = codex_home_dir()?.join("session_index.jsonl");
    read_codex_session_index_path(&path)
}

/// 从指定路径读取有界 Codex 会话索引，隔离真实 CODEX_HOME 定位与文件解析边界。
/// 流程：先拒绝路径上的非普通对象，再打开句柄并基于句柄元数据复核类型，最后通过 take(上限+1) 读取和判断真实字节数后解析 JSONL。
/// 参数：path 为官方索引路径；返回不存在时为空、合法时为最新 500 条摘要。
/// 异常/边界：校验后文件增长也只能读取上限加一字节；超大、非普通文件、无效 UTF-8 和读取失败均拒绝且不回显绝对路径。
fn read_codex_session_index_path(path: &Path) -> Result<Vec<CodexThreadSummary>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(
                "Codex 会话索引不是普通文件（错误码：CODEX_SESSION_INDEX_INVALID）".to_string(),
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "读取 Codex 会话索引元数据失败：{}",
                trim_error_message(&error.to_string())
            ));
        }
    }
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "打开 Codex 会话索引失败：{}",
            trim_error_message(&error.to_string())
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        format!(
            "读取 Codex 会话索引句柄元数据失败：{}",
            trim_error_message(&error.to_string())
        )
    })?;
    if !metadata.is_file() {
        return Err(
            "Codex 会话索引不是普通文件（错误码：CODEX_SESSION_INDEX_INVALID）".to_string(),
        );
    }
    let content = read_bounded_codex_file(
        file,
        CODEX_SESSION_INDEX_MAX_BYTES,
        "CODEX_SESSION_INDEX_TOO_LARGE",
        "Codex 会话索引",
    )?;
    let content = String::from_utf8(content).map_err(|_| {
        "Codex 会话索引不是有效 UTF-8（错误码：CODEX_SESSION_INDEX_INVALID）".to_string()
    })?;
    let mut threads = Vec::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let title = value
            .get("thread_name")
            .and_then(Value::as_str)
            .unwrap_or("未命名会话")
            .trim();
        let updated_at = value
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let summary = CodexThreadSummary {
            id: id.to_string(),
            title: if title.is_empty() {
                "未命名会话".to_string()
            } else {
                title.to_string()
            },
            parent_thread_id: String::new(),
            depth: 0,
            agent_nickname: String::new(),
            agent_role: String::new(),
            updated_at: updated_at.to_string(),
        };
        if threads.len() < CODEX_SESSION_INDEX_ENTRY_LIMIT {
            threads.push(summary);
        } else if let Some((oldest_index, _)) = threads
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| left.updated_at.cmp(&right.updated_at))
        {
            if summary.updated_at > threads[oldest_index].updated_at {
                threads[oldest_index] = summary;
            }
        }
    }
    Ok(threads)
}

/// 从已打开文件句柄读取严格有界的字节内容。
/// 流程：在句柄上使用 take(上限+1) 累计读取，读取结束后按实际字节数判断是否超限。
/// 参数：file 为已打开句柄，max_bytes 为最大允许字节数，too_large_code/label 为固定安全诊断信息；返回有界字节。
/// 异常/边界：不信任打开前路径元数据，文件在校验后增长也最多读取上限加一字节；超限或读取错误不返回部分内容。
fn read_bounded_codex_file(
    file: fs::File,
    max_bytes: u64,
    too_large_code: &str,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut content = Vec::with_capacity((max_bytes as usize).min(8 * 1024));
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut content)
        .map_err(|_| {
            format!(
                "读取{}失败（错误码：CODEX_SESSION_FILE_READ_FAILED）",
                label
            )
        })?;
    if content.len() as u64 > max_bytes {
        return Err(format!(
            "{}超过 {} 字节，已拒绝加载（错误码：{}）",
            label, max_bytes, too_large_code
        ));
    }
    Ok(content)
}

/// 扫描最近修改的 Codex session 文件，补齐 CLI exec 新建但暂未进入 session_index 的会话。
fn read_recent_codex_session_summaries() -> Result<Vec<CodexThreadSummary>, String> {
    let sessions_dir = codex_home_dir()?.join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = collect_codex_session_files(&sessions_dir)?;
    files.sort_by_key(|item| std::cmp::Reverse(item.1));
    let mut summaries = Vec::new();
    for (path, _) in files.into_iter().take(CODEX_SESSION_SCAN_LIMIT) {
        if let Ok(Some(summary)) = read_codex_session_summary(&path) {
            summaries.push(summary);
        }
    }
    Ok(summaries)
}

/// 递归收集 Codex session JSONL 文件及修改时间，用于按最近活跃度补漏。
/// 流程：遍历非符号链接目录，只累计普通 JSONL；单文件超限或元数据失败跳过，候选过多时滚动保留最近修改文件，最后再应用文件数和总字节预算。
/// 参数：dir 为 sessions 根目录；返回预算内候选及修改时间。
/// 异常/边界：补充扫描的候选限制不返回整批失败，避免拖垮官方 index；不会因目录枚举顺序把最新任务 JSONL 挤出候选。
fn collect_codex_session_files(dir: &Path) -> Result<Vec<(PathBuf, u128)>, String> {
    let mut stack = vec![dir.to_path_buf()];
    let mut candidates = Vec::new();
    let mut visited_directories = 0usize;
    while let Some(current_dir) = stack.pop() {
        visited_directories = visited_directories.saturating_add(1);
        if visited_directories > CODEX_SESSION_DIRECTORY_LIMIT {
            return Err(format!(
                "Codex 会话目录超过 {} 个，已停止扫描（错误码：CODEX_SESSION_DIRECTORY_LIMIT_EXCEEDED）",
                CODEX_SESSION_DIRECTORY_LIMIT
            ));
        }
        let entries = fs::read_dir(&current_dir).map_err(|error| {
            format!(
                "读取 Codex 会话目录失败：{}",
                trim_error_message(&error.to_string())
            )
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("jsonl")
            {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > CODEX_SESSION_FILE_MAX_BYTES {
                continue;
            }
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis())
                .unwrap_or(0);
            candidates.push((path, modified_ms, metadata.len()));
            if candidates.len() > CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT {
                candidates.sort_by(|left, right| right.1.cmp(&left.1));
                candidates.truncate(CODEX_SESSION_FILE_ENUM_LIMIT);
            }
        }
    }
    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    for (path, modified_ms, size) in candidates {
        if files.len() >= CODEX_SESSION_FILE_ENUM_LIMIT {
            break;
        }
        let Some(next_total_bytes) = total_bytes.checked_add(size) else {
            break;
        };
        if next_total_bytes > CODEX_SESSION_TOTAL_BYTES_LIMIT {
            break;
        }
        total_bytes = next_total_bytes;
        files.push((path, modified_ms));
    }
    Ok(files)
}

/// 从单个 session JSONL 文件中读取会话 ID、标题和最近更新时间。
fn read_codex_session_summary(path: &Path) -> Result<Option<CodexThreadSummary>, String> {
    let file = open_bounded_codex_session_file(path, "摘要")?;
    let mut reader = BufReader::new(file.take(CODEX_SESSION_FILE_MAX_BYTES + 1));
    let mut id = String::new();
    let mut title = String::new();
    let mut parent_thread_id = String::new();
    let mut depth = 0i64;
    let mut agent_nickname = String::new();
    let mut agent_role = String::new();
    let mut updated_at = file_modified_timestamp(path);
    for _ in 0..CODEX_SESSION_SUMMARY_MAX_LINES {
        let Some(frame) = read_bounded_codex_session_frame(&mut reader)? else {
            break;
        };
        if frame.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&frame) else {
            continue;
        };
        let timestamp = value.get("timestamp").and_then(Value::as_str).unwrap_or("");
        if !timestamp.is_empty() {
            updated_at = timestamp.to_string();
        }
        if id.is_empty() {
            id = value
                .get("payload")
                .and_then(|payload| payload.get("id").or_else(|| payload.get("session_id")))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            if let Some(payload) = value.get("payload") {
                parent_thread_id = payload
                    .get("parent_thread_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                depth = payload
                    .get("source")
                    .and_then(|source| source.get("subagent"))
                    .and_then(|subagent| subagent.get("thread_spawn"))
                    .and_then(|thread_spawn| thread_spawn.get("depth"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                agent_nickname = payload
                    .get("agent_nickname")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                agent_role = payload
                    .get("agent_role")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
        }
        if title.is_empty() {
            title = extract_codex_summary_title(&value);
        }
        if !id.is_empty() && !title.is_empty() && !updated_at.is_empty() {
            break;
        }
    }
    if reader.get_ref().limit() == 0 {
        return Err(format!(
            "Codex 会话文件超过 {} 字节，已拒绝加载（错误码：CODEX_SESSION_FILE_TOO_LARGE）",
            CODEX_SESSION_FILE_MAX_BYTES
        ));
    }
    if id.is_empty() {
        return Ok(None);
    }
    Ok(Some(CodexThreadSummary {
        id,
        title: if title.is_empty() {
            "未命名会话".to_string()
        } else {
            title
        },
        parent_thread_id,
        depth,
        agent_nickname,
        agent_role,
        updated_at,
    }))
}

/// 读取文件修改时间作为会话排序兜底值，避免为摘要扫描完整大文件。
fn file_modified_timestamp(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if let Some(timestamp) = extract_codex_timestamp_from_file_name(file_name) {
        return timestamp;
    }
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_default()
}

/// 从 Codex rollout 文件名里还原可被前端解析的时间字符串。
fn extract_codex_timestamp_from_file_name(file_name: &str) -> Option<String> {
    let start = file_name.find("rollout-")? + "rollout-".len();
    let value = file_name.get(start..start + 19)?;
    let date = value.get(0..10)?;
    let time = value.get(11..19)?.replace('-', ":");
    Some(format!("{}T{}", date, time))
}

/// 用首条用户消息生成 exec 会话标题，避免新会话在列表里只能显示未命名。
fn extract_codex_summary_title(value: &Value) -> String {
    if value.get("type").and_then(Value::as_str) == Some("event_msg")
        && value
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            == Some("user_message")
    {
        return limit_chars(
            value
                .get("payload")
                .and_then(|payload| payload.get("message"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            36,
        );
    }
    if value.get("type").and_then(Value::as_str) == Some("response_item")
        && value
            .get("payload")
            .and_then(|payload| payload.get("role"))
            .and_then(Value::as_str)
            == Some("user")
    {
        let content = value
            .get("payload")
            .and_then(|payload| payload.get("content"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        return limit_chars(&content, 36);
    }
    String::new()
}

/// 在 Codex sessions 目录下按会话 ID 精确定位 JSONL 文件。
/// 流程：使用独立目录/条目预算递归匹配文件名，不复用 supplemental 的前 500 文件和总字节候选列表。
/// 参数：thread_id 为已校验会话 ID；返回通过普通文件、大小和打开句柄复核的目标路径。
/// 异常/边界：目标可以位于第 501 个 JSONL 之后；目录或条目预算超限、匹配符号链接/非文件、句柄异常或目标缺失均 fail closed。
fn find_codex_session_file(thread_id: &str) -> Result<PathBuf, String> {
    let sessions_dir = codex_home_dir()?.join("sessions");
    find_codex_session_file_in_dir(
        &sessions_dir,
        thread_id,
        CODEX_SESSION_DIRECTORY_LIMIT,
        CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT,
    )
}

/// 在指定 sessions 根目录执行有预算的精确 thread ID 查找。
/// 流程：预计算 `thread_id.jsonl` 与 `-thread_id.jsonl` 文件名边界，深度遍历非符号链接目录并累计所有目录条目，精确命中后立即打开并复用 session 句柄校验。
/// 参数：sessions_dir/thread_id 为根目录与目标，directory_limit/entry_limit 为生产预算及测试边界；返回合法目标路径。
/// 异常/边界：`abc` 不得命中 `abc-extra`；不按无关文件字节累计提前截断；任一预算必须大于零，超过目录或条目限制返回稳定错误码，匹配到符号链接或非普通文件明确拒绝。
fn find_codex_session_file_in_dir(
    sessions_dir: &Path,
    thread_id: &str,
    directory_limit: usize,
    entry_limit: usize,
) -> Result<PathBuf, String> {
    if directory_limit == 0 {
        return Err(
            "Codex 会话精确查找目录预算为零（错误码：CODEX_SESSION_DIRECTORY_LIMIT_EXCEEDED）"
                .to_string(),
        );
    }
    if entry_limit == 0 {
        return Err(
            "Codex 会话精确查找条目预算为零（错误码：CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT_EXCEEDED）"
                .to_string(),
        );
    }
    let exact_file_name = format!("{}.jsonl", thread_id);
    let rollout_file_suffix = format!("-{}.jsonl", thread_id);
    let mut stack = vec![sessions_dir.to_path_buf()];
    let mut visited_directories = 0usize;
    let mut visited_entries = 0usize;
    while let Some(current_dir) = stack.pop() {
        visited_directories = visited_directories.saturating_add(1);
        if visited_directories > directory_limit {
            return Err(format!(
                "Codex 会话精确查找目录超过 {} 个（错误码：CODEX_SESSION_DIRECTORY_LIMIT_EXCEEDED）",
                directory_limit
            ));
        }
        let entries = fs::read_dir(&current_dir).map_err(|error| {
            format!(
                "读取 Codex 会话精确查找目录失败：{}",
                trim_error_message(&error.to_string())
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "读取 Codex 会话精确查找条目失败：{}",
                    trim_error_message(&error.to_string())
                )
            })?;
            visited_entries = visited_entries.saturating_add(1);
            if visited_entries > entry_limit {
                return Err(format!(
                    "Codex 会话精确查找条目超过 {} 个（错误码：CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT_EXCEEDED）",
                    entry_limit
                ));
            }
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let matches_thread =
                file_name == exact_file_name || file_name.ends_with(&rollout_file_suffix);
            let file_type = entry.file_type().map_err(|error| {
                format!(
                    "读取 Codex 会话精确查找条目类型失败：{}",
                    trim_error_message(&error.to_string())
                )
            })?;
            if file_type.is_symlink() {
                if matches_thread {
                    return Err(
                        "Codex 会话目标不能是符号链接（错误码：CODEX_SESSION_FILE_INVALID）"
                            .to_string(),
                    );
                }
                continue;
            }
            if file_type.is_dir() {
                if matches_thread {
                    return Err(
                        "Codex 会话目标不是普通文件（错误码：CODEX_SESSION_FILE_INVALID）"
                            .to_string(),
                    );
                }
                stack.push(path);
                continue;
            }
            if !matches_thread || !file_type.is_file() {
                continue;
            }
            drop(open_bounded_codex_session_file(&path, "精确定位")?);
            return Ok(path);
        }
    }
    Err(format!("未找到 Codex 会话文件：{}", thread_id))
}

/// 从 Codex 会话 JSONL 中抽取最近用户消息和助手消息。
fn read_codex_session_messages(path: &Path) -> Result<Vec<CodexThreadMessage>, String> {
    let file = open_bounded_codex_session_file(path, "详情")?;
    read_codex_session_messages_from_file(file)
}

/// 从 Codex session JSONL 中读取指定 turn 的 task_complete/turn_aborted 状态。
/// 流程：复用受限 session 文件打开与逐帧读取预算，只匹配 turn_id 完全一致的完成或中止事件。
/// 参数：path 为精确 thread 定位出的会话文件，turn_id 为当前任务绑定的 turn；返回当前 JSONL 已确认的完成状态。
/// 异常/边界：turn_id 为空或文件增长超限时 fail closed；找不到匹配事件返回 Pending，不猜测其它 assistant 消息。
fn read_codex_turn_completion_state(
    path: &Path,
    turn_id: &str,
) -> Result<CodexTurnCompletionState, String> {
    if turn_id.trim().is_empty() {
        return Ok(CodexTurnCompletionState::Pending);
    }
    let file = open_bounded_codex_session_file(path, "任务完成事件")?;
    read_codex_turn_completion_state_from_file(file, turn_id)
}

/// 从已打开 session 句柄中解析指定 turn 的完成状态。
fn read_codex_turn_completion_state_from_file(
    file: fs::File,
    turn_id: &str,
) -> Result<CodexTurnCompletionState, String> {
    let mut reader = BufReader::new(file.take(CODEX_SESSION_FILE_MAX_BYTES + 1));
    let mut state = CodexTurnCompletionState::Pending;
    while let Some(frame) = read_bounded_codex_session_frame(&mut reader)? {
        if frame.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&frame) else {
            continue;
        };
        let Some(payload) = value.get("payload") else {
            continue;
        };
        let payload_type = payload.get("type").and_then(Value::as_str);
        if !matches!(payload_type, Some("task_complete" | "turn_aborted")) {
            continue;
        }
        if payload.get("turn_id").and_then(Value::as_str) != Some(turn_id) {
            continue;
        }
        state = if payload_type == Some("task_complete") {
            let message = payload
                .get("last_agent_message")
                .and_then(Value::as_str)
                .map(|text| limit_chars(text, CODEX_TASK_RESULT_TEXT_MAX_CHARS))
                .unwrap_or_default();
            CodexTurnCompletionState::Completed(message)
        } else {
            CodexTurnCompletionState::Aborted
        };
    }
    if reader.get_ref().limit() == 0 {
        return Err(format!(
            "Codex 会话文件超过 {} 字节，已拒绝加载（错误码：CODEX_SESSION_FILE_TOO_LARGE）",
            CODEX_SESSION_FILE_MAX_BYTES
        ));
    }
    Ok(state)
}

/// 从已打开 session 句柄解析最近可展示消息。
/// 流程：在句柄上施加单文件上限加一字节预算，逐帧解析并仅保留最后 80 条消息，结束时检查预算是否耗尽。
/// 参数：file 为路径与句柄元数据均已校验的普通文件；返回最近消息列表。
/// 异常/边界：文件在打开后增长也不能越过读取预算；读取到第上限加一字节时拒绝全部结果，不返回截断会话。
fn read_codex_session_messages_from_file(
    file: fs::File,
) -> Result<Vec<CodexThreadMessage>, String> {
    let mut reader = BufReader::new(file.take(CODEX_SESSION_FILE_MAX_BYTES + 1));
    let mut messages = Vec::new();
    while let Some(frame) = read_bounded_codex_session_frame(&mut reader)? {
        if frame.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&frame) else {
            continue;
        };
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(message) = extract_codex_event_message(&value, &timestamp) {
            if messages.len() >= CODEX_THREAD_MESSAGE_LIMIT {
                messages.remove(0);
            }
            messages.push(message);
        }
    }
    if reader.get_ref().limit() == 0 {
        return Err(format!(
            "Codex 会话文件超过 {} 字节，已拒绝加载（错误码：CODEX_SESSION_FILE_TOO_LARGE）",
            CODEX_SESSION_FILE_MAX_BYTES
        ));
    }
    Ok(messages)
}

/// 打开并复核受限 Codex session 普通文件句柄。
/// 流程：先拒绝当前路径上的符号链接或非文件，再打开句柄并基于句柄元数据复核普通文件和初始大小。
/// 参数：path 为受限 sessions 枚举得到的路径，operation 为固定操作名；返回可交给 take(上限+1) 的文件句柄。
/// 异常/边界：路径校验与打开之间发生替换时以句柄元数据为准；初始超限直接拒绝，校验后增长由调用方的句柄读取预算兜底。
fn open_bounded_codex_session_file(path: &Path, operation: &str) -> Result<fs::File, String> {
    validate_codex_session_file(path)?;
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "读取 Codex 会话{}失败：{}",
            operation,
            trim_error_message(&error.to_string())
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        format!(
            "读取 Codex 会话{}句柄元数据失败：{}",
            operation,
            trim_error_message(&error.to_string())
        )
    })?;
    if !metadata.is_file() {
        return Err("Codex 会话句柄不是普通文件（错误码：CODEX_SESSION_FILE_INVALID）".to_string());
    }
    if metadata.len() > CODEX_SESSION_FILE_MAX_BYTES {
        return Err(format!(
            "Codex 会话文件超过 {} 字节，已拒绝加载（错误码：CODEX_SESSION_FILE_TOO_LARGE）",
            CODEX_SESSION_FILE_MAX_BYTES
        ));
    }
    Ok(file)
}

/// 在打开 Codex session JSONL 前执行路径级快速校验。
/// 流程：读取路径元数据，确认当前路径是普通文件且不超过单文件上限；参数为已由受限目录枚举得到的路径。
/// 返回：合法时返回空值；异常/边界：符号链接、目录、元数据失败或超大文件先行拒绝；调用方仍必须在打开句柄后复核并限制真实读取量。
fn validate_codex_session_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "读取 Codex 会话文件元数据失败：{}",
            trim_error_message(&error.to_string())
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err("Codex 会话路径不是普通文件（错误码：CODEX_SESSION_FILE_INVALID）".to_string());
    }
    if metadata.len() > CODEX_SESSION_FILE_MAX_BYTES {
        return Err(format!(
            "Codex 会话文件超过 {} 字节，已拒绝加载（错误码：CODEX_SESSION_FILE_TOO_LARGE）",
            CODEX_SESSION_FILE_MAX_BYTES
        ));
    }
    Ok(())
}

/// 从本地 Codex session 文件读取一条有界 JSONL 帧。
/// 流程：最多读取事件上限加 CRLF 两字节，剥离换行后返回原始 JSON 字节；参数为缓冲读取器。
/// 返回：EOF 返回 None，合法事件返回 Some；异常/边界：单行超限立即停止整个文件且只返回固定错误码，不回显消息正文。
fn read_bounded_codex_session_frame<R: BufRead>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut frame = Vec::with_capacity(CODEX_SESSION_FRAME_MAX_BYTES.min(8 * 1024));
    let bytes_read = reader
        .take((CODEX_SESSION_FRAME_MAX_BYTES.saturating_add(2)) as u64)
        .read_until(b'\n', &mut frame)
        .map_err(|_| {
            "读取 Codex 会话事件失败（错误码：CODEX_SESSION_FRAME_READ_FAILED）".to_string()
        })?;
    if bytes_read == 0 {
        return Ok(None);
    }
    let has_newline = frame.last() == Some(&b'\n');
    if has_newline {
        frame.pop();
        if frame.last() == Some(&b'\r') {
            frame.pop();
        }
    }
    if frame.len() > CODEX_SESSION_FRAME_MAX_BYTES
        || (!has_newline && bytes_read > CODEX_SESSION_FRAME_MAX_BYTES)
    {
        return Err(format!(
            "Codex 会话单条事件超过 {} 字节，已拒绝加载（错误码：CODEX_SESSION_FRAME_TOO_LARGE）",
            CODEX_SESSION_FRAME_MAX_BYTES
        ));
    }
    Ok(Some(frame))
}

/// 从单行 Codex JSONL 事件中抽取一条可展示消息。
fn extract_codex_event_message(value: &Value, timestamp: &str) -> Option<CodexThreadMessage> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = value.get("payload")?;
    match payload.get("type").and_then(Value::as_str)? {
        "user_message" => Some(CodexThreadMessage {
            role: "user".to_string(),
            content: limit_chars(
                payload.get("message").and_then(Value::as_str).unwrap_or(""),
                CODEX_MESSAGE_CONTENT_MAX_CHARS,
            ),
            created_at: timestamp.to_string(),
        }),
        "agent_message" => Some(CodexThreadMessage {
            role: "assistant".to_string(),
            content: limit_chars(
                payload.get("message").and_then(Value::as_str).unwrap_or(""),
                CODEX_MESSAGE_CONTENT_MAX_CHARS,
            ),
            created_at: timestamp.to_string(),
        }),
        _ => None,
    }
}

/// 校验 Codex 会话 ID 只能包含本地索引中的安全字符，避免 shell 命令注入。
fn validate_codex_thread_id(thread_id: &str) -> Result<(), String> {
    let is_safe = thread_id
        .chars()
        .all(|char| char.is_ascii_alphanumeric() || char == '-' || char == '_');
    if is_safe {
        Ok(())
    } else {
        Err("会话 ID 包含不支持的字符".to_string())
    }
}

/// 读取当前 Codex home 目录，优先使用 CODEX_HOME，默认回落到用户目录。
pub(crate) fn codex_home_dir() -> Result<PathBuf, String> {
    if let Ok(value) = env::var("CODEX_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = env::var("HOME").map_err(|_| "无法读取 HOME 环境变量".to_string())?;
    Ok(PathBuf::from(home).join(".codex"))
}

/// 按字符数裁剪文本，避免前端展示和命令反馈被长上下文撑爆。
fn limit_chars(value: &str, max_chars: usize) -> String {
    let chars = value.trim().chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return chars.into_iter().collect();
    }
    let mut text = chars.into_iter().take(max_chars).collect::<String>();
    text.push_str("...");
    text
}

/// 打开 macOS 辅助功能设置，用于授予自动粘贴权限。
/// 流程：校验 hub 后调用平台设置入口；参数为调用窗口；成功返回空值。
/// 异常/边界：非 hub 默认拒绝，非 macOS 平台由既有平台实现返回对应结果。
#[tauri::command]
fn open_accessibility_settings(window: tauri::WebviewWindow) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    open_accessibility_preferences()
}

/// 打开 macOS 麦克风权限设置，用于授予语音采集权限。
/// 流程：校验 hub 后调用平台设置入口；参数为调用窗口；成功返回空值。
/// 异常/边界：非 hub 默认拒绝，非 macOS 平台由既有平台实现返回对应结果。
#[tauri::command]
fn open_microphone_settings(window: tauri::WebviewWindow) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    open_microphone_preferences()
}

/// 读取当前运行时保存的快捷键，用于新配置注册失败后恢复旧快捷键。
fn read_shortcut_runtime_profile(state: &RuntimeShortcuts) -> Result<ShortcutProfile, String> {
    state
        .profile
        .lock()
        .map_err(|_| "读取快捷键状态失败：状态锁已损坏".to_string())
        .map(|profile| profile.clone())
}

/// 保存当前快捷键配置和系统注册结果，供快捷键触发与设置页诊断共同使用。
fn set_shortcut_runtime(
    state: &RuntimeShortcuts,
    profile: ShortcutProfile,
    registration_error: Option<String>,
) -> Result<(), String> {
    let status = match registration_error {
        Some(error) => ShortcutRegistrationStatus {
            ready: false,
            message: format!("快捷键注册失败：{}", trim_error_message(&error)),
        },
        None => ShortcutRegistrationStatus {
            ready: true,
            message: "快捷键已注册".to_string(),
        },
    };
    set_shortcut_runtime_status(state, profile, status.ready, status.message)
}

/// 保存当前快捷键和指定诊断状态，支持“新快捷键失败但旧快捷键已恢复”的中间态。
fn set_shortcut_runtime_status(
    state: &RuntimeShortcuts,
    profile: ShortcutProfile,
    ready: bool,
    message: String,
) -> Result<(), String> {
    {
        let mut stored_profile = state
            .profile
            .lock()
            .map_err(|_| "保存快捷键失败：状态锁已损坏".to_string())?;
        *stored_profile = profile;
    }
    let mut stored_status = state
        .registration_status
        .lock()
        .map_err(|_| "保存快捷键注册结果失败：状态锁已损坏".to_string())?;
    *stored_status = ShortcutRegistrationStatus { ready, message };
    Ok(())
}

/// 将快捷键写入 Tauri 全局快捷键插件。
#[cfg(desktop)]
fn register_shortcut_profile(
    app: &tauri::AppHandle,
    profile: &ShortcutProfile,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let mut seen = std::collections::HashSet::new();
    let mut shortcuts = Vec::new();
    for shortcut in [
        profile.asr.as_str(),
        profile.dictate.as_str(),
        profile.polish.as_str(),
    ] {
        let normalized = normalize_shortcut(shortcut);
        if seen.insert(normalized) {
            shortcuts.push(shortcut.to_string());
        }
    }
    for binding in &profile.app_bindings {
        let normalized = normalize_shortcut(&binding.shortcut);
        if seen.insert(normalized) {
            shortcuts.push(binding.shortcut.clone());
        }
    }
    let shortcut_refs = shortcuts.iter().map(String::as_str).collect::<Vec<_>>();
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| format!("注销旧快捷键失败：{}", error))?;
    app.global_shortcut()
        .register_multiple(shortcut_refs)
        .map_err(|error| format!("注册新快捷键失败：{}", error))?;
    Ok(())
}

/// 注销当前全局快捷键；仅用于前端录制新快捷键的短暂窗口。
#[cfg(desktop)]
fn suspend_shortcut_profile(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    app.global_shortcut()
        .unregister_all()
        .map_err(|error| format!("暂停快捷键失败：{}", error))
}

/// 非桌面环境不注册系统级快捷键。
#[cfg(not(desktop))]
fn register_shortcut_profile(
    _app: &tauri::AppHandle,
    _profile: &ShortcutProfile,
) -> Result<(), String> {
    Ok(())
}

/// 非桌面环境没有系统级快捷键需要暂停。
#[cfg(not(desktop))]
fn suspend_shortcut_profile(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

/// 规范化前端快捷键配置，并检查是否存在冲突。
fn normalize_shortcut_profile(profile: ShortcutProfile) -> Result<ShortcutProfile, String> {
    let normalized = ShortcutProfile {
        asr: normalize_shortcut_or_default(&profile.asr, DEFAULT_ASR_TEXT_SHORTCUT),
        dictate: normalize_shortcut_or_default(&profile.dictate, DEFAULT_DICTATE_SHORTCUT),
        polish: normalize_shortcut_or_default(&profile.polish, DEFAULT_POLISH_SHORTCUT),
        app_bindings: normalize_app_shortcut_bindings(profile.app_bindings)?,
    };
    let mut seen = std::collections::HashSet::new();
    for shortcut in [&normalized.asr, &normalized.dictate, &normalized.polish] {
        let key = normalize_shortcut(shortcut);
        if !seen.insert(key) {
            return Err("语音转文字、语音润色和文本润色不能使用同一个快捷键".to_string());
        }
    }
    for binding in &normalized.app_bindings {
        let key = normalize_shortcut(&binding.shortcut);
        if !seen.insert(key) {
            return Err(format!("快捷键绑定冲突：{}", binding.shortcut));
        }
    }
    Ok(normalized)
}

/// 规范化用户创建的打开应用快捷键绑定。
/// 流程：过滤字段空白、固定动作类型为 openApp、规范化快捷键，并校验目标路径是 .app。
/// 参数：bindings 为前端提交的绑定列表。
/// 返回：可注册到系统全局快捷键的绑定列表。
/// 异常/边界：任一绑定字段非法时整体拒绝，避免部分注册造成前后端状态不一致。
fn normalize_app_shortcut_bindings(
    bindings: Vec<AppShortcutBinding>,
) -> Result<Vec<AppShortcutBinding>, String> {
    let mut normalized_bindings = Vec::new();
    for binding in bindings {
        let shortcut = normalize_shortcut(&binding.shortcut);
        let app_path = binding.app_path.trim().to_string();
        if binding.action_type != "openApp" {
            return Err("当前只支持打开应用动作".to_string());
        }
        if binding.id.trim().is_empty() || shortcut.is_empty() {
            return Err("快捷键绑定缺少 ID 或快捷键".to_string());
        }
        if binding.app_name.trim().is_empty() || app_path.is_empty() {
            return Err("快捷键绑定缺少目标 APP".to_string());
        }
        if Path::new(&app_path)
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("app")
        {
            return Err(format!("目标 APP 路径无效：{}", app_path));
        }
        normalized_bindings.push(AppShortcutBinding {
            id: binding.id.trim().to_string(),
            shortcut,
            action_type: "openApp".to_string(),
            app_name: binding.app_name.trim().to_string(),
            app_path,
            created_at: binding.created_at.trim().to_string(),
        });
    }
    Ok(normalized_bindings)
}

/// 规范化单个快捷键，空值时使用默认值。
fn normalize_shortcut_or_default(value: &str, fallback: &str) -> String {
    let normalized = normalize_shortcut(value);
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

/// 统一快捷键大小写和修饰键别名，便于前后端比较。
fn normalize_shortcut(value: &str) -> String {
    let mut has_ctrl = false;
    let mut has_cmd = false;
    let mut has_alt = false;
    let mut has_shift = false;
    let mut keys = Vec::new();
    for part in value
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "")
        .split('+')
        .filter(|part| !part.is_empty())
    {
        match normalize_shortcut_part(part).as_str() {
            "ctrl" => has_ctrl = true,
            "cmd" => has_cmd = true,
            "alt" => has_alt = true,
            "shift" => has_shift = true,
            key => keys.push(key.to_string()),
        }
    }
    let mut parts = Vec::new();
    if has_ctrl {
        parts.push("ctrl".to_string());
    }
    if has_cmd {
        parts.push("cmd".to_string());
    }
    if has_alt {
        parts.push("alt".to_string());
    }
    if has_shift {
        parts.push("shift".to_string());
    }
    parts.extend(keys);
    parts.join("+")
}

/// 规范化单个快捷键片段，兼容 Tauri 回调里可能出现的 KeyS / Digit1 等名称。
fn normalize_shortcut_part(part: &str) -> String {
    let normalized = match part {
        "control" => "ctrl".to_string(),
        "command" | "meta" => "cmd".to_string(),
        "option" => "alt".to_string(),
        other => other.to_string(),
    };
    if let Some(key) = normalized.strip_prefix("key") {
        if key.len() == 1 {
            return key.to_string();
        }
    }
    if let Some(key) = normalized.strip_prefix("digit") {
        if key.len() == 1 {
            return key.to_string();
        }
    }
    normalized
}

/// 显示胶囊悬浮条，供前端在需要时主动唤起。
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<String, String> {
    let frontmost_app = get_frontmost_app().unwrap_or_default();
    let context = resolve_voice_trigger_context(&frontmost_app);
    if let Some(result) = app.get_webview_window("result") {
        let _ = result.hide();
    }
    if context.show_floating_window {
        present_window(&app, "main", true)?;
    } else if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    Ok(context.target_app)
}

/// 隐藏胶囊悬浮条，应用继续在后台等待全局快捷键。
#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;
    window
        .hide()
        .map_err(|error| format!("隐藏悬浮窗失败：{}", error))
}

/// 显示 Hub 主窗口，用于查看历史、词典和设置。
#[tauri::command]
fn show_hub_window(app: tauri::AppHandle) -> Result<(), String> {
    present_window(&app, "hub", false)
}

/// 隐藏 Hub 主窗口，保留后台快捷键能力。
#[tauri::command]
fn hide_hub_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("hub")
        .ok_or_else(|| "未找到 Hub 窗口".to_string())?;
    window
        .hide()
        .map_err(|error| format!("隐藏 Hub 失败：{}", error))
}

/// 在屏幕顶部显示错误气泡，让用户知道本次失败原因。
#[tauri::command]
fn show_error_bubble(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let toast = app
        .get_webview_window("toast")
        .ok_or_else(|| "未找到错误提示窗口".to_string())?;
    position_toast_window(&app, &toast)?;
    toast
        .set_always_on_top(true)
        .map_err(|error| format!("设置错误提示置顶失败：{}", error))?;
    toast
        .show()
        .map_err(|error| format!("显示错误提示失败：{}", error))?;
    toast
        .emit(
            "toast-message",
            ErrorBubblePayload {
                message: trim_error_message(&message),
            },
        )
        .map_err(|error| format!("发送错误提示失败：{}", error))
}

/// 隐藏顶部错误气泡。
#[tauri::command]
fn hide_toast_window(app: tauri::AppHandle) -> Result<(), String> {
    let toast = app
        .get_webview_window("toast")
        .ok_or_else(|| "未找到错误提示窗口".to_string())?;
    toast
        .hide()
        .map_err(|error| format!("隐藏错误提示失败：{}", error))
}

/// 显示转写结果窗口，用于自动粘贴失败或没有输入焦点时手动复制。
#[tauri::command]
fn show_result_window(
    app: tauri::AppHandle,
    result_state: State<'_, RuntimeResult>,
    text: String,
    reason: String,
    requires_accessibility: bool,
) -> Result<(), String> {
    let normalized_text = text.trim();
    if normalized_text.is_empty() {
        return Err("没有可展示的转写结果".to_string());
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let result = app
        .get_webview_window("result")
        .ok_or_else(|| "未找到结果窗口".to_string())?;
    position_top_center_window(&app, &result, RESULT_WINDOW_WIDTH, RESULT_WINDOW_TOP)
        .map_err(|error| error.replace("定位窗口", "定位结果窗口"))?;
    result
        .set_always_on_top(true)
        .map_err(|error| format!("设置结果窗口置顶失败：{}", error))?;
    result
        .show()
        .map_err(|error| format!("显示结果窗口失败：{}", error))?;
    result
        .set_focus()
        .map_err(|error| format!("聚焦结果窗口失败：{}", error))?;
    if let Some(toast) = app.get_webview_window("toast") {
        if toast.is_visible().unwrap_or(false) {
            position_toast_window(&app, &toast)?;
        }
    }
    let payload = ResultWindowPayload {
        text: normalized_text.to_string(),
        reason: trim_error_message(&reason),
        requires_accessibility,
    };
    {
        let mut stored_payload = result_state
            .payload
            .lock()
            .map_err(|_| "保存结果窗口内容失败：状态锁已损坏".to_string())?;
        *stored_payload = Some(payload.clone());
    }
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| format!("编码结果内容失败：{}", error))?;
    let _ = result.eval(format!(
        "if (window.__AIToolRenderResult) window.__AIToolRenderResult({});",
        payload_json
    ));
    result
        .emit("result-message", payload)
        .map_err(|error| format!("发送结果内容失败：{}", error))
}

/// 隐藏语音链路的临时窗口；粘贴前必须先移除这些置顶窗口，再读取外部 App 焦点。
fn hide_transient_voice_windows(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("result") {
        let _ = window.hide();
    }
}

/// 隐藏结果窗口，应用继续在后台等待全局快捷键。
#[tauri::command]
fn hide_result_window(app: tauri::AppHandle) -> Result<(), String> {
    let result = app
        .get_webview_window("result")
        .ok_or_else(|| "未找到结果窗口".to_string())?;
    result
        .hide()
        .map_err(|error| format!("隐藏结果窗口失败：{}", error))
}

/// 读取最近一次结果窗口内容，供结果窗口初始化时恢复状态。
#[tauri::command]
fn get_last_result_window_payload(
    result_state: State<'_, RuntimeResult>,
) -> Result<Option<ResultWindowPayload>, String> {
    result_state
        .payload
        .lock()
        .map_err(|_| "读取结果窗口内容失败：状态锁已损坏".to_string())
        .map(|payload| payload.clone())
}

/// 下载浏览器插件 ZIP 到用户下载目录。
/// 流程：仅允许 hub 调用，把编译进 App 的固定插件 ZIP 写入下载目录；若同名文件存在则追加序号。
/// 参数：window 用于校验调用窗口并取得 AppHandle。
/// 返回：最终保存路径。
/// 异常/边界：不接收任意源路径或任意文件内容，写入失败返回脱敏桌面错误。
#[tauri::command]
fn download_browser_extension_zip(
    window: tauri::WebviewWindow,
) -> Result<BrowserExtensionDownloadResponse, String> {
    let app = window.app_handle().clone();
    (|| -> Result<BrowserExtensionDownloadResponse, String> {
        ensure_public_api_token_window(window.label())?;
        let download_dir = app
            .path()
            .download_dir()
            .map_err(|error| format!("读取下载目录失败：{}", error))?;
        fs::create_dir_all(&download_dir)
            .map_err(|error| format!("创建下载目录失败：{}", error))?;
        let mut target_path = download_dir.join(BROWSER_EXTENSION_ZIP_FILE_NAME);
        if target_path.exists() {
            target_path = (1..=99)
                .map(|index| download_dir.join(format!("typesass-extension-{}.zip", index)))
                .find(|candidate| !candidate.exists())
                .ok_or_else(|| {
                    "下载目录中已存在过多同名插件 ZIP，请先清理旧文件后重试。".to_string()
                })?;
        }
        fs::write(&target_path, BROWSER_EXTENSION_ZIP_BYTES)
            .map_err(|error| format!("写入浏览器插件 ZIP 失败：{}", error))?;
        Ok(BrowserExtensionDownloadResponse {
            file_path: target_path.to_string_lossy().into_owned(),
        })
    })()
    .map_err(|error| {
        desktop_error::record_desktop_error(
            &app,
            "BROWSER_EXTENSION_DOWNLOAD_FAILED",
            "download_browser_extension_zip",
            None,
            &error,
        )
    })
}

/// 保存 App 进程内共享的公共 HTTP 短期 Token。
/// 流程：先校验调用方必须是 hub，再把设备授权结果写入受 Rust 互斥锁保护的内存。
/// 参数：window 用于校验窗口标签，token 为服务端签发的短期 Bearer Token；空字符串表示清除当前会话。
/// 返回：保存成功时无返回数据。
/// 边界：不持久化、不记录日志；状态锁损坏时返回明确错误，避免假装写入成功。
#[tauri::command]
fn set_public_api_token(
    window: tauri::WebviewWindow,
    state: State<'_, RuntimePublicApiToken>,
    token: String,
) -> Result<(), String> {
    ensure_public_api_token_window(window.label())?;
    let mut stored_token = state
        .token
        .lock()
        .map_err(|_| "保存公共 HTTP 会话失败：状态锁已损坏".to_string())?;
    *stored_token = token.trim().to_string();
    Ok(())
}

/// 读取 App 进程内共享的公共 HTTP 短期 Token。
/// 流程：先校验调用方必须是 hub，再从 Rust 内存复制当前 Token 供 hub 发起 HTTP 请求。
/// 参数：window 用于校验窗口标签。
/// 返回：当前短期 Token；尚未授权或已清除时返回空字符串。
/// 边界：不访问磁盘、钥匙串或网络；状态锁损坏时返回明确错误。
#[tauri::command]
fn get_public_api_token(
    window: tauri::WebviewWindow,
    state: State<'_, RuntimePublicApiToken>,
) -> Result<String, String> {
    ensure_public_api_token_window(window.label())?;
    state
        .token
        .lock()
        .map(|token| token.clone())
        .map_err(|_| "读取公共 HTTP 会话失败：状态锁已损坏".to_string())
}

/// 在旧 Token 仍为当前值时向受管 sidecar 续签，并原子替换共享 Token。
/// 流程：持有 Token 锁比较 expectedToken；值已变化则直接返回新值，否则通过 sidecar 进程内 Basic 凭据只交换一次并写回；参数为运行状态和失败请求 Token；返回可重试的新 Token。
/// 异常/边界：空值、当前会话已清除、sidecar 退出或交换失败均返回错误；整个比较与交换期间锁定 Token，防止并发 401 请求风暴。
#[tauri::command]
fn refresh_public_api_token_if_matches(
    window: tauri::WebviewWindow,
    sidecar: State<'_, RuntimeSidecar>,
    state: State<'_, RuntimePublicApiToken>,
    expected_token: String,
) -> Result<String, String> {
    let app = window.app_handle().clone();
    (|| {
        ensure_public_api_token_window(window.label())?;
        refresh_public_api_token_value_if_matches(&state.token, &expected_token, || {
            sidecar.refresh_access_token()
        })
    })()
    .map_err(|error| {
        desktop_error::record_desktop_error(
            &app,
            PUBLIC_API_TOKEN_IPC_ERROR_CODE,
            "desktop_operation",
            None,
            &error,
        )
    })
}

/// 在共享 Token 锁内执行一次 compare-and-refresh。
/// 流程：比较失败请求 Token，已被续签则直接返回当前值，仍匹配才调用一次 refresh 并替换；参数为共享状态、预期值和续签函数；返回最新 Token。
/// 异常/边界：续签失败保留旧 Token，调用方不得在本函数外先读后写；闭包设计仅用于隔离网络交换并验证并发契约。
fn refresh_public_api_token_value_if_matches<F>(
    token: &Mutex<String>,
    expected_token: &str,
    refresh: F,
) -> Result<String, String>
where
    F: FnOnce() -> Result<String, String>,
{
    let expected_token = expected_token.trim();
    if expected_token.is_empty() {
        return Err("续签公共 HTTP 会话时 expectedToken 不能为空".to_string());
    }
    let mut stored_token = token
        .lock()
        .map_err(|_| "续签公共 HTTP 会话失败：状态锁已损坏".to_string())?;
    if stored_token.is_empty() {
        return Err("公共 HTTP 会话已清除，不能续签".to_string());
    }
    if stored_token.as_str() != expected_token {
        return Ok(stored_token.clone());
    }
    let refreshed_token = refresh()?;
    if refreshed_token.trim().is_empty() {
        return Err("sidecar 返回的续签 Token 为空".to_string());
    }
    *stored_token = refreshed_token.clone();
    Ok(refreshed_token)
}

/// 在同一把互斥锁内比较并清除公共 HTTP Token。
/// 流程：锁定共享 Token，只有当前值与旧请求携带值完全一致且非空时才清除，避免迟到的 401 覆盖其它窗口刚续签的 Token。
/// 参数：token 为进程内共享值；expected_token 为收到 401 的请求实际使用值。
/// 返回：完成清除返回 true；值已变化或为空返回 false。
/// 边界：锁损坏时返回明确错误；函数不记录、复制到持久层或输出 Token。
fn clear_public_api_token_value_if_matches(
    token: &Mutex<String>,
    expected_token: &str,
) -> Result<bool, String> {
    let mut stored_token = token
        .lock()
        .map_err(|_| "清除公共 HTTP 会话失败：状态锁已损坏".to_string())?;
    if stored_token.is_empty() || stored_token.as_str() != expected_token {
        return Ok(false);
    }
    stored_token.clear();
    Ok(true)
}

/// 仅在当前进程 Token 仍属于失败请求时清除它。
/// 流程：先校验调用方必须是 hub，再把比较与清除委托给单锁原子操作。
/// 参数：window 用于校验窗口标签，expected_token 为收到 401 的请求实际携带的短期 Bearer Token。
/// 返回：实际清除返回 true；其它窗口已经续签或当前无 Token 返回 false。
/// 边界：不接受“先读后写”的非原子清除；状态锁损坏时返回明确错误且保留当前值。
#[tauri::command]
fn clear_public_api_token_if_matches(
    window: tauri::WebviewWindow,
    state: State<'_, RuntimePublicApiToken>,
    expected_token: String,
) -> Result<bool, String> {
    let app = window.app_handle().clone();
    (|| {
        ensure_public_api_token_window(window.label())?;
        clear_public_api_token_value_if_matches(&state.token, &expected_token)
    })()
    .map_err(|error| {
        desktop_error::record_desktop_error(
            &app,
            PUBLIC_API_TOKEN_IPC_ERROR_CODE,
            "desktop_operation",
            None,
            &error,
        )
    })
}

/// 限制敏感公共 API Token IPC 只能由主 hub 窗口调用。
/// 流程：精确比较 Tauri 注入的窗口标签；参数为不可由 IPC Body 伪造的运行时 label；hub 返回成功，其它窗口拒绝。
/// 返回：授权成功为空值；异常/边界：任何未知、临时或空标签都 fail-closed，错误信息不包含 Token。
fn ensure_public_api_token_window(window_label: &str) -> Result<(), String> {
    if window_label == "hub" {
        Ok(())
    } else {
        Err("当前窗口无权访问公共 API Token（错误码：PUBLIC_API_TOKEN_FORBIDDEN）".to_string())
    }
}

/// 切换开机启动。macOS 下写入用户级 LaunchAgent。
/// 流程：校验 hub 后按 enabled 安装或卸载登录项；参数为调用窗口和目标状态；成功返回空值。
/// 异常/边界：非 hub 默认拒绝，文件或 launchctl 操作失败时不伪报设置成功。
#[tauri::command]
fn set_login_launch(window: tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    if enabled {
        install_login_agent()
    } else {
        uninstall_login_agent()
    }
}

/// 查询当前用户级开机启动项是否存在。
/// 流程：校验 hub 后读取固定登录项路径；参数为调用窗口；返回当前存在状态。
/// 异常/边界：非 hub 默认拒绝，路径解析失败时显式返回错误。
#[tauri::command]
fn get_login_launch(window: tauri::WebviewWindow) -> Result<bool, String> {
    ensure_sensitive_management_window(window.label())?;
    Ok(login_agent_path()?.exists())
}

/// 切换 Dock 图标显示状态。
/// 流程：校验 hub 后调用 Tauri 平台接口；参数为调用窗口和目标可见状态；成功返回空值。
/// 异常/边界：非 hub 默认拒绝，非 macOS 平台保持兼容空操作。
#[tauri::command]
fn set_dock_visible(window: tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    ensure_sensitive_management_window(window.label())?;
    let app = window.app_handle().clone();
    #[cfg(target_os = "macos")]
    {
        app.set_dock_visibility(visible)
            .map_err(|error| format!("切换 Dock 显示失败：{}", error))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        let _ = visible;
    }
    Ok(())
}

/// 读取当前前台 App 名称，用于 AI 输出风格上下文。
#[tauri::command]
fn get_frontmost_app() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        run_osascript(
            r#"tell application "System Events" to get name of first application process whose frontmost is true"#,
        )
        .map(|value| value.trim().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(String::new())
    }
}

/// 过滤 CodexMan 自身窗口，避免把录音浮窗当作自动粘贴目标。
fn normalize_target_app_name(app_name: &str) -> String {
    let normalized_app_name = app_name.trim();
    if normalized_app_name.is_empty()
        || normalized_app_name == "AiTool"
        || normalized_app_name == "CodexMan"
        || normalized_app_name == "ai-tool"
    {
        return String::new();
    }
    normalized_app_name.to_string()
}

/// 自动粘贴的主链路只确认系统粘贴指令是否发出，不把未回读的输入框写入当作已验证。
fn should_mark_paste_command_as_sent(
    accessibility_ready: bool,
    final_target_ready: bool,
    insertion_verified: bool,
) -> bool {
    let _ = (final_target_ready, insertion_verified);
    accessibility_ready
}

/// 根据是否有原剪贴板快照与恢复结果，生成统一的诊断状态。
fn build_clipboard_restore_status<T, E: ToString>(
    snapshot: Option<T>,
    restore_result: Result<(), E>,
) -> ClipboardRestoreStatus {
    if snapshot.is_none() {
        return ClipboardRestoreStatus {
            attempted: false,
            restored: false,
            message: "未获取原剪贴板快照，未执行恢复。".to_string(),
        };
    }
    match restore_result {
        Ok(()) => ClipboardRestoreStatus {
            attempted: true,
            restored: true,
            message: "已恢复用户原剪贴板。".to_string(),
        },
        Err(error) => ClipboardRestoreStatus {
            attempted: true,
            restored: false,
            message: format!(
                "恢复用户原剪贴板失败：{}",
                trim_error_message(&error.to_string())
            ),
        },
    }
}

/// 生成未触发剪贴板恢复的诊断状态，用于没有实际写入临时剪贴板的路径。
fn clipboard_restore_not_attempted(reason: &str) -> ClipboardRestoreStatus {
    ClipboardRestoreStatus {
        attempted: false,
        restored: false,
        message: reason.to_string(),
    }
}

/// 读取当前前台 App 的可输入焦点状态；可输入时才允许发送系统粘贴。
#[cfg(target_os = "macos")]
fn read_paste_focus_status() -> PasteFocusStatus {
    let script = r#"
tell application "System Events"
  try
    set frontApp to first application process whose frontmost is true
    set focusedElement to value of attribute "AXFocusedUIElement" of frontApp
    set focusedRole to ""
    set focusedSubrole to ""
    set focusedDescription to ""
    try
      set focusedRole to value of attribute "AXRole" of focusedElement
    end try
    try
      set focusedSubrole to value of attribute "AXSubrole" of focusedElement
    end try
    try
      set focusedDescription to value of attribute "AXDescription" of focusedElement
    end try
    return focusedRole & "|" & focusedSubrole & "|" & focusedDescription
  on error errMsg
    return "NO_FOCUS||" & errMsg
  end try
end tell
"#;
    match run_osascript_inline(script) {
        Ok(output) => parse_paste_focus_status(&output),
        Err(error) => PasteFocusStatus {
            ready: false,
            summary: format!("焦点读取失败：{}", trim_error_message(&error)),
        },
    }
}

/// 非 macOS 平台当前没有系统级粘贴焦点检测。
#[cfg(not(target_os = "macos"))]
fn read_paste_focus_status() -> PasteFocusStatus {
    PasteFocusStatus {
        ready: false,
        summary: "当前平台不支持读取系统输入焦点。".to_string(),
    }
}

/// 短时间重复读取粘贴焦点，避免 ChatGPT / Electron 输入框在窗口状态切换后短暂返回无焦点。
fn read_stable_paste_focus_status() -> PasteFocusStatus {
    let mut last_status = read_paste_focus_status();
    if last_status.ready {
        return last_status;
    }
    let mut summaries = vec![last_status.summary.clone()];
    for _ in 0..PASTE_FOCUS_RETRY_COUNT {
        thread::sleep(Duration::from_millis(PASTE_FOCUS_RETRY_DELAY_MS));
        last_status = read_paste_focus_status();
        if last_status.ready {
            last_status.summary = format!(
                "{}；焦点短暂丢失后已恢复，重试窗口约 {}ms",
                last_status.summary,
                PASTE_FOCUS_RETRY_COUNT as u64 * PASTE_FOCUS_RETRY_DELAY_MS
            );
            return last_status;
        }
        summaries.push(last_status.summary.clone());
    }
    PasteFocusStatus {
        ready: false,
        summary: format!(
            "{}；已重试 {} 次",
            summaries.join(" -> "),
            PASTE_FOCUS_RETRY_COUNT
        ),
    }
}

/// 读取当前可输入控件的位置快照；只有 role 确认为文本输入时才返回。
#[cfg(target_os = "macos")]
fn read_current_paste_focus_snapshot(target_app: &str) -> Option<PasteFocusSnapshot> {
    let normalized_target_app = normalize_target_app_name(target_app);
    if normalized_target_app.is_empty() {
        return None;
    }
    let script = r#"
tell application "System Events"
  try
    set frontApp to first application process whose frontmost is true
    set focusedElement to value of attribute "AXFocusedUIElement" of frontApp
    set focusedRole to ""
    set focusedSubrole to ""
    set focusedDescription to ""
    try
      set focusedRole to value of attribute "AXRole" of focusedElement
    end try
    try
      set focusedSubrole to value of attribute "AXSubrole" of focusedElement
    end try
    try
      set focusedDescription to value of attribute "AXDescription" of focusedElement
    end try
    set focusedPosition to value of attribute "AXPosition" of focusedElement
    set focusedSize to value of attribute "AXSize" of focusedElement
    return focusedRole & linefeed & focusedSubrole & linefeed & focusedDescription & linefeed & (item 1 of focusedPosition as text) & linefeed & (item 2 of focusedPosition as text) & linefeed & (item 1 of focusedSize as text) & linefeed & (item 2 of focusedSize as text)
  on error errMsg
    return "NO_FOCUS" & linefeed & "" & linefeed & errMsg
  end try
end tell
"#;
    let output = run_osascript_inline(script).ok()?;
    parse_paste_focus_snapshot(&normalized_target_app, &output)
}

/// 非 macOS 平台没有系统级输入焦点位置读取能力。
#[cfg(not(target_os = "macos"))]
fn read_current_paste_focus_snapshot(target_app: &str) -> Option<PasteFocusSnapshot> {
    let _ = target_app;
    None
}

/// 解析 AppleScript 返回的焦点位置快照，并过滤非文本输入控件。
fn parse_paste_focus_snapshot(target_app: &str, output: &str) -> Option<PasteFocusSnapshot> {
    let lines = output.lines().collect::<Vec<_>>();
    let role = lines.first().copied().unwrap_or_default().trim();
    let subrole = lines.get(1).copied().unwrap_or_default().trim();
    let description = lines.get(2).copied().unwrap_or_default().trim();
    let focus_status = parse_paste_focus_status(&format!("{}|{}|{}", role, subrole, description));
    if !focus_status.ready {
        return None;
    }
    let x = lines.get(3)?.trim().parse::<f64>().ok()?;
    let y = lines.get(4)?.trim().parse::<f64>().ok()?;
    let width = lines.get(5)?.trim().parse::<f64>().ok()?;
    let height = lines.get(6)?.trim().parse::<f64>().ok()?;
    if width < 4.0 || height < 4.0 {
        return None;
    }
    Some(PasteFocusSnapshot {
        target_app: normalize_target_app_name(target_app),
        summary: focus_status.summary,
        center_x: (x + width / 2.0).round() as i64,
        center_y: (y + height / 2.0).round() as i64,
    })
}

/// 根据录音开始时捕获的输入框坐标恢复同一个输入区域的焦点。
#[cfg(target_os = "macos")]
fn restore_paste_focus_snapshot(snapshot: &PasteFocusSnapshot) -> Result<String, String> {
    if normalize_target_app_name(&snapshot.target_app).is_empty() {
        return Err("没有可恢复的外部目标 App。".to_string());
    }
    let script = format!(
        r#"
tell application "System Events"
  click at {{{}, {}}}
end tell
"#,
        snapshot.center_x, snapshot.center_y
    );
    run_osascript_inline(&script).map(|value| {
        let normalized = value.trim();
        if normalized.is_empty() {
            format!(
                "已点击录音开始时的输入区域：{} ({}, {})",
                snapshot.summary, snapshot.center_x, snapshot.center_y
            )
        } else {
            format!("已点击录音开始时的输入区域：{}", normalized)
        }
    })
}

/// 非 macOS 平台暂不支持根据坐标恢复系统输入焦点。
#[cfg(not(target_os = "macos"))]
fn restore_paste_focus_snapshot(snapshot: &PasteFocusSnapshot) -> Result<String, String> {
    let _ = snapshot;
    Err("当前平台不支持恢复系统输入焦点。".to_string())
}

/// 解析 AppleScript 返回的焦点 role/subrole/description，判断是否适合发送 Cmd+V。
fn parse_paste_focus_status(output: &str) -> PasteFocusStatus {
    let normalized = output.trim();
    let parts = normalized.splitn(3, '|').collect::<Vec<_>>();
    let role = parts.first().copied().unwrap_or_default().trim();
    let subrole = parts.get(1).copied().unwrap_or_default().trim();
    let description = parts.get(2).copied().unwrap_or_default().trim();
    if role == "NO_FOCUS" || role.is_empty() {
        return PasteFocusStatus {
            ready: false,
            summary: if description.is_empty() {
                "未检测到当前输入焦点。".to_string()
            } else {
                format!("未检测到当前输入焦点：{}", description)
            },
        };
    }
    let is_text_focus = matches!(
        role,
        "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField"
    ) || matches!(subrole, "AXSearchField" | "AXTextField" | "AXTextArea");
    PasteFocusStatus {
        ready: is_text_focus,
        summary: format_paste_focus_summary(role, subrole, description),
    }
}

/// 生成焦点元素摘要，避免诊断日志只显示系统 role 字符串。
fn format_paste_focus_summary(role: &str, subrole: &str, description: &str) -> String {
    let mut parts = vec![format!("role={}", role)];
    if !subrole.is_empty() {
        parts.push(format!("subrole={}", subrole));
    }
    if !description.is_empty() {
        parts.push(format!("description={}", description));
    }
    parts.join("，")
}

/// 设置系统输出静音状态，并返回设置前的静音状态。
/// 流程：校验 hub，读取旧静音态后设置新值；参数为窗口和目标状态；返回设置前状态供调用方恢复。
/// 异常/边界：非 hub 默认拒绝，平台脚本失败时不返回虚构旧状态；非 macOS 返回 false。
#[tauri::command]
fn set_system_output_muted(window: tauri::WebviewWindow, muted: bool) -> Result<bool, String> {
    ensure_sensitive_management_window(window.label())?;
    #[cfg(target_os = "macos")]
    {
        let previous = run_osascript(r#"output muted of (get volume settings)"#)?
            .trim()
            .eq_ignore_ascii_case("true");
        let command = if muted {
            "set volume with output muted"
        } else {
            "set volume without output muted"
        };
        run_osascript(command)?;
        Ok(previous)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = muted;
        Ok(false)
    }
}

/// 播放录音开始或停止提示音，避免 WebView 全局快捷键触发时受自动播放策略影响。
#[tauri::command]
fn play_native_interaction_sound(kind: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let sound_path = match kind.as_str() {
            "start" => "/System/Library/Sounds/Tink.aiff",
            "stop" => "/System/Library/Sounds/Pop.aiff",
            _ => "/System/Library/Sounds/Tink.aiff",
        };
        Command::new("afplay")
            .arg(sound_path)
            .spawn()
            .map_err(|error| format!("播放交互提示音失败：{}", error))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Ok(())
    }
}

/// 安装当前 App 的用户级 LaunchAgent。
fn install_login_agent() -> Result<(), String> {
    let path = login_agent_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "开机启动路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建开机启动目录失败：{}", error))?;
    let app_path = current_app_launch_path()?;
    fs::write(&path, build_login_agent_plist(&app_path))
        .map_err(|error| format!("写入开机启动配置失败：{}", error))?;
    reload_login_agent(&path, true);
    Ok(())
}

/// 卸载当前 App 的用户级 LaunchAgent。
fn uninstall_login_agent() -> Result<(), String> {
    let path = login_agent_path()?;
    reload_login_agent(&path, false);
    if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("删除开机启动配置失败：{}", error))?;
    }
    Ok(())
}

/// 当前用户 LaunchAgent plist 路径。
fn login_agent_path() -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| "无法读取 HOME 目录".to_string())?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", LOGIN_AGENT_LABEL)))
}

/// 找到可用于启动的 App bundle；开发模式下回落到当前可执行文件。
fn current_app_launch_path() -> Result<PathBuf, String> {
    let executable =
        env::current_exe().map_err(|error| format!("读取当前程序路径失败：{}", error))?;
    for ancestor in executable.ancestors() {
        if ancestor.extension().and_then(|value| value.to_str()) == Some("app") {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(executable)
}

/// 生成 LaunchAgent plist。
fn build_login_agent_plist(app_path: &std::path::Path) -> String {
    let app_path = xml_escape(&app_path.to_string_lossy());
    let program_arguments = if app_path.ends_with(".app") {
        format!(
            "<string>/usr/bin/open</string>\n    <string>{}</string>",
            app_path
        )
    } else {
        format!("<string>{}</string>", app_path)
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
    {}
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#,
        LOGIN_AGENT_LABEL, program_arguments
    )
}

/// 重新加载或卸载 LaunchAgent，失败不影响 plist 写入/删除结果。
fn reload_login_agent(path: &std::path::Path, bootstrap: bool) {
    let uid = run_command_output("id", &["-u"]).unwrap_or_default();
    let domain = format!("gui/{}", uid.trim());
    let action = if bootstrap { "bootstrap" } else { "bootout" };
    let _ = Command::new("launchctl")
        .arg(action)
        .arg(domain)
        .arg(path)
        .output();
}

/// 执行 AppleScript 并返回标准输出。
fn run_osascript(script: &str) -> Result<String, String> {
    run_command_output("osascript", &["-e", script])
}

/// 转义 AppleScript 字符串字面量，避免 App 名称里的反斜杠或引号破坏脚本结构。
#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 把录音开始时的目标 App 重新置为前台，修复悬浮窗处理态导致外部输入框焦点丢失。
#[cfg(target_os = "macos")]
fn refocus_paste_target_app(target_app: &str) -> Result<String, String> {
    let normalized_target_app = normalize_target_app_name(target_app);
    if normalized_target_app.is_empty() {
        return Ok(String::new());
    }
    let target_app_script_value = escape_applescript_string(&normalized_target_app);
    let script = format!(
        r#"
tell application "System Events"
  set matchedProcesses to application processes whose name is "{target_app_script_value}"
  if (count of matchedProcesses) is 0 then
    return "目标 App 不在运行中：" & "{target_app_script_value}"
  end if
  set frontmost of item 1 of matchedProcesses to true
  return name of item 1 of matchedProcesses
end tell
"#,
    );
    run_osascript_inline(&script).map(|value| value.trim().to_string())
}

/// 非 macOS 平台暂不支持恢复系统前台 App。
#[cfg(not(target_os = "macos"))]
fn refocus_paste_target_app(target_app: &str) -> Result<String, String> {
    let _ = target_app;
    Ok(String::new())
}

/// 执行多行 AppleScript 并返回标准输出，避免把复杂脚本压成难维护的一行。
fn run_osascript_inline(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| format!("执行 osascript 失败：{}", error))?;
    if !output.status.success() {
        return Err(format!(
            "执行 osascript 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 运行命令并读取标准输出。
fn run_command_output(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("执行 {} 失败：{}", program, error))?;
    if !output.status.success() {
        return Err(format!(
            "执行 {} 失败：{}",
            program,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 转义 XML 字段，避免路径中的特殊字符破坏 plist。
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 把错误提示窗口定位到主屏幕顶部偏下的位置。
fn position_toast_window(
    app: &tauri::AppHandle,
    toast: &tauri::WebviewWindow,
) -> Result<(), String> {
    let work_area = preferred_window_work_area(app)?;
    let top = if app
        .get_webview_window("result")
        .and_then(|result| result.is_visible().ok())
        .unwrap_or(false)
    {
        let below_result_top = RESULT_WINDOW_TOP + RESULT_WINDOW_HEIGHT + RESULT_TOAST_GAP;
        let max_visible_top =
            (work_area.height - TOAST_WINDOW_HEIGHT - RESULT_TOAST_GAP).max(TOAST_WINDOW_TOP);
        below_result_top.min(max_visible_top)
    } else {
        TOAST_WINDOW_TOP
    };
    let position = top_center_position_in_work_area(work_area, TOAST_WINDOW_WIDTH, top);
    toast
        .set_position(Position::Logical(LogicalPosition::new(
            position.x, position.y,
        )))
        .map_err(|error| format!("定位错误提示失败：{}", error))
}

/// 把指定窗口定位到当前工作屏幕顶部居中的位置。
fn position_top_center_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    width: f64,
    top: f64,
) -> Result<(), String> {
    let work_area = preferred_window_work_area(app)?;
    let position = top_center_position_in_work_area(work_area, width, top);
    window
        .set_position(Position::Logical(LogicalPosition::new(
            position.x, position.y,
        )))
        .map_err(|error| format!("定位窗口失败：{}", error))
}

/// 逻辑坐标点，用于在多屏工作区之间选择目标屏幕。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenPoint {
    x: f64,
    y: f64,
}

/// 屏幕可用工作区的逻辑坐标，已经扣除菜单栏和 Dock 占用区域。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenWorkArea {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// 优先选择当前前台窗口所在屏幕；取不到时用鼠标所在屏幕，最后回到主屏幕。
fn preferred_window_work_area(app: &tauri::AppHandle) -> Result<ScreenWorkArea, String> {
    let monitors = app
        .available_monitors()
        .map_err(|error| format!("读取屏幕信息失败：{}", error))?;
    let work_areas = monitors
        .iter()
        .map(work_area_from_monitor)
        .collect::<Vec<_>>();
    if let Some(area) = select_work_area_for_anchor(&work_areas, read_frontmost_window_center()) {
        return Ok(area);
    }
    if let Ok(cursor_position) = app.cursor_position() {
        if let Ok(Some(monitor)) = app.monitor_from_point(cursor_position.x, cursor_position.y) {
            return Ok(work_area_from_monitor(&monitor));
        }
    }
    if let Some(monitor) = app
        .primary_monitor()
        .map_err(|error| format!("读取主屏幕失败：{}", error))?
    {
        return Ok(work_area_from_monitor(&monitor));
    }
    work_areas
        .first()
        .copied()
        .ok_or_else(|| "没有可用屏幕".to_string())
}

/// 把 Tauri 屏幕工作区转换为逻辑坐标，便于统一窗口定位。
fn work_area_from_monitor(monitor: &tauri::Monitor) -> ScreenWorkArea {
    let scale_factor = monitor.scale_factor();
    let work_area = monitor.work_area();
    ScreenWorkArea {
        x: work_area.position.x as f64 / scale_factor,
        y: work_area.position.y as f64 / scale_factor,
        width: work_area.size.width as f64 / scale_factor,
        height: work_area.size.height as f64 / scale_factor,
    }
}

/// 根据前台窗口中心点选择屏幕工作区；没有命中时取距离最近的工作区。
fn select_work_area_for_anchor(
    work_areas: &[ScreenWorkArea],
    anchor: Option<ScreenPoint>,
) -> Option<ScreenWorkArea> {
    let anchor = anchor?;
    work_areas
        .iter()
        .copied()
        .find(|area| point_in_work_area(anchor, *area))
        .or_else(|| {
            work_areas.iter().copied().min_by(|left, right| {
                distance_to_work_area(anchor, *left)
                    .total_cmp(&distance_to_work_area(anchor, *right))
            })
        })
}

/// 判断逻辑坐标点是否落在指定屏幕工作区内。
fn point_in_work_area(point: ScreenPoint, area: ScreenWorkArea) -> bool {
    point.x >= area.x
        && point.x <= area.x + area.width
        && point.y >= area.y
        && point.y <= area.y + area.height
}

/// 计算点到工作区中心点的平方距离，避免浮点开根号。
fn distance_to_work_area(point: ScreenPoint, area: ScreenWorkArea) -> f64 {
    let center_x = area.x + area.width / 2.0;
    let center_y = area.y + area.height / 2.0;
    (point.x - center_x).powi(2) + (point.y - center_y).powi(2)
}

/// 计算窗口在目标工作区顶部居中的逻辑坐标。
fn top_center_position_in_work_area(
    work_area: ScreenWorkArea,
    width: f64,
    top: f64,
) -> ScreenPoint {
    ScreenPoint {
        x: work_area.x + (work_area.width - width) / 2.0,
        y: work_area.y + top,
    }
}

/// 读取当前前台 App 的窗口中心点，让浮窗跟随用户正在工作的屏幕。
fn read_frontmost_window_center() -> Option<ScreenPoint> {
    #[cfg(target_os = "macos")]
    {
        let script = r#"
tell application "System Events"
    set frontProcesses to application processes whose frontmost is true
    if (count of frontProcesses) is 0 then return ""
    set frontProcess to item 1 of frontProcesses
    if not (exists window 1 of frontProcess) then return ""
    set windowPosition to position of window 1 of frontProcess
    set windowSize to size of window 1 of frontProcess
    set centerX to (item 1 of windowPosition) + ((item 1 of windowSize) / 2)
    set centerY to (item 2 of windowPosition) + ((item 2 of windowSize) / 2)
    return (centerX as text) & "," & (centerY as text)
end tell
"#;
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_screen_point(&String::from_utf8_lossy(&output.stdout))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// 解析 AppleScript 返回的 `x,y` 坐标。
fn parse_screen_point(value: &str) -> Option<ScreenPoint> {
    let (x, y) = value.trim().split_once(',')?;
    Some(ScreenPoint {
        x: x.trim().parse().ok()?,
        y: y.trim().parse().ok()?,
    })
}

/// 通过系统复制快捷键读取当前外部 App 的选中文本，并尽量恢复用户原剪贴板。
#[tauri::command]
fn read_selected_text() -> Result<SelectedTextResponse, String> {
    let target_app = get_frontmost_app().unwrap_or_default();
    let normalized_target_app = normalize_target_app_name(&target_app);
    if normalized_target_app.is_empty() {
        return Err("没有检测到可读取选中文本的外部应用。".to_string());
    }
    let accessibility_trusted = is_accessibility_trusted();
    if !accessibility_trusted {
        return Err("读取选中文本需要先给 CodexMan 开启辅助功能权限。".to_string());
    }
    let clipboard_snapshot = capture_clipboard_snapshot()
        .map_err(|error| format!("备份用户原剪贴板失败：{}", trim_error_message(&error)))?;
    let marker = format!("codexman-selection-marker-{}", std::process::id());
    write_clipboard_text(&marker)?;
    let copy_result = match trigger_system_copy() {
        Ok(value) => value,
        Err(error) => {
            let clipboard_restore_status = build_clipboard_restore_status(
                Some(()),
                restore_clipboard_snapshot(&clipboard_snapshot),
            );
            return Err(format!("{}；{}", error, clipboard_restore_status.message));
        }
    };
    thread::sleep(Duration::from_millis(120));
    let selected_text = match read_clipboard_text_raw() {
        Ok(text) => text,
        Err(error) => {
            let clipboard_restore_status = build_clipboard_restore_status(
                Some(()),
                restore_clipboard_snapshot(&clipboard_snapshot),
            );
            return Err(format!(
                "读取选中文本失败：{}；{}",
                trim_error_message(&error),
                clipboard_restore_status.message
            ));
        }
    };
    let clipboard_restore_status =
        build_clipboard_restore_status(Some(()), restore_clipboard_snapshot(&clipboard_snapshot));
    let normalized_text = selected_text.trim().to_string();
    if normalized_text.is_empty() || normalized_text == marker {
        return Err(format!(
            "没有读到选中文本，请先在外部应用中框选一段文字。{}",
            clipboard_restore_status.message
        ));
    }
    Ok(SelectedTextResponse {
        text: normalized_text,
        target_app: normalized_target_app,
        accessibility_trusted,
        clipboard_restored: clipboard_restore_status.restored,
        clipboard_restore_message: clipboard_restore_status.message,
        copy_method: copy_result.method,
    })
}

/// 把文字写入系统剪贴板，并模拟 Cmd+V 粘贴到当前前台输入框。
#[tauri::command]
async fn paste_text(
    app: tauri::AppHandle,
    focus_snapshot_state: State<'_, RuntimePasteFocusSnapshot>,
    text: String,
    target_app: String,
) -> Result<PasteResponse, String> {
    let normalized_text = text.trim();
    if normalized_text.is_empty() {
        return Err("转写结果为空，无法自动粘贴".to_string());
    }

    hide_transient_voice_windows(&app);
    thread::sleep(Duration::from_millis(PASTE_WINDOW_SETTLE_DELAY_MS));
    let frontmost_before_paste = get_frontmost_app().unwrap_or_default();
    let paste_target = resolve_paste_target(&target_app, &frontmost_before_paste);
    let normalized_target_app = paste_target.target_app;
    let accessibility_trusted = is_accessibility_trusted();
    if normalized_target_app.is_empty() {
        let requested_target_app = normalize_target_app_name(&target_app);
        let normalized_frontmost_app = normalize_target_app_name(&frontmost_before_paste);
        let message = if !requested_target_app.is_empty()
            && !normalized_frontmost_app.is_empty()
            && requested_target_app != normalized_frontmost_app
        {
            format!(
                "录音开始时的目标是 {}，但粘贴前当前前台已变为 {}；已阻止自动粘贴，避免写入错误输入框。",
                requested_target_app, normalized_frontmost_app
            )
        } else {
            "当前焦点不在外部输入目标上；已保持原剪贴板不变。".to_string()
        };
        let clipboard_restore_status =
            clipboard_restore_not_attempted("未写入临时剪贴板，避免覆盖用户原剪贴板。");
        return Ok(PasteResponse {
            command_sent: false,
            message,
            requires_accessibility: false,
            target_app: normalized_target_app,
            clipboard_written: false,
            clipboard_matches_expected: false,
            clipboard_restore_attempted: clipboard_restore_status.attempted,
            clipboard_restored: clipboard_restore_status.restored,
            clipboard_restore_message: clipboard_restore_status.message,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate: String::new(),
            frontmost_after_paste: String::new(),
            insertion_verified: false,
            verification_status: "没有可恢复的目标输入框".to_string(),
            focused_element_before_paste: String::new(),
            focused_element_after_activate: String::new(),
            focused_element_after_paste: String::new(),
        });
    }
    if !accessibility_trusted {
        let clipboard_restore_status =
            clipboard_restore_not_attempted("辅助功能未授权，未写入临时剪贴板。");
        return Ok(PasteResponse {
            command_sent: false,
            message: "自动粘贴需要先给 CodexMan 开启辅助功能权限；已保持原剪贴板不变。".to_string(),
            requires_accessibility: true,
            target_app: normalized_target_app,
            clipboard_written: false,
            clipboard_matches_expected: false,
            clipboard_restore_attempted: clipboard_restore_status.attempted,
            clipboard_restored: clipboard_restore_status.restored,
            clipboard_restore_message: clipboard_restore_status.message,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate: String::new(),
            frontmost_after_paste: String::new(),
            insertion_verified: false,
            verification_status: "辅助功能未授权，无法发送系统粘贴".to_string(),
            focused_element_before_paste: String::new(),
            focused_element_after_activate: String::new(),
            focused_element_after_paste: String::new(),
        });
    }

    let trusts_explicit_target =
        should_trust_explicit_paste_target(&target_app, &normalized_target_app);
    let should_refocus_target =
        should_refocus_requested_paste_target(&target_app, &normalized_target_app)
            && !trusts_explicit_target;
    let (frontmost_after_activate, focused_element_after_activate) = if should_refocus_target {
        let activate_summary = match refocus_paste_target_app(&normalized_target_app) {
            Ok(app_name) if app_name.trim().is_empty() => "目标 App 无需重新激活。".to_string(),
            Ok(app_name) => format!("已恢复目标 App 前台：{}", app_name),
            Err(error) => format!("恢复目标 App 前台失败：{}", trim_error_message(&error)),
        };
        thread::sleep(Duration::from_millis(PASTE_TARGET_REFOCUS_DELAY_MS));
        let frontmost_after_activate = get_frontmost_app().unwrap_or_default();
        let focus_after_activate = read_paste_focus_status();
        let mut focus_recovery_summary =
            format!("{}；{}", activate_summary, focus_after_activate.summary);
        if !focus_after_activate.ready {
            let stored_snapshot = focus_snapshot_state
                .snapshot
                .lock()
                .ok()
                .and_then(|snapshot| snapshot.clone())
                .filter(|snapshot| {
                    normalize_target_app_name(&snapshot.target_app) == normalized_target_app
                });
            if let Some(snapshot) = stored_snapshot {
                let restore_summary = match restore_paste_focus_snapshot(&snapshot) {
                    Ok(message) => message,
                    Err(error) => {
                        format!(
                            "恢复录音开始时的输入区域失败：{}",
                            trim_error_message(&error)
                        )
                    }
                };
                thread::sleep(Duration::from_millis(PASTE_TARGET_REFOCUS_DELAY_MS));
                let focus_after_restore = read_paste_focus_status();
                focus_recovery_summary = format!(
                    "{}；{}；恢复后：{}",
                    focus_recovery_summary, restore_summary, focus_after_restore.summary
                );
            } else {
                focus_recovery_summary = format!(
                    "{}；没有录音开始时的可输入控件快照，未执行输入区域恢复。",
                    focus_recovery_summary
                );
            }
        }
        (frontmost_after_activate, focus_recovery_summary)
    } else {
        let focus_strategy = if trusts_explicit_target {
            "显式目标快速粘贴：录音开始目标 App 与粘贴前前台 App 一致，跳过 App 激活、输入区域恢复和 AX 文本焦点轮询。"
        } else {
            "直接粘贴模式：没有录音开始时的明确外部目标 App，未执行前台恢复。"
        };
        (String::new(), focus_strategy.to_string())
    };

    let focus_status = if trusts_explicit_target {
        PasteFocusStatus {
            ready: true,
            summary: "显式目标快速粘贴：目标 App 未变化，直接发送系统粘贴。".to_string(),
        }
    } else {
        read_stable_paste_focus_status()
    };
    if !focus_status.ready && !trusts_explicit_target {
        let clipboard_restore_status =
            clipboard_restore_not_attempted("未写入临时剪贴板，避免覆盖用户原剪贴板。");
        return Ok(PasteResponse {
            command_sent: false,
            message: "当前没有聚焦可输入区域，未发送粘贴指令；结果已展示，可手动复制。".to_string(),
            requires_accessibility: false,
            target_app: normalized_target_app,
            clipboard_written: false,
            clipboard_matches_expected: false,
            clipboard_restore_attempted: clipboard_restore_status.attempted,
            clipboard_restored: clipboard_restore_status.restored,
            clipboard_restore_message: clipboard_restore_status.message,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate,
            frontmost_after_paste: String::new(),
            insertion_verified: false,
            verification_status: "粘贴前没有检测到可输入焦点，未发送 Cmd+V".to_string(),
            focused_element_before_paste: focus_status.summary,
            focused_element_after_activate,
            focused_element_after_paste: String::new(),
        });
    }
    let focused_element_before_paste = if focus_status.ready {
        focus_status.summary
    } else {
        format!(
            "{}；Web 输入焦点未暴露为 AX 文本控件，但录音开始目标 App 与粘贴前目标 App 一致，继续发送系统粘贴。",
            focus_status.summary
        )
    };
    let frontmost_before_clipboard_write = get_frontmost_app().unwrap_or_default();
    let normalized_frontmost_before_clipboard_write =
        normalize_target_app_name(&frontmost_before_clipboard_write);
    if normalized_frontmost_before_clipboard_write != normalized_target_app {
        let clipboard_restore_status =
            clipboard_restore_not_attempted("未写入临时剪贴板，避免覆盖用户原剪贴板。");
        return Ok(PasteResponse {
            command_sent: false,
            message: format!(
                "录音开始时的目标是 {}，但粘贴前当前前台已变为 {}；已阻止自动粘贴，避免写入错误输入框。",
                normalized_target_app,
                if normalized_frontmost_before_clipboard_write.is_empty() {
                    "未知应用".to_string()
                } else {
                    normalized_frontmost_before_clipboard_write
                }
            ),
            requires_accessibility: false,
            target_app: normalized_target_app,
            clipboard_written: false,
            clipboard_matches_expected: false,
            clipboard_restore_attempted: clipboard_restore_status.attempted,
            clipboard_restored: clipboard_restore_status.restored,
            clipboard_restore_message: clipboard_restore_status.message,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate,
            frontmost_after_paste: frontmost_before_clipboard_write,
            insertion_verified: false,
            verification_status: "粘贴前目标 App 已变化，未发送 Cmd+V".to_string(),
            focused_element_before_paste,
            focused_element_after_activate,
            focused_element_after_paste: String::new(),
        });
    }

    let clipboard_snapshot = match capture_clipboard_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let clipboard_restore_status = clipboard_restore_not_attempted(&format!(
                "备份用户原剪贴板失败：{}",
                trim_error_message(&error)
            ));
            return Ok(PasteResponse {
                command_sent: false,
                message: "无法备份原剪贴板，已停止自动粘贴，避免覆盖你的剪贴板内容。".to_string(),
                requires_accessibility: false,
                target_app: normalized_target_app,
                clipboard_written: false,
                clipboard_matches_expected: false,
                clipboard_restore_attempted: clipboard_restore_status.attempted,
                clipboard_restored: clipboard_restore_status.restored,
                clipboard_restore_message: clipboard_restore_status.message,
                accessibility_trusted,
                paste_method: "notSent".to_string(),
                frontmost_before_paste,
                frontmost_after_activate,
                frontmost_after_paste: String::new(),
                insertion_verified: false,
                verification_status: "原剪贴板备份失败，未写入临时剪贴板".to_string(),
                focused_element_before_paste,
                focused_element_after_activate,
                focused_element_after_paste: String::new(),
            });
        }
    };
    write_clipboard_text(normalized_text)?;
    let clipboard_written = true;
    let clipboard_matches_expected = true;

    hide_transient_voice_windows(&app);
    if paste_target.should_hide_hub {
        if let Some(window) = app.get_webview_window("hub") {
            let _ = window.hide();
        }
    }
    let paste_result = match trigger_system_paste() {
        Ok(value) => value,
        Err(error) => {
            let clipboard_restore_status = build_clipboard_restore_status(
                Some(()),
                restore_clipboard_snapshot(&clipboard_snapshot),
            );
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            return Err(format!("{}；{}", error, clipboard_restore_status.message));
        }
    };
    let mut paste_methods = vec![paste_result.method.clone()];
    thread::sleep(Duration::from_millis(CLIPBOARD_RESTORE_DELAY_MS));
    let clipboard_restore_status =
        build_clipboard_restore_status(Some(()), restore_clipboard_snapshot(&clipboard_snapshot));
    thread::sleep(Duration::from_millis(PASTE_DIAGNOSTIC_SETTLE_DELAY_MS));
    let frontmost_after_paste = get_frontmost_app().unwrap_or_default();
    let focused_element_after_paste =
        "直接粘贴模式：发送后未读取输入框正文，避免拖慢主链路。".to_string();
    let insertion_verified = false;
    if should_refocus_target {
        paste_methods.push("已恢复录音开始时的目标 App 后发送".to_string());
    } else {
        paste_methods.push("直接向当前焦点发送，不激活目标 App".to_string());
    }

    let verification_status = format!(
        "粘贴前已确认目标 App 未变化，并完成输入焦点门禁判断，已发出一次系统粘贴指令；{}；快速模式未回读目标输入框正文。",
        clipboard_restore_status.message
    );
    let paste_method = paste_methods.join(" -> ");
    let pasted = should_mark_paste_command_as_sent(
        paste_result.accessibility_ready,
        true,
        insertion_verified,
    );

    Ok(PasteResponse {
        command_sent: pasted,
        message: if clipboard_restore_status.restored {
            format!(
                "已向当前焦点发送一次粘贴指令，并已恢复原剪贴板。当前前台：{}。",
                normalized_target_app
            )
        } else {
            format!(
                "已向当前焦点发送一次粘贴指令，但原剪贴板恢复失败，请留意当前剪贴板内容。当前前台：{}。",
                normalized_target_app
            )
        },
        requires_accessibility: false,
        target_app: normalized_target_app,
        clipboard_written,
        clipboard_matches_expected,
        clipboard_restore_attempted: clipboard_restore_status.attempted,
        clipboard_restored: clipboard_restore_status.restored,
        clipboard_restore_message: clipboard_restore_status.message,
        accessibility_trusted,
        paste_method,
        frontmost_before_paste,
        frontmost_after_activate,
        frontmost_after_paste,
        insertion_verified,
        verification_status,
        focused_element_before_paste,
        focused_element_after_activate,
        focused_element_after_paste,
    })
}

/// 截断过长的上游错误，避免界面塞满响应体。
fn trim_error_message(message: &str) -> String {
    const MAX_ERROR_LENGTH: usize = 500;
    message.chars().take(MAX_ERROR_LENGTH).collect()
}

/// 获取 macOS 文本剪贴板类型。
#[cfg(target_os = "macos")]
fn pasteboard_string_type() -> &'static objc2_app_kit::NSPasteboardType {
    unsafe { NSPasteboardTypeString }
}

/// 捕获系统剪贴板完整快照，自动粘贴后用于恢复用户原本剪切或复制的内容。
#[cfg(target_os = "macos")]
fn capture_clipboard_snapshot() -> Result<ClipboardSnapshot, String> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        let Some(items) = pasteboard.pasteboardItems() else {
            return Ok(ClipboardSnapshot { items: Vec::new() });
        };
        let mut snapshot_items = Vec::new();
        for item in items.to_vec() {
            let mut representations = Vec::new();
            for pasteboard_type in item.types().to_vec() {
                if let Some(data) = item.dataForType(&pasteboard_type) {
                    representations.push(ClipboardRepresentationSnapshot {
                        type_name: pasteboard_type.to_string(),
                        data: data.to_vec(),
                    });
                }
            }
            if !representations.is_empty() {
                snapshot_items.push(ClipboardItemSnapshot { representations });
            }
        }
        Ok(ClipboardSnapshot {
            items: snapshot_items,
        })
    })
}

/// 非 macOS 平台当前没有自动粘贴能力，因此不需要捕获系统剪贴板。
#[cfg(not(target_os = "macos"))]
fn capture_clipboard_snapshot() -> Result<ClipboardSnapshot, String> {
    Ok(ClipboardSnapshot { items: Vec::new() })
}

/// 将系统剪贴板恢复为自动粘贴前的完整快照。
#[cfg(target_os = "macos")]
fn restore_clipboard_snapshot(snapshot: &ClipboardSnapshot) -> Result<(), String> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        if snapshot.items.is_empty() {
            return Ok(());
        }
        let mut pasteboard_items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> =
            Vec::new();
        for snapshot_item in &snapshot.items {
            let item = NSPasteboardItem::new();
            for representation in &snapshot_item.representations {
                let pasteboard_type = NSString::from_str(&representation.type_name);
                let data = NSData::with_bytes(&representation.data);
                if !item.setData_forType(&data, &pasteboard_type) {
                    return Err(format!("恢复剪贴板类型 {} 失败", representation.type_name));
                }
            }
            pasteboard_items.push(ProtocolObject::from_retained(item));
        }
        let objects = NSArray::from_retained_slice(&pasteboard_items);
        if !pasteboard.writeObjects(&objects) {
            return Err("写回原剪贴板快照失败".to_string());
        }
        Ok(())
    })
}

/// 非 macOS 平台当前没有自动粘贴能力，因此不执行剪贴板恢复。
#[cfg(not(target_os = "macos"))]
fn restore_clipboard_snapshot(_snapshot: &ClipboardSnapshot) -> Result<(), String> {
    Ok(())
}

/// 通过系统剪贴板 API 写入临时文本，避免把剪贴板内容经由前端暴露。
#[cfg(target_os = "macos")]
fn write_clipboard_text(text: &str) -> Result<(), String> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let value = NSString::from_str(text);
        if !pasteboard.setString_forType(&value, pasteboard_string_type()) {
            return Err("写入剪贴板失败：Pasteboard 拒绝写入文本".to_string());
        }
        Ok(())
    })
}

/// 非 macOS 平台通过 pbcopy 写入文本，主要用于保留编译兼容。
#[cfg(not(target_os = "macos"))]
fn write_clipboard_text(text: &str) -> Result<(), String> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("写入剪贴板失败：{}", error))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "写入剪贴板失败：无法打开输入管道".to_string())?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|error| format!("写入剪贴板失败：{}", error))?;
    drop(stdin);
    let status = child
        .wait()
        .map_err(|error| format!("等待剪贴板写入失败：{}", error))?;
    if !status.success() {
        return Err("写入剪贴板失败：pbcopy 执行失败".to_string());
    }
    Ok(())
}

/// 写入剪贴板后多次读回确认，避免 macOS 剪贴板短暂延迟导致误判。
fn write_clipboard_text_verified(text: &str) -> Result<bool, String> {
    for attempt in 0_u64..3 {
        write_clipboard_text(text)?;
        thread::sleep(Duration::from_millis(
            CLIPBOARD_VERIFY_INITIAL_DELAY_MS + attempt * CLIPBOARD_VERIFY_RETRY_STEP_MS,
        ));
        if read_clipboard_text_raw()
            .map(|clipboard_text| clipboard_text_matches_output(&clipboard_text, text))
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 判断剪贴板文本是否匹配本次输出，兼容系统工具可能追加的末尾换行。
fn clipboard_text_matches_output(clipboard_text: &str, output_text: &str) -> bool {
    clipboard_text == output_text || clipboard_text.trim_end_matches(['\n', '\r']) == output_text
}

/// 通过 macOS pbpaste 读取剪贴板原文，用于确认 pbcopy 写入是否真的生效。
#[cfg(target_os = "macos")]
fn read_clipboard_text_raw() -> Result<String, String> {
    let output = Command::new("pbpaste")
        .output()
        .map_err(|error| format!("读取剪贴板失败：{}", error))?;
    if !output.status.success() {
        return Err("读取剪贴板失败：pbpaste 执行失败".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 非 macOS 桌面端暂不支持读取剪贴板原文。
#[cfg(not(target_os = "macos"))]
fn read_clipboard_text_raw() -> Result<String, String> {
    Err("当前系统暂不支持读取剪贴板".to_string())
}

/// 触发系统级 Cmd+V；优先使用更接近物理按键的 CoreGraphics，再回退到 AppleScript。
#[cfg(target_os = "macos")]
fn trigger_system_paste() -> Result<PasteTriggerResult, String> {
    let accessibility_ready = is_accessibility_trusted();
    match trigger_core_graphics_paste(accessibility_ready) {
        Ok(_) => Ok(PasteTriggerResult {
            accessibility_ready,
            method: "CoreGraphics".to_string(),
        }),
        Err(_) => trigger_system_events_paste(accessibility_ready),
    }
}

/// 使用 AppleScript 触发 Cmd+V，作为 CoreGraphics 键盘事件创建失败时的兜底。
#[cfg(target_os = "macos")]
fn trigger_system_events_paste(accessibility_ready: bool) -> Result<PasteTriggerResult, String> {
    run_osascript(r#"tell application "System Events" to keystroke "v" using command down"#)?;
    Ok(PasteTriggerResult {
        accessibility_ready,
        method: "System Events".to_string(),
    })
}

/// 触发系统级 Cmd+C；用于从当前外部 App 读取选中文本。
#[cfg(target_os = "macos")]
fn trigger_system_copy() -> Result<PasteTriggerResult, String> {
    let accessibility_ready = is_accessibility_trusted();
    match trigger_core_graphics_shortcut("c", accessibility_ready) {
        Ok(method) => Ok(PasteTriggerResult {
            accessibility_ready,
            method,
        }),
        Err(_) => {
            run_osascript(
                r#"tell application "System Events" to keystroke "c" using command down"#,
            )?;
            Ok(PasteTriggerResult {
                accessibility_ready,
                method: "System Events".to_string(),
            })
        }
    }
}

/// 使用 CoreGraphics 触发 Cmd+V，尽量模拟真实键盘粘贴事件。
#[cfg(target_os = "macos")]
fn trigger_core_graphics_paste(accessibility_ready: bool) -> Result<PasteTriggerResult, String> {
    let method = trigger_core_graphics_shortcut("v", accessibility_ready)?;
    Ok(PasteTriggerResult {
        accessibility_ready,
        method,
    })
}

/// 使用 CoreGraphics 触发 Command + 指定字母快捷键。
#[cfg(target_os = "macos")]
fn trigger_core_graphics_shortcut(key: &str, _accessibility_ready: bool) -> Result<String, String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let key_code = match key {
        "c" => KeyCode::ANSI_C,
        "v" => KeyCode::ANSI_V,
        _ => return Err("系统快捷键失败：不支持的按键。".to_string()),
    };
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "系统快捷键失败：无法创建系统按键事件源".to_string())?;
    let key_down = CGEvent::new_keyboard_event(source.clone(), key_code, true)
        .map_err(|_| "系统快捷键失败：无法创建按下事件".to_string())?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::HID);

    thread::sleep(Duration::from_millis(24));

    let key_up = CGEvent::new_keyboard_event(source, key_code, false)
        .map_err(|_| "系统快捷键失败：无法创建松开事件".to_string())?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.post(CGEventTapLocation::HID);
    Ok("CoreGraphics".to_string())
}

/// 非 macOS 平台暂不支持系统级自动粘贴。
#[cfg(not(target_os = "macos"))]
fn trigger_system_paste() -> Result<PasteTriggerResult, String> {
    Err("自动粘贴失败：当前版本暂时只支持 macOS 自动粘贴".to_string())
}

/// 非 macOS 平台暂不支持系统级读取选中文本。
#[cfg(not(target_os = "macos"))]
fn trigger_system_copy() -> Result<PasteTriggerResult, String> {
    Err("读取选中文本失败：当前版本暂时只支持 macOS".to_string())
}

/// 查询当前进程是否已获得 macOS 辅助功能权限。
#[cfg(target_os = "macos")]
fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// 非 macOS 平台没有当前实现需要的辅助功能权限。
#[cfg(not(target_os = "macos"))]
fn is_accessibility_trusted() -> bool {
    false
}

/// 打开 macOS 辅助功能设置页。
#[cfg(target_os = "macos")]
fn open_accessibility_preferences() -> Result<(), String> {
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|error| format!("打开辅助功能设置失败：{}", error))?;
    Ok(())
}

/// 非 macOS 平台暂不支持打开对应权限页。
#[cfg(not(target_os = "macos"))]
fn open_accessibility_preferences() -> Result<(), String> {
    Err("当前版本只支持在 macOS 打开辅助功能设置".to_string())
}

/// 打开 macOS 麦克风隐私设置页。
#[cfg(target_os = "macos")]
fn open_microphone_preferences() -> Result<(), String> {
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn()
        .map_err(|error| format!("打开麦克风设置失败：{}", error))?;
    Ok(())
}

/// 非 macOS 平台暂不支持打开对应麦克风权限页。
#[cfg(not(target_os = "macos"))]
fn open_microphone_preferences() -> Result<(), String> {
    Err("当前版本只支持在 macOS 打开麦克风设置".to_string())
}

/// 扫描 macOS 常见应用目录并返回 .app bundle。
/// 流程：依次读取系统、用户和系统应用目录，按 bundle 文件名生成展示名称并按路径去重。
/// 返回：按应用名排序后的应用选项。
/// 异常/边界：单个目录无权限或不存在时跳过；全部扫描失败时返回空列表而不是中断页面。
#[cfg(target_os = "macos")]
fn list_installed_applications_core() -> Result<Vec<ApplicationOption>, String> {
    let mut directories = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Ok(home) = env::var("HOME") {
        directories.push(PathBuf::from(home).join("Applications"));
    }
    let mut seen_paths = HashSet::new();
    let mut applications = Vec::new();
    for directory in directories {
        if !directory.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("app") {
                continue;
            }
            let path_text = path.to_string_lossy().to_string();
            if !seen_paths.insert(path_text.clone()) {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|file_stem| file_stem.to_str())
                .unwrap_or("未知应用")
                .to_string();
            applications.push(ApplicationOption {
                name,
                path: path_text,
            });
        }
    }
    applications.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(applications)
}

/// 非 macOS 平台暂不支持扫描应用目录。
#[cfg(not(target_os = "macos"))]
fn list_installed_applications_core() -> Result<Vec<ApplicationOption>, String> {
    Err("当前版本只支持在 macOS 选择 APP".to_string())
}

/// 打开指定应用 bundle。
/// 流程：校验路径指向 .app 后用系统 open 命令启动或激活目标应用。
/// 参数：app_path 为 .app bundle 绝对路径。
/// 返回：open 命令发起成功时返回空值。
/// 异常/边界：非 .app、路径不存在或系统命令失败时返回明确错误。
#[cfg(target_os = "macos")]
fn open_application_bundle(app_path: &str) -> Result<(), String> {
    let path = PathBuf::from(app_path.trim());
    if path.extension().and_then(|extension| extension.to_str()) != Some("app") || !path.exists() {
        return Err("目标 APP 不存在或不是有效的 .app 应用".to_string());
    }
    Command::new("open")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("打开 APP 失败：{}", error))?;
    Ok(())
}

/// 非 macOS 平台暂不支持打开应用 bundle。
#[cfg(not(target_os = "macos"))]
fn open_application_bundle(_app_path: &str) -> Result<(), String> {
    Err("当前版本只支持在 macOS 打开 APP".to_string())
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 任务执行诊断只能保留完整固定的内部错误码，其它相似文本必须降级为通用执行失败。
    #[test]
    fn task_execution_diagnostic_code_preserves_only_exact_internal_marker() {
        assert_eq!(
            task_execution_diagnostic_code(
                "Codex Desktop 主页面地址无效。（错误码：CODEX_CDP_TARGET_INVALID）"
            ),
            "CODEX_CDP_TARGET_INVALID"
        );
        assert_eq!(
            task_execution_diagnostic_code("CODEX_CDP_TARGET_INVALID"),
            "TASK_EXECUTION_FAILED"
        );
        assert_eq!(
            task_execution_diagnostic_code("（错误码：CODEX_CDP_TARGET_INVALID_EXTRA）"),
            "TASK_EXECUTION_FAILED"
        );
    }

    /// 创建当前测试独占的系统临时目录。
    /// 流程：用测试名和随机 UUID 组成不可碰撞路径并创建目录；参数为便于人工识别的测试名。
    /// 返回：已创建目录路径；异常/边界：仅供测试使用，调用测试结束时必须删除该精确路径，禁止指向工作区或用户目录。
    fn create_test_temp_dir(test_name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!("codexman-{}-{}", test_name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("应创建测试临时目录");
        path
    }

    /// 首发配置只接受当前格式版本，未知版本不得触发历史迁移或静默回退。
    #[test]
    fn local_config_rejects_unknown_schema_version() {
        let error = parse_local_config_document(r#"{"version":2,"updatedAt":"","items":{}}"#)
            .expect_err("未知配置版本必须被拒绝");
        assert!(error.contains("版本不受支持"));
    }

    /// app-server completed turn 才能解析为待验收输入，并保留最终助手文本。
    #[test]
    fn codex_completed_turn_is_a_reliable_terminal_state() {
        let turn = json!({
            "id": "turn-1",
            "status": "completed",
            "completedAt": 123,
            "items": [{"type": "agentMessage", "text": "完成结果"}]
        });
        let (status, result, error) =
            parse_codex_terminal_turn(&turn).expect("completed 应为可靠终态");
        assert_eq!(status, "completed");
        assert!(result.contains("完成结果"));
        assert!(error.is_empty());
    }

    /// inProgress 不能被伪装成完成或失败。
    #[test]
    fn codex_in_progress_turn_is_not_terminal() {
        assert!(parse_codex_terminal_turn(&json!({
            "id": "turn-1",
            "status": "inProgress",
            "items": []
        }))
        .is_err());
    }

    /// 本地 turnId 缺失时只能认领任务专用 thread 的唯一 turn，用于闭合 turn/start 响应落库前的崩溃窗口。
    #[test]
    fn reconciliation_recovers_only_turn_without_client_message_query() {
        let turns = vec![json!({
            "id": "turn-recovered",
            "status": "inProgress",
            "items": [{"type": "userMessage", "clientId": "opaque-client-id"}]
        })];
        let recovered = select_codex_turn_for_reconciliation(&turns, "")
            .expect("专用 thread 的唯一 turn 应可恢复")
            .expect("唯一 turn 应存在");
        assert_eq!(
            recovered.get("id").and_then(Value::as_str),
            Some("turn-recovered")
        );
    }

    /// 缺少 turnId 且专用 thread 尚无 turn 时，单次读取只能保持等待，不能立即失败或重放 prompt。
    #[test]
    fn reconciliation_waits_when_dedicated_thread_has_no_turn() {
        assert!(select_codex_turn_for_reconciliation(&[], "")
            .expect("零 turn 是可继续轮询的状态")
            .is_none());
    }

    /// 专用 thread 必须连续五次成功读取为空才确认没有持久化 turn，任一非空结果会重置一致性窗口。
    #[test]
    fn empty_thread_requires_consecutive_consistency_confirmations() {
        let mut confirmations = 0;
        for _ in 0..(CODEX_EMPTY_THREAD_CONFIRMATIONS - 1) {
            let (next, is_absent) = advance_empty_thread_confirmation(confirmations, true);
            confirmations = next;
            assert!(!is_absent);
        }
        let (reset, is_absent) = advance_empty_thread_confirmation(confirmations, false);
        assert_eq!(reset, 0);
        assert!(!is_absent);

        confirmations = 0;
        for index in 1..=CODEX_EMPTY_THREAD_CONFIRMATIONS {
            let (next, is_absent) = advance_empty_thread_confirmation(confirmations, true);
            confirmations = next;
            assert_eq!(is_absent, index == CODEX_EMPTY_THREAD_CONFIRMATIONS);
        }
    }

    /// 专用 thread 出现多个 turn 时归属已不可证明，必须拒绝猜测任一 turn。
    #[test]
    fn reconciliation_rejects_ambiguous_turns() {
        let turns = vec![
            json!({"id": "turn-a", "status": "completed", "items": []}),
            json!({"id": "turn-b", "status": "inProgress", "items": []}),
        ];
        assert!(select_codex_turn_for_reconciliation(&turns, "").is_err());
    }

    /// 并发或迟到通知必须同时匹配 threadId 与 turnId，不能完成其它本地任务。
    #[test]
    fn codex_terminal_notification_isolated_by_thread_and_turn() {
        let notification = json!({
            "method": "turn/completed",
            "params": {
                "threadId": "thread-a",
                "turn": {"id": "turn-a", "status": "completed", "items": []}
            }
        });
        assert!(
            parse_matching_terminal_notification(&notification, "thread-b", "turn-a")
                .expect("无关 thread 应被忽略")
                .is_none()
        );
        assert!(
            parse_matching_terminal_notification(&notification, "thread-a", "turn-b")
                .expect("无关 turn 应被忽略")
                .is_none()
        );
        let matched = parse_matching_terminal_notification(&notification, "thread-a", "turn-a")
            .expect("目标通知应可解析")
            .expect("目标通知应返回终态");
        assert_eq!(matched.0, "completed");
    }

    /// Codex stdout JSONL 帧必须接受恰好上限的 CRLF 数据，并拒绝多一个字节且不回显正文。
    #[test]
    fn codex_stdout_jsonl_frame_is_strictly_bounded() {
        let mut exact_reader = std::io::Cursor::new(b"12345678\r\n".to_vec());
        let exact = read_bounded_codex_jsonl_frame(&mut exact_reader, 8)
            .expect("恰好八字节帧应允许")
            .expect("应返回帧");
        assert_eq!(exact, b"12345678");

        let secret_body = b"secret-123";
        let mut oversized_reader = std::io::Cursor::new([secret_body.as_slice(), b"\n"].concat());
        let error = read_bounded_codex_jsonl_frame(&mut oversized_reader, 8)
            .expect_err("超过一字节必须拒绝");
        assert!(error.contains("CODEX_STDOUT_FRAME_TOO_LARGE"));
        assert!(!error.contains("secret"));
    }

    /// Codex stdout 在完整 EOF 前没有换行时仍允许解析有界末帧，空 EOF 返回 None。
    #[test]
    fn codex_stdout_jsonl_accepts_bounded_final_frame() {
        let mut final_reader = std::io::Cursor::new(b"{}".to_vec());
        assert_eq!(
            read_bounded_codex_jsonl_frame(&mut final_reader, 2).expect("末帧应读取"),
            Some(b"{}".to_vec())
        );
        assert!(read_bounded_codex_jsonl_frame(&mut final_reader, 2)
            .expect("EOF 应正常")
            .is_none());
    }

    /// app-server stderr 消费器必须丢弃正文，并在超长记录上只返回截断标记且保持常量内存。
    #[test]
    fn codex_stderr_records_are_consumed_without_exposing_content() {
        let mut normal_reader = std::io::Cursor::new(b"secret prompt /private/path\n".to_vec());
        assert_eq!(
            consume_codex_stderr_record(&mut normal_reader, 1024).expect("普通记录应被消费"),
            Some(false)
        );
        assert_eq!(normal_reader.position(), 28);

        let mut oversized_reader = std::io::Cursor::new(vec![b'x'; 4097]);
        assert_eq!(
            consume_codex_stderr_record(&mut oversized_reader, 4096).expect("超长末记录应被消费"),
            Some(true)
        );
        assert_eq!(oversized_reader.position(), 4097);
    }

    /// app-server 诊断日志每次追加前必须轮转，并且活动日志只包含固定诊断码。
    #[test]
    fn codex_diagnostic_log_is_realtime_bounded_and_redacted() {
        let temp_dir = create_test_temp_dir("codex-diagnostic");
        let log_path = temp_dir.join("codexman-codex-app-server.log");
        let existing = fs::File::create(&log_path).expect("应创建测试日志");
        existing
            .set_len(CODEX_APP_SERVER_DIAGNOSTIC_LOG_MAX_BYTES)
            .expect("应构造达到上限的稀疏日志");

        append_codex_diagnostic(&log_path, "CODEX_APP_SERVER_STDERR_REPORTED");

        let active_content = fs::read_to_string(&log_path).expect("应读取轮转后的活动日志");
        assert_eq!(active_content, "CODEX_APP_SERVER_STDERR_REPORTED\n");
        assert!(
            fs::metadata(log_path.with_extension("log.1"))
                .expect("应保留单个备份")
                .len()
                <= CODEX_APP_SERVER_DIAGNOSTIC_LOG_MAX_BYTES
        );
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// Codex 会话索引必须在读入前拒绝超大文件，并始终只保留最新 500 条有效记录。
    #[test]
    fn codex_session_index_is_size_and_entry_bounded() {
        let temp_dir = create_test_temp_dir("codex-index");
        let oversized_path = temp_dir.join("oversized.jsonl");
        let oversized = fs::File::create(&oversized_path).expect("应创建超大索引占位文件");
        oversized
            .set_len(CODEX_SESSION_INDEX_MAX_BYTES + 1)
            .expect("应构造超限稀疏索引");
        let error =
            read_codex_session_index_path(&oversized_path).expect_err("超大索引必须在读入前拒绝");
        assert!(error.contains("CODEX_SESSION_INDEX_TOO_LARGE"));

        let bounded_path = temp_dir.join("bounded.jsonl");
        let mut bounded = fs::File::create(&bounded_path).expect("应创建有界索引");
        for index in 0..=CODEX_SESSION_INDEX_ENTRY_LIMIT {
            writeln!(
                bounded,
                "{}",
                json!({
                    "id": format!("thread-{index}"),
                    "thread_name": format!("任务 {index}"),
                    "updated_at": format!("{index:04}")
                })
            )
            .expect("应写入索引记录");
        }
        drop(bounded);
        let summaries = read_codex_session_index_path(&bounded_path).expect("有界索引应读取");
        assert_eq!(summaries.len(), CODEX_SESSION_INDEX_ENTRY_LIMIT);
        assert!(!summaries.iter().any(|summary| summary.id == "thread-0"));
        assert!(summaries.iter().any(|summary| summary.id == "thread-500"));

        let growing_path = temp_dir.join("growing-index.jsonl");
        fs::write(&growing_path, b"12345678").expect("应创建恰好达到测试上限的索引");
        let exact = fs::File::open(&growing_path).expect("应打开恰好上限索引");
        assert_eq!(
            read_bounded_codex_file(exact, 8, "TEST_TOO_LARGE", "测试索引")
                .expect("恰好上限应允许读取")
                .len(),
            8
        );
        let opened_before_growth = fs::File::open(&growing_path).expect("应在增长前打开索引句柄");
        fs::OpenOptions::new()
            .append(true)
            .open(&growing_path)
            .expect("应打开索引增长句柄")
            .write_all(b"9")
            .expect("应模拟索引在打开后增长");
        let growth_error =
            read_bounded_codex_file(opened_before_growth, 8, "TEST_TOO_LARGE", "测试索引")
                .expect_err("打开后增长仍必须由句柄读取预算拒绝");
        assert!(growth_error.contains("TEST_TOO_LARGE"));
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// sessions 递归枚举达到文件数上限时必须保留预算内候选，不得让 supplemental 整批失败。
    #[test]
    fn codex_session_enumeration_stops_at_file_count_limit() {
        let temp_dir = create_test_temp_dir("codex-session-count");
        for index in 0..=CODEX_SESSION_FILE_ENUM_LIMIT {
            fs::File::create(temp_dir.join(format!("rollout-{index}.jsonl")))
                .expect("应创建会话占位文件");
        }
        let files =
            collect_codex_session_files(&temp_dir).expect("达到文件数量上限应返回已收集候选");
        assert_eq!(files.len(), CODEX_SESSION_FILE_ENUM_LIMIT);
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// sessions 递归枚举达到总字节上限时必须停止并保留预算内候选，即使下一个文件仍满足单文件限制。
    #[test]
    fn codex_session_enumeration_stops_at_total_byte_limit() {
        let temp_dir = create_test_temp_dir("codex-session-total-bytes");
        let required_files =
            (CODEX_SESSION_TOTAL_BYTES_LIMIT / CODEX_SESSION_FILE_MAX_BYTES) as usize + 1;
        for index in 0..required_files {
            let file = fs::File::create(temp_dir.join(format!("rollout-total-{index}.jsonl")))
                .expect("应创建总量测试占位文件");
            file.set_len(CODEX_SESSION_FILE_MAX_BYTES)
                .expect("应构造单文件上限内的稀疏文件");
        }
        let files = collect_codex_session_files(&temp_dir).expect("达到总字节上限应返回已收集候选");
        assert_eq!(
            files.len(),
            (CODEX_SESSION_TOTAL_BYTES_LIMIT / CODEX_SESSION_FILE_MAX_BYTES) as usize
        );
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// supplemental 扫描失败时必须保留已合法读取的官方 index，不得把列表整次降级为错误。
    #[test]
    fn codex_thread_index_keeps_legal_entries_when_supplemental_fails() {
        let indexed = CodexThreadSummary {
            id: "indexed-thread".to_string(),
            title: "官方索引任务".to_string(),
            parent_thread_id: String::new(),
            depth: 0,
            agent_nickname: String::new(),
            agent_role: String::new(),
            updated_at: "2026-08-10T10:00:00Z".to_string(),
        };
        let merged = merge_codex_thread_summaries(
            vec![indexed],
            Err("CODEX_SESSION_TOTAL_BYTES_EXCEEDED".to_string()),
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "indexed-thread");
        assert_eq!(merged[0].title, "官方索引任务");
    }

    /// Deeplink 前的权威状态查询必须只接受精确且未归档的 thread ID。
    #[test]
    fn codex_thread_existence_requires_exact_unarchived_state_row() {
        let connection = Connection::open_in_memory().expect("应创建内存状态库");
        connection
            .execute_batch(
                "
                CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER NOT NULL);
                INSERT INTO threads (id, archived) VALUES
                    ('thread-live', 0),
                    ('thread-archived', 1);
                ",
            )
            .expect("应准备 Codex thread 索引");

        assert!(
            codex_thread_exists_in_state_connection(&connection, "thread-live")
                .expect("应精确查询活动会话")
        );
        assert!(
            !codex_thread_exists_in_state_connection(&connection, "thread-archived")
                .expect("已归档会话应返回不存在")
        );
        assert!(
            !codex_thread_exists_in_state_connection(&connection, "thread")
                .expect("ID 前缀不得误命中")
        );
        assert!(
            !codex_thread_exists_in_state_connection(&connection, "thread-missing")
                .expect("未知 ID 应返回不存在")
        );
    }

    /// supplemental 枚举遇到单个超大候选时必须跳过该文件并继续保留其它合法候选。
    #[test]
    fn codex_session_enumeration_skips_oversized_candidate() {
        let temp_dir = create_test_temp_dir("codex-session-skip-large");
        let oversized_path = temp_dir.join("oversized.jsonl");
        fs::File::create(&oversized_path)
            .expect("应创建超大候选")
            .set_len(CODEX_SESSION_FILE_MAX_BYTES + 1)
            .expect("应设置超大候选长度");
        let valid_path = temp_dir.join("valid.jsonl");
        fs::write(&valid_path, b"{}\n").expect("应创建合法候选");

        let files = collect_codex_session_files(&temp_dir).expect("超大单文件不应使补充扫描失败");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, valid_path);
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// supplemental 超过 500 个 JSONL 时必须优先保留最近修改文件，同时精确详情读取不受预算截断。
    #[test]
    fn codex_supplemental_keeps_recent_target_after_file_count_limit() {
        let temp_dir = create_test_temp_dir("codex-exact-after-supplemental");
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir(&sessions_dir).expect("应创建 sessions 测试目录");
        let target_thread_id = "target-thread-after-500";
        let index_path = temp_dir.join("session_index.jsonl");
        fs::write(
            &index_path,
            format!(
                "{}\n",
                json!({
                    "id": target_thread_id,
                    "thread_name": "索引可见目标",
                    "updated_at": "2026-08-10T12:00:00Z"
                })
            ),
        )
        .expect("应写入合法官方 index");
        for index in 0..CODEX_SESSION_FILE_ENUM_LIMIT {
            fs::write(
                sessions_dir.join(format!("rollout-distractor-{index:04}.jsonl")),
                b"{}\n",
            )
            .expect("应写入 supplemental 干扰文件");
        }
        let target_dir = sessions_dir.join("target-after-supplemental-budget");
        fs::create_dir(&target_dir).expect("应创建预算之后的目标目录");
        let target_path = target_dir.join(format!("rollout-{target_thread_id}.jsonl"));
        thread::sleep(Duration::from_millis(5));
        fs::write(
            &target_path,
            format!(
                "{}\n",
                json!({
                    "timestamp": "2026-08-10T12:00:00Z",
                    "type": "event_msg",
                    "payload": {"type": "user_message", "message": "target detail"}
                })
            ),
        )
        .expect("应写入第 500 个候选后的目标会话");

        let indexed = read_codex_session_index_path(&index_path).expect("官方 index 应合法可见");
        assert!(indexed.iter().any(|thread| thread.id == target_thread_id));
        let supplemental = collect_codex_session_files(&sessions_dir)
            .expect("supplemental 达到文件预算应返回已有候选");
        assert_eq!(supplemental.len(), CODEX_SESSION_FILE_ENUM_LIMIT);
        assert!(
            supplemental.iter().any(|(path, _)| path == &target_path),
            "最近修改的目标 JSONL 不应被目录枚举顺序挤出 supplemental 候选"
        );
        let found = find_codex_session_file_in_dir(
            &sessions_dir,
            target_thread_id,
            CODEX_SESSION_DIRECTORY_LIMIT,
            CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT,
        )
        .expect("精确查找不得受 supplemental 500 文件预算截断");
        assert_eq!(found, target_path);
        let messages = read_codex_session_messages(&found).expect("目标会话详情应可读取");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "target detail");
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// 精确 thread ID 查找必须按 rollout 文件名边界匹配，短 ID 不得命中带额外后缀的其它会话。
    #[test]
    fn codex_exact_session_lookup_rejects_thread_id_prefix_collisions() {
        let temp_dir = create_test_temp_dir("codex-exact-id-collision");
        for (file_name, message) in [
            ("abc-extra.jsonl", "wrong extra detail"),
            ("prefix-abc-extra.jsonl", "wrong prefixed extra detail"),
            ("abc.jsonl", "target abc detail"),
        ] {
            fs::write(
                temp_dir.join(file_name),
                format!(
                    "{}\n",
                    json!({
                        "timestamp": "2026-08-10T12:00:00Z",
                        "type": "event_msg",
                        "payload": {"type": "user_message", "message": message}
                    })
                ),
            )
            .expect("应写入 thread ID 碰撞测试会话");
        }

        let found = find_codex_session_file_in_dir(&temp_dir, "abc", 10, 10)
            .expect("短 thread ID 应只命中文件名边界完整目标");
        assert_eq!(
            found.file_name().and_then(|name| name.to_str()),
            Some("abc.jsonl")
        );
        let messages = read_codex_session_messages(&found).expect("应读取精确目标详情");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "target abc detail");
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// 精确 thread ID 查找必须在目录条目预算耗尽时返回稳定错误码，而不是无界遍历。
    #[test]
    fn codex_exact_session_lookup_enforces_entry_limit() {
        let temp_dir = create_test_temp_dir("codex-exact-entry-limit");
        let zero_budget_error = find_codex_session_file_in_dir(&temp_dir, "missing-thread", 10, 0)
            .expect_err("零条目预算必须直接失败");
        assert!(zero_budget_error.contains("CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT_EXCEEDED"));
        for index in 0..3 {
            fs::write(temp_dir.join(format!("rollout-{index}.jsonl")), b"{}\n")
                .expect("应创建条目预算测试文件");
        }
        let error = find_codex_session_file_in_dir(&temp_dir, "missing-thread", 10, 2)
            .expect_err("超过精确查找条目预算必须失败");
        assert!(error.contains("CODEX_SESSION_EXACT_SEARCH_ENTRY_LIMIT_EXCEEDED"));
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// 精确 thread ID 查找必须在目录预算耗尽时返回稳定错误码，并拒绝匹配符号链接。
    #[test]
    fn codex_exact_session_lookup_enforces_directory_limit_and_rejects_symlink() {
        let temp_dir = create_test_temp_dir("codex-exact-directory-limit");
        let zero_budget_error = find_codex_session_file_in_dir(&temp_dir, "missing-thread", 0, 10)
            .expect_err("零目录预算必须直接失败");
        assert!(zero_budget_error.contains("CODEX_SESSION_DIRECTORY_LIMIT_EXCEEDED"));
        fs::create_dir(temp_dir.join("nested")).expect("应创建目录预算测试目录");
        let error = find_codex_session_file_in_dir(&temp_dir, "missing-thread", 1, 10)
            .expect_err("超过精确查找目录预算必须失败");
        assert!(error.contains("CODEX_SESSION_DIRECTORY_LIMIT_EXCEEDED"));

        #[cfg(unix)]
        {
            let backing_path = temp_dir.join("backing.jsonl");
            fs::write(&backing_path, b"{}\n").expect("应创建符号链接目标");
            let link_path = temp_dir.join("rollout-symlink-thread.jsonl");
            std::os::unix::fs::symlink(&backing_path, &link_path).expect("应创建会话符号链接");
            let link_error = find_codex_session_file_in_dir(&temp_dir, "symlink-thread", 10, 10)
                .expect_err("匹配符号链接必须拒绝");
            assert!(link_error.contains("CODEX_SESSION_FILE_INVALID"));
        }
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// 单个 Codex session 文件必须在打开前校验大小，单帧超限错误不得包含事件正文。
    #[test]
    fn codex_session_file_and_frame_are_bounded_before_parsing() {
        let temp_dir = create_test_temp_dir("codex-session-size");
        let session_path = temp_dir.join("rollout-test.jsonl");
        let oversized = fs::File::create(&session_path).expect("应创建会话占位文件");
        oversized
            .set_len(CODEX_SESSION_FILE_MAX_BYTES + 1)
            .expect("应构造超大稀疏会话文件");
        let error =
            validate_codex_session_file(&session_path).expect_err("超大文件必须在打开正文前拒绝");
        assert!(error.contains("CODEX_SESSION_FILE_TOO_LARGE"));

        let mut frame = vec![b's'; CODEX_SESSION_FRAME_MAX_BYTES + 1];
        frame.push(b'\n');
        let mut reader = std::io::Cursor::new(frame);
        let frame_error =
            read_bounded_codex_session_frame(&mut reader).expect_err("超大单帧必须拒绝");
        assert!(frame_error.contains("CODEX_SESSION_FRAME_TOO_LARGE"));
        assert!(!frame_error.contains("ssss"));

        let growing_path = temp_dir.join("rollout-growing.jsonl");
        fs::write(
            &growing_path,
            format!(
                "{}\n",
                json!({
                    "timestamp": "0001",
                    "type": "event_msg",
                    "payload": {"type": "user_message", "message": "before-growth"}
                })
            ),
        )
        .expect("应创建初始合法会话");
        let opened =
            open_bounded_codex_session_file(&growing_path, "测试").expect("增长前句柄应通过校验");
        fs::OpenOptions::new()
            .append(true)
            .open(&growing_path)
            .expect("应重新打开增长句柄")
            .set_len(CODEX_SESSION_FILE_MAX_BYTES + 1)
            .expect("应模拟文件在打开后增长");
        let growth_error = read_codex_session_messages_from_file(opened)
            .expect_err("打开后增长的会话仍必须受句柄读取预算限制");
        assert!(
            growth_error.contains("CODEX_SESSION_FILE_TOO_LARGE")
                || growth_error.contains("CODEX_SESSION_FRAME_TOO_LARGE")
        );
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// Codex 会话详情流式扫描时必须只保留最后 80 条可展示消息，不能因长会话累积无界内存。
    #[test]
    fn codex_session_message_loading_keeps_latest_eighty() {
        let temp_dir = create_test_temp_dir("codex-session-messages");
        let session_path = temp_dir.join("rollout-messages.jsonl");
        let mut session = fs::File::create(&session_path).expect("应创建会话测试文件");
        for index in 0..=CODEX_THREAD_MESSAGE_LIMIT {
            writeln!(
                session,
                "{}",
                json!({
                    "timestamp": format!("{index:04}"),
                    "type": "event_msg",
                    "payload": {"type": "user_message", "message": format!("message-{index}")}
                })
            )
            .expect("应写入会话消息");
        }
        drop(session);

        let messages = read_codex_session_messages(&session_path).expect("有界会话应读取");
        assert_eq!(messages.len(), CODEX_THREAD_MESSAGE_LIMIT);
        assert_eq!(
            messages.first().map(|message| message.content.as_str()),
            Some("message-1")
        );
        assert_eq!(
            messages.last().map(|message| message.content.as_str()),
            Some("message-80")
        );
        fs::remove_dir_all(&temp_dir).expect("应删除精确测试临时目录");
    }

    /// 任务对账只能从同一 turn 的 task_complete 恢复最终回复，不能串用同文件其它 turn 的助手消息。
    #[test]
    fn codex_task_complete_result_recovery_matches_exact_turn() {
        let temp_dir = create_test_temp_dir("codex-task-complete-recovery");
        let session_path = temp_dir.join("rollout-recovery.jsonl");
        let mut session = fs::File::create(&session_path).expect("应创建任务完成事件测试文件");
        for (turn_id, message) in [
            ("turn-other", "其它 turn 回复"),
            ("turn-target", "目标 turn 最终回复"),
        ] {
            writeln!(
                session,
                "{}",
                json!({
                    "timestamp": "2026-08-12T10:00:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "task_complete",
                        "turn_id": turn_id,
                        "last_agent_message": message
                    }
                })
            )
            .expect("应写入 task_complete 事件");
        }
        drop(session);

        let recovered = read_codex_turn_completion_state(&session_path, "turn-target")
            .expect("应读取任务完成事件");
        assert!(matches!(
            recovered,
            CodexTurnCompletionState::Completed(ref message) if message == "目标 turn 最终回复"
        ));
        let missing = read_codex_turn_completion_state(&session_path, "turn-missing")
            .expect("缺失 turn 不应报错");
        assert!(matches!(missing, CodexTurnCompletionState::Pending));
        fs::remove_dir_all(&temp_dir).expect("应删除任务完成事件测试目录");
    }

    /// interrupted 对账必须等到 JSONL 出现 task_complete 或 turn_aborted，避免过早把仍在运行的 turn 标成失败。
    #[test]
    fn codex_turn_completion_state_waits_until_complete_or_aborted() {
        let temp_dir = create_test_temp_dir("codex-turn-completion-state");
        let session_path = temp_dir.join("rollout-state.jsonl");
        fs::write(
            &session_path,
            format!(
                "{}\n",
                json!({
                    "timestamp": "2026-08-12T10:00:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "agent_message",
                        "message": "还在输出",
                        "phase": "commentary"
                    }
                })
            ),
        )
        .expect("应写入尚未完成的会话");
        let pending = read_codex_turn_completion_state(&session_path, "turn-target")
            .expect("尚未完成时应可读取");
        assert!(matches!(pending, CodexTurnCompletionState::Pending));

        fs::write(
            &session_path,
            format!(
                "{}\n",
                json!({
                    "timestamp": "2026-08-12T10:00:01Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "turn_aborted",
                        "turn_id": "turn-target",
                        "reason": "interrupted"
                    }
                })
            ),
        )
        .expect("应写入中止事件");
        let aborted = read_codex_turn_completion_state(&session_path, "turn-target")
            .expect("中止事件应可读取");
        assert!(matches!(aborted, CodexTurnCompletionState::Aborted));
        fs::remove_dir_all(&temp_dir).expect("应删除任务完成状态测试目录");
    }

    /// interrupted 且 thread/read 没带 agentMessage 时，应把同 turn task_complete 的最终回复补回 resultJson。
    #[test]
    fn interrupted_result_can_be_completed_from_task_complete_message() {
        let result_json = json!({
            "turnId": "turn-target",
            "status": "interrupted",
            "completedAt": null,
            "finalText": ""
        })
        .to_string();
        let patched = with_codex_terminal_result_final_text(&result_json, "目标 turn 最终回复")
            .expect("应补写最终回复");
        let value = serde_json::from_str::<Value>(&patched).expect("补写后应仍是 JSON");
        assert_eq!(
            value.get("finalText").and_then(Value::as_str),
            Some("目标 turn 最终回复")
        );
        assert_eq!(
            value.get("finalTextSource").and_then(Value::as_str),
            Some("sessionTaskComplete")
        );
        assert_eq!(
            value.get("finalText").and_then(Value::as_str),
            Some("目标 turn 最终回复")
        );
    }

    /// interrupted 即使 thread/read items 中已有 commentary，也必须等待 task_complete，避免半截回复进入待验收。
    #[test]
    fn interrupted_result_with_intermediate_text_still_waits_for_task_complete() {
        let temp_dir = create_test_temp_dir("codex-interrupted-intermediate-text");
        let session_path = temp_dir.join("rollout-intermediate.jsonl");
        fs::write(
            &session_path,
            format!(
                "{}\n",
                json!({
                    "timestamp": "2026-08-12T10:00:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "agent_message",
                        "message": "中间回复",
                        "phase": "commentary"
                    }
                })
            ),
        )
        .expect("应写入中间回复");
        let completion = read_codex_turn_completion_state(&session_path, "turn-target")
            .expect("中间回复不应阻断读取");
        assert!(matches!(completion, CodexTurnCompletionState::Pending));
        fs::remove_dir_all(&temp_dir).expect("应删除中间回复测试目录");
    }

    /// Token 续签与清除 IPC 必须共享稳定桌面错误码，由 desktop_error 统一附加诊断 ID。
    #[test]
    fn public_api_token_ipc_uses_stable_desktop_error_code() {
        assert_eq!(PUBLIC_API_TOKEN_IPC_ERROR_CODE, "DESKTOP_OPERATION_FAILED");
    }

    /// 并发 401 的后到请求必须复用先到请求写入的新 Token，只有旧值仍匹配时才交换一次。
    #[test]
    fn public_api_token_refresh_is_single_flight_under_token_lock() {
        let token = Mutex::new("old-token".to_string());
        let mut first_exchange_count = 0;
        let refreshed = refresh_public_api_token_value_if_matches(&token, "old-token", || {
            first_exchange_count += 1;
            Ok("new-token".to_string())
        })
        .expect("匹配旧 Token 时应续签");
        assert_eq!(refreshed, "new-token");
        assert_eq!(first_exchange_count, 1);

        let mut late_exchange_count = 0;
        let reused = refresh_public_api_token_value_if_matches(&token, "old-token", || {
            late_exchange_count += 1;
            Ok("unexpected-token".to_string())
        })
        .expect("迟到 401 应直接复用新 Token");
        assert_eq!(reused, "new-token");
        assert_eq!(late_exchange_count, 0);
    }

    /// 验证迟到的 401 只能清除自己使用的旧 Token。
    /// 流程：先写入已续签值，使用旧值比较应保留，再用当前值比较应完成清除。
    /// 参数：无。
    /// 返回：无；任一步不符合原子 compare-and-clear 契约时测试失败。
    #[test]
    fn stale_unauthorized_response_does_not_clear_renewed_public_api_token() {
        let state = RuntimePublicApiToken::default();
        *state.token.lock().expect("测试 Token 锁应可用") = "renewed-token".to_string();

        assert!(
            !clear_public_api_token_value_if_matches(&state.token, "stale-token")
                .expect("旧 Token 比较不应失败")
        );
        assert_eq!(
            state.token.lock().expect("测试 Token 锁应可用").as_str(),
            "renewed-token"
        );
        assert!(
            clear_public_api_token_value_if_matches(&state.token, "renewed-token")
                .expect("当前 Token 应可原子清除")
        );
        assert!(state.token.lock().expect("测试 Token 锁应可用").is_empty());
    }

    /// 敏感 Token IPC 必须仅接受 hub 标签，所有其它 WebView 标签都按默认拒绝处理。
    #[test]
    fn public_api_token_window_access_is_hub_only() {
        assert!(ensure_public_api_token_window("hub").is_ok());
        assert!(ensure_public_api_token_window("float").is_err());
        assert!(ensure_public_api_token_window("result").is_err());
        assert!(ensure_public_api_token_window("").is_err());
    }

    /// 保留的模型、Token 等敏感管理 IPC 必须只接受 hub，任何其它或未知标签都默认拒绝。
    #[test]
    fn sensitive_management_window_access_is_hub_only() {
        assert!(ensure_sensitive_management_window("hub").is_ok());
        assert!(ensure_sensitive_management_window("main").is_err());
        assert!(ensure_sensitive_management_window("toast").is_err());
        assert!(ensure_sensitive_management_window("result").is_err());
        assert!(ensure_sensitive_management_window("").is_err());
    }

    /// sidecar 新配置成功时只保存新 Token，不应触发任何补偿步骤。
    #[test]
    fn sidecar_apply_success_skips_rollback() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = coordinate_sidecar_apply(
            "测试配置",
            || {
                events.borrow_mut().push("apply");
                Ok("new-token".to_string())
            },
            || {
                events.borrow_mut().push("restore-state");
                Ok(())
            },
            || {
                events.borrow_mut().push("restore-sidecar");
                Ok("old-token".to_string())
            },
            |token| {
                events.borrow_mut().push(if token == "new-token" {
                    "store-new-token"
                } else {
                    "store-old-token"
                });
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(*events.borrow(), vec!["apply", "store-new-token"]);
    }

    /// sidecar 应用失败时必须严格先恢复配置、再恢复旧进程、最后保存回滚 Token。
    #[test]
    fn sidecar_apply_failure_rolls_back_in_order() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = coordinate_sidecar_apply(
            "测试配置",
            || {
                events.borrow_mut().push("apply");
                Err("含敏感详情的启动错误".to_string())
            },
            || {
                events.borrow_mut().push("restore-state");
                Ok(())
            },
            || {
                events.borrow_mut().push("restore-sidecar");
                Ok("old-token".to_string())
            },
            |token| {
                events.borrow_mut().push(if token == "old-token" {
                    "store-old-token"
                } else {
                    "store-new-token"
                });
                Ok(())
            },
        );
        assert_eq!(result, Err("测试配置未生效，配置已回滚".to_string()));
        assert_eq!(
            *events.borrow(),
            vec![
                "apply",
                "restore-state",
                "restore-sidecar",
                "store-old-token"
            ]
        );
    }

    /// 启停、默认态和展示名编辑不得触发上游探针；连接关键参数或密钥变化必须触发。
    #[test]
    fn private_model_probe_only_runs_for_new_or_upstream_changes() {
        let existing = PrivateModelRecord {
            id: "model_550e8400-e29b-41d4-a716-446655440000".to_string(),
            display_name: "原名称".to_string(),
            capability: private_models::PrivateModelCapability::Text,
            enabled: true,
            is_default: false,
            provider: "openai-compatible".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            model_name: "text-model".to_string(),
            api_key: "secret-value".to_string(),
            has_api_key: true,
        };
        let status_only = SavePrivateModelRequest {
            id: Some(existing.id.clone()),
            display_name: "新名称".to_string(),
            capability: existing.capability.clone(),
            enabled: false,
            is_default: false,
            provider: existing.provider.clone(),
            base_url: format!("{}/", existing.base_url),
            model_name: existing.model_name.clone(),
            api_key: None,
        };
        assert!(!private_model_save_requires_probe(
            Some(&existing),
            &status_only
        ));
        let mut default_only = status_only.clone();
        default_only.enabled = true;
        default_only.is_default = true;
        assert!(!private_model_save_requires_probe(
            Some(&existing),
            &default_only
        ));
        assert!(private_model_save_requires_probe(None, &status_only));

        let mut changed_upstream = status_only.clone();
        changed_upstream.model_name = "text-model-v2".to_string();
        assert!(private_model_save_requires_probe(
            Some(&existing),
            &changed_upstream
        ));

        let mut rotated_key = status_only;
        rotated_key.api_key = Some("rotated-key".to_string());
        assert!(private_model_save_requires_probe(
            Some(&existing),
            &rotated_key
        ));
    }

    #[test]
    fn hub_frontmost_shortcut_keeps_hub_and_does_not_reuse_previous_external_target() {
        let decision = resolve_voice_trigger_context("CodexMan");

        assert_eq!(decision.target_app, "");
        assert!(!decision.show_floating_window);
        assert!(decision.keep_hub_visible);
    }

    #[test]
    fn external_frontmost_shortcut_keeps_current_focus_without_reactivation() {
        let decision = resolve_voice_trigger_context("ChatGPT");

        assert_eq!(decision.target_app, "ChatGPT");
        assert!(decision.show_floating_window);
        assert!(!decision.keep_hub_visible);
    }

    #[test]
    fn paste_without_explicit_target_stops_when_codexman_is_frontmost() {
        let decision = resolve_paste_target("", "CodexMan");

        assert_eq!(decision.target_app, "");
        assert!(!decision.should_hide_hub);
    }

    #[test]
    fn paste_with_explicit_target_stops_when_frontmost_app_changed() {
        let decision = resolve_paste_target("ChatGPT", "TextEdit");

        assert_eq!(decision.target_app, "");
        assert!(!decision.should_hide_hub);
    }

    #[test]
    fn paste_refocus_only_when_requested_target_is_still_resolved() {
        assert!(should_refocus_requested_paste_target("ChatGPT", "ChatGPT"));
        assert!(!should_refocus_requested_paste_target("", "ChatGPT"));
        assert!(!should_refocus_requested_paste_target(
            "ChatGPT", "TextEdit"
        ));
        assert!(!should_refocus_requested_paste_target(
            "CodexMan", "ChatGPT"
        ));
    }

    #[test]
    fn explicit_paste_target_is_trusted_only_when_target_is_unchanged() {
        assert!(should_trust_explicit_paste_target("ChatGPT", "ChatGPT"));
        assert!(!should_trust_explicit_paste_target("", "ChatGPT"));
        assert!(!should_trust_explicit_paste_target("ChatGPT", "TextEdit"));
        assert!(!should_trust_explicit_paste_target("CodexMan", "ChatGPT"));
    }

    #[test]
    fn paste_focus_snapshot_only_accepts_text_input() {
        let snapshot =
            parse_paste_focus_snapshot("ChatGPT", "AXTextArea\n\nMessage\n100\n200\n300\n80")
                .expect("文本输入控件应该生成可恢复快照");

        assert_eq!(snapshot.target_app, "ChatGPT");
        assert_eq!(snapshot.center_x, 250);
        assert_eq!(snapshot.center_y, 240);
    }

    #[test]
    fn paste_focus_snapshot_rejects_non_text_focus() {
        assert!(
            parse_paste_focus_snapshot("ChatGPT", "AXButton\n\nSend\n100\n200\n40\n40",).is_none()
        );
    }

    #[test]
    fn paste_ignores_explicit_target_when_current_focus_is_codexman() {
        let decision = resolve_paste_target("ChatGPT", "CodexMan");

        assert_eq!(decision.target_app, "");
        assert!(!decision.should_hide_hub);
    }

    #[test]
    fn floating_window_position_uses_screen_containing_focused_window_anchor() {
        let primary = ScreenWorkArea {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let secondary = ScreenWorkArea {
            x: 1440.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };

        let area = select_work_area_for_anchor(
            &[primary, secondary],
            Some(ScreenPoint {
                x: 2100.0,
                y: 420.0,
            }),
        )
        .expect("应该能根据前台窗口中心点选择屏幕");
        let position = top_center_position_in_work_area(area, 280.0, 18.0);

        assert_eq!(area, secondary);
        assert_eq!(position.x, 2260.0);
        assert_eq!(position.y, 18.0);
    }

    #[test]
    fn paste_timing_keeps_primary_path_responsive() {
        assert!(std::hint::black_box(PASTE_DIAGNOSTIC_SETTLE_DELAY_MS) <= 120);
        assert!(std::hint::black_box(CLIPBOARD_VERIFY_INITIAL_DELAY_MS) <= 50);
    }

    #[test]
    fn explicit_target_paste_path_keeps_fixed_waits_short() {
        let fixed_waits_before_user_visible_paste =
            PASTE_WINDOW_SETTLE_DELAY_MS + CLIPBOARD_VERIFY_INITIAL_DELAY_MS;
        let fixed_waits_until_command_returns = fixed_waits_before_user_visible_paste
            + CLIPBOARD_RESTORE_DELAY_MS
            + PASTE_DIAGNOSTIC_SETTLE_DELAY_MS;

        assert!(fixed_waits_before_user_visible_paste <= 80);
        assert!(fixed_waits_until_command_returns <= 140);
    }

    #[test]
    fn paste_result_treats_single_command_as_sent_without_frontmost_callback() {
        assert!(should_mark_paste_command_as_sent(true, false, false));
    }

    #[test]
    fn clipboard_restore_status_marks_original_clipboard_restored() {
        let status = build_clipboard_restore_status(Some(()), Result::<(), String>::Ok(()));

        assert!(status.attempted);
        assert!(status.restored);
        assert_eq!(status.message, "已恢复用户原剪贴板。");
    }

    #[test]
    fn clipboard_restore_status_reports_missing_snapshot() {
        let status =
            build_clipboard_restore_status::<&str, String>(None, Result::<(), String>::Ok(()));

        assert!(!status.attempted);
        assert!(!status.restored);
    }

    #[test]
    fn clipboard_match_accepts_system_trailing_newline() {
        assert!(clipboard_text_matches_output(
            "不行了，还是不行啊！\n",
            "不行了，还是不行啊！"
        ));
    }

    #[test]
    fn clipboard_match_rejects_old_clipboard_content() {
        assert!(!clipboard_text_matches_output(
            "旧内容",
            "不行了，还是不行啊！"
        ));
    }

    /// JSONL 提交身份必须同时匹配水位、非旧 ID、canonical cwd 和完整首条用户消息。
    #[test]
    fn cdp_submission_identity_matching_is_exact() {
        let identity = CodexSubmissionIdentity {
            thread_id: "thread-new".to_string(),
            canonical_cwd: "/tmp/project".to_string(),
            first_user_message: "exact prompt".to_string(),
            first_user_message_at_ms: 10_500,
        };
        let mut known = HashSet::new();
        assert!(submission_identity_matches(
            &identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        let newline_identity = CodexSubmissionIdentity {
            thread_id: "thread-newline".to_string(),
            canonical_cwd: "/tmp/project".to_string(),
            first_user_message: "exact prompt\n".to_string(),
            first_user_message_at_ms: 10_500,
        };
        assert!(submission_identity_matches(
            &newline_identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        let wrapped_identity = CodexSubmissionIdentity {
            thread_id: "thread-wrapped".to_string(),
            canonical_cwd: "/tmp/project".to_string(),
            first_user_message: concat!(
                "\n<in-app-browser-context source=\"ambient-ui-state\">\n",
                "Current URL: http://127.0.0.1:1420/voice-polish\n",
                "</in-app-browser-context>\n\n",
                "## My request:\n",
                "exact prompt\n"
            )
            .to_string(),
            first_user_message_at_ms: 10_500,
        };
        assert!(submission_identity_matches(
            &wrapped_identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        let image_wrapped_identity = CodexSubmissionIdentity {
            thread_id: "thread-image-wrapped".to_string(),
            canonical_cwd: "/tmp/project".to_string(),
            first_user_message: concat!(
                "\n# Files mentioned by the user:\n\n",
                "## codexman-task-attachment.webp: /tmp/codexman-task-attachment.webp\n\n",
                "## My request:\n",
                "exact prompt\n"
            )
            .to_string(),
            first_user_message_at_ms: 10_500,
        };
        assert!(submission_identity_matches(
            &image_wrapped_identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        let stale_message_identity = CodexSubmissionIdentity {
            thread_id: "thread-stale".to_string(),
            canonical_cwd: "/tmp/project".to_string(),
            first_user_message: "exact prompt".to_string(),
            first_user_message_at_ms: 9_999,
        };
        assert!(!submission_identity_matches(
            &stale_message_identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        known.insert("thread-new".to_string());
        assert!(submission_identity_matches(
            &identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        known.clear();
        assert!(!submission_identity_matches(
            &identity,
            8_000,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt"
        ));
        assert!(!submission_identity_matches(
            &identity,
            10_500,
            10_000,
            &known,
            "/tmp/Project",
            "exact prompt"
        ));
        assert!(!submission_identity_matches(
            &identity,
            10_500,
            10_000,
            &known,
            "/tmp/project",
            "exact prompt "
        ));
    }

    /// 恢复只能接受唯一 JSONL 候选，UI 活跃 thread 滞后时不得覆盖本地会话文件证据。
    #[test]
    fn cdp_recovery_trusts_unique_jsonl_candidate_over_stale_ui_id() {
        let empty = HashSet::new();
        assert!(select_unique_recovered_thread(&empty, None).is_err());
        let unique = HashSet::from(["thread-a".to_string()]);
        assert_eq!(
            select_unique_recovered_thread(&unique, Some("thread-a")).expect("双通道一致应恢复"),
            "thread-a"
        );
        assert_eq!(
            select_unique_recovered_thread(&unique, Some("thread-b"))
                .expect("UI 活跃态滞后时应信任唯一 JSONL 候选"),
            "thread-a"
        );
        let multiple = HashSet::from(["thread-a".to_string(), "thread-b".to_string()]);
        assert!(select_unique_recovered_thread(&multiple, None).is_err());
    }

    /// 权威状态库创建水位必须使用精确毫秒下界，不得向提交前放宽一秒，也不得接受零水位。
    #[test]
    fn cdp_thread_creation_watermark_is_exact_and_positive() {
        let connection = Connection::open_in_memory().expect("应创建内存状态库");
        connection
            .execute_batch(
                "CREATE TABLE threads (id TEXT NOT NULL, cwd TEXT NOT NULL, created_at INTEGER NOT NULL, created_at_ms INTEGER);
                 INSERT INTO threads (id, cwd, created_at, created_at_ms) VALUES ('before', '/tmp/project', 101, 100999);
                 INSERT INTO threads (id, cwd, created_at, created_at_ms) VALUES ('exact', '/tmp/project', 101, 101000);",
            )
            .expect("应准备 thread 水位数据");
        assert!(!codex_thread_created_after_watermark_with_connection(
            &connection,
            "before",
            "/tmp/project",
            101_000,
        )
        .expect("提交前 thread 应正常返回 false"));
        assert!(codex_thread_created_after_watermark_with_connection(
            &connection,
            "exact",
            "/tmp/project",
            101_000,
        )
        .expect("精确水位 thread 应命中"));
        assert!(!codex_thread_created_after_watermark_with_connection(
            &connection,
            "exact",
            "/tmp/project",
            0,
        )
        .expect("零水位应直接拒绝"));
    }

    /// session JSONL 解析必须读取 session_meta canonical cwd 和提交水位后的第一条完整用户消息。
    #[test]
    fn cdp_submission_identity_reads_first_user_message_after_watermark() {
        let path = env::temp_dir().join(format!(
            "codexman-submission-{}.jsonl",
            uuid::Uuid::new_v4().simple()
        ));
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-jsonl\",\"cwd\":\"/tmp/project\"}}\n",
                "{\"timestamp\":\"2026-08-11T16:00:00.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"before\"}}\n",
                "{\"timestamp\":\"2026-08-11T16:00:10.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"first exact\"}}\n",
                "{\"timestamp\":\"2026-08-11T16:00:20.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"second\"}}\n"
            ),
        )
        .expect("应写入测试 JSONL");
        let identity = read_codex_submission_identity(&path, 1_786_464_005_000)
            .expect("合法 JSONL 应解析")
            .expect("身份字段应齐全");
        fs::remove_file(&path).expect("应删除测试 JSONL");
        assert_eq!(identity.thread_id, "thread-jsonl");
        assert_eq!(identity.canonical_cwd, "/tmp/project");
        assert_eq!(identity.first_user_message, "first exact");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "会临时读写系统剪贴板，只在自动粘贴排查时手动运行"]
    fn verified_clipboard_write_round_trips_with_system_clipboard() {
        struct ClipboardRestore {
            snapshot: Option<ClipboardSnapshot>,
        }

        impl Drop for ClipboardRestore {
            fn drop(&mut self) {
                if let Some(snapshot) = self.snapshot.as_ref() {
                    let _ = restore_clipboard_snapshot(snapshot);
                }
            }
        }

        let _restore = ClipboardRestore {
            snapshot: capture_clipboard_snapshot().ok(),
        };
        let marker = format!("codexman-clipboard-roundtrip-{}", std::process::id());

        assert!(
            write_clipboard_text_verified(&marker).expect("系统剪贴板写入流程不应报错"),
            "系统剪贴板写入后没有读回本次测试标记"
        );
        let readback = read_clipboard_text_raw().expect("系统剪贴板应该可以读取");
        assert!(
            clipboard_text_matches_output(&readback, &marker),
            "系统剪贴板读回内容没有匹配本次测试标记"
        );
    }
}
