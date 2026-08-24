use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Manager};

/// 公网访问根域名；服务器侧宝塔已将 `*.tolern.com` 反代到 frps HTTP/HTTPS 虚拟主机入口。
pub const PUBLIC_DOMAIN_ROOT: &str = "tolern.com";
/// 公网访问协议；当前泛域名证书已配置到服务器侧 Nginx。
const PUBLIC_DOMAIN_SCHEME: &str = "https";
/// FRP 服务端地址。
const FRP_SERVER_ADDR: &str = "106.55.11.117";
/// FRP 服务端控制端口。
const FRP_SERVER_PORT: u16 = 7000;
/// FRP 客户端认证 token；仅用于本机 frpc 连接服务器侧 frps。
const FRP_AUTH_TOKEN: &str = "adb0dc05760dc247f842953ee5434813a418949c7b9effa1";
/// FRP 客户端版本；必须与自动下载 URL 中的发行包版本保持一致。
const FRP_VERSION: &str = "0.61.0";
/// macOS arm64 FRP 发行包目录名。
const FRP_DARWIN_ARM64_DIR: &str = "frp_0.61.0_darwin_arm64";
/// macOS amd64 FRP 发行包目录名。
const FRP_DARWIN_AMD64_DIR: &str = "frp_0.61.0_darwin_amd64";
/// 自动下载 frpc 的连接超时时间；避免保存或启动流程长时间卡在网络连接阶段。
const FRPC_DOWNLOAD_CONNECT_TIMEOUT_SECONDS: &str = "5";
/// 自动下载 frpc 的总超时时间；失败时快速返回，让用户能继续处理本地应用。
const FRPC_DOWNLOAD_MAX_TIME_SECONDS: &str = "12";

/// 单个 FRP HTTP 隧道配置。
pub struct FrpHttpTunnelConfig<'a> {
    /// 代理名称，用于 frps 侧标识当前连接。
    pub name: &'a str,
    /// 二级域名前缀。
    pub subdomain: &'a str,
    /// 本地监听端口。
    pub local_port: u16,
}

/// 根据二级域名前缀生成公网 URL。
/// 流程：使用当前服务器侧已配置的 HTTPS 泛域名证书和根域名拼接完整地址。
/// 参数：subdomain 为已通过 DNS label 校验的二级域名前缀。
/// 返回：公网访问地址。
/// 异常/边界：不附加路径，调用方按业务自行补充路径。
pub fn public_url_for_subdomain(subdomain: &str) -> String {
    format!(
        "{}://{}.{}",
        PUBLIC_DOMAIN_SCHEME, subdomain, PUBLIC_DOMAIN_ROOT
    )
}

/// 读取 FRP 客户端工作目录。
/// 流程：放在 App 数据目录下，避免污染用户项目目录。
/// 参数：app 为 Tauri App 句柄。
/// 返回：frpc 配置、下载包和解压目录所在目录。
/// 异常/边界：无法读取 App 数据目录时返回错误。
pub fn frpc_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("读取 App 数据目录失败：{}", error))
        .map(|path| path.join("frpc"))
}

/// 写入 frpc HTTP 代理配置。
/// 流程：根据配置名称、二级域名前缀和本地端口生成最小 HTTP 代理配置。
/// 参数：app 为 Tauri App 句柄，config 为隧道配置。
/// 返回：配置文件路径。
/// 异常/边界：配置只写入 App 数据目录，不包含用户业务内容。
pub fn write_frpc_config(
    app: &AppHandle,
    config: &FrpHttpTunnelConfig<'_>,
) -> Result<PathBuf, String> {
    let dir = frpc_data_dir(app)?.join("configs");
    fs::create_dir_all(&dir).map_err(|error| format!("创建公网访问配置目录失败：{}", error))?;
    let config_path = dir.join(format!("{}.toml", config.name));
    let content = format!(
        r#"serverAddr = "{server_addr}"
serverPort = {server_port}

auth.method = "token"
auth.token = "{token}"

[[proxies]]
name = "codexman-{name}"
type = "http"
localIP = "127.0.0.1"
localPort = {port}
subdomain = "{subdomain}"
"#,
        server_addr = FRP_SERVER_ADDR,
        server_port = FRP_SERVER_PORT,
        token = FRP_AUTH_TOKEN,
        name = config.name,
        port = config.local_port,
        subdomain = config.subdomain
    );
    fs::write(&config_path, content).map_err(|error| format!("写入公网访问配置失败：{}", error))?;
    Ok(config_path)
}

