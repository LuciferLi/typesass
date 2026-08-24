use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header::ORIGIN;
use tungstenite::http::HeaderValue;
use tungstenite::{client, Message};
use url::Url;

use crate::codex_desktop::{codex_page_websocket_url, CODEX_CDP_PORT};
use crate::task_store::TaskAttachmentRecord;

/// CDP TCP 建连和单条协议交互的最大等待时间。
const CDP_IO_TIMEOUT: Duration = Duration::from_secs(5);
/// 等待 Codex 路由切换和精确 composer 出现的轮询次数。
const COMPOSER_READY_ATTEMPTS: usize = 40;
/// composer 就绪轮询间隔。
const COMPOSER_READY_INTERVAL: Duration = Duration::from_millis(100);
/// composer 允许提交的单条 prompt 最大 UTF-8 字节数，业务字符上限仍由 TaskStore 负责。
const CDP_PROMPT_MAX_BYTES: usize = 1024 * 1024;
/// Codex 全局状态文件读取上限；当前真实文件约 1.6 MiB，4 MiB 预算可容纳增长且阻止本机异常文件无界分配。
const CODEX_GLOBAL_STATE_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 等待主进程创建并选中新 local project 的最大轮询次数。
const WORKSPACE_SELECTION_ATTEMPTS: usize = 40;
/// 全局状态读取轮询间隔；短暂文件替换或中间态 JSON 会在总时限内重试。
const WORKSPACE_SELECTION_INTERVAL: Duration = Duration::from_millis(100);
/// 全局状态允许的 local project 数量上限，避免外部 JSON 即使未超字节上限也形成异常大业务集合。
const LOCAL_PROJECT_LIMIT: usize = 512;
/// 单个 local project 允许的 rootPaths 数量上限。
const LOCAL_PROJECT_ROOT_LIMIT: usize = 32;
/// 单条 CDP 请求在目标响应前允许跳过的页面通知数量；Codex Desktop 动画、流式状态和多窗口事件会产生大量异步通知。
const CDP_RESPONSE_NOTIFICATION_LIMIT: usize = 2048;

/// Codex 全局状态中当前选中的项目身份。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedProjectState {
    /// 项目类型；任务提交只接受 local，不允许 cloud 或未知类型。
    #[serde(rename = "type")]
    project_type: String,
    /// 当前选中项目 ID，必须精确指向 local-projects 中唯一 root 命中项。
    project_id: String,
}

/// Codex 全局状态中的本地项目记录。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalProjectState {
    /// 记录自身 ID，必须与 local-projects 对象键和 selected projectId 同时一致。
    id: String,
    /// 项目覆盖的工作空间根；只允许与 Rust canonical root 完全相等。
    root_paths: Vec<String>,
}

/// 工作空间确认所需的最小 Codex 全局状态视图。
/// 未声明字段由 serde 忽略，避免把 Desktop 的其它独立设置耦合进任务提交契约。
#[derive(Debug, Deserialize)]
struct CodexWorkspaceGlobalState {
    /// 当前 Desktop 选择的项目。
    #[serde(rename = "selected-project")]
    selected_project: SelectedProjectState,
    /// projectId 到本地项目记录的映射。
    #[serde(rename = "local-projects")]
    local_projects: HashMap<String, LocalProjectState>,
}

/// Codex Desktop 已执行一次 Enter 后的提交回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexSubmissionReceipt {
    /// Enter 请求写入 CDP 前记录的 Unix 毫秒水位，用于排除旧 thread/session。
    pub(crate) submitted_at_ms: i64,
    /// Enter 后 Codex Desktop 活动会话暴露的非本地临时 thread ID，仍需调用方用 JSONL 精确复核。
    pub(crate) thread_id: Option<String>,
}

/// CDP 原生提交的失败分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexCdpFailure {
    /// 稳定内部错误码，调用方据此决定是否允许任务再次排队。
    pub(crate) code: &'static str,
    /// 不包含 prompt、cwd、WebSocket、端口或 DOM 的安全说明。
    pub(crate) message: String,
    /// 是否已经越过可能执行 Enter 的边界；为 true 时禁止自动重放 prompt。
    pub(crate) submission_uncertain: bool,
}

