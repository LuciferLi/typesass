use std::env;
use std::fs;
#[cfg(not(target_os = "macos"))]
use std::io::Write;
use std::path::PathBuf;
#[cfg(not(target_os = "macos"))]
use std::process::Stdio;
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Position, Size, State};

#[cfg(target_os = "macos")]
use objc2::rc::{autoreleasepool, Retained};
#[cfg(target_os = "macos")]
use objc2::runtime::ProtocolObject;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSArray, NSData, NSString};

const DEFAULT_BASE_URL: &str = "https://token-plan-cn.xiaomimimo.com/v1";
const DEFAULT_ASR_MODEL: &str = "mimo-v2.5-asr";
const DEFAULT_TEXT_MODEL: &str = "mimo-v2.5";
const DEFAULT_DICTATE_SHORTCUT: &str = "ctrl+p";
const DEFAULT_TRANSLATE_SHORTCUT: &str = "ctrl+t";
const DEFAULT_ASK_SHORTCUT: &str = "ctrl+space";
const DEFAULT_POLISH_SHORTCUT: &str = "ctrl+shift+p";
const DEFAULT_SUBTITLE_SHORTCUT: &str = "ctrl+shift+s";
const LOGIN_AGENT_LABEL: &str = "asia.aijob.aitool.login";
const KEYCHAIN_SERVICE: &str = "asia.aijob.aitool";
const KEYCHAIN_ACCOUNT: &str = "mimo-api-key";
const FLOAT_WINDOW_WIDTH: f64 = 132.0;
const FLOAT_WINDOW_TOP: f64 = 60.0;
const TOAST_WINDOW_WIDTH: f64 = 460.0;
const TOAST_WINDOW_HEIGHT: f64 = 86.0;
const TOAST_WINDOW_TOP: f64 = 42.0;
const RESULT_WINDOW_WIDTH: f64 = 520.0;
const RESULT_WINDOW_HEIGHT: f64 = 320.0;
const RESULT_WINDOW_TOP: f64 = 76.0;
const RESULT_TOAST_GAP: f64 = 12.0;
const SUBTITLE_WINDOW_WIDTH: f64 = 1000.0;
const SUBTITLE_WINDOW_HEIGHT: f64 = 170.0;
const SUBTITLE_WINDOW_BOTTOM: f64 = 54.0;
const SUBTITLE_HISTORY_WINDOW_WIDTH: f64 = 360.0;
const SUBTITLE_HISTORY_WINDOW_HEIGHT: f64 = 460.0;
const SUBTITLE_HISTORY_WINDOW_TOP: f64 = 72.0;
const SUBTITLE_HISTORY_WINDOW_RIGHT: f64 = 28.0;
const CLIPBOARD_VERIFY_INITIAL_DELAY_MS: u64 = 30;
const CLIPBOARD_VERIFY_RETRY_STEP_MS: u64 = 80;
const CLIPBOARD_RESTORE_DELAY_MS: u64 = 80;
const PASTE_DIAGNOSTIC_SETTLE_DELAY_MS: u64 = 30;

/// 运行期间保存的敏感配置，只放内存，不写入本地文件。
#[derive(Default)]
struct RuntimeSecrets {
    /// 当前会话的小米 Mimo 接口密钥。
    api_key: Mutex<String>,
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

/// 运行期间保存最近一段原生系统音频字幕结果，避免长耗时命令阻塞 WebView。
#[derive(Default)]
struct RuntimeSubtitleTranscribe {
    /// 最近一次原生字幕转写任务的完成结果；前端按片段序号轮询并消费。
    payload: Mutex<Option<ProcessTapTranscribeOutcome>>,
}

/// 前端提交的全局模式快捷键。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutProfile {
    /// 听写模式快捷键。
    dictate: String,
    /// 翻译模式快捷键。
    translate: String,
    /// 随便问模式快捷键。
    ask: String,
    /// 文本润色模式快捷键。
    polish: String,
    /// 实时字幕监听模式快捷键。
    subtitle: String,
}

impl Default for ShortcutProfile {
    fn default() -> Self {
        Self {
            dictate: DEFAULT_DICTATE_SHORTCUT.to_string(),
            translate: DEFAULT_TRANSLATE_SHORTCUT.to_string(),
            ask: DEFAULT_ASK_SHORTCUT.to_string(),
            polish: DEFAULT_POLISH_SHORTCUT.to_string(),
            subtitle: DEFAULT_SUBTITLE_SHORTCUT.to_string(),
        }
    }
}

/// 前端提交给 Mimo ASR 的语音转写请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscribeRequest {
    /// 小米 Mimo 接口密钥；为空时从会话内存或环境变量读取。
    api_key: String,
    /// OpenAI 兼容接口地址。
    base_url: String,
    /// 语音识别模型名称。
    asr_model: String,
    /// 语音识别语言，auto 表示自动识别。
    language: String,
    /// 音频 MIME 类型。
    content_type: String,
    /// 音频 base64 内容，不包含 data URL 头。
    audio_base64: String,
}

/// 返回给前端的语音转写结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscribeResponse {
    /// 转写后的文字。
    text: String,
    /// 服务端统计的转写耗时。
    elapsed_ms: u128,
    /// 实际返回的模型名称。
    model: String,
}

/// 前端请求原生系统音频片段的参数。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTapCaptureRequest {
    /// 本次采集的目标音频进程名称、Bundle ID 或 PID；为空时由 helper 自动选择活跃进程。
    target_keyword: String,
    /// 单个字幕切片采集时长，毫秒。
    duration_ms: u64,
}

/// 原生系统音频片段采集结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTapCaptureResponse {
    /// WAV 音频 base64 内容，不包含 data URL 头。
    audio_base64: String,
    /// 音频 MIME 类型，固定为 ASR 可识别的 WAV。
    content_type: String,
    /// 采集到的音频字节数。
    bytes: u64,
    /// helper 输出的采集摘要，用于诊断日志展示目标进程和采样帧数。
    summary: String,
    /// 本地采集和文件读取总耗时。
    elapsed_ms: u128,
}

/// 当前可被 Core Audio Process Tap 发现的音频 App。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTapAudioApp {
    /// 音频进程 PID，可作为精确采集目标。
    pid: i32,
    /// App 或进程名称。
    name: String,
    /// App Bundle ID，部分系统进程可能为空。
    bundle_id: String,
    /// Core Audio 标记该进程是否正在运行音频。
    audio_active: bool,
}

/// 前端请求原生系统音频并直接转写的参数。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTapTranscribeRequest {
    /// 前端字幕片段序号，用于事件回传后按片段归因日志。
    chunk_index: u64,
    /// 本次采集的目标音频进程名称、Bundle ID 或 PID；为空时由 helper 自动选择活跃进程。
    target_keyword: String,
    /// 单个字幕切片采集时长，毫秒。
    duration_ms: u64,
    /// 可选的请求密钥；为空时读取会话密钥、钥匙串或环境变量。
    api_key: String,
    /// OpenAI 兼容接口地址。
    base_url: String,
    /// 语音识别模型名称。
    asr_model: String,
    /// 语音识别语言，auto 表示自动识别。
    language: String,
}

/// 原生系统音频直接转写结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTapTranscribeResponse {
    /// 前端字幕片段序号，用于事件回传后按片段归因日志。
    chunk_index: u64,
    /// 转写后的文字。
    text: String,
    /// 实际返回的模型名称。
    model: String,
    /// ASR 请求耗时，毫秒。
    elapsed_ms: u128,
    /// 本地采集和文件读取总耗时，毫秒。
    capture_elapsed_ms: u128,
    /// 采集到的音频字节数。
    bytes: u64,
    /// helper 输出的采集摘要，用于诊断日志展示目标进程和采样帧数。
    summary: String,
}

