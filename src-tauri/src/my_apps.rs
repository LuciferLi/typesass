use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;
use zip::ZipArchive;

use crate::frp_tunnel::{
    public_url_for_subdomain, resolve_existing_frpc_binary, resolve_frpc_binary, write_frpc_config,
    FrpHttpTunnelConfig,
};

/// 我的应用配置文件名；保存用户创建的本地托管和远程 URL 应用元数据。
const MY_APPS_FILE_NAME: &str = "my-apps.json";
/// 我的应用站点根目录名；每个本地托管应用按 appId 独立子目录隔离。
const MY_APPS_SITES_DIR_NAME: &str = "my-app-sites";
/// 静态服务监听主机；绑定所有网卡以允许同局域网电脑访问。
const SITE_SERVER_HOST: &str = "0.0.0.0";
/// 自动分配端口的最小值，避开系统常用端口。
const AUTO_PORT_MIN: u16 = 18_100;
/// 自动分配端口的最大值，为用户本地应用保留一段稳定区间。
const AUTO_PORT_MAX: u16 = 28_999;
/// 单个静态服务请求读取上限；只需要首行和基础 Header，避免异常客户端占用内存。
const STATIC_REQUEST_MAX_BYTES: usize = 4096;
/// 公网二级域名可用性探测使用的本地丢弃端口；frpc 注册阶段不依赖该端口真实提供服务。
const PUBLIC_SUBDOMAIN_PROBE_LOCAL_PORT: u16 = 9;
/// 公网二级域名可用性探测等待时长，留给 frps 返回重复域名拒绝结果。
const PUBLIC_SUBDOMAIN_PROBE_WAIT_MS: u64 = 900;

/// 我的应用运行时状态管理器。
#[derive(Default)]
pub struct RuntimeMyApps {
    /// 受管静态服务与最近状态；只记录 CodexMan 自己启动的服务，禁止管理其它进程。
    state: Mutex<MyAppsRuntimeState>,
}

/// 我的应用运行期可变状态。
#[derive(Default)]
struct MyAppsRuntimeState {
    /// appId 到静态服务线程的映射。
    servers: HashMap<String, ManagedSiteServer>,
    /// appId 到公网 FRP 客户端进程的映射。
    frpc_clients: HashMap<String, ManagedFrpcClient>,
    /// appId 到最近服务状态的映射。
    statuses: HashMap<String, MyAppRuntimeStatus>,
}

/// 单个受管静态服务线程。
struct ManagedSiteServer {
    /// 停止服务的单向信号。
    shutdown: Sender<()>,
    /// 服务线程句柄；停止时等待退出，确保端口释放。
    thread: thread::JoinHandle<()>,
    /// 当前服务监听端口。
    port: u16,
}

/// 单个受管 FRP 客户端进程。
struct ManagedFrpcClient {
    /// frpc 子进程句柄；停止应用时只终止 CodexMan 自己启动的进程。
    child: Child,
    /// 当前注册到服务器侧 frps 的二级域名前缀。
    subdomain: String,
}

/// 我的应用持久化文档。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MyAppsDocument {
    /// 持久化文档版本；后续结构升级时用于迁移。
    version: u32,
    /// 用户创建的应用列表。
    apps: Vec<MyAppRecord>,
}

/// 我的应用持久化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyAppRecord {
    /// 应用稳定 ID。
    pub id: String,
    /// 应用名称。
    pub name: String,
    /// logo data URL；为空时前端展示默认图标。
    pub logo_data_url: String,
    /// 访问方式：local 表示本地托管，remote 表示远程 URL。
    pub access_type: MyAppAccessType,
    /// 本地托管端口；远程 URL 为空。
    pub port: Option<u16>,
    /// 远程 URL；本地托管为空。
    pub remote_url: Option<String>,
    /// 公网访问二级域名前缀；为空时仅保留本机和局域网访问。
    #[serde(default)]
    pub public_subdomain: Option<String>,
    /// 创建时间，UTC ISO 字符串。
    pub created_at: String,
    /// 更新时间，UTC ISO 字符串。
    pub updated_at: String,
}

/// 我的应用访问方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MyAppAccessType {
    /// 本地静态资源托管。
    Local,
    /// 远程 URL 访问。
    Remote,
}

/// 我的应用服务状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MyAppServiceStatus {
    /// 服务正在启动。
    Starting,
    /// 服务已启动。
    Running,
    /// 服务已暂停或未启动。
    Paused,
    /// 服务启动失败。
    Failed,
    /// 远程 URL 应用没有本地服务。
    Unavailable,
}

/// 单个应用运行状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyAppRuntimeStatus {
    /// 当前服务状态。
    pub status: MyAppServiceStatus,
    /// 状态说明或最近错误。
    pub message: String,
}

/// 我的应用前端列表项响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyAppResponse {
    /// 应用稳定 ID。
    pub id: String,
    /// 应用名称。
    pub name: String,
    /// logo data URL。
    pub logo_data_url: String,
    /// 应用访问方式。
    pub access_type: MyAppAccessType,
    /// 本地托管端口。
    pub port: Option<u16>,
    /// 远程访问 URL。
    pub remote_url: Option<String>,
    /// 本机访问地址。
    pub local_url: String,
    /// 局域网访问地址。
    pub lan_url: String,
    /// 公网访问地址；未配置二级域名或远程 URL 应用为空。
    pub public_url: String,
    /// 公网访问二级域名前缀。
    pub public_subdomain: Option<String>,
    /// CodexMan 默认打开地址。
    pub open_url: String,
    /// 当前服务状态。
    pub service_status: MyAppServiceStatus,
    /// 服务状态说明。
    pub service_message: String,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
}

/// 创建我的应用请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMyAppParams {
    /// 应用名称。
    pub name: String,
    /// logo data URL。
    #[serde(default)]
    pub logo_data_url: String,
    /// 访问方式。
    pub access_type: MyAppAccessType,
    /// 本地托管端口。
    #[serde(default)]
    pub port: Option<u16>,
    /// 远程 URL。
    #[serde(default)]
    pub remote_url: Option<String>,
    /// 公网访问二级域名前缀。
    #[serde(default)]
    pub public_subdomain: Option<String>,
    /// 本地托管 zip data URL。
    #[serde(default)]
    pub zip_data_url: Option<String>,
}

/// 更新我的应用请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateMyAppParams {
    /// 应用稳定 ID。
    pub id: String,
    /// 应用名称。
    pub name: String,
    /// logo data URL。
    #[serde(default)]
    pub logo_data_url: String,
    /// 访问方式。
    pub access_type: MyAppAccessType,
    /// 本地托管端口。
    #[serde(default)]
    pub port: Option<u16>,
    /// 远程 URL。
    #[serde(default)]
    pub remote_url: Option<String>,
    /// 公网访问二级域名前缀。
    #[serde(default)]
    pub public_subdomain: Option<String>,
    /// 可选新 zip data URL；为空时复用现有解压目录。
    #[serde(default)]
    pub zip_data_url: Option<String>,
}

/// 单应用 ID 请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MyAppIdParams {
    /// 应用稳定 ID。
    pub app_id: String,
}

/// 打开我的应用请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenMyAppParams {
    /// 应用稳定 ID。
    pub app_id: String,
    /// 打开方式。
    pub target: MyAppOpenTarget,
}

/// 我的应用打开方式。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MyAppOpenTarget {
    /// 使用 CodexMan 新窗口打开。
    Codexman,
    /// 使用系统默认浏览器打开。
    Browser,
}

/// 自动分配端口响应。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocateMyAppPortResponse {
    /// 当前可用端口。
    pub port: u16,
}