/// 通过 Codex Desktop 主 renderer 向已存在会话提交一条文本或图片消息。
/// 流程：打开目标 `/local/{threadId}` 路由，精确确认活动会话和唯一 composer，上传图片附件，写入并校验正文后只按一次 Enter。
/// 参数：thread_id 为已由上层校验存在的 CodeX 会话 ID，prompt 为待发送正文，attachments 为图片附件，before_enter 用于在越过发送边界前持久化调用方状态。
/// 返回：Enter 前时间水位；已有会话不需要恢复新 thread ID，因此回执中的 threadId 固定为目标会话。
/// 异常/边界：路由、输入框、正文校验失败均发生在 Enter 前；Enter 后任何协议失败都标记为不确定，调用方禁止自动重放。
pub(crate) fn submit_existing_thread_message<F>(
    thread_id: &str,
    prompt: &str,
    attachments: &[TaskAttachmentRecord],
    before_enter: F,
) -> Result<CodexSubmissionReceipt, CodexCdpFailure>
where
    F: FnOnce(i64) -> Result<(), String>,
{
    if thread_id.trim().is_empty() || (prompt.trim().is_empty() && attachments.is_empty()) {
        return Err(pre_submission_failure(
            "CODEX_CDP_INPUT_INVALID",
            "Codex Desktop 提交参数无效。",
        ));
    }
    if prompt.len() > CDP_PROMPT_MAX_BYTES {
        return Err(pre_submission_failure(
            "CODEX_CDP_PROMPT_TOO_LARGE",
            "消息内容超过 Codex Desktop 提交上限。",
        ));
    }
    let mut session = CdpSession::connect()?;
    session.request(1, "Runtime.enable", json!({}), false)?;
    session.request(9_000, "Page.bringToFront", json!({}), false)?;
    let thread_id_json = serde_json::to_string(thread_id).map_err(|_| {
        pre_submission_failure("CODEX_CDP_INPUT_INVALID", "无法准备 Codex Desktop 会话。")
    })?;
    let navigation_result = session.evaluate(
        2,
        &format!(
            r#"(() => {{
                const targetThreadId = {thread_id_json};
                const bridge = window.electronBridge;
                if (!bridge || typeof bridge.sendMessageFromView !== 'function') return false;
                window.postMessage({{
                    type: 'navigate-to-route',
                    path: `/local/${{targetThreadId}}`,
                    state: {{ focusComposerNonce: Date.now() }}
                }}, window.location.origin);
                return true;
            }})()"#
        ),
        false,
    )?;
    if navigation_result.as_bool() != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_THREAD_NAVIGATION_FAILED",
            "Codex Desktop 未确认会话切换。",
        ));
    }
    let mut composer_ready = false;
    for attempt in 0..COMPOSER_READY_ATTEMPTS {
        let result = session.evaluate(
            10 + attempt as u64,
            &format!(
                r#"(() => {{
                    const targetThreadId = {thread_id_json};
                    const normalizeThreadId = (value) => String(value || '').replace(/^local:/, '');
                    const routeMatch = window.location.pathname.match(/^\/local\/([^/?#]+)/);
                    const active = document.querySelector('[data-app-action-sidebar-thread-active="true"]');
                    const activeThreadId = normalizeThreadId(active?.getAttribute('data-app-action-sidebar-thread-id') || routeMatch?.[1] || '');
                    if (activeThreadId !== targetThreadId) return {{ ok: false, reason: 'thread-mismatch', activeThreadId }};
                    const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                      .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                    if (composers.length !== 1) return {{ ok: false, reason: 'composer-count', count: composers.length }};
                    const composer = composers[0];
                    const disabled = composer.getAttribute('aria-disabled') === 'true'
                      || composer.closest('[aria-disabled="true"]')
                      || composer.matches('[disabled]');
                    if (disabled) return {{ ok: false, reason: 'composer-disabled' }};
                    if ((composer.textContent || '').trim() !== '') return {{ ok: false, reason: 'composer-not-empty' }};
                    composer.focus();
                    return {{ ok: document.activeElement === composer, reason: 'ready' }};
                }})()"#
            ),
            false,
        )?;
        if result.get("ok").and_then(Value::as_bool) == Some(true) {
            composer_ready = true;
            break;
        }
        std::thread::sleep(COMPOSER_READY_INTERVAL);
    }
    if !composer_ready {
        return Err(pre_submission_failure(
            "CODEX_CDP_COMPOSER_NOT_READY",
            "Codex Desktop 目标会话输入框未在限定时间内就绪。",
        ));
    }
    prepare_composer_attachments(&mut session, attachments)?;
    if !prompt.trim().is_empty() {
        if let Err(failure) =
            session.request(60, "Input.insertText", json!({"text": prompt}), false)
        {
            return Err(if session.clear_composer().is_ok() {
                failure
            } else {
                protocol_failure_at(true, "existing composer cleanup failed after insert error")
            });
        }
    }
    let prompt_json = serde_json::to_string(prompt).map_err(|_| {
        pre_submission_failure(
            "CODEX_CDP_INPUT_INVALID",
            "无法校验 Codex Desktop 消息内容。",
        )
    })?;
    let expected_attachment_count = attachments.len();
    let composer_check = session.evaluate(
        61,
        &format!(
            r#"(() => {{
                const targetThreadId = {thread_id_json};
                const expectedAttachmentCount = {expected_attachment_count};
                const normalizeThreadId = (value) => String(value || '').replace(/^local:/, '');
                const routeMatch = window.location.pathname.match(/^\/local\/([^/?#]+)/);
                const active = document.querySelector('[data-app-action-sidebar-thread-active="true"]');
                const activeThreadId = normalizeThreadId(active?.getAttribute('data-app-action-sidebar-thread-id') || routeMatch?.[1] || '');
                const readComposerText = (composer) => {{
                    const blocks = Array.from(composer.childNodes)
                      .filter((node) => node instanceof HTMLElement && (node.matches('p,div') || node.getClientRects().length > 0));
                    if (blocks.length === 0) return composer.textContent || '';
                    return blocks.map((block) => block.textContent || '').join('\n');
                }};
                const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                const attachmentImages = document.querySelectorAll(
                  'img[src^="blob:"], img[src^="app://fs/"], '
                  + '.composer-attachment-surface img[alt="User attachment"], '
                  + '.composer-attachment-surface img[alt="用户附件"]'
                ).length;
                return activeThreadId === targetThreadId
                  && composers.length === 1
                  && readComposerText(composers[0]) === {prompt_json}
                  && attachmentImages >= expectedAttachmentCount;
            }})()"#
        ),
        false,
    );
    if !matches!(composer_check, Ok(Value::Bool(true))) {
        if session.clear_composer().is_err() {
            return Err(protocol_failure_at(
                true,
                "existing composer cleanup failed after write check",
            ));
        }
        return Err(pre_submission_failure(
            "CODEX_CDP_COMPOSER_WRITE_FAILED",
            "Codex Desktop 输入框未确认完整消息内容。",
        ));
    }
    let submitted_at_ms = current_unix_millis();
    if before_enter(submitted_at_ms).is_err() {
        return Err(if session.clear_composer().is_ok() {
            pre_submission_failure(
                "CODEX_CDP_SUBMISSION_PERSIST_FAILED",
                "无法在发送前持久化 Codex Desktop 提交阶段。",
            )
        } else {
            protocol_failure_at(true, "existing composer cleanup failed after persist error")
        });
    }
    session.evaluate(
        62,
        &format!(
            r#"(() => {{
                const targetThreadId = {thread_id_json};
                const normalizeThreadId = (value) => String(value || '').replace(/^local:/, '');
                const routeMatch = window.location.pathname.match(/^\/local\/([^/?#]+)/);
                const active = document.querySelector('[data-app-action-sidebar-thread-active="true"]');
                const activeThreadId = normalizeThreadId(active?.getAttribute('data-app-action-sidebar-thread-id') || routeMatch?.[1] || '');
                const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                const composer = activeThreadId === targetThreadId && composers.length === 1 ? composers[0] : null;
                if (!composer) return false;
                composer.focus();
                const range = document.createRange();
                range.selectNodeContents(composer);
                range.collapse(false);
                const selection = window.getSelection();
                selection?.removeAllRanges();
                selection?.addRange(range);
                return document.activeElement === composer;
            }})()"#
        ),
        true,
    )?;
    session.request(
        63,
        "Input.dispatchKeyEvent",
        json!({
            "type": "rawKeyDown",
            "key": "Enter",
            "code": "Enter",
            "windowsVirtualKeyCode": 13,
            "nativeVirtualKeyCode": 36,
            "macCharCode": 13,
            "unmodifiedText": "\r",
            "text": "\r"
        }),
        true,
    )?;
    session.request(
        64,
        "Input.dispatchKeyEvent",
        json!({
            "type": "keyUp",
            "key": "Enter",
            "code": "Enter",
            "windowsVirtualKeyCode": 13,
            "nativeVirtualKeyCode": 36
        }),
        true,
    )?;
    Ok(CodexSubmissionReceipt {
        submitted_at_ms,
        thread_id: Some(thread_id.to_string()),
    })
}

