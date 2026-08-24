use std::fs::{self, File};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use rand::Rng;
use serde::Serialize;
use tauri::AppHandle;

use crate::frp_tunnel::{
    frpc_data_dir, public_url_for_subdomain, resolve_frpc_binary, write_frpc_config,
    FrpHttpTunnelConfig,
};

/// 公共 HTTP API 本地服务端口；必须与 sidecar 暴露的 `127.0.0.1:18080` 保持一致。
const PUBLIC_API_LOCAL_PORT: u16 = 18_080;
/// 公共 HTTP API 隧道固定配置名，用于生成 frpc 配置文件和代理名称。
const PUBLIC_API_TUNNEL_NAME: &str = "public-api";
/// 公共 HTTP API 随机兜底二级域名前缀长度。
const PUBLIC_API_RANDOM_SUBDOMAIN_LENGTH: usize = 6;
/// 公共 HTTP API 随机兜底二级域名字符集；只使用 DNS label 安全的小写字母和数字。
const PUBLIC_API_SUBDOMAIN_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
/// 公共 HTTP API 用户英文名域名前缀最大长度。
const PUBLIC_API_NAMED_SUBDOMAIN_MAX_LENGTH: usize = 32;
/// 公共 HTTP API frpc 启动后读取冲突输出的等待时间。
const PUBLIC_API_FRPC_START_WAIT_MS: u64 = 700;

/// 公共 HTTP API 外网访问运行时。
#[derive(Default)]
pub struct RuntimePublicApiTunnel {
    /// 当前 CodexMan 自己启动的 frpc 进程；只管理这一条 API 隧道。
    state: Mutex<Option<ManagedPublicApiFrpcClient>>,
}

/// 公共 HTTP API 受管 frpc 进程。
struct ManagedPublicApiFrpcClient {
    /// frpc 子进程句柄；停止外网访问时只终止当前记录的进程。
    child: Child,
    /// 当前已注册到 frps 的二级域名前缀。
    subdomain: String,
}

/// 公共 HTTP API 外网访问状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiTunnelStatus {
    /// 是否允许外网访问。
    pub enabled: bool,
    /// 固定二级域名前缀；关闭且从未生成时为空。
    pub subdomain: Option<String>,
    /// 完整远程访问地址；未生成时为空。
    pub public_url: Option<String>,
    /// 当前 frpc 是否正在运行。
    pub running: bool,
}