/// 通用操作响应。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyAppOperationResponse {
    /// 操作是否已完成。
    pub ok: bool,
}

impl RuntimeMyApps {
    /// 初始化我的应用运行时。
    /// 流程：确保数据目录存在，读取本地托管应用并尝试启动所有静态服务。
    /// 参数：app 为 Tauri App 句柄。
    /// 返回：初始化结果。
    /// 异常/边界：单个应用启动失败只记录该应用状态，不阻止 App 主窗口进入。
    pub fn initialize(&self, app: &AppHandle) -> Result<(), String> {
        ensure_my_apps_dirs(app)?;
        let records = load_document(app)?.apps;
        for record in records
            .iter()
            .filter(|item| item.access_type == MyAppAccessType::Local)
        {
            if let Err(error) = self.start_local_app(app, record) {
                self.set_status(
                    &record.id,
                    MyAppServiceStatus::Failed,
                    format!("启动失败：{}", error),
                )?;
            }
        }
        Ok(())
    }

    /// 读取我的应用列表。
    /// 流程：读取持久化记录并合并运行时服务状态和访问地址。
    /// 参数：app 为 Tauri App 句柄。
    /// 返回：前端展示列表。
    /// 异常/边界：缺少运行状态的本地应用按已暂停展示，远程 URL 按不可用展示服务状态。
    pub fn list(&self, app: &AppHandle) -> Result<Vec<MyAppResponse>, String> {
        let document = load_document(app)?;
        let state = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
        let lan_ip = resolve_lan_ip();
        Ok(document
            .apps
            .iter()
            .map(|record| build_app_response(record, &state.statuses, &lan_ip))
            .collect())
    }

    /// 自动分配可用端口。
    /// 流程：避开已配置应用端口，再在固定端口段内寻找可绑定端口。
    /// 参数：app 为 Tauri App 句柄。
    /// 返回：当前可用端口。
    /// 异常/边界：不保留端口占用，保存时仍会再次校验。
    pub fn allocate_port(&self, app: &AppHandle) -> Result<AllocateMyAppPortResponse, String> {
        let used_ports: Vec<u16> = load_document(app)?
            .apps
            .iter()
            .filter_map(|record| record.port)
            .collect();
        for port in AUTO_PORT_MIN..=AUTO_PORT_MAX {
            if used_ports.contains(&port) {
                continue;
            }
            if is_port_available(port) {
                return Ok(AllocateMyAppPortResponse { port });
            }
        }
        Err("没有找到可用端口，请手动填写端口。".to_string())
    }

    /// 创建我的应用。
    /// 流程：校验字段、解压本地 zip、写入持久化记录并启动本地服务。
    /// 参数：app 为 Tauri App 句柄，params 为 HTTP 请求参数。
    /// 返回：创建后的应用列表项。
    /// 异常/边界：任一步失败不会写入半成品记录；本地 zip 目录会先写入临时目录再原子替换。
    pub fn create(
        &self,
        app: &AppHandle,
        params: CreateMyAppParams,
    ) -> Result<MyAppResponse, String> {
        let mut document = load_document(app)?;
        let id = format!("app_{}", Uuid::new_v4().simple());
        let now = now_iso_string();
        let zip_data_url = params.zip_data_url.clone().unwrap_or_default();
        let record = normalize_create_params(&id, &now, params)?;
        ensure_unique_port(&document.apps, None, record.port)?;
        ensure_unique_public_subdomain(&document.apps, None, record.public_subdomain.as_deref())?;
        ensure_remote_public_subdomain_available(app, record.public_subdomain.as_deref())?;
        if record.access_type == MyAppAccessType::Local {
            extract_site_zip(app, &id, &zip_data_url)?;
        }
        document.apps.push(record.clone());
        save_document(app, &document)?;
        if record.access_type == MyAppAccessType::Local {
            if let Err(error) = self.start_local_app(app, &record) {
                self.set_status(
                    &record.id,
                    MyAppServiceStatus::Failed,
                    format!("启动失败：{}", error),
                )?;
            }
        }
        let statuses = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?
            .statuses
            .clone();
        Ok(build_app_response(&record, &statuses, &resolve_lan_ip()))
    }

    /// 更新我的应用。
    /// 流程：按 ID 替换元数据；本地端口变化时先停止旧服务，zip 变化时替换站点目录，再启动新服务。
    /// 参数：app 为 Tauri App 句柄，params 为 HTTP 请求参数。
    /// 返回：更新后的应用列表项。
    /// 异常/边界：旧服务只停止 CodexMan 当前持有的线程，不结束其它进程。
    pub fn update(
        &self,
        app: &AppHandle,
        params: UpdateMyAppParams,
    ) -> Result<MyAppResponse, String> {
        let mut document = load_document(app)?;
        let index = document
            .apps
            .iter()
            .position(|record| record.id == params.id)
            .ok_or_else(|| "应用不存在，请刷新后重试。".to_string())?;
        let previous = document.apps[index].clone();
        let now = now_iso_string();
        let zip_data_url = params.zip_data_url.clone().unwrap_or_default();
        let next = normalize_update_params(&previous, &now, params)?;
        ensure_unique_port(&document.apps, Some(&next.id), next.port)?;
        ensure_unique_public_subdomain(
            &document.apps,
            Some(&next.id),
            next.public_subdomain.as_deref(),
        )?;
        if next.public_subdomain != previous.public_subdomain {
            ensure_remote_public_subdomain_available(app, next.public_subdomain.as_deref())?;
        }
        let should_replace_site =
            next.access_type == MyAppAccessType::Local && !zip_data_url.trim().is_empty();
        if next.access_type == MyAppAccessType::Local
            && !site_root(&next.id, app)?.join("index.html").is_file()
        {
            extract_site_zip(app, &next.id, &zip_data_url)?;
        } else if should_replace_site {
            extract_site_zip(app, &next.id, &zip_data_url)?;
        }
        if previous.access_type == MyAppAccessType::Local {
            self.stop(&previous.id)?;
        }
        if next.access_type == MyAppAccessType::Remote {
            remove_site_dir(app, &next.id)?;
        }
        document.apps[index] = next.clone();
        save_document(app, &document)?;
        if next.access_type == MyAppAccessType::Local {
            if let Err(error) = self.start_local_app(app, &next) {
                self.set_status(
                    &next.id,
                    MyAppServiceStatus::Failed,
                    format!("启动失败：{}", error),
                )?;
            }
        } else {
            self.set_status(
                &next.id,
                MyAppServiceStatus::Unavailable,
                "远程 URL 无本地服务。".to_string(),
            )?;
        }
        let statuses = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?
            .statuses
            .clone();
        Ok(build_app_response(&next, &statuses, &resolve_lan_ip()))
    }