/// 准备当前 Codex composer 的图片附件。
/// 流程：先清理已有图片预览，再为每个结构化附件写入临时文件，通过隐藏 file input 和拖拽事件交给原生 composer。
/// 参数：session 为已连接并聚焦目标 renderer 的 CDP 会话；attachments 为已由业务层校验过的图片附件。
/// 返回：无返回值，成功表示 composer 已出现不少于本次附件数量的图片预览。
/// 异常/边界：不记录 data URL、临时路径或文件内容；上传失败发生在 Enter 前，可由调用方安全重试。
fn prepare_composer_attachments(
    session: &mut CdpSession,
    attachments: &[TaskAttachmentRecord],
) -> Result<(), CodexCdpFailure> {
    let cleared_existing_attachments = session.evaluate(
        51,
        r#"(() => new Promise((resolve) => {
            const composer = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
              .find((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
            if (!composer) {
              resolve(false);
              return;
            }
            const images = document.querySelectorAll(
              '.composer-attachment-surface img[alt="User attachment"], '
              + '.composer-attachment-surface img[alt="用户附件"], '
              + 'img[src^="blob:"], img[src^="app://fs/"]'
            );
            for (const image of images) {
              const surface = image.closest('.composer-attachment-surface[role="button"]');
              const remove = surface?.querySelector('button[aria-label^="Remove"], button[aria-label^="移除"]');
              remove?.click();
            }
            let attempts = 0;
            const timer = setInterval(() => {
              attempts += 1;
              const remaining = document.querySelectorAll(
                '.composer-attachment-surface img[alt="User attachment"], '
                + '.composer-attachment-surface img[alt="用户附件"], '
                + 'img[src^="blob:"], img[src^="app://fs/"]'
              ).length;
              if (remaining === 0 || attempts >= 20) {
                clearInterval(timer);
                resolve(remaining === 0);
              }
            }, 100);
        }))()"#,
        false,
    )?;
    if cleared_existing_attachments.as_bool() != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex Desktop 当前输入框残留图片附件未清理完成。",
        ));
    }
    if attachments.is_empty() {
        return Ok(());
    }
    let attachment_paths = attachments
        .iter()
        .map(write_attachment_temp_file)
        .collect::<Result<Vec<_>, _>>()?;
    let input_id = format!("codexman-task-attachment-{}", uuid::Uuid::new_v4().simple());
    let input_id_json = serde_json::to_string(&input_id)
        .map_err(|_| protocol_failure_at(false, "attachment input id serialization failed"))?;
    let prepared = session.evaluate(
        59,
        &format!(
            r#"(() => new Promise((resolve) => {{
                const inputId = {input_id_json};
                const existing = document.getElementById(inputId);
                if (existing) existing.remove();
                const composer = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .find((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                if (!composer) {{
                  resolve({{ ok: false, attachmentImagesBefore: 0 }});
                  return;
                }}
                const attachmentImagesBefore = document.querySelectorAll(
                  'img[src^="blob:"], img[src^="app://fs/"], '
                  + '.composer-attachment-surface img[alt="User attachment"], '
                  + '.composer-attachment-surface img[alt="用户附件"]'
                ).length;
                const input = document.createElement('input');
                input.type = 'file';
                input.multiple = true;
                input.id = inputId;
                input.style.display = 'none';
                document.body.append(input);
                resolve({{ ok: true, attachmentImagesBefore }});
            }}))()"#
        ),
        false,
    )?;
    if prepared.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex Desktop 当前输入框未找到可接收图片的区域。",
        ));
    }
    let attachment_images_before = prepared
        .get("attachmentImagesBefore")
        .and_then(Value::as_u64)
        .ok_or_else(|| protocol_failure_at(false, "attachment count missing"))?;
    session.request(58, "DOM.enable", json!({}), false)?;
    let document_node_id = session
        .request(56, "DOM.getDocument", json!({"depth": 0}), false)?
        .pointer("/result/root/nodeId")
        .and_then(Value::as_u64)
        .ok_or_else(|| protocol_failure_at(false, "attachment document node missing"))?;
    let input_node_id = session
        .request(
            57,
            "DOM.querySelector",
            json!({
                "nodeId": document_node_id,
                "selector": format!("#{input_id}")
            }),
            false,
        )?
        .pointer("/result/nodeId")
        .and_then(Value::as_u64)
        .ok_or_else(|| protocol_failure_at(false, "attachment input node missing"))?;
    if input_node_id == 0 {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex Desktop 图片附件临时输入控件创建失败。",
        ));
    }
    session.request(
        55,
        "DOM.setFileInputFiles",
        json!({
            "nodeId": input_node_id,
            "files": attachment_paths
        }),
        false,
    )?;
    let dispatched = session.evaluate(
        54,
        &format!(
            r#"(() => {{
                const input = document.getElementById({input_id_json});
                const composer = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .find((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                if (!input || !composer) return false;
                const transfer = new DataTransfer();
                for (const file of input.files) transfer.items.add(file);
                if (transfer.files.length !== input.files.length || transfer.files.length === 0) return false;
                const rect = composer.getBoundingClientRect();
                const eventOptions = {{
                  bubbles: true,
                  cancelable: true,
                  composed: true,
                  dataTransfer: transfer,
                  shiftKey: true,
                  clientX: Math.round(rect.left + rect.width / 2),
                  clientY: Math.round(rect.top + rect.height / 2),
                }};
                composer.focus();
                for (const type of ['dragenter', 'dragover', 'drop']) {{
                  composer.dispatchEvent(new DragEvent(type, eventOptions));
                }}
                return true;
            }})()"#
        ),
        false,
    )?;
    if dispatched.as_bool() != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex Desktop 未接收图片附件拖拽事件。",
        ));
    }
    let expected_attachment_count = attachment_images_before + attachments.len() as u64;
    let confirmed = session.evaluate(
        53,
        &format!(
            r#"(() => new Promise((resolve) => {{
                const expected = {expected_attachment_count};
                let attempts = 0;
                const timer = setInterval(() => {{
                  attempts += 1;
                  const attachmentImages = document.querySelectorAll(
                    'img[src^="blob:"], img[src^="app://fs/"], '
                    + '.composer-attachment-surface img[alt="User attachment"], '
                    + '.composer-attachment-surface img[alt="用户附件"]'
                  ).length;
                  const pending = document.body.innerText.includes('正在上传') || document.body.innerText.includes('Uploading');
                  if ((attachmentImages >= expected && !pending) || attempts >= 50) {{
                    clearInterval(timer);
                    resolve(attachmentImages >= expected && !pending);
                  }}
                }}, 100);
            }}))()"#
        ),
        false,
    )?;
    let _ = session.evaluate(
        52,
        &format!(
            r#"(() => {{
                document.getElementById({input_id_json})?.remove();
                return true;
            }})()"#
        ),
        false,
    );
    if confirmed.as_bool() != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex Desktop 当前输入框未确认图片附件预览。",
        ));
    }
    Ok(())
}

