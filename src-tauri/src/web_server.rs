use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// APP 内置 Web 网站固定监听端口；用于局域网浏览器访问打包后的前端页面。
pub const WEB_SERVER_PORT: u16 = 1_421;
/// APP 内置 Web 网站默认监听地址；绑定所有网卡以支持内网 IP 访问。
const WEB_SERVER_HOST: &str = "0.0.0.0";

/// APP 生命周期持有的内置 Web 网站运行状态。
#[derive(Default)]
pub struct RuntimeWebServer {
    /// 当前 Web 服务停止信号与线程句柄；同一 App 生命周期只允许启动一份服务。
    state: Mutex<Option<ManagedWebServer>>,
}

/// APP 内置 Web 网站受管线程。
struct ManagedWebServer {
    /// 通知服务线程退出的单向信号。
    shutdown: Sender<()>,
    /// 服务线程句柄；APP 退出时 join，避免端口短时间残留。
    thread: thread::JoinHandle<()>,
}

impl RuntimeWebServer {
    /// 启动内置 Web 网站。
    /// 流程：解析前端静态资源目录，绑定固定端口并启动只读 HTTP 服务线程；参数为 Tauri App 句柄；成功返回局域网可访问的基础地址。
    /// 异常/边界：重复启动、端口被占用或资源目录无法解析时返回错误；服务线程只读取静态文件，不暴露任意目录。
    pub fn start(&self, app: &AppHandle) -> Result<String, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "内置 Web 网站状态锁已损坏".to_string())?;
        if state.is_some() {
            return Err("内置 Web 网站已启动，禁止重复启动".to_string());
        }
        let web_root = resolve_web_root(app)?;
        let listener = TcpListener::bind((WEB_SERVER_HOST, WEB_SERVER_PORT))
            .map_err(|error| format!("启动内置 Web 网站失败：{}", error))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("设置内置 Web 网站非阻塞监听失败：{}", error))?;
        let (shutdown, shutdown_receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("codexman-web-server".to_string())
            .spawn(move || loop {
                if shutdown_receiver.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => handle_connection(stream, &web_root),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            })
            .map_err(|error| format!("创建内置 Web 网站线程失败：{}", error))?;
        *state = Some(ManagedWebServer { shutdown, thread });
        Ok(format!("http://127.0.0.1:{}", WEB_SERVER_PORT))
    }

    /// 停止内置 Web 网站。
    /// 流程：发送停止信号并等待线程退出；参数为空；成功返回空值。
    /// 异常/边界：服务未启动时视为成功；线程 panic 时返回诊断，避免静默遗留端口。
    pub fn shutdown(&self) -> Result<(), String> {
        let managed = self
            .state
            .lock()
            .map_err(|_| "内置 Web 网站状态锁已损坏".to_string())?
            .take();
        if let Some(managed_server) = managed {
            let _ = managed_server.shutdown.send(());
            managed_server
                .thread
                .join()
                .map_err(|_| "内置 Web 网站线程异常退出".to_string())?;
        }
        Ok(())
    }
}

/// 解析内置 Web 网站静态资源根目录。
/// 流程：优先使用环境变量覆盖；打包后读取 Tauri 资源目录中的 dist 或上级资源目录的 _up_/dist；开发期回退当前仓库 dist。
/// 参数：app 为 Tauri App 句柄；返回包含 index.html 的静态资源目录。
/// 异常/边界：所有候选目录都缺少 index.html 时返回错误，避免服务误暴露错误路径。
fn resolve_web_root(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(web_root) = std::env::var("AITOOL_WEB_ROOT") {
        candidates.push(PathBuf::from(web_root));
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("dist"));
        candidates.push(resource_dir.join("_up_").join("dist"));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("dist"));
        candidates.push(current_dir.join("..").join("dist"));
    }
    for candidate in candidates {
        if candidate.join("index.html").is_file() {
            return candidate
                .canonicalize()
                .map_err(|error| format!("解析内置 Web 网站资源目录失败：{}", error));
        }
    }
    Err("未找到内置 Web 网站资源目录，请先执行 npm run build".to_string())
}

/// 处理单个 HTTP 连接。
/// 流程：读取请求首行，解析路径后返回静态文件；参数为 TCP 流和资源根目录；返回空值。
/// 异常/边界：非法请求返回 400，越界路径返回 403，SPA 子路由统一回退 index.html。
fn handle_connection(mut stream: TcpStream, web_root: &Path) {
    let mut buffer = [0_u8; 2048];
    let read_bytes = match stream.read(&mut buffer) {
        Ok(read_bytes) => read_bytes,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buffer[..read_bytes]);
    let pathname = match parse_request_path(&request) {
        Some(pathname) => pathname,
        None => {
            write_response(
                &mut stream,
                400,
                "text/plain; charset=utf-8",
                b"Bad Request",
            );
            return;
        }
    };
    serve_path(&mut stream, web_root, &pathname);
}