/// 解析当前已经存在的 frpc 二进制路径。
/// 流程：依次检查环境变量、App 数据目录自动安装产物、Tauri 资源目录和 PATH，不触发任何网络下载。
/// 参数：app 为 Tauri App 句柄。
/// 返回：可执行 frpc 路径。
/// 异常/边界：用于保存前预检等同步路径，避免公网访问客户端缺失时把用户操作卡在下载阶段。
pub fn resolve_existing_frpc_binary(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("AITOOL_FRPC_PATH") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    if let Ok(dir_name) = frpc_release_dir_name() {
        let managed_candidate = frpc_data_dir(app)?.join(dir_name).join("frpc");
        if managed_candidate.is_file() {
            return Ok(managed_candidate);
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        let resource_candidate = resource_dir.join("frpc");
        if resource_candidate.is_file() {
            return Ok(resource_candidate);
        }
    }
    if let Some(path_candidate) = find_command_in_path("frpc") {
        return Ok(path_candidate);
    }
    Err("本机缺少公网访问客户端 frpc。".to_string())
}

/// 解析可用的 frpc 二进制路径。
/// 流程：先复用已有 frpc；都不存在时尝试自动下载安装到 App 数据目录。
/// 参数：app 为 Tauri App 句柄。
/// 返回：可执行 frpc 路径。
/// 异常/边界：当前自动安装仅覆盖 macOS arm64/x64；其它平台需要通过 AITOOL_FRPC_PATH 或 PATH 提供 frpc。
pub fn resolve_frpc_binary(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(path) = resolve_existing_frpc_binary(app) {
        return Ok(path);
    }
    install_frpc_binary(app)
}

/// 自动安装 frpc 到 App 数据目录。
/// 流程：使用系统 curl 下载官方发行包，再用系统 tar 解压；成功后返回解压出的 frpc 路径。
/// 参数：app 为 Tauri App 句柄。
/// 返回：可执行 frpc 路径。
/// 异常/边界：不使用 shell 拼接命令；下载失败时返回用户可读错误，应用仍可通过本地地址访问。
fn install_frpc_binary(app: &AppHandle) -> Result<PathBuf, String> {
    let dir_name = frpc_release_dir_name()?;
    let base_dir = frpc_data_dir(app)?;
    fs::create_dir_all(&base_dir)
        .map_err(|error| format!("创建公网访问客户端目录失败：{}", error))?;
    let archive_path = base_dir.join(format!("{}.tar.gz", dir_name));
    let url = format!(
        "https://github.com/fatedier/frp/releases/download/v{}/{}.tar.gz",
        FRP_VERSION, dir_name
    );
    let curl_status = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--connect-timeout")
        .arg(FRPC_DOWNLOAD_CONNECT_TIMEOUT_SECONDS)
        .arg("--max-time")
        .arg(FRPC_DOWNLOAD_MAX_TIME_SECONDS)
        .arg("-o")
        .arg(&archive_path)
        .arg(url)
        .status()
        .map_err(|error| format!("下载公网访问客户端失败：{}", error))?;
    if !curl_status.success() {
        return Err("下载公网访问客户端失败，请检查网络或手动配置 AITOOL_FRPC_PATH。".to_string());
    }
    let tar_status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&base_dir)
        .status()
        .map_err(|error| format!("解压公网访问客户端失败：{}", error))?;
    if !tar_status.success() {
        return Err("解压公网访问客户端失败，请手动配置 AITOOL_FRPC_PATH。".to_string());
    }
    let frpc_path = base_dir.join(dir_name).join("frpc");
    if !frpc_path.is_file() {
        return Err("公网访问客户端安装后缺少 frpc 可执行文件。".to_string());
    }
    Ok(frpc_path)
}

/// 读取当前平台对应的 FRP 发行包目录名。
/// 流程：根据 Rust 编译目标系统和架构匹配官方 macOS 包名。
/// 参数：无。
/// 返回：发行包目录名。
/// 异常/边界：非 macOS 平台暂不自动下载，调用方可通过环境变量或 PATH 提供 frpc。
fn frpc_release_dir_name() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(FRP_DARWIN_ARM64_DIR),
        ("macos", "x86_64") => Ok(FRP_DARWIN_AMD64_DIR),
        _ => Err("当前平台暂不支持自动安装公网访问客户端，请配置 AITOOL_FRPC_PATH。".to_string()),
    }
}

/// 在 PATH 中查找命令。
/// 流程：读取 PATH 并逐个目录拼接命令名。
/// 参数：command 为命令名。
/// 返回：存在普通文件时返回路径。
/// 异常/边界：不检查可执行权限，最终启动阶段仍会给出系统错误。
fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_value = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_value) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