/// 通过 Codex Desktop 主 renderer 在精确工作空间创建原生新会话并按一次 Enter。
/// 流程：通过受支持的 set-active bridge 让主进程选中项目；只读轮询全局状态精确确认 canonical root，点击项目行新聊天按钮创建空会话，写入前复核选择未变化，最后仅发送一次 Enter。
/// 参数：workspace_path 为 canonical 工作目录，prompt 为任务正文；返回 Enter 前时间水位，真实 thread ID 必须由调用方以 UI/JSONL 双通道唯一恢复。
/// 异常/边界：全局状态超限、符号链接、selected 记录内重复 root、非 local 选择或中间态超时均在输入前失败；其它项目同 root 不阻断权威选择。
pub(crate) fn submit_new_chat<F>(
    workspace_path: &str,
    prompt: &str,
    attachments: &[TaskAttachmentRecord],
    before_enter: F,
) -> Result<CodexSubmissionReceipt, CodexCdpFailure>
where
    F: FnOnce(i64) -> Result<(), String>,
{
    if workspace_path.trim().is_empty() || prompt.trim().is_empty() {
        return Err(pre_submission_failure(
            "CODEX_CDP_INPUT_INVALID",
            "Codex Desktop 提交参数无效。",
        ));
    }
    if prompt.len() > CDP_PROMPT_MAX_BYTES {
        return Err(pre_submission_failure(
            "CODEX_CDP_PROMPT_TOO_LARGE",
            "任务内容超过 Codex Desktop 提交上限。",
        ));
    }
    let mut session = CdpSession::connect()?;
    session.request(1, "Runtime.enable", json!({}), false)?;
    session.request(9_000, "Page.bringToFront", json!({}), false)?;
    session.evaluate(
        2,
        r#"(() => {
            const active = document.querySelector('[data-app-action-sidebar-thread-active="true"]');
            const routeMatch = window.location.pathname.match(/^\/local\/([^/?#]+)/);
            const threadId = active?.getAttribute('data-app-action-sidebar-thread-id') || routeMatch?.[1] || '';
            return {
                route: `${window.location.pathname}${window.location.search}${window.location.hash}`,
                threadId
            };
        })()"#,
        false,
    )?;
    let workspace_json = serde_json::to_string(workspace_path).map_err(|_| {
        pre_submission_failure(
            "CODEX_CDP_INPUT_INVALID",
            "无法准备 Codex Desktop 工作空间。",
        )
    })?;
    let workspace_result = session.evaluate(
        3,
        &format!(
            r#"(async () => {{
                const bridge = window.electronBridge;
                if (!bridge || typeof bridge.sendMessageFromView !== 'function') return false;
                await bridge.sendMessageFromView({{type:'electron-set-active-workspace-root', root:{workspace_json}}});
                return true;
            }})()"#
        ),
        false,
    )?;
    if workspace_result.as_bool() != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_WORKSPACE_SWITCH_FAILED",
            "Codex Desktop 未确认精确工作空间切换。",
        ));
    }
    wait_for_selected_workspace(workspace_path)?;
    let selected_workspace_composer = session.evaluate(
        4,
        r#"(() => {
            const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
              .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
            return {
                count: composers.length,
                draft: composers.length === 1 ? composers[0].textContent : null
            };
        })()"#,
        false,
    )?;
    if selected_workspace_composer
        .get("count")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Err(protocol_failure_at(
            false,
            "selected workspace composer count invalid",
        ));
    }
    let workspace_label = Path::new(workspace_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            pre_submission_failure(
                "CODEX_CDP_INPUT_INVALID",
                "无法准备 Codex Desktop 工作空间。",
            )
        })?;
    let workspace_label_json = serde_json::to_string(workspace_label)
        .map_err(|_| protocol_failure_at(false, "workspace label serialization failed"))?;
    let navigation_result = session.evaluate(
        5,
        &format!(
            r#"(() => {{
                const expectedLabel = {workspace_label_json};
                const projectRow = Array.from(document.querySelectorAll('[data-app-action-sidebar-project-row]'))
                  .find((row) => row.getAttribute('data-app-action-sidebar-project-label') === expectedLabel);
                const newChatButton = projectRow && Array.from(projectRow.querySelectorAll('button')).find((button) => {{
                    const label = (button.getAttribute('aria-label') || '').toLowerCase();
                    return label.includes('开始新聊天') || label.includes('start new chat');
                }});
                if (newChatButton) {{
                  window.setTimeout(() => newChatButton.click(), 0);
                  return {{ ok: true, navigation: 'project-button' }};
                }}
                window.setTimeout(() => window.postMessage({{
                    type: 'navigate-to-route',
                    path: '/',
                    state: {{ focusComposerNonce: Date.now() }}
                  }}, window.location.origin), 250);
                return {{ ok: true, navigation: 'workspace-root' }};
            }})()"#
        ),
        false,
    )?;
    if navigation_result.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_NEW_CHAT_FAILED",
            "Codex Desktop 未确认新会话导航。",
        ));
    }
    let require_project_label =
        navigation_result.get("navigation").and_then(Value::as_str) == Some("project-button");
    let mut composer_ready = false;
    for attempt in 0..COMPOSER_READY_ATTEMPTS {
        let result = session.evaluate(
            10 + attempt as u64,
            &format!(r#"(() => {{
                const expectedLabel = {workspace_label_json};
                const requireProjectLabel = {require_project_label};
                const activeProject = document.querySelector(
                  '[data-app-action-sidebar-project-row][aria-current="page"], '
                  + '[data-app-action-sidebar-project-row][data-app-action-sidebar-project-active="true"]'
                );
                const activeProjectLabel = activeProject?.getAttribute('data-app-action-sidebar-project-label') || '';
                if (requireProjectLabel && activeProjectLabel !== expectedLabel) return false;
                const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                if (composers.length !== 1) return false;
                const composer = composers[0];
                if ((composer.textContent || '').trim() !== '') return false;
                composer.focus();
                return document.activeElement === composer;
            }})()"#),
            false,
        )?;
        if result.as_bool() == Some(true) {
            composer_ready = true;
            break;
        }
        std::thread::sleep(COMPOSER_READY_INTERVAL);
    }
    if !composer_ready {
        return Err(pre_submission_failure(
            "CODEX_CDP_COMPOSER_NOT_READY",
            "Codex Desktop 新会话输入框未在限定时间内就绪。",
        ));
    }
    if !read_selected_workspace_state(
        &crate::codex_home_dir()
            .map_err(|_| workspace_state_failure())?
            .join(".codex-global-state.json"),
        workspace_path,
    )? {
        return Err(workspace_state_failure());
    }
    let cleared_existing_attachments = session.evaluate(
        51,
        r#"(() => new Promise((resolve) => {
            const composer = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
              .find((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
            if (!composer) {
              resolve(false);
              return;
            }
            const images = document.querySelectorAll(
              '.composer-attachment-surface img[alt="User attachment"], '
              + '.composer-attachment-surface img[alt="用户附件"], '
              + 'img[src^="blob:"], img[src^="app://fs/"]'
            );
            for (const image of images) {
              const surface = image.closest('.composer-attachment-surface[role="button"]');
              const remove = surface?.querySelector('button[aria-label^="Remove"], button[aria-label^="移除"]');
              remove?.click();
            }
            let attempts = 0;
            const timer = setInterval(() => {
              attempts += 1;
              const remaining = document.querySelectorAll(
                '.composer-attachment-surface img[alt="User attachment"], '
                + '.composer-attachment-surface img[alt="用户附件"], '
                + 'img[src^="blob:"], img[src^="app://fs/"]'
              ).length;
              if (remaining === 0 || attempts >= 20) {
                clearInterval(timer);
                resolve(remaining === 0);
              }
            }, 100);
        }))()"#,
        false,
    )?;
    if cleared_existing_attachments.as_bool() != Some(true) {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
            "Codex Desktop 当前输入框残留图片附件未清理完成。",
        ));
    }
    if !attachments.is_empty() {
        let attachment_paths = attachments
            .iter()
            .map(write_attachment_temp_file)
            .collect::<Result<Vec<_>, _>>()?;
        let input_id = format!("codexman-task-attachment-{}", uuid::Uuid::new_v4().simple());
        let input_id_json = serde_json::to_string(&input_id)
            .map_err(|_| protocol_failure_at(false, "attachment input id serialization failed"))?;
        let prepared = session.evaluate(
            59,
            &format!(
                r#"(() => new Promise((resolve) => {{
                    const inputId = {input_id_json};
                    const existing = document.getElementById(inputId);
                    if (existing) existing.remove();
                    const composer = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                      .find((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                    if (!composer) {{
                      resolve({{ ok: false, attachmentImagesBefore: 0 }});
                      return;
                    }}
                    const attachmentImagesBefore = document.querySelectorAll(
                      'img[src^="blob:"], img[src^="app://fs/"], '
                      + '.composer-attachment-surface img[alt="User attachment"], '
                      + '.composer-attachment-surface img[alt="用户附件"]'
                    ).length;
                    const input = document.createElement('input');
                    input.type = 'file';
                    input.multiple = true;
                    input.id = inputId;
                    input.style.display = 'none';
                    document.body.append(input);
                    resolve({{ ok: true, attachmentImagesBefore }});
                }}))()"#
            ),
            false,
        )?;
        if prepared.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(pre_submission_failure(
                "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
                "Codex Desktop 当前输入框未找到可接收图片的区域。",
            ));
        }
        let attachment_images_before = prepared
            .get("attachmentImagesBefore")
            .and_then(Value::as_u64)
            .ok_or_else(|| protocol_failure_at(false, "attachment count missing"))?;
        session.request(58, "DOM.enable", json!({}), false)?;
        let document_node_id = session
            .request(56, "DOM.getDocument", json!({"depth": 0}), false)?
            .pointer("/result/root/nodeId")
            .and_then(Value::as_u64)
            .ok_or_else(|| protocol_failure_at(false, "attachment document node missing"))?;
        let input_node_id = session
            .request(
                57,
                "DOM.querySelector",
                json!({
                    "nodeId": document_node_id,
                    "selector": format!("#{input_id}")
                }),
                false,
            )?
            .pointer("/result/nodeId")
            .and_then(Value::as_u64)
            .ok_or_else(|| protocol_failure_at(false, "attachment input node missing"))?;
        if input_node_id == 0 {
            return Err(pre_submission_failure(
                "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
                "Codex Desktop 图片附件临时输入控件创建失败。",
            ));
        }
        session.request(
            55,
            "DOM.setFileInputFiles",
            json!({
                "nodeId": input_node_id,
                "files": attachment_paths
            }),
            false,
        )?;
        let dispatched = session.evaluate(
            54,
            &format!(
                r#"(() => {{
                    const input = document.getElementById({input_id_json});
                    const composer = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                      .find((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                    if (!input || !composer) return false;
                    const transfer = new DataTransfer();
                    for (const file of input.files) transfer.items.add(file);
                    if (transfer.files.length !== input.files.length || transfer.files.length === 0) return false;
                    const rect = composer.getBoundingClientRect();
                    const eventOptions = {{
                      bubbles: true,
                      cancelable: true,
                      composed: true,
                      dataTransfer: transfer,
                      shiftKey: true,
                      clientX: Math.round(rect.left + rect.width / 2),
                      clientY: Math.round(rect.top + rect.height / 2),
                    }};
                    composer.focus();
                    for (const type of ['dragenter', 'dragover', 'drop']) {{
                      composer.dispatchEvent(new DragEvent(type, eventOptions));
                    }}
                    return true;
                }})()"#
            ),
            false,
        )?;
        if dispatched.as_bool() != Some(true) {
            return Err(pre_submission_failure(
                "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
                "Codex Desktop 未接收图片附件拖拽事件。",
            ));
        }
        let expected_attachment_count = attachment_images_before + attachments.len() as u64;
        let confirmed = session.evaluate(
            53,
            &format!(
                r#"(() => new Promise((resolve) => {{
                    const expected = {expected_attachment_count};
                    let attempts = 0;
                    const timer = setInterval(() => {{
                      attempts += 1;
                      const attachmentImages = document.querySelectorAll(
                        'img[src^="blob:"], img[src^="app://fs/"], '
                        + '.composer-attachment-surface img[alt="User attachment"], '
                        + '.composer-attachment-surface img[alt="用户附件"]'
                      ).length;
                      const pending = document.body.innerText.includes('正在上传') || document.body.innerText.includes('Uploading');
                      if ((attachmentImages >= expected && !pending) || attempts >= 50) {{
                        clearInterval(timer);
                        resolve(attachmentImages >= expected && !pending);
                      }}
                    }}, 100);
                }}))()"#
            ),
            false,
        )?;
        let _ = session.evaluate(
            52,
            &format!(
                r#"(() => {{
                    document.getElementById({input_id_json})?.remove();
                    return true;
                }})()"#
            ),
            false,
        );
        if confirmed.as_bool() != Some(true) {
            return Err(pre_submission_failure(
                "CODEX_CDP_ATTACHMENT_INPUT_MISSING",
                "Codex Desktop 当前输入框未确认图片附件预览。",
            ));
        }
    }
    if let Err(failure) = session.request(60, "Input.insertText", json!({"text": prompt}), false) {
        return Err(if session.clear_composer().is_ok() {
            failure
        } else {
            protocol_failure_at(true, "composer cleanup failed after insert error")
        });
    }
    let composer_check = session.evaluate(
        61,
        &format!(
            r#"(() => {{
                const readComposerText = (composer) => {{
                    const blocks = Array.from(composer.childNodes)
                      .filter((node) => node instanceof HTMLElement && (node.matches('p,div') || node.getClientRects().length > 0));
                    if (blocks.length === 0) return composer.textContent || '';
                    return blocks.map((block) => block.textContent || '').join('\n');
                }};
                const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                return composers.length === 1 && readComposerText(composers[0]) === {};
            }})()"#,
            serde_json::to_string(prompt).map_err(|_| pre_submission_failure(
                "CODEX_CDP_INPUT_INVALID",
                "无法校验 Codex Desktop 任务内容。",
            ))?
        ),
        false,
    );
    if !matches!(composer_check, Ok(Value::Bool(true))) {
        if session.clear_composer().is_err() {
            return Err(protocol_failure_at(
                true,
                "composer cleanup failed after write check",
            ));
        }
        return Err(pre_submission_failure(
            "CODEX_CDP_COMPOSER_WRITE_FAILED",
            "Codex Desktop 输入框未确认完整任务内容。",
        ));
    }
    let submitted_at_ms = current_unix_millis();
    if before_enter(submitted_at_ms).is_err() {
        return Err(if session.clear_composer().is_ok() {
            pre_submission_failure(
                "CODEX_CDP_SUBMISSION_PERSIST_FAILED",
                "无法在发送前持久化 Codex Desktop 提交阶段。",
            )
        } else {
            protocol_failure_at(true, "composer cleanup failed after persist error")
        });
    }
    session.evaluate(
        62,
        r#"(() => {
            const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
              .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
            const composer = composers.length === 1 ? composers[0] : null;
            if (!composer) return false;
            composer?.focus();
            const range = document.createRange();
            range.selectNodeContents(composer);
            range.collapse(false);
            const selection = window.getSelection();
            selection?.removeAllRanges();
            selection?.addRange(range);
            return document.activeElement === composer;
        })()"#,
        true,
    )?;
    session.request(
        63,
        "Input.dispatchKeyEvent",
        json!({
            "type": "rawKeyDown",
            "key": "Enter",
            "code": "Enter",
            "windowsVirtualKeyCode": 13,
            "nativeVirtualKeyCode": 36,
            "macCharCode": 13,
            "unmodifiedText": "\r",
            "text": "\r"
        }),
        true,
    )?;
    session.request(
        64,
        "Input.dispatchKeyEvent",
        json!({
            "type": "keyUp",
            "key": "Enter",
            "code": "Enter",
            "windowsVirtualKeyCode": 13,
            "nativeVirtualKeyCode": 36
        }),
        true,
    )?;
    let mut thread_id = String::new();
    for attempt in 0..COMPOSER_READY_ATTEMPTS {
        let value = session.evaluate(
            70 + attempt as u64,
            r#"(() => {
                const active = document.querySelector('[data-app-action-sidebar-thread-active="true"]');
                const routeMatch = window.location.pathname.match(/^\/local\/([^/?#]+)/);
                const id = active?.getAttribute('data-app-action-sidebar-thread-id') || routeMatch?.[1] || '';
                return id && !id.startsWith('local:client-new-thread:') ? id : '';
            })()"#,
            true,
        )?;
        if let Some(value) = value.as_str().filter(|value| !value.trim().is_empty()) {
            thread_id = value.to_string();
            break;
        }
        std::thread::sleep(COMPOSER_READY_INTERVAL);
    }
    Ok(CodexSubmissionReceipt {
        submitted_at_ms,
        thread_id: (!thread_id.is_empty()).then_some(thread_id),
    })
}

