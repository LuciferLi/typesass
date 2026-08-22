use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::my_apps::{
    CreateMyAppParams, MyAppIdParams, OpenMyAppParams, RuntimeMyApps, UpdateMyAppParams,
};
use crate::task_store::{
    CreateProjectRequest, CreateTaskRequest, TaskAttachmentRecord, UpdateProjectRequest,
    UpdateTaskRequest,
};
use crate::{
    complete_session_task_core, create_session_project_core, create_session_task_core,
    delete_session_project_core, delete_session_task_core, get_codex_connection_core,
    list_codex_threads_core, list_codex_workspaces_core, load_session_workspace_data_core,
    open_session_external_thread_core, queue_session_task_core, read_codex_thread_messages_core,
    request_access_token_approval_core, restart_codex_core, update_session_project_core,
    update_session_task_core, CodexThreadListRequest,
};

/// 私有 RPC 单次请求 JSON 的最大长度；与公开 HTTP body 上限对齐，覆盖我的应用 zip data URL 上传。
const MAX_REQUEST_BYTES: usize = 12 * 1024 * 1024;
/// 私有 RPC 单次响应 JSON 的最大长度，覆盖有限工作区聚合且阻止响应放大。
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// 编译期容量契约：TaskStore 聚合必须给 RPC envelope 至少保留 1 MiB，不允许两层上限独立漂移。
const _: () =
    assert!(MAX_RESPONSE_BYTES - crate::task_store::WORKSPACE_RESPONSE_BUDGET_BYTES >= 1024 * 1024);
/// 每条私有 RPC 连接的读写期限，授权确认会等待用户操作，因此需要覆盖一次弹窗确认窗口。
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(75);
/// 非阻塞 listener 无连接时的短轮询间隔，确保 App 退出可及时回收线程。
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
/// 私有 RPC 同时执行业务分发的固定线程数，避免单个慢 Codex 调用阻塞所有 HTTP 请求。
const MAX_CONCURRENT_CONNECTIONS: usize = 8;
/// worker 全忙时允许短暂排队的连接数；超过后立即返回 RPC_BUSY，不形成无界线程或内存队列。
const MAX_PENDING_CONNECTIONS: usize = 8;
/// FastAPI 可调用的私有 RPC 方法全集；未登记方法在进入业务分发器前默认拒绝。
const ALLOWED_METHODS: [&str; 23] = [
    "requestAccessTokenApproval",
    "loadWorkspaceData",
    "createProject",
    "updateProject",
    "deleteProject",
    "createTask",
    "updateTask",
    "deleteTask",
    "queueTask",
    "completeTask",
    "listCodexWorkspaces",
    "listCodexThreads",
    "openCodexThread",
    "readCodexThreadMessages",
    "getCodexConnection",
    "restartCodex",
    "listMyApps",
    "allocateMyAppPort",
    "createMyApp",
    "updateMyApp",
    "deleteMyApp",
    "restartMyApp",
    "openMyApp",
];

/// 传给 FastAPI sidecar 的私有 RPC 启动配置。
/// 该对象只进入一次性 stdin bootstrap，不写入环境变量、命令行、日志或 WebView。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrivateRpcBootstrap {
    /// App 数据目录内权限为 0600 的 Unix domain socket 路径。
    pub(crate) socket_path: String,
    /// 每次 App 启动重新生成的 48 字节随机凭据的 URL-safe 表示。
    pub(crate) secret: String,
}

/// App 生命周期持有的私有 RPC listener 状态。
#[derive(Default)]
pub(crate) struct RuntimePrivateRpc {
    /// listener 线程、停止信号和启动凭据受同一把锁保护，防止重复启动或跨代配置。
    state: Mutex<Option<ManagedPrivateRpc>>,
}

/// 单代私有 RPC listener 的受管资源。
struct ManagedPrivateRpc {
    /// socket 文件绝对路径，仅用于 sidecar bootstrap 和精确清理。
    socket_path: PathBuf,
    /// 当前代高熵凭据，只保留在 Rust 内存并通过一次性 stdin 交给 sidecar。
    secret: String,
    /// 通知非阻塞 listener 停止接受新连接。
    stopping: Arc<AtomicBool>,
    /// listener 工作线程，退出时必须 join，确保 socket 不再被使用。
    worker: Option<thread::JoinHandle<()>>,
}

/// 私有 RPC 请求 envelope。
/// 客户端必须在每个单请求连接中携带当前代 secret、allowlist 方法名和对象参数。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RpcRequest {
    /// 当前 App 启动代的私有凭据。
    secret: String,
    /// 受 allowlist 限制的业务方法名。
    method: String,
    /// 从公开 HTTP 请求贯穿到私有桥接的安全追踪 ID，不用于日志记录正文。
    request_id: String,
    /// 方法参数对象；无参数方法使用空对象。
    #[serde(default)]
    params: Value,
}

/// 私有 RPC 成功响应 envelope。
#[derive(Serialize)]
struct RpcSuccessResponse {
    /// 固定为 true，供 Python 桥接严格区分成功与错误。
    ok: bool,
    /// Rust 唯一业务所有者返回的 JSON 结果。
    result: Value,
}

/// 私有 RPC 错误响应中的稳定错误信息。
#[derive(Debug, Serialize)]
struct RpcErrorBody {
    /// 可由 Python 映射为公共 HTTP 状态的稳定错误码。
    code: String,
    /// 不含 secret、socket 路径、prompt 或结果正文的用户可读错误。
    message: String,
}

/// 私有 RPC 失败响应 envelope。
#[derive(Serialize)]
struct RpcErrorResponse {
    /// 固定为 false，禁止把业务失败伪装成成功结果。
    ok: bool,
    /// 稳定错误码和安全说明。
    error: RpcErrorBody,
}

/// 单字段 projectId 参数，用于工作区筛选和项目删除。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectIdParams {
    /// 项目 ID；工作区读取时允许不传或为 null。
    #[serde(default)]
    project_id: Option<String>,
}

/// 单字段 taskId 参数，用于排队和完成任务。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskIdParams {
    /// 目标任务稳定 ID。
    task_id: String,
}

/// Codex 会话 ID 参数，用于通过 deeplink 打开会话或读取会话详情。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadIdParams {
    /// 目标 Codex 会话 ID。
    thread_id: String,
    /// 向前分页锚点；读取详情时返回该顺序之前的消息，打开会话时忽略。
    #[serde(default)]
    before_message_order: Option<usize>,
    /// 读取详情窗口大小；打开会话时忽略。
    #[serde(default)]
    limit: Option<usize>,
}