/// 原生系统音频后台转写任务完成结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTapTranscribeOutcome {
    /// 前端字幕片段序号，用于轮询时过滤旧结果。
    chunk_index: u64,
    /// 任务是否成功。
    ok: bool,
    /// 成功时的转写结果。
    response: Option<ProcessTapTranscribeResponse>,
    /// 失败时的错误原因。
    error: Option<String>,
}

/// 原生系统音频采集后的内存音频片段。
struct ProcessTapCapturedAudio {
    /// WAV 音频二进制内容。
    audio: Vec<u8>,
    /// 音频 MIME 类型，固定为 ASR 可识别的 WAV。
    content_type: String,
    /// helper 输出的采集摘要，用于诊断日志展示目标进程和采样帧数。
    summary: String,
    /// 本地采集和文件读取总耗时。
    elapsed_ms: u128,
}

/// AI 文本处理模式。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ProcessMode {
    /// 听写整理。
    Dictate,
    /// 翻译。
    Translate,
    /// 随便问。
    Ask,
    /// 选中文本润色。
    Polish,
}

/// 前端提交给 AI 文本处理接口的请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTextRequest {
    /// 小米 Mimo 接口密钥；为空时从会话内存或环境变量读取。
    api_key: String,
    /// OpenAI 兼容接口地址。
    base_url: String,
    /// 文本处理模型名称。
    text_model: String,
    /// 文本处理模式。
    mode: ProcessMode,
    /// ASR 原文或用户输入文本。
    text: String,
    /// 词典术语列表，用于提升专有名词保真度。
    dictionary: Vec<String>,
    /// 翻译目标语言列表。
    target_languages: Vec<String>,
    /// 录音触发时的前台应用名称。
    context_app: String,
    /// 本地个性化输出偏好。
    style_instruction: String,
}

/// AI 文本处理后的响应。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTextResponse {
    /// 处理后的文本。
    processed_text: String,
    /// 服务端统计的处理耗时。
    elapsed_ms: u128,
    /// 实际返回的模型名称。
    model: String,
}

/// 自动粘贴命令的执行结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PasteResponse {
    /// 是否已成功发出系统粘贴指令；macOS 不提供可靠的目标输入框插入回调。
    pasted: bool,
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
    /// 隐藏 typesass 窗口前的系统前台应用。
    frontmost_before_paste: String,
    /// 尝试激活目标应用后的系统前台应用。
    frontmost_after_activate: String,
    /// 发送粘贴指令后的系统前台应用。
    frontmost_after_paste: String,
    /// 是否已从目标输入框文本中确认本次输出。
    insertion_verified: bool,
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
    /// 当前会话内存里是否已有 Mimo Key。
    has_session_api_key: bool,
    /// macOS 钥匙串里是否已保存 Mimo Key。
    has_keychain_api_key: bool,
    /// 启动环境变量里是否已有 Mimo Key。
    has_env_api_key: bool,
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
        .manage(RuntimeSecrets::default())
        .manage(RuntimeShortcuts::default())
        .manage(RuntimeResult::default())
        .manage(RuntimeSubtitleTranscribe::default())
        .setup(|app| {
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
            transcribe_audio,
            list_process_tap_audio_apps,
            capture_process_tap_audio,
            capture_process_tap_transcribe,
            start_process_tap_transcribe_task,
            take_process_tap_transcribe_outcome,
            process_text,
            read_selected_text,
            paste_text,
            set_session_api_key,
            save_api_key,
            clear_saved_api_key,
            show_main_window,
            hide_main_window,
            show_hub_window,
            hide_hub_window,
            show_error_bubble,
            hide_toast_window,
            show_result_window,
            hide_result_window,
            show_subtitle_windows,
            hide_subtitle_windows,
            toggle_subtitle_mode,
            get_last_result_window_payload,
            register_shortcuts,
            suspend_shortcuts_for_recording,
            get_runtime_diagnostics,
            open_accessibility_settings,
            open_microphone_settings,
            set_login_launch,
            get_login_launch,
            set_dock_visible,
            get_frontmost_app,
            set_system_output_muted
        ])
        .build(tauri::generate_context!())
        .expect("启动 typesass 失败")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                let _ = present_window(app, "hub", false);
            }
        });
}

/// 创建系统托盘菜单，并把常用动作接到桌面端命令。
#[cfg(desktop)]
fn configure_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::image::Image;
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::tray::TrayIconBuilder;

    let open_home = MenuItem::with_id(app, "open_home", "打开 typesass 主页", true, None::<&str>)?;
    let open_history = MenuItem::with_id(app, "open_history", "显示历史记录", true, None::<&str>)?;
    let add_dictionary_word = MenuItem::with_id(
        app,
        "add_dictionary_word",
        "将词汇添加到词典",
        true,
        None::<&str>,
    )?;
    let open_settings = MenuItem::with_id(app, "open_settings", "设置...", true, Some("Cmd+,"))?;
    let microphone_default = MenuItem::with_id(
        app,
        "microphone_default",
        "系统默认麦克风",
        true,
        None::<&str>,
    )?;
    let microphone_settings = MenuItem::with_id(
        app,
        "microphone_settings",
        "打开麦克风设置",
        true,
        None::<&str>,
    )?;
    let microphone_refresh = MenuItem::with_id(
        app,
        "microphone_refresh",
        "刷新麦克风列表",
        true,
        None::<&str>,
    )?;
    let microphone_separator = PredefinedMenuItem::separator(app)?;
    let microphone_menu = Submenu::with_id_and_items(
        app,
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
    let version = MenuItem::with_id(
        app,
        "version",
        format!("版本 {}", app.package_info().version),
        false,
        None::<&str>,
    )?;
    let check_updates = MenuItem::with_id(app, "check_updates", "检查更新...", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 typesass", true, Some("Cmd+Q"))?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open_home,
            &open_history,
            &add_dictionary_word,
            &first_separator,
            &open_settings,
            &microphone_menu,
            &second_separator,
            &version,
            &check_updates,
            &quit,
        ],
    )?;
    let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(false)
        .tooltip("typesass")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_home" => present_hub_view(app, "home"),
            "open_history" => present_hub_view(app, "history"),
            "add_dictionary_word" => add_clipboard_words_to_dictionary(app),
            "open_settings" => present_hub_view(app, "settings"),
            "microphone_default" | "microphone_settings" => present_hub_view(app, "settings"),
            "microphone_refresh" => {
                present_hub_view(app, "settings");
                emit_hub_event(app.clone(), "hub-refresh-microphones", String::new());
            }
            "check_updates" => show_update_status(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
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

/// 根据当前前台 App 计算粘贴行为；请求目标只用于兼容旧调用，不参与恢复或激活。
fn resolve_paste_target(_requested_target_app: &str, frontmost_app: &str) -> PasteTargetDecision {
    let normalized_frontmost_app = normalize_target_app_name(frontmost_app);
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

/// 从剪贴板读取词汇并交给 Hub 写入本地词典。
#[cfg(desktop)]
fn add_clipboard_words_to_dictionary(app: &AppHandle) {
    present_hub_view(app, "dictionary");
    match read_clipboard_text().map(|text| split_dictionary_words(&text)) {
        Ok(words) if !words.is_empty() => {
            let app_handle = app.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(160));
                let _ = app_handle.emit_to("hub", "hub-add-dictionary-words", words);
            });
        }
        Ok(_) => emit_hub_notice(app, "剪贴板里没有可加入词典的词汇。", "error"),
        Err(error) => emit_hub_notice(app, &format!("读取剪贴板失败：{}", error), "error"),
    }
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

