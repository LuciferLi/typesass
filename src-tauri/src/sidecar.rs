use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::private_rpc::{PrivateRpcBootstrap, RuntimePrivateRpc};

/// FastAPI sidecar 固定监听端口；产品契约禁止自动漂移端口。
pub const SIDECAR_PORT: u16 = 18_080;
/// App 内置 sidecar 的稳定基础地址。
const SIDECAR_BASE_URL: &str = "http://127.0.0.1:18080";
/// 本机开发 curl 联调使用的固定 Bearer Token；仅 debug 构建注入 sidecar。
#[cfg(debug_assertions)]
pub const DEV_BEARER_TOKEN: &str = "codexman-dev-bearer-token-000000000001";
/// Tauri externalBin 逻辑名称；打包后目标三元组后缀会被 Tauri 移除。
const SIDECAR_BINARY_NAME: &str = "codexman-ai-sidecar";
/// sidecar 完成 onefile 解包、配置加载和健康检查的最大等待时间。
const HEALTH_TIMEOUT: Duration = Duration::from_secs(45);
/// stdin bootstrap 的完整 JSON+LF 帧上限；总长度不得超过 1 MiB。
const MAX_BOOTSTRAP_FRAME_BYTES: usize = 1024 * 1024;
/// sidecar 启动兜底日志单文件上限；活动文件和一个备份共同把总占用限制在 1 MiB。
const PROCESS_LOG_MAX_BYTES: u64 = 512 * 1024;
/// 每次从 stdout/stderr 管道读取的最大块，保证单次追加不会突破单文件上限。
const PROCESS_LOG_CHUNK_BYTES: usize = 4 * 1024;
/// 两条 sidecar 输出管道共享的写锁，避免 stdout/stderr 同时触发轮转造成丢失或覆盖。
static PROCESS_LOG_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// App 生命周期持有的 FastAPI sidecar 子进程与临时凭据。
#[derive(Default)]
struct SidecarProcessState {
    /// 当前受 App 管理的 sidecar 直接子进程及独立进程组；退出或重启时必须整组清理。
    process: Option<ManagedSidecarProcess>,
    /// 旧 Basic client ID 兼容字段；授权码改造后不再生成或使用。
    client_id: String,
    /// 旧 Basic secret 兼容字段；授权码改造后不再生成或使用。
    client_secret: String,
    /// 旧设备码批准限流兼容字段；授权码改造后不再生成 pending 设备码。
    approval_attempts: Vec<Instant>,
}

/// App 持有的 sidecar 直接子进程及其已验证独立进程组。
struct ManagedSidecarProcess {
    /// PyInstaller 外层直接子进程，用于读取退出状态和回收僵尸。
    child: Child,
    /// 启动后验证等于直接子进程 PID 的 PGID，用于清理 PyInstaller 内层 Uvicorn 后代。
    process_group_id: i32,
    /// 持续排空 stdout/stderr 并写入有界兜底日志的线程；进程确认退出后回收，保证末尾诊断已落盘。
    process_log_threads: Vec<thread::JoinHandle<()>>,
}

/// App 生命周期持有的 FastAPI sidecar 运行时状态。
#[derive(Default)]
pub struct RuntimeSidecar {
    /// 子进程与临时凭据共享同一把锁，避免重启、退出和续签读取到跨代状态。
    state: Mutex<SidecarProcessState>,
}

/// 构造严格单 envelope 的 sidecar stdin 启动帧。
/// 流程：解析调用方模型目录 JSON，与当前代私有 RPC 配置共同包装，以紧凑 JSON 序列化，追加单个 LF 后校验完整帧不超过 1 MiB。
/// 参数：model_catalog_json 为已从安全凭证合成、仅存在于 Rust 内存的模型目录，private_rpc 为仅传给 sidecar 的 socket 与凭据；返回可一次性写入子进程 stdin 的完整帧。
/// 异常/边界：非法 JSON 或完整 JSON+LF 帧超过 1 MiB 时在启动子进程前失败。
fn build_model_catalog_bootstrap(
    model_catalog_json: &str,
    private_rpc: &PrivateRpcBootstrap,
) -> Result<Vec<u8>, String> {
    let model_catalog = serde_json::from_str::<serde_json::Value>(model_catalog_json)
        .map_err(|error| format!("解析 sidecar 模型注册表失败：{}", error))?;
    let mut bootstrap = serde_json::to_vec(&json!({
        "modelCatalog": model_catalog,
        "privateRpc": private_rpc,
    }))
    .map_err(|error| format!("序列化 sidecar stdin bootstrap 失败：{}", error))?;
    bootstrap.push(b'\n');
    if bootstrap.len() > MAX_BOOTSTRAP_FRAME_BYTES {
        bootstrap.fill(0);
        return Err("sidecar stdin bootstrap 超过 1 MiB 限制".to_string());
    }
    Ok(bootstrap)
}

/// 写入完整 bootstrap 帧并关闭 sidecar stdin 写端。
/// 流程：按顺序写完单个 JSON+LF 帧并 flush，无论成功或失败都显式 drop writer，使 Python 立即收到 EOF 或写入失败。
/// 参数：writer 以所有权传入，frame 为已通过 1 MiB 校验的完整启动帧；成功返回空值。
/// 异常/边界：任何写入或 flush 错误只返回稳定操作说明；writer 在返回前关闭，禁止 Python 因等待 EOF 永久阻塞。
fn write_and_close_bootstrap<W: Write>(mut writer: W, frame: &[u8]) -> Result<(), String> {
    let write_result = writer.write_all(frame).and_then(|_| writer.flush());
    drop(writer);
    write_result.map_err(|error| format!("写入 sidecar 一次性模型 bootstrap 失败：{}", error))
}