    /// 删除我的应用。
    /// 流程：停止受管本地服务，删除持久化记录和站点目录。
    /// 参数：app 为 Tauri App 句柄，app_id 为待删除应用。
    /// 返回：操作结果。
    /// 异常/边界：应用不存在时返回明确错误；站点目录删除失败会阻止误报成功。
    pub fn delete(&self, app: &AppHandle, app_id: &str) -> Result<MyAppOperationResponse, String> {
        let mut document = load_document(app)?;
        let before_len = document.apps.len();
        let removed = document
            .apps
            .iter()
            .find(|record| record.id == app_id)
            .cloned()
            .ok_or_else(|| "应用不存在，请刷新后重试。".to_string())?;
        self.stop(app_id)?;
        document.apps.retain(|record| record.id != app_id);
        if document.apps.len() == before_len {
            return Err("应用不存在，请刷新后重试。".to_string());
        }
        save_document(app, &document)?;
        if removed.access_type == MyAppAccessType::Local {
            remove_site_dir(app, app_id)?;
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
        state.statuses.remove(app_id);
        Ok(MyAppOperationResponse { ok: true })
    }

    /// 启动或重启本地应用服务。
    /// 流程：读取应用记录，停止旧服务后重新绑定记录端口。
    /// 参数：app 为 Tauri App 句柄，app_id 为本地托管应用 ID。
    /// 返回：最新应用列表项。
    /// 异常/边界：远程 URL 应用不能启动服务。
    pub fn restart(&self, app: &AppHandle, app_id: &str) -> Result<MyAppResponse, String> {
        let record = load_document(app)?
            .apps
            .into_iter()
            .find(|record| record.id == app_id)
            .ok_or_else(|| "应用不存在，请刷新后重试。".to_string())?;
        if record.access_type != MyAppAccessType::Local {
            return Err("远程 URL 应用没有可重启的本地服务。".to_string());
        }
        self.stop(app_id)?;
        if let Err(error) = self.start_local_app(app, &record) {
            self.set_status(
                &record.id,
                MyAppServiceStatus::Failed,
                format!("启动失败：{}", error),
            )?;
            return Err(error);
        }
        let statuses = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?
            .statuses
            .clone();
        Ok(build_app_response(&record, &statuses, &resolve_lan_ip()))
    }

    /// 打开我的应用。
    /// 流程：本地应用先确保服务已启动，再按目标用 CodexMan 窗口或默认浏览器打开。
    /// 参数：app 为 Tauri App 句柄，params 为应用 ID 和打开目标。
    /// 返回：操作结果。
    /// 异常/边界：服务启动失败时不打开空窗口；远程 URL 只校验 URL 格式。
    pub fn open(
        &self,
        app: &AppHandle,
        params: OpenMyAppParams,
    ) -> Result<MyAppOperationResponse, String> {
        let record = load_document(app)?
            .apps
            .into_iter()
            .find(|record| record.id == params.app_id)
            .ok_or_else(|| "应用不存在，请刷新后重试。".to_string())?;
        if record.access_type == MyAppAccessType::Local {
            if let Err(error) = self.start_local_app(app, &record) {
                self.set_status(
                    &record.id,
                    MyAppServiceStatus::Failed,
                    format!("启动失败：{}", error),
                )?;
                return Err(error);
            }
        }
        let url = app_open_url(&record);
        match params.target {
            MyAppOpenTarget::Browser => open_default_browser(&url)?,
            MyAppOpenTarget::Codexman => open_codexman_window(app, &record, &url)?,
        }
        Ok(MyAppOperationResponse { ok: true })
    }

    /// 停止所有我的应用受管服务。
    /// 流程：逐个发送停止信号并等待线程退出。
    /// 参数：无。
    /// 返回：停止结果。
    /// 异常/边界：单个线程异常会返回错误，调用方可记录桌面错误。
    pub fn shutdown(&self) -> Result<(), String> {
        let app_ids = {
            let state = self
                .state
                .lock()
                .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
            state.servers.keys().cloned().collect::<Vec<_>>()
        };
        for app_id in app_ids {
            self.stop(&app_id)?;
        }
        Ok(())
    }

    /// 启动单个本地应用服务。
    /// 流程：校验端口和站点目录，绑定 `0.0.0.0`，启动只读 HTTP 线程。
    /// 参数：app 为 Tauri App 句柄，record 为本地应用记录。
    /// 返回：启动结果。
    /// 异常/边界：已在同端口运行时直接视为已启动；不同端口则先停止旧服务后重启。
    fn start_local_app(&self, app: &AppHandle, record: &MyAppRecord) -> Result<(), String> {
        let port = record
            .port
            .ok_or_else(|| "本地托管应用缺少端口。".to_string())?;
        let root = site_root(&record.id, app)?;
        if !root.join("index.html").is_file() {
            return Err("站点目录缺少 index.html，请重新上传 zip 包。".to_string());
        }
        {
            let state = self
                .state
                .lock()
                .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
            if let Some(server) = state.servers.get(&record.id) {
                if server.port == port {
                    drop(state);
                    self.start_frpc(app, record, port)?;
                    return Ok(());
                }
            }
        }
        self.stop(&record.id)?;
        self.set_status(
            &record.id,
            MyAppServiceStatus::Starting,
            "服务启动中。".to_string(),
        )?;
        let listener = TcpListener::bind((SITE_SERVER_HOST, port))
            .map_err(|error| format!("端口 {} 不可用：{}", port, error))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("配置静态服务监听失败：{}", error))?;
        let (shutdown, shutdown_receiver) = mpsc::channel();
        let server_root = root.clone();
        let thread = thread::Builder::new()
            .name(format!("codexman-my-app-{}", record.id))
            .spawn(move || loop {
                if shutdown_receiver.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => handle_static_connection(stream, &server_root),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            })
            .map_err(|error| format!("创建静态服务线程失败：{}", error))?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
        state.servers.insert(
            record.id.clone(),
            ManagedSiteServer {
                shutdown,
                thread,
                port,
            },
        );
        state.statuses.insert(
            record.id.clone(),
            MyAppRuntimeStatus {
                status: MyAppServiceStatus::Running,
                message: "服务已启动。".to_string(),
            },
        );
        drop(state);
        if let Err(error) = self.start_frpc(app, record, port) {
            self.stop(&record.id)?;
            return Err(error);
        }
        if record.public_subdomain.is_some() {
            self.set_status(
                &record.id,
                MyAppServiceStatus::Running,
                "服务已启动，公网访问已连接。".to_string(),
            )?;
        }
        Ok(())
    }

    /// 停止单个受管服务。
    /// 流程：只查找当前 Runtime 持有的线程，发送停止信号并 join。
    /// 参数：app_id 为应用稳定 ID。
    /// 返回：停止结果。
    /// 异常/边界：未启动时视为成功并标记已暂停；不会结束其它进程。
    fn stop(&self, app_id: &str) -> Result<(), String> {
        let managed = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?
            .servers
            .remove(app_id);
        if let Some(managed_server) = managed {
            let _ = managed_server.shutdown.send(());
            managed_server
                .thread
                .join()
                .map_err(|_| "静态服务线程异常退出。".to_string())?;
        }
        self.stop_frpc(app_id)?;
        self.set_status(
            app_id,
            MyAppServiceStatus::Paused,
            "服务已暂停。".to_string(),
        )
    }

    /// 写入单个应用运行状态。
    /// 流程：在运行状态锁内原子替换对应 appId 的状态。
    /// 参数：app_id 为应用 ID，status/message 为新状态。
    /// 返回：写入结果。
    /// 异常/边界：状态写入失败说明锁损坏，调用方应停止继续操作。
    fn set_status(
        &self,
        app_id: &str,
        status: MyAppServiceStatus,
        message: String,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
        state
            .statuses
            .insert(app_id.to_string(), MyAppRuntimeStatus { status, message });
        Ok(())
    }

    /// 停止单个受管 FRP 客户端。
    /// 流程：只终止当前 Runtime 记录的 frpc 子进程，先发送 kill 再等待退出。
    /// 参数：app_id 为应用稳定 ID。
    /// 返回：停止结果。
    /// 异常/边界：未配置或未启动公网访问时视为成功；不会扫描或结束系统中其它 frpc 进程。
    fn stop_frpc(&self, app_id: &str) -> Result<(), String> {
        let managed = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?
            .frpc_clients
            .remove(app_id);
        if let Some(mut managed_client) = managed {
            let _ = managed_client.child.kill();
            managed_client
                .child
                .wait()
                .map_err(|error| format!("停止公网访问客户端失败：{}", error))?;
        }
        Ok(())
    }

