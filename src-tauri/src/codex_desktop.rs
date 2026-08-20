use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use url::Url;

/// Codex Desktop 在 macOS 上的应用目录。
const CODEX_APP_PATH: &str = "/Applications/ChatGPT.app";
/// Codex Desktop 主进程的精确可执行文件路径。
const CODEX_MAIN_PATH: &str = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";
/// OpenAI macOS 官方签名团队标识；bundle 外 helper 必须精确匹配。
const CODEX_TEAM_IDENTIFIER: &str = "2DC432GLL2";
/// 允许参与 Codex CDP 监听的官方签名 Identifier 精确白名单。
const CODEX_SIGNING_IDENTIFIER_ALLOWLIST: [&str; 4] = [
    "com.openai.codex",
    "com.openai.codex.helper",
    "com.openai.codex.helper.renderer",
    "com.openai.sky.CUAService",
];
/// CodexMan 管理的固定 CDP 回环端口；公开 HTTP 不返回该内部实现值。
pub(crate) const CODEX_CDP_PORT: u16 = 9333;
/// 单次 CDP HTTP 探针的最大等待时间。
const CODEX_CDP_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
/// 单个 CDP HTTP 探针响应的最大字节数，防止未知本机服务返回无界正文。
const CODEX_CDP_PROBE_MAX_BYTES: u64 = 256 * 1024;
/// 请求 Codex 正常退出后的等待时间。
const CODEX_QUIT_TIMEOUT: Duration = Duration::from_secs(10);
/// 向精确主进程发送 TERM 后的等待时间。
const CODEX_TERM_TIMEOUT: Duration = Duration::from_secs(10);
/// 重新启动后等待 CDP renderer 就绪的最长时间。
const CODEX_RESTART_READY_TIMEOUT: Duration = Duration::from_secs(25);
/// 进程退出和 CDP readiness 的短轮询间隔。
const CODEX_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// 进程可执行文件签名身份缓存，避免连接状态轮询重复触发完整 bundle 校验。
static CODEX_SIGNATURE_CACHE: OnceLock<Mutex<HashMap<String, CodeSignatureIdentity>>> =
    OnceLock::new();

/// Codex Desktop 连接状态协议值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CodexConnectionState {
    /// CDP renderer 已验证，可执行原生会话创建。
    Connected,
    /// Codex 未运行或未以 CDP 模式启动。
    Disconnected,
    /// 用户已确认重启，Rust 正在后台等待新 renderer。
    Restarting,
    /// 固定端口被其它服务占用或上一次重启出现明确故障。
    Blocked,
    /// 当前平台不支持 Codex Desktop CDP 管理。
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}

/// 公开 HTTP 返回的 Codex Desktop 连接快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexConnectionStatus {
    /// 当前连接状态。
    pub(crate) state: CodexConnectionState,
    /// 是否已验证真实 Codex renderer。
    pub(crate) connected: bool,
    /// Codex Desktop 主进程是否正在运行。
    pub(crate) desktop_running: bool,
    /// 当前是否允许用户发起显式重启；执行中任务门禁由上层业务入口进一步收紧。
    pub(crate) can_restart: bool,
    /// 稳定原因码，前端据此选择交互，不解析 message 文案。
    pub(crate) reason_code: String,
    /// 不包含路径、端口、PID、DOM 或登录信息的用户说明。
    pub(crate) message: String,
    /// 本次探针完成时间，Unix 毫秒字符串。
    pub(crate) checked_at: String,
}

/// Codex 重启请求的异步接受结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexRestartAccepted {
    /// 请求已经进入唯一后台重启流程。
    pub(crate) accepted: bool,
    /// 接受后固定为 restarting，最终结果由连接状态接口轮询。
    pub(crate) state: CodexConnectionState,
}

/// 后台重启失败后的脱敏诊断，只保留稳定码和安全文案。
#[derive(Debug, Clone)]
struct CodexRestartFailure {
    /// 稳定错误码。
    code: String,
    /// 可直接返回连接状态接口的安全说明。
    message: String,
}

/// 重启前固化的受信进程角色，决定 TERM/KILL 的固定先后顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestartProcessRole {
    /// CDP listener 或官方 helper 后代，信号阶段必须优先于主进程。
    ListenerHelper,
    /// 经过真实路径和主签名验证的 ChatGPT 主进程，始终最后发送信号。
    Main,
}

/// 单个官方旧进程的不可变重启身份快照。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RestartProcessSnapshot {
    /// 操作系统 PID；必须与 start_identity 联合使用，禁止单独信任。
    pid: i32,
    /// `proc_pidinfo` 返回的进程启动秒/微秒身份，防止 PID 退出后被复用。
    start_identity: String,
    /// lsof txt 映射确认的真实可执行文件绝对路径。
    executable: String,
    /// strict codesign 后解析的 Team/Identifier 身份。
    signature: CodeSignatureIdentity,
    /// 信号排序角色。
    role: RestartProcessRole,
    /// snapshot 时该 PID 是否直接监听固定 CDP 端口。
    was_listener: bool,
    /// snapshot 时从该 PID 到已验证主 PID 的完整父链，记录其受信来源。
    parent_chain: Vec<i32>,
}

/// 一次显式重启在任何副作用前建立的完整进程快照。
#[derive(Debug, Clone)]
struct CodexRestartSnapshot {
    /// 主进程及固定 CDP listener/helper 的去重快照。
    processes: Vec<RestartProcessSnapshot>,
    /// 副作用前直接监听固定 CDP 端口的 PID 集合；后续出现任何新 PID 都 fail closed。
    listener_pids: Vec<i32>,
}

/// 固定 CDP 端口的内部身份探针结果。
enum CodexProbeState {
    /// 端口没有服务监听。
    Disconnected,
    /// 版本身份可信但主 renderer 尚未出现。
    NotReady,
    /// 唯一可信主 renderer 及其内部 WebSocket。
    Ready(String),
    /// 同时存在多个可信主 renderer，不能安全选择。
    Ambiguous,
    /// 端口有响应但不是受信 Codex CDP 身份。
    Untrusted,
    /// 无法可靠取得监听 PID、可执行文件或受信进程树，禁止继续猜测连接状态。
    StateFailed,
}

/// Codex Desktop 进程生命周期和重启单飞状态。
pub(crate) struct RuntimeCodexDesktop {
    /// 串行化“开始重启”和“queued 领取为 running”，关闭检查与 CAS 之间的竞态窗口。
    execution_restart_gate: Mutex<()>,
    /// 是否正在执行用户确认后的唯一重启流程。
    restarting: AtomicBool,
    /// 最近一次后台重启的安全失败结果；连接成功后清除。
    last_failure: Mutex<Option<CodexRestartFailure>>,
}

impl Default for RuntimeCodexDesktop {
    /// 创建未连接且没有历史错误的运行时状态。
    /// 流程：初始化单飞原子位和空失败槽；参数无；返回新状态。
    /// 异常/边界：不在构造时探测或启动 Codex，避免 App setup 产生破坏性副作用。
    fn default() -> Self {
        Self {
            execution_restart_gate: Mutex::new(()),
            restarting: AtomicBool::new(false),
            last_failure: Mutex::new(None),
        }
    }
}