/// 为本次 App 生命周期初始化 sidecar 启动兜底日志。
/// 流程：取得 stdout/stderr 共享写锁，删除上次单备份，再创建并截断活动日志，确保新启动不会继承历史累计大小。
/// 参数：process_log_path 为 App 数据目录下固定进程日志路径；成功返回空值。
/// 异常/边界：目录已由调用方创建；删除备份或截断活动文件失败时阻止 sidecar 启动，避免无界或不可诊断运行。
fn prepare_process_log(process_log_path: &std::path::Path) -> Result<(), String> {
    let _write_guard = PROCESS_LOG_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let backup_path = process_log_path.with_extension("log.1");
    if backup_path.exists() {
        fs::remove_file(&backup_path)
            .map_err(|error| format!("清理 sidecar 启动兜底日志备份失败：{}", error))?;
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(process_log_path)
        .map_err(|error| format!("初始化 sidecar 启动兜底日志失败：{}", error))?;
    Ok(())
}

/// 向严格有界的 sidecar 启动兜底日志追加一个输出块。
/// 流程：在共享锁内读取活动文件大小；本次追加会越过 512 KiB 时把活动文件轮转为唯一 `.1` 备份，再追加并 flush。
/// 参数：process_log_path 为活动日志路径，chunk 为最多 4 KiB 的 stdout/stderr 原始输出；成功返回空值。
/// 异常/边界：调用方不得传入 bootstrap 或密钥；空块直接成功，超出固定块上限拒绝写入，活动文件与备份均不会超过单文件上限。
fn append_process_log_chunk(
    process_log_path: &std::path::Path,
    chunk: &[u8],
) -> Result<(), String> {
    if chunk.is_empty() {
        return Ok(());
    }
    if chunk.len() > PROCESS_LOG_CHUNK_BYTES {
        return Err("sidecar 启动兜底日志单次写入超过 4 KiB".to_string());
    }
    let _write_guard = PROCESS_LOG_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current_size = fs::metadata(process_log_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if current_size.saturating_add(chunk.len() as u64) > PROCESS_LOG_MAX_BYTES {
        let backup_path = process_log_path.with_extension("log.1");
        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|error| format!("清理 sidecar 启动兜底日志备份失败：{}", error))?;
        }
        if process_log_path.exists() {
            fs::rename(process_log_path, &backup_path)
                .map_err(|error| format!("轮转 sidecar 启动兜底日志失败：{}", error))?;
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(process_log_path)
        .map_err(|error| format!("打开 sidecar 启动兜底日志失败：{}", error))?;
    file.write_all(chunk)
        .and_then(|_| file.flush())
        .map_err(|error| format!("写入 sidecar 启动兜底日志失败：{}", error))
}

/// 启动单条 sidecar 输出管道的持续排空线程。
/// 流程：以固定 4 KiB 缓冲循环读取 stdout 或 stderr，每个块交给有界追加逻辑，EOF 后自然退出。
/// 参数：reader 为已从 Child 取出的管道读端，process_log_path 为共享活动日志；返回待进程退出后回收的线程句柄。
/// 异常/边界：读取或日志写入失败时停止该管道，绝不记录输入 bootstrap、环境变量或额外错误正文。
fn spawn_process_log_drain<R>(mut reader: R, process_log_path: PathBuf) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0_u8; PROCESS_LOG_CHUNK_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    if append_process_log_chunk(&process_log_path, &buffer[..read_bytes]).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// 取得 sidecar 的 stdout/stderr 管道并分别启动有界日志排空线程。
/// 流程：先同时取出两个读端，确认均存在后再启动线程，避免只启动一半后无法由调用方回收。
/// 参数：child 为刚完成独立 PGID 验证的 sidecar 子进程，process_log_path 为活动兜底日志；返回两个线程句柄。
/// 异常/边界：任一管道缺失时返回启动错误，调用方必须立即清理进程组；不降级为继承终端或无限文件句柄。
fn start_process_log_drains(
    child: &mut Child,
    process_log_path: &std::path::Path,
) -> Result<Vec<thread::JoinHandle<()>>, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "sidecar stdout 管道不可用".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "sidecar stderr 管道不可用".to_string())?;
    Ok(vec![
        spawn_process_log_drain(stdout, process_log_path.to_path_buf()),
        spawn_process_log_drain(stderr, process_log_path.to_path_buf()),
    ])
}

/// 回收已经到达 EOF 的 sidecar 兜底日志线程。
/// 流程：逐个取出并 join，确保进程末尾输出已 flush 后才向调用方报告清理完成。
/// 参数：threads 为当前受管进程持有的 stdout/stderr 线程集合；成功返回空值并清空集合。
/// 异常/边界：仅允许在进程组确认退出后调用，避免等待仍持有管道写端的活进程；线程 panic 返回可诊断错误。
fn join_process_log_threads(threads: &mut Vec<thread::JoinHandle<()>>) -> Result<(), String> {
    while let Some(thread_handle) = threads.pop() {
        thread_handle
            .join()
            .map_err(|_| "sidecar 启动兜底日志线程异常退出".to_string())?;
    }
    Ok(())
}