/// 根据全局快捷键字符串判断目标模式，并通知悬浮窗开始或停止。
fn trigger_voice_shortcut(app: tauri::AppHandle, shortcut: String) {
    let mode = shortcut_to_mode(&app, &shortcut);
    if mode == "subtitle" {
        trigger_subtitle_mode(app);
        return;
    }
    trigger_voice_mode(app, &mode);
}

/// 按实时字幕快捷键进入或退出字幕监听模式，交给可见 Hub WebView 采集音频。
fn trigger_subtitle_mode(app: tauri::AppHandle) {
    if let Some(result) = app.get_webview_window("result") {
        let _ = result.hide();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    let _ = present_window(&app, "hub", false);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(180));
        let payload = json!({
            "mode": "subtitle",
            "targetApp": "",
            "keepHubVisible": true
        });
        let _ = app.emit_to("hub", "hub-start-mode", payload);
    });
}

/// 按指定模式通知悬浮录音条开始或停止。
fn trigger_voice_mode(app: tauri::AppHandle, mode: &str) {
    if mode.trim().is_empty() {
        return;
    }
    let frontmost_app = get_frontmost_app().unwrap_or_default();
    let context = resolve_voice_trigger_context(&frontmost_app);
    if let Some(result) = app.get_webview_window("result") {
        let _ = result.hide();
    }
    if context.show_floating_window {
        let _ = present_window(&app, "main", true);
    } else if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    let mode = mode.to_string();
    let target_app = context.target_app;
    let keep_hub_visible = context.keep_hub_visible;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(180));
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

/// 把快捷键字符串转换成 typesass 的语音模式。
fn shortcut_to_mode(app: &tauri::AppHandle, shortcut: &str) -> String {
    let normalized = normalize_shortcut(shortcut);
    let profile = app
        .state::<RuntimeShortcuts>()
        .profile
        .lock()
        .map(|profile| profile.clone())
        .unwrap_or_default();
    if normalized == normalize_shortcut(&profile.translate) {
        "translate".to_string()
    } else if normalized == normalize_shortcut(&profile.ask) {
        "ask".to_string()
    } else if normalized == normalize_shortcut(&profile.polish) {
        "polish".to_string()
    } else if normalized == normalize_shortcut(&profile.subtitle) {
        "subtitle".to_string()
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
#[tauri::command]
fn register_shortcuts(
    app: tauri::AppHandle,
    shortcuts: ShortcutProfile,
    state: State<'_, RuntimeShortcuts>,
) -> Result<ShortcutProfile, String> {
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
#[tauri::command]
fn suspend_shortcuts_for_recording(app: tauri::AppHandle) -> Result<(), String> {
    suspend_shortcut_profile(&app)
}

/// 读取当前桌面端能力状态，供设置页展示真实诊断结果。
#[tauri::command]
fn get_runtime_diagnostics(
    secrets: State<'_, RuntimeSecrets>,
    shortcuts: State<'_, RuntimeShortcuts>,
) -> Result<RuntimeDiagnostics, String> {
    let session_key_ready = secrets
        .api_key
        .lock()
        .map_err(|_| "读取会话密钥失败：状态锁已损坏".to_string())?
        .trim()
        .is_empty()
        == false;
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
        has_session_api_key: session_key_ready,
        has_keychain_api_key: read_keychain_api_key()
            .map(|api_key| api_key.is_some())
            .unwrap_or(false),
        has_env_api_key: env::var("MIMO_API_KEY")
            .map(|value| value.trim().is_empty() == false)
            .unwrap_or(false),
        accessibility_trusted: is_accessibility_trusted(),
        shortcuts: profile,
        shortcut_registration_ready: shortcut_registration_status.ready,
        shortcut_registration_message: shortcut_registration_status.message,
    })
}

/// 打开 macOS 辅助功能设置，用于授予自动粘贴权限。
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    open_accessibility_preferences()
}