/// 读取当前 Codex Desktop CDP 连接快照。
/// 流程：优先返回重启中，再验证固定端口是否为真实 Codex renderer，最后区分桌面未运行、未开启 CDP、端口占用和上次重启失败。
/// 参数：runtime 为 App 生命周期单飞状态；返回不泄露 CDP 端点的连接快照。
/// 异常/边界：探针网络或 JSON 异常只会降级为未连接；状态锁损坏时返回稳定内部错误，绝不猜测已连接。
pub(crate) fn connection_status(
    runtime: &RuntimeCodexDesktop,
) -> Result<CodexConnectionStatus, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = runtime;
        return Ok(CodexConnectionStatus {
            state: CodexConnectionState::Unsupported,
            connected: false,
            desktop_running: false,
            can_restart: false,
            reason_code: "CODEX_PLATFORM_UNSUPPORTED".to_string(),
            message: "当前平台不支持 Codex Desktop 本机连接。".to_string(),
            checked_at: current_unix_millis(),
        });
    }

    #[cfg(target_os = "macos")]
    {
        let checked_at = current_unix_millis();
        if runtime.restarting.load(Ordering::Acquire) {
            return Ok(CodexConnectionStatus {
                state: CodexConnectionState::Restarting,
                connected: false,
                desktop_running: false,
                can_restart: false,
                reason_code: "CODEX_RESTART_IN_PROGRESS".to_string(),
                message: "Codex 正在重启，连接恢复后即可创建和发送任务。".to_string(),
                checked_at,
            });
        }
        let desktop_running = !codex_main_pids()
            .map_err(|_| {
                "无法确认 Codex Desktop 主进程状态（错误码：CODEX_CONNECTION_STATE_FAILED）"
                    .to_string()
            })?
            .is_empty();

        let probe_state = probe_codex_cdp_state();
        match &probe_state {
            CodexProbeState::Ready(_) => {
                runtime
                    .last_failure
                    .lock()
                    .map_err(|_| {
                        "读取 Codex 重启状态失败（错误码：CODEX_CONNECTION_STATE_FAILED）"
                            .to_string()
                    })?
                    .take();
                return Ok(CodexConnectionStatus {
                    state: CodexConnectionState::Connected,
                    connected: true,
                    desktop_running: true,
                    can_restart: true,
                    reason_code: "CODEX_CONNECTED".to_string(),
                    message: "Codex 已连接，可以由 Codex Desktop 原生创建并发送任务。".to_string(),
                    checked_at,
                });
            }
            CodexProbeState::Ambiguous => {
                return Ok(CodexConnectionStatus {
                    state: CodexConnectionState::Blocked,
                    connected: false,
                    desktop_running,
                    can_restart: true,
                    reason_code: "CODEX_CDP_TARGET_AMBIGUOUS".to_string(),
                    message: "检测到多个 Codex 主窗口，无法安全选择任务发送目标。".to_string(),
                    checked_at,
                });
            }
            CodexProbeState::NotReady => {
                return Ok(CodexConnectionStatus {
                    state: CodexConnectionState::Disconnected,
                    connected: false,
                    desktop_running,
                    can_restart: true,
                    reason_code: "CODEX_CDP_NOT_READY".to_string(),
                    message: "Codex 已启动，但任务页面尚未准备完成。".to_string(),
                    checked_at,
                });
            }
            CodexProbeState::StateFailed => {
                return Err(
                    "无法确认 Codex 连接进程身份（错误码：CODEX_CONNECTION_STATE_FAILED）"
                        .to_string(),
                );
            }
            CodexProbeState::Disconnected | CodexProbeState::Untrusted => {}
        }

        let port_in_use = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CDP_PORT),
            Duration::from_millis(80),
        )
        .is_ok();
        if port_in_use || matches!(probe_state, CodexProbeState::Untrusted) {
            return Ok(CodexConnectionStatus {
                state: CodexConnectionState::Blocked,
                connected: false,
                desktop_running,
                can_restart: false,
                reason_code: "CODEX_CDP_PORT_IN_USE".to_string(),
                message: "Codex 连接端口正被其它本机服务占用，CodexMan 不会结束未知进程。"
                    .to_string(),
                checked_at,
            });
        }

        if let Some(failure) = runtime
            .last_failure
            .lock()
            .map_err(|_| {
                "读取 Codex 重启状态失败（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string()
            })?
            .clone()
        {
            return Ok(CodexConnectionStatus {
                state: CodexConnectionState::Blocked,
                connected: false,
                desktop_running,
                can_restart: true,
                reason_code: failure.code,
                message: failure.message,
                checked_at,
            });
        }

        Ok(CodexConnectionStatus {
            state: CodexConnectionState::Disconnected,
            connected: false,
            desktop_running,
            can_restart: true,
            reason_code: if desktop_running {
                "CODEX_CDP_NOT_ENABLED".to_string()
            } else {
                "CODEX_DESKTOP_NOT_RUNNING".to_string()
            },
            message: if desktop_running {
                "Codex 正在运行，但尚未建立 CodexMan 创建任务所需的本机连接。".to_string()
            } else {
                "Codex 尚未运行，需要由 CodexMan 重新启动并建立本机连接。".to_string()
            },
            checked_at,
        })
    }
}

/// 接受一次用户明确确认的 Codex Desktop 异步重启。
/// 流程：用共享门禁复核无 running 任务和全部 listener 身份，设置单飞状态后启动后台线程；显式请求即使当前 Ready 也真正退出旧进程并干净启动。
/// 参数：app 用于取得运行状态并记录脱敏错误；返回 202 接口可直接序列化的接受结果。
/// 异常/边界：重复调用、端口被未知服务占用或非 macOS 平台均在产生进程副作用前拒绝。
pub(crate) fn begin_restart(app: &AppHandle) -> Result<CodexRestartAccepted, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        return Err(
            "当前平台不支持重启 Codex Desktop（错误码：CODEX_PLATFORM_UNSUPPORTED）".to_string(),
        );
    }

    #[cfg(target_os = "macos")]
    {
        let runtime = app.state::<RuntimeCodexDesktop>();
        let _gate = runtime.execution_restart_gate.lock().map_err(|_| {
            "获取 Codex 执行与重启门禁失败（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string()
        })?;
        let probe_state = probe_codex_cdp_state();
        if matches!(probe_state, CodexProbeState::StateFailed) {
            return Err(
                "无法确认 Codex 连接进程身份（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string(),
            );
        }
        if crate::task_store::has_running_task(app)? {
            return Err(
                "存在执行中的任务，当前不能重启 Codex（错误码：CODEX_RESTART_TASK_ACTIVE）"
                    .to_string(),
            );
        }
        let port_in_use = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CDP_PORT),
            Duration::from_millis(80),
        )
        .is_ok();
        if matches!(probe_state, CodexProbeState::Untrusted)
            || (matches!(probe_state, CodexProbeState::Disconnected) && port_in_use)
        {
            return Err(
                "Codex 连接端口正被其它本机服务占用（错误码：CODEX_CDP_PORT_IN_USE）".to_string(),
            );
        }
        let mut last_failure = runtime.last_failure.lock().map_err(|_| {
            "保存 Codex 重启状态失败（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string()
        })?;
        runtime
            .restarting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Codex 已在重启中（错误码：CODEX_RESTART_IN_PROGRESS）".to_string())?;
        last_failure.take();
        drop(last_failure);

        let restart_app = app.clone();
        let spawn_result = thread::Builder::new()
            .name("codexman-codex-restart".to_string())
            .spawn(move || {
                let outcome = restart_codex_desktop();
                let runtime = restart_app.state::<RuntimeCodexDesktop>();
                if let Err(failure) = outcome {
                    if let Ok(mut last_failure) = runtime.last_failure.lock() {
                        *last_failure = Some(failure.clone());
                    }
                    let _ = crate::desktop_error::record_desktop_error(
                        &restart_app,
                        &failure.code,
                        "restart_codex_desktop",
                        None,
                        &failure.message,
                    );
                }
                runtime.restarting.store(false, Ordering::Release);
            });
        if spawn_result.is_err() {
            runtime.restarting.store(false, Ordering::Release);
            return Err("无法启动 Codex 重启后台任务（错误码：CODEX_RESTART_FAILED）".to_string());
        }
        Ok(CodexRestartAccepted {
            accepted: true,
            state: CodexConnectionState::Restarting,
        })
    }
}

/// 在与重启互斥的门禁内领取一个 queued 任务。
/// 流程：持有 RuntimeCodexDesktop 共享锁，确认未重启且真实 renderer 已连接，再执行调用方提供的 TaskStore CAS；参数为 AppHandle 和单次领取闭包；返回闭包结果。
/// 异常/边界：锁损坏、重启中或断连均在 CAS 前失败，queued 保持原状；重启入口必须持有同一锁完成 running 复核并设置 restarting，关闭 TOCTOU 竞态。
pub(crate) fn with_execution_start_gate<T, F>(app: &AppHandle, action: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let runtime = app.state::<RuntimeCodexDesktop>();
    let _gate = runtime.execution_restart_gate.lock().map_err(|_| {
        "获取 Codex 执行与重启门禁失败（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string()
    })?;
    if runtime.restarting.load(Ordering::Acquire) {
        return Err(
            "Codex Desktop 尚未建立本机连接（错误码：CODEX_DESKTOP_NOT_CONNECTED）".to_string(),
        );
    }
    match probe_codex_cdp() {
        Ok(true) => {}
        Ok(false) => {
            return Err(
                "Codex Desktop 尚未建立本机连接（错误码：CODEX_DESKTOP_NOT_CONNECTED）".to_string(),
            );
        }
        Err(error) if error.contains("CODEX_CONNECTION_STATE_FAILED") => {
            return Err(crate::desktop_error::record_desktop_error(
                app,
                "CODEX_CONNECTION_STATE_FAILED",
                "get_codex_connection",
                None,
                &error,
            ));
        }
        Err(error) => return Err(error),
    }
    action()
}