impl RuntimeSidecar {
    /// 启动并接管 FastAPI sidecar。
    /// 流程：开发模式先清理自身热重载残留，再验证固定端口空闲，注入绝对数据/轮转日志路径，通过 stdin 一次性传入模型注册表，以严格双文件上限持续收集 stdout/stderr，等待 `/health`。
    /// 参数：app 用于定位 app_data_dir，model_catalog_json 只通过子进程 stdin 传递；返回空字符串，App 内部 HTTP 请求依赖内网 Origin 免授权码。
    /// 异常/边界：端口占用、进程提前退出或健康超时均 fail-fast；不会杀死占用端口的外部进程，启动兜底日志总量不超过两个 512 KiB 文件。
    pub fn start(&self, app: &AppHandle, model_catalog_json: &str) -> Result<String, String> {
        let mut process_state = self
            .state
            .lock()
            .map_err(|_| "sidecar 进程锁已损坏".to_string())?;
        if process_state.process.is_some() {
            return Err("sidecar 已由当前 App 生命周期管理，禁止重复启动".to_string());
        }
        let binary_path = resolve_sidecar_binary()?;
        cleanup_stale_development_sidecar(&binary_path)?;
        ensure_port_available()?;
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("读取 sidecar 应用数据目录失败：{}", error))?;
        let data_dir = app_data_dir.join("sidecar").join("data");
        let log_dir = app_data_dir.join("sidecar").join("logs");
        fs::create_dir_all(&data_dir)
            .map_err(|error| format!("创建 sidecar 数据目录失败：{}", error))?;
        fs::create_dir_all(&log_dir)
            .map_err(|error| format!("创建 sidecar 日志目录失败：{}", error))?;
        let log_file_path = log_dir.join("aitool-sidecar.log");
        let process_log_file_path = log_dir.join("aitool-sidecar-process.log");
        prepare_process_log(&process_log_file_path)?;
        let mut private_rpc = app.state::<RuntimePrivateRpc>().bootstrap()?;
        let bootstrap_result = build_model_catalog_bootstrap(model_catalog_json, &private_rpc);
        private_rpc.secret.clear();
        let mut bootstrap = bootstrap_result?;
        let mut command = Command::new(&binary_path);
        #[cfg(unix)]
        command.process_group(0);
        command
            .env("AITOOL_SIDECAR_HOST", "127.0.0.1")
            .env("AITOOL_SIDECAR_PORT", SIDECAR_PORT.to_string())
            .env(
                "AITOOL_ACCESS_TOKEN_DATABASE_FILE",
                data_dir.join("access-tokens.sqlite3"),
            )
            .env("AITOOL_QUOTA_DATABASE_FILE", data_dir.join("quota.sqlite3"))
            .env("AITOOL_LOG_FILE", &log_file_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(debug_assertions)]
        command
            .env("AITOOL_ENABLE_DEV_BEARER_TOKEN", "1")
            .env("AITOOL_DEV_ACCESS_TOKEN", DEV_BEARER_TOKEN);
        let mut child = command.spawn().map_err(|error| {
            format!(
                "启动 FastAPI sidecar 失败（{}）：{}",
                binary_path.display(),
                error
            )
        })?;
        let process_group_id = match i32::try_from(child.id()) {
            Ok(value) => value,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("sidecar PID 超出平台范围，已回收直接子进程".to_string());
            }
        };
        #[cfg(unix)]
        {
            let process_id = process_group_id;
            let process_group_id = unsafe { libc::getpgid(process_id) };
            if process_group_id != process_id {
                let process_group_error = if process_group_id == -1 {
                    std::io::Error::last_os_error().to_string()
                } else {
                    format!("实际 PGID {}", process_group_id)
                };
                let cleanup_result = terminate_direct_child_with_timeout(&mut child);
                return Err(match cleanup_result {
                    Ok(()) => format!(
                        "sidecar 未进入独立进程组，已拒绝继续启动并回收直接子进程：{}",
                        process_group_error
                    ),
                    Err(cleanup_error) => format!(
                        "sidecar 未进入独立进程组，且直接子进程清理失败：{}；{}",
                        process_group_error, cleanup_error
                    ),
                });
            }
        }
        let mut process_log_threads =
            match start_process_log_drains(&mut child, &process_log_file_path) {
                Ok(threads) => threads,
                Err(error) => {
                    let mut no_process_log_threads = Vec::new();
                    return Err(cleanup_start_failure(
                        error,
                        &mut child,
                        process_group_id,
                        &mut no_process_log_threads,
                        (&log_file_path, &process_log_file_path),
                    ));
                }
            };
        let bootstrap_result = child
            .stdin
            .take()
            .ok_or_else(|| "FastAPI sidecar stdin 管道不可用".to_string())
            .and_then(|stdin| write_and_close_bootstrap(stdin, &bootstrap));
        bootstrap.fill(0);
        bootstrap.clear();
        if let Err(error) = bootstrap_result {
            return Err(cleanup_start_failure(
                error,
                &mut child,
                process_group_id,
                &mut process_log_threads,
                (&log_file_path, &process_log_file_path),
            ));
        }
        if let Err(error) = wait_until_healthy(&mut child) {
            return Err(cleanup_start_failure(
                error,
                &mut child,
                process_group_id,
                &mut process_log_threads,
                (&log_file_path, &process_log_file_path),
            ));
        }
        process_state.process = Some(ManagedSidecarProcess {
            child,
            process_group_id,
            process_log_threads,
        });
        Ok(String::new())
    }

    /// 停止当前 App 启动的 sidecar 进程组。
    /// 流程：从互斥状态取出 Child，对独立进程组先发 SIGTERM，超时后 SIGKILL，再 wait 回收直接子进程。
    /// 参数：无；返回完整清理结果。
    /// 异常/边界：没有子进程时幂等成功；只使用本次 Child PID 对应进程组，绝不扫描端口或终止其它进程。
    pub fn shutdown(&self) -> Result<(), String> {
        let mut process_state = self
            .state
            .lock()
            .map_err(|_| "sidecar 进程锁已损坏".to_string())?;
        let managed_process = process_state.process.take();
        process_state.client_id.clear();
        process_state.client_secret.clear();
        process_state.approval_attempts.clear();
        if let Some(mut process) = managed_process {
            if let Err(error) =
                terminate_sidecar_process_group(&mut process.child, process.process_group_id)
            {
                process_state.process = Some(process);
                return Err(error);
            }
            join_process_log_threads(&mut process.process_log_threads)?;
        }
        Ok(())
    }

    /// 兼容旧前端的短 Token 续签入口。
    /// 流程：授权码改造后不再签发短 Token，仅确认 sidecar 仍运行后返回空字符串。
    /// 异常/边界：不启动新进程、不读取磁盘、不输出凭据；业务请求应依赖内网 Origin 或 App 授权码。
    pub fn refresh_access_token(&self) -> Result<String, String> {
        let mut process_state = self
            .state
            .lock()
            .map_err(|_| "sidecar 进程锁已损坏".to_string())?;
        ensure_managed_sidecar_running(&mut process_state, "确认授权码模式 sidecar 状态")?;
        Ok(String::new())
    }

    /// 兼容旧前端的设备码批准入口。
    /// 流程：授权码改造后设备码流程已下线，调用时返回固定错误提示。
    /// 参数：user_code 为旧设备码输入，当前不再使用。
    /// 异常/边界：不创建 pending、不调用 HTTP 批准接口、不接触任何 Basic 凭据。
    pub fn approve_device_authorization(&self, _user_code: &str) -> Result<String, String> {
        Err("设备码授权已下线，请在系统设置中创建或申请 App 授权码。".to_string())
    }

    /// 使用新模型注册表重启 sidecar 并换取全新短 Token。
    /// 流程：只停止本 App 持有的子进程，再复用完整启动门禁；参数同 start；返回新 Token。
    /// 异常/边界：重启失败不恢复旧进程，调用方必须向 UI 返回清晰错误，禁止继续使用失效 Token。
    pub fn restart(&self, app: &AppHandle, model_catalog_json: &str) -> Result<String, String> {
        self.shutdown()?;
        self.start(app, model_catalog_json)
    }
}