impl RuntimePublicApiTunnel {
    /// 启动公共 HTTP API 外网访问隧道。
    /// 流程：为固定本机端口写入 frpc 配置，解析或自动安装 frpc 后启动 HTTP 代理进程，并读取早期输出识别远端域名冲突。
    /// 参数：app 为 Tauri 句柄，subdomain 为已校验的二级域名前缀。
    /// 返回：最新运行状态。
    /// 异常/边界：同一二级域名已运行时保持幂等；不同域名会先停止旧进程再启动。
    pub fn start(&self, app: &AppHandle, subdomain: &str) -> Result<PublicApiTunnelStatus, String> {
        let normalized_subdomain = validate_public_api_subdomain(subdomain)?;
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "公共 HTTP 外网访问状态锁已损坏".to_string())?;
            if let Some(client) = state.as_mut() {
                if client.subdomain == normalized_subdomain {
                    if client
                        .child
                        .try_wait()
                        .map_err(|error| format!("检查公网访问客户端失败：{}", error))?
                        .is_none()
                    {
                        return Ok(status_from_subdomain(&normalized_subdomain, true, true));
                    }
                }
            }
        }
        self.stop()?;
        let frpc_path = resolve_frpc_binary(app)?;
        let tunnel_config = FrpHttpTunnelConfig {
            name: PUBLIC_API_TUNNEL_NAME,
            subdomain: &normalized_subdomain,
            local_port: PUBLIC_API_LOCAL_PORT,
        };
        let config_path = write_frpc_config(app, &tunnel_config)?;
        let log_dir = frpc_data_dir(app)?.join("logs");
        fs::create_dir_all(&log_dir)
            .map_err(|error| format!("创建公网访问日志目录失败：{}", error))?;
        let stdout_path = log_dir.join(format!("{}-stdout.log", PUBLIC_API_TUNNEL_NAME));
        let stderr_path = log_dir.join(format!("{}-stderr.log", PUBLIC_API_TUNNEL_NAME));
        let stdout_file = File::create(&stdout_path)
            .map_err(|error| format!("创建 HTTP API 公网访问输出日志失败：{}", error))?;
        let stderr_file = File::create(&stderr_path)
            .map_err(|error| format!("创建 HTTP API 公网访问错误日志失败：{}", error))?;
        let mut child = Command::new(frpc_path)
            .arg("-c")
            .arg(config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .spawn()
            .map_err(|error| format!("启动 HTTP API 公网访问客户端失败：{}", error))?;
        thread::sleep(Duration::from_millis(PUBLIC_API_FRPC_START_WAIT_MS));
        if let Ok(Some(status)) = child.try_wait() {
            let output = read_public_api_frpc_output(&stdout_path, &stderr_path);
            if is_frpc_public_subdomain_conflict_output(&output) {
                return Err(format!(
                    "HTTP API 公网域名 {} 已被占用，请更换用户英文名后重试。",
                    public_url_for_subdomain(&normalized_subdomain)
                ));
            }
            let message = output
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().chars().take(180).collect::<String>())
                .unwrap_or_else(|| format!("状态码：{}", status.code().unwrap_or(-1)));
            return Err(format!("HTTP API 公网访问客户端启动后退出，{}。", message));
        }
        let output = read_public_api_frpc_output(&stdout_path, &stderr_path);
        if is_frpc_public_subdomain_conflict_output(&output) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "HTTP API 公网域名 {} 已被占用，请更换用户英文名后重试。",
                public_url_for_subdomain(&normalized_subdomain)
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| "公共 HTTP 外网访问状态锁已损坏".to_string())?;
        *state = Some(ManagedPublicApiFrpcClient {
            child,
            subdomain: normalized_subdomain.clone(),
        });
        Ok(status_from_subdomain(&normalized_subdomain, true, true))
    }

    /// 停止公共 HTTP API 外网访问隧道。
    /// 流程：只结束当前 Runtime 持有的 frpc 进程，不扫描系统中的其它进程。
    /// 参数：无。
    /// 返回：停止结果。
    /// 异常/边界：未启动时保持幂等成功。
    pub fn stop(&self) -> Result<(), String> {
        let managed = self
            .state
            .lock()
            .map_err(|_| "公共 HTTP 外网访问状态锁已损坏".to_string())?
            .take();
        if let Some(mut client) = managed {
            let _ = client.child.kill();
            client
                .child
                .wait()
                .map_err(|error| format!("停止 HTTP API 公网访问客户端失败：{}", error))?;
        }
        Ok(())
    }

    /// 读取公共 HTTP API 外网访问运行状态。
    /// 流程：检查当前受管 frpc 是否仍在运行，并返回与持久化配置组合后的展示状态。
    /// 参数：enabled 为持久化开关，subdomain 为持久化域名前缀。
    /// 返回：页面可展示状态。
    /// 异常/边界：如果进程已退出，会清空运行态并返回 running=false。
    pub fn status(
        &self,
        enabled: bool,
        subdomain: Option<String>,
    ) -> Result<PublicApiTunnelStatus, String> {
        let mut running = false;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "公共 HTTP 外网访问状态锁已损坏".to_string())?;
        let mut should_clear_client = false;
        if let Some(client) = state.as_mut() {
            if Some(client.subdomain.as_str()) == subdomain.as_deref() {
                if client
                    .child
                    .try_wait()
                    .map_err(|error| format!("检查公网访问客户端失败：{}", error))?
                    .is_none()
                {
                    running = true;
                } else {
                    should_clear_client = true;
                }
            } else {
                should_clear_client = true;
            }
        }
        if should_clear_client {
            if let Some(mut client) = state.take() {
                let _ = client.child.kill();
                let _ = client.child.wait();
            }
        }
        Ok(PublicApiTunnelStatus {
            enabled,
            public_url: subdomain.as_deref().map(public_url_for_subdomain),
            subdomain,
            running,
        })
    }
}

impl Drop for RuntimePublicApiTunnel {
    /// Runtime 销毁时尽力停止 API 隧道，作为 App RunEvent 清理之外的兜底。
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// 生成公共 HTTP API 随机兜底二级域名前缀。
/// 流程：使用线程随机数从 DNS 安全字符集中抽取 6 位字符。
/// 参数：无。
/// 返回：小写字母和数字组成的固定长度字符串。
/// 异常/边界：不包含短横线，避免首尾字符规则和人工辨识歧义。
pub fn generate_public_api_subdomain() -> String {
    let mut rng = rand::thread_rng();
    (0..PUBLIC_API_RANDOM_SUBDOMAIN_LENGTH)
        .map(|_| {
            let index = rng.gen_range(0..PUBLIC_API_SUBDOMAIN_ALPHABET.len());
            PUBLIC_API_SUBDOMAIN_ALPHABET[index] as char
        })
        .collect()
}

/// 校验公共 HTTP API 二级域名前缀。
/// 流程：接受用户英文名或随机兜底域名，统一转小写并校验 DNS label 安全字符。
/// 参数：subdomain 为配置文件、用户英文名或本次生成的前缀。
/// 返回：规范化前缀。
/// 异常/边界：字段为空、超过 32 位、包含非法字符或短横线位于首尾时拒绝启动公网访问。
pub fn validate_public_api_subdomain(subdomain: &str) -> Result<String, String> {
    let normalized = subdomain.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("HTTP API 公网二级域名不能为空。".to_string());
    }
    if normalized.len() > PUBLIC_API_NAMED_SUBDOMAIN_MAX_LENGTH {
        return Err("HTTP API 公网二级域名最长支持 32 位。".to_string());
    }
    if !normalized
        .bytes()
        .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
    {
        return Err("HTTP API 公网二级域名仅支持小写字母、数字和短横线。".to_string());
    }
    if normalized.starts_with('-') || normalized.ends_with('-') {
        return Err("HTTP API 公网二级域名不能以短横线开头或结尾。".to_string());
    }
    Ok(normalized)
}