/// 验证固定回环端口是否属于真实 Codex renderer。
/// 流程：分别读取 `/json/version` 与 `/json/list`，要求版本响应含回环 WebSocket 地址且目标列表存在 app:// Codex page。
/// 参数：无；返回 true 表示可以建立 CDP 发送连接，false 表示端口未监听。
/// 异常/边界：端口返回非成功 HTTP、畸形 JSON 或非 Codex 目标时返回错误，调用方不得把其它 Chromium 实例当作 Codex。
pub(crate) fn probe_codex_cdp() -> Result<bool, String> {
    match probe_codex_cdp_state() {
        CodexProbeState::Ready(_) => Ok(true),
        CodexProbeState::Disconnected | CodexProbeState::NotReady => Ok(false),
        CodexProbeState::Ambiguous => {
            Err("Codex 主页面不唯一（错误码：CODEX_CDP_TARGET_AMBIGUOUS）".to_string())
        }
        CodexProbeState::Untrusted => {
            Err("固定连接端口不是可信 Codex 服务（错误码：CODEX_CDP_PORT_IN_USE）".to_string())
        }
        CodexProbeState::StateFailed => {
            Err("无法确认 Codex 连接进程身份（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string())
        }
    }
}

/// 完整验证固定端口的版本身份和唯一主 renderer。
/// 流程：先绑定监听 PID 到精确 Codex 主进程或 App bundle 受信后代，再读取 version/list 并选择唯一可信 app page；参数无；返回细分内部状态。
/// 异常/边界：无监听为 Disconnected，PID 探测失败为 StateFailed，未知监听者为 Untrusted；HTTP 内容不能覆盖进程身份判断。
fn probe_codex_cdp_state() -> CodexProbeState {
    probe_codex_cdp_state_on_port(CODEX_CDP_PORT)
}

/// 在指定回环端口验证监听进程身份和 CDP HTTP 身份。
/// 流程：端口未监听直接返回 Disconnected；端口监听时先要求 PID 属于精确 Codex 主进程或 App bundle 内受信后代，再读取 version/list。
/// 参数：port 为内部固定端口，测试使用临时端口验证伪服务拒绝；返回细分探针状态。
/// 异常/边界：PID、可执行文件或父进程链任一探测失败均返回 StateFailed；未知监听者在读取其 HTTP 正文前即返回 Untrusted。
fn probe_codex_cdp_state_on_port(port: u16) -> CodexProbeState {
    #[cfg(target_os = "macos")]
    match trusted_listener_on_port(port) {
        Ok(None) => return CodexProbeState::Disconnected,
        Ok(Some(false)) => return CodexProbeState::Untrusted,
        Ok(Some(true)) => {}
        Err(_) => return CodexProbeState::StateFailed,
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(CODEX_CDP_PROBE_TIMEOUT)
        .timeout(CODEX_CDP_PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build();
    let Ok(client) = client else {
        return CodexProbeState::Untrusted;
    };
    let base_url = format!("http://127.0.0.1:{port}");
    let version_response = match client.get(format!("{base_url}/json/version")).send() {
        Ok(response) => response,
        Err(error) if error.is_connect() || error.is_timeout() => {
            return CodexProbeState::Disconnected
        }
        Err(_) => return CodexProbeState::Untrusted,
    };
    if !version_response.status().is_success() {
        return CodexProbeState::Untrusted;
    }
    let Ok(version) = read_probe_json::<Value>(version_response) else {
        return CodexProbeState::Untrusted;
    };
    if !version
        .get("Browser")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("Chrome/"))
    {
        return CodexProbeState::Untrusted;
    }
    if version
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .filter(|value| {
            Url::parse(value).ok().is_some_and(|parsed| {
                parsed.scheme() == "ws"
                    && matches!(parsed.host_str(), Some("127.0.0.1") | Some("localhost"))
                    && parsed.port() == Some(CODEX_CDP_PORT)
                    && parsed.path().starts_with("/devtools/browser/")
                    && !parsed
                        .path()
                        .trim_start_matches("/devtools/browser/")
                        .is_empty()
                    && parsed.query().is_none()
                    && parsed.fragment().is_none()
                    && parsed.username().is_empty()
                    && parsed.password().is_none()
            })
        })
        .is_none()
    {
        return CodexProbeState::Untrusted;
    }
    let Ok(targets) = client.get(format!("{base_url}/json/list")).send() else {
        return CodexProbeState::Untrusted;
    };
    if !targets.status().is_success() {
        return CodexProbeState::Untrusted;
    }
    let Ok(values) = read_probe_json::<Vec<Value>>(targets) else {
        return CodexProbeState::Untrusted;
    };
    match select_codex_page_websocket_url(&values) {
        Ok(Some(websocket_url)) => CodexProbeState::Ready(websocket_url),
        Ok(None) => CodexProbeState::NotReady,
        Err(_) => CodexProbeState::Ambiguous,
    }
}

/// 读取唯一可信 Codex 主页面的内部 WebSocket 地址。
/// 流程：复用版本与 target 双探针，再从 target 列表中只选择唯一可信主页面；参数无；返回仅供 Rust CDP 模块使用的内部地址。
/// 异常/边界：零个 renderer 表示尚未连接，多个可信 renderer 或任何非回环地址均 fail closed；地址不得进入公开 HTTP、日志或错误文案。
pub(crate) fn codex_page_websocket_url() -> Result<Option<String>, String> {
    match probe_codex_cdp_state() {
        CodexProbeState::Ready(websocket_url) => Ok(Some(websocket_url)),
        CodexProbeState::Disconnected | CodexProbeState::NotReady => Ok(None),
        CodexProbeState::Ambiguous => {
            Err("Codex 主页面不唯一（错误码：CODEX_CDP_TARGET_AMBIGUOUS）".to_string())
        }
        CodexProbeState::Untrusted => {
            Err("固定连接端口不是可信 Codex 服务（错误码：CODEX_CDP_PORT_IN_USE）".to_string())
        }
        CodexProbeState::StateFailed => {
            Err("无法确认 Codex 连接进程身份（错误码：CODEX_CONNECTION_STATE_FAILED）".to_string())
        }
    }
}

/// 判断 CDP target 列表中是否存在可管理的 Codex 主页面。
/// 流程：要求 page 类型、可信 app:// URL、Codex 标题和回环 WebSocket 地址同时成立；参数为原始 target 数组；返回是否命中。
/// 异常/边界：排除 initialRoute 临时 renderer，避免把登录或启动中页面误判为可发送窗口。
fn select_codex_page_websocket_url(targets: &[Value]) -> Result<Option<String>, String> {
    let matches = targets
        .iter()
        .filter_map(|target| {
            let target_type = target.get("type").and_then(Value::as_str);
            let title = target.get("title").and_then(Value::as_str);
            let url = target
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let websocket_url = target
                .get("webSocketDebuggerUrl")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let trusted_app_url = url == "app://-/index.html"
                || url.starts_with("app://-/index.html?")
                || url == "app://codex/index.html"
                || url.starts_with("app://codex/index.html?")
                || url == "app://chatgpt.com/index.html"
                || url.starts_with("app://chatgpt.com/index.html?");
            let trusted_websocket = Url::parse(websocket_url).ok().is_some_and(|parsed| {
                parsed.scheme() == "ws"
                    && matches!(parsed.host_str(), Some("127.0.0.1") | Some("localhost"))
                    && parsed.port() == Some(CODEX_CDP_PORT)
                    && parsed.path().starts_with("/devtools/page/")
                    && !parsed
                        .path()
                        .trim_start_matches("/devtools/page/")
                        .is_empty()
                    && parsed.query().is_none()
                    && parsed.fragment().is_none()
                    && parsed.username().is_empty()
                    && parsed.password().is_none()
            });
            if target_type == Some("page")
                && title == Some("Codex")
                && trusted_app_url
                && !url.contains("initialRoute=")
                && trusted_websocket
            {
                Some(websocket_url.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [websocket_url] => Ok(Some(websocket_url.clone())),
        _ => Err("发现多个 Codex 主页面，无法安全选择发送目标".to_string()),
    }
}

/// 有界读取并解析单个 CDP HTTP JSON 响应。
/// 流程：先拒绝超大 Content-Length，再从响应句柄最多读取上限加一字节并解析；参数为 blocking response；返回目标 JSON 类型。
/// 异常/边界：缺少长度仍受 Read::take 限制，超限和畸形 JSON 使用固定错误且不保留正文。
fn read_probe_json<T>(response: reqwest::blocking::Response) -> Result<T, String>
where
    T: DeserializeOwned,
{
    if response
        .content_length()
        .is_some_and(|length| length > CODEX_CDP_PROBE_MAX_BYTES)
    {
        return Err("Codex 连接探针响应过大".to_string());
    }
    let mut bytes = Vec::new();
    response
        .take(CODEX_CDP_PROBE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取 Codex 连接探针失败".to_string())?;
    if bytes.len() as u64 > CODEX_CDP_PROBE_MAX_BYTES {
        return Err("Codex 连接探针响应过大".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Codex 连接探针 JSON 无效".to_string())
}

/// 在后台执行 Codex Desktop 的精确退出与 CDP 重启。
/// 流程：副作用前固化所有受信主进程/listener 的出生身份、路径、签名和父链；请求正常退出后，仅对仍完全匹配快照者依次 TERM、必要时 helper 优先再主进程 KILL，确认端口释放后才启动。
/// 参数：无；返回成功或稳定错误码与脱敏说明。
/// 异常/边界：新 listener、PID 复用、身份变化或任一探测失败立即 fail closed；SIGKILL 只覆盖 immutable snapshot 中仍匹配的官方旧进程，不结束未知进程。
#[cfg(target_os = "macos")]
fn restart_codex_desktop() -> Result<(), CodexRestartFailure> {
    let probe_state = probe_codex_cdp_state();
    if matches!(probe_state, CodexProbeState::StateFailed) {
        return Err(restart_failure(
            "CODEX_CONNECTION_STATE_FAILED",
            "无法确认 Codex 连接进程身份，已停止重启。",
        ));
    }
    let port_in_use = TcpStream::connect_timeout(
        &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CDP_PORT),
        Duration::from_millis(80),
    )
    .is_ok();
    if matches!(probe_state, CodexProbeState::Untrusted)
        || (matches!(probe_state, CodexProbeState::Disconnected) && port_in_use)
    {
        return Err(restart_failure(
            "CODEX_CDP_PORT_IN_USE",
            "Codex 连接端口正被其它本机服务占用，CodexMan 未结束该进程。",
        ));
    }
    let snapshot = capture_restart_snapshot().map_err(|_| {
        restart_failure(
            "CODEX_CONNECTION_STATE_FAILED",
            "无法建立 Codex 重启进程快照，已停止重启。",
        )
    })?;

    let _ = Command::new("osascript")
        .args(["-e", "tell application id \"com.openai.codex\" to quit"])
        .status();
    if !wait_for_snapshot_exit(&snapshot, CODEX_QUIT_TIMEOUT).map_err(|_| {
        restart_failure(
            "CODEX_CONNECTION_STATE_FAILED",
            "Codex 正常退出后的进程身份发生变化，已停止重启。",
        )
    })? {
        signal_snapshot_processes(&snapshot, libc::SIGTERM)
            .map_err(|error| restart_failure("CODEX_RESTART_FAILED", &error))?;
        if !wait_for_snapshot_exit(&snapshot, CODEX_TERM_TIMEOUT).map_err(|_| {
            restart_failure(
                "CODEX_CONNECTION_STATE_FAILED",
                "Codex TERM 后的进程身份发生变化，已停止重启。",
            )
        })? {
            signal_snapshot_processes(&snapshot, libc::SIGKILL)
                .map_err(|error| restart_failure("CODEX_RESTART_FAILED", &error))?;
            if !wait_for_snapshot_exit(&snapshot, CODEX_TERM_TIMEOUT).map_err(|_| {
                restart_failure(
                    "CODEX_CONNECTION_STATE_FAILED",
                    "Codex KILL 后的进程身份发生变化，已停止重启。",
                )
            })? {
                return Err(restart_failure(
                    "CODEX_RESTART_FAILED",
                    "官方旧 Codex 进程在受限 SIGKILL 后仍未退出。",
                ));
            }
        }
    }
    if !wait_for_port_release(CODEX_TERM_TIMEOUT).map_err(|_| {
        restart_failure(
            "CODEX_CONNECTION_STATE_FAILED",
            "无法确认旧 Codex 监听进程退出，已停止重启。",
        )
    })? {
        return Err(restart_failure(
            "CODEX_CDP_PORT_IN_USE",
            "Codex 退出后连接端口仍被占用，CodexMan 已停止重启。",
        ));
    }

    let port = CODEX_CDP_PORT.to_string();
    let status = Command::new("open")
        .args([
            "-na",
            CODEX_APP_PATH,
            "--args",
            "--remote-debugging-address=127.0.0.1",
            &format!("--remote-debugging-port={port}"),
            &format!("--remote-allow-origins=http://127.0.0.1:{port}"),
        ])
        .status()
        .map_err(|_| restart_failure("CODEX_RESTART_FAILED", "无法启动 Codex Desktop。"))?;
    if !status.success() {
        return Err(restart_failure(
            "CODEX_RESTART_FAILED",
            "系统未能启动 Codex Desktop。",
        ));
    }
    let deadline = Instant::now() + CODEX_RESTART_READY_TIMEOUT;
    while Instant::now() < deadline {
        match probe_codex_cdp() {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if error.contains("CODEX_CONNECTION_STATE_FAILED") => {
                return Err(restart_failure(
                    "CODEX_CONNECTION_STATE_FAILED",
                    "无法确认 Codex 连接进程身份，已停止重启。",
                ));
            }
            Err(_) => {
                return Err(restart_failure(
                    "CODEX_CDP_PORT_IN_USE",
                    "Codex 连接端口出现未知监听者，已停止重启。",
                ));
            }
        }
        thread::sleep(CODEX_PROCESS_POLL_INTERVAL);
    }
    Err(restart_failure(
        "CODEX_RESTART_TIMEOUT",
        "Codex 已启动，但未在限定时间内建立 CodexMan 连接。",
    ))
}

/// 读取精确 Codex Desktop 主进程 PID 列表。
/// 流程：用 `ps -ww` 枚举真实 comm 并只保留精确主程序路径，再逐个读取 executable，要求路径不变并通过 strict codesign 且 Identifier=com.openai.codex。
/// 异常/边界：不使用在当前 macOS 会漏报主进程的 pgrep 名称匹配，也不读取可伪造 argv；候选路径或签名探测失败显式失败，未运行返回空列表。
#[cfg(target_os = "macos")]
fn codex_main_pids() -> Result<Vec<i32>, String> {
    let output = Command::new("ps")
        .args(["-ww", "-axo", "pid=,comm="])
        .output()
        .map_err(|_| "无法检查 Codex Desktop 主进程".to_string())?;
    if !output.status.success() {
        return Err("检查 Codex Desktop 主进程失败".to_string());
    }
    let stdout =
        String::from_utf8(output.stdout).map_err(|_| "Codex Desktop 主进程结果无效".to_string())?;
    let candidates = parse_main_process_candidates(&stdout)?;
    let mut verified = Vec::new();
    for pid in candidates {
        if process_executable_path(pid)? != CODEX_MAIN_PATH {
            continue;
        }
        let identity = code_signature_identity(CODEX_MAIN_PATH)?;
        if identity.team_identifier != CODEX_TEAM_IDENTIFIER
            || identity.identifier != "com.openai.codex"
        {
            return Err("Codex Desktop 主进程签名身份不可信".to_string());
        }
        verified.push(pid);
    }
    Ok(verified)
}

/// 在任何退出副作用前建立官方 Codex 进程不可变快照。
/// 流程：读取已验证主 PID 和固定 CDP listener；主进程记录主角色，listener 要求父链命中主 PID并记录 helper 角色，所有记录固化出生身份、真实路径和 strict codesign。
/// 参数无；返回当前代主进程/listener 的完整证据。异常/边界：listener 无主进程、重复身份异常或任一探测失败均拒绝重启，不执行 osascript/TERM/KILL。
#[cfg(target_os = "macos")]
fn capture_restart_snapshot() -> Result<CodexRestartSnapshot, String> {
    let main_pids = codex_main_pids()?;
    let listener_pids = listener_pids_on_port(CODEX_CDP_PORT)?;
    if !listener_pids.is_empty() && main_pids.is_empty() {
        return Err("Codex listener 不属于已验证主进程".to_string());
    }
    let mut processes = Vec::new();
    for pid in &main_pids {
        processes.push(capture_process_snapshot(
            *pid,
            RestartProcessRole::Main,
            listener_pids.contains(pid),
            vec![*pid],
        )?);
    }
    for listener_pid in &listener_pids {
        if main_pids.contains(listener_pid) {
            continue;
        }
        let parent_chain = process_parent_chain_to_main(*listener_pid, &main_pids)?
            .ok_or_else(|| "Codex listener 父进程链不可信".to_string())?;
        processes.push(capture_process_snapshot(
            *listener_pid,
            RestartProcessRole::ListenerHelper,
            true,
            parent_chain,
        )?);
    }
    let snapshot = CodexRestartSnapshot {
        processes,
        listener_pids,
    };
    validate_snapshot_listener_set(&snapshot)?;
    Ok(snapshot)
}

/// 捕获一个已确认来源进程的不可变身份字段。
/// 流程：依次读取出生时间、真实 executable 和 strict codesign 身份；参数还携带角色、监听来源和 snapshot 父链；返回完整记录。
/// 异常/边界：进程在捕获中退出或任何字段不可读均失败，禁止生成部分 snapshot。
#[cfg(target_os = "macos")]
fn capture_process_snapshot(
    pid: i32,
    role: RestartProcessRole,
    was_listener: bool,
    parent_chain: Vec<i32>,
) -> Result<RestartProcessSnapshot, String> {
    let start_identity =
        process_start_identity(pid)?.ok_or_else(|| "Codex snapshot 进程已退出".to_string())?;
    let executable = process_executable_path(pid)?;
    let signature = code_signature_identity(&executable)?;
    if !listener_signature_is_allowed(&signature)
        || (role == RestartProcessRole::Main
            && (executable != CODEX_MAIN_PATH || signature.identifier != "com.openai.codex"))
    {
        return Err("Codex snapshot 进程身份不可信".to_string());
    }
    Ok(RestartProcessSnapshot {
        pid,
        start_identity,
        executable,
        signature,
        role,
        was_listener,
        parent_chain,
    })
}

/// 等待 snapshot 中全部旧进程退出，并持续拒绝新 listener 或 PID 身份变化。
/// 流程：固定间隔重读当前 listener 集合与每个 snapshot 身份；参数为不可变 snapshot 和等待时限；返回是否全部退出。
/// 异常/边界：新 listener、PID 复用、路径/签名变化和探测失败立即返回错误，不等到超时后再猜测。
#[cfg(target_os = "macos")]
fn wait_for_snapshot_exit(
    snapshot: &CodexRestartSnapshot,
    timeout: Duration,
) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if live_snapshot_processes(snapshot)?.is_empty() {
            return Ok(true);
        }
        thread::sleep(CODEX_PROCESS_POLL_INTERVAL);
    }
    live_snapshot_processes(snapshot).map(|processes| processes.is_empty())
}

/// 向仍与 snapshot 完全一致的官方旧进程发送一轮信号。
/// 流程：按 helper/listener 优先、主进程最后排序；每个信号前重新验证 listener 集合和目标出生身份/路径/签名，再调用受限 kill。
/// 参数：snapshot 为副作用前证据，signal 仅允许 SIGTERM 或 SIGKILL；返回整轮结果。
/// 异常/边界：新 listener、PID 复用或身份变化立即停止；ESRCH 视为已退出，EPERM/其它系统错误返回稳定失败。
#[cfg(target_os = "macos")]
fn signal_snapshot_processes(snapshot: &CodexRestartSnapshot, signal: i32) -> Result<(), String> {
    if !matches!(signal, libc::SIGTERM | libc::SIGKILL) {
        return Err("Codex 重启信号不在允许范围".to_string());
    }
    for process in ordered_snapshot_processes(snapshot) {
        validate_snapshot_listener_set(snapshot)?;
        if snapshot_process_is_live_and_matching(process)? {
            send_restricted_signal(process.pid, signal)?;
        }
    }
    Ok(())
}

/// 返回 snapshot 固定信号顺序，listener/helper 在前、主进程在后，同角色按 PID 稳定排序。
/// 流程：复制引用后按角色权重和 PID 排序；参数为 snapshot；返回只读引用列表。
/// 异常/边界：不根据当前进程状态改变顺序，确保 KILL 不会先杀主进程破坏 helper 的来源证据。
fn ordered_snapshot_processes(snapshot: &CodexRestartSnapshot) -> Vec<&RestartProcessSnapshot> {
    let mut processes = snapshot.processes.iter().collect::<Vec<_>>();
    processes.sort_by_key(|process| {
        (
            match process.role {
                RestartProcessRole::ListenerHelper => 0_u8,
                RestartProcessRole::Main => 1_u8,
            },
            process.pid,
        )
    });
    processes
}

/// 读取 snapshot 中当前仍存活且身份未变化的进程。
/// 流程：先拒绝任何新 listener，再逐项复核出生身份、真实 executable 与 strict codesign；参数为 snapshot；返回存活记录引用。
/// 异常/边界：已退出项跳过；PID 存在但任一身份不等即视为复用/替换并 fail closed。
#[cfg(target_os = "macos")]
fn live_snapshot_processes(
    snapshot: &CodexRestartSnapshot,
) -> Result<Vec<&RestartProcessSnapshot>, String> {
    validate_snapshot_listener_set(snapshot)?;
    let mut live = Vec::new();
    for process in &snapshot.processes {
        if snapshot_process_is_live_and_matching(process)? {
            live.push(process);
        }
    }
    Ok(live)
}

/// 确认当前 listener 集合不包含 snapshot 之外的新 PID，并复核仍监听的旧 PID 身份。
/// 流程：读取 lsof 集合后做子集判断，再定位对应 snapshot 并验证身份；参数为 immutable snapshot；返回安全结果。
/// 异常/边界：任何新 PID、缺失 snapshot 记录或探测失败立即错误；旧 listener 已退出允许继续。
#[cfg(target_os = "macos")]
fn validate_snapshot_listener_set(snapshot: &CodexRestartSnapshot) -> Result<(), String> {
    let current_listener_pids = listener_pids_on_port(CODEX_CDP_PORT)?;
    if !listener_set_is_snapshot_subset(&current_listener_pids, &snapshot.listener_pids) {
        return Err("检测到 snapshot 之外的新 Codex listener".to_string());
    }
    for pid in current_listener_pids {
        let process = snapshot
            .processes
            .iter()
            .find(|process| process.pid == pid && process.was_listener)
            .ok_or_else(|| "当前 listener 缺少 immutable snapshot".to_string())?;
        if !snapshot_process_is_live_and_matching(process)? {
            return Err("snapshot listener 身份已变化".to_string());
        }
    }
    Ok(())
}

/// 纯函数判断当前 listener PID 是否全部来自副作用前 snapshot。
/// 流程：逐项执行精确 PID 集合包含判断；参数为当前和 snapshot PID；返回是否无新 listener。
/// 异常/边界：不接受数量、排序或近似关系替代精确成员判断。
fn listener_set_is_snapshot_subset(current: &[i32], snapshot: &[i32]) -> bool {
    current.iter().all(|pid| snapshot.contains(pid))
}

/// 判断一个 snapshot PID 当前是已退出还是仍保持完整身份。
/// 流程：先读出生身份；不存在返回 false，存在则重读 executable 和 strict codesign 并与 snapshot 精确比较。
/// 参数：process 为旧记录；返回 true 表示可以继续等待或发送信号。异常/边界：任一字段变化显式错误，禁止把 PID 复用者当旧进程。
#[cfg(target_os = "macos")]
fn snapshot_process_is_live_and_matching(process: &RestartProcessSnapshot) -> Result<bool, String> {
    let Some(start_identity) = process_start_identity(process.pid)? else {
        return Ok(false);
    };
    let executable = process_executable_path(process.pid)?;
    let signature = code_signature_identity(&executable)?;
    if !snapshot_identity_matches(process, &start_identity, &executable, &signature) {
        return Err("Codex snapshot PID 身份发生变化".to_string());
    }
    Ok(true)
}

/// 纯函数比较当前观测与 immutable snapshot 的出生身份、路径和签名。
/// 流程：三个身份维度全部精确相等才返回 true；参数为 snapshot 和当前观测字段。
/// 异常/边界：不比较可变标题、命令行或父 PID，旧父链只作为副作用前受信来源证据保存。
fn snapshot_identity_matches(
    snapshot: &RestartProcessSnapshot,
    start_identity: &str,
    executable: &str,
    signature: &CodeSignatureIdentity,
) -> bool {
    snapshot.start_identity == start_identity
        && snapshot.executable == executable
        && snapshot.signature == *signature
}

/// 读取 PID 的微秒级不可变进程出生身份。
/// 流程：调用 macOS `proc_pidinfo(PROC_PIDTBSDINFO)`，组合 pbi_start_tvsec/tvusec；参数为 PID；返回 None 表示进程已退出。
/// 异常/边界：结构长度、返回 PID 或时间异常显式失败；调用失败时用 kill(pid,0) 仅区分 ESRCH，权限/其它错误绝不降级为退出。
#[cfg(target_os = "macos")]
fn process_start_identity(pid: i32) -> Result<Option<String>, String> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let expected_size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: buffer 指向按 proc_bsdinfo 大小分配且对齐的 MaybeUninit；仅在返回完整结构字节数后 assume_init。
    let read_size = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            expected_size as i32,
        )
    };
    if read_size != expected_size as i32 {
        // SAFETY: signal=0 不发送信号，只原子检查该 PID 当前是否存在。
        let exists = unsafe { libc::kill(pid, 0) };
        if exists != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err("读取 Codex 进程微秒出生身份失败".to_string());
    }
    // SAFETY: proc_pidinfo 已确认写入完整 proc_bsdinfo 结构。
    let info = unsafe { info.assume_init() };
    if info.pbi_pid != pid as u32 || info.pbi_start_tvsec == 0 || info.pbi_start_tvusec >= 1_000_000
    {
        return Err("Codex 进程微秒出生身份结果无效".to_string());
    }
    Ok(Some(format!(
        "{}:{:06}",
        info.pbi_start_tvsec, info.pbi_start_tvusec
    )))
}

/// 对 snapshot 中仍匹配的单个 PID 发送 TERM/KILL，并稳定处理系统错误。
/// 流程：调用 libc::kill；成功或 ESRCH 返回 Ok，EPERM 与其它 errno 返回固定错误文案；参数为精确 PID 和白名单信号。
/// 异常/边界：不接收 PID 列表、名称或用户输入；调用方必须已完成当次身份复核。
#[cfg(target_os = "macos")]
fn send_restricted_signal(pid: i32, signal: i32) -> Result<(), String> {
    // SAFETY: pid 来自 immutable snapshot 且调用前已复核出生身份、路径和 strict codesign；signal 仅允许 TERM/KILL。
    let result = unsafe { libc::kill(pid, signal) };
    let errno = (result != 0)
        .then(|| std::io::Error::last_os_error().raw_os_error())
        .flatten();
    classify_restricted_signal_result(result, errno)
}

/// 纯函数把 libc::kill 结果收敛为稳定业务错误。
/// 流程：成功和 ESRCH 视为目标已退出，EPERM 与其它 errno 映射固定文案；参数为返回值和即时捕获 errno；返回统一结果。
/// 异常/边界：不回显 PID、信号或系统错误正文，避免诊断泄露并保证测试无需真实发送信号。
fn classify_restricted_signal_result(result: i32, errno: Option<i32>) -> Result<(), String> {
    if result == 0 {
        return Ok(());
    }
    match errno {
        Some(libc::ESRCH) => Ok(()),
        Some(libc::EPERM) => Err("没有权限结束已验证的官方 Codex 旧进程".to_string()),
        _ => Err("结束已验证的官方 Codex 旧进程失败".to_string()),
    }
}

/// 验证指定端口的全部监听 PID 是否属于受信 Codex App 进程树。
/// 流程：先用 lsof 精确读取 LISTEN PID；每个 PID 必须属于已验证主进程树，随后对其真实 executable 执行 strict codesign 并匹配 Team 与 Identifier 白名单。
/// 参数：port 为待验证的回环 TCP 端口；返回 None 表示未监听，Some(true/false) 表示监听者受信或未知。
/// 异常/边界：lsof、可执行文件或父 PID 探测失败显式返回错误；不因 HTTP 内容看似 Chromium 而信任未知进程。
#[cfg(target_os = "macos")]
fn trusted_listener_on_port(port: u16) -> Result<Option<bool>, String> {
    let listener_pids = listener_pids_on_port(port)?;
    if listener_pids.is_empty() {
        let connected = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            Duration::from_millis(80),
        )
        .is_ok();
        return if connected {
            Err("端口可连接但未能确认监听进程".to_string())
        } else {
            Ok(None)
        };
    }
    let main_pids = codex_main_pids()?;
    if main_pids.is_empty() {
        return Ok(Some(false));
    }
    for listener_pid in listener_pids {
        if !process_descends_from(listener_pid, &main_pids)? {
            return Ok(Some(false));
        }
        let executable = process_executable_path(listener_pid)?;
        let signature = code_signature_identity(&executable)?;
        if !listener_signature_is_allowed(&signature) {
            return Ok(Some(false));
        }
    }
    Ok(Some(true))
}

/// codesign 输出中解析出的最小签名身份。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeSignatureIdentity {
    /// macOS 签名 Identifier。
    identifier: String,
    /// Apple Developer TeamIdentifier。
    team_identifier: String,
}