/// 创建项目的严格 RPC 参数，避免公共 HTTP 未知字段静默进入 Rust 业务层。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateProjectParams {
    /// 项目展示名称。
    name: String,
    /// 已存在的本地工作空间绝对路径。
    workspace_path: String,
    /// 项目基础提示词；为空时后续任务只发送任务自身内容。
    #[serde(default)]
    base_prompt: String,
}

/// 更新项目的严格 RPC 参数，字段与现有 TaskStore 请求一一对应。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateProjectParams {
    /// 待更新项目稳定 ID。
    id: String,
    /// 新项目展示名称。
    name: String,
    /// 后续任务使用的本地工作空间绝对路径。
    workspace_path: String,
    /// 项目基础提示词；为空时后续任务只发送任务自身内容。
    #[serde(default)]
    base_prompt: String,
}

/// 创建任务的严格 RPC 参数，防止未知字段形成未实现的兼容协议。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateTaskParams {
    /// 任务所属项目稳定 ID。
    project_id: String,
    /// 任务看板标题。
    title: String,
    /// 真实发送给 Codex 的任务提示词。
    prompt: String,
    /// 随任务发送给 Codex 的图片附件。
    #[serde(default)]
    attachments: Vec<TaskAttachmentRecord>,
}

/// 更新任务的严格 RPC 参数，防止绕过状态机补写额外字段。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateTaskParams {
    /// 待更新任务稳定 ID。
    id: String,
    /// 任务新看板标题。
    title: String,
    /// 真实发送给 Codex 的新任务提示词。
    prompt: String,
    /// 随任务发送给 Codex 的图片附件；更新时整体替换。
    #[serde(default)]
    attachments: Vec<TaskAttachmentRecord>,
}

/// 查询 Codex 会话列表的严格 RPC 参数。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListCodexThreadsParams {
    /// 要查询的 Codex 工作空间绝对路径。
    workspace_cwd: String,
    /// 单页数量，后续由现有核心再次执行范围收敛。
    limit: i64,
    /// 分页偏移，负数后续按既有核心归零。
    offset: i64,
    /// 标题、预览或 thread ID 搜索关键词。
    keyword: String,
}

/// 申请 App 授权码的严格 RPC 参数。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccessTokenApprovalParams {
    /// 公开 HTTP 请求追踪 ID，前端确认时原样返回。
    request_id: String,
    /// 插件或外部调用方展示名称。
    name: String,
    /// 授权码到期时间；空值表示永久有效。
    #[serde(default)]
    expires_at: Option<String>,
}

impl RuntimePrivateRpc {
    /// 在 sidecar 启动前创建并接管本机私有 Unix Socket RPC。
    /// 流程：创建权限 0700 的专用目录、清理同名陈旧 socket、绑定 listener 并设为 0600，生成 48 字节系统随机 secret 后启动非阻塞服务线程。
    /// 参数：app 用于定位 App 数据目录并作为唯一 TaskStore/Codex 服务上下文；成功返回空值。
    /// 异常/边界：重复启动、非 Unix 平台、目录或权限设置失败均 fail-fast；不会扫描或删除约定路径以外的文件。
    pub fn start(&self, app: &AppHandle) -> Result<(), String> {
        #[cfg(not(unix))]
        {
            let _ = app;
            return Err("私有 RPC 仅支持 Unix domain socket 平台".to_string());
        }
        #[cfg(unix)]
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "私有 RPC 状态锁已损坏".to_string())?;
            if state.is_some() {
                return Err("私有 RPC 已在当前 App 生命周期运行".to_string());
            }
            let socket_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("读取私有 RPC 数据目录失败：{}", error))?
                .join("private-rpc");
            ensure_private_socket_dir(&socket_dir)?;
            fs::set_permissions(&socket_dir, fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("设置私有 RPC 数据目录权限失败：{}", error))?;
            let socket_path = socket_dir.join("aitool.sock");
            remove_stale_socket(&socket_path)?;
            let listener = UnixListener::bind(&socket_path)
                .map_err(|error| format!("绑定私有 RPC socket 失败：{}", error))?;
            if let Err(error) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            {
                let _ = fs::remove_file(&socket_path);
                return Err(format!("设置私有 RPC socket 权限失败：{}", error));
            }
            if let Err(error) = listener.set_nonblocking(true) {
                let _ = fs::remove_file(&socket_path);
                return Err(format!("设置私有 RPC listener 非阻塞模式失败：{}", error));
            }
            let mut secret_bytes = [0_u8; 48];
            OsRng.fill_bytes(&mut secret_bytes);
            let secret = URL_SAFE_NO_PAD.encode(secret_bytes);
            secret_bytes.fill(0);
            let stopping = Arc::new(AtomicBool::new(false));
            let worker_stopping = Arc::clone(&stopping);
            let worker_secret = secret.clone();
            let worker_app = app.clone();
            let worker = thread::spawn(move || {
                run_listener(listener, worker_stopping, worker_secret, worker_app)
            });
            *state = Some(ManagedPrivateRpc {
                socket_path,
                secret,
                stopping,
                worker: Some(worker),
            });
            Ok(())
        }
    }

    /// 取得当前 listener 的 sidecar bootstrap 配置。
    /// 流程：在状态锁内确认 listener 已启动，再复制 socket 绝对路径和当前代 secret 供一次性 stdin 序列化。
    /// 参数：无；返回仅供 Rust sidecar 启动器使用的配置。
    /// 异常/边界：未启动或状态锁损坏时拒绝启动 sidecar；调用方禁止日志记录返回值。
    pub fn bootstrap(&self) -> Result<PrivateRpcBootstrap, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "私有 RPC 状态锁已损坏".to_string())?;
        let managed = state
            .as_ref()
            .ok_or_else(|| "私有 RPC 尚未启动，无法启动 sidecar".to_string())?;
        Ok(PrivateRpcBootstrap {
            socket_path: managed.socket_path.to_string_lossy().into_owned(),
            secret: managed.secret.clone(),
        })
    }

    /// 读取当前 sidecar 启动代私有控制密钥。
    /// 流程：在状态锁内克隆已 bootstrap 给 sidecar 的高熵 secret，供 Rust 主进程调用内部控制接口。
    /// 参数：无。
    /// 返回：当前启动代 secret。
    /// 异常/边界：私有 RPC 未启动时返回稳定错误；调用方不得记录、回显或持久化该密钥。
    pub fn control_secret(&self) -> Result<String, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "私有 RPC 状态锁已损坏".to_string())?;
        let managed = state
            .as_ref()
            .ok_or_else(|| "私有 RPC 尚未启动，无法读取内部控制密钥".to_string())?;
        Ok(managed.secret.clone())
    }

    /// 停止当前 App 持有的私有 RPC listener 并删除精确 socket 文件。
    /// 流程：从状态中取出当前代资源、置停止标志并用本机连接唤醒 accept，join 工作线程后清除 secret 和 socket。
    /// 参数：无；幂等返回清理结果。
    /// 异常/边界：只删除本状态记录的 socket；线程异常或文件删除失败会显式返回，不影响调用方继续清理 sidecar。
    pub fn shutdown(&self) -> Result<(), String> {
        let managed = self
            .state
            .lock()
            .map_err(|_| "私有 RPC 状态锁已损坏".to_string())?
            .take();
        let Some(mut managed) = managed else {
            return Ok(());
        };
        managed.stopping.store(true, Ordering::Release);
        #[cfg(unix)]
        let _ = UnixStream::connect(&managed.socket_path);
        if let Some(worker) = managed.worker.take() {
            worker
                .join()
                .map_err(|_| "私有 RPC listener 线程异常退出".to_string())?;
        }
        managed.secret.clear();
        if managed.socket_path.exists() {
            fs::remove_file(&managed.socket_path)
                .map_err(|error| format!("删除私有 RPC socket 失败：{}", error))?;
        }
        Ok(())
    }
}