/// 有界等待 Codex 主进程把 canonical root 创建为本地项目并设为当前选择。
/// 流程：定位既有 CODEX_HOME 全局状态文件，在固定四秒窗口内只读解析；短暂不存在或 JSON 中间态继续等待，精确命中立即返回。
/// 参数：workspace_path 必须是上层已 canonicalize 的绝对目录；返回成功表示后续 New Chat 可绑定该工作空间。
/// 异常/边界：超限、符号链接、selected 记录内重复 root 或结构越界立即失败；超时也失败，绝不按 basename、contains 或最近项目猜测。
fn wait_for_selected_workspace(workspace_path: &str) -> Result<(), CodexCdpFailure> {
    let state_path = crate::codex_home_dir()
        .map_err(|_| workspace_state_failure())?
        .join(".codex-global-state.json");
    for attempt in 0..WORKSPACE_SELECTION_ATTEMPTS {
        if read_selected_workspace_state(&state_path, workspace_path)? {
            return Ok(());
        }
        if attempt + 1 < WORKSPACE_SELECTION_ATTEMPTS {
            std::thread::sleep(WORKSPACE_SELECTION_INTERVAL);
        }
    }
    Err(workspace_state_failure())
}

/// 从单个 Codex 全局状态文件句柄读取并确认当前工作空间。
/// 流程：拒绝最终路径符号链接，使用 O_NOFOLLOW 打开后复核普通文件和长度，以 take 上限加一读取，再交给纯函数做唯一 root 判断。
/// 参数：path 为固定 CODEX_HOME 状态文件，workspace_path 为 canonical root；返回 false 表示文件暂缺、读取中间态或尚未选中。
/// 异常/边界：文件替换竞态由句柄元数据约束；权限/IO、超限和非普通文件显式失败，解析错误作为可能的原子替换中间态交给有界轮询。
fn read_selected_workspace_state(
    path: &Path,
    workspace_path: &str,
) -> Result<bool, CodexCdpFailure> {
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(workspace_state_failure()),
    };
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(workspace_state_failure());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(workspace_state_failure()),
    };
    let metadata = file.metadata().map_err(|_| workspace_state_failure())?;
    if !metadata.is_file() || metadata.len() > CODEX_GLOBAL_STATE_MAX_BYTES {
        return Err(workspace_state_failure());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take(CODEX_GLOBAL_STATE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| workspace_state_failure())?;
    if bytes.len() as u64 > CODEX_GLOBAL_STATE_MAX_BYTES {
        return Err(workspace_state_failure());
    }
    let state = match serde_json::from_slice::<CodexWorkspaceGlobalState>(&bytes) {
        Ok(state) => state,
        Err(_) => return Ok(false),
    };
    selected_workspace_matches(&state, workspace_path)
}