/// 读取监听进程可执行文件的严格 codesign 身份。
/// 流程：先独立执行 `codesign --verify --strict` 验证代码完整性，成功后再执行 `codesign -dv` 读取字段并交给纯解析函数。
/// 异常/边界：bundle 内外一视同仁；任一步失败、非 UTF-8、字段缺失或重复均显式失败，不按路径位置跳过签名验证。
#[cfg(target_os = "macos")]
fn code_signature_identity(executable: &str) -> Result<CodeSignatureIdentity, String> {
    let cache = CODEX_SIGNATURE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut signatures = cache
        .lock()
        .map_err(|_| "Codex 进程签名缓存不可用".to_string())?;
    if let Some(identity) = signatures.get(executable) {
        return Ok(identity.clone());
    }
    let verification = Command::new("codesign")
        .args(["--verify", "--strict", "--verbose=2", executable])
        .output()
        .map_err(|_| "无法执行 Codex 进程严格签名验证".to_string())?;
    if !verification.status.success() {
        return Err("Codex 进程严格签名验证失败".to_string());
    }
    let output = Command::new("codesign")
        .args(["-dv", "--verbose=4", executable])
        .output()
        .map_err(|_| "无法验证 Codex 外部 helper 签名".to_string())?;
    if !output.status.success() {
        return Err("验证 Codex 外部 helper 签名失败".to_string());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "Codex 外部 helper 签名结果无效".to_string())?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|_| "Codex 外部 helper 签名结果无效".to_string())?;
    let identity = parse_code_signature_identity(&format!("{stdout}\n{stderr}"))?;
    signatures.insert(executable.to_string(), identity.clone());
    Ok(identity)
}