impl Drop for RuntimePrivateRpc {
    /// Runtime 被销毁时尽力停止 listener、清空凭据并删除 socket，作为 RunEvent 清理之外的最后防线。
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// 删除当前 App 私有目录内的同名陈旧 socket。
/// 流程：使用 symlink_metadata 精确检查目标；仅 socket 或普通文件允许删除，目录与符号链接默认拒绝。
/// 参数：socket_path 为固定专用目录下的目标路径；成功返回空值。
/// 异常/边界：拒绝跟随符号链接或递归删除，避免路径替换攻击扩大清理范围。
#[cfg(unix)]
fn remove_stale_socket(socket_path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("检查私有 RPC socket 失败：{}", error)),
    };
    if metadata.file_type().is_symlink() || metadata.is_dir() {
        return Err("私有 RPC socket 路径被非普通文件占用".to_string());
    }
    fs::remove_file(socket_path).map_err(|error| format!("清理陈旧私有 RPC socket 失败：{}", error))
}

/// 创建并验证私有 RPC 专用目录。
/// 流程：不存在时递归创建，随后以 symlink_metadata 精确确认最终节点是目录且不是符号链接，再由启动方法收紧为 0700。
/// 参数：socket_dir 为 App 数据目录下固定的 private-rpc 路径；成功返回空值。
/// 异常/边界：最终节点是符号链接或普通文件时默认拒绝，禁止跟随路径替换到 App 数据目录之外。
#[cfg(unix)]
fn ensure_private_socket_dir(socket_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(socket_dir)
        .map_err(|error| format!("创建私有 RPC 数据目录失败：{}", error))?;
    let metadata = fs::symlink_metadata(socket_dir)
        .map_err(|error| format!("检查私有 RPC 数据目录失败：{}", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("私有 RPC 数据目录不是可信目录".to_string());
    }
    Ok(())
}

/// 执行私有 RPC listener 和固定并发 worker 池。
/// 流程：listener 只负责非阻塞 accept，并把连接投递到八个固定 worker；每个 worker 独立恢复阻塞 stream、设置期限并处理一帧；队列满时立即返回 RPC_BUSY。
/// 参数：listener 为已设 0600 的 Unix Socket，stopping 为生命周期标志，secret 为当前代凭据，app 为业务上下文。
/// 异常/边界：单连接超时只占用一个 worker，不再队头阻塞全局；并发和待处理连接均有硬上限，退出时等待已接收请求完成且不记录正文。
#[cfg(unix)]
fn run_listener(
    listener: UnixListener,
    stopping: Arc<AtomicBool>,
    mut secret: String,
    app: AppHandle,
) {
    let (sender, receiver) = mpsc::sync_channel::<UnixStream>(MAX_PENDING_CONNECTIONS);
    let shared_receiver = Arc::new(Mutex::new(receiver));
    let mut workers = Vec::with_capacity(MAX_CONCURRENT_CONNECTIONS);
    for _ in 0..MAX_CONCURRENT_CONNECTIONS {
        let worker_receiver = Arc::clone(&shared_receiver);
        let worker_app = app.clone();
        let mut worker_secret = secret.clone();
        workers.push(thread::spawn(move || {
            loop {
                let received = match worker_receiver.lock() {
                    Ok(receiver) => receiver.recv(),
                    Err(_) => break,
                };
                let mut stream = match received {
                    Ok(stream) => stream,
                    Err(_) => break,
                };
                if configure_accepted_stream(&stream).is_err() {
                    continue;
                }
                let _ = handle_connection(&mut stream, &worker_secret, |method, params| {
                    dispatch_method(&worker_app, method, params)
                });
            }
            worker_secret.clear();
        }));
    }
    while !stopping.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                match sender.try_send(stream) {
                    Ok(()) => {}
                    Err(TrySendError::Full(mut stream)) => {
                        if configure_accepted_stream(&stream).is_ok() {
                            let _ = write_error_response(
                                &mut stream,
                                "RPC_BUSY",
                                "私有 RPC 当前请求过多，请稍后重试",
                            );
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(_) => thread::sleep(ACCEPT_POLL_INTERVAL),
        }
    }
    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }
    secret.clear();
}

/// 把非阻塞 listener 接受的 Unix stream 配置为可靠的有期限阻塞连接。
/// 流程：先显式关闭非阻塞模式，避免 macOS 继承 listener 标志导致大响应写到 socket buffer 后返回 WouldBlock，再设置固定读写超时。
/// 参数：stream 为刚由私有 listener 接受的连接；成功返回空值。
/// 异常/边界：任一步失败都返回固定脱敏错误，调用方必须关闭连接且不得进入业务分发；超时仍会阻止异常客户端永久占用 listener。
#[cfg(unix)]
fn configure_accepted_stream(stream: &UnixStream) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|_| "配置私有 RPC 连接阻塞模式失败".to_string())?;
    stream
        .set_read_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|_| "配置私有 RPC 连接读取超时失败".to_string())?;
    stream
        .set_write_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|_| "配置私有 RPC 连接写入超时失败".to_string())
}