    /// 启动单个应用的 FRP 公网访问。
    /// 流程：根据二级域名前缀生成临时 frpc 配置，解析或自动安装 frpc 后启动 HTTP 代理进程。
    /// 参数：app 为 Tauri App 句柄，record 为本地应用记录，port 为本地静态服务端口。
    /// 返回：启动结果。
    /// 异常/边界：未填写二级域名时直接跳过；已用同一二级域名运行时视为成功。
    fn start_frpc(&self, app: &AppHandle, record: &MyAppRecord, port: u16) -> Result<(), String> {
        let Some(subdomain) = record.public_subdomain.as_deref() else {
            return Ok(());
        };
        {
            let state = self
                .state
                .lock()
                .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
            if let Some(client) = state.frpc_clients.get(&record.id) {
                if client.subdomain == subdomain {
                    return Ok(());
                }
            }
        }
        self.stop_frpc(&record.id)?;
        let frpc_path = resolve_frpc_binary(app)?;
        let tunnel_config = FrpHttpTunnelConfig {
            name: &record.id,
            subdomain,
            local_port: port,
        };
        let config_path = write_frpc_config(app, &tunnel_config)?;
        let mut child = Command::new(frpc_path)
            .arg("-c")
            .arg(config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("启动公网访问客户端失败：{}", error))?;
        thread::sleep(Duration::from_millis(350));
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "公网访问客户端启动后退出，状态码：{}。",
                status.code().unwrap_or(-1)
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| "我的应用运行状态锁已损坏".to_string())?;
        state.frpc_clients.insert(
            record.id.clone(),
            ManagedFrpcClient {
                child,
                subdomain: subdomain.to_string(),
            },
        );
        Ok(())
    }
}

impl Drop for RuntimeMyApps {
    /// Runtime 销毁时尽力停止所有站点服务，作为 App RunEvent 清理之外的兜底。
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// 规范化创建应用参数。
/// 流程：按访问方式分别校验端口、URL 和 zip 字段，再生成持久化记录。
/// 参数：id/now 为服务端生成值，params 为 HTTP 参数。
/// 返回：持久化记录。
/// 异常/边界：远程 URL 必须为 http/https；本地应用必须有端口和 zip。
fn normalize_create_params(
    id: &str,
    now: &str,
    params: CreateMyAppParams,
) -> Result<MyAppRecord, String> {
    validate_name(&params.name)?;
    validate_logo_data_url(&params.logo_data_url)?;
    match params.access_type {
        MyAppAccessType::Local => {
            let port = params
                .port
                .ok_or_else(|| "请填写本地服务端口。".to_string())?;
            validate_port(port)?;
            if params
                .zip_data_url
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err("请上传静态页面 zip 包。".to_string());
            }
            Ok(MyAppRecord {
                id: id.to_string(),
                name: params.name.trim().to_string(),
                logo_data_url: params.logo_data_url.trim().to_string(),
                access_type: MyAppAccessType::Local,
                port: Some(port),
                remote_url: None,
                public_subdomain: normalize_public_subdomain(params.public_subdomain.as_deref())?,
                created_at: now.to_string(),
                updated_at: now.to_string(),
            })
        }
        MyAppAccessType::Remote => {
            let remote_url = validate_remote_url(params.remote_url.as_deref().unwrap_or(""))?;
            Ok(MyAppRecord {
                id: id.to_string(),
                name: params.name.trim().to_string(),
                logo_data_url: params.logo_data_url.trim().to_string(),
                access_type: MyAppAccessType::Remote,
                port: None,
                remote_url: Some(remote_url),
                public_subdomain: None,
                created_at: now.to_string(),
                updated_at: now.to_string(),
            })
        }
    }
}

/// 规范化更新应用参数。
/// 流程：保留原创建时间和 ID，只替换用户可编辑字段。
/// 参数：previous 为旧记录，now 为更新时间，params 为 HTTP 参数。
/// 返回：新持久化记录。
/// 异常/边界：从本地改远程会清空端口；从远程改本地需要端口和后续站点校验。
fn normalize_update_params(
    previous: &MyAppRecord,
    now: &str,
    params: UpdateMyAppParams,
) -> Result<MyAppRecord, String> {
    validate_name(&params.name)?;
    validate_logo_data_url(&params.logo_data_url)?;
    match params.access_type {
        MyAppAccessType::Local => {
            let port = params
                .port
                .ok_or_else(|| "请填写本地服务端口。".to_string())?;
            validate_port(port)?;
            Ok(MyAppRecord {
                id: previous.id.clone(),
                name: params.name.trim().to_string(),
                logo_data_url: params.logo_data_url.trim().to_string(),
                access_type: MyAppAccessType::Local,
                port: Some(port),
                remote_url: None,
                public_subdomain: normalize_public_subdomain(params.public_subdomain.as_deref())?,
                created_at: previous.created_at.clone(),
                updated_at: now.to_string(),
            })
        }
        MyAppAccessType::Remote => {
            let remote_url = validate_remote_url(params.remote_url.as_deref().unwrap_or(""))?;
            Ok(MyAppRecord {
                id: previous.id.clone(),
                name: params.name.trim().to_string(),
                logo_data_url: params.logo_data_url.trim().to_string(),
                access_type: MyAppAccessType::Remote,
                port: None,
                remote_url: Some(remote_url),
                public_subdomain: None,
                created_at: previous.created_at.clone(),
                updated_at: now.to_string(),
            })
        }
    }
}

/// 校验应用名称。
/// 流程：去除首尾空白后校验长度。
/// 参数：name 为用户输入名称。
/// 返回：校验结果。
/// 异常/边界：空名称和超长名称均拒绝。
fn validate_name(name: &str) -> Result<(), String> {
    let length = name.trim().chars().count();
    if length == 0 {
        return Err("请填写应用名称。".to_string());
    }
    if length > 80 {
        return Err("应用名称最多 80 个字符。".to_string());
    }
    Ok(())
}

/// 校验 logo data URL。
/// 流程：允许为空；非空时只接受常见图片 data URL。
/// 参数：logo_data_url 为用户上传 logo。
/// 返回：校验结果。
/// 异常/边界：不解码图片内容，只做格式和长度边界。
fn validate_logo_data_url(logo_data_url: &str) -> Result<(), String> {
    let value = logo_data_url.trim();
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > 400_000 {
        return Err("logo 文件过大，请选择更小的图片。".to_string());
    }
    if value.starts_with("data:image/png;base64,")
        || value.starts_with("data:image/jpeg;base64,")
        || value.starts_with("data:image/webp;base64,")
        || value.starts_with("data:image/svg+xml;base64,")
    {
        return Ok(());
    }
    Err("logo 仅支持 png、jpeg、webp 或 svg 图片。".to_string())
}

/// 校验本地服务端口。
/// 流程：限制普通用户端口范围，避免系统保留端口。
/// 参数：port 为用户选择端口。
/// 返回：校验结果。
/// 异常/边界：端口是否占用在启动时再次校验。
fn validate_port(port: u16) -> Result<(), String> {
    if port < 1024 {
        return Err("端口必须大于等于 1024。".to_string());
    }
    Ok(())
}