/// 纯函数确认全局状态是否唯一选中了目标 canonical root。
/// 流程：限制项目总量，先要求 selected 类型为 local，再只读取 selected projectId 对应记录，校验 map key/记录 ID 后统计该记录内目标 root 的精确出现次数。
/// 参数：state 为最小状态视图，workspace_path 为 canonical 绝对路径；返回是否已精确选中。
/// 异常/边界：selected 记录内目标 root 重复、项目 ID 不一致或集合超限均显式失败；其它项目存在同 root 不影响当前权威选择。
fn selected_workspace_matches(
    state: &CodexWorkspaceGlobalState,
    workspace_path: &str,
) -> Result<bool, CodexCdpFailure> {
    if workspace_path.is_empty()
        || !Path::new(workspace_path).is_absolute()
        || state.local_projects.len() > LOCAL_PROJECT_LIMIT
    {
        return Err(workspace_state_failure());
    }
    if state.selected_project.project_type != "local"
        || state.selected_project.project_id.is_empty()
    {
        return Ok(false);
    }
    let Some(project) = state.local_projects.get(&state.selected_project.project_id) else {
        return Ok(false);
    };
    if project.id != state.selected_project.project_id
        || project.root_paths.len() > LOCAL_PROJECT_ROOT_LIMIT
    {
        return Err(workspace_state_failure());
    }
    Ok(project
        .root_paths
        .iter()
        .filter(|root| root.as_str() == workspace_path)
        .count()
        == 1)
}

/// 构造工作空间状态确认失败。
/// 流程：只返回固定错误码和安全文案；参数无；返回发生在输入前、允许人工修复后重新排队的确定失败。
/// 异常/边界：不包含 CODEX_HOME、canonical root、项目 ID、状态正文或文件元数据。
fn workspace_state_failure() -> CodexCdpFailure {
    pre_submission_failure(
        "CODEX_CDP_WORKSPACE_SWITCH_FAILED",
        "Codex Desktop 未确认精确工作空间切换。",
    )
}

/// 单个可信 Codex page 的 CDP WebSocket 会话。
struct CdpSession {
    /// 已设置读写 deadline 的固定回环 WebSocket。
    socket: tungstenite::WebSocket<TcpStream>,
}