/// 读取一条长度前缀请求并写回一条长度前缀响应。
/// 流程：读取 4 字节 big-endian 长度，执行请求上限校验和完整读取，再处理鉴权/分发并限制序列化响应为 8 MiB。
/// 参数：stream 为单连接读写流，secret 为当前代凭据，dispatch 为 allowlist 业务分发器；成功返回空值。
/// 异常/边界：零长度、超过 1 MiB、截断帧和响应超限均返回稳定错误；每连接只处理一个请求。
fn handle_connection<S, F>(stream: &mut S, secret: &str, dispatch: F) -> Result<(), String>
where
    S: Read + Write,
    F: FnOnce(&str, Value) -> Result<Value, RpcErrorBody>,
{
    let mut length_bytes = [0_u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .map_err(|_| "读取私有 RPC 帧长度失败".to_string())?;
    let request_length = u32::from_be_bytes(length_bytes) as usize;
    if request_length == 0 || request_length > MAX_REQUEST_BYTES {
        return write_error_response(
            stream,
            "RPC_REQUEST_TOO_LARGE",
            "私有 RPC 请求大小不符合限制",
        );
    }
    let mut request_bytes = vec![0_u8; request_length];
    stream
        .read_exact(&mut request_bytes)
        .map_err(|_| "读取私有 RPC 请求正文失败".to_string())?;
    let response = process_request(&request_bytes, secret, dispatch);
    request_bytes.fill(0);
    let response_bytes =
        serde_json::to_vec(&response).map_err(|_| "序列化私有 RPC 响应失败".to_string())?;
    if response_bytes.len() > MAX_RESPONSE_BYTES {
        return write_error_response(
            stream,
            "RPC_RESPONSE_TOO_LARGE",
            "私有 RPC 响应超过 8 MiB 限制",
        );
    }
    write_frame(stream, &response_bytes)
}

/// 校验请求 envelope 并调用 allowlist 分发器。
/// 流程：严格反序列化字段、常量时间比较 secret，再把 method/params 交给分发器并包装统一响应。
/// 参数：request_bytes 为不超过 1 MiB 的完整 JSON，expected_secret 为当前代凭据，dispatch 为业务分发器；返回可序列化响应。
/// 异常/边界：非法 JSON、未知字段、鉴权失败或业务失败均返回 ok=false，不回显凭据和原始正文。
fn process_request<F>(request_bytes: &[u8], expected_secret: &str, dispatch: F) -> Value
where
    F: FnOnce(&str, Value) -> Result<Value, RpcErrorBody>,
{
    let mut request = match serde_json::from_slice::<RpcRequest>(request_bytes) {
        Ok(request) => request,
        Err(_) => return error_value("RPC_INVALID_REQUEST", "私有 RPC 请求格式无效"),
    };
    if !constant_time_equal(request.secret.as_bytes(), expected_secret.as_bytes()) {
        request.secret.clear();
        return error_value("RPC_UNAUTHORIZED", "私有 RPC 鉴权失败");
    }
    request.secret.clear();
    if !is_valid_request_id(&request.request_id) {
        return error_value("RPC_INVALID_REQUEST", "私有 RPC requestId 格式无效");
    }
    if !ALLOWED_METHODS.contains(&request.method.as_str()) {
        return error_value("RPC_METHOD_NOT_ALLOWED", "私有 RPC 方法未开放");
    }
    match dispatch(&request.method, request.params) {
        Ok(result) => serde_json::to_value(RpcSuccessResponse { ok: true, result })
            .unwrap_or_else(|_| error_value("RPC_SERIALIZATION_FAILED", "私有 RPC 响应序列化失败")),
        Err(error) => serde_json::to_value(RpcErrorResponse { ok: false, error })
            .unwrap_or_else(|_| error_value("RPC_SERIALIZATION_FAILED", "私有 RPC 响应序列化失败")),
    }
}

/// 校验私有 RPC 请求追踪 ID 的安全字符集和长度。
/// 流程：先限制为 1 至 128 个 ASCII 字节，再逐字节允许字母、数字、点、下划线和短横线；参数为 requestId。
/// 返回：完全匹配 `[A-Za-z0-9._-]{1,128}` 时为 true；异常/边界：空值、Unicode、空白、路径分隔符和超长值均拒绝。
fn is_valid_request_id(request_id: &str) -> bool {
    !request_id.is_empty()
        && request_id.len() <= 128
        && request_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
}

/// 常量时间比较两段凭据字节。
/// 流程：始终遍历两者最大长度，并把长度差与逐字节差累计后统一判断；参数为实际和期望 secret 字节。
/// 返回：完全相同为 true；异常/边界：空值不会被特殊放行，长度不同也不会提前返回。
fn constant_time_equal(actual: &[u8], expected: &[u8]) -> bool {
    let mut difference = actual.len() ^ expected.len();
    for index in 0..actual.len().max(expected.len()) {
        let actual_byte = actual.get(index).copied().unwrap_or(0);
        let expected_byte = expected.get(index).copied().unwrap_or(0);
        difference |= usize::from(actual_byte ^ expected_byte);
    }
    difference == 0
}

/// 把 allowlist 方法分发到现有 Rust 业务核心。
/// 流程：逐方法严格反序列化 params，调用同一 TaskStore/Codex 实现并把结果转成 JSON；所有方法均在固定 match 中登记。
/// 参数：app 为唯一业务上下文，method 为协议方法名，params 为请求对象；返回业务 JSON 或稳定错误。
/// 异常/边界：未知方法默认拒绝；参数错误与业务错误使用不同错误码，不记录 prompt、结果或私有凭据。
fn dispatch_method(app: &AppHandle, method: &str, params: Value) -> Result<Value, RpcErrorBody> {
    match method {
        "requestAccessTokenApproval" => {
            let request = decode_params::<AccessTokenApprovalParams>(params)?;
            serialize_business_result(
                request_access_token_approval_core(
                    app,
                    request.request_id,
                    request.name,
                    request.expires_at,
                ),
                "ACCESS_TOKEN_APPROVAL_FAILED",
                "App 授权确认失败。",
            )
        }
        "loadWorkspaceData" => {
            let request = decode_params::<ProjectIdParams>(params)?;
            serialize_business_result(
                load_session_workspace_data_core(app, request.project_id),
                "TASK_WORKSPACE_LOAD_FAILED",
                "读取任务工作区失败。",
            )
        }
        "createProject" => {
            let request = decode_params::<CreateProjectParams>(params)?;
            serialize_business_result(
                create_session_project_core(
                    app,
                    CreateProjectRequest {
                        name: request.name,
                        workspace_path: request.workspace_path,
                        base_prompt: request.base_prompt,
                    },
                ),
                "TASK_PROJECT_CREATE_FAILED",
                "创建任务项目失败。",
            )
        }
        "updateProject" => {
            let request = decode_params::<UpdateProjectParams>(params)?;
            serialize_business_result(
                update_session_project_core(
                    app,
                    UpdateProjectRequest {
                        id: request.id,
                        name: request.name,
                        workspace_path: request.workspace_path,
                        base_prompt: request.base_prompt,
                    },
                ),
                "TASK_PROJECT_UPDATE_FAILED",
                "更新任务项目失败。",
            )
        }
        "deleteProject" => {
            let request = decode_params::<ProjectIdParams>(params)?;
            let project_id = request.project_id.ok_or_else(|| RpcErrorBody {
                code: "RPC_INVALID_PARAMS".to_string(),
                message: "projectId 不能为空".to_string(),
            })?;
            serialize_business_result(
                delete_session_project_core(app, project_id),
                "TASK_PROJECT_DELETE_FAILED",
                "删除任务项目失败。",
            )
        }
        "createTask" => {
            let request = decode_params::<CreateTaskParams>(params)?;
            serialize_business_result(
                create_session_task_core(
                    app,
                    CreateTaskRequest {
                        project_id: request.project_id,
                        title: request.title,
                        prompt: request.prompt,
                        attachments: request.attachments,
                    },
                ),
                "TASK_CREATE_FAILED",
                "创建任务失败。",
            )
        }
        "updateTask" => {
            let request = decode_params::<UpdateTaskParams>(params)?;
            serialize_business_result(
                update_session_task_core(
                    app,
                    UpdateTaskRequest {
                        id: request.id,
                        title: request.title,
                        prompt: request.prompt,
                        attachments: request.attachments,
                    },
                ),
                "TASK_UPDATE_FAILED",
                "更新任务失败。",
            )
        }
        "deleteTask" => {
            let request = decode_params::<TaskIdParams>(params)?;
            serialize_business_result(
                delete_session_task_core(app, request.task_id),
                "TASK_DELETE_FAILED",
                "删除任务失败。",
            )
        }
        "queueTask" => {
            let request = decode_params::<TaskIdParams>(params)?;
            serialize_business_result(
                queue_session_task_core(app, request.task_id),
                "TASK_QUEUE_FAILED",
                "任务入队失败。",
            )
        }
        "completeTask" => {
            let request = decode_params::<TaskIdParams>(params)?;
            serialize_business_result(
                complete_session_task_core(app, request.task_id),
                "TASK_ACCEPTANCE_FAILED",
                "更新任务验收状态失败。",
            )
        }
        "listCodexWorkspaces" => {
            ensure_empty_params(params)?;
            serialize_business_result(
                list_codex_workspaces_core(),
                "CODEX_UNAVAILABLE",
                "Codex 会话服务暂不可用。",
            )
        }
        "listCodexThreads" => {
            let request = decode_params::<ListCodexThreadsParams>(params)?;
            serialize_business_result(
                list_codex_threads_core(CodexThreadListRequest {
                    workspace_cwd: request.workspace_cwd,
                    limit: request.limit,
                    offset: request.offset,
                    keyword: request.keyword,
                }),
                "CODEX_UNAVAILABLE",
                "Codex 会话服务暂不可用。",
            )
        }
        "openCodexThread" => {
            let request = decode_params::<ThreadIdParams>(params)?;
            serialize_business_result(
                open_session_external_thread_core(request.thread_id),
                "CODEX_UNAVAILABLE",
                "Codex 会话服务暂不可用。",
            )
        }
        "readCodexThreadMessages" => {
            let request = decode_params::<ThreadIdParams>(params)?;
            serialize_business_result(
                read_codex_thread_messages_core(
                    request.thread_id,
                    request.before_message_order,
                    request.limit,
                ),
                "CODEX_UNAVAILABLE",
                "Codex 会话详情暂不可用。",
            )
        }
        "getCodexConnection" => {
            ensure_empty_params(params)?;
            serialize_business_result(
                get_codex_connection_core(app),
                "CODEX_CONNECTION_CHECK_FAILED",
                "读取 Codex Desktop 连接状态失败。",
            )
        }
        "restartCodex" => {
            ensure_empty_params(params)?;
            serialize_business_result(
                restart_codex_core(app),
                "CODEX_RESTART_FAILED",
                "Codex Desktop 重启请求失败。",
            )
        }
        "listMyApps" => {
            ensure_empty_params(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().list(app),
                "MY_APP_LIST_FAILED",
                "读取我的应用列表失败。",
            )
        }
        "allocateMyAppPort" => {
            ensure_empty_params(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().allocate_port(app),
                "MY_APP_PORT_ALLOCATE_FAILED",
                "自动分配应用端口失败。",
            )
        }
        "createMyApp" => {
            let request = decode_params::<CreateMyAppParams>(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().create(app, request),
                "MY_APP_CREATE_FAILED",
                "创建我的应用失败。",
            )
        }
        "updateMyApp" => {
            let request = decode_params::<UpdateMyAppParams>(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().update(app, request),
                "MY_APP_UPDATE_FAILED",
                "更新我的应用失败。",
            )
        }
        "deleteMyApp" => {
            let request = decode_params::<MyAppIdParams>(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().delete(app, &request.app_id),
                "MY_APP_DELETE_FAILED",
                "删除我的应用失败。",
            )
        }
        "restartMyApp" => {
            let request = decode_params::<MyAppIdParams>(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().restart(app, &request.app_id),
                "MY_APP_RESTART_FAILED",
                "启动或重启我的应用失败。",
            )
        }
        "openMyApp" => {
            let request = decode_params::<OpenMyAppParams>(params)?;
            serialize_business_result(
                app.state::<RuntimeMyApps>().open(app, request),
                "MY_APP_OPEN_FAILED",
                "打开我的应用失败。",
            )
        }
        _ => Err(RpcErrorBody {
            code: "RPC_METHOD_NOT_ALLOWED".to_string(),
            message: "私有 RPC 方法未开放".to_string(),
        }),
    }
}

/// 严格反序列化单个方法的 params。
/// 流程：使用目标 DTO 的 serde 规则解析对象；参数为 JSON 值；返回目标 DTO。
/// 异常/边界：缺字段、类型错误和未知字段统一返回稳定参数错误，不回显原始正文。
fn decode_params<T>(params: Value) -> Result<T, RpcErrorBody>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(params).map_err(|_| RpcErrorBody {
        code: "RPC_INVALID_PARAMS".to_string(),
        message: "私有 RPC 方法参数无效".to_string(),
    })
}