/// 打开 macOS 麦克风权限设置，用于授予语音采集权限。
#[tauri::command]
fn open_microphone_settings() -> Result<(), String> {
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
        profile.dictate.as_str(),
        profile.translate.as_str(),
        profile.ask.as_str(),
        profile.polish.as_str(),
        profile.subtitle.as_str(),
    ] {
        let normalized = normalize_shortcut(shortcut);
        if seen.insert(normalized) {
            shortcuts.push(shortcut.to_string());
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
        dictate: normalize_shortcut_or_default(&profile.dictate, DEFAULT_DICTATE_SHORTCUT),
        translate: normalize_shortcut_or_default(&profile.translate, DEFAULT_TRANSLATE_SHORTCUT),
        ask: normalize_shortcut_or_default(&profile.ask, DEFAULT_ASK_SHORTCUT),
        polish: normalize_shortcut_or_default(&profile.polish, DEFAULT_POLISH_SHORTCUT),
        subtitle: normalize_shortcut_or_default(&profile.subtitle, DEFAULT_SUBTITLE_SHORTCUT),
    };
    let mut seen = std::collections::HashSet::new();
    for shortcut in [
        &normalized.dictate,
        &normalized.translate,
        &normalized.ask,
        &normalized.polish,
        &normalized.subtitle,
    ] {
        let key = normalize_shortcut(shortcut);
        if !seen.insert(key) {
            return Err("五个模式不能使用同一个快捷键".to_string());
        }
    }
    Ok(normalized)
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

/// 保存当前会话的 Mimo Key 到内存，避免写入本地文件。
#[tauri::command]
fn set_session_api_key(secrets: State<'_, RuntimeSecrets>, api_key: String) -> Result<(), String> {
    let mut stored_key = secrets
        .api_key
        .lock()
        .map_err(|_| "保存会话密钥失败：状态锁已损坏".to_string())?;
    *stored_key = api_key.trim().to_string();
    Ok(())
}

/// 把 Mimo Key 保存到当前会话和 macOS 钥匙串，避免重启后重复输入。
#[tauri::command]
fn save_api_key(secrets: State<'_, RuntimeSecrets>, api_key: String) -> Result<(), String> {
    let normalized_key = api_key.trim();
    if normalized_key.is_empty() {
        return Err("Mimo Key 不能为空".to_string());
    }
    write_keychain_api_key(normalized_key)?;
    set_session_api_key(secrets, normalized_key.to_string())
}

/// 清除当前会话和 macOS 钥匙串中的 Mimo Key。
#[tauri::command]
fn clear_saved_api_key(secrets: State<'_, RuntimeSecrets>) -> Result<(), String> {
    set_session_api_key(secrets, String::new())?;
    delete_keychain_api_key()
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
    let _ = result.eval(&format!(
        "if (window.__AIToolRenderResult) window.__AIToolRenderResult({});",
        payload_json
    ));
    result
        .emit("result-message", payload)
        .map_err(|error| format!("发送结果内容失败：{}", error))
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

/// 显示实时字幕底部窗口和右上角历史窗口。
#[tauri::command]
fn show_subtitle_windows(app: tauri::AppHandle) -> Result<(), String> {
    let subtitle = app
        .get_webview_window("subtitle")
        .ok_or_else(|| "未找到实时字幕窗口".to_string())?;
    position_subtitle_window(&app, &subtitle)?;
    subtitle
        .set_always_on_top(true)
        .map_err(|error| format!("设置实时字幕置顶失败：{}", error))?;
    subtitle
        .show()
        .map_err(|error| format!("显示实时字幕窗口失败：{}", error))?;

    let history = app
        .get_webview_window("subtitleHistory")
        .ok_or_else(|| "未找到字幕历史窗口".to_string())?;
    position_subtitle_history_window(&app, &history)?;
    history
        .set_always_on_top(true)
        .map_err(|error| format!("设置字幕历史置顶失败：{}", error))?;
    history
        .show()
        .map_err(|error| format!("显示字幕历史窗口失败：{}", error))
}

/// 隐藏实时字幕相关窗口，应用继续在后台等待快捷键。
#[tauri::command]
fn hide_subtitle_windows(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(subtitle) = app.get_webview_window("subtitle") {
        subtitle
            .hide()
            .map_err(|error| format!("隐藏实时字幕窗口失败：{}", error))?;
    }
    if let Some(history) = app.get_webview_window("subtitleHistory") {
        history
            .hide()
            .map_err(|error| format!("隐藏字幕历史窗口失败：{}", error))?;
    }
    Ok(())
}

/// 从 Hub 主窗口切换实时字幕模式，复用可见 Hub WebView 内的音频采集链路。
#[tauri::command]
fn toggle_subtitle_mode(app: tauri::AppHandle) -> Result<(), String> {
    trigger_subtitle_mode(app);
    Ok(())
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

/// 切换开机启动。macOS 下写入用户级 LaunchAgent。
#[tauri::command]
fn set_login_launch(enabled: bool) -> Result<(), String> {
    if enabled {
        install_login_agent()
    } else {
        uninstall_login_agent()
    }
}

/// 查询当前用户级开机启动项是否存在。
#[tauri::command]
fn get_login_launch() -> Result<bool, String> {
    Ok(login_agent_path()?.exists())
}

/// 切换 Dock 图标显示状态。
#[tauri::command]
fn set_dock_visible(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
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

/// 过滤 typesass 自身窗口，避免把录音浮窗当作自动粘贴目标。
fn normalize_target_app_name(app_name: &str) -> String {
    let normalized_app_name = app_name.trim();
    if normalized_app_name.is_empty()
        || normalized_app_name == "AiTool"
        || normalized_app_name == "typesass"
        || normalized_app_name == "ai-tool"
    {
        return String::new();
    }
    normalized_app_name.to_string()
}

/// 自动粘贴的主链路只确认系统粘贴指令是否发出，不等待目标输入框的慢速回读。
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
#[tauri::command]
fn set_system_output_muted(muted: bool) -> Result<bool, String> {
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

/// 把实时字幕承载窗口定位到当前工作屏幕底部安全区域上方。
fn position_subtitle_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
) -> Result<(), String> {
    let work_area = preferred_window_work_area(app)?;
    let work_width = work_area.width;
    let work_height = work_area.height;
    let width = SUBTITLE_WINDOW_WIDTH.min((work_width - 48.0).max(360.0));
    let x = work_area.x + (work_width - width) / 2.0;
    let y = work_area.y + work_height - SUBTITLE_WINDOW_HEIGHT - SUBTITLE_WINDOW_BOTTOM;
    window
        .set_size(Size::Logical(LogicalSize::new(
            width,
            SUBTITLE_WINDOW_HEIGHT,
        )))
        .map_err(|error| format!("设置实时字幕窗口尺寸失败：{}", error))?;
    window
        .set_position(Position::Logical(LogicalPosition::new(x, y)))
        .map_err(|error| format!("定位实时字幕窗口失败：{}", error))
}

/// 把字幕历史窗口定位到当前工作屏幕右上方。
fn position_subtitle_history_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
) -> Result<(), String> {
    let work_area = preferred_window_work_area(app)?;
    let x = work_area.x + work_area.width
        - SUBTITLE_HISTORY_WINDOW_WIDTH
        - SUBTITLE_HISTORY_WINDOW_RIGHT;
    let y = work_area.y + SUBTITLE_HISTORY_WINDOW_TOP;
    window
        .set_size(Size::Logical(LogicalSize::new(
            SUBTITLE_HISTORY_WINDOW_WIDTH,
            SUBTITLE_HISTORY_WINDOW_HEIGHT,
        )))
        .map_err(|error| format!("设置字幕历史窗口尺寸失败：{}", error))?;
    window
        .set_position(Position::Logical(LogicalPosition::new(x, y)))
        .map_err(|error| format!("定位字幕历史窗口失败：{}", error))
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
        return parse_screen_point(&String::from_utf8_lossy(&output.stdout));
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

/// 接收前端音频数据，调用小米 Mimo 语音识别模型并返回纯文本结果。
#[tauri::command]
async fn transcribe_audio(
    secrets: State<'_, RuntimeSecrets>,
    request: TranscribeRequest,
) -> Result<TranscribeResponse, String> {
    let api_key = resolve_api_key(&request.api_key, &secrets)?;
    validate_transcribe_request(&request, &api_key)?;

    let started_at = Instant::now();
    let base_url = normalize_base_url(&request.base_url);
    let asr_model = normalize_asr_model(&request.asr_model);
    let content_type = normalize_content_type(&request.content_type);
    let mut body = json!({
        "model": asr_model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": format!("data:{};base64,{}", content_type, request.audio_base64)
                        }
                    }
                ]
            }
        ]
    });

    if request.language.trim() != "auto" {
        body["asr_options"] = json!({ "language": request.language.trim() });
    }

    let response_json =
        send_chat_completion(&base_url, &api_key, &body, Some(Duration::from_secs(25))).await?;

    Ok(TranscribeResponse {
        text: response_json
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        elapsed_ms: started_at.elapsed().as_millis(),
        model: response_json
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(asr_model)
            .to_string(),
    })
}

/// 列出当前 Core Audio 能看到的音频进程，供实时字幕选择采集 App。
#[tauri::command]
async fn list_process_tap_audio_apps() -> Result<Vec<ProcessTapAudioApp>, String> {
    tauri::async_runtime::spawn_blocking(list_process_tap_audio_apps_blocking)
        .await
        .map_err(|error| format!("系统音频 App 列表读取任务失败：{}", error))?
}

/// 调用打包的 Core Audio Process Tap helper 采集一段系统播放声音。
#[tauri::command]
async fn capture_process_tap_audio(
    request: ProcessTapCaptureRequest,
) -> Result<ProcessTapCaptureResponse, String> {
    tauri::async_runtime::spawn_blocking(move || capture_process_tap_audio_blocking(request))
        .await
        .map_err(|error| format!("系统音频采集任务失败：{}", error))?
}

/// 调用打包的 Core Audio Process Tap helper 采集系统播放声音，并在 Rust 端直接转写。
#[tauri::command]
async fn capture_process_tap_transcribe(
    app: AppHandle,
    secrets: State<'_, RuntimeSecrets>,
    request: ProcessTapTranscribeRequest,
) -> Result<ProcessTapTranscribeResponse, String> {
    let api_key = resolve_api_key(&request.api_key, &secrets)?;
    let response = run_process_tap_transcribe(request, api_key).await?;
    let _ = app.emit("subtitle-native-transcribe-result", response.clone());
    Ok(response)
}