/// 校验远程 URL。
/// 流程：解析 URL 并只允许 http/https。
/// 参数：url 为用户输入 URL。
/// 返回：规范化 URL 字符串。
/// 异常/边界：不允许 file、javascript 或自定义协议。
fn validate_remote_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    let parsed = url::Url::parse(trimmed).map_err(|_| "请输入合法的网址。".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("远程 URL 仅支持 http 或 https。".to_string());
    }
    Ok(parsed.to_string())
}

/// 规范化公网二级域名前缀。
/// 流程：允许为空；非空时转换为小写并校验 DNS label 字符、长度和首尾字符。
/// 参数：subdomain 为用户填写的二级域名前缀。
/// 返回：规范化后的可选前缀。
/// 异常/边界：只接受单级前缀，不接受点号、协议、根域名或通配符，避免用户绕过服务器侧泛域名约束。
fn normalize_public_subdomain(subdomain: Option<&str>) -> Result<Option<String>, String> {
    let trimmed = subdomain.unwrap_or("").trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let normalized = trimmed.to_ascii_lowercase();
    if normalized.len() > 63 {
        return Err("公网二级域名前缀最多 63 个字符。".to_string());
    }
    if normalized.starts_with('-') || normalized.ends_with('-') {
        return Err("公网二级域名前缀不能以短横线开头或结尾。".to_string());
    }
    let valid = normalized
        .bytes()
        .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-');
    if !valid {
        return Err("公网二级域名仅支持小写字母、数字和短横线。".to_string());
    }
    Ok(Some(normalized))
}

/// 校验端口在记录内唯一。
/// 流程：排除当前编辑记录后检查其它本地应用端口。
/// 参数：records 为现有记录，current_id 为当前编辑 ID，port 为目标端口。
/// 返回：校验结果。
/// 异常/边界：只校验应用配置唯一，真实端口占用由启动校验。
fn ensure_unique_port(
    records: &[MyAppRecord],
    current_id: Option<&str>,
    port: Option<u16>,
) -> Result<(), String> {
    let Some(target_port) = port else {
        return Ok(());
    };
    let duplicated = records.iter().any(|record| {
        record.port == Some(target_port) && current_id.map(|id| id != record.id).unwrap_or(true)
    });
    if duplicated {
        return Err("该端口已被其它我的应用占用。".to_string());
    }
    Ok(())
}

/// 校验公网二级域名前缀在我的应用记录中唯一。
/// 流程：排除当前编辑记录后，比较其它本地应用已配置的二级域名前缀。
/// 参数：records 为现有记录，current_id 为当前编辑 ID，subdomain 为目标前缀。
/// 返回：校验结果。
/// 异常/边界：未配置公网访问时直接通过；远程 URL 应用不会保留该字段。
fn ensure_unique_public_subdomain(
    records: &[MyAppRecord],
    current_id: Option<&str>,
    subdomain: Option<&str>,
) -> Result<(), String> {
    let Some(target_subdomain) = subdomain else {
        return Ok(());
    };
    let duplicated = records.iter().any(|record| {
        record.public_subdomain.as_deref() == Some(target_subdomain)
            && current_id.map(|id| id != record.id).unwrap_or(true)
    });
    if duplicated {
        return Err("该公网二级域名已被其它我的应用占用。".to_string());
    }
    Ok(())
}

/// 探测公网二级域名是否已被远端 frps 占用。
/// 流程：对用户填写的二级域名启动一次短生命周期 frpc 预注册；成功连上后立即停止，重复注册或检测失败则阻止保存。
/// 参数：app 为 Tauri App 句柄，subdomain 为已通过本地格式校验的二级域名前缀。
/// 返回：域名可用时为空。
/// 异常/边界：空域名不探测；探测不会写入应用记录，也不会长期占用域名。
fn ensure_remote_public_subdomain_available(
    app: &AppHandle,
    subdomain: Option<&str>,
) -> Result<(), String> {
    let Some(subdomain) = subdomain else {
        return Ok(());
    };
    let probe_name = format!(
        "my-app-domain-probe-{}-{}",
        subdomain,
        Uuid::new_v4().simple()
    );
    let tunnel_config = FrpHttpTunnelConfig {
        name: &probe_name,
        subdomain,
        local_port: PUBLIC_SUBDOMAIN_PROBE_LOCAL_PORT,
    };
    let frpc_path = resolve_existing_frpc_binary(app).map_err(|error| {
        format!(
            "检测公网二级域名可用性失败：{}请安装内置 frpc 的新版 CodexMan，或配置 AITOOL_FRPC_PATH 后重试。",
            error
        )
    })?;
    let config_path = write_frpc_config(app, &tunnel_config)?;
    let mut child = Command::new(frpc_path)
        .arg("-c")
        .arg(config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("检测公网二级域名可用性失败：{}", error))?;
    thread::sleep(Duration::from_millis(PUBLIC_SUBDOMAIN_PROBE_WAIT_MS));
    let exit_status = child
        .try_wait()
        .map_err(|error| format!("检测公网二级域名可用性失败：{}", error))?;
    if exit_status.is_none() {
        let _ = child.kill();
        child
            .wait()
            .map_err(|error| format!("结束公网二级域名检测客户端失败：{}", error))?;
        let output = read_finished_process_output(&mut child);
        if is_frpc_public_subdomain_conflict_output(&output) {
            return Err(format!(
                "公网二级域名 {} 已被占用，请更换后再保存。",
                public_url_for_subdomain(subdomain)
            ));
        }
        return Ok(());
    }
    if let Some(status) = exit_status {
        let output = read_finished_process_output(&mut child);
        if is_frpc_public_subdomain_conflict_output(&output) {
            return Err(format!(
                "公网二级域名 {} 已被占用，请更换后再保存。",
                public_url_for_subdomain(subdomain)
            ));
        }
        let message = output
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().chars().take(180).collect::<String>())
            .unwrap_or_else(|| format!("frpc 退出状态码 {}", status.code().unwrap_or(-1)));
        return Err(format!("检测公网二级域名可用性失败：{}。", message));
    }
    Ok(())
}

/// 读取已退出 frpc 进程的输出。
/// 流程：分别读取 stdout/stderr 的 UTF-8 文本并拼接为诊断摘要。
/// 参数：child 为已经退出或即将退出的 frpc 子进程。
/// 返回：合并后的输出文本。
/// 异常/边界：读取失败时忽略对应流，避免遮蔽原始启动失败状态。
fn read_finished_process_output(child: &mut Child) -> String {
    let mut output = String::new();
    if let Some(stdout) = child.stdout.as_mut() {
        let _ = stdout.read_to_string(&mut output);
    }
    if let Some(stderr) = child.stderr.as_mut() {
        let _ = stderr.read_to_string(&mut output);
    }
    output
}

/// 判断 frpc 输出是否表示远端二级域名冲突。
/// 流程：兼容 frp 不同版本常见重复代理、重复 custom domain、重复 subdomain 文案。
/// 参数：output 为 frpc 启动失败输出。
/// 返回：命中占用语义时 true。
/// 异常/边界：其它网络、认证、配置错误不伪装成已占用，交给调用方展示检测失败。
fn is_frpc_public_subdomain_conflict_output(output: &str) -> bool {
    let normalized = output.to_ascii_lowercase();
    (normalized.contains("already")
        || normalized.contains("duplicate")
        || normalized.contains("repeated")
        || normalized.contains("conflict"))
        && (normalized.contains("subdomain")
            || normalized.contains("custom domain")
            || normalized.contains("domain")
            || normalized.contains("proxy"))
}