/// 校验无参数方法只接收空对象。
/// 流程：精确判断 JSON 对象且字段数为零；参数为 params；成功返回空值。
/// 异常/边界：null、数组或含未知字段的对象均拒绝，保证协议没有隐式兼容分支。
fn ensure_empty_params(params: Value) -> Result<(), RpcErrorBody> {
    if params.as_object().is_some_and(serde_json::Map::is_empty) {
        Ok(())
    } else {
        Err(RpcErrorBody {
            code: "RPC_INVALID_PARAMS".to_string(),
            message: "私有 RPC 方法参数无效".to_string(),
        })
    }
}

/// 序列化 Rust 业务结果并统一映射业务失败。
/// 流程：优先提取既有安全文案中的稳定错误码；我的应用错误保留用户可修复原因；其它错误使用固定兜底码和文案，成功值再转为 JSON。
/// 参数：result 为任意可序列化业务响应，fallback_code/message 为当前方法的稳定失败分类；返回 JSON。
/// 异常/边界：仅带合法错误码的既有安全文案可透传；我的应用错误裁剪长度后透传；其它无错误码的数据库、路径或进程正文由固定文案替代。
fn serialize_business_result<T>(
    result: Result<T, String>,
    fallback_code: &str,
    fallback_message: &str,
) -> Result<Value, RpcErrorBody>
where
    T: Serialize,
{
    let value = result.map_err(|message| {
        if let Some(code) = extract_embedded_error_code(&message) {
            return RpcErrorBody { code, message };
        }
        if fallback_code == "CODEX_UNAVAILABLE"
            && (message.contains("不能为空") || message.contains("不支持的字符"))
        {
            return RpcErrorBody {
                code: "INVALID_THREAD_ID".to_string(),
                message: "会话 ID 无效。".to_string(),
            };
        }
        if fallback_code.starts_with("MY_APP_") {
            return RpcErrorBody {
                code: fallback_code.to_string(),
                message: build_safe_user_message(&message, fallback_message),
            };
        }
        RpcErrorBody {
            code: fallback_code.to_string(),
            message: fallback_message.to_string(),
        }
    })?;
    serde_json::to_value(value).map_err(|_| RpcErrorBody {
        code: "RPC_SERIALIZATION_FAILED".to_string(),
        message: "私有 RPC 响应序列化失败".to_string(),
    })
}