/// 启动原生系统音频后台转写任务，结果由前端用短命令轮询消费。
#[tauri::command]
async fn start_process_tap_transcribe_task(
    app: AppHandle,
    secrets: State<'_, RuntimeSecrets>,
    request: ProcessTapTranscribeRequest,
) -> Result<(), String> {
    let chunk_index = request.chunk_index;
    let api_key = resolve_api_key(&request.api_key, &secrets)?;
    let subtitle_state = app.state::<RuntimeSubtitleTranscribe>();
    {
        let mut payload = subtitle_state
            .payload
            .lock()
            .map_err(|_| "写入实时字幕任务状态失败：状态锁已损坏".to_string())?;
        *payload = None;
    }
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = match run_process_tap_transcribe(request, api_key).await {
            Ok(response) => ProcessTapTranscribeOutcome {
                chunk_index,
                ok: true,
                response: Some(response),
                error: None,
            },
            Err(error) => ProcessTapTranscribeOutcome {
                chunk_index,
                ok: false,
                response: None,
                error: Some(error),
            },
        };
        let subtitle_state = app_for_task.state::<RuntimeSubtitleTranscribe>();
        let lock_result = subtitle_state.payload.lock();
        if let Ok(mut payload) = lock_result {
            *payload = Some(outcome.clone());
        }
        deliver_process_tap_transcribe_outcome(&app_for_task, &outcome);
    });
    Ok(())
}

/// 通过已验证可用的 WebView eval 桥把原生字幕结果交给 Hub。
fn deliver_process_tap_transcribe_outcome(app: &AppHandle, outcome: &ProcessTapTranscribeOutcome) {
    let Ok(payload_json) = serde_json::to_string(outcome) else {
        return;
    };
    for label in ["main", "hub"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.eval(&format!(
                "if (window.__AIToolHandleNativeSubtitleOutcome) window.__AIToolHandleNativeSubtitleOutcome({});",
                payload_json
            ));
        }
    }
}

/// 消费指定片段的原生系统音频后台转写结果。
#[tauri::command]
fn take_process_tap_transcribe_outcome(
    state: State<'_, RuntimeSubtitleTranscribe>,
    chunk_index: u64,
) -> Result<Option<ProcessTapTranscribeOutcome>, String> {
    let mut payload = state
        .payload
        .lock()
        .map_err(|_| "读取实时字幕任务状态失败：状态锁已损坏".to_string())?;
    if payload
        .as_ref()
        .map(|item| item.chunk_index == chunk_index)
        .unwrap_or(false)
    {
        return Ok(payload.take());
    }
    Ok(None)
}

/// 执行一次原生系统音频采集并直接请求 Mimo ASR。
async fn run_process_tap_transcribe(
    request: ProcessTapTranscribeRequest,
    api_key: String,
) -> Result<ProcessTapTranscribeResponse, String> {
    let chunk_index = request.chunk_index;
    if api_key.trim().is_empty() {
        return Err(
            "请先在设置里保存 Mimo API Key，或用 MIMO_API_KEY 环境变量启动应用".to_string(),
        );
    }

    let base_url = normalize_base_url(&request.base_url);
    let asr_model = normalize_asr_model(&request.asr_model).to_string();
    let language = request.language.trim().to_string();
    let capture_request = ProcessTapCaptureRequest {
        target_keyword: request.target_keyword,
        duration_ms: request.duration_ms,
    };
    let captured = tauri::async_runtime::spawn_blocking(move || {
        capture_process_tap_audio_bytes(capture_request)
    })
    .await
    .map_err(|error| format!("系统音频采集任务失败：{}", error))??;
    let started_at = Instant::now();
    let mut body = json!({
        "model": asr_model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": format!(
                                "data:{};base64,{}",
                                captured.content_type,
                                base64::engine::general_purpose::STANDARD.encode(&captured.audio)
                            )
                        }
                    }
                ]
            }
        ]
    });
    if language != "auto" {
        body["asr_options"] = json!({ "language": language });
    }

    let response_json =
        send_chat_completion(&base_url, &api_key, &body, Some(Duration::from_secs(25))).await?;

    Ok(ProcessTapTranscribeResponse {
        chunk_index,
        text: response_json
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        model: response_json
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&asr_model)
            .to_string(),
        elapsed_ms: started_at.elapsed().as_millis(),
        capture_elapsed_ms: captured.elapsed_ms,
        bytes: captured.audio.len() as u64,
        summary: captured.summary,
    })
}

/// 同步调用 helper 的列表模式并解析为前端可用的 App 选项。
fn list_process_tap_audio_apps_blocking() -> Result<Vec<ProcessTapAudioApp>, String> {
    let helper = resolve_process_tap_helper_path()?;
    let output = run_process_tap_helper_with_timeout(
        &helper,
        &[
            env::temp_dir()
                .join("typesass-process-tap-list.wav")
                .to_string_lossy()
                .to_string(),
            "0.1".to_string(),
            "--list".to_string(),
        ],
        Duration::from_secs(4),
        "读取系统音频 App 列表",
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(format!(
            "读取系统音频 App 列表失败：{}{}",
            stdout,
            if stderr.is_empty() {
                "".to_string()
            } else {
                format!("；{}", stderr)
            }
        ));
    }
    Ok(stdout
        .lines()
        .filter_map(parse_process_tap_audio_app_line)
        .collect())
}

/// 解析 helper 输出的一行音频进程描述。
fn parse_process_tap_audio_app_line(line: &str) -> Option<ProcessTapAudioApp> {
    let pid = line
        .split_whitespace()
        .find_map(|part| part.strip_prefix("pid="))
        .and_then(|value| value.parse::<i32>().ok())?;
    let audio_active = line
        .split_whitespace()
        .find_map(|part| part.strip_prefix("active="))
        .map(|value| value == "true")
        .unwrap_or(false);
    let name_start = line.find(" name=")? + " name=".len();
    let bundle_marker = " bundle=";
    let bundle_start = line.find(bundle_marker)?;
    let name = line[name_start..bundle_start].trim().to_string();
    let bundle_id = line[bundle_start + bundle_marker.len()..]
        .trim()
        .to_string();
    Some(ProcessTapAudioApp {
        pid,
        name,
        bundle_id,
        audio_active,
    })
}

/// 同步执行系统音频 helper，避免阻塞 Tauri 异步运行时。
fn capture_process_tap_audio_blocking(
    request: ProcessTapCaptureRequest,
) -> Result<ProcessTapCaptureResponse, String> {
    let captured = capture_process_tap_audio_bytes(request)?;
    Ok(ProcessTapCaptureResponse {
        audio_base64: base64::engine::general_purpose::STANDARD.encode(&captured.audio),
        content_type: captured.content_type,
        bytes: captured.audio.len() as u64,
        summary: captured.summary,
        elapsed_ms: captured.elapsed_ms,
    })
}