/// 从 codesign 详细文本严格提取 Identifier 与 TeamIdentifier。
/// 流程：逐行只接受精确字段前缀，要求两个字段各出现一次且非空；参数为命令输出；返回最小签名身份。
/// 异常/边界：缺失、重复、空值或近似字段名均拒绝，防止异常输出被误判为官方 helper。
fn parse_code_signature_identity(output: &str) -> Result<CodeSignatureIdentity, String> {
    let identifiers = output
        .lines()
        .filter_map(|line| line.strip_prefix("Identifier="))
        .collect::<Vec<_>>();
    let team_identifiers = output
        .lines()
        .filter_map(|line| line.strip_prefix("TeamIdentifier="))
        .collect::<Vec<_>>();
    if identifiers.len() != 1
        || team_identifiers.len() != 1
        || identifiers[0].trim().is_empty()
        || team_identifiers[0].trim().is_empty()
    {
        return Err("Codex 外部 helper 签名身份字段无效".to_string());
    }
    Ok(CodeSignatureIdentity {
        identifier: identifiers[0].trim().to_string(),
        team_identifier: team_identifiers[0].trim().to_string(),
    })
}

/// 判断监听进程是否命中官方签名白名单。
/// 流程：精确比较 TeamIdentifier，并要求 Identifier 是主进程、官方 helper、renderer 或 Sky CUAService 之一。
/// 异常/边界：不允许仅同团队、任意 bundle 子进程、前后缀或大小写折叠匹配。
fn listener_signature_is_allowed(identity: &CodeSignatureIdentity) -> bool {
    identity.team_identifier == CODEX_TEAM_IDENTIFIER
        && CODEX_SIGNING_IDENTIFIER_ALLOWLIST.contains(&identity.identifier.as_str())
}