impl Drop for RuntimeSidecar {
    /// Runtime 状态销毁时尽力清理子进程，作为 Tauri RunEvent 清理之外的最后防线。
    fn drop(&mut self) {
        if let Ok(process_state) = self.state.get_mut() {
            process_state.client_id.clear();
            process_state.client_secret.clear();
            process_state.approval_attempts.clear();
            if let Some(mut process) = process_state.process.take() {
                if terminate_sidecar_process_group(&mut process.child, process.process_group_id)
                    .is_ok()
                {
                    let _ = join_process_log_threads(&mut process.process_log_threads);
                }
            }
        }
    }
}

/// 确认受管 sidecar 直接子进程仍运行，并在组长提前退出时清理其进程组。
/// 流程：读取直接子进程状态；若已退出则取出受管 PGID、整组清理后清空临时凭据。
/// 参数：process_state 为 App 生命周期状态，action 为错误中的当前操作；成功返回空值。
/// 异常/边界：PyInstaller 外层退出不等于 Uvicorn 后代退出，清理失败会与原退出状态一起返回。
fn ensure_managed_sidecar_running(
    process_state: &mut SidecarProcessState,
    action: &str,
) -> Result<(), String> {
    let status = process_state
        .process
        .as_mut()
        .ok_or_else(|| format!("sidecar 未运行，无法{}", action))?
        .child
        .try_wait()
        .map_err(|error| format!("读取 sidecar 状态失败：{}", error))?;
    let Some(exit_status) = status else {
        return Ok(());
    };
    let mut managed_process = process_state
        .process
        .take()
        .ok_or_else(|| "sidecar 运行状态在清理前丢失".to_string())?;
    let cleanup_result = terminate_sidecar_process_group(
        &mut managed_process.child,
        managed_process.process_group_id,
    );
    let log_result = if cleanup_result.is_ok() {
        join_process_log_threads(&mut managed_process.process_log_threads)
    } else {
        Ok(())
    };
    process_state.client_id.clear();
    process_state.client_secret.clear();
    process_state.approval_attempts.clear();
    Err(match (cleanup_result, log_result) {
        (Ok(()), Ok(())) => format!(
            "sidecar 已退出（{}），无法{}；残余进程组已回收",
            exit_status, action
        ),
        (Ok(()), Err(error)) => format!(
            "sidecar 已退出（{}），无法{}；残余进程组已回收，但兜底日志线程回收失败：{}",
            exit_status, action, error
        ),
        (Err(error), _) => format!(
            "sidecar 已退出（{}），无法{}；残余进程组清理失败：{}",
            exit_status, action, error
        ),
    })
}