/// 同步执行系统音频 helper 并返回内存中的 WAV 数据。
fn capture_process_tap_audio_bytes(
    request: ProcessTapCaptureRequest,
) -> Result<ProcessTapCapturedAudio, String> {
    let started_at = Instant::now();
    let helper = resolve_process_tap_helper_path()?;
    let duration_ms = request.duration_ms.clamp(800, 8_000);
    let duration_seconds = format!("{:.3}", duration_ms as f64 / 1000.0);
    let target_keyword = if request.target_keyword.trim().is_empty() {
        "active"
    } else {
        request.target_keyword.trim()
    };
    let output_path = env::temp_dir().join(format!(
        "typesass-process-tap-{}-{}.wav",
        std::process::id(),
        started_at.elapsed().as_nanos()
    ));
    let output = run_process_tap_helper_with_timeout(
        &helper,
        &[
            output_path.to_string_lossy().to_string(),
            duration_seconds,
            target_keyword.to_string(),
        ],
        Duration::from_millis(duration_ms + 12_000),
        "采集系统音频",
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        let _ = fs::remove_file(&output_path);
        return Err(format!(
            "系统音频采集失败：{}{}",
            stdout,
            if stderr.is_empty() {
                "".to_string()
            } else {
                format!("；{}", stderr)
            }
        ));
    }
    let audio =
        fs::read(&output_path).map_err(|error| format!("读取系统音频片段失败：{}", error))?;
    let _ = fs::remove_file(&output_path);
    if audio.len() <= 4_096 {
        return Err(format!("系统音频片段为空：{}", stdout));
    }
    Ok(ProcessTapCapturedAudio {
        audio,
        content_type: "audio/wav".to_string(),
        summary: stdout,
        elapsed_ms: started_at.elapsed().as_millis(),
    })
}

/// 运行系统音频 helper，并在 Core Audio 卡住时主动结束子进程。
fn run_process_tap_helper_with_timeout(
    helper: &PathBuf,
    args: &[String],
    timeout: Duration,
    action: &str,
) -> Result<Output, String> {
    let mut child = Command::new(helper)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动系统音频采集器失败：{}", error))?;
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|error| format!("读取系统音频采集器输出失败：{}", error));
            }
            Ok(None) => {
                if started_at.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{}超时，请刷新音频 App 或重新选择采集目标。",
                        action
                    ));
                }
                thread::sleep(Duration::from_millis(80));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(format!("等待系统音频采集器失败：{}", error));
            }
        }
    }
}

/// 查找随 Tauri 打包或开发环境生成的 Core Audio Process Tap helper。
fn resolve_process_tap_helper_path() -> Result<PathBuf, String> {
    let current_exe =
        env::current_exe().map_err(|error| format!("读取当前程序路径失败：{}", error))?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| "当前程序路径没有父目录".to_string())?;
    let helper_names = process_tap_helper_names();
    let mut candidates = Vec::new();
    for name in &helper_names {
        candidates.push(exe_dir.join(name));
        candidates.push(exe_dir.join("../Resources").join(name));
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join(name),
        );
    }
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| "未找到系统音频采集器，请重新打包 typesass。".to_string())
}

/// 当前平台可能出现的 helper 文件名。
fn process_tap_helper_names() -> Vec<String> {
    let mut names = vec!["typesass-process-tap".to_string()];
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    names.push("typesass-process-tap-aarch64-apple-darwin".to_string());
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    names.push("typesass-process-tap-x86_64-apple-darwin".to_string());
    names
}