/// 判断端口是否可绑定。
/// 流程：尝试绑定 `0.0.0.0:port` 并立即释放。
/// 参数：port 为待检测端口。
/// 返回：可绑定时 true。
/// 异常/边界：检测和实际启动之间可能被其它进程抢占，启动时仍需处理失败。
fn is_port_available(port: u16) -> bool {
    TcpListener::bind((SITE_SERVER_HOST, port)).is_ok()
}

/// 解码并解压站点 zip。
/// 流程：把 data URL 解码到内存，安全解压到临时目录，定位 index.html 后替换正式站点目录。
/// 参数：app 为 Tauri App 句柄，app_id 为应用 ID，zip_data_url 为上传包。
/// 返回：解压结果。
/// 异常/边界：拒绝路径穿越、绝对路径、空包和缺少 index.html 的包。
fn extract_site_zip(app: &AppHandle, app_id: &str, zip_data_url: &str) -> Result<(), String> {
    let zip_bytes = decode_data_url(zip_data_url, "application/zip")?;
    let mut archive =
        ZipArchive::new(Cursor::new(zip_bytes)).map_err(|_| "zip 包格式无效。".to_string())?;
    let temp_dir = site_root(app_id, app)?.with_extension("tmp");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|error| format!("清理临时站点目录失败：{}", error))?;
    }
    fs::create_dir_all(&temp_dir).map_err(|error| format!("创建临时站点目录失败：{}", error))?;
    let mut extracted_count = 0_usize;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| "读取 zip 包内容失败。".to_string())?;
        let Some(safe_name) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err("zip 包包含不安全路径。".to_string());
        };
        if safe_name.as_os_str().is_empty() {
            continue;
        }
        let output_path = temp_dir.join(safe_name);
        if file.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(|error| format!("创建解压目录失败：{}", error))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建解压父目录失败：{}", error))?;
        }
        let mut output =
            File::create(&output_path).map_err(|error| format!("写入解压文件失败：{}", error))?;
        std::io::copy(&mut file, &mut output)
            .map_err(|error| format!("解压文件失败：{}", error))?;
        extracted_count += 1;
    }
    if extracted_count == 0 {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err("zip 包内没有可托管的文件。".to_string());
    }
    let content_root = resolve_extracted_site_root(&temp_dir)?;
    let final_root = site_root(app_id, app)?;
    if final_root.exists() {
        fs::remove_dir_all(&final_root)
            .map_err(|error| format!("替换旧站点目录失败：{}", error))?;
    }
    if content_root == temp_dir {
        fs::rename(&temp_dir, &final_root)
            .map_err(|error| format!("保存站点目录失败：{}", error))?;
    } else {
        fs::create_dir_all(&final_root).map_err(|error| format!("创建站点目录失败：{}", error))?;
        copy_dir_all(&content_root, &final_root)?;
        fs::remove_dir_all(&temp_dir)
            .map_err(|error| format!("清理临时站点目录失败：{}", error))?;
    }
    Ok(())
}

/// 解码 data URL。
/// 流程：兼容 `application/zip` 与 `application/x-zip-compressed` 两类 MIME。
/// 参数：data_url 为上传内容，expected_mime 为期望 MIME。
/// 返回：解码字节。
/// 异常/边界：只接受 base64 data URL，避免前端传入任意路径。
fn decode_data_url(data_url: &str, expected_mime: &str) -> Result<Vec<u8>, String> {
    let trimmed = data_url.trim();
    let marker = ";base64,";
    let marker_index = trimmed
        .find(marker)
        .ok_or_else(|| "上传文件格式无效。".to_string())?;
    if !trimmed.starts_with("data:") || marker_index <= "data:".len() {
        return Err("上传文件格式无效。".to_string());
    }
    let mime = &trimmed["data:".len()..marker_index];
    if expected_mime == "application/zip"
        && !matches!(
            mime,
            "application/zip" | "application/x-zip-compressed" | "application/octet-stream"
        )
    {
        return Err("请上传 zip 压缩包。".to_string());
    }
    let payload = &trimmed[marker_index + marker.len()..];
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "上传文件 base64 内容无效。".to_string())
}

/// 定位解压后的站点根目录。
/// 流程：优先使用根目录 index.html；否则忽略系统附加目录后，查找第一层唯一目录中的 index.html。
/// 参数：temp_dir 为解压临时目录。
/// 返回：应该作为站点根的目录。
/// 异常/边界：macOS 压缩生成的 __MACOSX/.DS_Store 不参与根目录判断；多层或无 index.html 时拒绝，让用户重新打包。
fn resolve_extracted_site_root(temp_dir: &Path) -> Result<PathBuf, String> {
    if temp_dir.join("index.html").is_file() {
        return Ok(temp_dir.to_path_buf());
    }
    let mut first_level_dirs = Vec::new();
    for entry in fs::read_dir(temp_dir).map_err(|error| format!("读取解压目录失败：{}", error))?
    {
        let entry = entry.map_err(|error| format!("读取解压目录项失败：{}", error))?;
        let path = entry.path();
        if is_ignorable_extracted_entry(&path) {
            continue;
        }
        if path.is_dir() {
            first_level_dirs.push(path);
        }
    }
    if first_level_dirs.len() == 1 && first_level_dirs[0].join("index.html").is_file() {
        return Ok(first_level_dirs.remove(0));
    }
    Err("zip 包根目录或第一层目录必须包含 index.html。".to_string())
}

/// 判断解压根目录下是否为系统自动生成的可忽略项目。
/// 流程：读取文件名，过滤 macOS 压缩包常见的资源目录和隐藏元数据。
/// 参数：path 为解压根目录的第一层路径。
/// 返回：应从站点根目录判断中忽略时为 true。
/// 异常/边界：仅用于根目录候选过滤，不删除真实站点文件，避免误伤用户资源。
fn is_ignorable_extracted_entry(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name == "__MACOSX" || name == ".DS_Store" || name.starts_with("._")
}

/// 递归复制目录。
/// 流程：遍历源目录，按相对结构复制到目标目录。
/// 参数：from/to 为源目录和目标目录。
/// 返回：复制结果。
/// 异常/边界：只用于已安全解压的临时目录。
fn copy_dir_all(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| format!("创建目录失败：{}", error))?;
    for entry in fs::read_dir(from).map_err(|error| format!("读取目录失败：{}", error))? {
        let entry = entry.map_err(|error| format!("读取目录项失败：{}", error))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_all(&source, &target)?;
        } else {
            fs::copy(&source, &target).map_err(|error| format!("复制文件失败：{}", error))?;
        }
    }
    Ok(())
}

/// 处理静态资源连接。
/// 流程：读取请求首行，解析 GET/HEAD 路径并返回站点目录内文件。
/// 参数：stream 为 TCP 连接，root 为站点根目录。
/// 返回：无。
/// 异常/边界：路径逃逸返回 403；缺失文件回退 index.html 支持前端路由。
fn handle_static_connection(mut stream: TcpStream, root: &Path) {
    let mut buffer = [0_u8; STATIC_REQUEST_MAX_BYTES];
    let read_bytes = match stream.read(&mut buffer) {
        Ok(read_bytes) => read_bytes,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buffer[..read_bytes]);
    let Some(pathname) = parse_request_path(&request) else {
        write_static_response(
            &mut stream,
            400,
            "text/plain; charset=utf-8",
            b"Bad Request",
        );
        return;
    };
    serve_static_path(&mut stream, root, &pathname);
}

