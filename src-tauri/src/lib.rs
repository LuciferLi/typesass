use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Position, State};

const DEFAULT_BASE_URL: &str = "https://token-plan-cn.xiaomimimo.com/v1";
const DEFAULT_ASR_MODEL: &str = "mimo-v2.5-asr";
const DEFAULT_TEXT_MODEL: &str = "mimo-v2.5";
const DEFAULT_DICTATE_SHORTCUT: &str = "ctrl+p";
const DEFAULT_TRANSLATE_SHORTCUT: &str = "ctrl+t";
const DEFAULT_ASK_SHORTCUT: &str = "ctrl+space";
const LOGIN_AGENT_LABEL: &str = "asia.aijob.aitool.login";
const KEYCHAIN_SERVICE: &str = "asia.aijob.aitool";
const KEYCHAIN_ACCOUNT: &str = "mimo-api-key";
const FLOAT_WINDOW_WIDTH: f64 = 132.0;
const FLOAT_WINDOW_TOP: f64 = 60.0;
const TOAST_WINDOW_WIDTH: f64 = 460.0;
const TOAST_WINDOW_TOP: f64 = 42.0;
const RESULT_WINDOW_WIDTH: f64 = 520.0;
const RESULT_WINDOW_TOP: f64 = 76.0;

/// 运行期间保存的敏感配置，只放内存，不写入本地文件。
#[derive(Default)]
struct RuntimeSecrets {
    /// 当前会话的小米 Mimo 接口密钥。
    api_key: Mutex<String>,
}

/// 运行期间保存的全局快捷键映射。
struct RuntimeShortcuts {
    /// 三种模式当前实际注册的快捷键。
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

/// 运行期间保存最近一次外部前台 App，Hub 前台时用于恢复真实粘贴目标。
#[derive(Default)]
struct RuntimeFocus {
    /// 最近一次非 typesass 的前台应用名称。
    last_external_app: Mutex<String>,
}

/// 全局快捷键触发录音时的窗口和目标 App 决策。
#[derive(Debug, Clone, PartialEq, Eq)]
struct VoiceTriggerContext {
    /// 本次录音结束后允许恢复的目标 App，Hub 前台触发时必须为空。
    target_app: String,
    /// 是否需要展示顶部悬浮胶囊。
    show_floating_window: bool,
    /// 是否必须保持 Hub 主界面不受录音影响。
    keep_hub_visible: bool,
    /// 是否需要把焦点恢复给目标 App。
    restore_target_focus: bool,
}

/// 自动粘贴前的目标 App 与窗口隐藏决策。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PasteTargetDecision {
    /// 本次粘贴应恢复的目标 App。
    target_app: String,
    /// 是否需要激活目标 App。
    should_activate_target: bool,
    /// 是否允许隐藏 Hub 以避免粘贴回 typesass。
    should_hide_hub: bool,
}

/// 全局快捷键注册结果，避免系统冲突时只表现为按键无响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutRegistrationStatus {
    /// 三种模式快捷键是否已成功注册到系统。
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

/// 前端提交的三种语音模式快捷键。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutProfile {
    /// 听写模式快捷键。
    dictate: String,
    /// 翻译模式快捷键。
    translate: String,
    /// 随便问模式快捷键。
    ask: String,
}