/// 接收 ASR 原文并按模式执行听写整理、翻译或问答。
#[tauri::command]
async fn process_text(
    secrets: State<'_, RuntimeSecrets>,
    request: ProcessTextRequest,
) -> Result<ProcessTextResponse, String> {
    let api_key = resolve_api_key(&request.api_key, &secrets)?;
    let normalized_text = request.text.trim();
    if normalized_text.is_empty() {
        return Err("AI 处理失败：文本为空".to_string());
    }

    let started_at = Instant::now();
    let base_url = normalize_base_url(&request.base_url);
    let text_model = normalize_text_model(&request.text_model);
    let (system_prompt, user_prompt) = build_process_prompt(&request, normalized_text);
    let max_completion_tokens = calculate_process_max_tokens(&request, normalized_text);
    let temperature = calculate_process_temperature(&request.mode);
    let body = json!({
        "model": text_model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "temperature": temperature,
        "max_completion_tokens": max_completion_tokens,
        "thinking": { "type": "disabled" }
    });

    let response_json = send_chat_completion(
        &base_url,
        &api_key,
        &body,
        Some(calculate_process_timeout(&request.mode)),
    )
    .await?;
    Ok(ProcessTextResponse {
        processed_text: response_json
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        elapsed_ms: started_at.elapsed().as_millis(),
        model: response_json
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(text_model)
            .to_string(),
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
        return Err("读取选中文本需要先给 typesass 开启辅助功能权限。".to_string());
    }
    let clipboard_snapshot = capture_clipboard_snapshot()
        .map_err(|error| format!("备份用户原剪贴板失败：{}", trim_error_message(&error)))?;
    let marker = format!("typesass-selection-marker-{}", std::process::id());
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

/// 根据处理模式和用户词典构造 AI 指令。
fn build_process_prompt(request: &ProcessTextRequest, text: &str) -> (String, String) {
    let dictionary = request
        .dictionary
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .take(80)
        .collect::<Vec<_>>()
        .join("、");
    let glossary_rule = if dictionary.is_empty() {
        "没有额外词典。".to_string()
    } else {
        format!("优先保留这些专有名词和大小写：{}。", dictionary)
    };
    let context_rule = build_context_rule(request);
    let fast_context_rule = build_fast_context_rule(request);
    let fast_glossary_rule = if dictionary.is_empty() {
        "词典：无。".to_string()
    } else {
        format!("词典：{}。", dictionary)
    };

    match request.mode {
        ProcessMode::Dictate => (
            "你是桌面语音口述整理助手。只输出整理后的最终文本，不解释过程，不输出标题。".to_string(),
            format!(
                "{}\n{}\n规则：把口语转写整理成清晰、可直接发送或记录的文字；删除“嗯、啊、然后、就是说”等无意义语气词和重复；合并断裂句；修正 ASR 误识别和标点；保持事实、意图、语气和句式，不新增信息。短句或明确表态只做必要标点和错字修正，不能总结、不能改成回答、不能把“这个没有问题”改成“是的”这类语义更泛的表达。只有原文是一整段散乱想法时，才提炼核心意思并简洁总结。\nASR：{}",
                fast_glossary_rule, fast_context_rule, text
            ),
        ),
        ProcessMode::Translate => {
            let targets = if request.target_languages.is_empty() {
                "简体中文".to_string()
            } else {
                request
                    .target_languages
                    .iter()
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
                    .join("、")
            };
            (
                "你是一个桌面语音翻译助手。请只输出翻译结果，不解释过程，不输出标题。".to_string(),
                format!(
                    "{}\n{}\n目标语言：{}。\n如果有多种目标语言，请按目标语言分段输出。\n\n原文：{}",
                    glossary_rule, context_rule, targets, text
                ),
            )
        }
        ProcessMode::Ask => (
            "你是一个简洁可靠的桌面问答助手。请直接回答用户问题，不重复问题，不输出无关说明。"
                .to_string(),
            format!("{}\n{}\n用户问题：{}", glossary_rule, context_rule, text),
        ),
        ProcessMode::Polish => (
            "你是桌面文本润色助手。只输出润色后的最终文本，不解释过程，不输出标题。".to_string(),
            format!(
                "{}\n{}\n规则：在不改变事实、含义、称谓、数字和专有名词的前提下，优化表达、语序、标点和可读性；删掉多余口语、重复和病句；保留原文语言、语气、句式和段落结构；不要扩写，不要新增信息，不要把陈述句改成回答或确认。短句只做必要标点和错字修正。\n原文：{}",
                glossary_rule, context_rule, text
            ),
        ),
    }
}

/// 根据模式和原文长度限制 AI 输出长度，避免短句口述被模型长时间思考拖慢。
fn calculate_process_max_tokens(request: &ProcessTextRequest, text: &str) -> u32 {
    let char_count = text.chars().count() as u32;
    match request.mode {
        ProcessMode::Dictate => (char_count.saturating_mul(2) + 160).clamp(192, 800),
        ProcessMode::Polish => (char_count.saturating_mul(2) + 128).clamp(192, 1000),
        ProcessMode::Translate => {
            let target_count = request
                .target_languages
                .iter()
                .filter(|item| !item.trim().is_empty())
                .count()
                .max(1) as u32;
            (char_count.saturating_mul(2).saturating_mul(target_count) + 96).clamp(192, 1200)
        }
        ProcessMode::Ask => (char_count.saturating_mul(2) + 512).clamp(512, 1600),
    }
}

/// 口述和翻译优先确定性输出，问答保留一点表达弹性。
fn calculate_process_temperature(mode: &ProcessMode) -> f64 {
    match mode {
        ProcessMode::Ask => 0.2,
        ProcessMode::Dictate | ProcessMode::Translate | ProcessMode::Polish => 0.0,
    }
}

/// 根据模式限制 AI 文本处理等待时间，口述优先快速粘贴，翻译和问答保留更长响应窗口。
fn calculate_process_timeout(mode: &ProcessMode) -> Duration {
    match mode {
        ProcessMode::Dictate => Duration::from_millis(10000),
        ProcessMode::Polish => Duration::from_millis(12000),
        ProcessMode::Translate => Duration::from_millis(9000),
        ProcessMode::Ask => Duration::from_millis(15000),
    }
}

/// 构造前台应用和用户风格偏好提示，只影响表达，不允许新增事实。
fn build_context_rule(request: &ProcessTextRequest) -> String {
    let mut rules = Vec::new();
    let context_app = request.context_app.trim();
    if !context_app.is_empty() {
        rules.push(format!(
            "当前前台应用是 {}，输出语气可贴合该应用的输入场景。",
            context_app
        ));
    }
    let style_instruction = request.style_instruction.trim();
    if !style_instruction.is_empty() {
        rules.push(format!(
            "用户本地输出偏好：{}。这些偏好只能影响表达风格，不能改变事实或意图。",
            style_instruction
        ));
    }
    if rules.is_empty() {
        "没有额外上下文。".to_string()
    } else {
        rules.join("\n")
    }
}

/// 构造口述润色的短上下文提示，减少短句处理时的模型排队和生成负担。
fn build_fast_context_rule(request: &ProcessTextRequest) -> String {
    let mut rules = Vec::new();
    let context_app = request.context_app.trim();
    if !context_app.is_empty() {
        rules.push(format!("应用：{}。", context_app));
    }
    let style_instruction = request.style_instruction.trim();
    if !style_instruction.is_empty() {
        rules.push(format!("偏好：{}。", style_instruction));
    }
    if rules.is_empty() {
        "上下文：无。".to_string()
    } else {
        rules.join("\n")
    }
}

/// 调用 OpenAI 兼容 chat/completions 接口并返回 JSON。
async fn send_chat_completion(
    base_url: &str,
    api_key: &str,
    body: &Value,
    request_timeout: Option<Duration>,
) -> Result<Value, String> {
    let mut client_builder = reqwest::Client::builder();
    if let Some(timeout) = request_timeout {
        client_builder = client_builder.timeout(timeout);
    }
    let client = client_builder
        .build()
        .map_err(|error| format!("创建 Mimo 客户端失败：{}", error))?;
    let response = client
        .post(format!("{}/chat/completions", base_url))
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("Mimo 请求失败：{}", error))?;
    let status = response.status();
    let response_text = response
        .text()
        .await
        .map_err(|error| format!("读取 Mimo 响应失败：{}", error))?;
    let response_json: Value = serde_json::from_str(&response_text).unwrap_or(Value::Null);

    if !status.is_success() {
        let message = response_json
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or(response_text.as_str());
        return Err(format!("Mimo 请求失败：{}", trim_error_message(message)));
    }
    Ok(response_json)
}

/// 把文字写入系统剪贴板，并模拟 Cmd+V 粘贴到当前前台输入框。
#[tauri::command]
async fn paste_text(
    app: tauri::AppHandle,
    text: String,
    target_app: String,
) -> Result<PasteResponse, String> {
    let normalized_text = text.trim();
    if normalized_text.is_empty() {
        return Err("转写结果为空，无法自动粘贴".to_string());
    }

    let frontmost_before_paste = get_frontmost_app().unwrap_or_default();
    let paste_target = resolve_paste_target(&target_app, &frontmost_before_paste);
    let normalized_target_app = paste_target.target_app;
    let accessibility_trusted = is_accessibility_trusted();
    if normalized_target_app.is_empty() {
        let clipboard_restore_status =
            clipboard_restore_not_attempted("未写入临时剪贴板，避免覆盖用户原剪贴板。");
        return Ok(PasteResponse {
            pasted: false,
            message: "当前焦点不在外部输入目标上；已保持原剪贴板不变。".to_string(),
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
            pasted: false,
            message: "自动粘贴需要先给 typesass 开启辅助功能权限；已保持原剪贴板不变。".to_string(),
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

    let clipboard_snapshot = match capture_clipboard_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let clipboard_restore_status = clipboard_restore_not_attempted(&format!(
                "备份用户原剪贴板失败：{}",
                trim_error_message(&error)
            ));
            return Ok(PasteResponse {
                pasted: false,
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
                frontmost_after_activate: String::new(),
                frontmost_after_paste: String::new(),
                insertion_verified: false,
                verification_status: "原剪贴板备份失败，未写入临时剪贴板".to_string(),
                focused_element_before_paste: String::new(),
                focused_element_after_activate: String::new(),
                focused_element_after_paste: String::new(),
            });
        }
    };
    let clipboard_matches_expected = write_clipboard_text_verified(normalized_text)?;
    let clipboard_written = clipboard_matches_expected;
    if !clipboard_matches_expected {
        let clipboard_restore_status = build_clipboard_restore_status(
            Some(()),
            restore_clipboard_snapshot(&clipboard_snapshot),
        );
        return Ok(PasteResponse {
            pasted: false,
            message: "写入临时剪贴板后读回内容不一致，已停止自动粘贴并恢复原剪贴板。".to_string(),
            requires_accessibility: false,
            target_app: normalized_target_app,
            clipboard_written,
            clipboard_matches_expected,
            clipboard_restore_attempted: clipboard_restore_status.attempted,
            clipboard_restored: clipboard_restore_status.restored,
            clipboard_restore_message: clipboard_restore_status.message,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate: String::new(),
            frontmost_after_paste: String::new(),
            insertion_verified: false,
            verification_status: "临时剪贴板读回不一致".to_string(),
            focused_element_before_paste: String::new(),
            focused_element_after_activate: String::new(),
            focused_element_after_paste: String::new(),
        });
    }

    let focus_status = read_paste_focus_status();
    if !focus_status.ready {
        let clipboard_restore_status = build_clipboard_restore_status(
            Some(()),
            restore_clipboard_snapshot(&clipboard_snapshot),
        );
        return Ok(PasteResponse {
            pasted: false,
            message: "当前没有聚焦可输入区域，未发送粘贴指令；结果已展示，可手动复制。".to_string(),
            requires_accessibility: false,
            target_app: normalized_target_app,
            clipboard_written,
            clipboard_matches_expected,
            clipboard_restore_attempted: clipboard_restore_status.attempted,
            clipboard_restored: clipboard_restore_status.restored,
            clipboard_restore_message: clipboard_restore_status.message,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate: String::new(),
            frontmost_after_paste: String::new(),
            insertion_verified: false,
            verification_status: "粘贴前没有检测到可输入焦点，未发送 Cmd+V".to_string(),
            focused_element_before_paste: focus_status.summary,
            focused_element_after_activate: String::new(),
            focused_element_after_paste: String::new(),
        });
    }
    let focused_element_before_paste = focus_status.summary;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window("result") {
        let _ = window.hide();
    }
    if paste_target.should_hide_hub {
        if let Some(window) = app.get_webview_window("hub") {
            let _ = window.hide();
        }
    }
    let frontmost_after_activate = String::new();
    let focused_element_after_activate =
        "直接粘贴模式：未激活目标 App，直接向当前焦点发送粘贴。".to_string();
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
    let insertion_verified = true;
    paste_methods.push("直接向当前焦点发送，不激活目标 App".to_string());

    let verification_status = format!(
        "粘贴前已检测到可输入焦点，并已发出一次系统粘贴指令；{}；macOS 不提供可靠的写入成功回调。",
        clipboard_restore_status.message
    );
    let paste_method = paste_methods.join(" -> ");
    let pasted = should_mark_paste_command_as_sent(paste_result.accessibility_ready, true, true);

    Ok(PasteResponse {
        pasted,
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

/// 校验转写请求是否具备必要字段。
fn validate_transcribe_request(request: &TranscribeRequest, api_key: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err(
            "请先在设置里保存 Mimo API Key，或用 MIMO_API_KEY 环境变量启动应用".to_string(),
        );
    }
    if request.audio_base64.trim().is_empty() {
        return Err("音频为空".to_string());
    }
    Ok(())
}

/// 优先使用请求密钥，其次使用会话内存密钥、钥匙串密钥，最后读取环境变量。
fn resolve_api_key(request_key: &str, secrets: &RuntimeSecrets) -> Result<String, String> {
    let api_key = request_key.trim();
    if !api_key.is_empty() {
        return Ok(api_key.to_string());
    }

    let session_key = secrets
        .api_key
        .lock()
        .map_err(|_| "读取会话密钥失败：状态锁已损坏".to_string())?
        .trim()
        .to_string();
    if !session_key.is_empty() {
        return Ok(session_key);
    }

    if let Some(keychain_key) = read_keychain_api_key()? {
        return Ok(keychain_key);
    }

    std::env::var("MIMO_API_KEY").map_err(|_| "未找到 MIMO_API_KEY 环境变量".to_string())
}

/// 从 macOS 钥匙串读取 Mimo Key；不存在时返回 None。
fn read_keychain_api_key() -> Result<Option<String>, String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
        ])
        .output()
        .map_err(|error| format!("读取钥匙串失败：{}", error))?;
    if output.status.success() {
        let api_key = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!api_key.is_empty()).then_some(api_key));
    }
    let error_text = String::from_utf8_lossy(&output.stderr);
    if error_text.contains("could not be found")
        || error_text.contains("The specified item could not be found")
    {
        return Ok(None);
    }
    Err(format!(
        "读取钥匙串失败：{}",
        trim_error_message(&error_text)
    ))
}