/// 构建可以展示给用户的业务错误文案。
/// 流程：去除首尾空白，空值使用兜底文案，非空文案按字符数裁剪避免响应过长。
/// 参数：message 为业务层返回的可读失败原因，fallback_message 为固定兜底文案。
/// 返回：适合 HTTP API 和前端 toast 展示的短文案。
/// 异常/边界：裁剪按 char 边界执行，避免破坏中文 UTF-8。
fn build_safe_user_message(message: &str, fallback_message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return fallback_message.to_string();
    }
    trimmed.chars().take(500).collect()
}

/// 从既有安全业务文案中提取稳定 UPPER_SNAKE 错误码。
/// 流程：定位首个 `错误码：` 标记，读取其后连续 ASCII 大写字母、数字或下划线，并执行 1 至 128 字节限制。
/// 参数：message 为现有 TaskStore 或统一桌面错误入口返回的安全文案；返回合法错误码或 None。
/// 异常/边界：不接受小写、空值、超长或其它符号；解析失败由调用方使用明确 operation 兜底码，绝不回显内部数据库正文。
fn extract_embedded_error_code(message: &str) -> Option<String> {
    let marker_index = message.find("错误码：")? + "错误码：".len();
    let code = message[marker_index..]
        .bytes()
        .take_while(|value| value.is_ascii_uppercase() || value.is_ascii_digit() || *value == b'_')
        .take(129)
        .collect::<Vec<_>>();
    if code.is_empty() || code.len() > 128 {
        return None;
    }
    String::from_utf8(code).ok()
}

/// 构造统一错误 JSON 值。
/// 流程：把稳定 code/message 包装为 ok=false envelope；参数为安全字符串；返回 JSON 值。
/// 异常/边界：固定结构理论上不可序列化失败，极端失败时回退最小静态 JSON。
fn error_value(code: &str, message: &str) -> Value {
    serde_json::to_value(RpcErrorResponse {
        ok: false,
        error: RpcErrorBody {
            code: code.to_string(),
            message: message.to_string(),
        },
    })
    .unwrap_or_else(|_| json!({"ok": false, "error": {"code": "RPC_INTERNAL_ERROR", "message": "私有 RPC 内部错误"}}))
}

/// 向连接写入一个有界 big-endian 长度前缀 JSON 帧。
/// 流程：校验响应上限，把长度编码为 u32 后依次 write_all 并 flush；参数为流和完整 JSON 字节。
/// 异常/边界：超过 8 MiB 或写入失败显式返回，禁止部分成功。
fn write_frame<S>(stream: &mut S, bytes: &[u8]) -> Result<(), String>
where
    S: Write,
{
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("私有 RPC 响应超过 8 MiB 限制".to_string());
    }
    let length =
        u32::try_from(bytes.len()).map_err(|_| "私有 RPC 响应长度超出协议范围".to_string())?;
    stream
        .write_all(&length.to_be_bytes())
        .and_then(|_| stream.write_all(bytes))
        .and_then(|_| stream.flush())
        .map_err(|_| "写入私有 RPC 响应失败".to_string())
}