/// 合并 sidecar 启动主错误与进程清理结果。
/// 流程：始终执行整组清理，清理成功后等待 stdout/stderr 排空落盘，再合并清理结果与两个有界日志路径。
/// 参数：error 为启动阶段错误，process/PGID 为受管进程，process_log_threads 为兜底日志线程，log_paths 为业务日志与进程日志；返回完整诊断文本。
/// 异常/边界：不会吞掉清理错误，也不会在 PGID 未确认消失时宣称已经回收。
fn cleanup_start_failure(
    error: String,
    process: &mut Child,
    process_group_id: i32,
    process_log_threads: &mut Vec<thread::JoinHandle<()>>,
    log_paths: (&PathBuf, &PathBuf),
) -> String {
    let cleanup_message = match terminate_sidecar_process_group(process, process_group_id) {
        Ok(()) => match join_process_log_threads(process_log_threads) {
            Ok(()) => "sidecar 进程组已回收，兜底日志已落盘".to_string(),
            Err(log_error) => format!(
                "sidecar 进程组已回收，但兜底日志线程回收失败：{}",
                log_error
            ),
        },
        Err(cleanup_error) => format!("sidecar 进程组清理失败：{}", cleanup_error),
    };
    format!(
        "{}；{}；业务日志：{}；启动兜底日志：{}",
        error,
        cleanup_message,
        log_paths.0.display(),
        log_paths.1.display()
    )
}

/// 终止并回收本次 App 创建的独立 sidecar 进程组。
/// 流程：以直接子进程 PID 作为 PGID 发送 SIGTERM，轮询父进程和组成员，2 秒后仍存在则 SIGKILL，最后 wait。
/// 参数：process 为 RuntimeSidecar 持有的 PyInstaller 外层 Child；返回清理完成结果。
/// 异常/边界：ESRCH 视为已退出；不会按名称或端口查杀，避免影响其它应用；非 Unix 平台退化为 Child kill/wait。
fn terminate_sidecar_process_group(
    process: &mut Child,
    process_group_id: i32,
) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut signal_error: Option<String> = None;
        let terminate_result = unsafe { libc::killpg(process_group_id, libc::SIGTERM) };
        if terminate_result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                signal_error = Some(format!("停止 sidecar 进程组失败：{}", error));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let parent_exited = match process.try_wait() {
                Ok(status) => status.is_some(),
                Err(error) => {
                    if signal_error.is_none() {
                        signal_error = Some(format!("读取 sidecar 状态失败：{}", error));
                    }
                    false
                }
            };
            let group_exists = match sidecar_process_group_exists(process_group_id) {
                Ok(exists) => exists,
                Err(error) => {
                    if signal_error.is_none() {
                        signal_error = Some(error);
                    }
                    true
                }
            };
            if parent_exited && !group_exists {
                return signal_error.map_or(Ok(()), Err);
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let kill_result = unsafe { libc::killpg(process_group_id, libc::SIGKILL) };
        if kill_result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) && signal_error.is_none() {
                signal_error = Some(format!("强制停止 sidecar 进程组失败：{}", error));
            }
        }
        let cleanup_deadline = Instant::now() + Duration::from_secs(2);
        let mut parent_exited = false;
        loop {
            match process.try_wait() {
                Ok(Some(_)) => parent_exited = true,
                Ok(None) => {
                    if let Err(error) = process.kill() {
                        if signal_error.is_none() {
                            signal_error =
                                Some(format!("强制停止 sidecar 直接子进程失败：{}", error));
                        }
                    }
                }
                Err(error) => {
                    if signal_error.is_none() {
                        signal_error = Some(format!("读取 sidecar 状态失败：{}", error));
                    }
                }
            }
            let group_exists = match sidecar_process_group_exists(process_group_id) {
                Ok(exists) => exists,
                Err(error) => {
                    if signal_error.is_none() {
                        signal_error = Some(error);
                    }
                    true
                }
            };
            if parent_exited && !group_exists {
                break;
            }
            if Instant::now() >= cleanup_deadline {
                if signal_error.is_none() {
                    signal_error = Some(format!(
                        "强制停止后 sidecar 仍未完整退出：PGID {}，组长已退出={}，进程组存在={}",
                        process_group_id, parent_exited, group_exists
                    ));
                }
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        signal_error.map_or(Ok(()), Err)
    }
    #[cfg(not(unix))]
    {
        if process
            .try_wait()
            .map_err(|error| format!("读取 sidecar 状态失败：{}", error))?
            .is_none()
        {
            process
                .kill()
                .map_err(|error| format!("停止 sidecar 失败：{}", error))?;
        }
        process
            .wait()
            .map_err(|error| format!("回收 sidecar 子进程失败：{}", error))?;
        Ok(())
    }
}