/// 解析静态服务请求路径。
/// 流程：只接受 GET/HEAD，去掉 query/hash，并百分号解码。
/// 参数：request 为原始 HTTP 请求片段。
/// 返回：URL path。
/// 异常/边界：不支持写请求，避免站点服务被误用为上传入口。
fn parse_request_path(request: &str) -> Option<String> {
    let request_line = request.lines().next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    let version = parts.next()?;
    if !matches!(method, "GET" | "HEAD") || !version.starts_with("HTTP/") {
        return None;
    }
    let path = target
        .split(['?', '#'])
        .next()
        .filter(|value| value.starts_with('/'))?;
    Some(percent_decode_path(path))
}

/// 解码 URL path 百分号编码。
/// 流程：逐字节处理 `%XX`，非法编码保持原样。
/// 参数：path 为 URL path。
/// 返回：尽力解码后的路径。
/// 异常/边界：不把非法编码映射成其它字符，避免路径歧义。
fn percent_decode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&path[index + 1..index + 3], 16) {
                output.push(value);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

/// 返回静态路径。
/// 流程：规范化目标路径，确认位于站点根目录内，再返回文件或回退 index.html。
/// 参数：stream 为连接，root 为站点根，pathname 为请求路径。
/// 返回：无。
/// 异常/边界：目录请求回退 index.html，支持 SPA 路由。
fn serve_static_path(stream: &mut TcpStream, root: &Path, pathname: &str) {
    let relative_path = pathname.trim_start_matches('/');
    let requested_path = if relative_path.is_empty() {
        root.join("index.html")
    } else {
        root.join(relative_path)
    };
    let root_path = match root.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            write_static_response(stream, 404, "text/plain; charset=utf-8", b"Not Found");
            return;
        }
    };
    let file_path = match requested_path.canonicalize() {
        Ok(path) if path.starts_with(&root_path) && path.is_file() => path,
        Ok(_) => {
            write_static_response(stream, 403, "text/plain; charset=utf-8", b"Forbidden");
            return;
        }
        Err(_) => root_path.join("index.html"),
    };
    write_static_file(stream, &file_path);
}

/// 写入静态文件响应。
/// 流程：读取完整文件并按扩展名返回 Content-Type。
/// 参数：stream 为连接，path 为文件路径。
/// 返回：无。
/// 异常/边界：文件读取失败返回 404 或 500。
fn write_static_file(stream: &mut TcpStream, path: &Path) {
    match File::open(path) {
        Ok(mut file) => {
            let mut content = Vec::new();
            if file.read_to_end(&mut content).is_err() {
                write_static_response(
                    stream,
                    500,
                    "text/plain; charset=utf-8",
                    b"Internal Server Error",
                );
                return;
            }
            write_static_response(stream, 200, mime_by_path(path), &content);
        }
        Err(_) => write_static_response(stream, 404, "text/plain; charset=utf-8", b"Not Found"),
    }
}

/// 写入静态 HTTP 响应。
/// 流程：组装状态行、CORS、Content-Type、Content-Length 和关闭连接头。
/// 参数：stream 为连接，status/content_type/body 为响应内容。
/// 返回：无。
/// 异常/边界：写失败直接结束连接，不影响服务线程。
fn write_static_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        status_message(status),
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

/// 根据文件扩展名返回 MIME。
/// 流程：覆盖 Vue/Vite 静态产物常见类型。
/// 参数：path 为文件路径。
/// 返回：Content-Type。
/// 异常/边界：未知扩展名使用二进制流。
fn mime_by_path(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// 返回 HTTP 状态文案。
/// 流程：只映射当前静态服务会返回的状态码。
/// 参数：status 为状态码。
/// 返回：reason phrase。
/// 异常/边界：未知状态统一 Unknown。
fn status_message(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// 打开 CodexMan 应用窗口。
/// 流程：同一应用复用同 label 窗口；不存在时新建保留头部的普通窗口。
/// 参数：app 为 Tauri App 句柄，record 为应用记录，url 为目标 URL。
/// 返回：打开结果。
/// 异常/边界：URL 非法或窗口创建失败时返回明确错误。
fn open_codexman_window(app: &AppHandle, record: &MyAppRecord, url: &str) -> Result<(), String> {
    let label = format!("my-app-{}", record.id);
    let parsed_url = url::Url::parse(url).map_err(|_| "应用访问地址无效。".to_string())?;
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.navigate(parsed_url.clone());
        window
            .show()
            .map_err(|error| format!("显示应用窗口失败：{}", error))?;
        window
            .set_focus()
            .map_err(|error| format!("聚焦应用窗口失败：{}", error))?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed_url))
        .title(&record.name)
        .inner_size(1180.0, 760.0)
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|error| format!("打开 CodexMan 应用窗口失败：{}", error))?;
    Ok(())
}

/// 使用默认浏览器打开 URL。
/// 流程：macOS 使用系统 `open`，其它平台使用 opener 插件命令兜底前的系统命令。
/// 参数：url 为目标地址。
/// 返回：打开结果。
/// 异常/边界：不拼接 shell 字符串，避免 URL 注入命令行。
fn open_default_browser(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|error| format!("使用默认浏览器打开失败：{}", error))?;
    Ok(())
}

/// 构建应用响应。
/// 流程：根据持久化记录和运行状态补齐访问地址。
/// 参数：record 为应用记录，statuses 为运行状态快照，lan_ip 为局域网 IP。
/// 返回：前端列表项。
/// 异常/边界：远程 URL 不展示本地地址。
fn build_app_response(
    record: &MyAppRecord,
    statuses: &HashMap<String, MyAppRuntimeStatus>,
    lan_ip: &str,
) -> MyAppResponse {
    let fallback_status = if record.access_type == MyAppAccessType::Local {
        MyAppRuntimeStatus {
            status: MyAppServiceStatus::Paused,
            message: "服务未启动。".to_string(),
        }
    } else {
        MyAppRuntimeStatus {
            status: MyAppServiceStatus::Unavailable,
            message: "远程 URL 无本地服务。".to_string(),
        }
    };
    let status = statuses.get(&record.id).cloned().unwrap_or(fallback_status);
    let local_url = record
        .port
        .map(|port| format!("http://127.0.0.1:{}", port))
        .unwrap_or_default();
    let lan_url = record
        .port
        .map(|port| format!("http://{}:{}", lan_ip, port))
        .unwrap_or_default();
    let public_url = record
        .public_subdomain
        .as_deref()
        .filter(|_| record.access_type == MyAppAccessType::Local)
        .map(public_url_for_subdomain)
        .unwrap_or_default();
    MyAppResponse {
        id: record.id.clone(),
        name: record.name.clone(),
        logo_data_url: record.logo_data_url.clone(),
        access_type: record.access_type.clone(),
        port: record.port,
        remote_url: record.remote_url.clone(),
        local_url: local_url.clone(),
        lan_url,
        public_url,
        public_subdomain: record.public_subdomain.clone(),
        open_url: app_open_url(record),
        service_status: status.status,
        service_message: status.message,
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
    }
}

/// 读取应用默认打开地址。
/// 流程：本地应用使用 127.0.0.1 地址；远程应用使用用户 URL。
/// 参数：record 为应用记录。
/// 返回：URL 字符串。
/// 异常/边界：非法记录返回空字符串，由打开阶段校验。
fn app_open_url(record: &MyAppRecord) -> String {
    match record.access_type {
        MyAppAccessType::Local => record
            .port
            .map(|port| format!("http://127.0.0.1:{}", port))
            .unwrap_or_default(),
        MyAppAccessType::Remote => record.remote_url.clone().unwrap_or_default(),
    }
}

/// 解析局域网 IP。
/// 流程：通过 UDP 路由探测当前默认出网网卡地址；失败时回退 127.0.0.1。
/// 参数：无。
/// 返回：IP 字符串。
/// 异常/边界：不发送真实 UDP 包，只让系统选择路由。
fn resolve_lan_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            socket.local_addr()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