impl CdpSession {
    /// 连接唯一可信 Codex renderer。
    /// 流程：读取内部 target，严格校验固定回环 URL，设置 TCP deadline 和固定 Origin，再完成 WebSocket 握手；参数无；返回会话。
    /// 异常/边界：任何地址或握手异常均发生在提交前，不向错误文案泄露内部端点。
    fn connect() -> Result<Self, CodexCdpFailure> {
        let websocket_url = codex_page_websocket_url()
            .map_err(|_| {
                pre_submission_failure(
                    "CODEX_CDP_TARGET_CHECK_FAILED",
                    "无法验证 Codex Desktop 主页面。",
                )
            })?
            .ok_or_else(|| {
                pre_submission_failure("CODEX_NOT_CONNECTED", "Codex Desktop 尚未建立本机连接。")
            })?;
        let parsed_url = validate_websocket_url(&websocket_url)?;
        let stream = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CDP_PORT),
            CDP_IO_TIMEOUT,
        )
        .map_err(|_| {
            pre_submission_failure(
                "CODEX_CDP_CONNECT_FAILED",
                "无法连接 Codex Desktop 主页面。",
            )
        })?;
        stream
            .set_read_timeout(Some(CDP_IO_TIMEOUT))
            .and_then(|_| stream.set_write_timeout(Some(CDP_IO_TIMEOUT)))
            .map_err(|_| {
                pre_submission_failure(
                    "CODEX_CDP_CONNECT_FAILED",
                    "无法设置 Codex Desktop 连接时限。",
                )
            })?;
        let mut request = parsed_url.as_str().into_client_request().map_err(|_| {
            pre_submission_failure("CODEX_CDP_CONNECT_FAILED", "Codex Desktop 连接请求无效。")
        })?;
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static("http://127.0.0.1:9333"));
        let (socket, _) = client(request, stream).map_err(|_| {
            pre_submission_failure("CODEX_CDP_CONNECT_FAILED", "Codex Desktop 主页面拒绝连接。")
        })?;
        Ok(Self { socket })
    }

    /// 执行一条 Runtime.evaluate 并读取 by-value 结果。
    /// 流程：使用固定 awaitPromise/returnByValue 参数调用 request，再取 result.result.value；参数为协议 ID、表达式和不确定边界；返回 JSON 值。
    /// 异常/边界：不记录或回显表达式；缺少 value 视为协议失败，并沿用调用方指定的提交边界分类。
    fn evaluate(
        &mut self,
        id: u64,
        expression: &str,
        submission_boundary: bool,
    ) -> Result<Value, CodexCdpFailure> {
        let response = self.request(
            id,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "awaitPromise": true,
                "returnByValue": true,
                "userGesture": true
            }),
            submission_boundary,
        )?;
        response
            .pointer("/result/result/value")
            .cloned()
            .ok_or_else(|| protocol_failure_at(submission_boundary, "evaluate value missing"))
    }

    /// 清理已写入但尚未按 Enter 的 composer 内容。
    /// 流程：在仍聚焦的唯一 composer 中发送 Meta+A 和 Backspace，再精确确认 textContent 为空；参数无；返回是否清理完成。
    /// 异常/边界：任一协议或校验失败都返回不确定错误，调用方必须把任务标记 sendUncertain，避免遗留 prompt 被用户误发后又自动重排。
    fn clear_composer(&mut self) -> Result<(), CodexCdpFailure> {
        for (id, params) in [
            (
                64,
                json!({"type":"keyDown","key":"a","code":"KeyA","modifiers":4}),
            ),
            (
                65,
                json!({"type":"keyUp","key":"a","code":"KeyA","modifiers":4}),
            ),
            (
                66,
                json!({"type":"keyDown","key":"Backspace","code":"Backspace","windowsVirtualKeyCode":8,"nativeVirtualKeyCode":51}),
            ),
            (
                67,
                json!({"type":"keyUp","key":"Backspace","code":"Backspace","windowsVirtualKeyCode":8,"nativeVirtualKeyCode":51}),
            ),
        ] {
            self.request(id, "Input.dispatchKeyEvent", params, true)?;
        }
        let cleared = self.evaluate(
            68,
            r#"(() => {
                const composers = Array.from(document.querySelectorAll('[data-codex-composer="true"][contenteditable="true"]'))
                  .filter((element) => element instanceof HTMLElement && element.getClientRects().length > 0);
                return composers.length === 1 && (composers[0].textContent || '').trim() === '';
            })()"#,
            true,
        )?;
        if cleared.as_bool() == Some(true) {
            Ok(())
        } else {
            Err(protocol_failure_at(
                true,
                "composer clear verification failed",
            ))
        }
    }

    /// 发送一条 CDP 请求并读取同 ID 响应。
    /// 流程：序列化固定 method/params，完整写入 WebSocket，再忽略有限通知直到收到目标响应；参数含是否越过 Enter 边界；返回响应 JSON。
    /// 异常/边界：最多忽略固定数量页面通知，任何协议错误均不回显原始帧；Enter 请求写出后的所有失败统一标记不确定。
    fn request(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
        submission_boundary: bool,
    ) -> Result<Value, CodexCdpFailure> {
        let payload = serde_json::to_string(&json!({"id": id, "method": method, "params": params}))
            .map_err(|_| {
                protocol_failure_at(submission_boundary, "request serialization failed")
            })?;
        self.socket
            .send(Message::Text(payload.into()))
            .map_err(|_| protocol_failure_at(submission_boundary, "request send failed"))?;
        for _ in 0..CDP_RESPONSE_NOTIFICATION_LIMIT {
            let message = self
                .socket
                .read()
                .map_err(|_| protocol_failure_at(submission_boundary, "request read failed"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let value = serde_json::from_str::<Value>(&text)
                .map_err(|_| protocol_failure_at(submission_boundary, "response json invalid"))?;
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if value.get("error").is_some() {
                return Err(protocol_failure_at(
                    submission_boundary,
                    "response contains error",
                ));
            }
            return Ok(value);
        }
        Err(protocol_failure_at(
            submission_boundary,
            "response id not received",
        ))
    }
}

/// 校验 target 返回的 WebSocket 只能指向固定回环端口和 page 路径。
/// 流程：解析 URL 后精确检查协议、host、port、路径前缀以及空认证信息；参数为内部探针值；返回安全 URL。
/// 异常/边界：拒绝 IPv6、任意端口、query/fragment、用户名密码和非 page target，防止 CDP 客户端被引向其它服务。
fn validate_websocket_url(value: &str) -> Result<Url, CodexCdpFailure> {
    let url = Url::parse(value).map_err(|_| {
        pre_submission_failure("CODEX_CDP_TARGET_INVALID", "Codex Desktop 主页面地址无效。")
    })?;
    let trusted_host = matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    if url.scheme() != "ws"
        || !trusted_host
        || url.port() != Some(CODEX_CDP_PORT)
        || !url.path().starts_with("/devtools/page/")
        || url.path().trim_start_matches("/devtools/page/").is_empty()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(pre_submission_failure(
            "CODEX_CDP_TARGET_INVALID",
            "Codex Desktop 主页面地址不可信。",
        ));
    }
    Ok(url)
}

/// 构造协议失败，并按是否越过 Enter 边界决定可重试性。
/// 流程：提交前返回普通协议错误，提交后固定返回 CODEX_SEND_UNCERTAIN；参数为边界标记；返回安全错误。
/// 异常/边界：不接受底层帧或错误正文，防止泄露 DOM 和内部端点。
#[cfg(test)]
fn protocol_failure(submission_boundary: bool) -> CodexCdpFailure {
    protocol_failure_at(submission_boundary, "unknown")
}

/// 构造带安全阶段名的协议失败，帮助定位 CDP 状态机问题。
/// 流程：只拼接固定英文阶段，不包含 prompt、DOM、WebSocket、端口、cwd 或响应正文；提交后仍固定进入 send-uncertain 保护。
/// 参数：``submission_boundary`` 表示是否越过可能执行 Enter 的边界，``stage`` 为代码内固定诊断阶段。
/// 返回：可向用户展示且不泄露敏感内容的错误。
/// 异常/边界：调用方不得把外部错误正文传入 stage。
fn protocol_failure_at(submission_boundary: bool, stage: &'static str) -> CodexCdpFailure {
    if submission_boundary {
        CodexCdpFailure {
            code: "CODEX_SEND_UNCERTAIN",
            message: format!(
                "Codex Desktop 未返回可靠提交确认，发送结果无法确认（阶段：{}）。",
                stage
            ),
            submission_uncertain: true,
        }
    } else {
        let mut failure = pre_submission_failure(
            "CODEX_CDP_PROTOCOL_FAILED",
            "Codex Desktop 本机协议交互失败。",
        );
        failure.message = format!("Codex Desktop 本机协议交互失败（阶段：{}）。", stage);
        failure
    }
}

/// 构造发送前确定失败。
/// 流程：保存稳定码和安全文案；参数均不得包含业务正文；返回允许调用方按普通 failed 处理的错误。
/// 异常/边界：submission_uncertain 固定 false。
fn pre_submission_failure(code: &'static str, message: &str) -> CodexCdpFailure {
    CodexCdpFailure {
        code,
        message: message.to_string(),
        submission_uncertain: false,
    }
}

/// 把任务附件 data URL 写入临时图片文件。
/// 流程：解析 `data:<mime>;base64,<payload>`，按 MIME 选择扩展名，写入系统临时目录的唯一文件。
/// 参数：attachment 为已由 TaskStore 校验的图片附件。
/// 返回：可交给 CDP `DOM.setFileInputFiles` 的本地绝对路径。
/// 异常/边界：不在错误中回显 data URL 或文件内容；无法解码/写入时发生在 Enter 前，可安全失败。
fn write_attachment_temp_file(
    attachment: &TaskAttachmentRecord,
) -> Result<String, CodexCdpFailure> {
    let prefix = format!("data:{};base64,", attachment.mime_type);
    let Some(payload) = attachment.data_url.strip_prefix(&prefix) else {
        return Err(pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_INVALID",
            "任务图片附件格式无效。",
        ));
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| {
            pre_submission_failure("CODEX_CDP_ATTACHMENT_INVALID", "任务图片附件无法解码。")
        })?;
    let extension = match attachment.mime_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => {
            return Err(pre_submission_failure(
                "CODEX_CDP_ATTACHMENT_INVALID",
                "任务图片附件类型不支持。",
            ))
        }
    };
    let mut path = std::env::temp_dir();
    path.push(format!(
        "codexman-task-attachment-{}.{}",
        uuid::Uuid::new_v4(),
        extension
    ));
    fs::write(&path, bytes).map_err(|_| {
        pre_submission_failure(
            "CODEX_CDP_ATTACHMENT_WRITE_FAILED",
            "任务图片附件写入失败。",
        )
    })?;
    Ok(path.to_string_lossy().to_string())
}