impl Default for ShortcutProfile {
    fn default() -> Self {
        Self {
            dictate: DEFAULT_DICTATE_SHORTCUT.to_string(),
            translate: DEFAULT_TRANSLATE_SHORTCUT.to_string(),
            ask: DEFAULT_ASK_SHORTCUT.to_string(),
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
    /// 是否成功触发系统粘贴。
    pasted: bool,
    /// 给前端展示的执行说明。
    message: String,
    /// 是否需要用户授予辅助功能权限。
    requires_accessibility: bool,
    /// 本次尝试粘贴前恢复的目标应用。
    target_app: String,
    /// 是否已成功写入系统剪贴板。
    clipboard_written: bool,
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
    /// 发送粘贴指令前目标 App 内的系统焦点元素。
    focused_element_before_paste: String,
    /// 激活目标 App 后的系统焦点元素。
    focused_element_after_activate: String,
    /// 发送粘贴指令后的系统焦点元素。
    focused_element_after_paste: String,
}

/// 系统级粘贴触发结果，记录具体路径以便前端诊断。
#[derive(Debug)]
struct PasteTriggerResult {
    /// 触发粘贴时辅助功能权限是否可信。
    accessibility_ready: bool,
    /// 实际使用的系统粘贴触发方式。
    method: String,
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
        .manage(RuntimeFocus::default())
        .manage(RuntimeResult::default())
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
            process_text,
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
            get_last_result_window_payload,
            register_shortcuts,
            get_runtime_diagnostics,
            open_accessibility_settings,
            set_login_launch,
            get_login_launch,
            set_dock_visible,
            get_frontmost_app,
            get_recording_target_app,
            activate_app,
            set_system_output_muted
        ])
        .build(tauri::generate_context!())
        .expect("启动 typesass 失败")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                remember_current_external_app(app);
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
    remember_current_external_app(app);
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

/// 记住当前非 typesass 前台 App，避免 Hub 抢焦点后丢失自动粘贴目标。
fn remember_current_external_app(app: &AppHandle) -> String {
    get_frontmost_app()
        .map(|app_name| remember_external_app(app, &app_name))
        .unwrap_or_default()
}

/// 读取当前外部目标 App；如果当前前台是 typesass，则回退到最近一次外部 App。
fn read_external_target_app(app: &AppHandle) -> String {
    let current_app = get_frontmost_app().unwrap_or_default();
    let normalized_current_app = normalize_target_app_name(&current_app);
    if !normalized_current_app.is_empty() {
        return remember_external_app(app, &normalized_current_app);
    }
    read_last_external_app(app)
}

/// 写入最近一次外部 App 名称，空值或 typesass 自身不会覆盖旧目标。
fn remember_external_app(app: &AppHandle, app_name: &str) -> String {
    let normalized_app_name = normalize_target_app_name(app_name);
    if normalized_app_name.is_empty() {
        return String::new();
    }
    let focus_state = app.state::<RuntimeFocus>();
    if let Ok(mut last_external_app) = focus_state.last_external_app.lock() {
        *last_external_app = normalized_app_name.clone();
    }
    normalized_app_name
}

/// 读取最近一次外部 App 名称，状态锁异常时回退为空，避免中断主链路。
fn read_last_external_app(app: &AppHandle) -> String {
    let focus_state = app.state::<RuntimeFocus>();
    focus_state
        .last_external_app
        .lock()
        .map(|app_name| normalize_target_app_name(&app_name))
        .unwrap_or_default()
}

/// 根据当前前台 App 计算快捷键录音行为；Hub 前台时不复用历史外部目标。
fn resolve_voice_trigger_context(
    frontmost_app: &str,
    _last_external_app: &str,
) -> VoiceTriggerContext {
    let normalized_frontmost_app = normalize_target_app_name(frontmost_app);
    if normalized_frontmost_app.is_empty() {
        return VoiceTriggerContext {
            target_app: String::new(),
            show_floating_window: false,
            keep_hub_visible: true,
            restore_target_focus: false,
        };
    }
    VoiceTriggerContext {
        target_app: normalized_frontmost_app,
        show_floating_window: true,
        keep_hub_visible: false,
        restore_target_focus: true,
    }
}

/// 根据请求目标和当前前台 App 计算粘贴行为；无目标且 typesass 前台时不隐藏 Hub。
fn resolve_paste_target(
    requested_target_app: &str,
    frontmost_app: &str,
    _last_external_app: &str,
) -> PasteTargetDecision {
    let normalized_requested_app = normalize_target_app_name(requested_target_app);
    if !normalized_requested_app.is_empty() {
        return PasteTargetDecision {
            target_app: normalized_requested_app,
            should_activate_target: true,
            should_hide_hub: true,
        };
    }
    let normalized_frontmost_app = normalize_target_app_name(frontmost_app);
    if !normalized_frontmost_app.is_empty() {
        return PasteTargetDecision {
            target_app: normalized_frontmost_app,
            should_activate_target: true,
            should_hide_hub: true,
        };
    }
    PasteTargetDecision {
        target_app: String::new(),
        should_activate_target: false,
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
    trigger_voice_mode(app, &mode);
}

/// 按指定模式通知悬浮录音条开始或停止。
fn trigger_voice_mode(app: tauri::AppHandle, mode: &str) {
    let frontmost_app = get_frontmost_app().unwrap_or_default();
    let mut context = resolve_voice_trigger_context(&frontmost_app, &read_last_external_app(&app));
    if !context.target_app.is_empty() {
        context.target_app = remember_external_app(&app, &context.target_app);
    }
    if let Some(result) = app.get_webview_window("result") {
        let _ = result.hide();
    }
    if context.show_floating_window {
        let _ = present_window(&app, "main", true);
    } else if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    if context.restore_target_focus && !context.target_app.is_empty() {
        let _ = activate_macos_app(&context.target_app);
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
    } else {
        "dictate".to_string()
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

/// 非桌面环境不注册系统级快捷键。
#[cfg(not(desktop))]
fn register_shortcut_profile(
    _app: &tauri::AppHandle,
    _profile: &ShortcutProfile,
) -> Result<(), String> {
    Ok(())
}

/// 规范化前端快捷键配置，并检查是否存在冲突。
fn normalize_shortcut_profile(profile: ShortcutProfile) -> Result<ShortcutProfile, String> {
    let normalized = ShortcutProfile {
        dictate: normalize_shortcut_or_default(&profile.dictate, DEFAULT_DICTATE_SHORTCUT),
        translate: normalize_shortcut_or_default(&profile.translate, DEFAULT_TRANSLATE_SHORTCUT),
        ask: normalize_shortcut_or_default(&profile.ask, DEFAULT_ASK_SHORTCUT),
    };
    let mut seen = std::collections::HashSet::new();
    for shortcut in [&normalized.dictate, &normalized.translate, &normalized.ask] {
        let key = normalize_shortcut(shortcut);
        if !seen.insert(key) {
            return Err("三个模式不能使用同一个快捷键".to_string());
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
    value
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "")
        .replace("control+", "ctrl+")
        .replace("command+", "cmd+")
        .replace("meta+", "cmd+")
        .replace("option+", "alt+")
}

/// 显示胶囊悬浮条，供前端在需要时主动唤起。
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<String, String> {
    let frontmost_app = get_frontmost_app().unwrap_or_default();
    let mut context = resolve_voice_trigger_context(&frontmost_app, &read_last_external_app(&app));
    if !context.target_app.is_empty() {
        context.target_app = remember_external_app(&app, &context.target_app);
    }
    if let Some(result) = app.get_webview_window("result") {
        let _ = result.hide();
    }
    if context.show_floating_window {
        present_window(&app, "main", true)?;
    } else if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    if context.restore_target_focus && !context.target_app.is_empty() {
        let _ = activate_macos_app(&context.target_app);
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
    remember_current_external_app(&app);
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

/// 读取本次录音应恢复的目标 App；当前前台是 typesass 时回退到最近一次外部 App。
#[tauri::command]
fn get_recording_target_app(app: tauri::AppHandle) -> Result<String, String> {
    Ok(read_external_target_app(&app))
}

/// 激活指定 macOS App，用于转写完成前回到录音触发时的目标输入应用。
#[tauri::command]
fn activate_app(app_name: String) -> Result<(), String> {
    let normalized_app_name = normalize_target_app_name(&app_name);
    if normalized_app_name.is_empty() {
        return Ok(());
    }
    activate_macos_app(&normalized_app_name)
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

/// 判断当前前台 App 是否仍是自动粘贴目标，避免前台被系统设置等窗口抢走。
fn is_frontmost_target(frontmost_app: &str, target_app: &str) -> bool {
    let normalized_target_app = normalize_target_app_name(target_app);
    !normalized_target_app.is_empty()
        && normalize_target_app_name(frontmost_app) == normalized_target_app
}

/// 当前台在发送粘贴后变成非目标 App 时，允许补救一次，避免粘贴事件落到系统设置。
fn should_retry_paste_for_target(target_app: &str, frontmost_after_paste: &str) -> bool {
    let normalized_target_app = normalize_target_app_name(target_app);
    !normalized_target_app.is_empty()
        && !is_frontmost_target(frontmost_after_paste, &normalized_target_app)
}

/// 激活 macOS 目标 App；非 macOS 平台保留空实现以便编译通过。
fn activate_macos_app(app_name: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        run_osascript(&format!(
            r#"tell application "{}" to activate"#,
            apple_script_escape(app_name)
        ))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_name;
    }
    Ok(())
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

/// 转义 AppleScript 双引号和反斜杠，避免 App 名称破坏脚本。
fn apple_script_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 把错误提示窗口定位到主屏幕顶部偏下的位置。
fn position_toast_window(
    app: &tauri::AppHandle,
    toast: &tauri::WebviewWindow,
) -> Result<(), String> {
    position_top_center_window(app, toast, TOAST_WINDOW_WIDTH, TOAST_WINDOW_TOP)
        .map_err(|error| error.replace("定位窗口", "定位错误提示"))
}

/// 把指定窗口定位到主屏幕顶部居中的位置。
fn position_top_center_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    width: f64,
    top: f64,
) -> Result<(), String> {
    let monitor = app
        .primary_monitor()
        .map_err(|error| format!("读取屏幕信息失败：{}", error))?
        .ok_or_else(|| "没有可用屏幕".to_string())?;
    let scale_factor = monitor.scale_factor();
    let work_area = monitor.work_area();
    let x = work_area.position.x as f64 / scale_factor
        + (work_area.size.width as f64 / scale_factor - width) / 2.0;
    let y = work_area.position.y as f64 / scale_factor + top;
    window
        .set_position(Position::Logical(LogicalPosition::new(x, y)))
        .map_err(|error| format!("定位窗口失败：{}", error))
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

    let response_json = send_chat_completion(&base_url, &api_key, &body, None).await?;

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
            "只输出整理后的最终文本，保持原意，不解释。".to_string(),
            format!(
                "{}\n{}\n规则：修正 ASR 误识别、标点、明显口误和重复；不总结，不扩写。\nASR：{}",
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
    }
}

/// 根据模式和原文长度限制 AI 输出长度，避免短句口述被模型长时间思考拖慢。
fn calculate_process_max_tokens(request: &ProcessTextRequest, text: &str) -> u32 {
    let char_count = text.chars().count() as u32;
    match request.mode {
        ProcessMode::Dictate => (char_count.saturating_mul(2) + 64).clamp(128, 512),
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
        ProcessMode::Dictate | ProcessMode::Translate => 0.0,
    }
}

/// 根据模式限制 AI 文本处理等待时间，口述优先快速粘贴，翻译和问答保留更长响应窗口。
fn calculate_process_timeout(mode: &ProcessMode) -> Duration {
    match mode {
        ProcessMode::Dictate => Duration::from_millis(6500),
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
    let paste_target = resolve_paste_target(
        &target_app,
        &frontmost_before_paste,
        &read_last_external_app(&app),
    );
    let normalized_target_app = if !paste_target.target_app.is_empty() {
        remember_external_app(&app, &paste_target.target_app)
    } else {
        String::new()
    };
    write_clipboard_text(normalized_text)?;
    let clipboard_written = true;
    let accessibility_trusted = is_accessibility_trusted();
    if normalized_target_app.is_empty() {
        return Ok(PasteResponse {
            pasted: false,
            message: "已写入剪贴板；当前没有可恢复的目标输入框，已保持 typesass 界面不变。"
                .to_string(),
            requires_accessibility: false,
            target_app: normalized_target_app,
            clipboard_written,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate: String::new(),
            frontmost_after_paste: String::new(),
            focused_element_before_paste: String::new(),
            focused_element_after_activate: String::new(),
            focused_element_after_paste: String::new(),
        });
    }
    if !accessibility_trusted {
        return Ok(PasteResponse {
            pasted: false,
            message: "已写入剪贴板；自动粘贴需要先给 typesass 开启辅助功能权限。".to_string(),
            requires_accessibility: true,
            target_app: normalized_target_app,
            clipboard_written,
            accessibility_trusted,
            paste_method: "notSent".to_string(),
            frontmost_before_paste,
            frontmost_after_activate: String::new(),
            frontmost_after_paste: String::new(),
            focused_element_before_paste: String::new(),
            focused_element_after_activate: String::new(),
            focused_element_after_paste: String::new(),
        });
    }

    let focused_element_before_paste = read_focused_element_summary(&normalized_target_app);
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
    if paste_target.should_activate_target && !normalized_target_app.is_empty() {
        let _ = activate_macos_app(&normalized_target_app);
    }
    thread::sleep(Duration::from_millis(620));
    let frontmost_after_activate = get_frontmost_app().unwrap_or_default();
    let focused_element_after_activate = read_focused_element_summary(&normalized_target_app);
    let paste_result = match trigger_system_paste() {
        Ok(value) => value,
        Err(error) => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            return Err(error);
        }
    };
    let mut paste_methods = vec![paste_result.method.clone()];
    thread::sleep(Duration::from_millis(160));
    let mut frontmost_after_paste = get_frontmost_app().unwrap_or_default();
    let mut focused_element_after_paste = read_focused_element_summary(&normalized_target_app);

    if should_retry_paste_for_target(&normalized_target_app, &frontmost_after_paste) {
        paste_methods.push(format!(
            "前台被{}抢占，恢复目标后重试",
            frontmost_after_paste
        ));
        let _ = activate_macos_app(&normalized_target_app);
        thread::sleep(Duration::from_millis(760));
        if is_frontmost_target(
            &get_frontmost_app().unwrap_or_default(),
            &normalized_target_app,
        ) {
            match trigger_system_paste() {
                Ok(retry_result) => paste_methods.push(format!("重试：{}", retry_result.method)),
                Err(error) => {
                    paste_methods.push(format!("重试失败：{}", trim_error_message(&error)))
                }
            }
            thread::sleep(Duration::from_millis(160));
            frontmost_after_paste = get_frontmost_app().unwrap_or_default();
            focused_element_after_paste = read_focused_element_summary(&normalized_target_app);
        } else {
            paste_methods.push("重试跳过：目标 App 未能回到前台".to_string());
        }
    }
    let paste_method = paste_methods.join(" -> ");
    let final_target_ready = is_frontmost_target(&frontmost_after_paste, &normalized_target_app);

    Ok(PasteResponse {
        pasted: paste_result.accessibility_ready,
        message: if final_target_ready {
            format!(
                "已向 {} 发送粘贴指令；是否插入取决于目标输入框焦点。",
                normalized_target_app
            )
        } else {
            format!(
                "已向 {} 发送粘贴指令，但当前前台是 {}；如果未插入，请重新聚焦输入框。",
                normalized_target_app, frontmost_after_paste
            )
        },
        requires_accessibility: false,
        target_app: normalized_target_app,
        clipboard_written,
        accessibility_trusted,
        paste_method,
        frontmost_before_paste,
        frontmost_after_activate,
        frontmost_after_paste,
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

/// 通过 macOS pbcopy 写入剪贴板，避免把剪贴板内容经由前端暴露。
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

/// 通过 macOS pbpaste 读取剪贴板文本，用于托盘菜单快速加入本地词典。
#[cfg(target_os = "macos")]
fn read_clipboard_text() -> Result<String, String> {
    let output = Command::new("pbpaste")
        .output()
        .map_err(|error| format!("读取剪贴板失败：{}", error))?;
    if !output.status.success() {
        return Err("读取剪贴板失败：pbpaste 执行失败".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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

/// 读取目标 App 当前辅助功能焦点摘要，用于定位 App 前台但输入框未聚焦的问题。
#[cfg(target_os = "macos")]
fn read_focused_element_summary(app_name: &str) -> String {
    let normalized_app_name = normalize_target_app_name(app_name);
    if normalized_app_name.is_empty() {
        return String::new();
    }
    let script = format!(
        r#"
tell application "System Events"
    if not (exists process "{app_name}") then return ""
    tell process "{app_name}"
        try
            set focusedElement to value of attribute "AXFocusedUIElement"
            set roleText to ""
            set subroleText to ""
            set titleText to ""
            set descriptionText to ""
            try
                set roleText to value of attribute "AXRole" of focusedElement as text
            end try
            try
                set subroleText to value of attribute "AXSubrole" of focusedElement as text
            end try
            try
                set titleText to value of attribute "AXTitle" of focusedElement as text
            end try
            try
                set descriptionText to value of attribute "AXDescription" of focusedElement as text
            end try
            return roleText & " / " & subroleText & " / " & titleText & " / " & descriptionText
        on error errorMessage
            return "读取焦点失败：" & errorMessage
        end try
    end tell
end tell
"#,
        app_name = apple_script_escape(&normalized_app_name)
    );
    run_osascript(&script)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

/// 非 macOS 暂不提供系统焦点诊断。
#[cfg(not(target_os = "macos"))]
fn read_focused_element_summary(app_name: &str) -> String {
    let _ = app_name;
    String::new()
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

/// 使用 CoreGraphics 触发 Cmd+V，尽量模拟真实键盘粘贴事件。
#[cfg(target_os = "macos")]
fn trigger_core_graphics_paste(accessibility_ready: bool) -> Result<PasteTriggerResult, String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "自动粘贴失败：无法创建系统按键事件源".to_string())?;
    let key_down = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true)
        .map_err(|_| "自动粘贴失败：无法创建粘贴按下事件".to_string())?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::HID);

    thread::sleep(Duration::from_millis(24));

    let key_up = CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, false)
        .map_err(|_| "自动粘贴失败：无法创建粘贴松开事件".to_string())?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.post(CGEventTapLocation::HID);
    Ok(PasteTriggerResult {
        accessibility_ready,
        method: "CoreGraphics".to_string(),
    })
}

/// 非 macOS 平台暂不支持系统级自动粘贴。
#[cfg(not(target_os = "macos"))]
fn trigger_system_paste() -> Result<PasteTriggerResult, String> {
    Err("自动粘贴失败：当前版本暂时只支持 macOS 自动粘贴".to_string())
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
        let decision = resolve_voice_trigger_context("typesass", "ChatGPT");

        assert_eq!(decision.target_app, "");
        assert!(!decision.show_floating_window);
        assert!(decision.keep_hub_visible);
        assert!(!decision.restore_target_focus);
    }

    #[test]
    fn external_frontmost_shortcut_uses_current_app_as_paste_target() {
        let decision = resolve_voice_trigger_context("ChatGPT", "TextEdit");

        assert_eq!(decision.target_app, "ChatGPT");
        assert!(decision.show_floating_window);
        assert!(!decision.keep_hub_visible);
        assert!(decision.restore_target_focus);
    }

    #[test]
    fn paste_without_explicit_target_does_not_fallback_to_last_external_when_typesass_is_frontmost()
    {
        let decision = resolve_paste_target("", "typesass", "ChatGPT");

        assert_eq!(decision.target_app, "");
        assert!(!decision.should_activate_target);
        assert!(!decision.should_hide_hub);
    }

    #[test]
    fn paste_with_explicit_target_can_hide_hub_and_activate_target() {
        let decision = resolve_paste_target("ChatGPT", "typesass", "TextEdit");

        assert_eq!(decision.target_app, "ChatGPT");
        assert!(decision.should_activate_target);
        assert!(decision.should_hide_hub);
    }
}