/// 确保我的应用目录存在。
/// 流程：创建配置文件父目录和站点根目录。
/// 参数：app 为 Tauri App 句柄。
/// 返回：目录准备结果。
/// 异常/边界：目录创建失败会阻止后续文件操作。
fn ensure_my_apps_dirs(app: &AppHandle) -> Result<(), String> {
    fs::create_dir_all(my_apps_data_dir(app)?)
        .map_err(|error| format!("创建我的应用数据目录失败：{}", error))?;
    fs::create_dir_all(my_apps_sites_dir(app)?)
        .map_err(|error| format!("创建我的应用站点目录失败：{}", error))
}

/// 读取我的应用数据目录。
/// 流程：基于 Tauri app_data_dir 追加 my-apps 目录。
/// 参数：app 为 Tauri App 句柄。
/// 返回：数据目录。
/// 异常/边界：无法读取 App 数据目录时返回错误。
fn my_apps_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("读取 App 数据目录失败：{}", error))
        .map(|path| path.join("my-apps"))
}

/// 读取我的应用配置文件路径。
/// 流程：拼接数据目录和固定文件名。
/// 参数：app 为 Tauri App 句柄。
/// 返回：配置文件路径。
/// 异常/边界：无法读取数据目录时返回错误。
fn my_apps_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(my_apps_data_dir(app)?.join(MY_APPS_FILE_NAME))
}

/// 读取我的应用站点目录。
/// 流程：拼接数据目录和固定站点目录。
/// 参数：app 为 Tauri App 句柄。
/// 返回：站点根目录。
/// 异常/边界：无法读取数据目录时返回错误。
fn my_apps_sites_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(my_apps_data_dir(app)?.join(MY_APPS_SITES_DIR_NAME))
}

/// 读取单个应用站点根目录。
/// 流程：在站点目录下按 appId 隔离。
/// 参数：app_id 为应用 ID，app 为 Tauri App 句柄。
/// 返回：站点目录。
/// 异常/边界：appId 由服务端生成，禁止用户路径片段。
fn site_root(app_id: &str, app: &AppHandle) -> Result<PathBuf, String> {
    Ok(my_apps_sites_dir(app)?.join(app_id))
}

/// 删除单个应用站点目录。
/// 流程：仅删除 appData/my-apps/my-app-sites/appId。
/// 参数：app 为 Tauri App 句柄，app_id 为应用 ID。
/// 返回：删除结果。
/// 异常/边界：目录不存在视为成功；不会递归删除站点根之外路径。
fn remove_site_dir(app: &AppHandle, app_id: &str) -> Result<(), String> {
    let root = site_root(app_id, app)?;
    if root.exists() {
        fs::remove_dir_all(root).map_err(|error| format!("删除站点目录失败：{}", error))?;
    }
    Ok(())
}

/// 读取持久化文档。
/// 流程：文件不存在时返回空文档；存在时解析 JSON。
/// 参数：app 为 Tauri App 句柄。
/// 返回：持久化文档。
/// 异常/边界：解析失败会显式返回，避免覆盖用户数据。
fn load_document(app: &AppHandle) -> Result<MyAppsDocument, String> {
    ensure_my_apps_dirs(app)?;
    let path = my_apps_file(app)?;
    if !path.exists() {
        return Ok(MyAppsDocument {
            version: 1,
            apps: Vec::new(),
        });
    }
    let content =
        fs::read_to_string(path).map_err(|error| format!("读取我的应用配置失败：{}", error))?;
    serde_json::from_str::<MyAppsDocument>(&content)
        .map_err(|error| format!("解析我的应用配置失败：{}", error))
}

/// 保存持久化文档。
/// 流程：写入临时文件后 rename 替换，减少崩溃半写风险。
/// 参数：app 为 Tauri App 句柄，document 为待保存文档。
/// 返回：保存结果。
/// 异常/边界：保存失败不修改现有配置文件。
fn save_document(app: &AppHandle, document: &MyAppsDocument) -> Result<(), String> {
    ensure_my_apps_dirs(app)?;
    let path = my_apps_file(app)?;
    let temp_path = path.with_extension("tmp");
    let content = serde_json::to_string_pretty(document)
        .map_err(|error| format!("编码我的应用配置失败：{}", error))?;
    fs::write(&temp_path, content).map_err(|error| format!("写入我的应用配置失败：{}", error))?;
    fs::rename(temp_path, path).map_err(|error| format!("保存我的应用配置失败：{}", error))
}

/// 返回当前时间字符串。
/// 流程：使用 UTC 秒级 Unix 时间构造稳定 ISO 字符串。
/// 参数：无。
/// 返回：UTC ISO 字符串。
/// 异常/边界：系统时间异常时返回 epoch。
fn now_iso_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds as i64, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建当前测试专用的临时目录。
    /// 流程：使用系统临时目录和 UUID 生成隔离目录，避免并发测试冲突。
    /// 参数：无。
    /// 返回：已创建的临时目录路径。
    /// 异常/边界：目录创建失败时直接 panic，使测试明确暴露环境问题。
    fn create_test_temp_dir() -> PathBuf {
        let temp_dir =
            std::env::temp_dir().join(format!("ai-tool-my-app-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(&temp_dir).expect("测试临时目录必须创建成功");
        temp_dir
    }

    /// 验证 macOS 压缩包附加目录不会影响 dist 根目录识别。
    /// 流程：构造 dist/index.html 与 __MACOSX 同级目录，再解析站点根目录。
    /// 参数：无。
    /// 返回：无。
    /// 异常/边界：测试结束后清理临时目录，避免污染本机临时文件。
    #[test]
    fn extracted_site_root_ignores_macos_metadata_directory() {
        let temp_dir = create_test_temp_dir();
        let dist_dir = temp_dir.join("dist");
        fs::create_dir_all(&dist_dir).expect("dist 目录必须创建成功");
        fs::write(dist_dir.join("index.html"), "<!doctype html>").expect("index.html 必须写入成功");
        fs::create_dir_all(temp_dir.join("__MACOSX")).expect("__MACOSX 目录必须创建成功");
        fs::write(temp_dir.join(".DS_Store"), "").expect(".DS_Store 必须写入成功");

        let resolved = resolve_extracted_site_root(&temp_dir).expect("必须识别 dist 为站点根");
        assert_eq!(resolved, dist_dir);

        fs::remove_dir_all(temp_dir).expect("测试临时目录必须清理成功");
    }

    /// 验证 frpc 重复域名输出能被识别为占用冲突。
    /// 流程：分别传入重复 subdomain、普通网络失败和空输出，确认只把重复域名类失败映射为占用。
    /// 参数：无。
    /// 返回：无。
    /// 异常/边界：不启动真实 frpc，避免单测依赖公网服务。
    #[test]
    fn frpc_public_subdomain_conflict_output_is_detected() {
        assert!(is_frpc_public_subdomain_conflict_output(
            "proxy [codexman-demo] start error: subdomain demo is already registered"
        ));
        assert!(is_frpc_public_subdomain_conflict_output(
            "custom domain demo.tolern.com already exists"
        ));
        assert!(is_frpc_public_subdomain_conflict_output(
            "[codexman-demo] proxy added\n[codexman-demo] start error: router config conflict"
        ));
        assert!(!is_frpc_public_subdomain_conflict_output(
            "login to server failed: dial tcp i/o timeout"
        ));
        assert!(!is_frpc_public_subdomain_conflict_output(""));
    }
}