/// 读取公共 API frpc 进程当前输出。
/// 流程：读取 stdout/stderr 日志文件中已经落盘的 UTF-8 文本并拼接为诊断摘要。
/// 参数：stdout_path 和 stderr_path 分别为本次启动创建的输出日志路径。
/// 返回：合并后的输出文本。
/// 异常/边界：读取失败时忽略对应文件，避免遮蔽进程启动状态。
fn read_public_api_frpc_output(
    stdout_path: &std::path::Path,
    stderr_path: &std::path::Path,
) -> String {
    let mut output = String::new();
    if let Ok(stdout) = fs::read_to_string(stdout_path) {
        output.push_str(&stdout);
    }
    if let Ok(stderr) = fs::read_to_string(stderr_path) {
        output.push_str(&stderr);
    }
    output
}

/// 判断 frpc 输出是否表示远端二级域名冲突。
/// 流程：兼容 frp 不同版本常见重复代理、重复 custom domain、重复 subdomain 文案。
/// 参数：output 为 frpc 启动失败或启动后的输出。
/// 返回：命中占用语义时 true。
/// 异常/边界：其它网络、认证、配置错误不伪装成已占用。
fn is_frpc_public_subdomain_conflict_output(output: &str) -> bool {
    let normalized = output.to_ascii_lowercase();
    if normalized.contains("router config conflict") {
        return true;
    }
    (normalized.contains("already")
        || normalized.contains("duplicate")
        || normalized.contains("repeated")
        || normalized.contains("conflict"))
        && (normalized.contains("subdomain")
            || normalized.contains("custom domain")
            || normalized.contains("domain")
            || normalized.contains("proxy"))
}

/// 从二级域名前缀构建公共 API 隧道状态。
/// 流程：集中拼接公网 URL，并保留开关和运行态。
/// 参数：subdomain 为已校验前缀，enabled/running 为状态位。
/// 返回：页面可展示状态。
/// 异常/边界：只用于已有合法前缀的状态构造。
fn status_from_subdomain(subdomain: &str, enabled: bool, running: bool) -> PublicApiTunnelStatus {
    PublicApiTunnelStatus {
        enabled,
        subdomain: Some(subdomain.to_string()),
        public_url: Some(public_url_for_subdomain(subdomain)),
        running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_public_api_subdomain_is_six_dns_safe_chars() {
        for _ in 0..32 {
            let subdomain = generate_public_api_subdomain();
            assert_eq!(subdomain.len(), 6);
            assert_eq!(subdomain.len(), PUBLIC_API_RANDOM_SUBDOMAIN_LENGTH);
            assert!(validate_public_api_subdomain(&subdomain).is_ok());
        }
    }

    #[test]
    fn public_api_subdomain_validator_accepts_user_english_name() {
        assert!(validate_public_api_subdomain("lucifer").is_ok());
        assert!(validate_public_api_subdomain("lucifer-01").is_ok());
        assert!(validate_public_api_subdomain("abc123").is_ok());
        assert!(validate_public_api_subdomain("").is_err());
        assert!(validate_public_api_subdomain("-lucifer").is_err());
        assert!(validate_public_api_subdomain("lucifer-").is_err());
        assert!(validate_public_api_subdomain("lucifer.tolern.com").is_err());
    }

    #[test]
    fn public_api_frpc_conflict_output_is_detected() {
        assert!(is_frpc_public_subdomain_conflict_output(
            "[codexman-public-api] start error: router config conflict"
        ));
        assert!(is_frpc_public_subdomain_conflict_output(
            "custom domain lucifer.tolern.com already exists"
        ));
        assert!(!is_frpc_public_subdomain_conflict_output(
            "login to server failed: dial tcp i/o timeout"
        ));
    }
}