/// 从 `ps -ww -axo pid=,comm=` 输出中提取精确 Codex 主程序候选。
/// 流程：逐行拆分 PID 与完整 comm，只保留 comm 精确等于固定主程序路径的记录，并按首次出现顺序去重。
/// 参数：output 为 ps 标准输出；返回待执行真实 executable 与签名复核的正整数 PID 列表。
/// 异常/边界：非目标行安全忽略；目标路径对应的 PID 非法时显式失败，不接受参数、前后缀、basename 或截断路径匹配。
fn parse_main_process_candidates(output: &str) -> Result<Vec<i32>, String> {
    let mut pids = Vec::new();
    for line in output.lines() {
        let mut fields = line.trim_start().splitn(2, char::is_whitespace);
        let Some(pid_field) = fields.next() else {
            continue;
        };
        let Some(command) = fields.next().map(str::trim) else {
            continue;
        };
        if command != CODEX_MAIN_PATH {
            continue;
        }
        let pid = pid_field
            .parse::<i32>()
            .map_err(|_| "Codex Desktop 主进程标识无效".to_string())?;
        if pid <= 0 {
            return Err("Codex Desktop 主进程标识无效".to_string());
        }
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    Ok(pids)
}

/// 读取精确 TCP LISTEN 端口对应的 PID 集合。
/// 流程：调用 lsof 的字段模式，只解析 `p` 前缀正整数并去重；参数为端口；返回监听 PID。
/// 异常/边界：退出码 1 代表无监听；其它失败、空字段或非法 PID 均视为状态探测失败。
#[cfg(target_os = "macos")]
fn listener_pids_on_port(port: u16) -> Result<Vec<i32>, String> {
    let output = Command::new("lsof")
        .args(["-nP", "-a", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fp"])
        .output()
        .map_err(|_| "无法读取 Codex 连接端口监听进程".to_string())?;
    if output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        return Err("读取 Codex 连接端口监听进程失败".to_string());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "Codex 连接端口监听进程结果无效".to_string())?;
    let mut pids = Vec::new();
    for line in stdout.lines().filter(|line| line.starts_with('p')) {
        let pid = line[1..]
            .parse::<i32>()
            .map_err(|_| "Codex 连接端口监听进程标识无效".to_string())?;
        if pid <= 0 {
            return Err("Codex 连接端口监听进程标识无效".to_string());
        }
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    if pids.is_empty() {
        return Err("Codex 连接端口监听进程结果为空".to_string());
    }
    Ok(pids)
}

/// 读取 PID 当前映射的真实可执行文件路径。
/// 流程：使用 lsof 的 txt 文件描述符字段读取进程映像，选择第一个绝对路径；参数为 PID；返回可执行文件路径。
/// 异常/边界：不使用可伪造的命令行参数；进程退出、权限不足或路径缺失均显式失败。
#[cfg(target_os = "macos")]
fn process_executable_path(pid: i32) -> Result<String, String> {
    let output = Command::new("lsof")
        .args(["-nP", "-a", "-p", &pid.to_string(), "-d", "txt", "-Fn"])
        .output()
        .map_err(|_| "无法读取 Codex 监听进程可执行文件".to_string())?;
    if !output.status.success() {
        return Err("读取 Codex 监听进程可执行文件失败".to_string());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "Codex 监听进程可执行文件结果无效".to_string())?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix('n').filter(|path| path.starts_with('/')))
        .map(str::to_string)
        .ok_or_else(|| "Codex 监听进程可执行文件路径缺失".to_string())
}

/// 判断监听 PID 的父进程链是否命中任一精确 Codex 主进程。
/// 流程：从当前 PID 向上最多追溯 64 层，每层先匹配主 PID 再读取 PPID；参数为监听 PID 和主 PID 集合；返回是否属于受信树。
/// 异常/边界：循环、非法 PPID、超深链或 ps 失败均显式失败，不把孤儿进程当作 Codex 后代。
#[cfg(target_os = "macos")]
fn process_descends_from(pid: i32, main_pids: &[i32]) -> Result<bool, String> {
    process_parent_chain_to_main(pid, main_pids).map(|chain| chain.is_some())
}

/// 读取 PID 到任一已验证主进程的完整父链。
/// 流程：从当前 PID 向上最多追溯 64 层并记录每个节点，命中主 PID 时返回链；参数为起点和主 PID；返回可选父链。
/// 异常/边界：循环、非法 PPID 返回 None，ps 失败或超深链显式错误；snapshot 使用返回链证明 listener/helper 的副作用前来源。
#[cfg(target_os = "macos")]
fn process_parent_chain_to_main(
    mut pid: i32,
    main_pids: &[i32],
) -> Result<Option<Vec<i32>>, String> {
    let mut visited = Vec::new();
    for _ in 0..64 {
        visited.push(pid);
        if main_pids.contains(&pid) {
            return Ok(Some(visited));
        }
        if pid <= 1 {
            return Ok(None);
        }
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "ppid="])
            .output()
            .map_err(|_| "无法读取 Codex 监听进程父进程".to_string())?;
        if !output.status.success() {
            return Err("读取 Codex 监听进程父进程失败".to_string());
        }
        let parent_pid = String::from_utf8(output.stdout)
            .map_err(|_| "Codex 监听进程父进程结果无效".to_string())?
            .trim()
            .parse::<i32>()
            .map_err(|_| "Codex 监听进程父进程标识无效".to_string())?;
        if parent_pid <= 0 || visited.contains(&parent_pid) {
            return Ok(None);
        }
        pid = parent_pid;
    }
    Err("Codex 监听进程父进程链超过安全上限".to_string())
}