/// 在没有可信独立 PGID 时有界终止并回收直接子进程。
/// 流程：发送单进程 kill 后轮询 try_wait，最多等待两秒，禁止无期限阻塞启动线程。
/// 参数：process 为尚未通过进程组验证的直接子进程；返回回收结果。
/// 异常/边界：不会扫描或终止未知进程组，避免误杀其它应用。
#[cfg(unix)]
fn terminate_direct_child_with_timeout(process: &mut Child) -> Result<(), String> {
    let mut cleanup_error = process
        .kill()
        .err()
        .map(|error| format!("停止未受管 sidecar 直接子进程失败：{}", error));
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match process.try_wait() {
            Ok(Some(_)) => return cleanup_error.map_or(Ok(()), Err),
            Ok(None) => {}
            Err(error) => {
                if cleanup_error.is_none() {
                    cleanup_error = Some(format!("读取未受管 sidecar 状态失败：{}", error));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(cleanup_error.unwrap_or_else(|| {
                "停止未受管 sidecar 直接子进程超时，进程可能仍存活".to_string()
            }));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// 探测指定 sidecar 进程组是否仍存在。
/// 流程：使用信号 0 查询 PGID；成功或 EPERM 均表示组仍存在，只有 ESRCH 表示组已消失。
/// 参数：process_group_id 为本次 App 创建的独立 PGID；返回进程组是否存在。
/// 异常/边界：其它系统错误返回诊断信息，禁止把权限错误误判为清理成功。
#[cfg(unix)]
fn sidecar_process_group_exists(process_group_id: i32) -> Result<bool, String> {
    if unsafe { libc::killpg(process_group_id, 0) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Ok(true),
        _ => Err(format!(
            "探测 sidecar 进程组状态失败（PGID {}）：{}",
            process_group_id, error
        )),
    }
}

/// 清理开发热重载留下的旧 sidecar 监听进程。
/// 流程：仅 debug + macOS 生效，通过 lsof 找到 18080 监听 PID，确认命令行指向当前项目 sidecar 二进制后结束同进程组。
/// 参数：binary_path 为当前启动将使用的 sidecar 二进制；返回清理结果。
/// 异常/边界：正式包不启用；无法确认进程来源时不终止，交回固定端口检查输出明确错误。
fn cleanup_stale_development_sidecar(binary_path: &Path) -> Result<(), String> {
    #[cfg(all(debug_assertions, target_os = "macos"))]
    {
        let output = Command::new("/usr/sbin/lsof")
            .args(["-nP", "-tiTCP:18080", "-sTCP:LISTEN"])
            .output()
            .map_err(|error| format!("检查开发 sidecar 端口占用失败：{}", error))?;
        if !output.status.success() || output.stdout.is_empty() {
            return Ok(());
        }
        let pid_output = String::from_utf8_lossy(&output.stdout);
        for raw_pid in pid_output.lines() {
            let pid = raw_pid
                .trim()
                .parse::<i32>()
                .map_err(|error| format!("解析开发 sidecar 端口占用 PID 失败：{}", error))?;
            if development_sidecar_process_matches(pid, binary_path)? {
                terminate_development_sidecar_process(pid)?;
            }
        }
    }
    let _ = binary_path;
    Ok(())
}

/// 判断监听进程是否为当前项目构建出的 sidecar。
/// 流程：读取 ps 命令行，只接受当前二进制绝对路径或当前 src-tauri/target/debug 下的开发副本。
/// 参数：pid 为端口监听进程；binary_path 为当前 sidecar 二进制路径；返回是否允许清理。
/// 异常/边界：进程已退出时按不匹配处理；命令执行失败不误杀。
#[cfg(all(debug_assertions, target_os = "macos"))]
fn development_sidecar_process_matches(pid: i32, binary_path: &Path) -> Result<bool, String> {
    let output = Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map_err(|error| format!("读取开发 sidecar 进程命令失败：{}", error))?;
    if !output.status.success() {
        return Ok(false);
    }
    let command_line = String::from_utf8_lossy(&output.stdout);
    let expected_binary = binary_path.to_string_lossy();
    let expected_debug_copy = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(SIDECAR_BINARY_NAME);
    Ok(command_line.contains(expected_binary.as_ref())
        || command_line.contains(expected_debug_copy.to_string_lossy().as_ref()))
}

/// 终止开发期残留 sidecar。
/// 流程：优先按进程组发送 SIGTERM，短等待后仍监听则补 SIGKILL；只服务开发热重载清理。
/// 参数：pid 为已确认属于当前项目 sidecar 的监听进程；返回清理结果。
/// 异常/边界：进程自然退出视为成功；清理后端口仍占用则返回错误，让启动阶段停止。
#[cfg(all(debug_assertions, target_os = "macos"))]
fn terminate_development_sidecar_process(pid: i32) -> Result<(), String> {
    let process_group_id = unsafe { libc::getpgid(pid) };
    let signal_target = if process_group_id > 0 {
        -process_group_id
    } else {
        pid
    };
    let terminate_result = unsafe { libc::kill(signal_target, libc::SIGTERM) };
    if terminate_result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(format!("停止开发 sidecar 残留进程失败：{}", error));
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if ensure_port_available().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let kill_result = unsafe { libc::kill(signal_target, libc::SIGKILL) };
    if kill_result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(format!("强制停止开发 sidecar 残留进程失败：{}", error));
        }
    }
    let cleanup_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < cleanup_deadline {
        if ensure_port_available().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "开发 sidecar 残留进程已发送停止信号，但固定端口 {} 仍被占用",
        SIDECAR_PORT
    ))
}

/// 验证 18080 未被任何进程监听。
fn ensure_port_available() -> Result<(), String> {
    TcpListener::bind(("127.0.0.1", SIDECAR_PORT))
        .map(drop)
        .map_err(|error| {
            format!(
                "FastAPI sidecar 无法启动：固定端口 {} 已被占用（{}）。应用不会终止占用进程，也不会改用其它端口",
                SIDECAR_PORT, error
            )
        })
}

/// 等待 sidecar 健康检查成功，同时检测进程是否提前退出。
fn wait_until_healthy(child: &mut Child) -> Result<(), String> {
    let deadline = Instant::now() + HEALTH_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("读取 sidecar 启动状态失败：{}", error))?
        {
            return Err(format!("FastAPI sidecar 在健康检查前退出：{}", status));
        }
        if TcpStream::connect_timeout(
            &([127, 0, 0, 1], SIDECAR_PORT).into(),
            Duration::from_millis(150),
        )
        .is_ok()
        {
            let response = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(1))
                .build()
                .map_err(|error| format!("创建 sidecar 健康检查客户端失败：{}", error))?
                .get(format!("{}/health", SIDECAR_BASE_URL))
                .send();
            if matches!(response, Ok(value) if value.status().is_success())
                && Instant::now() < deadline
            {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(120));
    }
    Err(format!(
        "等待 FastAPI sidecar /health 超时（{} 秒）",
        HEALTH_TIMEOUT.as_secs()
    ))
}