/// 读取当前 Unix 毫秒水位。
/// 流程：计算系统时间与 Unix epoch 差并转换为 i64；参数无；返回 Enter 前候选过滤水位。
/// 异常/边界：系统时间异常或溢出时回落为 0，调用方持久化阶段会拒绝无效水位且不会发送。
fn current_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从最小 JSON 构造工作空间状态，避免测试依赖用户真实 Codex 配置。
    fn workspace_state(value: Value) -> CodexWorkspaceGlobalState {
        serde_json::from_value(value).expect("测试工作空间状态应合法")
    }

    /// WebSocket 地址必须精确限制到固定回环端口和 page target。
    #[test]
    fn websocket_url_validation_fails_closed() {
        assert!(validate_websocket_url("ws://127.0.0.1:9333/devtools/page/abc").is_ok());
        assert!(validate_websocket_url("ws://localhost:9333/devtools/page/abc").is_ok());
        for invalid in [
            "ws://127.0.0.1:9231/devtools/page/abc",
            "ws://192.168.1.2:9333/devtools/page/abc",
            "wss://127.0.0.1:9333/devtools/page/abc",
            "ws://127.0.0.1:9333/devtools/browser/abc",
            "ws://127.0.0.1:9333/devtools/page/abc?token=x",
        ] {
            assert!(validate_websocket_url(invalid).is_err());
        }
    }

    /// Enter 后的协议失败必须稳定进入不可重排状态。
    #[test]
    fn post_enter_protocol_failure_is_uncertain() {
        let failure = protocol_failure(true);
        assert_eq!(failure.code, "CODEX_SEND_UNCERTAIN");
        assert!(failure.submission_uncertain);
        assert!(!protocol_failure(false).submission_uncertain);
    }

    /// Unix 毫秒水位必须为可持久化正数。
    #[test]
    fn submission_watermark_is_positive() {
        assert!(current_unix_millis() > 0);
    }

    /// 只有 selected local project 自身精确包含一次 root 才通过；basename、其它选择和 selected 记录内重复 root 必须拒绝。
    #[test]
    fn workspace_state_selection_is_exact_and_unique() {
        let exact = workspace_state(json!({
            "selected-project": {"type": "local", "projectId": "p1"},
            "local-projects": {
                "p1": {"id": "p1", "rootPaths": ["/tmp/exact"]},
                "p2": {"id": "p2", "rootPaths": ["/tmp/other"]}
            }
        }));
        assert!(selected_workspace_matches(&exact, "/tmp/exact").expect("唯一精确选择应通过"));
        assert!(!selected_workspace_matches(&exact, "/other/exact").expect("同 basename 不得匹配"));

        let wrong_selected = workspace_state(json!({
            "selected-project": {"type": "local", "projectId": "p2"},
            "local-projects": {
                "p1": {"id": "p1", "rootPaths": ["/tmp/exact"]},
                "p2": {"id": "p2", "rootPaths": ["/tmp/other"]}
            }
        }));
        assert!(!selected_workspace_matches(&wrong_selected, "/tmp/exact")
            .expect("选择其它项目应继续等待"));

        let official_duplicate_root = workspace_state(json!({
            "selected-project": {"type": "local", "projectId": "p1"},
            "local-projects": {
                "p1": {"id": "p1", "rootPaths": ["/tmp/exact"]},
                "p2": {"id": "p2", "rootPaths": ["/tmp/exact"]}
            }
        }));
        assert!(
            selected_workspace_matches(&official_duplicate_root, "/tmp/exact")
                .expect("其它官方项目重复 root 不得阻断 selected 项目")
        );

        let duplicate_in_selected = workspace_state(json!({
            "selected-project": {"type": "local", "projectId": "p1"},
            "local-projects": {
                "p1": {"id": "p1", "rootPaths": ["/tmp/exact", "/tmp/exact"]}
            }
        }));
        assert!(
            !selected_workspace_matches(&duplicate_in_selected, "/tmp/exact")
                .expect("selected 记录内重复 root 不具备精确一次语义")
        );
    }

    /// 全局状态文件读取必须接受合法普通文件，把半写 JSON 当作可重试中间态，并拒绝最终路径符号链接。
    #[test]
    fn workspace_state_file_read_is_bounded_and_symlink_safe() {
        let directory = std::env::temp_dir().join(format!(
            "codexman-workspace-state-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&directory).expect("应创建测试目录");
        let state_path = directory.join("state.json");
        fs::write(
            &state_path,
            br#"{"selected-project":{"type":"local","projectId":"p1"},"local-projects":{"p1":{"id":"p1","rootPaths":["/tmp/exact"]}}}"#,
        )
        .expect("应写入合法状态");
        assert!(
            read_selected_workspace_state(&state_path, "/tmp/exact").expect("合法普通文件应读取")
        );
        fs::write(&state_path, b"{\"selected-project\":").expect("应写入模拟中间态");
        assert!(!read_selected_workspace_state(&state_path, "/tmp/exact")
            .expect("中间态应返回 false 供有界轮询"));

        #[cfg(unix)]
        {
            let target_path = directory.join("target.json");
            fs::write(&target_path, b"{}").expect("应写入符号链接目标");
            let link_path = directory.join("link.json");
            std::os::unix::fs::symlink(&target_path, &link_path).expect("应创建测试符号链接");
            assert!(read_selected_workspace_state(&link_path, "/tmp/exact").is_err());
        }
        fs::remove_dir_all(&directory).expect("应清理测试目录");
    }

    /// 生产提交链必须固定使用 workspace bridge、精确项目新聊天按钮、精确 composer、Input.insertText 和单次 Enter。
    #[test]
    fn cdp_submission_source_preserves_exact_mvp_contract() {
        let source = include_str!("codex_cdp.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let submit_start = production
            .find("pub(crate) fn submit_new_chat")
            .expect("生产提交函数必须存在");
        let submit_source = &production[submit_start..];
        let workspace_side_effect = submit_source
            .find("electron-set-active-workspace-root")
            .expect("必须保留工作空间 bridge");
        let new_chat_side_effect = submit_source
            .find("start new chat")
            .expect("必须保留项目行新聊天按钮");
        assert!(production.contains("electron-set-active-workspace-root"));
        assert!(!production.contains("electron-add-new-workspace-root-option"));
        assert!(!production.contains("electron-get-active-workspace-root"));
        assert!(production.contains(".codex-global-state.json"));
        assert!(production.contains("[data-codex-composer=\"true\"][contenteditable=\"true\"]"));
        assert!(production.contains("[data-app-action-sidebar-project-row]"));
        assert!(production.contains("开始新聊天"));
        assert!(production.contains("(composer.textContent || '').trim() !== ''"));
        assert!(production.contains("local:client-new-thread:"));
        assert!(production.contains("readComposerText(composers[0]) === {}"));
        assert!(!production.contains("CODEX_CDP_COMPOSER_NOT_EMPTY"));
        assert!(!production.contains("previous_draft"));
        assert!(!production.contains("selected_workspace_draft"));
        assert!(
            workspace_side_effect < new_chat_side_effect,
            "必须先通过 bridge 选择精确工作空间，再点击项目行新聊天按钮"
        );
        assert!(!production.contains("composer.innerText"));
        assert!(production.contains("\"Input.insertText\""));
        assert_eq!(production.matches("send.click()").count(), 0);
        assert_eq!(production.matches("nativeVirtualKeyCode\": 36").count(), 2);
    }
}