/// 等待旧 Codex listener 全部退出并确认固定端口不可连接。
/// 流程：在截止时间内同时读取 lsof LISTEN PID 和 TCP 可连接性，只有二者都为空才允许后续执行 `open -na`；参数为最长等待时间；返回是否释放。
/// 异常/边界：PID 探测失败立即返回错误；出现新 listener 或残留 helper 时只等待/失败，绝不结束未知 PID，也不漂移端口。
#[cfg(target_os = "macos")]
fn wait_for_port_release(timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let listener_pids = listener_pids_on_port(CODEX_CDP_PORT)?;
        let tcp_connectable = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CDP_PORT),
            Duration::from_millis(80),
        )
        .is_ok();
        if port_release_observed(&listener_pids, tcp_connectable) {
            return Ok(true);
        }
        thread::sleep(CODEX_PROCESS_POLL_INTERVAL);
    }
    let listener_pids = listener_pids_on_port(CODEX_CDP_PORT)?;
    let tcp_connectable = TcpStream::connect_timeout(
        &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CDP_PORT),
        Duration::from_millis(80),
    )
    .is_ok();
    Ok(port_release_observed(&listener_pids, tcp_connectable))
}

/// 纯函数判断旧监听是否已经满足启动下一实例的释放门禁。
/// 流程：要求 lsof PID 集合为空且 TCP 不可连接；参数为同一次轮询的两个观测；返回能否执行 open。
/// 异常/边界：任一信号仍显示占用都返回 false，避免 lsof/TCP 短暂不同步时提前启动。
fn port_release_observed(listener_pids: &[i32], tcp_connectable: bool) -> bool {
    listener_pids.is_empty() && !tcp_connectable
}