/// 将 Mimo Key 写入 macOS 钥匙串的 generic password 条目。
fn write_keychain_api_key(api_key: &str) -> Result<(), String> {
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
            api_key,
            "-U",
        ])
        .output()
        .map_err(|error| format!("保存钥匙串失败：{}", error))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "保存钥匙串失败：{}",
        trim_error_message(&String::from_utf8_lossy(&output.stderr))
    ))
}

/// 删除 macOS 钥匙串中的 Mimo Key；不存在时也视为已清除。
fn delete_keychain_api_key() -> Result<(), String> {
    let output = Command::new("security")
        .args([
            "delete-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
        ])
        .output()
        .map_err(|error| format!("清除钥匙串失败：{}", error))?;
    if output.status.success() {
        return Ok(());
    }
    let error_text = String::from_utf8_lossy(&output.stderr);
    if error_text.contains("could not be found")
        || error_text.contains("The specified item could not be found")
    {
        return Ok(());
    }
    Err(format!(
        "清除钥匙串失败：{}",
        trim_error_message(&error_text)
    ))
}

/// 规范化 OpenAI 兼容接口地址。
fn normalize_base_url(base_url: &str) -> String {
    let value = base_url.trim();
    let value = if value.is_empty() {
        DEFAULT_BASE_URL
    } else {
        value
    };
    value.trim_end_matches('/').to_string()
}

/// 规范化语音识别模型名称。
fn normalize_asr_model(model: &str) -> &str {
    let value = model.trim();
    if value.is_empty() {
        DEFAULT_ASR_MODEL
    } else {
        value
    }
}

/// 规范化文本处理模型名称。
fn normalize_text_model(model: &str) -> &str {
    let value = model.trim();
    if value.is_empty() {
        DEFAULT_TEXT_MODEL
    } else {
        value
    }
}

/// 规范化音频 MIME 类型。
fn normalize_content_type(content_type: &str) -> &str {
    let value = content_type.trim();
    if value.is_empty() {
        "audio/webm"
    } else {
        value
    }
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
    clipboard_text == output_text
        || clipboard_text.trim_end_matches(|character| character == '\n' || character == '\r')
            == output_text
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

/// 通过 macOS pbpaste 读取剪贴板文本，用于托盘菜单快速加入本地词典。
#[cfg(target_os = "macos")]
fn read_clipboard_text() -> Result<String, String> {
    Ok(read_clipboard_text_raw()?.trim().to_string())
}

/// 非 macOS 桌面端暂不支持读取剪贴板原文。
#[cfg(not(target_os = "macos"))]
fn read_clipboard_text_raw() -> Result<String, String> {
    Err("当前系统暂不支持读取剪贴板".to_string())
}

/// 非 macOS 桌面端暂不支持通过托盘读取剪贴板。
#[cfg(not(target_os = "macos"))]
fn read_clipboard_text() -> Result<String, String> {
    Err("当前系统暂不支持从托盘读取剪贴板".to_string())
}

/// 将剪贴板文本拆成词典词条，兼容逗号、顿号、分号和换行。
fn split_dictionary_words(text: &str) -> Vec<String> {
    text.split(|character: char| {
        character == '\n'
            || character == '\r'
            || character == ','
            || character == '，'
            || character == '、'
            || character == ';'
            || character == '；'
            || character == '\t'
    })
    .map(str::trim)
    .filter(|word| !word.is_empty())
    .take(20)
    .map(ToString::to_string)
    .collect()
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

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_frontmost_shortcut_keeps_hub_and_does_not_reuse_previous_external_target() {
        let decision = resolve_voice_trigger_context("typesass");

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
    fn paste_without_explicit_target_stops_when_typesass_is_frontmost() {
        let decision = resolve_paste_target("", "typesass");

        assert_eq!(decision.target_app, "");
        assert!(!decision.should_hide_hub);
    }

    #[test]
    fn paste_with_explicit_target_uses_current_focus_without_reactivation() {
        let decision = resolve_paste_target("ChatGPT", "TextEdit");

        assert_eq!(decision.target_app, "TextEdit");
        assert!(!decision.should_hide_hub);
    }

    #[test]
    fn paste_ignores_explicit_target_when_current_focus_is_typesass() {
        let decision = resolve_paste_target("ChatGPT", "typesass");

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
        assert!(PASTE_DIAGNOSTIC_SETTLE_DELAY_MS <= 120);
        assert!(CLIPBOARD_VERIFY_INITIAL_DELAY_MS <= 50);
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
        let marker = format!("typesass-clipboard-roundtrip-{}", std::process::id());

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