/// 写入无需业务分发的稳定错误响应。
/// 流程：构造 ok=false JSON 后复用有界帧写入；参数为流、错误码和安全文案；返回写入结果。
/// 异常/边界：不接受原始请求正文作为 message，避免错误链泄露敏感内容。
fn write_error_response<S>(stream: &mut S, code: &str, message: &str) -> Result<(), String>
where
    S: Write,
{
    let bytes = serde_json::to_vec(&error_value(code, message))
        .map_err(|_| "序列化私有 RPC 错误响应失败".to_string())?;
    write_frame(stream, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 测试用双工流，把请求读取游标与响应缓冲分离，模拟 socket 的全双工行为。
    struct TestDuplex {
        /// 生产协议读取的请求字节。
        read: Cursor<Vec<u8>>,
        /// 生产协议写出的响应字节。
        written: Vec<u8>,
    }

    impl Read for TestDuplex {
        /// 从独立请求游标读取字节。
        /// 流程：把调用直接委托给 Cursor；参数为目标缓冲；返回读取数量。
        /// 异常/边界：遵循 Cursor 的 EOF 语义，不把响应数据暴露给读取端。
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.read.read(buffer)
        }
    }

    impl Write for TestDuplex {
        /// 把响应字节追加到独立输出缓冲。
        /// 流程：完整追加本次切片；参数为响应字节；返回写入数量。
        /// 异常/边界：内存测试流不会短写。
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        /// 刷新测试输出缓冲。
        /// 流程：内存缓冲无需实际刷新；参数无；返回成功。
        /// 异常/边界：不会产生 I/O 错误。
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 从测试输出流读取一条响应帧并解析 JSON。
    /// 流程：读取 4 字节长度后精确读取正文并反序列化；参数为完整输出字节；返回响应 JSON。
    /// 异常/边界：测试辅助方法只接收由生产 write_frame 生成的完整帧，损坏帧直接 panic 暴露失败。
    fn decode_response_frame(bytes: Vec<u8>) -> Value {
        let mut cursor = Cursor::new(bytes);
        let mut length = [0_u8; 4];
        cursor.read_exact(&mut length).expect("响应必须包含长度");
        let mut body = vec![0_u8; u32::from_be_bytes(length) as usize];
        cursor.read_exact(&mut body).expect("响应正文必须完整");
        serde_json::from_slice(&body).expect("响应必须是 JSON")
    }

    /// 构造一条生产协议请求帧。
    /// 流程：序列化 JSON 并添加 4 字节长度；参数为请求 JSON；返回完整输入字节。
    /// 异常/边界：仅用于小型测试数据，超过 u32 时测试直接失败。
    fn request_frame(value: Value) -> Vec<u8> {
        let body = serde_json::to_vec(&value).expect("请求必须可序列化");
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend(body);
        frame
    }

    #[test]
    fn framing_dispatches_one_authenticated_request() {
        let input = request_frame(json!({
            "secret": "correct-secret",
            "method": "listCodexWorkspaces",
            "requestId": "request-123",
            "params": {"value": 7}
        }));
        let mut stream = TestDuplex {
            read: Cursor::new(input),
            written: Vec::new(),
        };
        let calls = AtomicUsize::new(0);
        handle_connection(&mut stream, "correct-secret", |method, params| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(method, "listCodexWorkspaces");
            Ok(params)
        })
        .expect("完整鉴权请求应可分发并响应");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(decode_response_frame(stream.written)["result"]["value"], 7);
    }

    #[cfg(unix)]
    #[test]
    fn real_unix_socket_delivers_complete_response_larger_than_eight_kibibytes() {
        let random_suffix = uuid::Uuid::new_v4().simple().to_string();
        let socket_path =
            PathBuf::from("/tmp").join(format!("aitool-rpc-{}.sock", &random_suffix[..12]));
        let listener = UnixListener::bind(&socket_path).expect("应能绑定真实测试 socket");
        listener
            .set_nonblocking(true)
            .expect("测试 listener 应进入生产非阻塞模式");
        let expected_result = "x".repeat(64 * 1024);
        let server_result = expected_result.clone();
        let server = thread::spawn(move || -> Result<(), String> {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL_INTERVAL);
                    }
                    Err(_) => return Err("真实测试 socket accept 失败".to_string()),
                }
            };
            configure_accepted_stream(&stream)?;
            handle_connection(&mut stream, "correct-secret", |method, params| {
                if method != "listCodexWorkspaces" || params != json!({}) {
                    return Err(RpcErrorBody {
                        code: "TEST_DISPATCH_INVALID".to_string(),
                        message: "测试分发参数无效".to_string(),
                    });
                }
                Ok(Value::String(server_result))
            })
        });

        let mut client = UnixStream::connect(&socket_path).expect("客户端应连接真实测试 socket");
        client
            .set_read_timeout(Some(CONNECTION_TIMEOUT))
            .expect("测试客户端应设置读取超时");
        client
            .set_write_timeout(Some(CONNECTION_TIMEOUT))
            .expect("测试客户端应设置写入超时");
        let request = request_frame(json!({
            "secret": "correct-secret",
            "method": "listCodexWorkspaces",
            "requestId": "large-response-regression",
            "params": {}
        }));
        client.write_all(&request).expect("客户端应完整写入请求帧");
        let mut length_bytes = [0_u8; 4];
        client
            .read_exact(&mut length_bytes)
            .expect("大响应必须包含完整长度头");
        let declared_length = u32::from_be_bytes(length_bytes) as usize;
        assert!(declared_length > 8 * 1024);
        let mut response_bytes = vec![0_u8; declared_length];
        client
            .read_exact(&mut response_bytes)
            .expect("大响应正文必须达到声明长度，不能在 8192 字节处截断");
        let response: Value =
            serde_json::from_slice(&response_bytes).expect("大响应必须是完整 JSON");
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"], expected_result);
        server
            .join()
            .expect("真实 socket 服务线程不得 panic")
            .expect("真实 socket 服务端必须完整写入大响应");
        fs::remove_file(&socket_path).expect("测试结束应删除真实 socket");
    }

    #[test]
    fn authentication_failure_never_dispatches() {
        let calls = AtomicUsize::new(0);
        let response = process_request(
            br#"{"secret":"wrong","method":"listCodexWorkspaces","requestId":"request-123","params":{}}"#,
            "correct-secret",
            |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Value::Null)
            },
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(response["error"]["code"], "RPC_UNAUTHORIZED");
    }

    #[test]
    fn unknown_method_is_rejected_by_allowlist() {
        let calls = AtomicUsize::new(0);
        let response = process_request(
            br#"{"secret":"secret","method":"notAllowed","requestId":"request-123","params":{}}"#,
            "secret",
            |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Value::Null)
            },
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "RPC_METHOD_NOT_ALLOWED");
    }

    #[test]
    fn oversized_request_is_rejected_before_allocation() {
        let mut input = ((MAX_REQUEST_BYTES + 1) as u32).to_be_bytes().to_vec();
        input.extend_from_slice(b"ignored");
        let mut stream = TestDuplex {
            read: Cursor::new(input),
            written: Vec::new(),
        };
        handle_connection(&mut stream, "secret", |_, _| Ok(Value::Null))
            .expect("越界请求应写回错误帧");
        let response = decode_response_frame(stream.written);
        assert_eq!(response["error"]["code"], "RPC_REQUEST_TOO_LARGE");
    }

    #[test]
    fn oversized_response_is_rejected_before_writing() {
        let mut output = Cursor::new(Vec::new());
        let response = vec![0_u8; MAX_RESPONSE_BYTES + 1];
        let error = write_frame(&mut output, &response).expect_err("越界响应必须拒绝");
        assert_eq!(error, "私有 RPC 响应超过 8 MiB 限制");
        assert!(output.into_inner().is_empty());
    }

    /// 并发队列过载响应必须保持可解析的稳定错误码，让 Python 可明确返回可重试失败而不是截断协议。
    #[test]
    fn busy_response_uses_stable_error_envelope() {
        let mut output = Cursor::new(Vec::new());
        write_error_response(&mut output, "RPC_BUSY", "私有 RPC 当前请求过多，请稍后重试")
            .expect("过载错误应可完整写入");
        let response = decode_response_frame(output.into_inner());
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "RPC_BUSY");
    }

    #[test]
    fn strict_method_params_reject_unknown_fields() {
        let error = decode_params::<CreateProjectParams>(json!({
            "name": "项目",
            "workspacePath": "/tmp",
            "legacyField": true
        }))
        .err()
        .expect("首发协议不得兼容未知历史字段");
        assert_eq!(error.code, "RPC_INVALID_PARAMS");
    }

    #[test]
    fn missing_request_id_is_rejected_as_invalid_request() {
        let response = process_request(
            br#"{"secret":"secret","method":"listCodexWorkspaces","params":{}}"#,
            "secret",
            |_, _| Ok(Value::Null),
        );
        assert_eq!(response["error"]["code"], "RPC_INVALID_REQUEST");
    }

    #[test]
    fn unsafe_request_id_is_rejected_before_dispatch() {
        let calls = AtomicUsize::new(0);
        let response = process_request(
            br#"{"secret":"secret","method":"listCodexWorkspaces","requestId":"bad/request id","params":{}}"#,
            "secret",
            |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Value::Null)
            },
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(response["error"]["code"], "RPC_INVALID_REQUEST");
    }

    #[test]
    fn business_error_mapping_preserves_stable_task_codes() {
        for expected_code in [
            "TASK_NOT_FOUND",
            "TASK_TITLE_REQUIRED",
            "TASK_UPDATE_STATUS_FORBIDDEN",
            "TASK_DELETE_STATUS_FORBIDDEN",
            "TASK_PROJECT_CREATE_FAILED",
            "CODEX_DESKTOP_NOT_CONNECTED",
            "CODEX_SEND_UNCERTAIN",
            "CODEX_RESTART_TASK_ACTIVE",
        ] {
            let message = format!("安全说明（错误码：{}，诊断 ID：test）", expected_code);
            let error = serialize_business_result::<Value>(
                Err(message.clone()),
                "TASK_CREATE_FAILED",
                "创建任务失败。",
            )
            .expect_err("业务失败必须返回结构化错误");
            assert_eq!(error.code, expected_code);
            assert_eq!(error.message, message);
        }
    }

    /// 连接状态和显式重启必须属于固定 allowlist，且不能出现通用命令入口。
    #[test]
    fn codex_connection_methods_are_explicitly_allowlisted() {
        assert_eq!(ALLOWED_METHODS.len(), 22);
        assert!(ALLOWED_METHODS.contains(&"getCodexConnection"));
        assert!(ALLOWED_METHODS.contains(&"restartCodex"));
        assert!(!ALLOWED_METHODS.contains(&"command"));
    }

    #[test]
    fn my_app_error_mapping_preserves_user_fixable_detail() {
        let error = serialize_business_result::<Value>(
            Err("zip 包根目录或第一层目录必须包含 index.html。".to_string()),
            "MY_APP_CREATE_FAILED",
            "创建我的应用失败。",
        )
        .expect_err("我的应用失败必须返回结构化错误");
        assert_eq!(error.code, "MY_APP_CREATE_FAILED");
        assert_eq!(
            error.message,
            "zip 包根目录或第一层目录必须包含 index.html。"
        );
    }

    #[test]
    fn business_error_mapping_uses_operation_fallback_without_internal_detail() {
        let error = serialize_business_result::<Value>(
            Err("打开数据库失败：/private/path.sqlite3".to_string()),
            "TASK_WORKSPACE_LOAD_FAILED",
            "读取任务工作区失败。",
        )
        .expect_err("业务失败必须返回结构化错误");
        assert_eq!(error.code, "TASK_WORKSPACE_LOAD_FAILED");
        assert_eq!(error.message, "读取任务工作区失败。");

        let codex_error = serialize_business_result::<Value>(
            Err("Codex app-server stderr private payload".to_string()),
            "CODEX_UNAVAILABLE",
            "Codex 会话服务暂不可用。",
        )
        .expect_err("Codex 失败必须返回结构化错误");
        assert_eq!(codex_error.code, "CODEX_UNAVAILABLE");
        assert_eq!(codex_error.message, "Codex 会话服务暂不可用。");
    }

    #[test]
    fn constant_time_comparison_handles_length_and_content_differences() {
        assert!(constant_time_equal(b"same", b"same"));
        assert!(!constant_time_equal(b"same", b"different"));
        assert!(!constant_time_equal(b"same", b"samf"));
    }
}