/// 解析 HTTP 请求路径。
/// 流程：仅接受 GET 和 HEAD 请求首行，去掉 query/hash 并保留路径部分；参数为原始 HTTP 请求文本；返回规范化前的 URL path。
/// 异常/边界：缺少方法、路径或 HTTP 版本时返回 None；非 GET/HEAD 请求不提供写入能力。
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

/// 解码 URL 路径中的百分号编码。
/// 流程：逐字节扫描 `%XX` 片段并转换为 UTF-8 字符串；参数为 URL path；返回尽力解码后的路径。
/// 异常/边界：非法百分号编码保持原样，避免把异常请求错误映射到其它文件。
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

/// 返回指定静态路径。
/// 流程：把根路径映射到 index.html，校验 canonicalize 后仍在资源目录内，再按扩展名写响应。
/// 参数：stream 为当前连接，web_root 为资源根目录，pathname 为请求路径；返回空值。
/// 异常/边界：静态资源缺失时回退 index.html 支持前端路由；index.html 缺失时返回 404。
fn serve_path(stream: &mut TcpStream, web_root: &Path, pathname: &str) {
    let relative_path = pathname.trim_start_matches('/');
    let requested_path = if relative_path.is_empty() {
        web_root.join("index.html")
    } else {
        web_root.join(relative_path)
    };
    match safe_file_path(web_root, &requested_path) {
        Ok(file_path) => write_file_response(stream, web_root, &file_path),
        Err(status) => write_response(
            stream,
            status,
            "text/plain; charset=utf-8",
            status_message(status).as_bytes(),
        ),
    }
}

/// 校验请求文件位于静态资源根目录内。
/// 流程：canonicalize 请求路径和资源根目录，再检查前缀关系；参数为资源根目录和请求路径；返回安全文件路径。
/// 异常/边界：文件不存在返回 index.html 路径用于 SPA 回退，路径逃逸返回 403。
fn safe_file_path(web_root: &Path, requested_path: &Path) -> Result<PathBuf, u16> {
    if let Ok(file_path) = requested_path.canonicalize() {
        let root_path = web_root.canonicalize().map_err(|_| 404_u16)?;
        if file_path.starts_with(root_path) && file_path.is_file() {
            return Ok(file_path);
        }
        return Err(403);
    }
    Ok(web_root.join("index.html"))
}

/// 写入文件响应。
/// 流程：打开文件并完整读取，再按 MIME 类型返回；参数为连接、资源根目录和安全文件路径；返回空值。
/// 异常/边界：缺失 index.html 返回 404；读取失败返回 500。
fn write_file_response(stream: &mut TcpStream, web_root: &Path, file_path: &Path) {
    match File::open(file_path) {
        Ok(mut file) => {
            let mut content = Vec::new();
            if file.read_to_end(&mut content).is_err() {
                write_response(
                    stream,
                    500,
                    "text/plain; charset=utf-8",
                    b"Internal Server Error",
                );
                return;
            }
            write_response(stream, 200, mime_by_path(file_path), &content);
        }
        Err(_) => {
            let index_path = web_root.join("index.html");
            if file_path != index_path && index_path.is_file() {
                write_file_response(stream, web_root, &index_path);
                return;
            }
            write_response(stream, 404, "text/plain; charset=utf-8", b"Not Found");
        }
    }
}

/// 写入 HTTP 响应。
/// 流程：组装状态行、必要响应头和响应体后 flush；参数为连接、状态码、MIME 和响应体；返回空值。
/// 异常/边界：写入失败时静默结束当前连接，不影响服务线程处理后续请求。
fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        status_message(status),
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

/// 根据文件扩展名返回响应 MIME。
/// 流程：匹配前端构建产物常见扩展名；参数为文件路径；返回 Content-Type。
/// 异常/边界：未知扩展名使用二进制流，避免浏览器错误解析可执行文本。
fn mime_by_path(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

/// 返回 HTTP 状态码说明。
/// 流程：为当前服务会返回的状态码提供固定 reason phrase；参数为状态码；返回英文说明。
/// 异常/边界：未知状态码统一返回 Unknown，避免拼接空状态行。
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

#[cfg(test)]
mod tests {
    use super::{parse_request_path, percent_decode_path};

    #[test]
    fn request_path_decodes_queryless_route() {
        assert_eq!(
            parse_request_path("GET /taskManage?mode=hub HTTP/1.1\r\nHost: test\r\n"),
            Some("/taskManage".to_string())
        );
    }

    #[test]
    fn percent_decoder_keeps_invalid_escape_visible() {
        assert_eq!(percent_decode_path("/assets/%zz.js"), "/assets/%zz.js");
        assert_eq!(percent_decode_path("/a%20b"), "/a b");
    }
}