/// 解析开发期或打包后的 externalBin 绝对路径。
fn resolve_sidecar_binary() -> Result<PathBuf, String> {
    if target_triple() == "unsupported-target" {
        return Err(
            "当前 CodexMan 桌面 sidecar 仅支持 macOS；其它平台不会降级到外部 Python".to_string(),
        );
    }
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("读取当前可执行文件路径失败：{}", error))?;
    if let Some(parent) = current_exe.parent() {
        let bundled = parent.join(SIDECAR_BINARY_NAME);
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!("{}-{}", SIDECAR_BINARY_NAME, target_triple()));
    if development.is_file() {
        return Ok(development);
    }
    Err(format!(
        "未找到 PyInstaller sidecar：{}；请先执行 npm run build:sidecar",
        development.display()
    ))
}

/// 返回当前编译目标三元组，用于匹配 Tauri externalBin 文件名。
fn target_triple() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64")
    )))]
    {
        "unsupported-target"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造不包含真实路径或凭据的测试私有 RPC 配置。
    /// 流程：返回固定短字符串供 bootstrap 长度和字段断言使用；参数无；返回测试配置。
    /// 异常/边界：仅用于单元测试，不会启动 listener 或子进程。
    fn test_private_rpc_bootstrap() -> PrivateRpcBootstrap {
        PrivateRpcBootstrap {
            socket_path: "/tmp/aitool-test.sock".to_string(),
            secret: "test-secret".to_string(),
        }
    }

    /// bootstrap 必须只含一个紧凑 envelope、以 LF 结束，并允许完整帧精确达到 1 MiB。
    #[test]
    fn bootstrap_frame_is_single_lf_terminated_envelope_at_exact_limit() {
        let private_rpc = test_private_rpc_bootstrap();
        let empty_frame = build_model_catalog_bootstrap("\"\"", &private_rpc)
            .expect("空字符串目录应能构造启动帧");
        let catalog_value = "x".repeat(MAX_BOOTSTRAP_FRAME_BYTES - empty_frame.len());
        let catalog_json = serde_json::to_string(&catalog_value).expect("测试目录应能序列化");

        let frame = build_model_catalog_bootstrap(&catalog_json, &private_rpc)
            .expect("精确 1 MiB 完整帧应通过");

        assert_eq!(frame.len(), MAX_BOOTSTRAP_FRAME_BYTES);
        assert_eq!(frame.last(), Some(&b'\n'));
        assert_eq!(frame.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&frame[..frame.len() - 1])
                .expect("LF 前应为合法 JSON")["modelCatalog"],
            catalog_value
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&frame[..frame.len() - 1])
                .expect("LF 前应为合法 JSON")["privateRpc"]["socketPath"],
            private_rpc.socket_path
        );
    }

    /// bootstrap 完整 JSON+LF 帧超过 1 MiB 一个字节时必须在启动子进程前拒绝。
    #[test]
    fn bootstrap_frame_rejects_one_byte_over_limit() {
        let private_rpc = test_private_rpc_bootstrap();
        let empty_frame = build_model_catalog_bootstrap("\"\"", &private_rpc)
            .expect("空字符串目录应能构造启动帧");
        let catalog_value = "x".repeat(MAX_BOOTSTRAP_FRAME_BYTES - empty_frame.len() + 1);
        let catalog_json = serde_json::to_string(&catalog_value).expect("测试目录应能序列化");

        let error = build_model_catalog_bootstrap(&catalog_json, &private_rpc)
            .expect_err("越界完整帧必须拒绝");

        assert_eq!(error, "sidecar stdin bootstrap 超过 1 MiB 限制");
    }

    /// 写入函数返回前必须关闭其拥有的管道写端，让 Python 无需额外数据即可读取到 EOF。
    #[cfg(unix)]
    #[test]
    fn bootstrap_writer_is_closed_after_single_frame() {
        let (writer, mut reader) =
            std::os::unix::net::UnixStream::pair().expect("应能创建测试用本机匿名连接");
        let frame = build_model_catalog_bootstrap("[]", &test_private_rpc_bootstrap())
            .expect("空目录应能构造启动帧");

        write_and_close_bootstrap(writer, &frame).expect("启动帧应能完整写入并关闭写端");

        let mut received = Vec::new();
        reader
            .read_to_end(&mut received)
            .expect("写端关闭后读端应立即读取到 EOF");
        assert_eq!(received, frame);
    }

    /// 固定端口契约测试，防止后续改动悄悄引入端口漂移。
    #[test]
    fn sidecar_port_is_stable() {
        assert_eq!(SIDECAR_PORT, 18_080);
        assert_eq!(SIDECAR_BASE_URL, "http://127.0.0.1:18080");
    }

    /// 开发期固定 Bearer Token 必须满足服务端最小强度门禁。
    #[cfg(debug_assertions)]
    #[test]
    fn debug_dev_bearer_token_matches_server_contract() {
        assert!(DEV_BEARER_TOKEN.is_ascii());
        assert!(DEV_BEARER_TOKEN.len() >= 32);
    }

    /// externalBin 名称测试，确保开发目标文件和 tauri.conf 逻辑名一致。
    #[test]
    fn development_binary_name_contains_target_triple() {
        let name = format!("{}-{}", SIDECAR_BINARY_NAME, target_triple());
        assert!(name.starts_with("codexman-ai-sidecar-"));
    }

    /// 每次 App 启动必须覆盖旧活动日志并删除旧备份，避免跨启动累计历史进程输出。
    #[test]
    fn process_log_preparation_clears_previous_run() {
        let log_dir = std::env::temp_dir().join(format!(
            "aitool-sidecar-process-log-prepare-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&log_dir).expect("应能创建测试日志目录");
        let log_path = log_dir.join("aitool-sidecar-process.log");
        let backup_path = log_path.with_extension("log.1");
        fs::write(&log_path, b"old-active").expect("应能写入旧活动日志");
        fs::write(&backup_path, b"old-backup").expect("应能写入旧备份日志");

        prepare_process_log(&log_path).expect("新启动应能重置兜底日志");

        assert_eq!(fs::metadata(&log_path).expect("活动日志应存在").len(), 0);
        assert!(!backup_path.exists());
        fs::remove_dir_all(log_dir).expect("应能清理测试日志目录");
    }

    /// 长时间 stdout/stderr 输出必须持续轮转，且活动文件与唯一备份都不能超过 512 KiB。
    #[test]
    fn process_log_rotation_has_strict_total_bound() {
        let log_dir = std::env::temp_dir().join(format!(
            "aitool-sidecar-process-log-bound-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&log_dir).expect("应能创建测试日志目录");
        let log_path = log_dir.join("aitool-sidecar-process.log");
        let backup_path = log_path.with_extension("log.1");
        prepare_process_log(&log_path).expect("应能初始化兜底日志");
        let chunk = vec![b'x'; PROCESS_LOG_CHUNK_BYTES];

        for _ in 0..((PROCESS_LOG_MAX_BYTES as usize / PROCESS_LOG_CHUNK_BYTES) * 3) {
            append_process_log_chunk(&log_path, &chunk).expect("有界日志追加不应失败");
        }

        let active_bytes = fs::metadata(&log_path).expect("活动日志应存在").len();
        let backup_bytes = fs::metadata(&backup_path).expect("轮转备份应存在").len();
        assert!(active_bytes <= PROCESS_LOG_MAX_BYTES);
        assert!(backup_bytes <= PROCESS_LOG_MAX_BYTES);
        assert!(active_bytes + backup_bytes <= PROCESS_LOG_MAX_BYTES * 2);
        fs::remove_dir_all(log_dir).expect("应能清理测试日志目录");
    }

    /// 预配置失败文本必须能通过同一排空线程写入兜底日志，保证业务日志初始化前仍可诊断。
    #[test]
    fn process_log_drain_persists_early_startup_failure() {
        let log_dir = std::env::temp_dir().join(format!(
            "aitool-sidecar-process-log-drain-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&log_dir).expect("应能创建测试日志目录");
        let log_path = log_dir.join("aitool-sidecar-process.log");
        prepare_process_log(&log_path).expect("应能初始化兜底日志");

        let drain = spawn_process_log_drain(
            std::io::Cursor::new(b"sidecar_start_failed: RuntimeError\n".to_vec()),
            log_path.clone(),
        );
        drain.join().expect("兜底日志线程不应异常退出");

        let content = fs::read_to_string(&log_path).expect("应能读取兜底日志");
        assert_eq!(content, "sidecar_start_failed: RuntimeError\n");
        fs::remove_dir_all(log_dir).expect("应能清理测试日志目录");
    }

    /// 进程组清理必须连同忽略 SIGTERM 的后代一起强制回收，且耗时有界。
    #[cfg(unix)]
    #[test]
    fn sidecar_process_group_cleanup_kills_stubborn_descendants() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("trap '' TERM; (trap '' TERM; sleep 30) & wait")
            .process_group(0);
        let mut child = command.spawn().expect("应能启动独立测试进程组");
        let process_group_id = i32::try_from(child.id()).expect("测试 PID 应在平台范围内");
        thread::sleep(Duration::from_millis(100));
        let started_at = Instant::now();
        terminate_sidecar_process_group(&mut child, process_group_id).expect("进程组应被完整回收");
        assert!(started_at.elapsed() < Duration::from_secs(5));
        assert!(!sidecar_process_group_exists(process_group_id).expect("应能探测测试进程组"));
    }
}