/// 构造后台重启失败结果。
/// 流程：复制固定错误码和安全文案；参数均来自内部常量；返回可存入运行时状态的失败对象。
/// 异常/边界：不得传入命令输出、路径、端口、PID 或外部响应正文。
#[cfg(target_os = "macos")]
fn restart_failure(code: &str, message: &str) -> CodexRestartFailure {
    CodexRestartFailure {
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// 返回当前 Unix 毫秒字符串。
/// 流程：计算系统时间与 Unix epoch 的安全差值；参数无；返回文档化时间值。
/// 异常/边界：系统时钟早于 epoch 时回落为 0，不影响连接判断。
fn current_unix_millis() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[cfg(target_os = "macos")]
    fn live_codex_connection_probe_for_current_fix() {
        assert!(!codex_main_pids()
            .expect("应读取当前 Codex 主进程")
            .is_empty());
        assert_eq!(
            trusted_listener_on_port(CODEX_CDP_PORT).expect("应完成 listener 身份探针"),
            Some(true)
        );
        assert!(probe_codex_cdp().expect("当前官方 Codex listener 应通过身份探针"));
    }

    /// 构造不触发系统进程操作的 immutable snapshot 测试记录。
    fn restart_process_snapshot(pid: i32, role: RestartProcessRole) -> RestartProcessSnapshot {
        RestartProcessSnapshot {
            pid,
            start_identity: format!("100:{pid:06}"),
            executable: format!("/trusted/{pid}"),
            signature: CodeSignatureIdentity {
                identifier: if role == RestartProcessRole::Main {
                    "com.openai.codex".to_string()
                } else {
                    "com.openai.sky.CUAService".to_string()
                },
                team_identifier: CODEX_TEAM_IDENTIFIER.to_string(),
            },
            role,
            was_listener: role == RestartProcessRole::ListenerHelper,
            parent_chain: if role == RestartProcessRole::Main {
                vec![pid]
            } else {
                vec![pid, 12_111]
            },
        }
    }

    /// renderer 身份判断必须同时限制类型、标题、URL 和回环 WebSocket，拒绝普通 Chromium 或临时页面。
    #[test]
    fn codex_target_validation_fails_closed() {
        let valid = json!({
            "type": "page",
            "title": "Codex",
            "url": "app://-/index.html",
            "webSocketDebuggerUrl": "ws://127.0.0.1:9333/devtools/page/one"
        });
        assert_eq!(
            select_codex_page_websocket_url(std::slice::from_ref(&valid))
                .expect("唯一目标应可选择"),
            Some("ws://127.0.0.1:9333/devtools/page/one".to_string())
        );
        for invalid in [
            json!({"type":"page","title":"Chrome","url":"http://example.com","webSocketDebuggerUrl":"ws://127.0.0.1:9333/devtools/page/one"}),
            json!({"type":"page","title":"Codex","url":"app://codex/index.html?initialRoute=x","webSocketDebuggerUrl":"ws://127.0.0.1:9333/devtools/page/one"}),
            json!({"type":"page","title":"Codex","url":"app://codex/index.html","webSocketDebuggerUrl":"ws://192.168.1.2:9333/devtools/page/one"}),
            json!({"type":"page","title":"Chrome","url":"app://-/index.html","webSocketDebuggerUrl":"ws://127.0.0.1:9333/devtools/page/one"}),
            json!({"type":"page","title":"Codex","url":"app://-/index.html","webSocketDebuggerUrl":"ws://127.0.0.1:9333/devtools/page/one?token=x"}),
        ] {
            assert!(select_codex_page_websocket_url(&[invalid])
                .expect("无效目标应安全忽略")
                .is_none());
        }
        assert!(select_codex_page_websocket_url(&[valid.clone(), valid]).is_err());
    }

    /// ps 主进程候选只接受固定完整路径和去重正整数 PID，basename、参数、前后缀与异常字段必须拒绝。
    #[test]
    fn main_process_candidate_parsing_is_path_exact_and_deduplicated() {
        assert_eq!(
            parse_main_process_candidates(
                " 12111 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n\
                 12111 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n\
                 12948 /Users/example/SkyComputerUseService\n"
            )
            .unwrap(),
            [12_111]
        );
        assert!(parse_main_process_candidates(
            "0 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n"
        )
        .is_err());
        assert!(parse_main_process_candidates(
            "pid /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n"
        )
        .is_err());
        assert!(parse_main_process_candidates(
            "12111 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT --fake\n\
             12112 ChatGPT\n\
             12113 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT-copy\n"
        )
        .unwrap()
        .is_empty());
    }

    /// 即使伪服务监听并伪造 HTTP 响应，只要监听进程不属于 Codex App bundle 受信树，就必须在读取 HTTP 前拒绝。
    #[cfg(target_os = "macos")]
    #[test]
    fn fake_local_listener_is_rejected_by_process_identity() {
        let listener =
            std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("应创建临时伪服务监听器");
        let port = listener.local_addr().expect("应读取临时端口").port();
        assert!(matches!(
            probe_codex_cdp_state_on_port(port),
            CodexProbeState::Untrusted | CodexProbeState::StateFailed
        ));
    }

    /// 所有 listener 必须同时命中 OpenAI Team 和精确 Identifier 白名单，且签名输出字段必须唯一。
    #[test]
    fn listener_signature_requires_exact_double_allowlist() {
        let identity = parse_code_signature_identity(
            "Executable=/tmp/helper\nIdentifier=com.openai.sky.CUAService\nTeamIdentifier=2DC432GLL2\n",
        )
        .expect("官方签名字段应解析");
        assert!(listener_signature_is_allowed(&identity));
        for identifier in CODEX_SIGNING_IDENTIFIER_ALLOWLIST {
            assert!(listener_signature_is_allowed(&CodeSignatureIdentity {
                identifier: identifier.to_string(),
                team_identifier: CODEX_TEAM_IDENTIFIER.to_string(),
            }));
        }
        assert!(!listener_signature_is_allowed(&CodeSignatureIdentity {
            identifier: "com.openai.sky.Other".to_string(),
            team_identifier: CODEX_TEAM_IDENTIFIER.to_string(),
        }));
        assert!(!listener_signature_is_allowed(&CodeSignatureIdentity {
            identifier: "com.openai.sky.CUAService".to_string(),
            team_identifier: "OTHERTEAM".to_string(),
        }));
        assert!(parse_code_signature_identity("Identifier=com.openai.sky.CUAService\n").is_err());
        assert!(parse_code_signature_identity(
            "Identifier=com.openai.sky.CUAService\nIdentifier=duplicate\nTeamIdentifier=2DC432GLL2"
        )
        .is_err());
        let production = include_str!("codex_desktop.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        assert!(production.contains("[\"--verify\", \"--strict\", \"--verbose=2\", executable]"));
        assert!(production.contains("[\"-ww\", \"-axo\", \"pid=,comm=\"]"));
        assert!(!production.contains("[\"-x\", \"ChatGPT\"]"));
        assert!(!production.contains("[\"-f\""));
    }

    /// 下一实例只能在 lsof 与 TCP 同时确认旧端口释放后启动。
    #[test]
    fn restart_open_gate_requires_listener_and_tcp_release() {
        assert!(port_release_observed(&[], false));
        assert!(!port_release_observed(&[12_111], false));
        assert!(!port_release_observed(&[], true));
        assert!(!port_release_observed(&[12_111, 12_948], true));
    }

    /// TERM/KILL 顺序必须始终为旧 listener/helper 在前、主进程最后，且新 listener PID 必须被 snapshot 集合拒绝。
    #[test]
    fn restart_snapshot_orders_helpers_first_and_rejects_new_listener() {
        let snapshot = CodexRestartSnapshot {
            processes: vec![
                restart_process_snapshot(12_111, RestartProcessRole::Main),
                restart_process_snapshot(12_948, RestartProcessRole::ListenerHelper),
                restart_process_snapshot(12_949, RestartProcessRole::ListenerHelper),
            ],
            listener_pids: vec![12_111, 12_948],
        };
        assert_eq!(
            ordered_snapshot_processes(&snapshot)
                .iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>(),
            [12_948, 12_949, 12_111]
        );
        assert!(listener_set_is_snapshot_subset(
            &[12_111, 12_948],
            &snapshot.listener_pids
        ));
        assert!(!listener_set_is_snapshot_subset(
            &[12_111, 13_000],
            &snapshot.listener_pids
        ));
    }

    /// PID 只有出生微秒身份、真实路径和 strict 签名全部不变时才可继续发送信号；任一变化都代表复用或替换。
    #[test]
    fn restart_snapshot_identity_match_is_exact() {
        let process = restart_process_snapshot(12_948, RestartProcessRole::ListenerHelper);
        assert!(snapshot_identity_matches(
            &process,
            &process.start_identity,
            &process.executable,
            &process.signature
        ));
        assert!(!snapshot_identity_matches(
            &process,
            "100:999999",
            &process.executable,
            &process.signature
        ));
        assert!(!snapshot_identity_matches(
            &process,
            &process.start_identity,
            "/trusted/replaced",
            &process.signature
        ));
        assert!(!snapshot_identity_matches(
            &process,
            &process.start_identity,
            &process.executable,
            &CodeSignatureIdentity {
                identifier: "com.openai.sky.Other".to_string(),
                team_identifier: CODEX_TEAM_IDENTIFIER.to_string(),
            }
        ));
    }

    /// kill 返回值必须把 ESRCH 当作已退出，同时稳定区分 EPERM 与其它失败且不需要真实发送信号。
    #[test]
    fn restricted_signal_result_handles_esrch_and_permission_errors() {
        assert!(classify_restricted_signal_result(0, None).is_ok());
        assert!(classify_restricted_signal_result(-1, Some(libc::ESRCH)).is_ok());
        assert!(classify_restricted_signal_result(-1, Some(libc::EPERM))
            .expect_err("EPERM 必须失败")
            .contains("权限"));
        assert!(classify_restricted_signal_result(-1, Some(libc::EINVAL)).is_err());
    }

    /// macOS proc_pidinfo 必须为当前测试进程稳定返回同一微秒级出生身份。
    #[cfg(target_os = "macos")]
    #[test]
    fn process_birth_identity_is_stable_and_microsecond_precise() {
        let pid = std::process::id() as i32;
        let first = process_start_identity(pid)
            .expect("当前进程出生身份应可读取")
            .expect("当前进程必须存在");
        let second = process_start_identity(pid)
            .expect("当前进程出生身份应可重复读取")
            .expect("当前进程必须存在");
        assert_eq!(first, second);
        let microseconds = first.split_once(':').expect("出生身份应包含秒和微秒").1;
        assert_eq!(microseconds.len(), 6);
    }

    /// 连接快照必须使用 camelCase 且不包含 CDP 端口、WebSocket 或 PID 字段。
    #[test]
    fn connection_status_serialization_exposes_only_public_fields() {
        let status = CodexConnectionStatus {
            state: CodexConnectionState::Connected,
            connected: true,
            desktop_running: true,
            can_restart: true,
            reason_code: "CODEX_CONNECTED".to_string(),
            message: "ok".to_string(),
            checked_at: "1".to_string(),
        };
        let value = serde_json::to_value(status).expect("状态应可序列化");
        assert_eq!(value["desktopRunning"], true);
        assert_eq!(value["reasonCode"], "CODEX_CONNECTED");
        assert_eq!(value["canRestart"], true);
        assert!(value.get("port").is_none());
        assert!(value.get("webSocketDebuggerUrl").is_none());
        assert!(value.get("pid").is_none());
    }
}
