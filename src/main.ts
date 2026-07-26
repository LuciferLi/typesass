import "./styles.css";
import {
  BookOpen,
  CheckSmall,
  CloseSmall,
  Copy,
  Dashboard,
  History,
  KeyboardOne,
  MessageOne,
  Microphone,
  PlayOne,
  Plus,
  Refresh,
  Setting,
  Terminal,
  Translate,
  VoiceInput,
  setConfig,
} from "@icon-park/svg";
import brandLogoUrl from "./assets/typesass-logo.png";

const DEFAULT_BASE_URL = "https://token-plan-cn.xiaomimimo.com/v1";
const DEFAULT_ASR_MODEL = "mimo-v2.5-asr";
const DEFAULT_TEXT_MODEL = "mimo-v2.5";
const DEFAULT_TARGET_LANGUAGE = "简体中文";
const DEFAULT_DICTATION_OUTPUT_LANGUAGE = "source";
const CONFIG_STORAGE_KEY = "aiToolVoiceConfigV2";
const LEGACY_CONFIG_STORAGE_KEY = "aiToolVoiceConfig";
const HISTORY_STORAGE_KEY = "aiToolVoiceHistoryV1";
const SUBTITLE_HISTORY_STORAGE_KEY = "aiToolSubtitleHistoryV1";
const DICTIONARY_STORAGE_KEY = "aiToolDictionaryV1";
const DIAGNOSTIC_LOG_STORAGE_KEY = "aiToolDiagnosticLogV1";
const DEFAULT_HUB_NOTICE = "所有设置和历史都只保存在本机。";
const MIN_RECORDING_MS = 800;
const SUBTITLE_CHUNK_MS = 1800;
const SUBTITLE_NATIVE_CHUNK_MS = 8000;
const SUBTITLE_OVERLAP_MS = 450;
const SUBTITLE_MIN_CHUNK_MS = 1000;
const SUBTITLE_MAX_CHUNK_MS = 4200;
const SUBTITLE_SILENCE_FINALIZE_MS = 1200;
const SUBTITLE_FORCE_FINALIZE_MS = 8000;
const SUBTITLE_HIDE_DELAY_MS = 4200;
const SUBTITLE_DISPATCH_INTERVAL_MS = 360;
const SUBTITLE_SOUND_LEVEL_THRESHOLD = 0.018;
const SUBTITLE_AUDIO_SETUP_TIMEOUT_MS = 8000;
const SUBTITLE_STARTUP_STEP_TIMEOUT_MS = 10000;
const SUBTITLE_NATIVE_CAPTURE_TIMEOUT_MS = SUBTITLE_NATIVE_CHUNK_MS + 5000;
const NATIVE_SYSTEM_AUDIO_DEVICE_ID = "native-process-tap";
const MAX_SUBTITLE_HISTORY_ITEMS = 160;
const HUB_DIAGNOSTICS_REFRESH_INTERVAL_MS = 4000;
const MAX_DIAGNOSTIC_LOG_ITEMS = 160;
const EMPTY_TRANSCRIPTION_MARKERS = [
  "无实际内容输出",
  "没有实际内容输出",
  "无有效语音",
  "未识别到语音",
  "无语音内容",
  "无内容输出",
];
const DEFAULT_SHORTCUTS: ShortcutConfig = {
  dictate: "ctrl+p",
  translate: "ctrl+t",
  ask: "ctrl+space",
  polish: "ctrl+shift+p",
  subtitle: "ctrl+shift+s",
};
const ICON_RENDERERS = {
  book: BookOpen,
  check: CheckSmall,
  close: CloseSmall,
  copy: Copy,
  dashboard: Dashboard,
  history: History,
  keyboard: KeyboardOne,
  message: MessageOne,
  microphone: Microphone,
  play: PlayOne,
  plus: Plus,
  refresh: Refresh,
  setting: Setting,
  terminal: Terminal,
  translate: Translate,
  voice: VoiceInput,
} as const;

type VoiceMode = "dictate" | "translate" | "ask" | "polish";
type ShortcutMode = VoiceMode | "subtitle";
type StatusState = "idle" | "ready" | "recording" | "busy" | "error";
type HubNoticeState = "idle" | "busy" | "success" | "error";
type WindowMode = "main" | "hub" | "toast" | "result" | "subtitle" | "subtitleHistory";
type HistoryRetention = "forever" | "30" | "7" | "never";
type DictionaryFilter = "all" | "auto" | "manual";
type DiagnosticState = "idle" | "success" | "warning" | "error";
type DiagnosticLogLevel = "info" | "success" | "warning" | "error";
type DiagnosticLogCategory = "recording" | "transcribe" | "process" | "paste" | "subtitle" | "system";
type SubtitleAudioSource = "microphone" | "system" | "mixed";
type SubtitleOverlayState = "hidden" | "listening" | "text" | "error";
type IconName = keyof typeof ICON_RENDERERS;
type ReadinessAction = "apiKey" | "microphone" | "accessibility" | "shortcut" | "modes" | "start" | "refresh";
type PermissionKind = "apiKey" | "microphone" | "accessibility" | "shortcut" | "systemAudio";

interface PendingConfirmation {
  /** 本次等待确认的动作 ID，用于区分不同危险操作。 */
  id: string;
  /** 进入确认态的按钮元素。 */
  button: HTMLButtonElement;
  /** 按钮进入确认态前的原始文案。 */
  originalLabel: string;
  /** 自动退出确认态的计时器。 */
  timeoutHandle: number;
}

interface ShortcutRecordingSnapshot {
  /** 当前正在录制快捷键的模式。 */
  mode: ShortcutMode;
  /** 进入录制态之前输入框展示的快捷键文本。 */
  label: string;
}

interface ShortcutTriggerPayload {
  /** 快捷键触发的模式。 */
  mode: ShortcutMode;
  /** 按下快捷键瞬间的前台目标 App。 */
  targetApp: string;
  /** 本次录音是否来自 Hub 主界面，需要避免影响 Hub 显示。 */
  keepHubVisible?: boolean;
}

interface ShortcutConfig {
  /** 听写模式全局快捷键。 */
  dictate: string;
  /** 翻译模式全局快捷键。 */
  translate: string;
  /** 随便问模式全局快捷键。 */
  ask: string;
  /** 润色模式全局快捷键。 */
  polish: string;
  /** 实时字幕监听模式全局快捷键。 */
  subtitle: string;
}

interface SubtitleRecorderChunk {
  /** 实时字幕录音片段序号，用于诊断日志定位连续切片。 */
  index: number;
  /** 当前片段完整音频 Blob。 */
  blob: Blob;
  /** 浏览器录音器产出的原始音频格式。 */
  mimeType: string;
}

interface VoiceConfig {
  /** OpenAI 兼容接口地址。 */
  baseUrl: string;
  /** 语音识别模型名称。 */
  asrModel: string;
  /** AI 文本处理模型名称。 */
  textModel: string;
  /** 语音识别语言，auto 表示自动识别。 */
  language: string;
  /** 翻译目标语言列表。 */
  targetLanguages: string[];
  /** 历史记录保留策略。 */
  historyRetention: HistoryRetention;
  /** 口述后是否执行 AI 润色。 */
  postProcessDictation: boolean;
  /** 口述 AI 润色后的输出语言，source 表示跟随原文。 */
  dictationOutputLanguage: string;
  /** 选定的麦克风设备 ID，default 表示系统默认设备。 */
  microphoneDeviceId: string;
  /** 实时字幕使用的系统音频输入设备，auto 表示自动检测虚拟声卡，none 表示只采集麦克风。 */
  systemAudioDeviceId: string;
  /** 实时字幕是否同时采集麦克风输入。 */
  subtitleIncludeMicrophone: boolean;
  /** 实时字幕原生系统音频采集目标，active 表示自动选择正在发声的 App。 */
  subtitleTargetApps: string[];
  /** 是否播放开始和停止录音提示音。 */
  interactionSounds: boolean;
  /** 录音期间是否临时静音系统输出。 */
  muteWhileDictating: boolean;
  /** 是否开机后自动启动。 */
  launchAtLogin: boolean;
  /** 是否在 Dock 中展示图标。 */
  showInDock: boolean;
  /** 各语音与字幕模式的快捷键配置。 */
  shortcuts: ShortcutConfig;
  /** 所有 AI 文本处理共同遵循的本地输出偏好。 */
  personalStyle: string;
  /** 只在口述润色时追加的本地输出偏好。 */
  dictationStyle: string;
  /** 只在翻译时追加的本地输出偏好。 */
  translationStyle: string;
  /** 只在随便问时追加的本地回答偏好。 */
  askStyle: string;
  /** 只在选中文本润色时追加的本地输出偏好。 */
  polishStyle: string;
}

interface TranscribeRequest {
  /** 小米 Mimo 接口密钥；桌面端通常为空，交给 Rust 读取会话密钥。 */
  apiKey: string;
  /** OpenAI 兼容接口地址。 */
  baseUrl: string;
  /** 语音识别模型名称。 */
  asrModel: string;
  /** 语音识别语言，auto 表示自动识别。 */
  language: string;
  /** 音频 MIME 类型。 */
  contentType: string;
  /** 音频 base64 内容，不包含 data URL 头。 */
  audioBase64: string;
}

interface TranscribeResponse {
  /** 转写后的文字。 */
  text: string;
  /** 服务端统计的转写耗时。 */
  elapsedMs: number;
  /** 实际返回的模型名称。 */
  model: string;
}

interface ProcessTapCaptureResponse {
  /** WAV 音频 base64 内容，不包含 data URL 头。 */
  audioBase64: string;
  /** 音频 MIME 类型。 */
  contentType: string;
  /** 采集到的音频字节数。 */
  bytes: number;
  /** 原生 helper 输出的目标进程、帧数和采样率摘要。 */
  summary: string;
  /** 本地采集总耗时。 */
  elapsedMs: number;
}

interface ProcessTapAudioApp {
  /** 音频进程 PID，可作为精确采集目标。 */
  pid: number;
  /** App 或进程名称。 */
  name: string;
  /** App Bundle ID。 */
  bundleId: string;
  /** 当前是否有运行中的音频。 */
  audioActive: boolean;
}

interface ProcessTapTranscribeResponse {
  /** 字幕片段序号。 */
  chunkIndex: number;
  /** 转写后的文字。 */
  text: string;
  /** 实际返回的模型名称。 */
  model: string;
  /** ASR 请求耗时。 */
  elapsedMs: number;
  /** 本地采集总耗时。 */
  captureElapsedMs: number;
  /** 采集到的音频字节数。 */
  bytes: number;
  /** 原生 helper 输出的目标进程、帧数和采样率摘要。 */
  summary: string;
}

interface ProcessTapTranscribeOutcome {
  /** 字幕片段序号。 */
  chunkIndex: number;
  /** 任务是否成功。 */
  ok: boolean;
  /** 成功时的转写结果。 */
  response?: ProcessTapTranscribeResponse;
  /** 失败时的错误原因。 */
  error?: string;
}

interface SelectedTextResponse {
  /** 从当前外部 App 读到的选中文本。 */
  text: string;
  /** 读取选中文本前的前台 App。 */
  targetApp: string;
  /** 读取时辅助功能权限是否已授权。 */
  accessibilityTrusted: boolean;
  /** 读取完成后是否恢复原剪贴板。 */
  clipboardRestored: boolean;
  /** 原剪贴板恢复说明。 */
  clipboardRestoreMessage: string;
  /** 触发复制时使用的系统路径。 */
  copyMethod: string;
}

interface ProcessTextRequest {
  /** 小米 Mimo 接口密钥；桌面端通常为空，交给 Rust 读取会话密钥。 */
  apiKey: string;
  /** OpenAI 兼容接口地址。 */
  baseUrl: string;
  /** AI 文本处理模型名称。 */
  textModel: string;
  /** AI 文本处理模式。 */
  mode: VoiceMode;
  /** ASR 原文或用户输入文本。 */
  text: string;
  /** 本地词典术语。 */
  dictionary: string[];
  /** 翻译目标语言。 */
  targetLanguages: string[];
  /** 触发录音时的前台应用名称。 */
  contextApp: string;
  /** 本地个性化输出偏好。 */
  styleInstruction: string;
}

interface ProcessTextResponse {
  /** AI 处理后的文字。 */
  processedText: string;
  /** AI 处理耗时。 */
  elapsedMs: number;
  /** 实际返回的模型名称。 */
  model: string;
}

interface PasteResponse {
  /** 是否已成功发出系统粘贴指令；macOS 不提供可靠的输入框插入回调。 */
  pasted: boolean;
  /** 自动粘贴后的状态说明。 */
  message: string;
  /** 是否需要用户授予辅助功能权限。 */
  requiresAccessibility: boolean;
  /** 桌面端尝试恢复的目标应用。 */
  targetApp: string;
  /** 是否已经把最终输出写入系统剪贴板。 */
  clipboardWritten: boolean;
  /** 剪贴板读回内容是否与本次输出一致。 */
  clipboardMatchesExpected: boolean;
  /** 是否尝试恢复用户原本的系统剪贴板。 */
  clipboardRestoreAttempted: boolean;
  /** 用户原本的系统剪贴板是否已恢复。 */
  clipboardRestored: boolean;
  /** 剪贴板恢复状态说明，不包含剪贴板正文。 */
  clipboardRestoreMessage: string;
  /** Rust 侧触发粘贴前检测到的辅助功能授权状态。 */
  accessibilityTrusted: boolean;
  /** 本次粘贴指令的触发方式，便于区分 System Events 和 CoreGraphics 兜底。 */
  pasteMethod: string;
  /** 隐藏 typesass 窗口前系统前台应用。 */
  frontmostBeforePaste: string;
  /** 尝试激活目标 App 后系统前台应用。 */
  frontmostAfterActivate: string;
  /** 粘贴指令发出后系统前台应用。 */
  frontmostAfterPaste: string;
  /** 是否已从目标输入框确认本次输出；快速模式下默认不做慢速回读。 */
  insertionVerified: boolean;
  /** 粘贴校验说明，不包含目标输入框正文。 */
  verificationStatus: string;
  /** 发送粘贴指令前目标 App 内的系统焦点元素。 */
  focusedElementBeforePaste: string;
  /** 激活目标 App 后的系统焦点元素。 */
  focusedElementAfterActivate: string;
  /** 发送粘贴指令后的系统焦点元素。 */
  focusedElementAfterPaste: string;
}

interface ResultWindowPayload {
  /** 本次需要用户手动处理的最终输出。 */
  text: string;
  /** 自动粘贴没有完成的原因。 */
  reason: string;
  /** 是否需要展示辅助功能设置入口。 */
  requiresAccessibility: boolean;
}

interface HubNoticePayload {
  /** 托盘或桌面端命令需要 Hub 展示的反馈文案。 */
  message: string;
  /** 反馈状态，用于复用 Hub 顶部提示样式。 */
  state: HubNoticeState;
}

interface SubtitleOverlayPayload {
  /** 当前要展示在底部字幕条里的文本。 */
  text: string;
  /** 底部字幕条是否可见。 */
  visible: boolean;
  /** 字幕窗口当前状态。 */
  state: SubtitleOverlayState;
  /** 触发本次更新的时间戳。 */
  updatedAt: number;
}

interface SubtitleHistoryItem {
  /** 本地字幕历史 ID。 */
  id: string;
  /** 固化后的字幕正文。 */
  text: string;
  /** 字幕固化时间戳。 */
  createdAt: number;
  /** 本条字幕来自麦克风、系统音频或混合音频。 */
  source: SubtitleAudioSource;
  /** 当前字幕片段累计耗时。 */
  elapsedMs: number;
  /** ASR 实际返回模型名称。 */
  model: string;
}

interface SubtitleHistoryUpdatePayload {
  /** 历史窗口状态文案。 */
  status: string;
  /** 当前监听是否开启。 */
  listening: boolean;
}

interface SubtitleSampleChunk {
  /** 当前分片在字幕音频流里的绝对起始采样点。 */
  startSample: number;
  /** 当前分片的单声道 PCM 数据。 */
  samples: Float32Array;
  /** 当前分片估算音量，用于静音判断。 */
  level: number;
}

interface RuntimeDiagnostics {
  /** 当前会话内存里是否已有 Mimo Key。 */
  hasSessionApiKey: boolean;
  /** macOS 钥匙串里是否已保存 Mimo Key。 */
  hasKeychainApiKey: boolean;
  /** 启动环境变量里是否已有 Mimo Key。 */
  hasEnvApiKey: boolean;
  /** macOS 辅助功能权限是否已授权。 */
  accessibilityTrusted: boolean;
  /** Rust 侧当前注册的各模式快捷键。 */
  shortcuts: ShortcutConfig;
  /** 当前全局快捷键是否已成功注册到系统。 */
  shortcutRegistrationReady: boolean;
  /** 最近一次全局快捷键注册结果说明。 */
  shortcutRegistrationMessage: string;
}

interface RuntimePermissionSnapshot {
  /** 当前环境是否为 Tauri 桌面端，决定能否打开系统权限页。 */
  isDesktopRuntime: boolean;
  /** 当前模式能否读取到 Mimo Key。 */
  hasApiKey: boolean;
  /** Mimo Key 的来源或缺失说明。 */
  apiKeyText: string;
  /** 麦克风或输入设备检测状态。 */
  microphoneState: DiagnosticState;
  /** 麦克风或输入设备状态文案。 */
  microphoneText: string;
  /** macOS 辅助功能是否已授权。 */
  accessibilityReady: boolean;
  /** 全局快捷键是否已成功注册。 */
  shortcutReady: boolean;
  /** 全局快捷键状态文案。 */
  shortcutText: string;
  /** 实时字幕系统音频配置状态。 */
  systemAudioState: DiagnosticState;
  /** 实时字幕系统音频配置文案。 */
  systemAudioText: string;
}

interface ModePermissionItem {
  /** 权限类型，用于弹窗说明和跳转目标。 */
  kind: PermissionKind;
  /** 展示给用户的权限名称。 */
  label: string;
  /** 当前权限是否满足本模式运行要求。 */
  ready: boolean;
  /** 权限状态颜色。 */
  state: DiagnosticState;
  /** 当前状态说明。 */
  description: string;
}

interface HistoryItem {
  /** 本地历史记录 ID。 */
  id: string;
  /** 语音模式。 */
  mode: VoiceMode;
  /** ASR 原文。 */
  sourceText: string;
  /** 最终输出文字。 */
  outputText: string;
  /** 创建时间戳。 */
  createdAt: number;
  /** 录音耗时。 */
  recordElapsedMs: number;
  /** 转写耗时。 */
  transcribeElapsedMs: number;
  /** AI 处理耗时。 */
  processElapsedMs: number;
  /** 模型说明。 */
  model: string;
  /** 触发录音时的前台应用名称。 */
  contextApp: string;
}

interface DictionaryItem {
  /** 本地词条 ID。 */
  id: string;
  /** 词条文本。 */
  word: string;
  /** 词条来源：手动或自动候选。 */
  source: "manual" | "auto";
  /** 创建时间戳。 */
  createdAt: number;
}

interface DiagnosticLogDraft {
  /** 日志等级，用于快速区分正常、警告和错误。 */
  level: DiagnosticLogLevel;
  /** 日志所属链路阶段。 */
  category: DiagnosticLogCategory;
  /** 日志标题，展示给用户快速扫描。 */
  title: string;
  /** 不含转写正文的诊断说明。 */
  message: string;
  /** 关联的语音或字幕模式。 */
  mode?: ShortcutMode;
  /** 本次链路记录的目标 App。 */
  targetApp?: string;
  /** 写日志时观测到的前台 App。 */
  frontmostApp?: string;
  /** 粘贴动作使用的系统触发方式。 */
  pasteMethod?: string;
  /** 当时辅助功能权限是否已授权。 */
  accessibilityTrusted?: boolean;
  /** 是否已成功写入剪贴板。 */
  clipboardWritten?: boolean;
  /** 剪贴板读回内容是否与本次输出一致。 */
  clipboardMatchesExpected?: boolean;
  /** 是否尝试恢复用户原本的系统剪贴板。 */
  clipboardRestoreAttempted?: boolean;
  /** 用户原本的系统剪贴板是否已恢复。 */
  clipboardRestored?: boolean;
  /** 剪贴板恢复状态说明，不包含剪贴板正文。 */
  clipboardRestoreMessage?: string;
  /** 是否已从目标输入框确认本次输出；快速模式下默认不做慢速回读。 */
  insertionVerified?: boolean;
  /** 粘贴校验说明，不包含目标输入框正文。 */
  verificationStatus?: string;
  /** 发送粘贴指令前的系统焦点元素摘要。 */
  focusedElementBeforePaste?: string;
  /** 激活目标 App 后的系统焦点元素摘要。 */
  focusedElementAfterActivate?: string;
  /** 发送粘贴指令后的系统焦点元素摘要。 */
  focusedElementAfterPaste?: string;
  /** 当前阶段耗时，单位毫秒。 */
  elapsedMs?: number;
  /** 额外诊断字段，严禁放入转写正文。 */
  details?: string[];
}

interface DiagnosticLogItem extends DiagnosticLogDraft {
  /** 本地日志 ID。 */
  id: string;
  /** 记录创建时间戳。 */
  createdAt: number;
  /** 额外诊断字段，已做类型兜底。 */
  details: string[];
}

interface TauriWindow extends Window {
  /** Tauri 运行时注入对象，浏览器预览模式不存在。 */
  __TAURI_INTERNALS__?: unknown;
  /** Rust 快捷键直达前端的处理函数。 */
  __AIToolHandleShortcutMode?: (mode: ShortcutMode, targetApp?: string, keepHubVisible?: boolean) => void;
  /** Rust 原生字幕后台任务完成后的处理函数。 */
  __AIToolHandleNativeSubtitleOutcome?: (payload: ProcessTapTranscribeOutcome) => void;
  /** 前端尚未加载完成时暂存的快捷键触发模式。 */
  __AIToolPendingShortcutMode?: ShortcutMode | ShortcutTriggerPayload;
  /** Rust 结果窗口直达前端的渲染函数。 */
  __AIToolRenderResult?: (payload: ResultWindowPayload) => void;
}

const MODE_LABELS: Record<VoiceMode, string> = {
  dictate: "口述",
  translate: "翻译",
  ask: "随便问",
  polish: "润色",
};

const SHORTCUT_MODE_LABELS: Record<ShortcutMode, string> = {
  dictate: "口述",
  translate: "翻译",
  ask: "随便问",
  polish: "润色",
  subtitle: "实时字幕",
};

const MODE_START_LABELS: Record<VoiceMode, string> = {
  dictate: "开始口述",
  translate: "开始翻译",
  ask: "开始提问",
  polish: "润色选中文本",
};

const MODE_ACTION_ICONS: Record<VoiceMode, IconName> = {
  dictate: "microphone",
  translate: "translate",
  ask: "message",
  polish: "check",
};

const DIAGNOSTIC_LOG_LEVEL_LABELS: Record<DiagnosticLogLevel, string> = {
  info: "信息",
  success: "成功",
  warning: "警告",
  error: "错误",
};

const DIAGNOSTIC_LOG_CATEGORY_LABELS: Record<DiagnosticLogCategory, string> = {
  recording: "录音",
  transcribe: "转写",
  process: "AI",
  paste: "粘贴",
  subtitle: "字幕",
  system: "系统",
};

const MODE_DETAIL_VIEWS: Record<ShortcutMode, string> = {
  dictate: "dictateSettings",
  translate: "translateSettings",
  ask: "askSettings",
  polish: "polishSettings",
  subtitle: "subtitleSettings",
};

const VIEW_TITLES: Record<string, { eyebrow: string; title: string }> = {
  home: { eyebrow: "说话，不要打字", title: "仪表盘" },
  modes: { eyebrow: "选择真实语音流程", title: "语音模式" },
  dictateSettings: { eyebrow: "只影响口述", title: "口述设置" },
  translateSettings: { eyebrow: "只影响翻译", title: "翻译设置" },
  askSettings: { eyebrow: "只影响随便问", title: "随便问设置" },
  polishSettings: { eyebrow: "只影响润色", title: "润色设置" },
  subtitleSettings: { eyebrow: "只影响实时字幕", title: "实时字幕设置" },
  history: { eyebrow: "只保存在本机", title: "历史记录" },
  dictionary: { eyebrow: "专有名词更准确", title: "词典" },
  diagnosticsLog: { eyebrow: "定位自动粘贴问题", title: "诊断日志" },
  settings: { eyebrow: "本机配置", title: "系统设置" },
};

const windowMode = getWindowMode();
const floatShell = getElement<HTMLElement>("floatShell");
const hubShell = getElement<HTMLElement>("hubShell");
const resultShell = getElement<HTMLElement>("resultShell");
const statusBubble = getElement<HTMLDivElement>("statusBubble");
const cancelButton = getElement<HTMLButtonElement>("cancelButton");
const recordButton = getElement<HTMLButtonElement>("recordButton");
const copyButton = getElement<HTMLButtonElement>("copyButton");
const soundStage = getElement<HTMLButtonElement>("soundStage");
const recordTimer = getElement<HTMLDivElement>("recordTimer");
const statusText = getElement<HTMLDivElement>("statusText");
const runtimeBadge = getElement<HTMLDivElement>("runtimeBadge");
const recordDurationText = getElement<HTMLElement>("recordDurationText");
const transcribeDurationText = getElement<HTMLElement>("transcribeDurationText");
const processDurationText = getElement<HTMLElement>("processDurationText");
const audioSizeText = getElement<HTMLElement>("audioSizeText");
const resultMeta = getElement<HTMLElement>("resultMeta");
const resultTextarea = getElement<HTMLTextAreaElement>("resultTextarea");
const apiKeyInput = getElement<HTMLInputElement>("apiKeyInput");
const clearApiKeyButton = getElement<HTMLButtonElement>("clearApiKeyButton");
const baseUrlInput = getElement<HTMLInputElement>("baseUrlInput");
const modelInput = getElement<HTMLInputElement>("modelInput");
const textModelInput = getElement<HTMLInputElement>("textModelInput");
const languageSelect = getElement<HTMLSelectElement>("languageSelect");
const targetLanguagesSelect = getElement<HTMLSelectElement>("targetLanguagesSelect");
const historyRetentionSelect = getElement<HTMLSelectElement>("historyRetentionSelect");
const postProcessDictationInput = getElement<HTMLInputElement>("postProcessDictationInput");
const dictationOutputLanguageRow = getElement<HTMLLabelElement>("dictationOutputLanguageRow");
const dictationOutputLanguageSelect = getElement<HTMLSelectElement>("dictationOutputLanguageSelect");
const microphoneSelect = getElement<HTMLSelectElement>("microphoneSelect");
const systemAudioSelect = getElement<HTMLSelectElement>("systemAudioSelect");
const subtitleIncludeMicrophoneInput = getElement<HTMLInputElement>("subtitleIncludeMicrophoneInput");
const subtitleTargetAppsSelect = getElement<HTMLSelectElement>("subtitleTargetAppsSelect");
const refreshSubtitleAppsButton = getElement<HTMLButtonElement>("refreshSubtitleAppsButton");
const refreshMicrophonesButton = getElement<HTMLButtonElement>("refreshMicrophonesButton");
const interactionSoundsInput = getElement<HTMLInputElement>("interactionSoundsInput");
const muteWhileDictatingInput = getElement<HTMLInputElement>("muteWhileDictatingInput");
const launchAtLoginInput = getElement<HTMLInputElement>("launchAtLoginInput");
const showInDockInput = getElement<HTMLInputElement>("showInDockInput");
const personalStyleInput = getElement<HTMLTextAreaElement>("personalStyleInput");
const dictationStyleInput = getElement<HTMLTextAreaElement>("dictationStyleInput");
const translationStyleInput = getElement<HTMLTextAreaElement>("translationStyleInput");
const askStyleInput = getElement<HTMLTextAreaElement>("askStyleInput");
const polishStyleInput = getElement<HTMLTextAreaElement>("polishStyleInput");
const dictateShortcutInput = getElement<HTMLInputElement>("dictateShortcutInput");
const translateShortcutInput = getElement<HTMLInputElement>("translateShortcutInput");
const askShortcutInput = getElement<HTMLInputElement>("askShortcutInput");
const polishShortcutInput = getElement<HTMLInputElement>("polishShortcutInput");
const subtitleShortcutInput = getElement<HTMLInputElement>("subtitleShortcutInput");
const shortcutValidationTexts = Array.from(document.querySelectorAll<HTMLElement>("[data-shortcut-validation]"));
const dictateShortcutText = getElement<HTMLElement>("dictateShortcutText");
const translateShortcutText = getElement<HTMLElement>("translateShortcutText");
const askShortcutText = getElement<HTMLElement>("askShortcutText");
const polishShortcutText = getElement<HTMLElement>("polishShortcutText");
const subtitleShortcutText = getElement<HTMLElement>("subtitleShortcutText");
const homeDictateShortcutText = getElement<HTMLElement>("homeDictateShortcutText");
const homeTranslateShortcutText = getElement<HTMLElement>("homeTranslateShortcutText");
const homeAskShortcutText = getElement<HTMLElement>("homeAskShortcutText");
const homePolishShortcutText = getElement<HTMLElement>("homePolishShortcutText");
const homeSubtitleShortcutText = getElement<HTMLElement>("homeSubtitleShortcutText");
const clearConfigButton = getElement<HTMLButtonElement>("clearConfigButton");
const quickStartButton = document.getElementById("quickStartButton") as HTMLButtonElement | null;
const refreshStatusButton = getElement<HTMLButtonElement>("refreshStatusButton");
const hubTitle = getElement<HTMLElement>("hubTitle");
const hubEyebrow = getElement<HTMLElement>("hubEyebrow");
const hubStatusText = getElement<HTMLElement>("hubStatusText");
const systemStateText = getElement<HTMLElement>("systemStateText");
const metricSessions = getElement<HTMLElement>("metricSessions");
const metricWords = getElement<HTMLElement>("metricWords");
const metricSpeed = getElement<HTMLElement>("metricSpeed");
const metricPersonalization = getElement<HTMLElement>("metricPersonalization");
const usageWords = getElement<HTMLElement>("usageWords");
const usageTrackFill = getElement<HTMLElement>("usageTrackFill");
const hubResultTextarea = getElement<HTMLTextAreaElement>("hubResultTextarea");
const latestResultMeta = getElement<HTMLElement>("latestResultMeta");
const copyHubResultButton = getElement<HTMLButtonElement>("copyHubResultButton");
const retryHubResultButton = getElement<HTMLButtonElement>("retryHubResultButton");
const nextStepPanel = getElement<HTMLElement>("nextStepPanel");
const nextStepTitle = getElement<HTMLElement>("nextStepTitle");
const nextStepDescription = getElement<HTMLElement>("nextStepDescription");
const nextStepPrimaryButton = getElement<HTMLButtonElement>("nextStepPrimaryButton");
const nextStepPrimaryIcon = getElement<HTMLElement>("nextStepPrimaryIcon");
const nextStepPrimaryLabel = getElement<HTMLElement>("nextStepPrimaryLabel");
const nextStepRefreshButton = getElement<HTMLButtonElement>("nextStepRefreshButton");
const historyList = getElement<HTMLElement>("historyList");
const clearHistoryButton = getElement<HTMLButtonElement>("clearHistoryButton");
const dictionaryForm = getElement<HTMLFormElement>("dictionaryForm");
const dictionaryInput = getElement<HTMLInputElement>("dictionaryInput");
const dictionarySearchInput = getElement<HTMLInputElement>("dictionarySearchInput");
const dictionaryList = getElement<HTMLElement>("dictionaryList");
const dictionaryImportInput = getElement<HTMLInputElement>("dictionaryImportInput");
const importDictionaryButton = getElement<HTMLButtonElement>("importDictionaryButton");
const exportDictionaryButton = getElement<HTMLButtonElement>("exportDictionaryButton");
const diagnosticLogList = getElement<HTMLElement>("diagnosticLogList");
const diagnosticLogCount = getElement<HTMLElement>("diagnosticLogCount");
const copyDiagnosticLogButton = getElement<HTMLButtonElement>("copyDiagnosticLogButton");
const clearDiagnosticLogButton = getElement<HTMLButtonElement>("clearDiagnosticLogButton");
const authorizeMicrophoneButton = getElement<HTMLButtonElement>("authorizeMicrophoneButton");
const apiKeyStatus = getElement<HTMLElement>("apiKeyStatus");
const microphoneStatus = getElement<HTMLElement>("microphoneStatus");
const accessibilityStatus = getElement<HTMLElement>("accessibilityStatus");
const shortcutStatus = getElement<HTMLElement>("shortcutStatus");
const homeApiKeyStatus = getElement<HTMLElement>("homeApiKeyStatus");
const homeMicrophoneStatus = getElement<HTMLElement>("homeMicrophoneStatus");
const homeAccessibilityStatus = getElement<HTMLElement>("homeAccessibilityStatus");
const homeShortcutStatus = getElement<HTMLElement>("homeShortcutStatus");
const refreshDiagnosticsButton = getElement<HTMLButtonElement>("refreshDiagnosticsButton");
const openAccessibilityButton = getElement<HTMLButtonElement>("openAccessibilityButton");
const resultReason = getElement<HTMLElement>("resultReason");
const resultWindowTextarea = getElement<HTMLTextAreaElement>("resultWindowTextarea");
const resultCopyButton = getElement<HTMLButtonElement>("resultCopyButton");
const resultOpenAccessibilityButton = getElement<HTMLButtonElement>("resultOpenAccessibilityButton");
const resultCloseButton = getElement<HTMLButtonElement>("resultCloseButton");
const permissionDialog = getElement<HTMLElement>("permissionDialog");
const permissionDialogEyebrow = getElement<HTMLElement>("permissionDialogEyebrow");
const permissionDialogTitle = getElement<HTMLElement>("permissionDialogTitle");
const permissionDialogBody = getElement<HTMLElement>("permissionDialogBody");
const permissionDialogSteps = getElement<HTMLOListElement>("permissionDialogSteps");
const permissionDialogPrimaryButton = getElement<HTMLButtonElement>("permissionDialogPrimaryButton");
const permissionDialogSecondaryButton = getElement<HTMLButtonElement>("permissionDialogSecondaryButton");
const permissionDialogCloseButton = getElement<HTMLButtonElement>("permissionDialogCloseButton");
const subtitleBubble = getElement<HTMLElement>("subtitleBubble");
const subtitleText = getElement<HTMLElement>("subtitleText");
const subtitleHistoryStatus = getElement<HTMLElement>("subtitleHistoryStatus");
const subtitleHistoryList = getElement<HTMLElement>("subtitleHistoryList");
const subtitleHistoryCopyButton = getElement<HTMLButtonElement>("subtitleHistoryCopyButton");
const subtitleHistoryClearButton = getElement<HTMLButtonElement>("subtitleHistoryClearButton");
const brandLogo = getElement<HTMLImageElement>("brandLogo");
const voiceLevelDots = Array.from(soundStage.querySelectorAll<HTMLSpanElement>("span"));
const VOICE_DOT_FACTORS = [0.48, 0.72, 0.94, 0.66, 1, 0.78, 0.9, 0.58, 0.42];

let recordingStream: MediaStream | null = null;
let audioContext: AudioContext | null = null;
let audioSource: MediaStreamAudioSourceNode | null = null;
let audioProcessor: ScriptProcessorNode | null = null;
let audioSink: GainNode | null = null;
let recordedSamples: Float32Array[] = [];
let recordedSampleLength = 0;
let recordedSampleRate = 0;
let isRecording = false;
let recordStartedAt = 0;
let timerHandle: number | null = null;
let bubbleTimerHandle: number | null = null;
let isProcessing = false;
let isPolishingSelection = false;
let isStartingRecording = false;
let isSubtitleListening = false;
let isStartingSubtitleListening = false;
let lastShortcutAt = 0;
let activeMode: VoiceMode = "dictate";
let historyFilter: VoiceMode | "all" = "all";
let dictionaryFilter: DictionaryFilter = "all";
let shortcutRecordingMode: ShortcutMode | null = null;
let previousSystemMuteState: boolean | null = null;
let recordingTargetApp = "";
let recordingKeepsHubVisible = false;
let resultCopyFeedbackTimer: number | null = null;
let nextReadinessAction: ReadinessAction = "apiKey";
let pendingConfirmation: PendingConfirmation | null = null;
let shortcutRecordingSnapshot: ShortcutRecordingSnapshot | null = null;
let accessibilityWatchHandle: number | null = null;
let hubDiagnosticsRefreshHandle: number | null = null;
let isRefreshingDiagnostics = false;
let configAutoSaveHandle: number | null = null;
let isConfigAutoSaving = false;
let activePermissionDialog: { mode: ShortcutMode; kind: PermissionKind } | null = null;
let lastPermissionSnapshot: RuntimePermissionSnapshot = createDefaultPermissionSnapshot();
let subtitleMicStream: MediaStream | null = null;
let subtitleSystemStream: MediaStream | null = null;
let subtitleAudioContext: AudioContext | null = null;
let subtitleMicSource: MediaStreamAudioSourceNode | null = null;
let subtitleSystemSource: MediaStreamAudioSourceNode | null = null;
let subtitleMixer: GainNode | null = null;
let subtitleProcessor: ScriptProcessorNode | null = null;
let subtitleSink: GainNode | null = null;
let subtitleMediaRecorder: MediaRecorder | null = null;
let subtitleRecorderMimeType = "";
let subtitleRecorderChunkIndex = 0;
let subtitleRecorderStopTimerHandle: number | null = null;
let subtitlePendingRecorderChunk: SubtitleRecorderChunk | null = null;
let isSubtitleUsingNativeSystemAudio = false;
let subtitleNativeChunkInFlight = false;
let subtitleNativeChunkTimeoutHandle: number | null = null;
let subtitleSampleChunks: SubtitleSampleChunk[] = [];
let subtitleTotalSamples = 0;
let subtitleDispatchedSampleEnd = 0;
let subtitleSampleRate = 0;
let subtitleDispatchTimerHandle: number | null = null;
let subtitleUiTimerHandle: number | null = null;
let subtitleOverlayHideTimerHandle: number | null = null;
let subtitleInFlight = false;
let subtitleDispatchQueued = false;
let subtitleStartedAt = 0;
let subtitleSegmentStartedAt = 0;
let subtitleLastSoundAt = 0;
let subtitleLastTextAt = 0;
let subtitlePendingText = "";
let subtitleLastDisplayedText = "";
let subtitleLastModel = "";
let subtitleCurrentSource: SubtitleAudioSource = "microphone";

init();

/** 初始化窗口模式、事件和本地数据。 */
function init(): void {
  document.body.dataset.windowMode = windowMode;
  setupBrandAndIcons();
  loadConfigToForm();

  if (windowMode === "toast") {
    void initToastWindow();
    return;
  }

  if (windowMode === "result") {
    void initResultWindow();
    return;
  }

  if (windowMode === "subtitle") {
    void initSubtitleWindow();
    return;
  }

  if (windowMode === "subtitleHistory") {
    void initSubtitleHistoryWindow();
    return;
  }

  if (windowMode === "main") {
    initFloatingWindow();
    return;
  }

  initHubWindow();
}

/** 初始化 typesass 品牌资源和 IconPark 图标。 */
function setupBrandAndIcons(): void {
  brandLogo.src = brandLogoUrl;
  setConfig({
    size: "1em",
    strokeWidth: 4,
    strokeLinecap: "round",
    strokeLinejoin: "round",
    prefix: "i",
    theme: "outline",
    colors: {
      outline: {
        fill: "currentColor",
        background: "transparent",
      },
      filled: {
        fill: "currentColor",
        background: "transparent",
      },
      twoTone: {
        fill: "currentColor",
        twoTone: "currentColor",
      },
      multiColor: {
        outStrokeColor: "currentColor",
        outFillColor: "transparent",
        innerStrokeColor: "currentColor",
        innerFillColor: "transparent",
      },
    },
  });
  document.querySelectorAll<HTMLElement>("[data-icon]").forEach((element) => {
    const iconName = element.dataset.icon;
    if (!isIconName(iconName)) {
      return;
    }
    renderIcon(element, iconName);
  });
}

/** 判断 DOM 上声明的图标名是否来自 IconPark 白名单。 */
function isIconName(value: string | undefined): value is IconName {
  return Boolean(value && value in ICON_RENDERERS);
}

/** 用 IconPark 渲染指定图标，动态按钮切换时保持和全局图标配置一致。 */
function renderIcon(element: HTMLElement, iconName: IconName): void {
  element.innerHTML = ICON_RENDERERS[iconName]({
    size: "1em",
    theme: "outline",
    fill: "currentColor",
  });
}

/** 读取当前 WebView 的窗口模式。 */
function getWindowMode(): WindowMode {
  const mode = new URLSearchParams(window.location.search).get("mode");
  if (mode === "hub" || mode === "toast" || mode === "result" || mode === "subtitle" || mode === "subtitleHistory") {
    return mode;
  }
  return "main";
}

/** 初始化悬浮录音条窗口。 */
function initFloatingWindow(): void {
  (window as TauriWindow).__AIToolHandleNativeSubtitleOutcome = (payload) => {
    void handleSubtitleNativeTranscribeOutcome(payload);
  };
  runtimeBadge.textContent = isTauriRuntime() ? "悬浮模式" : "网页预览";
  recordButton.title = "开始录音";
  recordButton.setAttribute("aria-label", "开始录音");
  bindNativeShortcutBridge();
  bindFloatingEvents();
  void registerShortcutsFromConfig(readSavedConfig());
  void applyDockPreference(readSavedConfig());
  setStatus("按快捷键开始录音。", "ready");
  void bindHubStartEvent();
}

/** 初始化 Hub 管理窗口。 */
function initHubWindow(): void {
  (window as TauriWindow).__AIToolHandleNativeSubtitleOutcome = (payload) => {
    void handleSubtitleNativeTranscribeOutcome(payload);
  };
  bindHubEvents();
  void bindHubControlEvents();
  void bindSubtitleNativeEvents();
  void populateMicrophones();
  void populateSubtitleTargetApps();
  void syncDesktopPreferences(readSavedConfig());
  renderHub();
  void refreshDiagnostics();
  startHubDiagnosticsRefresh();
  window.addEventListener("storage", renderHub);
  window.addEventListener("beforeunload", stopHubDiagnosticsRefresh);
}

/** 监听 Rust 端原生系统音频转写完成事件，避免长耗时 invoke 卡住实时字幕循环。 */
async function bindSubtitleNativeEvents(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<ProcessTapTranscribeResponse>("subtitle-native-transcribe-result", (event) => {
      void handleSubtitleNativeTranscribeResult(event.payload);
    });
  } catch (error) {
    showHubNotice(`实时字幕事件监听失败：${formatError(error)}`, "error");
  }
}

/** 监听其它窗口要求 Hub 切换页面的事件。 */
async function bindHubControlEvents(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<string>("hub-switch-view", (event) => {
      switchHubView(event.payload);
    });
    await listen<string[]>("hub-add-dictionary-words", (event) => {
      addDictionaryWordsFromTray(event.payload);
    });
    await listen<string>("hub-refresh-microphones", () => {
      switchHubView("settings");
      void populateMicrophones();
      showHubNotice("正在刷新麦克风列表。", "busy");
    });
    await listen<HubNoticePayload>("hub-show-notice", (event) => {
      showHubNotice(event.payload.message, normalizeHubNoticeState(event.payload.state));
    });
  } catch (error) {
    showHubNotice(`Hub 控制事件监听失败：${formatError(error)}`, "error");
  }
}

/** 初始化顶部错误气泡窗口，只监听错误消息。 */
async function initToastWindow(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<{ message: string }>("toast-message", (event) => {
      showLocalErrorBubble(event.payload.message);
    });
  } catch (error) {
    showLocalErrorBubble(`错误提示监听失败：${formatError(error)}`);
  }
}

/** 初始化结果兜底窗口，只在自动粘贴未完成时展示真实转写内容。 */
async function initResultWindow(): Promise<void> {
  bindResultWindowEvents();
  (window as TauriWindow).__AIToolRenderResult = renderResultWindow;
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<ResultWindowPayload>("result-message", (event) => {
      renderResultWindow(event.payload);
    });
    await restoreResultWindowPayload();
  } catch (error) {
    resultReason.textContent = `结果窗口监听失败：${formatError(error)}`;
    resultReason.dataset.state = "error";
  }
}

/** 初始化底部实时字幕窗口，只负责渲染当前字幕条。 */
async function initSubtitleWindow(): Promise<void> {
  renderSubtitleOverlay({ text: "", visible: false, state: "hidden", updatedAt: Date.now() });
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<SubtitleOverlayPayload>("subtitle-message", (event) => {
      renderSubtitleOverlay(event.payload);
    });
  } catch {
    renderSubtitleOverlay({ text: "", visible: false, state: "error", updatedAt: Date.now() });
  }
}

/** 初始化右上角字幕历史窗口，并监听主窗口的历史刷新事件。 */
async function initSubtitleHistoryWindow(): Promise<void> {
  bindSubtitleHistoryEvents();
  renderSubtitleHistory("等待字幕", false);
  window.addEventListener("storage", () => renderSubtitleHistory("历史已更新", isSubtitleListening));
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<SubtitleHistoryUpdatePayload>("subtitle-history-updated", (event) => {
      renderSubtitleHistory(event.payload.status, event.payload.listening);
    });
  } catch {
    subtitleHistoryStatus.textContent = "监听失败";
  }
}

/** 渲染底部字幕条当前状态，不在这里生成任何字幕内容。 */
function renderSubtitleOverlay(payload: SubtitleOverlayPayload): void {
  subtitleBubble.dataset.state = payload.state;
  subtitleBubble.dataset.visible = payload.visible ? "true" : "false";
  subtitleText.textContent = payload.text || (payload.state === "listening" ? "等待字幕" : "");
}

/** 绑定字幕历史窗口按钮事件。 */
function bindSubtitleHistoryEvents(): void {
  if (subtitleHistoryCopyButton.dataset.bound === "true") {
    return;
  }
  subtitleHistoryCopyButton.dataset.bound = "true";
  subtitleHistoryClearButton.dataset.bound = "true";
  subtitleHistoryCopyButton.addEventListener("click", () => {
    const content = readSubtitleHistory()
      .map((item) => item.text)
      .join("\n");
    if (!content.trim()) {
      return;
    }
    void navigator.clipboard.writeText(content);
    renderSubtitleHistory("已复制全部字幕", isSubtitleListening);
  });
  subtitleHistoryClearButton.addEventListener("click", () => {
    writeSubtitleHistory([]);
    renderSubtitleHistory("已清空", isSubtitleListening);
  });
}

/** 渲染右上角字幕历史窗口。 */
function renderSubtitleHistory(status: string, listening: boolean): void {
  isSubtitleListening = listening;
  const items = readSubtitleHistory();
  subtitleHistoryStatus.textContent = listening ? status || "字幕窗口已打开" : status || "等待字幕";
  subtitleHistoryCopyButton.disabled = !items.length;
  subtitleHistoryClearButton.disabled = !items.length;
  subtitleHistoryList.innerHTML = items.length
    ? items
        .map(
          (item) => `
            <article class="subtitleHistoryItem">
              <div class="historyMeta">
                <span>${item.source === "mixed" ? "混合音频" : item.source === "system" ? "系统音频" : "麦克风"}</span>
                <time>${formatDateTime(item.createdAt)}</time>
              </div>
              <p>${escapeHtml(item.text)}</p>
              <small>${formatDuration(item.elapsedMs)} · ${escapeHtml(item.model || "ASR")}</small>
            </article>`,
        )
        .join("")
    : '<div class="subtitleHistoryEmpty">还没有字幕。</div>';
}

/** 结果窗口初始化时从 Rust 内存恢复最近一次内容，避免首次打开时错过事件。 */
async function restoreResultWindowPayload(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const payload = await invoke<ResultWindowPayload | null>("get_last_result_window_payload");
  if (payload) {
    renderResultWindow(payload);
  }
}

/** 绑定 Rust 全局快捷键和前端录音模式。 */
function bindNativeShortcutBridge(): void {
  const tauriWindow = window as TauriWindow;
  tauriWindow.__AIToolHandleShortcutMode = handleShortcutMode;
  if (tauriWindow.__AIToolPendingShortcutMode) {
    const pendingShortcut = tauriWindow.__AIToolPendingShortcutMode;
    tauriWindow.__AIToolPendingShortcutMode = undefined;
    window.setTimeout(() => {
      if (typeof pendingShortcut === "string") {
        handleShortcutMode(pendingShortcut);
      } else {
        handleShortcutMode(pendingShortcut.mode, pendingShortcut.targetApp, pendingShortcut.keepHubVisible);
      }
    }, 0);
  }
}

/** 监听 Hub 发来的开始录音请求。 */
async function bindHubStartEvent(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<ShortcutMode | ShortcutTriggerPayload>("hub-start-mode", (event) => {
      if (typeof event.payload === "string") {
        handleShortcutMode(event.payload);
        return;
      }
      handleShortcutMode(event.payload.mode, event.payload.targetApp, event.payload.keepHubVisible);
    });
  } catch (error) {
    setStatus(`Hub 事件监听失败：${formatError(error)}`, "error");
  }
}

/** 绑定悬浮条上的按钮事件。 */
function bindFloatingEvents(): void {
  cancelButton.addEventListener("click", cancelRecordingOrReset);
  recordButton.addEventListener("click", () => void toggleRecording(activeMode));
  soundStage.addEventListener("click", () => void toggleRecording(activeMode));
  copyButton.addEventListener("click", () => void copyText(resultTextarea.value));
}

/** 绑定结果窗口复制、权限和关闭操作。 */
function bindResultWindowEvents(): void {
  resultCopyButton.addEventListener("click", () => void copyResultWindowText());
  resultOpenAccessibilityButton.addEventListener("click", () => void openAccessibilityFromResult());
  resultCloseButton.addEventListener("click", () => void hideResultWindow());
  window.addEventListener("keydown", handleResultWindowKeydown);
}

/** 处理结果窗口键盘操作：Esc 关闭，Command/Ctrl+C 复制，Command/Ctrl+Enter 复制并关闭。 */
function handleResultWindowKeydown(event: KeyboardEvent): void {
  if (windowMode !== "result") {
    return;
  }
  const normalizedKey = event.key.toLowerCase();
  const hasSystemModifier = event.metaKey || event.ctrlKey;
  if (normalizedKey === "escape") {
    event.preventDefault();
    void hideResultWindow();
    return;
  }
  if (hasSystemModifier && normalizedKey === "c") {
    event.preventDefault();
    void copyResultWindowText();
    return;
  }
  if (hasSystemModifier && normalizedKey === "enter") {
    event.preventDefault();
    void copyResultWindowText().then(() => hideResultWindow());
  }
}

/** 绑定 Hub 视图、历史、词典和设置事件。 */
function bindHubEvents(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-view-target]").forEach((button) => {
    button.addEventListener("click", () => switchHubView(button.dataset.viewTarget || "home"));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-view-jump]").forEach((button) => {
    button.addEventListener("click", () => switchHubView(button.dataset.viewJump || "home"));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-readiness-action]").forEach((button) => {
    button.addEventListener("click", () => void handleReadinessAction(button.dataset.readinessAction || ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-mode-start]").forEach((button) => {
    button.addEventListener("click", () => void requestFloatingMode(normalizeMode(button.dataset.modeStart)));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-mode-settings]").forEach((button) => {
    button.addEventListener("click", () => switchModeSettingsView(normalizeMode(button.dataset.modeSettings)));
  });
  document.querySelectorAll<HTMLElement>("[data-shortcut-zone]").forEach((zone) => {
    zone.addEventListener("click", (event) => handleShortcutZoneClick(event, normalizeShortcutMode(zone.dataset.shortcutZone)));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-history-filter]").forEach((button) => {
    button.addEventListener("click", () => {
      historyFilter = normalizeHistoryFilter(button.dataset.historyFilter);
      renderHistory();
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-dictionary-filter]").forEach((button) => {
    button.addEventListener("click", () => {
      dictionaryFilter = normalizeDictionaryFilter(button.dataset.dictionaryFilter);
      renderDictionary();
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-shortcut-record]").forEach((button) => {
    button.addEventListener("click", () => void startShortcutRecording(normalizeShortcutMode(button.dataset.shortcutRecord)));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-shortcut-reset]").forEach((button) => {
    button.addEventListener("click", () => resetShortcutInput(normalizeShortcutMode(button.dataset.shortcutReset)));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-subtitle-toggle]").forEach((button) => {
    button.addEventListener("click", () => void requestSubtitleMode());
  });
  document.querySelectorAll<HTMLButtonElement>("[data-permission-kind]").forEach((button) => {
    button.addEventListener("click", () =>
      openPermissionDialog(normalizeShortcutMode(button.dataset.permissionMode), normalizePermissionKind(button.dataset.permissionKind)),
    );
  });
  hubShell.addEventListener("click", handleHubPermissionClick);
  quickStartButton?.addEventListener("click", () => void requestFloatingMode("dictate"));
  bindConfigAutoSaveEvents();
  refreshStatusButton.addEventListener("click", () => void refreshHubRuntimeState());
  clearConfigButton.addEventListener("click", () => clearSavedConfig(clearConfigButton));
  clearApiKeyButton.addEventListener("click", () => void clearSavedApiKey());
  clearHistoryButton.addEventListener("click", () => clearHistory(clearHistoryButton));
  copyDiagnosticLogButton.addEventListener("click", () => void copyDiagnosticLogs());
  clearDiagnosticLogButton.addEventListener("click", () => clearDiagnosticLogs(clearDiagnosticLogButton));
  copyHubResultButton.addEventListener("click", () => void copyText(hubResultTextarea.value));
  retryHubResultButton.addEventListener("click", () => void retryLatestHistory());
  authorizeMicrophoneButton.addEventListener("click", () => void authorizeMicrophoneAccess());
  refreshMicrophonesButton.addEventListener("click", () => void populateMicrophones());
  refreshSubtitleAppsButton.addEventListener("click", () => void populateSubtitleTargetApps(true));
  importDictionaryButton.addEventListener("click", () => dictionaryImportInput.click());
  exportDictionaryButton.addEventListener("click", exportDictionaryCsv);
  refreshDiagnosticsButton.addEventListener("click", () => void refreshDiagnostics());
  openAccessibilityButton.addEventListener("click", () => void openAccessibilitySettings());
  nextStepPrimaryButton.addEventListener("click", () => void handleNextStepAction());
  nextStepRefreshButton.addEventListener("click", () => void refreshHubRuntimeState());
  permissionDialogPrimaryButton.addEventListener("click", () => void handlePermissionDialogPrimaryAction());
  permissionDialogSecondaryButton.addEventListener("click", closePermissionDialog);
  permissionDialogCloseButton.addEventListener("click", closePermissionDialog);
  permissionDialog.addEventListener("click", (event) => {
    if (event.target === permissionDialog) {
      closePermissionDialog();
    }
  });
  dictionaryImportInput.addEventListener("change", () => void importDictionaryCsv());
  dictionaryForm.addEventListener("submit", addDictionaryWord);
  dictionarySearchInput.addEventListener("input", renderDictionary);
  historyList.addEventListener("click", handleHistoryAction);
  dictionaryList.addEventListener("click", handleDictionaryAction);
  window.addEventListener("keydown", captureShortcutKeys, true);
}

/** 绑定设置控件的即时生效逻辑，用户修改后自动保存本地配置并同步桌面端偏好。 */
function bindConfigAutoSaveEvents(): void {
  const textControls: Array<HTMLInputElement | HTMLTextAreaElement> = [
    baseUrlInput,
    modelInput,
    textModelInput,
    personalStyleInput,
    dictationStyleInput,
    translationStyleInput,
    askStyleInput,
    polishStyleInput,
  ];
  const instantControls: Array<HTMLInputElement | HTMLSelectElement> = [
    languageSelect,
    targetLanguagesSelect,
    historyRetentionSelect,
    postProcessDictationInput,
    dictationOutputLanguageSelect,
    microphoneSelect,
    systemAudioSelect,
    subtitleIncludeMicrophoneInput,
    subtitleTargetAppsSelect,
    interactionSoundsInput,
    muteWhileDictatingInput,
    launchAtLoginInput,
    showInDockInput,
  ];
  textControls.forEach((control) => {
    control.addEventListener("input", () => scheduleConfigAutoSave("设置已自动生效。", 520));
    control.addEventListener("change", () => scheduleConfigAutoSave("设置已自动生效。", 0));
  });
  apiKeyInput.addEventListener("change", () => scheduleConfigAutoSave("Mimo Key 已自动生效。", 0));
  apiKeyInput.addEventListener("blur", () => scheduleConfigAutoSave("Mimo Key 已自动生效。", 0));
  instantControls.forEach((control) => {
    control.addEventListener("change", () => {
      if (control === postProcessDictationInput) {
        syncDictationPolishSwitches(postProcessDictationInput.checked);
      }
      scheduleConfigAutoSave("设置已自动生效。", 0);
    });
  });
}

/** 处理动态渲染出的模式权限按钮点击，打开该模式自己的权限说明。 */
function handleHubPermissionClick(event: MouseEvent): void {
  const target = event.target;
  if (!(target instanceof Element)) {
    return;
  }
  const button = target.closest<HTMLButtonElement>("[data-permission-kind]");
  if (!button) {
    return;
  }
  openPermissionDialog(normalizeShortcutMode(button.dataset.permissionMode), normalizePermissionKind(button.dataset.permissionKind));
}

/** 处理模式卡片快捷键区域点击；按钮自己处理，点击文字区域则直接进入录制。 */
function handleShortcutZoneClick(event: MouseEvent, mode: ShortcutMode): void {
  const target = event.target;
  if (target instanceof Element && target.closest("button")) {
    return;
  }
  void startShortcutRecording(mode);
}

/** 获取指定 DOM 元素，并保留准确类型。 */
function getElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`页面元素不存在：${id}`);
  }
  return element as T;
}

/** 判断当前是否运行在 Tauri WebView 中。 */
function isTauriRuntime(): boolean {
  return Boolean((window as TauriWindow).__TAURI_INTERNALS__);
}

/** 更新 Hub 顶部状态提示，只展示真实保存、错误和运行状态。 */
function showHubNotice(message: string, state: HubNoticeState): void {
  if (windowMode !== "hub") {
    return;
  }
  hubStatusText.textContent = message;
  hubStatusText.dataset.state = state;
}

/** 切换页面或完成短暂反馈后，恢复 Hub 顶部的默认本地隐私提示。 */
function resetHubNotice(): void {
  showHubNotice(DEFAULT_HUB_NOTICE, "idle");
}

/** 根据字符串恢复语音模式，非法值回落到听写。 */
function normalizeMode(value: string | undefined): VoiceMode {
  if (value === "translate" || value === "ask" || value === "polish") {
    return value;
  }
  return "dictate";
}

/** 根据字符串恢复全局快捷键模式，非法值回落到听写。 */
function normalizeShortcutMode(value: string | undefined): ShortcutMode {
  if (value === "subtitle") {
    return "subtitle";
  }
  return normalizeMode(value);
}

/** 根据字符串恢复权限类型，非法值回落到麦克风权限。 */
function normalizePermissionKind(value: string | undefined): PermissionKind {
  if (value === "apiKey" || value === "accessibility" || value === "shortcut" || value === "systemAudio") {
    return value;
  }
  return "microphone";
}

/** 从模式卡片或权限入口进入对应设置页，模式设置不改变任何录音触发状态。 */
function switchModeSettingsView(mode: ShortcutMode): void {
  switchHubView(MODE_DETAIL_VIEWS[mode]);
  showHubNotice(`正在编辑${SHORTCUT_MODE_LABELS[mode]}设置。`, "idle");
}

/** 根据字符串恢复历史筛选值，非法值回落到全部。 */
function normalizeHistoryFilter(value: string | undefined): VoiceMode | "all" {
  if (value === "dictate" || value === "translate" || value === "ask" || value === "polish") {
    return value;
  }
  return "all";
}

/** 根据字符串恢复词典筛选值，非法值回落到全部。 */
function normalizeDictionaryFilter(value: string | undefined): DictionaryFilter {
  if (value === "auto" || value === "manual") {
    return value;
  }
  return "all";
}

/** 规范化桌面端菜单传来的提示状态，避免异常 payload 破坏 Hub 样式。 */
function normalizeHubNoticeState(value: unknown): HubNoticeState {
  if (value === "busy" || value === "success" || value === "error") {
    return value;
  }
  return "idle";
}

/** 处理快捷键触发，并过滤按键连发造成的重复切换。 */
function handleShortcutMode(mode: ShortcutMode, targetApp = "", keepHubVisible = false): void {
  const now = Date.now();
  if (now - lastShortcutAt < 500) {
    addDiagnosticLog({
      level: "warning",
      category: "system",
      title: "快捷键触发过快",
      message: "系统已收到快捷键，但本次被防抖拦截。",
      mode,
      targetApp,
      details: [
        `状态：${isSubtitleListening ? "字幕监听中" : isRecording ? "录音中" : isProcessing ? "处理中" : "准备中"}`,
        `保持Hub：${keepHubVisible ? "是" : "否"}`,
      ],
    });
    flashFloatingNudge();
    if (isSubtitleListening) {
      void emitSubtitleHistoryUpdate("字幕监听中", true);
    } else if (isRecording) {
      setStatus("已经在录音，说完后再按一次停止。", "recording");
    } else if (isProcessing) {
      setStatus("正在处理上一段语音，请稍等。", "busy");
    } else {
      setStatus("正在准备麦克风，请稍等。", "busy");
    }
    return;
  }
  lastShortcutAt = now;
  addDiagnosticLog({
    level: "info",
    category: "system",
    title: "快捷键触发",
    message: mode === "polish" ? "已收到全局快捷键，准备润色选中文本。" : "已收到全局快捷键，准备切换录音状态。",
    mode,
    targetApp,
    details: [
      `状态：${mode === "subtitle" ? (isSubtitleListening ? "停止字幕监听" : "开始字幕监听") : mode === "polish" ? "润色选中文本" : isRecording ? "停止录音" : "开始录音"}`,
      `保持Hub：${keepHubVisible ? "是" : "否"}`,
    ],
  });
  if (mode === "subtitle") {
    void toggleSubtitleListening();
    return;
  }
  if (mode === "polish") {
    void polishSelectedText(targetApp, keepHubVisible);
    return;
  }
  if (isSubtitleListening || isStartingSubtitleListening) {
    addDiagnosticLog({
      level: "warning",
      category: "subtitle",
      title: "语音模式被字幕监听拦截",
      message: "实时字幕正在监听，先退出字幕模式再开始普通语音输入。",
      mode,
      targetApp,
    });
    void emitSubtitleHistoryUpdate("字幕监听中", true);
    return;
  }
  void toggleRecording(mode, targetApp, keepHubVisible);
}

/** 从本地存储读取配置并回填设置表单。 */
function loadConfigToForm(): void {
  const config = readSavedConfig();
  apiKeyInput.value = "";
  baseUrlInput.value = config.baseUrl;
  modelInput.value = config.asrModel;
  textModelInput.value = config.textModel;
  languageSelect.value = config.language;
  setTargetLanguageSelectValue(config.targetLanguages);
  historyRetentionSelect.value = config.historyRetention;
  postProcessDictationInput.checked = config.postProcessDictation;
  setSelectValueWithLegacyOption(dictationOutputLanguageSelect, config.dictationOutputLanguage);
  syncDictationOutputLanguageState(config.postProcessDictation);
  microphoneSelect.value = config.microphoneDeviceId;
  systemAudioSelect.value = config.systemAudioDeviceId;
  subtitleIncludeMicrophoneInput.checked = config.subtitleIncludeMicrophone;
  setSubtitleTargetAppsValue(config.subtitleTargetApps);
  interactionSoundsInput.checked = config.interactionSounds;
  muteWhileDictatingInput.checked = config.muteWhileDictating;
  launchAtLoginInput.checked = config.launchAtLogin;
  showInDockInput.checked = config.showInDock;
  personalStyleInput.value = config.personalStyle;
  dictationStyleInput.value = config.dictationStyle;
  translationStyleInput.value = config.translationStyle;
  askStyleInput.value = config.askStyle;
  polishStyleInput.value = config.polishStyle;
  dictateShortcutInput.value = formatShortcutLabel(config.shortcuts.dictate);
  translateShortcutInput.value = formatShortcutLabel(config.shortcuts.translate);
  askShortcutInput.value = formatShortcutLabel(config.shortcuts.ask);
  polishShortcutInput.value = formatShortcutLabel(config.shortcuts.polish);
  subtitleShortcutInput.value = formatShortcutLabel(config.shortcuts.subtitle);
  renderShortcutLabels(config.shortcuts);
  validateShortcutInputs();
}

/** 读取实时字幕 App 多选框的目标列表。 */
function readSelectedSubtitleTargetApps(): string[] {
  const selected = Array.from(subtitleTargetAppsSelect.selectedOptions)
    .map((option) => option.value.trim())
    .filter(Boolean);
  return selected.length ? selected : ["active"];
}

/** 回填实时字幕 App 多选框，列表尚未刷新时先补临时选项。 */
function setSubtitleTargetAppsValue(targets: string[]): void {
  const normalizedTargets = normalizeSubtitleTargetApps(targets, ["active"]);
  normalizedTargets.forEach((target) => {
    if (!Array.from(subtitleTargetAppsSelect.options).some((option) => option.value === target)) {
      const option = document.createElement("option");
      option.value = target;
      option.textContent = target === "active" ? "自动选择正在发声的 App" : target;
      subtitleTargetAppsSelect.appendChild(option);
    }
  });
  Array.from(subtitleTargetAppsSelect.options).forEach((option) => {
    option.selected = normalizedTargets.includes(option.value);
  });
}

/** 读取本地保存的非敏感语音配置。 */
function readSavedConfig(): VoiceConfig {
  const fallback = defaultConfig();
  const raw = localStorage.getItem(CONFIG_STORAGE_KEY);
  if (raw) {
    try {
      return normalizeConfig(JSON.parse(raw) as Partial<VoiceConfig>, fallback);
    } catch {
      return fallback;
    }
  }

  const legacyRaw = localStorage.getItem(LEGACY_CONFIG_STORAGE_KEY);
  if (!legacyRaw) {
    return fallback;
  }
  try {
    const legacy = JSON.parse(legacyRaw) as Partial<VoiceConfig> & { apiKey?: string };
    return normalizeConfig(legacy, fallback);
  } catch {
    return fallback;
  }
}

/** 生成默认配置。 */
function defaultConfig(): VoiceConfig {
  return {
    baseUrl: DEFAULT_BASE_URL,
    asrModel: DEFAULT_ASR_MODEL,
    textModel: DEFAULT_TEXT_MODEL,
    language: "auto",
    targetLanguages: [DEFAULT_TARGET_LANGUAGE],
    historyRetention: "forever",
    postProcessDictation: true,
    dictationOutputLanguage: DEFAULT_DICTATION_OUTPUT_LANGUAGE,
    microphoneDeviceId: "default",
    systemAudioDeviceId: NATIVE_SYSTEM_AUDIO_DEVICE_ID,
    subtitleIncludeMicrophone: true,
    subtitleTargetApps: ["active"],
    interactionSounds: true,
    muteWhileDictating: false,
    launchAtLogin: false,
    showInDock: false,
    shortcuts: { ...DEFAULT_SHORTCUTS },
    personalStyle: "",
    dictationStyle: "",
    translationStyle: "",
    askStyle: "",
    polishStyle: "",
  };
}

/** 对读取到的配置做类型兜底。 */
function normalizeConfig(value: Partial<VoiceConfig>, fallback: VoiceConfig): VoiceConfig {
  return {
    baseUrl: typeof value.baseUrl === "string" && value.baseUrl.trim() ? value.baseUrl : fallback.baseUrl,
    asrModel: typeof value.asrModel === "string" && value.asrModel.trim() ? value.asrModel : fallback.asrModel,
    textModel: typeof value.textModel === "string" && value.textModel.trim() ? value.textModel : fallback.textModel,
    language: typeof value.language === "string" && value.language.trim() ? value.language : fallback.language,
    targetLanguages: Array.isArray(value.targetLanguages)
      ? value.targetLanguages.filter((item): item is string => typeof item === "string" && Boolean(item.trim()))
      : fallback.targetLanguages,
    historyRetention: normalizeRetention(value.historyRetention),
    postProcessDictation:
      typeof value.postProcessDictation === "boolean" ? value.postProcessDictation : fallback.postProcessDictation,
    dictationOutputLanguage:
      typeof value.dictationOutputLanguage === "string" && value.dictationOutputLanguage.trim()
        ? value.dictationOutputLanguage
        : fallback.dictationOutputLanguage,
    microphoneDeviceId:
      typeof value.microphoneDeviceId === "string" && value.microphoneDeviceId.trim()
        ? value.microphoneDeviceId
        : fallback.microphoneDeviceId,
    systemAudioDeviceId:
      typeof value.systemAudioDeviceId === "string" && value.systemAudioDeviceId.trim() && value.systemAudioDeviceId !== "auto"
        ? value.systemAudioDeviceId
        : fallback.systemAudioDeviceId,
    subtitleIncludeMicrophone:
      typeof value.subtitleIncludeMicrophone === "boolean" ? value.subtitleIncludeMicrophone : fallback.subtitleIncludeMicrophone,
    subtitleTargetApps: normalizeSubtitleTargetApps(value.subtitleTargetApps, fallback.subtitleTargetApps),
    interactionSounds:
      typeof value.interactionSounds === "boolean" ? value.interactionSounds : fallback.interactionSounds,
    muteWhileDictating:
      typeof value.muteWhileDictating === "boolean" ? value.muteWhileDictating : fallback.muteWhileDictating,
    launchAtLogin: typeof value.launchAtLogin === "boolean" ? value.launchAtLogin : fallback.launchAtLogin,
    showInDock: typeof value.showInDock === "boolean" ? value.showInDock : fallback.showInDock,
    shortcuts: normalizeShortcuts(value.shortcuts, fallback.shortcuts),
    personalStyle: typeof value.personalStyle === "string" ? value.personalStyle : fallback.personalStyle,
    dictationStyle: typeof value.dictationStyle === "string" ? value.dictationStyle : fallback.dictationStyle,
    translationStyle: typeof value.translationStyle === "string" ? value.translationStyle : fallback.translationStyle,
    askStyle: typeof value.askStyle === "string" ? value.askStyle : fallback.askStyle,
    polishStyle: typeof value.polishStyle === "string" ? value.polishStyle : fallback.polishStyle,
  };
}

/** 对语音和字幕全局快捷键配置做兜底和去重保护。 */
function normalizeShortcuts(value: unknown, fallback: ShortcutConfig): ShortcutConfig {
  const source = isShortcutConfigLike(value) ? value : fallback;
  return {
    dictate: normalizeShortcutText(source.dictate, fallback.dictate),
    translate: normalizeShortcutText(source.translate, fallback.translate),
    ask: normalizeShortcutText(source.ask, fallback.ask),
    polish: normalizeShortcutText(source.polish, fallback.polish),
    subtitle: normalizeShortcutText(source.subtitle, fallback.subtitle),
  };
}

/** 判断读取到的快捷键对象是否具备基础结构。 */
function isShortcutConfigLike(value: unknown): value is Partial<ShortcutConfig> {
  return Boolean(value && typeof value === "object");
}

/** 规范化快捷键显示和注册文本。 */
function normalizeShortcutText(value: unknown, fallback: string): string {
  if (typeof value !== "string") {
    return fallback;
  }
  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "")
    .split("+")
    .filter(Boolean)
    .map(normalizeShortcutPart);
  const modifiers = ["ctrl", "cmd", "alt", "shift"];
  const orderedModifiers = modifiers.filter((modifier) => normalized.includes(modifier));
  const keys = normalized.filter((part) => !modifiers.includes(part));
  return [...orderedModifiers, ...keys].join("+") || fallback;
}

/** 统一快捷键片段别名，保证前端校验和 Rust 注册使用同一套语义。 */
function normalizeShortcutPart(part: string): string {
  if (part === "control") {
    return "ctrl";
  }
  if (part === "command" || part === "meta") {
    return "cmd";
  }
  if (part === "option") {
    return "alt";
  }
  if (part.startsWith("key") && part.length === 4) {
    return part.slice(3);
  }
  if (part.startsWith("digit") && part.length === 6) {
    return part.slice(5);
  }
  return part;
}

/** 检查各模式是否配置了重复快捷键，避免即时注册时系统失败。 */
function hasShortcutConflict(shortcuts: ShortcutConfig): boolean {
  const values = [shortcuts.dictate, shortcuts.translate, shortcuts.ask, shortcuts.polish, shortcuts.subtitle].map((shortcut) =>
    normalizeShortcutText(shortcut, ""),
  );
  return new Set(values).size !== values.length;
}

/** 对历史保留策略做枚举兜底。 */
function normalizeRetention(value: unknown): HistoryRetention {
  if (value === "forever" || value === "30" || value === "7" || value === "never") {
    return value;
  }
  return "forever";
}

/** 规范化实时字幕系统音频 App 目标，避免旧配置或空值导致无法启动。 */
function normalizeSubtitleTargetApps(value: unknown, fallback: string[]): string[] {
  if (!Array.isArray(value)) {
    return fallback;
  }
  const targets = value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter(Boolean);
  return targets.length ? Array.from(new Set(targets)) : fallback;
}

/** 从表单收集当前配置。 */
function readConfigFromForm(): VoiceConfig {
  return {
    baseUrl: baseUrlInput.value.trim() || DEFAULT_BASE_URL,
    asrModel: modelInput.value.trim() || DEFAULT_ASR_MODEL,
    textModel: textModelInput.value.trim() || DEFAULT_TEXT_MODEL,
    language: languageSelect.value || "auto",
    targetLanguages: [targetLanguagesSelect.value || DEFAULT_TARGET_LANGUAGE],
    historyRetention: normalizeRetention(historyRetentionSelect.value),
    postProcessDictation: postProcessDictationInput.checked,
    dictationOutputLanguage: dictationOutputLanguageSelect.value || DEFAULT_DICTATION_OUTPUT_LANGUAGE,
    microphoneDeviceId: microphoneSelect.value || "default",
    systemAudioDeviceId: systemAudioSelect.value || "auto",
    subtitleIncludeMicrophone: subtitleIncludeMicrophoneInput.checked,
    subtitleTargetApps: readSelectedSubtitleTargetApps(),
    interactionSounds: interactionSoundsInput.checked,
    muteWhileDictating: muteWhileDictatingInput.checked,
    launchAtLogin: launchAtLoginInput.checked,
    showInDock: showInDockInput.checked,
    shortcuts: {
      dictate: normalizeShortcutText(dictateShortcutInput.value, DEFAULT_SHORTCUTS.dictate),
      translate: normalizeShortcutText(translateShortcutInput.value, DEFAULT_SHORTCUTS.translate),
      ask: normalizeShortcutText(askShortcutInput.value, DEFAULT_SHORTCUTS.ask),
      polish: normalizeShortcutText(polishShortcutInput.value, DEFAULT_SHORTCUTS.polish),
      subtitle: normalizeShortcutText(subtitleShortcutInput.value, DEFAULT_SHORTCUTS.subtitle),
    },
    personalStyle: personalStyleInput.value.trim(),
    dictationStyle: dictationStyleInput.value.trim(),
    translationStyle: translationStyleInput.value.trim(),
    askStyle: askStyleInput.value.trim(),
    polishStyle: polishStyleInput.value.trim(),
  };
}

/** 回填翻译目标下拉框，并兼容旧版本手输保存的自定义语言。 */
function setTargetLanguageSelectValue(targetLanguages: string[]): void {
  const targetLanguage =
    targetLanguages.map((item) => item.trim()).find((item) => Boolean(item)) || DEFAULT_TARGET_LANGUAGE;
  setSelectValueWithLegacyOption(targetLanguagesSelect, targetLanguage);
}

/** 回填下拉框，并兼容旧版本存过的自定义值。 */
function setSelectValueWithLegacyOption(select: HTMLSelectElement, value: string): void {
  if (!Array.from(select.options).some((option) => option.value === value)) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = `${value}（已保存）`;
    select.appendChild(option);
  }
  select.value = value;
}

/** 延迟自动保存当前设置，避免用户连续输入时反复注册快捷键或写入系统偏好。 */
function scheduleConfigAutoSave(successMessage = "设置已自动生效。", delayMs = 360): void {
  if (configAutoSaveHandle !== null) {
    window.clearTimeout(configAutoSaveHandle);
  }
  configAutoSaveHandle = window.setTimeout(() => {
    configAutoSaveHandle = null;
    void saveConfigFromForm(successMessage);
  }, delayMs);
}

/** 保存配置；Mimo Key 只写入 macOS 钥匙串，不进入 localStorage。 */
async function saveConfigFromForm(successMessage = "设置已自动生效。"): Promise<void> {
  if (isConfigAutoSaving) {
    scheduleConfigAutoSave(successMessage, 260);
    return;
  }
  const config = readConfigFromForm();
  if (!validateShortcutInputs() || hasShortcutConflict(config.shortcuts)) {
    showHubNotice("快捷键配置需要处理后才能保存。", "error");
    return;
  }
  isConfigAutoSaving = true;
  showHubNotice("正在应用设置。", "busy");
  try {
    const apiKeyReady = await syncSavedApiKey();
    if (!apiKeyReady) {
      return;
    }
    const desktopReady = await syncDesktopPreferences(config);
    if (!desktopReady) {
      await refreshDiagnostics();
      showHubNotice("设置未生效，部分系统设置需要检查权限。", "error");
      return;
    }
    localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(config));
    syncDictationPolishSwitches(config.postProcessDictation);
    renderHub();
    await refreshDiagnostics();
    showHubNotice(successMessage, "success");
  } finally {
    isConfigAutoSaving = false;
  }
}

/** 同步顶部快捷开关和设置页开关，确保同一配置没有两个状态。 */
function syncDictationPolishSwitches(enabled: boolean): void {
  postProcessDictationInput.checked = enabled;
  syncDictationOutputLanguageState(enabled);
}

/** 根据口述 AI 润色开关启用或禁用输出语言设置。 */
function syncDictationOutputLanguageState(enabled: boolean): void {
  dictationOutputLanguageSelect.disabled = !enabled;
  dictationOutputLanguageRow.dataset.disabled = enabled ? "false" : "true";
  dictationOutputLanguageRow.title = enabled ? "AI 润色后会按这里设置输出。" : "关闭 AI 润色时会直接使用 ASR 原文。";
}

/** 刷新 Hub 上可见的运行状态和本地统计。 */
async function refreshHubRuntimeState(): Promise<void> {
  renderHub();
  await refreshDiagnostics();
  showHubNotice("状态已刷新。", "success");
}

/** 把设置页填写的密钥保存到 macOS 钥匙串，不写入 localStorage。 */
async function syncSavedApiKey(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  const apiKey = apiKeyInput.value.trim();
  if (!apiKey) {
    return true;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("save_api_key", { apiKey });
    apiKeyInput.value = "";
    return true;
  } catch (error) {
    showHubNotice(`Mimo Key 保存失败：${formatError(error)}`, "error");
    return false;
  }
}

/** 明确清除 macOS 钥匙串和当前会话中的 Mimo Key，执行前要求二次点击确认。 */
async function clearSavedApiKey(): Promise<void> {
  if (!isTauriRuntime()) {
    showHubNotice("网页预览模式不能清除钥匙串 Key。", "error");
    return;
  }
  if (!confirmDangerousAction("clearApiKey", clearApiKeyButton, "再次点击清除", "再次点击将清除本机保存的 Mimo Key。")) {
    return;
  }
  showHubNotice("正在清除 Mimo Key。", "busy");
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("clear_saved_api_key");
    apiKeyInput.value = "";
    await refreshDiagnostics();
    resetPendingConfirmation();
    showHubNotice("Mimo Key 已从当前会话和钥匙串清除。", "success");
  } catch (error) {
    showHubNotice(`Mimo Key 清除失败：${formatError(error)}`, "error");
  }
}

/** 同步需要桌面端参与的本地偏好，并返回系统能力是否全部成功。 */
async function syncDesktopPreferences(config: VoiceConfig): Promise<boolean> {
  const shortcutReady = await registerShortcutsFromConfig(config);
  const launchReady = await applyLaunchPreference(config);
  const dockReady = await applyDockPreference(config);
  return shortcutReady && launchReady && dockReady;
}

/** 把快捷键配置注册到 Rust 全局快捷键插件。 */
async function registerShortcutsFromConfig(config: VoiceConfig): Promise<boolean> {
  renderShortcutLabels(config.shortcuts);
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const normalized = await invoke<ShortcutConfig>("register_shortcuts", { shortcuts: config.shortcuts });
    dictateShortcutInput.value = formatShortcutLabel(normalized.dictate);
    translateShortcutInput.value = formatShortcutLabel(normalized.translate);
    askShortcutInput.value = formatShortcutLabel(normalized.ask);
    polishShortcutInput.value = formatShortcutLabel(normalized.polish);
    subtitleShortcutInput.value = formatShortcutLabel(normalized.subtitle);
    renderShortcutLabels(normalized);
    return true;
  } catch (error) {
    showHubNotice(`快捷键注册失败：${formatError(error)}`, "error");
    await restoreShortcutInputsFromRuntime();
    return false;
  }
}

/** 快捷键注册失败后把表单恢复成系统实际生效的旧快捷键，避免用户误以为失败组合键已启用。 */
async function restoreShortcutInputsFromRuntime(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const diagnostics = await readRuntimeDiagnostics();
    dictateShortcutInput.value = formatShortcutLabel(diagnostics.shortcuts.dictate);
    translateShortcutInput.value = formatShortcutLabel(diagnostics.shortcuts.translate);
    askShortcutInput.value = formatShortcutLabel(diagnostics.shortcuts.ask);
    polishShortcutInput.value = formatShortcutLabel(diagnostics.shortcuts.polish);
    subtitleShortcutInput.value = formatShortcutLabel(diagnostics.shortcuts.subtitle);
    renderShortcutLabels(diagnostics.shortcuts);
    validateShortcutInputs();
  } catch {
    // 读取诊断失败时保留当前错误提示，避免覆盖真正的注册失败原因。
  }
}

/** 根据设置切换开机启动。 */
async function applyLaunchPreference(config: VoiceConfig): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_login_launch", { enabled: config.launchAtLogin });
    return true;
  } catch (error) {
    showHubNotice(`开机启动设置失败：${formatError(error)}`, "error");
    return false;
  }
}

/** 根据设置切换 Dock 显示。 */
async function applyDockPreference(config: VoiceConfig): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_dock_visible", { visible: config.showInDock });
    return true;
  } catch {
    showHubNotice("Dock 显示切换失败。", "error");
    return false;
  }
}

/** 刷新快捷键展示文字。 */
function renderShortcutLabels(shortcuts: ShortcutConfig): void {
  dictateShortcutText.textContent = formatShortcutLabel(shortcuts.dictate);
  translateShortcutText.textContent = formatShortcutLabel(shortcuts.translate);
  askShortcutText.textContent = formatShortcutLabel(shortcuts.ask);
  polishShortcutText.textContent = formatShortcutLabel(shortcuts.polish);
  subtitleShortcutText.textContent = formatShortcutLabel(shortcuts.subtitle);
  homeDictateShortcutText.textContent = formatShortcutLabel(shortcuts.dictate);
  homeTranslateShortcutText.textContent = formatShortcutLabel(shortcuts.translate);
  homeAskShortcutText.textContent = formatShortcutLabel(shortcuts.ask);
  homePolishShortcutText.textContent = formatShortcutLabel(shortcuts.polish);
  homeSubtitleShortcutText.textContent = formatShortcutLabel(shortcuts.subtitle);
  updateFloatingShortcutTitle(shortcuts);
}

/** 刷新设置页里所有桌面能力的真实状态。 */
async function refreshDiagnostics(): Promise<void> {
  if (windowMode !== "hub" || isRefreshingDiagnostics) {
    return;
  }
  isRefreshingDiagnostics = true;
  try {
    systemStateText.textContent = isTauriRuntime() ? "本机运行中" : "网页预览";
    systemStateText.dataset.state = isTauriRuntime() ? "success" : "warning";
    const microphoneDiagnostic = await readMicrophoneDiagnostic();
    setDiagnosticStatus(microphoneStatus, microphoneDiagnostic.text, microphoneDiagnostic.state);
    setDiagnosticStatus(homeMicrophoneStatus, microphoneDiagnostic.text, microphoneDiagnostic.state);

    if (!isTauriRuntime()) {
      setDiagnosticStatus(apiKeyStatus, "仅桌面端可检测", "warning");
      setDiagnosticStatus(accessibilityStatus, "仅桌面端可检测", "warning");
      setDiagnosticStatus(shortcutStatus, "仅桌面端可注册", "warning");
      setDiagnosticStatus(homeApiKeyStatus, "仅桌面端可检测", "warning");
      setDiagnosticStatus(homeAccessibilityStatus, "仅桌面端可检测", "warning");
      setDiagnosticStatus(homeShortcutStatus, "仅桌面端可注册", "warning");
      lastPermissionSnapshot = {
        ...createDefaultPermissionSnapshot(),
        isDesktopRuntime: false,
        microphoneState: microphoneDiagnostic.state,
        microphoneText: microphoneDiagnostic.text,
        systemAudioState: readSystemAudioDiagnostic(readSavedConfig()).state,
        systemAudioText: readSystemAudioDiagnostic(readSavedConfig()).text,
      };
      renderModePermissions();
      updateReadinessSummary(false, microphoneDiagnostic.state, false, false, false);
      return;
    }

    try {
      const diagnostics = await readRuntimeDiagnostics();
      const hasApiKey = diagnostics.hasSessionApiKey || diagnostics.hasKeychainApiKey || diagnostics.hasEnvApiKey;
      const keyText = diagnostics.hasSessionApiKey
        ? "会话 Key 已就绪"
        : diagnostics.hasKeychainApiKey
          ? "钥匙串 Key 已就绪"
          : diagnostics.hasEnvApiKey
            ? "环境 Key 已就绪"
            : "未配置";
      setDiagnosticStatus(apiKeyStatus, keyText, hasApiKey ? "success" : "error");
      setDiagnosticStatus(homeApiKeyStatus, keyText, hasApiKey ? "success" : "error");
      setDiagnosticStatus(
        accessibilityStatus,
        diagnostics.accessibilityTrusted ? "已授权" : "未授权，自动粘贴会受影响",
        diagnostics.accessibilityTrusted ? "success" : "warning",
      );
      setDiagnosticStatus(
        homeAccessibilityStatus,
        diagnostics.accessibilityTrusted ? "已授权" : "未授权",
        diagnostics.accessibilityTrusted ? "success" : "warning",
      );
      const shortcutDiagnostic = formatShortcutDiagnostic(diagnostics);
      setDiagnosticStatus(shortcutStatus, shortcutDiagnostic.text, shortcutDiagnostic.state);
      setDiagnosticStatus(homeShortcutStatus, shortcutDiagnostic.homeText, shortcutDiagnostic.state);
      renderShortcutLabels(diagnostics.shortcuts);
      const systemAudioDiagnostic = readSystemAudioDiagnostic(readSavedConfig());
      lastPermissionSnapshot = {
        isDesktopRuntime: true,
        hasApiKey,
        apiKeyText: keyText,
        microphoneState: microphoneDiagnostic.state,
        microphoneText: microphoneDiagnostic.text,
        accessibilityReady: diagnostics.accessibilityTrusted,
        shortcutReady: diagnostics.shortcutRegistrationReady,
        shortcutText: shortcutDiagnostic.homeText,
        systemAudioState: systemAudioDiagnostic.state,
        systemAudioText: systemAudioDiagnostic.text,
      };
      renderModePermissions();
      updateReadinessSummary(
        hasApiKey,
        microphoneDiagnostic.state,
        diagnostics.accessibilityTrusted,
        diagnostics.shortcutRegistrationReady,
        true,
      );
    } catch (error) {
      setDiagnosticStatus(apiKeyStatus, "检测失败", "error");
      setDiagnosticStatus(accessibilityStatus, "检测失败", "error");
      setDiagnosticStatus(shortcutStatus, `检测失败：${formatError(error)}`, "error");
      setDiagnosticStatus(homeApiKeyStatus, "检测失败", "error");
      setDiagnosticStatus(homeAccessibilityStatus, "检测失败", "error");
      setDiagnosticStatus(homeShortcutStatus, "检测失败", "error");
      lastPermissionSnapshot = {
        ...createDefaultPermissionSnapshot(),
        isDesktopRuntime: true,
        microphoneState: microphoneDiagnostic.state,
        microphoneText: microphoneDiagnostic.text,
        systemAudioState: readSystemAudioDiagnostic(readSavedConfig()).state,
        systemAudioText: readSystemAudioDiagnostic(readSavedConfig()).text,
      };
      renderModePermissions();
      updateNextStepPanel("error", "重新检查运行状态", `诊断失败：${formatError(error)}`, "重新检查", "refresh", "refresh");
    }
  } finally {
    isRefreshingDiagnostics = false;
  }
}

/** 启动 Hub 诊断轮询，覆盖用户在系统设置里手动移除权限后的状态变化。 */
function startHubDiagnosticsRefresh(): void {
  if (windowMode !== "hub" || hubDiagnosticsRefreshHandle !== null) {
    return;
  }
  hubDiagnosticsRefreshHandle = window.setInterval(() => {
    if (document.visibilityState === "visible") {
      void refreshDiagnostics();
    }
  }, HUB_DIAGNOSTICS_REFRESH_INTERVAL_MS);
  window.addEventListener("focus", refreshDiagnosticsAfterHubFocus);
  document.addEventListener("visibilitychange", refreshDiagnosticsAfterVisibilityChange);
}

/** 停止 Hub 诊断轮询，窗口销毁时清理定时器和监听。 */
function stopHubDiagnosticsRefresh(): void {
  if (hubDiagnosticsRefreshHandle !== null) {
    window.clearInterval(hubDiagnosticsRefreshHandle);
    hubDiagnosticsRefreshHandle = null;
  }
  window.removeEventListener("focus", refreshDiagnosticsAfterHubFocus);
  document.removeEventListener("visibilitychange", refreshDiagnosticsAfterVisibilityChange);
}

/** Hub 重新获得焦点时刷新一次桌面权限状态。 */
function refreshDiagnosticsAfterHubFocus(): void {
  void refreshDiagnostics();
}

/** Hub 从后台回到可见状态时刷新一次桌面权限状态。 */
function refreshDiagnosticsAfterVisibilityChange(): void {
  if (document.visibilityState === "visible") {
    void refreshDiagnostics();
  }
}

/** 读取 Tauri 桌面端真实运行诊断，供设置页和授权轮询复用。 */
async function readRuntimeDiagnostics(): Promise<RuntimeDiagnostics> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<RuntimeDiagnostics>("get_runtime_diagnostics");
}

/** 创建权限快照默认值，避免诊断完成前模式卡片出现空白。 */
function createDefaultPermissionSnapshot(): RuntimePermissionSnapshot {
  return {
    isDesktopRuntime: isTauriRuntime(),
    hasApiKey: false,
    apiKeyText: "未检测",
    microphoneState: "warning",
    microphoneText: "未检测",
    accessibilityReady: false,
    shortcutReady: false,
    shortcutText: "未检测",
    systemAudioState: "warning",
    systemAudioText: "未检测",
  };
}

/** 读取实时字幕系统声音配置状态，用于模式卡片展示而不是藏在全局设置里。 */
function readSystemAudioDiagnostic(config: VoiceConfig): { text: string; state: DiagnosticState } {
  if (config.systemAudioDeviceId === "none") {
    return config.subtitleIncludeMicrophone
      ? { text: "只采集麦克风", state: "success" }
      : { text: "未选择音频来源", state: "error" };
  }
  if (config.systemAudioDeviceId === NATIVE_SYSTEM_AUDIO_DEVICE_ID) {
    const targetCount = normalizeSubtitleTargetApps(config.subtitleTargetApps, ["active"]).length;
    return { text: targetCount > 1 ? `原生捕获 ${targetCount} 个 App` : "原生系统音频捕获", state: "success" };
  }
  if (!config.systemAudioDeviceId || config.systemAudioDeviceId === "auto") {
    return { text: "需要选择系统声音输入", state: "warning" };
  }
  return { text: "已选择系统声音输入", state: "success" };
}

/** 读取指定模式自己的快捷键权限文案，避免模式卡片复用口述快捷键状态。 */
function readModeShortcutPermissionText(mode: ShortcutMode): string {
  if (!lastPermissionSnapshot.shortcutReady) {
    return lastPermissionSnapshot.shortcutText;
  }
  const shortcut = readSavedConfig().shortcuts[mode] || DEFAULT_SHORTCUTS[mode];
  return `已注册 ${formatShortcutLabel(shortcut)}`;
}

/** 按语音模式生成所需权限列表，避免所有模式共用一张全局权限表。 */
function readModePermissions(mode: ShortcutMode): ModePermissionItem[] {
  const config = readSavedConfig();
  const microphoneReady =
    mode === "subtitle" && !config.subtitleIncludeMicrophone ? true : lastPermissionSnapshot.microphoneState === "success";
  const permissions: ModePermissionItem[] = [
    {
      kind: "apiKey",
      label: "Mimo Key",
      ready: lastPermissionSnapshot.hasApiKey,
      state: lastPermissionSnapshot.hasApiKey ? "success" : "error",
      description: lastPermissionSnapshot.apiKeyText,
    },
    {
      kind: "shortcut",
      label: "快捷键权限",
      ready: lastPermissionSnapshot.shortcutReady,
      state: lastPermissionSnapshot.shortcutReady ? "success" : "error",
      description: readModeShortcutPermissionText(mode),
    },
  ];
  if (mode !== "polish") {
    permissions.splice(1, 0, {
      kind: "microphone",
      label: mode === "subtitle" ? "麦克风输入" : "麦克风权限",
      ready: microphoneReady,
      state: mode === "subtitle" && !config.subtitleIncludeMicrophone ? "success" : lastPermissionSnapshot.microphoneState,
      description: mode === "subtitle" && !config.subtitleIncludeMicrophone ? "已关闭" : lastPermissionSnapshot.microphoneText,
    });
  }
  if (mode === "dictate" || mode === "translate" || mode === "polish") {
    permissions.push({
      kind: "accessibility",
      label: mode === "polish" ? "读取与替换权限" : "自动粘贴权限",
      ready: lastPermissionSnapshot.accessibilityReady,
      state: lastPermissionSnapshot.accessibilityReady ? "success" : "warning",
      description: lastPermissionSnapshot.accessibilityReady ? "已授权" : "未授权，无法自动操作选中文本",
    });
  }
  if (mode === "subtitle") {
    permissions.push({
      kind: "systemAudio",
      label: "系统声音",
      ready: lastPermissionSnapshot.systemAudioState === "success",
      state: lastPermissionSnapshot.systemAudioState,
      description: lastPermissionSnapshot.systemAudioText,
    });
  }
  return permissions;
}

/** 把各语音模式所需权限渲染到模式卡片和对应详情页。 */
function renderModePermissions(): void {
  if (windowMode !== "hub") {
    return;
  }
  document.querySelectorAll<HTMLElement>("[data-mode-permissions]").forEach((container) => {
    const mode = normalizeShortcutMode(container.dataset.modePermissions);
    const permissions = readModePermissions(mode);
    container.innerHTML = permissions
      .map((permission) => {
        const icon = permission.ready ? "✓" : "×";
        const action = permission.ready ? "查看" : "设置";
        return `
          <button class="modePermissionItem" type="button" data-state="${permission.state}" data-permission-mode="${mode}" data-permission-kind="${permission.kind}">
            <span class="modePermissionIcon" aria-hidden="true">${icon}</span>
            <span class="modePermissionCopy">
              <strong>${escapeHtml(permission.label)}</strong>
              <span>${escapeHtml(permission.description)}</span>
            </span>
            <span class="modePermissionAction">${action}</span>
          </button>`;
      })
      .join("");
  });
}

/** 把 Rust 返回的快捷键注册状态转成首页和设置页诊断文案。 */
function formatShortcutDiagnostic(diagnostics: RuntimeDiagnostics): {
  text: string;
  homeText: string;
  state: DiagnosticState;
} {
  const message = diagnostics.shortcutRegistrationMessage.trim();
  if (diagnostics.shortcutRegistrationReady) {
    const label = formatShortcutLabel(diagnostics.shortcuts.dictate);
    if (message && message !== "快捷键已注册") {
      return { text: message, homeText: "已保留原快捷键", state: "warning" };
    }
    return { text: `已注册 ${label}`, homeText: `已注册 ${label}`, state: "success" };
  }
  const fallbackMessage = message || "快捷键注册失败，请换一个组合键";
  return {
    text: fallbackMessage,
    homeText: "注册失败",
    state: "error",
  };
}

/** 根据首页准备状态更新顶部运行胶囊，让用户一眼知道是否可以完整自动粘贴。 */
function updateReadinessSummary(
  hasApiKey: boolean,
  microphoneState: DiagnosticState,
  accessibilityReady: boolean,
  shortcutReady: boolean,
  isDesktopRuntime: boolean,
): void {
  const microphoneReady = microphoneState === "success";
  const isFullyReady = isDesktopRuntime && hasApiKey && microphoneReady && accessibilityReady && shortcutReady;
  systemStateText.textContent = isFullyReady ? "准备完成" : isDesktopRuntime ? "需要配置" : "网页预览";
  systemStateText.dataset.state = isFullyReady ? "success" : "warning";
  if (!isDesktopRuntime) {
    updateNextStepPanel(
      "warning",
      "在桌面 App 中完成体验",
      "网页预览只能检查界面和麦克风状态；快捷键、钥匙串和自动粘贴需要在 typesass.app 中使用。",
      "打开设置",
      "apiKey",
      "setting",
    );
    return;
  }
  if (!hasApiKey) {
    updateNextStepPanel(
      "warning",
      "先保存 Mimo Key",
      "粘贴后 Key 会进入 macOS 钥匙串，不会写入本地配置文件。",
      "配置 Key",
      "apiKey",
      "setting",
    );
    return;
  }
  if (!shortcutReady) {
    updateNextStepPanel(
      "error",
      "修复快捷键",
      "当前全局快捷键没有成功注册，换一个未被系统占用的组合键后保存。",
      "编辑快捷键",
      "shortcut",
      "keyboard",
    );
    return;
  }
  if (!microphoneReady) {
    updateNextStepPanel(
      "warning",
      "授权麦克风",
      "完成麦克风授权后，悬浮条才能采集真实语音并显示实时波形。",
      "授权麦克风",
      "microphone",
      "microphone",
    );
    return;
  }
  if (!accessibilityReady) {
    updateNextStepPanel(
      "warning",
      "开启辅助功能",
      "开启后转写结果才能自动粘贴到当前输入框；否则会弹出结果窗口供复制。",
      "打开辅助功能",
      "accessibility",
      "check",
    );
    return;
  }
  updateReadyNextStepPanel();
}

/** 所有准备项就绪时，把首页下一步指向语音模式，避免其它页面出现具体模式的开始按钮。 */
function updateReadyNextStepPanel(): void {
  const shortcuts = readSavedConfig().shortcuts;
  const shortcutLabel = formatShortcutLabel(shortcuts.dictate);
  updateNextStepPanel(
    "success",
    "语音模式已就绪",
    `进入语音模式选择口述、翻译、随便问或实时字幕，也可以直接按 ${shortcutLabel}。`,
    "打开语音模式",
    "modes",
    "voice",
  );
}

/** 更新首页智能下一步区域，确保主按钮始终对应真实可执行动作。 */
function updateNextStepPanel(
  state: DiagnosticState,
  title: string,
  description: string,
  buttonLabel: string,
  action: ReadinessAction,
  icon: IconName,
): void {
  nextReadinessAction = action;
  nextStepPanel.dataset.state = state;
  nextStepTitle.textContent = title;
  nextStepDescription.textContent = description;
  nextStepPrimaryLabel.textContent = buttonLabel;
  renderIcon(nextStepPrimaryIcon, icon);
  syncStartActionButtons();
}

/** 执行首页智能下一步主动作。 */
async function handleNextStepAction(): Promise<void> {
  if (nextReadinessAction === "start") {
    await requestFloatingMode("dictate");
    return;
  }
  if (nextReadinessAction === "refresh") {
    await refreshHubRuntimeState();
    return;
  }
  if (nextReadinessAction === "modes") {
    switchHubView("modes");
    return;
  }
  await handleReadinessAction(nextReadinessAction);
}

/** 处理首页准备状态上的可行动按钮，减少用户寻找设置项的路径。 */
async function handleReadinessAction(action: ReadinessAction | string): Promise<void> {
  if (action === "apiKey") {
    switchHubView("settings");
    focusSettingControl(apiKeyInput);
    showHubNotice("在这里粘贴 Mimo Key，离开输入时会进入 macOS 钥匙串。", "busy");
    return;
  }
  if (action === "microphone") {
    switchHubView("settings");
    await authorizeMicrophoneAccess();
    return;
  }
  if (action === "accessibility") {
    await openAccessibilitySettings();
    return;
  }
  if (action === "shortcut") {
    switchModeSettingsView("dictate");
    focusSettingControl(dictateShortcutInput);
  }
}

/** 打开某个模式自己的权限说明弹窗。 */
function openPermissionDialog(mode: ShortcutMode, kind: PermissionKind): void {
  const copy = readPermissionDialogCopy(mode, kind);
  activePermissionDialog = { mode, kind };
  permissionDialogEyebrow.textContent = `${SHORTCUT_MODE_LABELS[mode]}权限`;
  permissionDialogTitle.textContent = copy.title;
  permissionDialogBody.textContent = copy.body;
  permissionDialogSteps.innerHTML = copy.steps.map((step) => `<li>${escapeHtml(step)}</li>`).join("");
  permissionDialogPrimaryButton.hidden = !copy.primaryLabel;
  permissionDialogPrimaryButton.querySelector("span:last-child")!.textContent = copy.primaryLabel || "打开设置";
  permissionDialog.dataset.open = "true";
  permissionDialogPrimaryButton.focus();
}

/** 关闭权限说明弹窗，并清理当前权限上下文。 */
function closePermissionDialog(): void {
  permissionDialog.dataset.open = "false";
  activePermissionDialog = null;
}

/** 生成不同权限的说明文案和可执行动作名称。 */
function readPermissionDialogCopy(
  mode: ShortcutMode,
  kind: PermissionKind,
): { title: string; body: string; steps: string[]; primaryLabel: string } {
  if (kind === "apiKey") {
    return {
      title: "设置 Mimo Key",
      body: `${SHORTCUT_MODE_LABELS[mode]}需要 Mimo Key 才能调用 ASR 或 AI 服务。Key 只保存在本机钥匙串或当前会话里。`,
      steps: ["打开系统设置页的模型与识别区域。", "粘贴 Mimo Key。", "离开输入框后会自动写入钥匙串，再重新检查权限状态。"],
      primaryLabel: "去填写 Key",
    };
  }
  if (kind === "microphone") {
    return {
      title: "设置麦克风权限",
      body: `${SHORTCUT_MODE_LABELS[mode]}需要麦克风输入来采集声音。没有授权时，快捷键会提示“请设置麦克风权限”。`,
      steps: ["在 macOS 系统设置中打开隐私与安全性。", "进入麦克风。", "允许 typesass 使用麦克风，然后回到应用重新检查。"],
      primaryLabel: "打开麦克风权限",
    };
  }
  if (kind === "accessibility") {
    if (mode === "polish") {
      return {
        title: "设置读取与替换权限",
        body: "润色模式需要 macOS 辅助功能权限，才能读取外部应用中的选中文本，并在 AI 润色完成后替换原选区。",
        steps: ["在 macOS 系统设置中打开隐私与安全性。", "进入辅助功能。", "勾选 typesass，回到应用后重新触发润色。"],
        primaryLabel: "打开辅助功能",
      };
    }
    return {
      title: "设置自动粘贴权限",
      body: `${SHORTCUT_MODE_LABELS[mode]}完成转写后如果要自动粘贴，需要 macOS 辅助功能权限。未授权时仍会展示结果窗口，方便手动复制。`,
      steps: ["在 macOS 系统设置中打开隐私与安全性。", "进入辅助功能。", "勾选 typesass，回到应用后会自动刷新状态。"],
      primaryLabel: "打开辅助功能",
    };
  }
  if (kind === "shortcut") {
    return {
      title: "设置快捷键权限",
      body: `${SHORTCUT_MODE_LABELS[mode]}需要全局快捷键注册成功，才能在其他应用里直接启动。`,
      steps: ["打开当前模式的设置页。", "为当前模式录制一个未被系统占用的组合键。", "录制完成后会自动重新注册快捷键。"],
      primaryLabel: "编辑快捷键",
    };
  }
  return {
    title: "设置系统声音",
    body: "实时字幕要识别电脑正在播放的声音，需要允许原生系统音频捕获，或选择可代表系统输出的输入设备作为兜底。",
    steps: [
      "优先使用“原生系统音频捕获”，macOS 弹出捕获其他应用音频权限时请允许 typesass。",
      "如果原生捕获不可用，再配置 BlackHole、Loopback、Soundflower 或聚合设备作为系统声音输入。",
      "播放带人声的音频后，再开启实时字幕验证是否出字。",
    ],
    primaryLabel: "打开音频来源",
  };
}

/** 执行权限弹窗主按钮动作，尽量直达对应系统页或设置控件。 */
async function handlePermissionDialogPrimaryAction(): Promise<void> {
  if (!activePermissionDialog) {
    return;
  }
  const { mode, kind } = activePermissionDialog;
  closePermissionDialog();
  if (kind === "apiKey") {
    switchHubView("settings");
    focusSettingControl(apiKeyInput);
    showHubNotice("在这里填写 Mimo Key，自动生效后回到模式卡片检查状态。", "busy");
    return;
  }
  if (kind === "microphone") {
    await openMicrophoneSettings();
    return;
  }
  if (kind === "accessibility") {
    await openAccessibilitySettings();
    return;
  }
  if (kind === "shortcut") {
    switchModeSettingsView(mode);
    focusSettingControl(getShortcutInput(mode));
    showHubNotice(`正在编辑${SHORTCUT_MODE_LABELS[mode]}快捷键。`, "busy");
    return;
  }
  switchHubView("settings");
  focusSettingControl(systemAudioSelect);
  showHubNotice("实时字幕的系统声音来源在这里选择。", "busy");
}

/** 切页后把用户带到最相关的输入控件，并给一个轻量焦点动效。 */
function focusSettingControl(control: HTMLElement): void {
  window.setTimeout(() => {
    control.focus();
    control.scrollIntoView({ block: "center", behavior: "smooth" });
    control.dataset.focusPulse = "true";
    window.setTimeout(() => {
      delete control.dataset.focusPulse;
    }, 900);
  }, 80);
}

/** 检测浏览器可见的麦克风设备数量，不主动弹权限。 */
async function readMicrophoneDiagnostic(): Promise<{ text: string; state: DiagnosticState }> {
  if (!navigator.mediaDevices?.enumerateDevices) {
    return { text: "当前环境不可用", state: "error" };
  }
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    const inputCount = devices.filter((device) => device.kind === "audioinput").length;
    if (inputCount > 0) {
      return { text: `${inputCount} 个输入设备`, state: "success" };
    }
    return { text: "未检测到输入设备", state: "warning" };
  } catch {
    return { text: "无法枚举设备", state: "warning" };
  }
}

/** 主动请求麦克风权限并立即释放音频流，便于用户在设置页一次性完成授权。 */
async function authorizeMicrophoneAccess(): Promise<void> {
  if (!navigator.mediaDevices?.getUserMedia) {
    showHubNotice("当前环境不支持麦克风授权。", "error");
    return;
  }
  showHubNotice("正在请求麦克风权限。", "busy");
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: buildAudioConstraints(readConfigFromForm()) });
    stream.getTracks().forEach((track) => track.stop());
    await populateMicrophones();
    await refreshDiagnostics();
    showHubNotice("麦克风已授权，可以开始录音。", "success");
  } catch (error) {
    setDiagnosticStatus(microphoneStatus, "授权失败", "error");
    showHubNotice(`麦克风授权失败：${formatError(error)}`, "error");
  }
}

/** 打开系统麦克风权限页，并提示用户回到对应模式卡片复查状态。 */
async function openMicrophoneSettings(): Promise<void> {
  if (!isTauriRuntime()) {
    showHubNotice("网页预览模式不能打开系统麦克风权限页。", "error");
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_microphone_settings");
    showHubNotice("已打开麦克风权限页，允许 typesass 后回到模式卡片重新检查。", "success");
  } catch (error) {
    showHubNotice(`打开麦克风权限页失败：${formatError(error)}`, "error");
  }
}

/** 更新诊断卡片的状态文字和颜色。 */
function setDiagnosticStatus(element: HTMLElement, text: string, state: DiagnosticState): void {
  element.textContent = text;
  element.title = text;
  element.dataset.state = state;
}

/** 打开 macOS 辅助功能设置，方便用户授予自动粘贴权限。 */
async function openAccessibilitySettings(): Promise<void> {
  if (!isTauriRuntime()) {
    showHubNotice("网页预览模式不能打开系统设置。", "error");
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_accessibility_settings");
    showHubNotice("已打开辅助功能设置，授权后会自动刷新诊断。", "success");
    startAccessibilityGrantWatch("hub");
  } catch (error) {
    showHubNotice(`打开辅助功能设置失败：${formatError(error)}`, "error");
  }
}

/** 打开系统权限页后短时间轮询辅助功能状态，授权完成时自动更新当前界面。 */
function startAccessibilityGrantWatch(surface: "hub" | "result"): void {
  if (!isTauriRuntime()) {
    return;
  }
  stopAccessibilityGrantWatch();
  const startedAt = Date.now();
  let isChecking = false;
  showAccessibilityGrantWatchState(surface);
  accessibilityWatchHandle = window.setInterval(() => {
    if (isChecking) {
      return;
    }
    isChecking = true;
    void checkAccessibilityGrant(surface, startedAt).finally(() => {
      isChecking = false;
    });
  }, 1200);
}

/** 打开系统权限页后立即同步等待态，避免用户不知道当前是否正在检测授权。 */
function showAccessibilityGrantWatchState(surface: "hub" | "result"): void {
  if (surface === "hub") {
    setDiagnosticStatus(accessibilityStatus, "等待授权", "warning");
    setDiagnosticStatus(homeAccessibilityStatus, "等待授权", "warning");
    updateNextStepPanel(
      "warning",
      "等待辅助功能授权",
      "在系统设置里勾选 typesass；完成后这里会自动刷新，不需要重启应用。",
      "重新检查",
      "refresh",
      "refresh",
    );
    showHubNotice("正在检测辅助功能授权状态。", "busy");
    return;
  }
  resultReason.textContent = "正在检测辅助功能授权状态。";
  resultReason.dataset.state = "warning";
  resultOpenAccessibilityButton.disabled = true;
  resultOpenAccessibilityButton.textContent = "检测中";
}

/** 停止辅助功能授权轮询，避免权限窗口关闭后继续占用计时器。 */
function stopAccessibilityGrantWatch(): void {
  if (accessibilityWatchHandle === null) {
    return;
  }
  window.clearInterval(accessibilityWatchHandle);
  accessibilityWatchHandle = null;
}

/** 检查辅助功能是否已授权，并把结果反馈到 Hub 或结果兜底窗口。 */
async function checkAccessibilityGrant(surface: "hub" | "result", startedAt: number): Promise<void> {
  try {
    const diagnostics = await readRuntimeDiagnostics();
    if (diagnostics.accessibilityTrusted) {
      stopAccessibilityGrantWatch();
      if (surface === "hub") {
        await refreshDiagnostics();
        showHubNotice("辅助功能已授权，下一次转写会自动粘贴。", "success");
        return;
      }
      resultReason.textContent = "辅助功能已授权，下一次转写会自动粘贴。";
      resultReason.dataset.state = "success";
      resultOpenAccessibilityButton.disabled = true;
      resultOpenAccessibilityButton.textContent = "已授权";
      return;
    }
    if (Date.now() - startedAt > 45000) {
      stopAccessibilityGrantWatch();
      if (surface === "hub") {
        showHubNotice("还没有检测到辅助功能授权，授权完成后可刷新诊断。", "idle");
        void refreshDiagnostics();
        return;
      }
      resultReason.textContent = "还没有检测到辅助功能授权；授权完成后下次会自动粘贴。";
      resultReason.dataset.state = "warning";
      resultOpenAccessibilityButton.disabled = false;
      resultOpenAccessibilityButton.textContent = "重新打开辅助功能设置";
    }
  } catch {
    stopAccessibilityGrantWatch();
    if (surface === "hub") {
      showHubNotice("辅助功能授权检测中断，可手动刷新诊断。", "error");
      return;
    }
    resultReason.textContent = "辅助功能授权检测中断，可稍后重试。";
    resultReason.dataset.state = "error";
    resultOpenAccessibilityButton.disabled = false;
    resultOpenAccessibilityButton.textContent = "重新打开辅助功能设置";
  }
}

/** 根据当前模式更新悬浮条提示，避免快捷键修改后仍显示旧值。 */
function updateFloatingShortcutTitle(shortcuts: ShortcutConfig): void {
  const shortcut = shortcuts[activeMode] || shortcuts.dictate;
  soundStage.title = `${formatShortcutLabel(shortcut)} 开始或停止${MODE_LABELS[activeMode]}`;
}

/** 把注册文本转成人更容易读的快捷键标签。 */
function formatShortcutLabel(shortcut: string): string {
  return shortcut
    .split("+")
    .filter(Boolean)
    .map((part) => {
      if (part === "ctrl") {
        return "Control";
      }
      if (part === "alt" || part === "option") {
        return "Option";
      }
      if (part === "cmd" || part === "command" || part === "meta") {
        return "Command";
      }
      if (part === "shift") {
        return "Shift";
      }
      if (part === "space") {
        return "Space";
      }
      return part.length === 1 ? part.toUpperCase() : part;
    })
    .join(" + ");
}

/** 进入某个模式的快捷键录制状态，桌面端会先临时暂停全局快捷键，避免按键被系统注册器拦截。 */
async function startShortcutRecording(mode: ShortcutMode): Promise<void> {
  restoreShortcutRecordingSnapshot();
  const isSuspended = await suspendShortcutsForRecording();
  if (!isSuspended) {
    return;
  }
  shortcutRecordingMode = mode;
  clearShortcutRecordingState();
  const input = getShortcutInput(mode);
  shortcutRecordingSnapshot = { mode, label: input.value };
  input.value = "请按新的组合键";
  input.dataset.recording = "true";
  const label = getShortcutLabel(mode);
  label.textContent = "请按新的组合键";
  label.dataset.recording = "true";
  setShortcutValidation("按下包含 Control、Command、Option 或 Shift 的组合键；Esc 可取消。", "busy", true);
  showHubNotice(`正在录制${SHORTCUT_MODE_LABELS[mode]}快捷键。`, "busy");
}

/** 暂停桌面端当前已注册快捷键，让 WebView 可以收到接下来这次组合键输入。 */
async function suspendShortcutsForRecording(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("suspend_shortcuts_for_recording");
    return true;
  } catch (error) {
    showHubNotice(`进入快捷键录制失败：${formatError(error)}`, "error");
    await registerShortcutsFromConfig(readSavedConfig());
    return false;
  }
}

/** 将某个模式的快捷键恢复为默认值。 */
function resetShortcutInput(mode: ShortcutMode): void {
  shortcutRecordingMode = null;
  shortcutRecordingSnapshot = null;
  clearShortcutRecordingState();
  getShortcutInput(mode).value = formatShortcutLabel(DEFAULT_SHORTCUTS[mode]);
  renderShortcutLabels(readConfigFromForm().shortcuts);
  const isValid = validateShortcutInputs();
  showHubNotice(
    isValid ? `${SHORTCUT_MODE_LABELS[mode]}快捷键已恢复默认，正在生效。` : "恢复默认后出现快捷键冲突，请调整后再生效。",
    isValid ? "success" : "error",
  );
  if (isValid) {
    scheduleConfigAutoSave(`${SHORTCUT_MODE_LABELS[mode]}快捷键已生效。`, 0);
  }
}

/** 捕获用户按下的快捷键组合并写入当前录制输入框。 */
function captureShortcutKeys(event: KeyboardEvent): void {
  if (!shortcutRecordingMode) {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    cancelShortcutRecording();
    return;
  }
  const shortcut = keyboardEventToShortcut(event);
  if (!shortcut) {
    setShortcutValidation("需要至少一个修饰键加一个按键，例如 Control + D。", "error", true);
    return;
  }
  const mode = shortcutRecordingMode;
  getShortcutInput(mode).value = formatShortcutLabel(shortcut);
  clearShortcutRecordingState();
  shortcutRecordingMode = null;
  shortcutRecordingSnapshot = null;
  renderShortcutLabels(readConfigFromForm().shortcuts);
  const isValid = validateShortcutInputs();
  showHubNotice(
    isValid
      ? `${SHORTCUT_MODE_LABELS[mode]}快捷键已设为 ${formatShortcutLabel(shortcut)}，正在生效。`
      : "这个快捷键和其它模式冲突，请重新录制。",
    isValid ? "success" : "error",
  );
  if (isValid) {
    scheduleConfigAutoSave(`${SHORTCUT_MODE_LABELS[mode]}快捷键已生效。`, 0);
  }
}

/** 清除快捷键输入框的录制态。 */
function clearShortcutRecordingState(): void {
  dictateShortcutInput.removeAttribute("data-recording");
  translateShortcutInput.removeAttribute("data-recording");
  askShortcutInput.removeAttribute("data-recording");
  polishShortcutInput.removeAttribute("data-recording");
  subtitleShortcutInput.removeAttribute("data-recording");
  dictateShortcutText.removeAttribute("data-recording");
  translateShortcutText.removeAttribute("data-recording");
  askShortcutText.removeAttribute("data-recording");
  polishShortcutText.removeAttribute("data-recording");
  subtitleShortcutText.removeAttribute("data-recording");
}

/** 取消当前快捷键录制并恢复进入录制态前的展示值。 */
function cancelShortcutRecording(): void {
  restoreShortcutRecordingSnapshot();
  shortcutRecordingMode = null;
  clearShortcutRecordingState();
  renderShortcutLabels(readConfigFromForm().shortcuts);
  validateShortcutInputs();
  showHubNotice("已取消快捷键录制。", "idle");
  void registerShortcutsFromConfig(readSavedConfig());
}

/** 如果存在快捷键录制草稿，则恢复对应输入框的原值。 */
function restoreShortcutRecordingSnapshot(): void {
  if (!shortcutRecordingSnapshot) {
    return;
  }
  getShortcutInput(shortcutRecordingSnapshot.mode).value = shortcutRecordingSnapshot.label;
  shortcutRecordingSnapshot = null;
  renderShortcutLabels(readConfigFromForm().shortcuts);
}

/** 实时校验快捷键配置，并同步保存按钮和提示文案。 */
function validateShortcutInputs(): boolean {
  if (windowMode !== "hub") {
    return true;
  }
  if (shortcutRecordingMode) {
    setShortcutValidation("正在录制快捷键，按 Esc 可取消。", "busy", true);
    return false;
  }

  const shortcuts = readConfigFromForm().shortcuts;
  const entries: Array<[ShortcutMode, string]> = [
    ["dictate", shortcuts.dictate],
    ["translate", shortcuts.translate],
    ["ask", shortcuts.ask],
    ["polish", shortcuts.polish],
    ["subtitle", shortcuts.subtitle],
  ];
  const invalidEntry = entries.find(([, shortcut]) => !isValidShortcutText(shortcut));
  if (invalidEntry) {
    setShortcutValidation(`${SHORTCUT_MODE_LABELS[invalidEntry[0]]}快捷键不完整，请重新录制。`, "error", true);
    return false;
  }
  const repeated = entries.find(([, shortcut], index) =>
    entries.some(([, compareShortcut], compareIndex) => compareIndex !== index && compareShortcut === shortcut),
  );
  if (repeated) {
    setShortcutValidation(`${formatShortcutLabel(repeated[1])} 已被多个模式使用，请换一个组合键。`, "error", true);
    return false;
  }
  setShortcutValidation("快捷键没有冲突，修改后会自动重新注册。", "success", false);
  return true;
}

/** 更新所有模式详情里的快捷键校验提示。 */
function setShortcutValidation(message: string, state: HubNoticeState, shouldDisableSave: boolean): void {
  shortcutValidationTexts.forEach((element) => {
    element.textContent = message;
    element.dataset.state = state;
    element.dataset.disabled = shouldDisableSave ? "true" : "false";
  });
}

/** 判断快捷键文本是否包含至少一个修饰键和一个实际按键。 */
function isValidShortcutText(shortcut: string): boolean {
  const parts = shortcut.split("+").filter(Boolean);
  if (parts.length < 2) {
    return false;
  }
  const modifiers = new Set(["ctrl", "cmd", "alt", "shift"]);
  const hasModifier = parts.some((part) => modifiers.has(part));
  const hasKey = parts.some((part) => !modifiers.has(part));
  return hasModifier && hasKey;
}

/** 把浏览器键盘事件转换为 Tauri 可注册的快捷键文本。 */
function keyboardEventToShortcut(event: KeyboardEvent): string {
  const parts: string[] = [];
  if (event.ctrlKey) {
    parts.push("ctrl");
  }
  if (event.metaKey) {
    parts.push("cmd");
  }
  if (event.altKey) {
    parts.push("alt");
  }
  if (event.shiftKey) {
    parts.push("shift");
  }
  const key = normalizeEventKey(event.key);
  if (!key || ["ctrl", "control", "cmd", "meta", "alt", "option", "shift"].includes(key)) {
    return "";
  }
  parts.push(key);
  return parts.length >= 2 ? parts.join("+") : "";
}

/** 规范化浏览器键名，保持和 Rust 侧快捷键解析兼容。 */
function normalizeEventKey(key: string): string {
  const normalized = key.trim().toLowerCase();
  if (normalized === " ") {
    return "space";
  }
  if (normalized === "control") {
    return "ctrl";
  }
  if (normalized === "meta") {
    return "cmd";
  }
  if (normalized === "option") {
    return "alt";
  }
  if (normalized.startsWith("arrow")) {
    return normalized.replace("arrow", "");
  }
  return normalized.length === 1 ? normalized : normalized;
}

/** 读取某个模式对应的快捷键输入框。 */
function getShortcutInput(mode: ShortcutMode): HTMLInputElement {
  if (mode === "subtitle") {
    return subtitleShortcutInput;
  }
  if (mode === "translate") {
    return translateShortcutInput;
  }
  if (mode === "ask") {
    return askShortcutInput;
  }
  if (mode === "polish") {
    return polishShortcutInput;
  }
  return dictateShortcutInput;
}

/** 读取语音模式卡片上的快捷键文本元素，用于录制时给出就地反馈。 */
function getShortcutLabel(mode: ShortcutMode): HTMLElement {
  if (mode === "subtitle") {
    return subtitleShortcutText;
  }
  if (mode === "translate") {
    return translateShortcutText;
  }
  if (mode === "ask") {
    return askShortcutText;
  }
  if (mode === "polish") {
    return polishShortcutText;
  }
  return dictateShortcutText;
}

/** 从浏览器枚举麦克风设备并填充选择器。 */
async function populateMicrophones(): Promise<void> {
  const config = readSavedConfig();
  const currentValue = microphoneSelect.value || config.microphoneDeviceId;
  const currentSystemAudioValue = systemAudioSelect.value || config.systemAudioDeviceId;
  microphoneSelect.innerHTML = '<option value="default">系统默认麦克风</option>';
  systemAudioSelect.innerHTML = [
    '<option value="native-process-tap">推荐：原生系统音频捕获</option>',
    '<option value="auto">自动检测系统音频输入</option>',
    '<option value="none">不采集系统声音</option>',
  ].join("");
  if (!navigator.mediaDevices?.enumerateDevices) {
    microphoneSelect.value = "default";
    systemAudioSelect.value = "auto";
    return;
  }
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    devices
      .filter((device) => device.kind === "audioinput")
      .forEach((device, index) => {
        const option = document.createElement("option");
        option.value = device.deviceId || "default";
        option.textContent = device.label || `麦克风 ${index + 1}`;
        microphoneSelect.appendChild(option);
        if (device.deviceId) {
          const systemOption = document.createElement("option");
          systemOption.value = device.deviceId;
          const systemAudioLabel = device.label || `音频输入 ${index + 1}`;
          systemOption.textContent = isSystemAudioInputLabel(systemAudioLabel)
            ? `推荐：${systemAudioLabel}`
            : systemAudioLabel;
          systemAudioSelect.appendChild(systemOption);
        }
      });
    microphoneSelect.value = Array.from(microphoneSelect.options).some((option) => option.value === currentValue)
      ? currentValue
      : "default";
    systemAudioSelect.value = Array.from(systemAudioSelect.options).some((option) => option.value === currentSystemAudioValue)
      ? currentSystemAudioValue
      : "auto";
  } catch {
    microphoneSelect.value = "default";
    systemAudioSelect.value = "auto";
  }
}

/** 刷新实时字幕可采集的系统音频 App 列表。 */
async function populateSubtitleTargetApps(showNotice = false): Promise<void> {
  const config = readSavedConfig();
  const selectedTargets = readSelectedSubtitleTargetApps();
  const desiredTargets = selectedTargets.length ? selectedTargets : config.subtitleTargetApps;
  subtitleTargetAppsSelect.innerHTML = '<option value="active">自动选择正在发声的 App</option>';
  if (!isTauriRuntime()) {
    setSubtitleTargetAppsValue(desiredTargets);
    return;
  }
  try {
    const apps = await listProcessTapAudioApps();
    apps
      .filter((app) => app.pid > 0 && app.name.trim())
      .sort((left, right) => Number(right.audioActive) - Number(left.audioActive) || left.name.localeCompare(right.name))
      .forEach((app) => {
        const option = document.createElement("option");
        option.value = String(app.pid);
        option.textContent = `${app.audioActive ? "正在发声 · " : ""}${app.name}${app.bundleId ? ` · ${app.bundleId}` : ""}`;
        subtitleTargetAppsSelect.appendChild(option);
      });
    setSubtitleTargetAppsValue(desiredTargets);
    if (showNotice) {
      showHubNotice(`已刷新 ${apps.length} 个音频 App。`, "success");
    }
  } catch (error) {
    setSubtitleTargetAppsValue(desiredTargets);
    if (showNotice) {
      showHubNotice(`刷新可采集 App 失败：${formatError(error)}`, "error");
    }
  }
}

/** 调用 Tauri 读取 Core Audio 当前可见的音频进程。 */
async function listProcessTapAudioApps(): Promise<ProcessTapAudioApp[]> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProcessTapAudioApp[]>("list_process_tap_audio_apps");
}

/** 清空本地保存的非敏感配置并恢复默认值，执行前要求二次点击确认。 */
function clearSavedConfig(button: HTMLButtonElement): void {
  if (
    !confirmDangerousAction(
      "clearConfig",
      button,
      "再次点击恢复",
      "再次点击将恢复默认设置，Mimo Key 会保留在钥匙串。",
    )
  ) {
    return;
  }
  localStorage.removeItem(CONFIG_STORAGE_KEY);
  resetPendingConfirmation();
  loadConfigToForm();
  void syncDesktopPreferences(readSavedConfig()).then((desktopReady) => {
    void refreshDiagnostics();
    showHubNotice(
      desktopReady ? "已恢复默认设置，Mimo Key 保持不变。" : "已恢复默认设置，部分系统设置需要检查权限。",
      desktopReady ? "success" : "error",
    );
  });
  renderHub();
}

/** 请求悬浮窗按指定模式开始或停止录音。 */
async function requestFloatingMode(mode: VoiceMode): Promise<void> {
  if (windowMode === "hub" && !isModePermissionReady(mode)) {
    await showFirstMissingModePermission(mode);
    return;
  }
  setFloatingMode(mode);
  if (!isTauriRuntime()) {
    switchHubView("home");
    showHubNotice("网页预览模式不能触发系统悬浮录音。", "error");
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const { emitTo } = await import("@tauri-apps/api/event");
    await invoke<string>("show_main_window");
    addDiagnosticLog({
      level: "info",
      category: "system",
      title: "Hub 发起录音",
      message: "用户从 Hub 发起语音流程，录音期间保持 Hub 主界面显示。",
      mode,
      details: ["保持Hub：是"],
    });
    await emitTo("main", "hub-start-mode", { mode, targetApp: "", keepHubVisible: true });
  } catch (error) {
    addDiagnosticLog({
      level: "error",
      category: "system",
      title: "Hub 发起录音失败",
      message: formatError(error),
      mode,
    });
    showHubNotice(`无法唤起悬浮录音：${formatError(error)}`, "error");
  }
}

/** 从 Hub 请求进入或退出实时字幕监听模式。 */
async function requestSubtitleMode(): Promise<void> {
  if (!isTauriRuntime()) {
    showHubNotice("网页预览模式不能触发桌面字幕监听。", "error");
    return;
  }
  if (windowMode === "hub" && !isModePermissionReady("subtitle")) {
    await showFirstMissingModePermission("subtitle");
    return;
  }
  try {
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "Hub 发起实时字幕",
      message: "用户从 Hub 切换字幕监听模式。",
      mode: "subtitle",
    });
    await toggleSubtitleListening();
    showHubNotice("实时字幕监听已切换。", "success");
  } catch (error) {
    addDiagnosticLog({
      level: "error",
      category: "subtitle",
      title: "Hub 发起实时字幕失败",
      message: formatError(error),
      mode: "subtitle",
    });
    showHubNotice(`无法切换实时字幕：${formatError(error)}`, "error");
  }
}

/** 记录悬浮条本次要执行的模式，仅用于录音开始/停止时的内部状态。 */
function setFloatingMode(mode: VoiceMode): void {
  activeMode = mode;
  floatShell.dataset.activeMode = mode;
  updateFloatingShortcutTitle(readConfigFromForm().shortcuts);
}

/** 开始或停止录音。 */
async function toggleRecording(mode: VoiceMode, targetApp = "", keepHubVisible = false): Promise<void> {
  if (isStartingRecording) {
    flashFloatingNudge();
    setStatus("正在准备麦克风，请稍等。", "busy");
    return;
  }
  if (isProcessing) {
    flashFloatingNudge();
    setStatus("正在处理上一段语音，请稍等。", "busy");
    return;
  }
  if (isRecording) {
    await stopRecordingAndTranscribe();
    return;
  }
  activeMode = mode;
  floatShell.dataset.activeMode = mode;
  updateFloatingShortcutTitle(readSavedConfig().shortcuts);
  await startRecording(mode, targetApp, keepHubVisible);
}

/** 请求麦克风权限并开始录音。 */
async function startRecording(mode: VoiceMode, targetApp = "", keepHubVisible = false): Promise<void> {
  if (mode === "polish") {
    await polishSelectedText(targetApp, keepHubVisible);
    return;
  }
  if (!navigator.mediaDevices?.getUserMedia) {
    addDiagnosticLog({
      level: "error",
      category: "recording",
      title: "录音能力不可用",
      message: "当前环境不支持浏览器录音能力。",
      mode,
      targetApp,
    });
    setStatus("当前环境不支持浏览器录音能力。", "error");
    return;
  }

  isStartingRecording = true;
  try {
    const config = readSavedConfig();
    if (!(await ensureReadyForRecording(mode))) {
      return;
    }
    recordingKeepsHubVisible = keepHubVisible;
    recordingTargetApp = keepHubVisible ? "" : normalizeRecordingTargetApp(targetApp);
    addDiagnosticLog({
      level: "info",
      category: "recording",
      title: "开始录音",
      message: "已通过权限和 Key 检查，开始采集麦克风音频。",
      mode,
      targetApp: recordingTargetApp || targetApp,
      details: [
        `麦克风：${config.microphoneDeviceId || "default"}`,
        `AI润色：${config.postProcessDictation ? "开启" : "关闭"}`,
        `保持Hub：${recordingKeepsHubVisible ? "是" : "否"}`,
      ],
    });
    recordedSamples = [];
    recordedSampleLength = 0;
    if (config.muteWhileDictating) {
      previousSystemMuteState = await setSystemOutputMuted(true);
    }
    recordingStream = await navigator.mediaDevices.getUserMedia({ audio: buildAudioConstraints(config) });
    audioContext = new AudioContext();
    recordedSampleRate = audioContext.sampleRate;
    audioSource = audioContext.createMediaStreamSource(recordingStream);
    audioProcessor = audioContext.createScriptProcessor(4096, 1, 1);
    audioSink = audioContext.createGain();
    audioSink.gain.value = 0;
    audioProcessor.onaudioprocess = collectAudioSamples;
    audioSource.connect(audioProcessor);
    audioProcessor.connect(audioSink);
    audioSink.connect(audioContext.destination);
    isRecording = true;
    recordStartedAt = Date.now();
    startTimer();
    recordButton.title = `停止并${MODE_LABELS[mode]}`;
    recordButton.setAttribute("aria-label", `停止并${MODE_LABELS[mode]}`);
    copyButton.disabled = true;
    resultMeta.textContent = `${MODE_LABELS[mode]}中`;
    updateVoiceLevelVisual(0.24);
    playInteractionSound("start", config);
    setStatus(`${MODE_LABELS[mode]}中，说完后再次按快捷键。`, "recording");
  } catch (error) {
    recordingKeepsHubVisible = false;
    stopStream();
    void restoreSystemMute();
    const message = formatRecordingError(error);
    addDiagnosticLog({
      level: "error",
      category: "recording",
      title: "开始录音失败",
      message,
      mode,
      targetApp: recordingTargetApp || targetApp,
    });
    setStatus(message, "error");
    if (isMicrophonePermissionError(error)) {
      await showHubWindow();
      await switchHubWindowToModeDetail(mode, "microphone");
    }
  } finally {
    isStartingRecording = false;
  }
}

/** 对外部 App 当前选中的文字执行 AI 润色，并用系统粘贴快捷键替换原选区。 */
async function polishSelectedText(targetApp = "", keepHubVisible = false): Promise<void> {
  if (isPolishingSelection || isProcessing) {
    flashFloatingNudge();
    setStatus("正在润色上一段文字，请稍等。", "busy");
    return;
  }
  if (isRecording || isStartingRecording || isSubtitleListening || isStartingSubtitleListening) {
    flashFloatingNudge();
    setStatus("请先结束当前语音或字幕任务。", "busy");
    return;
  }
  activeMode = "polish";
  floatShell.dataset.activeMode = "polish";
  updateFloatingShortcutTitle(readSavedConfig().shortcuts);
  if (!isTauriRuntime()) {
    setStatus("网页预览模式不能读取和替换外部选中文本。", "error");
    return;
  }
  isPolishingSelection = true;
  isProcessing = true;
  recordButton.disabled = true;
  cancelButton.disabled = true;
  copyButton.disabled = true;
  resultMeta.textContent = "润色中";
  resultTextarea.value = "";
  transcribeDurationText.textContent = "--";
  processDurationText.textContent = "--";
  audioSizeText.textContent = "--";
  setStatus("正在读取选中文本。", "busy");
  try {
    const config = readSavedConfig();
    if (!(await ensureReadyForTextPolish())) {
      return;
    }
    const selection = await readSelectedText();
    const contextApp = selection.targetApp || normalizeRecordingTargetApp(targetApp) || (await readFrontmostApp());
    addDiagnosticLog({
      level: "info",
      category: "process",
      title: "开始润色选中文本",
      message: "已读取当前选中文本，准备发送给 AI 润色。",
      mode: "polish",
      targetApp: contextApp,
      accessibilityTrusted: selection.accessibilityTrusted,
      clipboardRestoreAttempted: true,
      clipboardRestored: selection.clipboardRestored,
      clipboardRestoreMessage: selection.clipboardRestoreMessage,
      pasteMethod: selection.copyMethod,
      details: [`原文字数：${countTextUnits(selection.text)}`, `保持Hub：${keepHubVisible ? "是" : "否"}`],
    });
    const processed = await processRecognizedText(selection.text, "polish", config, contextApp);
    const outputText = processed.text.trim();
    if (!outputText) {
      throw new Error("AI 润色返回为空，已取消替换。");
    }
    resultTextarea.value = outputText;
    resultMeta.textContent = "润色完成";
    processDurationText.textContent = formatDuration(processed.elapsedMs);
    const historyItem = saveHistory({
      id: createId(),
      mode: "polish",
      sourceText: selection.text,
      outputText,
      createdAt: Date.now(),
      recordElapsedMs: 0,
      transcribeElapsedMs: 0,
      processElapsedMs: processed.elapsedMs,
      model: processed.model,
      contextApp,
    });
    updateRecentResult(historyItem);
    addDiagnosticLog({
      level: "success",
      category: "process",
      title: "选中文本润色完成",
      message: "最终输出已准备好替换原选区。",
      mode: "polish",
      targetApp: contextApp,
      elapsedMs: processed.elapsedMs,
      details: [`模型：${processed.model}`, `输出字数：${countTextUnits(outputText)}`],
    });
    if (keepHubVisible) {
      showHubNotice("润色完成，结果已写入最近结果。", "success");
      setStatus("润色完成，结果已更新到 Hub。", "ready");
      return;
    }
    setStatus("正在替换选中文本。", "busy");
    await pasteTranscription(outputText, contextApp);
  } catch (error) {
    const message = formatError(error);
    addDiagnosticLog({
      level: "error",
      category: "process",
      title: "润色选中文本失败",
      message,
      mode: "polish",
      targetApp,
    });
    setStatus(message, "error");
    if (message.includes("辅助功能")) {
      await showHubWindow();
      await switchHubWindowToModeDetail("polish", "accessibility");
    }
  } finally {
    isPolishingSelection = false;
    isProcessing = false;
    recordButton.disabled = false;
    cancelButton.disabled = false;
    floatShell.dataset.activeMode = activeMode;
    if (!keepHubVisible && windowMode === "main" && !isRecording) {
      window.setTimeout(() => void hideFloatingWindow(), 1200);
    }
  }
}

/** 润色模式启动前检查文本替换所需权限，不要求麦克风。 */
async function ensureReadyForTextPolish(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const diagnostics = await readRuntimeDiagnostics();
    if (!diagnostics.hasSessionApiKey && !diagnostics.hasKeychainApiKey && !diagnostics.hasEnvApiKey) {
      await showRequiredModePermission("polish", "apiKey", "请设置润色的 Mimo Key。");
      return false;
    }
    if (!diagnostics.shortcutRegistrationReady) {
      await showRequiredModePermission("polish", "shortcut", "请设置润色的快捷键权限。");
      return false;
    }
    if (!diagnostics.accessibilityTrusted) {
      await showRequiredModePermission("polish", "accessibility", "请设置润色的读取与替换权限。");
      return false;
    }
    return true;
  } catch (error) {
    setStatus(`润色前检查失败：${formatError(error)}`, "error");
    return false;
  }
}

/** 调用桌面端命令读取当前外部应用的选中文本。 */
async function readSelectedText(): Promise<SelectedTextResponse> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SelectedTextResponse>("read_selected_text");
}

/** 把浏览器麦克风异常转换成可行动的提示文案。 */
function formatRecordingError(error: unknown): string {
  if (isMicrophonePermissionError(error)) {
    return "麦克风权限未授权，请在设置中点击麦克风授权。";
  }
  return formatError(error);
}

/** 判断录音失败是否由麦克风权限拒绝或系统权限限制导致。 */
function isMicrophonePermissionError(error: unknown): boolean {
  const name = error instanceof DOMException ? error.name : "";
  return name === "NotAllowedError" || name === "PermissionDeniedError" || name === "SecurityError";
}

/** 根据设置创建浏览器录音约束。 */
function buildAudioConstraints(config: VoiceConfig): MediaTrackConstraints {
  const constraints: MediaTrackConstraints = {
    echoCancellation: true,
    noiseSuppression: true,
    autoGainControl: true,
  };
  if (config.microphoneDeviceId && config.microphoneDeviceId !== "default") {
    constraints.deviceId = { exact: config.microphoneDeviceId };
  }
  return constraints;
}

/** 为实时字幕的系统音频输入创建录音约束，避免浏览器降噪破坏电脑播放声。 */
function buildSystemAudioConstraints(deviceId: string): MediaTrackConstraints {
  return {
    deviceId: { exact: deviceId },
    echoCancellation: false,
    noiseSuppression: false,
    autoGainControl: false,
  };
}

/** 切换实时字幕监听模式。 */
async function toggleSubtitleListening(): Promise<void> {
  if (isStartingSubtitleListening) {
    await emitSubtitleHistoryUpdate("正在准备", false);
    return;
  }
  if (isSubtitleListening) {
    await stopSubtitleListening();
    return;
  }
  await startSubtitleListening();
}

/** 开始采集音频并按固定时间片进行实时 ASR。 */
async function startSubtitleListening(): Promise<void> {
  if (!navigator.mediaDevices?.getUserMedia) {
    await showSubtitleFailure("当前环境不支持音频采集。");
    return;
  }
  if (isRecording || isProcessing) {
    await showSubtitleFailure("当前正在处理普通语音输入，请完成后再开启字幕。");
    return;
  }
  isStartingSubtitleListening = true;
  try {
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "准备实时字幕",
      message: "已进入实时字幕启动流程，开始检查权限和音频输入。",
      mode: "subtitle",
    });
    const permissionReady = await runSubtitleStartupStep(
      ensureReadyForRecording("subtitle"),
      "权限检查",
      "实时字幕权限检查长时间无响应。",
    );
    if (!permissionReady) {
      addDiagnosticLog({
        level: "warning",
        category: "subtitle",
        title: "实时字幕权限检查未通过",
        message: "启动流程已被模式权限门禁拦截，请按弹窗提示补齐权限或配置。",
        mode: "subtitle",
      });
      return;
    }
    resetSubtitleRuntime();
    const config = readSavedConfig();
    await runSubtitleStartupStep(setupSubtitleAudioGraph(config), "录音器初始化", "实时字幕音频输入长时间无响应。");
    isSubtitleListening = true;
    subtitleStartedAt = Date.now();
    subtitleSegmentStartedAt = 0;
    subtitleLastSoundAt = Date.now();
    subtitleLastTextAt = 0;
    startSubtitleTimers();
    void showSubtitleWindowsWithDiagnostics();
    await emitSubtitleOverlay({ text: "", visible: false, state: "listening", updatedAt: Date.now() });
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "开始实时字幕",
      message: "已进入字幕监听模式，开始按固定时间片发送 ASR。",
      mode: "subtitle",
      details: [
        `麦克风：${config.microphoneDeviceId || "default"}`,
        `系统声音：${config.systemAudioDeviceId || "auto"}`,
        `音频来源：${formatSubtitleSource(subtitleCurrentSource)}`,
      ],
    });
    await emitSubtitleHistoryUpdate("监听中", true);
  } catch (error) {
    stopSubtitleAudioGraph();
    await showSubtitleFailure(`实时字幕启动失败：${formatError(error)}`);
  } finally {
    isStartingSubtitleListening = false;
  }
}

/**
 * 为实时字幕启动关键步骤增加超时诊断，避免界面一直停留在“正在准备”。
 * 流程：执行传入步骤；若超时先抛出带步骤名的错误；成功后写入步骤完成日志。
 * 参数：operation 为要等待的异步步骤；stepName 为诊断展示名称；timeoutMessage 为超时提示。
 * 返回：异步步骤的原始返回值。
 * 异常：步骤失败或超时都会抛出 Error，由外层统一展示到字幕历史。
 */
async function runSubtitleStartupStep<T>(operation: Promise<T>, stepName: string, timeoutMessage: string): Promise<T> {
  let timer: number | null = null;
  try {
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: `实时字幕${stepName}开始`,
      message: `${stepName}开始执行。`,
      mode: "subtitle",
    });
    const result = await Promise.race<T>([
      operation,
      new Promise<T>((_, reject) => {
        timer = window.setTimeout(() => {
          reject(new Error(timeoutMessage));
        }, SUBTITLE_STARTUP_STEP_TIMEOUT_MS);
      }),
    ]);
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: `实时字幕${stepName}完成`,
      message: `${stepName}已完成，继续启动实时字幕。`,
      mode: "subtitle",
    });
    return result;
  } finally {
    if (timer !== null) {
      window.clearTimeout(timer);
    }
  }
}

/**
 * 为关键异步操作增加前端侧超时保护。
 * 流程：并行等待原始 Promise 和定时器；原始 Promise 先返回则清理定时器；定时器先触发则抛出指定错误。
 * 参数：operation 为待保护的异步操作；timeoutMs 为超时时间；timeoutMessage 为超时错误文案。
 * 返回：原始异步操作的返回值。
 * 异常：原始操作失败或超时都会向调用方抛出 Error。
 */
async function withTimeout<T>(operation: Promise<T>, timeoutMs: number, timeoutMessage: string): Promise<T> {
  let timer: number | null = null;
  try {
    return await Promise.race<T>([
      operation,
      new Promise<T>((_, reject) => {
        timer = window.setTimeout(() => {
          reject(new Error(timeoutMessage));
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer !== null) {
      window.clearTimeout(timer);
    }
  }
}

/** 停止实时字幕监听，并把未固化字幕写入历史。 */
async function stopSubtitleListening(): Promise<void> {
  if (!isSubtitleListening && !isStartingSubtitleListening) {
    await hideSubtitleWindows();
    return;
  }
  stopSubtitleTimers();
  await finalizeSubtitleSegment("stop");
  isSubtitleListening = false;
  subtitleDispatchQueued = false;
  stopSubtitleAudioGraph();
  await emitSubtitleOverlay({ text: "", visible: false, state: "hidden", updatedAt: Date.now() });
  await emitSubtitleHistoryUpdate("已停止", false);
  await hideSubtitleWindows();
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "停止实时字幕",
    message: "已退出字幕监听模式，并停止音频采集。",
    mode: "subtitle",
  });
}

/** 初始化实时字幕的麦克风与系统音频录音器。 */
async function setupSubtitleAudioGraph(config: VoiceConfig): Promise<void> {
  isSubtitleUsingNativeSystemAudio = config.systemAudioDeviceId === NATIVE_SYSTEM_AUDIO_DEVICE_ID;
  if (isSubtitleUsingNativeSystemAudio) {
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "实时字幕使用原生系统音频",
      message: config.subtitleIncludeMicrophone
        ? "已选择 Core Audio Process Tap，并会同时采集麦克风输入。"
        : "已选择 Core Audio Process Tap，将只采集选定系统应用。",
      mode: "subtitle",
      details: [`目标 App：${formatSubtitleTargetApps(config)}`],
    });
    if (config.subtitleIncludeMicrophone) {
      try {
        subtitleMicStream = await requestSubtitleAudioStream({ audio: buildAudioConstraints(config) }, "麦克风");
      } catch (error) {
        addDiagnosticLog({
          level: "warning",
          category: "subtitle",
          title: "实时字幕麦克风不可用",
          message: `麦克风打开失败，本轮仅采集系统声音：${formatError(error)}`,
          mode: "subtitle",
        });
        subtitleMicStream = null;
      }
    }
    subtitleCurrentSource = subtitleMicStream ? "mixed" : "system";
    subtitleRecorderChunkIndex = 0;
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "实时字幕开始创建录音器",
      message: subtitleMicStream ? "音频输入已准备，开始创建原生系统音频和麦克风切片。" : "音频输入已准备，开始创建原生系统音频切片。",
      mode: "subtitle",
      details: [`音频来源：${formatSubtitleSource(subtitleCurrentSource)}`],
    });
    startSubtitleRecorderSegment();
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "实时字幕录音器就绪",
      message: "原生系统音频采集已就绪，按独立小段录制并发送 ASR。",
      mode: "subtitle",
      details: [`音频来源：${formatSubtitleSource(subtitleCurrentSource)}`],
    });
    return;
  }

  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕开始申请麦克风",
    message: "准备打开实时字幕麦克风输入。",
    mode: "subtitle",
    details: [`麦克风：${config.microphoneDeviceId || "default"}`],
  });
  subtitleMicStream = await requestSubtitleAudioStream({ audio: buildAudioConstraints(config) }, "麦克风");
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕麦克风已打开",
    message: "麦克风输入已返回，继续解析系统声音输入。",
    mode: "subtitle",
    details: [`轨道数：${subtitleMicStream.getAudioTracks().length}`],
  });
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕解析系统声音",
    message: "开始根据设置解析系统声音输入设备。",
    mode: "subtitle",
    details: [`系统声音设置：${config.systemAudioDeviceId || "auto"}`],
  });
  const systemAudioDeviceId = await resolveSystemAudioDeviceId(config);
  if (systemAudioDeviceId) {
    try {
      addDiagnosticLog({
        level: "info",
        category: "subtitle",
        title: "实时字幕开始申请系统声音",
        message: "已找到系统声音候选设备，准备打开系统声音输入。",
        mode: "subtitle",
        details: [`系统声音设备：${systemAudioDeviceId}`],
      });
      subtitleSystemStream = await requestSubtitleAudioStream(
        {
          audio: buildSystemAudioConstraints(systemAudioDeviceId),
        },
        "系统声音",
      );
    } catch (error) {
      addDiagnosticLog({
        level: "warning",
        category: "subtitle",
        title: "系统音频输入不可用",
        message: `系统声音设备打开失败，已先使用麦克风继续字幕监听：${formatError(error)}`,
        mode: "subtitle",
        details: [`系统声音设备：${systemAudioDeviceId}`],
      });
      subtitleSystemStream = null;
    }
  } else {
    addDiagnosticLog({
      level: "warning",
      category: "subtitle",
      title: "实时字幕未找到系统声音输入",
      message: "未解析到可用的系统声音输入设备，本轮将先使用麦克风输入继续监听。",
      mode: "subtitle",
      details: [`系统声音设置：${config.systemAudioDeviceId || "auto"}`],
    });
  }

  subtitleCurrentSource = isSubtitleUsingNativeSystemAudio ? "system" : subtitleSystemStream ? "mixed" : "microphone";
  subtitleRecorderChunkIndex = 0;
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕开始创建录音器",
    message: isSubtitleUsingNativeSystemAudio
      ? "音频输入已准备，开始创建原生系统音频切片。"
      : "音频输入已准备，开始创建 MediaRecorder。",
    mode: "subtitle",
    details: [`音频来源：${formatSubtitleSource(subtitleCurrentSource)}`],
  });
  startSubtitleRecorderSegment();
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕录音器就绪",
    message: isSubtitleUsingNativeSystemAudio
      ? "原生系统音频采集已就绪，按独立小段录制并发送 ASR。"
      : "麦克风与系统声音输入已接入 MediaRecorder，按独立小段录制并发送 ASR。",
    mode: "subtitle",
    details: [`音频来源：${formatSubtitleSource(subtitleCurrentSource)}`],
  });
}

/** 格式化实时字幕原生系统音频目标，便于诊断日志和设置卡片展示。 */
function formatSubtitleTargetApps(config: VoiceConfig): string {
  return normalizeSubtitleTargetApps(config.subtitleTargetApps, ["active"])
    .map((target) => (target === "active" ? "自动选择" : target))
    .join("、");
}

/** 读取原生系统音频 helper 的目标参数，多个 App 用逗号交给 helper 混合采集。 */
function readSubtitleNativeTargetKeyword(): string {
  const config = readSavedConfig();
  return normalizeSubtitleTargetApps(config.subtitleTargetApps, ["active"]).join(",");
}

/** 启动一段独立的实时字幕录音，停止后自动转写并进入下一段。 */
function startSubtitleRecorderSegment(): void {
  const hasAudioInput = isSubtitleUsingNativeSystemAudio || subtitleMicStream !== null;
  if ((!isSubtitleListening && !isStartingSubtitleListening) || !hasAudioInput) {
    addDiagnosticLog({
      level: "warning",
      category: "subtitle",
      title: "实时字幕录音片段未启动",
      message: "当前状态不允许启动录音片段。",
      mode: "subtitle",
      details: [
        `监听中：${isSubtitleListening ? "是" : "否"}`,
        `启动中：${isStartingSubtitleListening ? "是" : "否"}`,
        `麦克风：${subtitleMicStream ? "已打开" : "未打开"}`,
        `原生系统音频：${isSubtitleUsingNativeSystemAudio ? "已启用" : "未启用"}`,
      ],
    });
    return;
  }
  if (isSubtitleUsingNativeSystemAudio) {
    void startSubtitleNativeRecorderSegment();
    if (subtitleMicStream && !subtitleMediaRecorder) {
      startSubtitleMediaRecorderSegment(subtitleMicStream, null);
    }
    return;
  }
  if (!subtitleMicStream) {
    return;
  }
  startSubtitleMediaRecorderSegment(subtitleMicStream, subtitleSystemStream);
}

/** 启动一段浏览器 MediaRecorder 字幕录音，供麦克风或虚拟系统输入使用。 */
function startSubtitleMediaRecorderSegment(micStream: MediaStream, systemStream: MediaStream | null): void {
  const recorder = createSubtitleMediaRecorder(micStream, systemStream);
  const chunks: Blob[] = [];
  const mimeType = recorder.mimeType || "audio/webm";
  subtitleMediaRecorder = recorder;
  subtitleRecorderMimeType = mimeType;
  recorder.addEventListener("dataavailable", (event) => {
    if (event.data.size) {
      chunks.push(event.data);
    }
  });
  recorder.addEventListener("stop", () => {
    if (subtitleRecorderStopTimerHandle !== null) {
      window.clearTimeout(subtitleRecorderStopTimerHandle);
      subtitleRecorderStopTimerHandle = null;
    }
    subtitleMediaRecorder = null;
    const shouldContinue = isSubtitleListening && subtitleMicStream !== null;
    if (chunks.length) {
      subtitleRecorderChunkIndex += 1;
      const blob = new Blob(chunks, { type: mimeType });
      void handleSubtitleRecorderChunk({
        index: subtitleRecorderChunkIndex,
        blob,
        mimeType,
      });
    }
    if (shouldContinue) {
      startSubtitleRecorderSegment();
    }
  });
  recorder.addEventListener("error", (event) => {
    addDiagnosticLog({
      level: "error",
      category: "subtitle",
      title: "实时字幕录音器异常",
      message: formatError(event.error),
      mode: "subtitle",
    });
    void emitSubtitleHistoryUpdate("录音器异常", true);
  });
  recorder.start();
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕录音片段已开始",
    message: "MediaRecorder 已开始录制当前字幕片段。",
    mode: "subtitle",
    details: [`格式：${mimeType}`, `片段时长：${SUBTITLE_CHUNK_MS}ms`],
  });
  subtitleRecorderStopTimerHandle = window.setTimeout(() => {
    if (recorder.state !== "inactive") {
      recorder.stop();
    }
  }, SUBTITLE_CHUNK_MS);
}

/** 启动一段原生系统音频录制，停止后自动转写并进入下一段。 */
async function startSubtitleNativeRecorderSegment(): Promise<void> {
  if (
    (!isSubtitleListening && !isStartingSubtitleListening) ||
    !isSubtitleUsingNativeSystemAudio ||
    subtitleNativeChunkInFlight
  ) {
    return;
  }
  subtitleRecorderChunkIndex += 1;
  const chunkIndex = subtitleRecorderChunkIndex;
  try {
    subtitleNativeChunkInFlight = true;
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "实时字幕原生音频片段已开始",
      message: "Core Audio Process Tap 正在采集当前活跃系统声音。",
      mode: "subtitle",
      details: [`序号：${chunkIndex}`, `片段时长：${SUBTITLE_NATIVE_CHUNK_MS}ms`],
    });
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "实时字幕调用原生采集",
      message: "正在调用 Tauri 原生命令采集系统音频。",
      mode: "subtitle",
      details: [`序号：${chunkIndex}`, `超时：${SUBTITLE_NATIVE_CAPTURE_TIMEOUT_MS}ms`],
    });
    if (subtitleNativeChunkTimeoutHandle !== null) {
      window.clearTimeout(subtitleNativeChunkTimeoutHandle);
    }
    subtitleNativeChunkTimeoutHandle = window.setTimeout(() => {
      subtitleNativeChunkInFlight = false;
      addDiagnosticLog({
        level: "error",
        category: "subtitle",
        title: "原生系统音频采集超时",
        message: "原生系统音频采集或转写长时间无响应，已准备下一段重试。",
        mode: "subtitle",
        details: [`序号：${chunkIndex}`],
      });
      void emitSubtitleHistoryUpdate("系统音频采集超时", true);
      if (isSubtitleListening && isSubtitleUsingNativeSystemAudio) {
        startSubtitleRecorderSegment();
      }
    }, SUBTITLE_NATIVE_CAPTURE_TIMEOUT_MS + 25000);
    void startProcessTapTranscribeTask(SUBTITLE_NATIVE_CHUNK_MS, readSubtitleNativeTargetKeyword(), chunkIndex).catch((error: unknown) => {
      handleSubtitleNativeTranscribeFailure(chunkIndex, formatError(error));
    });
    void pollProcessTapTranscribeOutcome(chunkIndex);
  } catch (error) {
    subtitleNativeChunkInFlight = false;
    addDiagnosticLog({
      level: "error",
      category: "subtitle",
      title: "原生系统音频采集失败",
      message: formatError(error),
      mode: "subtitle",
    });
    await emitSubtitleHistoryUpdate("系统音频采集失败", true);
  }
}

/** 轮询 Rust 后台字幕任务结果，用短命令避免长耗时 invoke 卡住 WebView。 */
async function pollProcessTapTranscribeOutcome(chunkIndex: number): Promise<void> {
  const startedAt = Date.now();
  while (isSubtitleListening && isSubtitleUsingNativeSystemAudio && subtitleNativeChunkInFlight) {
    const outcome = await takeProcessTapTranscribeOutcome(chunkIndex);
    if (outcome) {
      await handleSubtitleNativeTranscribeOutcome(outcome);
      return;
    }
    if (Date.now() - startedAt >= SUBTITLE_NATIVE_CAPTURE_TIMEOUT_MS + 25000) {
      handleSubtitleNativeTranscribeFailure(chunkIndex, "原生系统音频采集或转写长时间无响应。");
      return;
    }
    await delay(500);
  }
}

/** 处理原生系统音频后台任务完成结果。 */
async function handleSubtitleNativeTranscribeOutcome(outcome: ProcessTapTranscribeOutcome): Promise<void> {
  if (outcome.ok && outcome.response) {
    await handleSubtitleNativeTranscribeResult(outcome.response);
    return;
  }
  handleSubtitleNativeTranscribeFailure(outcome.chunkIndex, outcome.error || "原生系统音频转写失败。");
}

/** 处理原生系统音频采集或转写失败，并在监听中继续下一段。 */
function handleSubtitleNativeTranscribeFailure(chunkIndex: number, message: string): void {
  if (!subtitleNativeChunkInFlight) {
    return;
  }
  subtitleNativeChunkInFlight = false;
  if (subtitleNativeChunkTimeoutHandle !== null) {
    window.clearTimeout(subtitleNativeChunkTimeoutHandle);
    subtitleNativeChunkTimeoutHandle = null;
  }
  const canContinueWithMicrophone = Boolean(subtitleMicStream);
  addDiagnosticLog({
    level: canContinueWithMicrophone ? "warning" : "error",
    category: "subtitle",
    title: canContinueWithMicrophone ? "系统声音目标暂不可用" : "原生系统音频采集失败",
    message: canContinueWithMicrophone ? `系统声音采集失败，已继续使用麦克风监听：${message}` : message,
    mode: "subtitle",
    details: [`序号：${chunkIndex}`],
  });
  void emitSubtitleHistoryUpdate(canContinueWithMicrophone ? "系统声音暂不可用，继续麦克风" : "系统音频采集失败", true);
  if (isSubtitleListening && isSubtitleUsingNativeSystemAudio) {
    startSubtitleRecorderSegment();
  }
}

/** 等待指定毫秒数后继续执行。 */
function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

/** 处理 Rust 端返回的原生系统音频转写结果，并继续下一段实时字幕。 */
async function handleSubtitleNativeTranscribeResult(response: ProcessTapTranscribeResponse): Promise<void> {
  if (!subtitleNativeChunkInFlight) {
    return;
  }
  subtitleNativeChunkInFlight = false;
  if (subtitleNativeChunkTimeoutHandle !== null) {
    window.clearTimeout(subtitleNativeChunkTimeoutHandle);
    subtitleNativeChunkTimeoutHandle = null;
  }
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕原生转写返回",
    message: "Tauri 原生命令已返回系统音频字幕。",
    mode: "subtitle",
    elapsedMs: response.captureElapsedMs + response.elapsedMs,
    details: [
      `序号：${response.chunkIndex}`,
      `音频大小：${formatBytes(response.bytes)}`,
      `采集耗时：${formatDuration(response.captureElapsedMs)}`,
      `转写耗时：${formatDuration(response.elapsedMs)}`,
      response.summary,
    ],
  });
  await handleSubtitleText(response.text, response);
  addDiagnosticLog({
    level: "info",
    category: "subtitle",
    title: "实时字幕原生音频片段完成",
    message: "系统音频片段已完成采集和 ASR。",
    mode: "subtitle",
    elapsedMs: response.captureElapsedMs + response.elapsedMs,
    details: [`序号：${response.chunkIndex}`, `音频大小：${formatBytes(response.bytes)}`, response.summary],
  });
  if (isSubtitleListening && isSubtitleUsingNativeSystemAudio) {
    startSubtitleRecorderSegment();
  }
}

/**
 * 创建实时字幕 MediaRecorder，优先把麦克风与系统声音轨道放进同一个录音流。
 * 流程：合并可用音频轨道；选择当前 WebView 支持的 mimeType；创建失败时回退到麦克风单轨。
 * 参数：micStream 为麦克风输入；systemStream 为可选系统声音输入。
 * 返回：可按固定时间片产出 Blob 的 MediaRecorder。
 * 异常：麦克风单轨也无法创建录音器时抛出浏览器原始错误。
 */
function createSubtitleMediaRecorder(micStream: MediaStream, systemStream: MediaStream | null): MediaRecorder {
  const combinedTracks = [...micStream.getAudioTracks(), ...(systemStream?.getAudioTracks() ?? [])];
  const combinedStream = new MediaStream(combinedTracks);
  try {
    return new MediaRecorder(combinedStream, createSubtitleRecorderOptions());
  } catch (error) {
    if (!systemStream) {
      throw error;
    }
    addDiagnosticLog({
      level: "warning",
      category: "subtitle",
      title: "系统声音混合录音失败",
      message: `系统声音与麦克风混合录音器创建失败，已回退到麦克风：${formatError(error)}`,
      mode: "subtitle",
    });
    subtitleCurrentSource = "microphone";
    return new MediaRecorder(new MediaStream(micStream.getAudioTracks()), createSubtitleRecorderOptions());
  }
}

/** 选择当前 WebView 支持的实时字幕录音格式，优先选择 ASR 常见可识别格式。 */
function createSubtitleRecorderOptions(): MediaRecorderOptions {
  const mimeType = ["audio/webm;codecs=opus", "audio/webm", "audio/mp4", "audio/aac"].find((candidate) =>
    MediaRecorder.isTypeSupported(candidate),
  );
  return mimeType ? { mimeType } : {};
}

/** 处理 MediaRecorder 产出的实时字幕音频片段，并串行发送给 ASR。 */
async function handleSubtitleRecorderChunk(chunk: SubtitleRecorderChunk): Promise<void> {
  if (!isSubtitleListening || !chunk.blob.size) {
    return;
  }
  subtitleLastSoundAt = Date.now();
  if (subtitleInFlight) {
    subtitleDispatchQueued = true;
    subtitlePendingRecorderChunk = chunk;
    addDiagnosticLog({
      level: "warning",
      category: "subtitle",
      title: "字幕切片等待",
      message: "上一段字幕仍在转写，已保留最新片段等待下一次发送。",
      mode: "subtitle",
      details: [`序号：${chunk.index}`, `音频大小：${formatBytes(chunk.blob.size)}`, `格式：${chunk.mimeType || "unknown"}`],
    });
    return;
  }
  subtitleInFlight = true;
  try {
    let currentChunk: SubtitleRecorderChunk | null = chunk;
    while (currentChunk && isSubtitleListening) {
      await transcribeSubtitleBlob(currentChunk.blob, currentChunk.index);
      currentChunk = subtitlePendingRecorderChunk;
      subtitlePendingRecorderChunk = null;
    }
  } finally {
    subtitleInFlight = false;
    subtitleDispatchQueued = false;
    if (subtitlePendingRecorderChunk && isSubtitleListening) {
      const nextChunk = subtitlePendingRecorderChunk;
      subtitlePendingRecorderChunk = null;
      void handleSubtitleRecorderChunk(nextChunk);
    }
  }
}

/**
 * 申请实时字幕音频流，并在系统长时间不返回权限或设备结果时主动失败。
 * 流程：发起 getUserMedia，同时设置超时；任一方先完成后清理计时器。
 * 参数：constraints 为浏览器音频采集约束；label 用于错误提示。
 * 返回：成功获取到的 MediaStream。
 * 异常：浏览器权限拒绝、设备不可用或超时都会抛出 Error。
 */
function requestSubtitleAudioStream(constraints: MediaStreamConstraints, label: string): Promise<MediaStream> {
  return new Promise<MediaStream>((resolve, reject) => {
    let settled = false;
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: `实时字幕${label}输入申请中`,
      message: `已向系统申请${label}音频输入。`,
      mode: "subtitle",
    });
    const timer = window.setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      addDiagnosticLog({
        level: "error",
        category: "subtitle",
        title: `实时字幕${label}输入超时`,
        message: `${label}音频输入长时间无响应，请检查设备权限或输入源。`,
        mode: "subtitle",
      });
      reject(new Error(`${label}音频输入长时间无响应，请检查设备权限或输入源。`));
    }, SUBTITLE_AUDIO_SETUP_TIMEOUT_MS);

    navigator.mediaDevices
      .getUserMedia(constraints)
      .then((stream) => {
        if (settled) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }
        settled = true;
        window.clearTimeout(timer);
        addDiagnosticLog({
          level: "info",
          category: "subtitle",
          title: `实时字幕${label}输入已返回`,
          message: `${label}音频输入已成功打开。`,
          mode: "subtitle",
          details: [`轨道数：${stream.getAudioTracks().length}`],
        });
        resolve(stream);
      })
      .catch((error: unknown) => {
        if (settled) {
          return;
        }
        settled = true;
        window.clearTimeout(timer);
        addDiagnosticLog({
          level: "error",
          category: "subtitle",
          title: `实时字幕${label}输入失败`,
          message: formatError(error),
          mode: "subtitle",
        });
        reject(error instanceof Error ? error : new Error(formatError(error)));
      });
  });
}

/**
 * 确保实时字幕 AudioContext 真正进入运行态，避免 WebView 创建后保持 suspended 导致无采样回调。
 * 流程：如果当前不是 running，则调用 resume；仍未运行时写入警告日志，便于现场排查。
 */
async function ensureSubtitleAudioContextRunning(): Promise<void> {
  if (!subtitleAudioContext || subtitleAudioContext.state === "running") {
    return;
  }
  await subtitleAudioContext.resume();
  const resumedState = subtitleAudioContext.state as AudioContextState;
  if (resumedState !== "running") {
    addDiagnosticLog({
      level: "warning",
      category: "subtitle",
      title: "实时字幕音频上下文未运行",
      message: "AudioContext 没有进入 running 状态，可能导致字幕切片无法产生。",
      mode: "subtitle",
      details: [`AudioContext：${resumedState}`],
    });
  }
}

/** 自动选择可代表系统播放声音的输入设备，优先识别虚拟声卡。 */
async function resolveSystemAudioDeviceId(config: VoiceConfig): Promise<string> {
  const savedDeviceId = config.systemAudioDeviceId.trim();
  if (savedDeviceId === "none") {
    return "";
  }
  if (savedDeviceId && savedDeviceId !== "auto") {
    if (savedDeviceId === config.microphoneDeviceId) {
      return "";
    }
    return savedDeviceId;
  }
  if (!navigator.mediaDevices?.enumerateDevices) {
    return "";
  }
  const devices = await navigator.mediaDevices.enumerateDevices();
  const microphoneDeviceId = config.microphoneDeviceId === "default" ? "" : config.microphoneDeviceId;
  const candidate = devices
    .filter((device) => device.kind === "audioinput" && device.deviceId && device.deviceId !== microphoneDeviceId)
    .find((device) => isSystemAudioInputLabel(device.label));
  return candidate?.deviceId || "";
}

/** 判断输入设备名称是否像系统音频或虚拟声卡。 */
function isSystemAudioInputLabel(label: string): boolean {
  const normalizedLabel = label.toLowerCase();
  return [
    "blackhole",
    "loopback",
    "soundflower",
    "system audio",
    "virtual",
    "aggregate",
    "multi-output",
    "monitor",
    "stereo mix",
    "系统",
    "电脑",
  ].some((keyword) => normalizedLabel.includes(keyword));
}

/** 重置实时字幕运行时缓存，确保新一轮监听不带上旧音频。 */
function resetSubtitleRuntime(): void {
  subtitleSampleChunks = [];
  subtitleTotalSamples = 0;
  subtitleDispatchedSampleEnd = 0;
  subtitleSampleRate = 0;
  subtitleInFlight = false;
  subtitleDispatchQueued = false;
  subtitlePendingRecorderChunk = null;
  subtitleNativeChunkInFlight = false;
  if (subtitleNativeChunkTimeoutHandle !== null) {
    window.clearTimeout(subtitleNativeChunkTimeoutHandle);
    subtitleNativeChunkTimeoutHandle = null;
  }
  subtitlePendingText = "";
  subtitleLastDisplayedText = "";
  subtitleLastModel = "";
  subtitleCurrentSource = "microphone";
}

/** 收集实时字幕音频样本，来源可能是麦克风或麦克风加系统声音混音。 */
function collectSubtitleSamples(event: AudioProcessingEvent): void {
  if (!isSubtitleListening && !isStartingSubtitleListening) {
    return;
  }
  const input = event.inputBuffer.getChannelData(0);
  const level = readAudioLevel(input);
  if (level >= SUBTITLE_SOUND_LEVEL_THRESHOLD) {
    subtitleLastSoundAt = Date.now();
  }
  const sampleCopy = new Float32Array(input.length);
  sampleCopy.set(input);
  subtitleSampleChunks.push({
    startSample: subtitleTotalSamples,
    samples: sampleCopy,
    level,
  });
  subtitleTotalSamples += sampleCopy.length;
  trimSubtitleSampleBuffer();
}

/** 裁剪实时字幕音频缓存，保留重叠区和最近一段上下文即可。 */
function trimSubtitleSampleBuffer(): void {
  if (!subtitleSampleRate || !subtitleSampleChunks.length) {
    return;
  }
  const overlapSamples = Math.floor((SUBTITLE_OVERLAP_MS / 1000) * subtitleSampleRate);
  const recentSamples = Math.floor(30 * subtitleSampleRate);
  const retainFrom = Math.max(0, Math.min(subtitleDispatchedSampleEnd - overlapSamples * 2, subtitleTotalSamples - recentSamples));
  while (subtitleSampleChunks.length) {
    const first = subtitleSampleChunks[0];
    if (first.startSample + first.samples.length >= retainFrom) {
      return;
    }
    subtitleSampleChunks.shift();
  }
}

/** 启动字幕切片和固化检查定时器。 */
function startSubtitleTimers(): void {
  stopSubtitleTimers();
  subtitleDispatchTimerHandle = window.setInterval(() => {
    void dispatchSubtitleChunkIfReady(false);
  }, SUBTITLE_DISPATCH_INTERVAL_MS);
  subtitleUiTimerHandle = window.setInterval(() => {
    void runSubtitleHousekeeping();
  }, 500);
}

/** 停止字幕切片和固化检查定时器。 */
function stopSubtitleTimers(): void {
  if (subtitleDispatchTimerHandle !== null) {
    window.clearInterval(subtitleDispatchTimerHandle);
    subtitleDispatchTimerHandle = null;
  }
  if (subtitleUiTimerHandle !== null) {
    window.clearInterval(subtitleUiTimerHandle);
    subtitleUiTimerHandle = null;
  }
  if (subtitleOverlayHideTimerHandle !== null) {
    window.clearTimeout(subtitleOverlayHideTimerHandle);
    subtitleOverlayHideTimerHandle = null;
  }
}

/** 在达到固定时间片后发送 ASR，请求并发时只保留下一次发送机会。 */
async function dispatchSubtitleChunkIfReady(force: boolean): Promise<void> {
  if ((!isSubtitleListening && !force) || !subtitleSampleRate || !subtitleSampleChunks.length) {
    return;
  }
  if (subtitleInFlight) {
    subtitleDispatchQueued = true;
    return;
  }
  const chunkSamples = Math.floor((SUBTITLE_CHUNK_MS / 1000) * subtitleSampleRate);
  const minChunkSamples = Math.floor((SUBTITLE_MIN_CHUNK_MS / 1000) * subtitleSampleRate);
  const maxChunkSamples = Math.floor((SUBTITLE_MAX_CHUNK_MS / 1000) * subtitleSampleRate);
  const pendingSamples = subtitleTotalSamples - subtitleDispatchedSampleEnd;
  if (!force && pendingSamples < chunkSamples) {
    return;
  }
  const endSample = subtitleTotalSamples;
  const overlapSamples = Math.floor((SUBTITLE_OVERLAP_MS / 1000) * subtitleSampleRate);
  const startSample = Math.max(0, endSample - maxChunkSamples, subtitleDispatchedSampleEnd - overlapSamples);
  if (endSample - startSample < minChunkSamples) {
    return;
  }
  const samples = sliceSubtitleSamples(startSample, endSample);
  subtitleDispatchedSampleEnd = endSample;
  const level = readAudioLevel(samples);
  if (level < SUBTITLE_SOUND_LEVEL_THRESHOLD && Date.now() - subtitleLastSoundAt >= SUBTITLE_SILENCE_FINALIZE_MS) {
    await finalizeSubtitleSegment("silence");
    return;
  }

  subtitleInFlight = true;
  try {
    await transcribeSubtitleSamples(samples, level);
  } finally {
    subtitleInFlight = false;
    if (subtitleDispatchQueued && isSubtitleListening) {
      subtitleDispatchQueued = false;
      void dispatchSubtitleChunkIfReady(false);
    }
  }
}

/** 从实时字幕采样缓存里截取指定绝对采样范围。 */
function sliceSubtitleSamples(startSample: number, endSample: number): Float32Array {
  const samples = new Float32Array(endSample - startSample);
  let offset = 0;
  for (const chunk of subtitleSampleChunks) {
    const chunkStart = chunk.startSample;
    const chunkEnd = chunk.startSample + chunk.samples.length;
    if (chunkEnd <= startSample || chunkStart >= endSample) {
      continue;
    }
    const sourceStart = Math.max(0, startSample - chunkStart);
    const sourceEnd = Math.min(chunk.samples.length, endSample - chunkStart);
    samples.set(chunk.samples.subarray(sourceStart, sourceEnd), offset);
    offset += sourceEnd - sourceStart;
  }
  return samples;
}

/** 把字幕音频片段发送给 Mimo ASR，并处理返回文本。 */
async function transcribeSubtitleSamples(samples: Float32Array, level: number): Promise<void> {
  const config = readSavedConfig();
  const audioBlob = new Blob([encodeWav(samples, subtitleSampleRate)], { type: "audio/wav" });
  await transcribeSubtitleBlob(audioBlob, subtitleRecorderChunkIndex, level);
}

/** 把 MediaRecorder 产出的字幕音频片段发送给 Mimo ASR，并处理返回文本。 */
async function transcribeSubtitleBlob(audioBlob: Blob, chunkIndex: number, level?: number): Promise<void> {
  const config = readSavedConfig();
  try {
    const requestBlob = await normalizeSubtitleAudioBlob(audioBlob);
    addDiagnosticLog({
      level: "info",
      category: "subtitle",
      title: "发送字幕切片",
      message: "正在把实时字幕音频片段发送给 ASR。",
      mode: "subtitle",
      details: [
        `序号：${chunkIndex}`,
        `音频大小：${formatBytes(audioBlob.size)}`,
        `格式：${audioBlob.type || subtitleRecorderMimeType || "unknown"}`,
        requestBlob === audioBlob ? "转码：未转码" : `转码：WAV ${formatBytes(requestBlob.size)}`,
        typeof level === "number" ? `音量：${level.toFixed(3)}` : "音量：MediaRecorder",
      ],
    });
    const response = await callTranscribe({
      apiKey: "",
      baseUrl: config.baseUrl,
      asrModel: config.asrModel,
      language: config.language,
      contentType: requestBlob.type || "audio/wav",
      audioBase64: await blobToBase64(requestBlob),
    });
    const text = normalizeSubtitleText(response.text);
    subtitleLastModel = response.model;
    if (!isMeaningfulTranscription(text)) {
      addDiagnosticLog({
        level: "warning",
        category: "subtitle",
        title: "字幕切片未返回文字",
        message: "ASR 已返回，但本片段没有可展示字幕。",
        mode: "subtitle",
        elapsedMs: response.elapsedMs,
      });
      return;
    }
    await handleSubtitleText(text, response);
  } catch (error) {
    addDiagnosticLog({
      level: "error",
      category: "subtitle",
      title: "字幕转写失败",
      message: formatError(error),
      mode: "subtitle",
    });
    await emitSubtitleHistoryUpdate("转写失败", true);
  }
}

/**
 * 把实时字幕录音器产出的浏览器格式统一转成 ASR 已验证可接受的 WAV。
 * 流程：如果已经是 WAV 直接返回；否则用 AudioContext 解码，再复用现有 WAV 编码器。
 * 参数：blob 为 MediaRecorder 的原始片段。
 * 返回：可发送给 ASR 的 WAV Blob。
 * 异常：浏览器无法解码该片段时抛出错误，外层会写入字幕转写失败日志。
 */
async function normalizeSubtitleAudioBlob(blob: Blob): Promise<Blob> {
  if (blob.type.includes("wav")) {
    return blob;
  }
  const arrayBuffer = await blob.arrayBuffer();
  const decodeContext = new AudioContext();
  try {
    const decodedBuffer = await decodeContext.decodeAudioData(arrayBuffer.slice(0));
    const channelData = decodedBuffer.getChannelData(0);
    const samples = new Float32Array(channelData.length);
    samples.set(channelData);
    return new Blob([encodeWav(samples, decodedBuffer.sampleRate)], { type: "audio/wav" });
  } catch (error) {
    addDiagnosticLog({
      level: "error",
      category: "subtitle",
      title: "字幕转码失败",
      message: formatError(error),
      mode: "subtitle",
      details: [`音频大小：${formatBytes(blob.size)}`, `格式：${blob.type || subtitleRecorderMimeType || "unknown"}`],
    });
    throw error;
  } finally {
    void decodeContext.close();
  }
}

/** 处理 ASR 返回的字幕文本，刷新底部字幕条并合并到当前待固化片段。 */
async function handleSubtitleText(text: string, response: TranscribeResponse): Promise<void> {
  const normalizedText = normalizeSubtitleText(text);
  if (!normalizedText) {
    return;
  }
  subtitlePendingText = mergeSubtitleText(subtitlePendingText, normalizedText);
  if (!subtitleSegmentStartedAt) {
    subtitleSegmentStartedAt = Date.now();
  }
  subtitleLastTextAt = Date.now();
  subtitleLastDisplayedText = normalizedText;
  await emitSubtitleOverlay({ text: normalizedText, visible: true, state: "text", updatedAt: subtitleLastTextAt });
  scheduleSubtitleOverlayHide();
  await emitSubtitleHistoryUpdate(`识别中 · ${formatDuration(response.elapsedMs)}`, true);
  addDiagnosticLog({
    level: "success",
    category: "subtitle",
    title: "字幕已返回",
    message: "ASR 已返回可展示字幕。",
    mode: "subtitle",
    elapsedMs: response.elapsedMs,
    details: [`字数：${countTextUnits(normalizedText)}`, `模型：${response.model}`],
  });
  if (Date.now() - subtitleSegmentStartedAt >= SUBTITLE_FORCE_FINALIZE_MS) {
    await finalizeSubtitleSegment("force");
  }
}

/** 处理字幕隐藏和停顿固化，不会因为短暂停顿退出监听。 */
async function runSubtitleHousekeeping(): Promise<void> {
  if (!isSubtitleListening) {
    return;
  }
  const now = Date.now();
  if (subtitlePendingText && subtitleLastSoundAt && now - subtitleLastSoundAt >= SUBTITLE_SILENCE_FINALIZE_MS) {
    await finalizeSubtitleSegment("silence");
  }
  if (subtitlePendingText && subtitleSegmentStartedAt && now - subtitleSegmentStartedAt >= SUBTITLE_FORCE_FINALIZE_MS) {
    await finalizeSubtitleSegment("force");
  }
  if (subtitleLastDisplayedText && subtitleLastTextAt && now - subtitleLastTextAt >= SUBTITLE_HIDE_DELAY_MS) {
    subtitleLastDisplayedText = "";
    await emitSubtitleOverlay({ text: "", visible: false, state: "hidden", updatedAt: now });
  }
}

/** 为底部字幕条安排自动隐藏，视觉隐藏不影响监听和历史。 */
function scheduleSubtitleOverlayHide(): void {
  if (subtitleOverlayHideTimerHandle !== null) {
    window.clearTimeout(subtitleOverlayHideTimerHandle);
  }
  subtitleOverlayHideTimerHandle = window.setTimeout(() => {
    subtitleLastDisplayedText = "";
    void emitSubtitleOverlay({ text: "", visible: false, state: "hidden", updatedAt: Date.now() });
  }, SUBTITLE_HIDE_DELAY_MS);
}

/** 将当前字幕片段固化到右上角历史。 */
async function finalizeSubtitleSegment(reason: "silence" | "force" | "stop"): Promise<void> {
  const text = normalizeSubtitleText(subtitlePendingText);
  if (!isMeaningfulTranscription(text)) {
    subtitlePendingText = "";
    subtitleSegmentStartedAt = 0;
    return;
  }
  const createdAt = Date.now();
  const historyItem: SubtitleHistoryItem = {
    id: createId(),
    text,
    createdAt,
    source: subtitleCurrentSource,
    elapsedMs: subtitleSegmentStartedAt ? createdAt - subtitleSegmentStartedAt : 0,
    model: subtitleLastModel || readSavedConfig().asrModel,
  };
  saveSubtitleHistory(historyItem);
  subtitlePendingText = "";
  subtitleSegmentStartedAt = 0;
  await emitSubtitleHistoryUpdate(reason === "stop" ? "已停止" : "已记录", isSubtitleListening);
}

/** 规范化字幕文本，去掉 ASR 容易返回的多余空白。 */
function normalizeSubtitleText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/** 合并带重叠区的字幕片段，尽量避免历史里出现重复文字。 */
function mergeSubtitleText(previous: string, next: string): string {
  const left = normalizeSubtitleText(previous);
  const right = normalizeSubtitleText(next);
  if (!left) {
    return right;
  }
  if (!right || left.includes(right)) {
    return left;
  }
  if (right.includes(left)) {
    return right;
  }
  const maxOverlap = Math.min(left.length, right.length, 32);
  for (let length = maxOverlap; length >= 2; length -= 1) {
    if (left.slice(-length) === right.slice(0, length)) {
      return `${left}${right.slice(length)}`;
    }
  }
  return `${left} ${right}`;
}

/** 停止实时字幕音频图并释放两个输入流。 */
function stopSubtitleAudioGraph(): void {
  if (subtitleRecorderStopTimerHandle !== null) {
    window.clearTimeout(subtitleRecorderStopTimerHandle);
    subtitleRecorderStopTimerHandle = null;
  }
  if (subtitleNativeChunkTimeoutHandle !== null) {
    window.clearTimeout(subtitleNativeChunkTimeoutHandle);
    subtitleNativeChunkTimeoutHandle = null;
  }
  subtitleNativeChunkInFlight = false;
  if (subtitleMediaRecorder && subtitleMediaRecorder.state !== "inactive") {
    subtitleMediaRecorder.stop();
  }
  subtitleProcessor?.disconnect();
  subtitleProcessor && (subtitleProcessor.onaudioprocess = null);
  subtitleMicSource?.disconnect();
  subtitleSystemSource?.disconnect();
  subtitleMixer?.disconnect();
  subtitleSink?.disconnect();
  subtitleMicStream?.getTracks().forEach((track) => track.stop());
  subtitleSystemStream?.getTracks().forEach((track) => track.stop());
  if (subtitleAudioContext) {
    void subtitleAudioContext.close();
  }
  subtitleMicStream = null;
  subtitleSystemStream = null;
  subtitleAudioContext = null;
  subtitleMicSource = null;
  subtitleSystemSource = null;
  subtitleMixer = null;
  subtitleProcessor = null;
  subtitleSink = null;
  subtitleMediaRecorder = null;
  subtitleRecorderMimeType = "";
  subtitlePendingRecorderChunk = null;
}

/** 向字幕窗口发送失败状态，并复用顶部错误提示做明显反馈。 */
async function showSubtitleFailure(message: string): Promise<void> {
  addDiagnosticLog({
    level: "error",
    category: "subtitle",
    title: "实时字幕不可用",
    message,
    mode: "subtitle",
  });
  await emitSubtitleHistoryUpdate("启动失败", false);
  await emitSubtitleOverlay({ text: "", visible: false, state: "error", updatedAt: Date.now() });
  showLocalErrorBubble(message);
}

/** 显示实时字幕相关窗口。 */
async function showSubtitleWindows(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("show_subtitle_windows");
}

/** 异步显示字幕窗口，窗口异常不阻断已经启动的音频监听。 */
async function showSubtitleWindowsWithDiagnostics(): Promise<void> {
  try {
    await runSubtitleStartupStep(showSubtitleWindows(), "字幕窗口打开", "实时字幕窗口打开长时间无响应。");
  } catch (error) {
    addDiagnosticLog({
      level: "error",
      category: "subtitle",
      title: "字幕窗口打开失败",
      message: formatError(error),
      mode: "subtitle",
    });
  }
}

/** 隐藏实时字幕相关窗口。 */
async function hideSubtitleWindows(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("hide_subtitle_windows");
}

/** 向底部字幕窗口发送当前字幕内容。 */
async function emitSubtitleOverlay(payload: SubtitleOverlayPayload): Promise<void> {
  if (windowMode === "subtitle") {
    renderSubtitleOverlay(payload);
  }
  if (!isTauriRuntime()) {
    return;
  }
  const { emitTo } = await import("@tauri-apps/api/event");
  await emitTo("subtitle", "subtitle-message", payload);
}

/** 通知字幕历史窗口刷新本地历史。 */
async function emitSubtitleHistoryUpdate(status: string, listening: boolean): Promise<void> {
  if (windowMode === "subtitleHistory") {
    renderSubtitleHistory(status, listening);
  }
  if (!isTauriRuntime()) {
    return;
  }
  const { emitTo } = await import("@tauri-apps/api/event");
  await emitTo("subtitleHistory", "subtitle-history-updated", { status, listening });
}

/** 根据字幕来源生成历史窗口可读标签。 */
function formatSubtitleSource(source: SubtitleAudioSource): string {
  if (source === "mixed") {
    return "麦克风+系统声音";
  }
  if (source === "system") {
    return "系统声音";
  }
  return "麦克风";
}

/** 临时切换系统输出静音，并返回切换前状态。 */
async function setSystemOutputMuted(muted: boolean): Promise<boolean | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<boolean>("set_system_output_muted", { muted });
  } catch {
    return null;
  }
}

/** 录音或字幕前按模式检查必要权限，避免快捷键触发后才静默失败。 */
async function ensureReadyForRecording(mode: ShortcutMode): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const diagnostics = await readRuntimeDiagnostics();
    const microphoneDiagnostic = await readMicrophoneDiagnostic();
    if (!diagnostics.hasSessionApiKey && !diagnostics.hasKeychainApiKey && !diagnostics.hasEnvApiKey) {
      await showRequiredModePermission(mode, "apiKey", `请设置${SHORTCUT_MODE_LABELS[mode]}的 Mimo Key。`);
      return false;
    }
    if (!diagnostics.shortcutRegistrationReady) {
      await showRequiredModePermission(mode, "shortcut", `请设置${SHORTCUT_MODE_LABELS[mode]}的快捷键权限。`);
      return false;
    }
    const config = readSavedConfig();
    const isNativeSubtitleMode = mode === "subtitle" && config.systemAudioDeviceId === NATIVE_SYSTEM_AUDIO_DEVICE_ID;
    const needsMicrophone = mode !== "subtitle" || config.subtitleIncludeMicrophone || !isNativeSubtitleMode;
    if (needsMicrophone && microphoneDiagnostic.state !== "success") {
      await showRequiredModePermission(mode, "microphone", `请设置${SHORTCUT_MODE_LABELS[mode]}的麦克风权限。`);
      return false;
    }
    if ((mode === "dictate" || mode === "translate") && !diagnostics.accessibilityTrusted) {
      await showRequiredModePermission(mode, "accessibility", `请设置${SHORTCUT_MODE_LABELS[mode]}的自动粘贴权限。`);
      return false;
    }
    if (mode === "subtitle" && readSystemAudioDiagnostic(config).state !== "success") {
      await showRequiredModePermission(mode, "systemAudio", "请设置实时字幕的系统声音来源。");
      return false;
    }
    return true;
  } catch (error) {
    setStatus(`录音前检查失败：${formatError(error)}`, "error");
    return false;
  }
}

/** 缺少模式权限时展示明确提示，并把 Hub 带到对应模式详情。 */
async function showRequiredModePermission(mode: ShortcutMode, kind: PermissionKind, message: string): Promise<void> {
  if (windowMode === "hub") {
    switchHubView(MODE_DETAIL_VIEWS[mode]);
    openPermissionDialog(mode, kind);
    showHubNotice(message, "error");
    return;
  }
  setStatus(message, "error");
  await showHubWindow();
  await switchHubWindowToModeDetail(mode, kind);
  window.setTimeout(() => void hideFloatingWindow(), 1800);
}

/** Hub 中点击模式启动但权限不足时，打开第一项缺失权限说明。 */
async function showFirstMissingModePermission(mode: ShortcutMode): Promise<void> {
  const missingPermission = readModePermissions(mode).find((permission) => !permission.ready);
  if (!missingPermission) {
    return;
  }
  await showRequiredModePermission(
    mode,
    missingPermission.kind,
    `请设置${SHORTCUT_MODE_LABELS[mode]}的${missingPermission.label}。`,
  );
}

/** 请求 Hub 切换到指定模式详情页，用于快捷键权限缺失时减少用户寻找路径。 */
async function switchHubWindowToModeDetail(mode: ShortcutMode, kind: PermissionKind): Promise<void> {
  if (windowMode === "hub") {
    switchHubView(MODE_DETAIL_VIEWS[mode]);
    openPermissionDialog(mode, kind);
    return;
  }
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("hub", "hub-switch-view", MODE_DETAIL_VIEWS[mode]);
  } catch {
    // Hub 已经打开时，事件失败不影响错误提示本身。
  }
}

/** 录音结束或取消后恢复系统静音状态。 */
async function restoreSystemMute(): Promise<void> {
  if (previousSystemMuteState === null) {
    return;
  }
  const muted = previousSystemMuteState;
  previousSystemMuteState = null;
  await setSystemOutputMuted(muted);
}

/** 播放轻量交互音，辅助判断开始和停止录音状态。 */
function playInteractionSound(kind: "start" | "stop", config: VoiceConfig): void {
  if (!config.interactionSounds) {
    return;
  }
  try {
    const context = new AudioContext();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.type = "sine";
    oscillator.frequency.value = kind === "start" ? 660 : 440;
    gain.gain.value = 0.045;
    oscillator.connect(gain);
    gain.connect(context.destination);
    oscillator.start();
    oscillator.stop(context.currentTime + 0.08);
    window.setTimeout(() => void context.close(), 180);
  } catch {
    // 交互声音失败不影响录音主流程。
  }
}

/** 停止录音并按当前模式调用转写和可选 AI 润色。 */
async function stopRecordingAndTranscribe(): Promise<void> {
  if (!isRecording) {
    return;
  }
  isProcessing = true;
  setFloatingDisabled(true);
  setStatus("正在停止录音并准备转写。", "busy");

  try {
    const audioBlob = stopRecordingToWav();
    stopStream();
    await restoreSystemMute();
    stopTimer();
    const recordElapsedMs = Date.now() - recordStartedAt;
    recordDurationText.textContent = formatDuration(recordElapsedMs);
    audioSizeText.textContent = formatBytes(audioBlob.size);
    addDiagnosticLog({
      level: "info",
      category: "recording",
      title: "停止录音",
      message: "已停止采集并生成待转写音频。",
      mode: activeMode,
      targetApp: recordingTargetApp,
      elapsedMs: recordElapsedMs,
      details: [`音频大小：${formatBytes(audioBlob.size)}`],
    });
    recordButton.title = "开始录音";
    recordButton.setAttribute("aria-label", "开始录音");
    playInteractionSound("stop", readSavedConfig());
    if (recordElapsedMs < MIN_RECORDING_MS) {
      resultMeta.textContent = "录音太短";
      copyButton.disabled = true;
      addDiagnosticLog({
        level: "warning",
        category: "recording",
        title: "录音太短",
        message: "本次录音短于最小时长，已跳过转写。",
        mode: activeMode,
        targetApp: recordingTargetApp,
        elapsedMs: recordElapsedMs,
      });
      setStatus("录音太短了，请说完一句话后再停止。", "error");
      return;
    }
    await transcribeAudio(audioBlob, activeMode, recordElapsedMs);
  } catch (error) {
    await restoreSystemMute();
    addDiagnosticLog({
      level: "error",
      category: isRecording ? "recording" : "system",
      title: "语音链路异常",
      message: formatError(error),
      mode: activeMode,
      targetApp: recordingTargetApp,
    });
    setStatus(formatError(error), "error");
  } finally {
    isProcessing = false;
    setFloatingDisabled(false);
    isRecording = false;
    recordingKeepsHubVisible = false;
  }
}

/** 批量切换悬浮条按钮禁用态。 */
function setFloatingDisabled(disabled: boolean): void {
  recordButton.disabled = disabled;
  soundStage.disabled = disabled;
  cancelButton.disabled = disabled;
}

/** 取消正在录制的音频，或清空上一次识别状态。 */
function cancelRecordingOrReset(): void {
  if (isProcessing) {
    return;
  }
  if (isRecording) {
    isRecording = false;
    stopStream();
    void restoreSystemMute();
    stopTimer();
    recordedSamples = [];
    recordedSampleLength = 0;
    addDiagnosticLog({
      level: "warning",
      category: "recording",
      title: "取消录音",
      message: "用户取消了正在进行的录音，本次不会转写。",
      mode: activeMode,
      targetApp: recordingTargetApp,
    });
    recordButton.title = "开始录音";
    recordButton.setAttribute("aria-label", "开始录音");
    recordDurationText.textContent = "--";
    audioSizeText.textContent = "--";
    resultMeta.textContent = "已取消";
    recordingKeepsHubVisible = false;
    setStatus("已取消录音。", "ready");
    playInteractionSound("stop", readSavedConfig());
    void hideFloatingWindow();
    return;
  }
  resultTextarea.value = "";
  recordingTargetApp = "";
  recordingKeepsHubVisible = false;
  copyButton.disabled = true;
  resultMeta.textContent = "等待录音";
  recordDurationText.textContent = "--";
  transcribeDurationText.textContent = "--";
  processDurationText.textContent = "--";
  audioSizeText.textContent = "--";
  setStatus("按快捷键开始录音。", "ready");
  void hideFloatingWindow();
}

/** 采集麦克风输入的 PCM 样本。 */
function collectAudioSamples(event: AudioProcessingEvent): void {
  if (!isRecording) {
    return;
  }
  const input = event.inputBuffer.getChannelData(0);
  updateVoiceLevelVisual(readAudioLevel(input));
  const sampleCopy = new Float32Array(input.length);
  sampleCopy.set(input);
  recordedSamples.push(sampleCopy);
  recordedSampleLength += sampleCopy.length;
}

/** 从音频采样中估算当前音量等级，驱动悬浮条的实时波形。 */
function readAudioLevel(samples: Float32Array): number {
  let sum = 0;
  let count = 0;
  for (let index = 0; index < samples.length; index += 16) {
    const sample = samples[index];
    sum += sample * sample;
    count += 1;
  }
  if (!count) {
    return 0;
  }
  return Math.min(1, Math.sqrt(sum / count) * 12);
}

/** 按当前麦克风音量更新悬浮条里的九段波形高度。 */
function updateVoiceLevelVisual(level: number): void {
  const normalizedLevel = Math.max(0.08, Math.min(1, level));
  voiceLevelDots.forEach((dot, index) => {
    const factor = VOICE_DOT_FACTORS[index] ?? 0.6;
    dot.style.height = `${(2 + normalizedLevel * 14 * factor).toFixed(1)}px`;
    dot.style.opacity = `${(0.42 + normalizedLevel * 0.58).toFixed(2)}`;
  });
}

/** 录音结束或进入处理态时移除实时波形样式，交回 CSS 状态动画。 */
function resetVoiceLevelVisual(): void {
  voiceLevelDots.forEach((dot) => {
    dot.style.removeProperty("height");
    dot.style.removeProperty("opacity");
  });
}

/** 停止 WebAudio 录音管线并把 PCM 样本编码成 WAV。 */
function stopRecordingToWav(): Blob {
  isRecording = false;
  disconnectAudioNodes();
  if (!recordedSampleLength || !recordedSampleRate) {
    throw new Error("录音内容为空");
  }
  const wavBuffer = encodeWav(mergeSamples(recordedSamples, recordedSampleLength), recordedSampleRate);
  recordedSamples = [];
  recordedSampleLength = 0;
  return new Blob([wavBuffer], { type: "audio/wav" });
}

/** 合并分片采样，生成连续的 Float32 PCM 数据。 */
function mergeSamples(sampleChunks: Float32Array[], totalLength: number): Float32Array {
  const samples = new Float32Array(totalLength);
  let offset = 0;
  for (const chunk of sampleChunks) {
    samples.set(chunk, offset);
    offset += chunk.length;
  }
  return samples;
}

/** 把 Float32 PCM 数据编码为 16-bit PCM WAV 文件。 */
function encodeWav(samples: Float32Array, sampleRate: number): ArrayBuffer {
  const bytesPerSample = 2;
  const channelCount = 1;
  const dataSize = samples.length * bytesPerSample;
  const buffer = new ArrayBuffer(44 + dataSize);
  const view = new DataView(buffer);
  writeAscii(view, 0, "RIFF");
  view.setUint32(4, 36 + dataSize, true);
  writeAscii(view, 8, "WAVE");
  writeAscii(view, 12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, channelCount, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * channelCount * bytesPerSample, true);
  view.setUint16(32, channelCount * bytesPerSample, true);
  view.setUint16(34, 16, true);
  writeAscii(view, 36, "data");
  view.setUint32(40, dataSize, true);
  let offset = 44;
  for (const sample of samples) {
    const clampedSample = Math.max(-1, Math.min(1, sample));
    view.setInt16(offset, clampedSample < 0 ? clampedSample * 0x8000 : clampedSample * 0x7fff, true);
    offset += bytesPerSample;
  }
  return buffer;
}

/** 向 WAV 头写入 ASCII 标记。 */
function writeAscii(view: DataView, offset: number, text: string): void {
  for (let index = 0; index < text.length; index += 1) {
    view.setUint8(offset + index, text.charCodeAt(index));
  }
}

/** 调用当前运行环境的转写能力。 */
async function transcribeAudio(audioBlob: Blob, mode: VoiceMode, recordElapsedMs: number): Promise<void> {
  const config = readSavedConfig();
  setStatus("正在上传音频并转写。", "busy");
  transcribeDurationText.textContent = "--";
  processDurationText.textContent = "--";
  resultMeta.textContent = "转写中";

  addDiagnosticLog({
    level: "info",
    category: "transcribe",
    title: "开始转写",
    message: "正在把本次音频发送给 ASR 接口。",
    mode,
    targetApp: recordingTargetApp,
    details: [`模型：${config.asrModel}`, `语言：${config.language}`, `音频大小：${formatBytes(audioBlob.size)}`],
  });

  let response: TranscribeResponse;
  try {
    response = await callTranscribe({
      apiKey: "",
      baseUrl: config.baseUrl,
      asrModel: config.asrModel,
      language: config.language,
      contentType: "audio/wav",
      audioBase64: await blobToBase64(audioBlob),
    });
  } catch (error) {
    addDiagnosticLog({
      level: "error",
      category: "transcribe",
      title: "转写失败",
      message: formatError(error),
      mode,
      targetApp: recordingTargetApp,
    });
    throw error;
  }

  const sourceText = response.text.trim();
  transcribeDurationText.textContent = formatDuration(response.elapsedMs);
  const hasMeaningfulTranscription = isMeaningfulTranscription(sourceText);
  addDiagnosticLog({
    level: hasMeaningfulTranscription ? "success" : "warning",
    category: "transcribe",
    title: hasMeaningfulTranscription ? "转写完成" : "转写返回空内容",
    message: hasMeaningfulTranscription ? "ASR 已返回可处理文本。" : "ASR 请求已返回，但内容为空或属于占位文案。",
    mode,
    targetApp: recordingTargetApp,
    elapsedMs: response.elapsedMs,
    details: [`模型：${response.model}`, `原文字数：${countTextUnits(sourceText)}`],
  });
  if (!hasMeaningfulTranscription) {
    resultMeta.textContent = "没有识别到语音";
    copyButton.disabled = true;
    addDiagnosticLog({
      level: "warning",
      category: "transcribe",
      title: "没有识别到有效语音",
      message: "ASR 返回内容为空或属于上游空内容占位。",
      mode,
      targetApp: recordingTargetApp,
      elapsedMs: response.elapsedMs,
    });
    setStatus("没有识别到有效语音，请靠近麦克风后再试。", "error");
    return;
  }

  const contextApp = recordingTargetApp || (await readFrontmostApp());
  const shouldProcess = mode !== "dictate" || config.postProcessDictation;
  let usedSourceFallback = false;
  let sourceFallbackReason = "";
  let processed: { text: string; elapsedMs: number; model: string };
  if (shouldProcess) {
    try {
      addDiagnosticLog({
        level: "info",
        category: "process",
        title: mode === "dictate" ? "开始 AI 润色" : `开始${MODE_LABELS[mode]}处理`,
        message: "正在把 ASR 文本发送给 AI 文本处理接口。",
        mode,
        targetApp: contextApp,
        details: [`模型：${config.textModel}`, `原文字数：${countTextUnits(sourceText)}`],
      });
      processed = await processRecognizedText(sourceText, mode, config, contextApp);
      if (mode === "dictate" && !processed.text.trim()) {
        usedSourceFallback = true;
        sourceFallbackReason = "AI 润色返回为空";
        processed = { text: sourceText, elapsedMs: 0, model: response.model };
      }
    } catch (error) {
      const processError = formatError(error);
      addDiagnosticLog({
        level: mode === "dictate" ? "warning" : "error",
        category: "process",
        title: mode === "dictate" ? "AI 润色失败，回退原文" : `${MODE_LABELS[mode]}处理失败`,
        message: processError,
        mode,
        targetApp: contextApp,
      });
      if (mode !== "dictate") {
        throw error;
      }
      usedSourceFallback = true;
      sourceFallbackReason = processError;
      processed = { text: sourceText, elapsedMs: 0, model: response.model };
    }
  } else {
    addDiagnosticLog({
      level: "info",
      category: "process",
      title: "跳过 AI 润色",
      message: "口述 AI 润色开关已关闭，本次直接使用 ASR 原文。",
      mode,
      targetApp: contextApp,
    });
    processed = { text: sourceText, elapsedMs: 0, model: response.model };
  }

  const outputText = processed.text.trim();
  addDiagnosticLog({
    level: usedSourceFallback ? "warning" : "success",
    category: "process",
    title: usedSourceFallback ? "使用原始转写结果" : "文本处理完成",
    message: usedSourceFallback ? formatAiFallbackMessage(sourceFallbackReason) : "最终输出已准备好进入粘贴流程。",
    mode,
    targetApp: contextApp,
    elapsedMs: processed.elapsedMs,
    details: [`模型：${processed.model || response.model}`, `输出字数：${countTextUnits(outputText)}`],
  });
  resultTextarea.value = outputText;
  resultMeta.textContent = `${MODE_LABELS[mode]}完成`;
  processDurationText.textContent = processed.elapsedMs ? formatDuration(processed.elapsedMs) : "未润色";
  copyButton.disabled = !outputText;

  const historyItem = saveHistory({
    id: createId(),
    mode,
    sourceText,
    outputText,
    createdAt: Date.now(),
    recordElapsedMs,
    transcribeElapsedMs: response.elapsedMs,
    processElapsedMs: processed.elapsedMs,
    model: processed.model || response.model,
    contextApp,
  });
  updateRecentResult(historyItem);

  if (mode === "ask") {
    setStatus("答案已生成，已在 Hub 展示。", "ready");
    await showHubWindow();
    await hideFloatingWindow();
    return;
  }

  if (usedSourceFallback) {
    setStatus(formatAiFallbackMessage(sourceFallbackReason), "error");
  }
  if (recordingKeepsHubVisible) {
    addDiagnosticLog({
      level: "info",
      category: "paste",
      title: "跳过自动粘贴",
      message: "本次录音来自 Hub 主界面，已保留 Hub 显示并把结果写入最近结果和历史记录。",
      mode,
      details: [`输出字数：${countTextUnits(outputText)}`],
    });
    setStatus("处理完成，结果已更新到最近结果和历史记录。", "ready");
    await hideFloatingWindow();
    return;
  }
  await pasteTranscription(outputText, contextApp);
}

/** 把 AI 润色回退原因整理成用户能判断的提示，避免把超时误解成转写失败。 */
function formatAiFallbackMessage(reason: string): string {
  const normalizedReason = reason.trim();
  if (isAiTimeoutReason(normalizedReason)) {
    return "AI 润色超时，本次已先使用 ASR 原文。";
  }
  if (normalizedReason) {
    return `AI 润色未完成，本次已先使用 ASR 原文。原因：${normalizedReason}`;
  }
  return "AI 润色未产出可用内容，本次已先使用 ASR 原文。";
}

/** 判断 Mimo 文本处理错误是否属于等待超时，便于诊断日志展示真实原因。 */
function isAiTimeoutReason(reason: string): boolean {
  const normalizedReason = reason.toLowerCase();
  return (
    normalizedReason.includes("timeout") ||
    normalizedReason.includes("timed out") ||
    normalizedReason.includes("deadline") ||
    normalizedReason.includes("超时")
  );
}

/** 判断 ASR 返回内容是否是可交付文本，过滤上游的空内容占位文案。 */
function isMeaningfulTranscription(text: string): boolean {
  const normalizedText = text.replace(/[\s"'“”‘’（）()【】[\]{}<>《》,.，。!！?？:：;；\-—_、]/g, "").toLowerCase();
  if (!normalizedText) {
    return false;
  }
  return !EMPTY_TRANSCRIPTION_MARKERS.some((marker) => normalizedText === marker.toLowerCase());
}

/** 调用 Tauri 或网页预览模式的转写接口。 */
async function callTranscribe(request: TranscribeRequest): Promise<TranscribeResponse> {
  if (isTauriRuntime()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<TranscribeResponse>("transcribe_audio", { request });
  }
  const response = await fetch("/api/transcribe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
  const payload = (await response.json()) as Partial<TranscribeResponse> & { error?: string };
  if (!response.ok) {
    throw new Error(payload.error || `转写失败：HTTP ${response.status}`);
  }
  return {
    text: typeof payload.text === "string" ? payload.text : "",
    elapsedMs: typeof payload.elapsedMs === "number" ? payload.elapsedMs : 0,
    model: typeof payload.model === "string" ? payload.model : request.asrModel,
  };
}

/** 调用 Tauri 原生 Core Audio Process Tap helper 采集一段系统音频。 */
async function captureProcessTapAudio(durationMs: number, targetKeyword: string): Promise<ProcessTapCaptureResponse> {
  if (!isTauriRuntime()) {
    throw new Error("网页预览模式不支持原生系统音频捕获。");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProcessTapCaptureResponse>("capture_process_tap_audio", {
    request: {
      durationMs,
      targetKeyword,
    },
  });
}

/** 调用 Tauri 原生命令采集系统音频并在 Rust 侧直接完成 ASR。 */
async function captureProcessTapTranscribe(
  durationMs: number,
  targetKeyword: string,
  chunkIndex: number,
): Promise<ProcessTapTranscribeResponse> {
  if (!isTauriRuntime()) {
    throw new Error("网页预览模式不支持原生系统音频捕获。");
  }
  const config = readSavedConfig();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProcessTapTranscribeResponse>("capture_process_tap_transcribe", {
    request: {
      chunkIndex,
      durationMs,
      targetKeyword,
      apiKey: "",
      baseUrl: config.baseUrl,
      asrModel: config.asrModel,
      language: config.language,
    },
  });
}

/** 启动 Tauri 原生后台任务：采集系统音频并在 Rust 侧完成 ASR。 */
async function startProcessTapTranscribeTask(durationMs: number, targetKeyword: string, chunkIndex: number): Promise<void> {
  if (!isTauriRuntime()) {
    throw new Error("网页预览模式不支持原生系统音频捕获。");
  }
  const config = readSavedConfig();
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("start_process_tap_transcribe_task", {
    request: {
      chunkIndex,
      durationMs,
      targetKeyword,
      apiKey: "",
      baseUrl: config.baseUrl,
      asrModel: config.asrModel,
      language: config.language,
    },
  });
}

/** 轮询消费 Tauri 原生后台字幕任务结果。 */
async function takeProcessTapTranscribeOutcome(chunkIndex: number): Promise<ProcessTapTranscribeOutcome | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProcessTapTranscribeOutcome | null>("take_process_tap_transcribe_outcome", { chunkIndex });
}

/** 对识别结果执行口述润色、翻译或问答。 */
async function processRecognizedText(
  text: string,
  mode: VoiceMode,
  config: VoiceConfig,
  contextApp = "",
): Promise<{ text: string; elapsedMs: number; model: string }> {
  if (!isTauriRuntime()) {
    return { text, elapsedMs: 0, model: config.textModel };
  }
  setStatus(mode === "dictate" || mode === "polish" ? "正在 AI 润色。" : `正在执行${MODE_LABELS[mode]}处理。`, "busy");
  const { invoke } = await import("@tauri-apps/api/core");
  const request: ProcessTextRequest = {
    apiKey: "",
    baseUrl: config.baseUrl,
    textModel: config.textModel,
    mode,
    text,
    dictionary: readDictionary(),
    targetLanguages: config.targetLanguages,
    contextApp,
    styleInstruction: buildStyleInstruction(config, mode),
  };
  const response = await invoke<ProcessTextResponse>("process_text", { request });
  return {
    text: response.processedText,
    elapsedMs: response.elapsedMs,
    model: response.model,
  };
}

/** 读取用户开始录音前所在的前台 App，作为个性化上下文。 */
async function readFrontmostApp(): Promise<string> {
  if (!isTauriRuntime()) {
    return "";
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("get_frontmost_app");
  } catch {
    return "";
  }
}

/** 清理 typesass 自身窗口名，避免把悬浮窗当成最终粘贴目标。 */
function normalizeRecordingTargetApp(appName: string): string {
  const normalizedAppName = appName.trim();
  if (!normalizedAppName || normalizedAppName === "typesass" || normalizedAppName === "AiTool" || normalizedAppName === "ai-tool") {
    return "";
  }
  return normalizedAppName;
}

/** 生成本地个性化提示，先拼通用偏好，再追加当前模式自己的偏好。 */
function buildStyleInstruction(config: VoiceConfig, mode: VoiceMode): string {
  const pieces: string[] = [];
  if (config.personalStyle.trim()) {
    pieces.push(`通用偏好：${config.personalStyle.trim()}。`);
  }
  const outputLanguageInstruction = readDictationOutputLanguageInstruction(config, mode);
  if (outputLanguageInstruction) {
    pieces.push(outputLanguageInstruction);
  }
  const modeStyle = readModeStyle(config, mode);
  if (modeStyle) {
    pieces.push(`${MODE_LABELS[mode]}偏好：${modeStyle}。`);
  }
  return pieces.join("\n");
}

/** 读取口述 AI 润色的输出语言要求，关闭润色或跟随原文时不追加。 */
function readDictationOutputLanguageInstruction(config: VoiceConfig, mode: VoiceMode): string {
  if (mode !== "dictate" || !config.postProcessDictation) {
    return "";
  }
  const language = config.dictationOutputLanguage.trim();
  if (!language || language === DEFAULT_DICTATION_OUTPUT_LANGUAGE) {
    return "";
  }
  return `口述输出语言：请使用${language}输出，保持原意，不新增事实。`;
}

/** 根据当前 AI 处理模式读取对应的局部输出偏好。 */
function readModeStyle(config: VoiceConfig, mode: VoiceMode): string {
  if (mode === "translate") {
    return config.translationStyle.trim();
  }
  if (mode === "ask") {
    return config.askStyle.trim();
  }
  if (mode === "polish") {
    return config.polishStyle.trim();
  }
  return config.dictationStyle.trim();
}

/** 转写完成后把结果自动粘贴到当前焦点输入框。 */
async function pasteTranscription(text: string, targetApp = ""): Promise<void> {
  if (!isTauriRuntime()) {
    addDiagnosticLog({
      level: "warning",
      category: "paste",
      title: "跳过自动粘贴",
      message: "网页预览模式无法触发系统级粘贴。",
      targetApp,
    });
    setStatus("转写完成，可以复制结果。", "ready");
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    addDiagnosticLog({
      level: "info",
      category: "paste",
      title: "开始自动粘贴",
      message: "正在临时写入剪贴板，请求系统粘贴后会恢复原剪贴板。",
      targetApp,
      details: [`输出字数：${countTextUnits(text)}`],
    });
    const response = await invoke<PasteResponse>("paste_text", { text, targetApp });
    addDiagnosticLog({
      level: response.pasted ? "info" : "warning",
      category: "paste",
      title: response.pasted ? "粘贴指令已发送" : "自动粘贴未完成",
      message: response.message,
      targetApp: response.targetApp,
      frontmostApp: response.frontmostAfterPaste || response.frontmostAfterActivate || response.frontmostBeforePaste,
      pasteMethod: response.pasteMethod,
      accessibilityTrusted: response.accessibilityTrusted,
      clipboardWritten: response.clipboardWritten,
      clipboardMatchesExpected: response.clipboardMatchesExpected,
      clipboardRestoreAttempted: response.clipboardRestoreAttempted,
      clipboardRestored: response.clipboardRestored,
      clipboardRestoreMessage: response.clipboardRestoreMessage,
      insertionVerified: response.insertionVerified,
      verificationStatus: response.verificationStatus,
      focusedElementBeforePaste: response.focusedElementBeforePaste,
      focusedElementAfterActivate: response.focusedElementAfterActivate,
      focusedElementAfterPaste: response.focusedElementAfterPaste,
      details: [
        `发送前：${response.frontmostBeforePaste || "未知"}`,
        "前台切换：未执行",
        `发送后：${response.frontmostAfterPaste || "未知"}`,
      ],
    });
    if (response.pasted) {
      setStatus(response.message, "ready");
      return;
    }
    const fallbackMessage = formatManualResultFallbackMessage(response.message, response.requiresAccessibility);
    setStatus(fallbackMessage, response.requiresAccessibility ? "error" : "ready");
    await showResultWindow(text, fallbackMessage, response.requiresAccessibility);
  } catch (error) {
    const message = `${formatError(error)}。结果已保留，可手动复制。`;
    addDiagnosticLog({
      level: "error",
      category: "paste",
      title: "自动粘贴异常",
      message,
      targetApp,
      details: [`输出字数：${countTextUnits(text)}`],
    });
    setStatus(message, "error");
    await showResultWindow(text, message, false);
  }
}

/** 把自动粘贴未完成的原因转成更像兜底结果的用户提示。 */
function formatManualResultFallbackMessage(message: string, requiresAccessibility: boolean): string {
  const normalizedMessage = message.trim();
  if (requiresAccessibility) {
    return normalizedMessage || "辅助功能未授权，结果已展示，可手动复制。";
  }
  if (normalizedMessage.includes("没有可恢复的目标输入框") || normalizedMessage.includes("当前焦点不在外部输入目标")) {
    return "没有检测到可粘贴的外部输入框，结果已展示，可手动复制。";
  }
  return normalizedMessage || "自动粘贴未完成，结果已展示，可手动复制。";
}

/** 在独立结果窗口展示无法自动粘贴的内容。 */
async function showResultWindow(text: string, reason: string, requiresAccessibility: boolean): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("show_result_window", { text, reason, requiresAccessibility });
  } catch (error) {
    setStatus(`结果窗口打开失败：${formatError(error)}`, "error");
  }
}

/** 在 Tauri 桌面端隐藏胶囊悬浮条。 */
async function hideFloatingWindow(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("hide_main_window");
  } catch {
    // 隐藏失败不影响录音和转写主流程。
  }
}

/** 在 Tauri 桌面端打开 Hub。 */
async function showHubWindow(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("show_hub_window");
  } catch {
    // 打开 Hub 失败时，历史仍然保存在本地。
  }
}

/** 根据 Rust 传来的真实结果刷新结果兜底窗口。 */
function renderResultWindow(payload: ResultWindowPayload): void {
  if (windowMode !== "result") {
    return;
  }
  const text = payload.text.trim();
  resultWindowTextarea.value = text;
  resultReason.textContent = payload.reason || "自动粘贴没有完成，结果未覆盖原剪贴板。";
  resultReason.dataset.state = payload.requiresAccessibility ? "warning" : "idle";
  resultCopyButton.disabled = !text;
  resultOpenAccessibilityButton.disabled = false;
  resultOpenAccessibilityButton.textContent = "打开辅助功能设置";
  resultOpenAccessibilityButton.hidden = !payload.requiresAccessibility;
  resultShell.dataset.ready = text ? "true" : "false";
  window.setTimeout(() => {
    resultWindowTextarea.focus();
    resultWindowTextarea.select();
  }, 40);
}

/** 复制结果窗口中的最终文字，并在本窗口直接反馈。 */
async function copyResultWindowText(): Promise<void> {
  const normalizedText = resultWindowTextarea.value.trim();
  if (!normalizedText) {
    resultReason.textContent = "没有可复制内容。";
    resultReason.dataset.state = "error";
    return;
  }
  try {
    await navigator.clipboard.writeText(normalizedText);
  } catch {
    resultWindowTextarea.select();
    document.execCommand("copy");
  }
  resultReason.textContent = "结果已复制到剪贴板。";
  resultReason.dataset.state = "success";
  setResultCopyButtonFeedback("已复制");
}

/** 复制成功后短暂更新结果窗口主按钮文案，给用户明确反馈。 */
function setResultCopyButtonFeedback(label: string): void {
  const labelElement = resultCopyButton.querySelector<HTMLElement>("span:last-child");
  if (!labelElement) {
    return;
  }
  if (resultCopyFeedbackTimer !== null) {
    window.clearTimeout(resultCopyFeedbackTimer);
  }
  labelElement.textContent = label;
  resultCopyFeedbackTimer = window.setTimeout(() => {
    labelElement.textContent = "复制结果";
    resultCopyFeedbackTimer = null;
  }, 1200);
}

/** 从结果窗口打开 macOS 辅助功能设置。 */
async function openAccessibilityFromResult(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_accessibility_settings");
    resultReason.textContent = "已打开辅助功能设置，勾选 typesass 后会自动检测授权状态。";
    resultReason.dataset.state = "success";
    startAccessibilityGrantWatch("result");
  } catch (error) {
    resultReason.textContent = `打开辅助功能设置失败：${formatError(error)}`;
    resultReason.dataset.state = "error";
  }
}

/** 关闭结果窗口，应用继续在后台等待下一次快捷键。 */
async function hideResultWindow(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("hide_result_window");
  } catch (error) {
    resultReason.textContent = `关闭窗口失败：${formatError(error)}`;
    resultReason.dataset.state = "error";
  }
}

/** 把 Blob 转成 base64 字符串。 */
async function blobToBase64(blob: Blob): Promise<string> {
  const buffer = await blob.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  const chunkSize = 8192;
  let binary = "";
  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.subarray(index, index + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
}

/** 把原生侧返回的 base64 音频还原为浏览器 Blob，继续复用实时字幕 ASR 队列。 */
function base64ToBlob(value: string, contentType: string): Blob {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return new Blob([bytes], { type: contentType });
}

/** 切换 Hub 当前视图。 */
function switchHubView(view: string): void {
  const title = VIEW_TITLES[view] || VIEW_TITLES.home;
  const navView = readNavViewForHubView(view);
  hubTitle.textContent = title.title;
  hubEyebrow.textContent = title.eyebrow;
  resetHubNotice();
  document.querySelectorAll<HTMLElement>("[data-view]").forEach((element) => {
    element.classList.toggle("isActive", element.dataset.view === view);
  });
  document.querySelectorAll<HTMLButtonElement>("[data-view-target]").forEach((button) => {
    button.classList.toggle("isActive", button.dataset.viewTarget === navView);
  });
  renderHub();
}

/** 子设置页仍归属语音模式导航，避免侧边栏出现不存在的当前模式状态。 */
function readNavViewForHubView(view: string): string {
  if (Object.values(MODE_DETAIL_VIEWS).includes(view)) {
    return "modes";
  }
  return view;
}

/** 更新带 IconPark 图标按钮的文字，避免重绘时丢失图标。 */
function updateActionButtonLabel(button: HTMLButtonElement, label: string): void {
  const labelElement =
    button.querySelector<HTMLElement>("[data-action-label]") ||
    button.querySelector<HTMLElement>(".buttonIcon + span");
  if (labelElement) {
    labelElement.textContent = label;
    return;
  }
  button.textContent = label;
}

/** 更新带 IconPark 图标按钮的图标。 */
function updateActionButtonIcon(button: HTMLButtonElement, iconName: IconName): void {
  const iconElement = button.querySelector<HTMLElement>(".buttonIcon");
  if (!iconElement) {
    return;
  }
  renderIcon(iconElement, iconName);
}

/** 根据准备状态同步所有开始按钮，避免未就绪时误唤起悬浮条再报错。 */
function syncStartActionButtons(): void {
  const isDictateReady = isModePermissionReady("dictate");
  const quickLabel = isDictateReady ? MODE_START_LABELS.dictate : "继续配置";
  if (quickStartButton) {
    updateActionButtonLabel(quickStartButton, quickLabel);
    updateActionButtonIcon(quickStartButton, isDictateReady ? "play" : "setting");
    quickStartButton.title = isDictateReady ? quickLabel : "继续完成口述需要的权限";
  }
  document.querySelectorAll<HTMLButtonElement>("[data-mode-start]").forEach((button) => {
    const mode = normalizeMode(button.dataset.modeStart);
    const isReady = isModePermissionReady(mode);
    updateActionButtonLabel(button, isReady ? MODE_START_LABELS[mode] : "继续配置");
    updateActionButtonIcon(button, isReady ? MODE_ACTION_ICONS[mode] : "setting");
    button.title = isReady ? MODE_START_LABELS[mode] : `继续完成${MODE_LABELS[mode]}需要的权限`;
  });
  document.querySelectorAll<HTMLButtonElement>("[data-subtitle-toggle]").forEach((button) => {
    const isReady = isModePermissionReady("subtitle");
    updateActionButtonLabel(button, isReady ? "开启字幕" : "继续配置");
    updateActionButtonIcon(button, isReady ? "microphone" : "setting");
    button.title = isReady ? "开启实时字幕" : "继续完成实时字幕需要的权限";
  });
}

/** 判断某个模式自己的权限是否满足启动条件。 */
function isModePermissionReady(mode: ShortcutMode): boolean {
  return readModePermissions(mode).every((permission) => permission.ready);
}

/** 渲染 Hub 中所有本地数据。 */
function renderHub(): void {
  if (windowMode !== "hub") {
    return;
  }
  syncStartActionButtons();
  renderStats();
  renderHistory();
  renderDictionary();
  renderDiagnosticLog();
  renderModePermissions();
  const latest = readHistory()[0];
  updateRecentResult(latest || null);
}

/** 渲染统计卡片。 */
function renderStats(): void {
  const history = readHistory();
  const todayHistory = history.filter((item) => isSameLocalDate(item.createdAt, Date.now()));
  const words = history.reduce((total, item) => total + countTextUnits(item.outputText), 0);
  const todayWords = todayHistory.reduce((total, item) => total + countTextUnits(item.outputText), 0);
  const durationMs = todayHistory.reduce((total, item) => total + item.recordElapsedMs, 0);
  metricSessions.textContent = String(todayHistory.length);
  metricWords.textContent = formatCompactNumber(todayWords);
  usageWords.textContent = `${history.length} 条 · ${formatCompactNumber(words)} 字`;
  usageTrackFill.style.width = words > 0 ? `${Math.min(100, Math.round((todayWords / words) * 100))}%` : "0%";
  metricSpeed.textContent = durationMs > 0 ? `${Math.round(todayWords / (durationMs / 60000))}/分钟` : "--";
  syncDictationPolishSwitches(readSavedConfig().postProcessDictation);
  metricPersonalization.textContent = `${readDictionary().length} 词条${hasOutputPreference(readSavedConfig()) ? " + 偏好" : ""}`;
}

/** 判断用户是否配置了任一通用或模式级输出偏好。 */
function hasOutputPreference(config: VoiceConfig): boolean {
  return Boolean(
    config.personalStyle.trim() ||
      config.dictationStyle.trim() ||
      config.translationStyle.trim() ||
      config.askStyle.trim(),
  );
}

/** 渲染历史列表。 */
function renderHistory(): void {
  if (windowMode !== "hub") {
    return;
  }
  document.querySelectorAll<HTMLButtonElement>("[data-history-filter]").forEach((button) => {
    button.classList.toggle("isActive", button.dataset.historyFilter === historyFilter);
  });
  const history = readHistory().filter((item) => historyFilter === "all" || item.mode === historyFilter);
  clearHistoryButton.disabled = readHistory().length === 0;
  if (!history.length) {
    historyList.innerHTML = '<div class="emptyState">还没有历史记录。</div>';
    return;
  }
  historyList.innerHTML = history.map(renderHistoryItem).join("");
}

/** 把单条历史记录渲染为列表项。 */
function renderHistoryItem(item: HistoryItem): string {
  const context = item.contextApp ? `<span class="historyContext">${escapeHtml(item.contextApp)}</span>` : "";
  const sourceDetail =
    item.sourceText.trim() && item.sourceText.trim() !== item.outputText.trim()
      ? `<details class="historySourceDisclosure"><summary>查看原文</summary><p>${escapeHtml(item.sourceText)}</p></details>`
      : "";
  return `
    <article class="historyItem">
      <div class="historyMeta">
        <span>${MODE_LABELS[item.mode]}</span>
        <time>${formatDateTime(item.createdAt)}</time>
      </div>
      <div class="historyTimingGrid">${formatHistoryTimingChips(item)}</div>
      ${context}
      <p>${escapeHtml(item.outputText)}</p>
      ${sourceDetail}
      <div class="rowActions">
        <button type="button" data-history-action="copy" data-history-id="${item.id}">复制</button>
        <button type="button" data-history-action="retry" data-history-id="${item.id}">重试</button>
        <button type="button" data-history-action="delete" data-history-id="${item.id}">删除</button>
      </div>
    </article>`;
}

/** 处理历史列表里的复制、重试和删除。 */
function handleHistoryAction(event: MouseEvent): void {
  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return;
  }
  const button = target.closest<HTMLButtonElement>("[data-history-action]");
  if (!button) {
    return;
  }
  const id = button.dataset.historyId || "";
  const action = button.dataset.historyAction || "";
  const item = readHistory().find((historyItem) => historyItem.id === id);
  if (!item) {
    return;
  }
  if (action === "copy") {
    void copyText(item.outputText);
  } else if (action === "delete") {
    if (!confirmDangerousAction(`deleteHistory:${id}`, button, "再次点击删除", "再次点击将删除这条历史记录。")) {
      return;
    }
    writeHistory(readHistory().filter((historyItem) => historyItem.id !== id));
    resetPendingConfirmation();
    renderHub();
  } else if (action === "retry") {
    void reprocessHistoryItem(item);
  }
}

/** 用现有 ASR 原文重新执行 AI 处理。 */
async function reprocessHistoryItem(item: HistoryItem): Promise<void> {
  try {
    const config = readSavedConfig();
    const processed = await processRecognizedText(item.sourceText, item.mode, config, item.contextApp);
    const nextItem: HistoryItem = {
      ...item,
      id: createId(),
      outputText: processed.text,
      processElapsedMs: processed.elapsedMs,
      model: processed.model,
      createdAt: Date.now(),
    };
    saveHistory(nextItem);
    renderHub();
    showHubNotice("已基于原文重新整理。", "success");
  } catch (error) {
    showHubNotice(`重新整理失败：${formatError(error)}`, "error");
  }
}

/** 重试最近一条历史记录。 */
async function retryLatestHistory(): Promise<void> {
  const latest = readHistory()[0];
  if (latest) {
    await reprocessHistoryItem(latest);
    return;
  }
  showHubNotice("没有可重新整理的历史记录。", "error");
}

/** 清空历史记录，执行前要求二次点击确认。 */
function clearHistory(button: HTMLButtonElement): void {
  if (!readHistory().length) {
    showHubNotice("当前没有历史记录。", "idle");
    return;
  }
  if (!confirmDangerousAction("clearHistory", button, "再次点击清空", "再次点击将清空全部历史记录。")) {
    return;
  }
  localStorage.removeItem(HISTORY_STORAGE_KEY);
  resetPendingConfirmation();
  renderHub();
  showHubNotice("历史记录已清空。", "success");
}

/** 渲染本机诊断日志，帮助定位快捷键、转写和自动粘贴链路问题。 */
function renderDiagnosticLog(): void {
  if (windowMode !== "hub") {
    return;
  }
  const logs = readDiagnosticLogs();
  diagnosticLogCount.textContent = `${logs.length} 条`;
  copyDiagnosticLogButton.disabled = logs.length === 0;
  clearDiagnosticLogButton.disabled = logs.length === 0;
  if (!logs.length) {
    diagnosticLogList.innerHTML = '<div class="emptyState">还没有诊断日志。</div>';
    return;
  }
  diagnosticLogList.innerHTML = logs.map(renderDiagnosticLogItem).join("");
}

/** 把单条诊断日志渲染为不暴露转写正文的列表项。 */
function renderDiagnosticLogItem(item: DiagnosticLogItem): string {
  const details = buildDiagnosticLogDetails(item);
  return `
    <article class="diagnosticLogItem" data-level="${item.level}">
      <div class="historyMeta">
        <span>${DIAGNOSTIC_LOG_CATEGORY_LABELS[item.category]} · ${DIAGNOSTIC_LOG_LEVEL_LABELS[item.level]}</span>
        <time>${formatDateTime(item.createdAt)}</time>
      </div>
      <h2>${escapeHtml(item.title)}</h2>
      <p>${escapeHtml(item.message)}</p>
      ${details.length ? `<div class="diagnosticLogDetails">${details.map((detail) => `<span>${escapeHtml(detail)}</span>`).join("")}</div>` : ""}
    </article>`;
}

/** 组装诊断日志的可视化标签，便于扫描焦点和粘贴路径。 */
function buildDiagnosticLogDetails(item: DiagnosticLogItem): string[] {
  const details: string[] = [];
  if (item.mode) {
    details.push(`模式：${SHORTCUT_MODE_LABELS[item.mode]}`);
  }
  if (item.targetApp) {
    details.push(`目标：${item.targetApp}`);
  }
  if (item.frontmostApp) {
    details.push(`前台：${item.frontmostApp}`);
  }
  if (item.pasteMethod) {
    details.push(`方式：${item.pasteMethod}`);
  }
  if (typeof item.accessibilityTrusted === "boolean") {
    details.push(`辅助功能：${item.accessibilityTrusted ? "已授权" : "未授权"}`);
  }
  if (typeof item.clipboardWritten === "boolean") {
    details.push(`临时剪贴板：${item.clipboardWritten ? "已写入" : "未写入"}`);
  }
  if (typeof item.clipboardMatchesExpected === "boolean") {
    details.push(`临时剪贴板校验：${item.clipboardMatchesExpected ? "一致" : "不一致"}`);
  }
  if (typeof item.clipboardRestoreAttempted === "boolean") {
    details.push(`原剪贴板恢复：${item.clipboardRestored ? "已恢复" : item.clipboardRestoreAttempted ? "恢复失败" : "未改动"}`);
  }
  if (item.clipboardRestoreMessage) {
    details.push(`恢复说明：${item.clipboardRestoreMessage}`);
  }
  if (typeof item.insertionVerified === "boolean") {
    details.push(`插入校验：${item.insertionVerified ? "已确认" : "快速模式未回读"}`);
  }
  if (item.verificationStatus) {
    details.push(`校验说明：${item.verificationStatus}`);
  }
  if (item.focusedElementBeforePaste) {
    details.push(`焦点发送前：${item.focusedElementBeforePaste}`);
  }
  if (item.focusedElementAfterActivate) {
    details.push(`焦点发送方式：${item.focusedElementAfterActivate}`);
  }
  if (item.focusedElementAfterPaste) {
    details.push(`焦点发送后：${item.focusedElementAfterPaste}`);
  }
  if (typeof item.elapsedMs === "number" && item.elapsedMs > 0) {
    details.push(`耗时：${formatDuration(item.elapsedMs)}`);
  }
  return [...details, ...item.details];
}

/** 新增本机诊断日志；只记录元信息，不保存用户转写正文。 */
function addDiagnosticLog(draft: DiagnosticLogDraft): void {
  const item: DiagnosticLogItem = {
    ...draft,
    id: createId(),
    createdAt: Date.now(),
    message: draft.message.trim() || draft.title,
    targetApp: normalizeOptionalText(draft.targetApp),
    frontmostApp: normalizeOptionalText(draft.frontmostApp),
    pasteMethod: normalizeOptionalText(draft.pasteMethod),
    focusedElementBeforePaste: normalizeOptionalText(draft.focusedElementBeforePaste),
    focusedElementAfterActivate: normalizeOptionalText(draft.focusedElementAfterActivate),
    focusedElementAfterPaste: normalizeOptionalText(draft.focusedElementAfterPaste),
    verificationStatus: normalizeOptionalText(draft.verificationStatus),
    details: (draft.details || []).map((detail) => detail.trim()).filter(Boolean),
  };
  const logs = [item, ...readDiagnosticLogs()].slice(0, MAX_DIAGNOSTIC_LOG_ITEMS);
  writeDiagnosticLogs(logs);
  renderDiagnosticLog();
}

/** 读取本机诊断日志，异常时返回空数组以免影响主流程。 */
function readDiagnosticLogs(): DiagnosticLogItem[] {
  const raw = localStorage.getItem(DIAGNOSTIC_LOG_STORAGE_KEY);
  if (!raw) {
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as Partial<DiagnosticLogItem>[];
    return parsed
      .map(normalizeDiagnosticLogItem)
      .filter((item): item is DiagnosticLogItem => Boolean(item))
      .sort((left, right) => right.createdAt - left.createdAt);
  } catch {
    return [];
  }
}

/** 对本地日志记录做类型兜底，兼容未来字段扩展。 */
function normalizeDiagnosticLogItem(value: Partial<DiagnosticLogItem>): DiagnosticLogItem | null {
  if (typeof value.id !== "string" || typeof value.title !== "string" || typeof value.message !== "string") {
    return null;
  }
  return {
    id: value.id,
    createdAt: typeof value.createdAt === "number" ? value.createdAt : Date.now(),
    level: normalizeDiagnosticLogLevel(value.level),
    category: normalizeDiagnosticLogCategory(value.category),
    title: value.title,
    message: value.message,
    mode: normalizeOptionalMode(value.mode),
    targetApp: normalizeOptionalText(value.targetApp),
    frontmostApp: normalizeOptionalText(value.frontmostApp),
    pasteMethod: normalizeOptionalText(value.pasteMethod),
    focusedElementBeforePaste: normalizeOptionalText(value.focusedElementBeforePaste),
    focusedElementAfterActivate: normalizeOptionalText(value.focusedElementAfterActivate),
    focusedElementAfterPaste: normalizeOptionalText(value.focusedElementAfterPaste),
    accessibilityTrusted:
      typeof value.accessibilityTrusted === "boolean" ? value.accessibilityTrusted : undefined,
    clipboardWritten: typeof value.clipboardWritten === "boolean" ? value.clipboardWritten : undefined,
    clipboardMatchesExpected:
      typeof value.clipboardMatchesExpected === "boolean" ? value.clipboardMatchesExpected : undefined,
    clipboardRestoreAttempted:
      typeof value.clipboardRestoreAttempted === "boolean" ? value.clipboardRestoreAttempted : undefined,
    clipboardRestored: typeof value.clipboardRestored === "boolean" ? value.clipboardRestored : undefined,
    clipboardRestoreMessage:
      typeof value.clipboardRestoreMessage === "string" ? value.clipboardRestoreMessage : undefined,
    insertionVerified: typeof value.insertionVerified === "boolean" ? value.insertionVerified : undefined,
    verificationStatus: normalizeOptionalText(value.verificationStatus),
    elapsedMs: typeof value.elapsedMs === "number" ? value.elapsedMs : undefined,
    details: Array.isArray(value.details)
      ? value.details.filter((detail): detail is string => typeof detail === "string" && Boolean(detail.trim()))
      : [],
  };
}

/** 规范化日志等级，防止异常存储值破坏样式。 */
function normalizeDiagnosticLogLevel(value: unknown): DiagnosticLogLevel {
  if (value === "success" || value === "warning" || value === "error") {
    return value;
  }
  return "info";
}

/** 规范化日志阶段，未知值归到系统阶段。 */
function normalizeDiagnosticLogCategory(value: unknown): DiagnosticLogCategory {
  if (value === "recording" || value === "transcribe" || value === "process" || value === "paste") {
    return value;
  }
  return "system";
}

/** 规范化可选语音模式，非法值直接不展示。 */
function normalizeOptionalMode(value: unknown): ShortcutMode | undefined {
  if (value === "dictate" || value === "translate" || value === "ask" || value === "polish" || value === "subtitle") {
    return value;
  }
  return undefined;
}

/** 规范化可选文本字段，空值不进入界面。 */
function normalizeOptionalText(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim();
  return normalized || undefined;
}

/** 写入诊断日志并通知同源窗口刷新。 */
function writeDiagnosticLogs(logs: DiagnosticLogItem[]): void {
  localStorage.setItem(DIAGNOSTIC_LOG_STORAGE_KEY, JSON.stringify(logs));
}

/** 复制诊断日志，方便把一次失败链路直接发给开发定位。 */
async function copyDiagnosticLogs(): Promise<void> {
  const logs = readDiagnosticLogs();
  if (!logs.length) {
    showHubNotice("当前没有可复制的诊断日志。", "idle");
    return;
  }
  await copyText(formatDiagnosticLogsText(logs));
  showHubNotice("诊断日志已复制。", "success");
}

/** 把诊断日志整理成纯文本，便于粘贴到对话里排查问题。 */
function formatDiagnosticLogsText(logs: DiagnosticLogItem[]): string {
  return logs
    .map((item) => {
      const details = buildDiagnosticLogDetails(item);
      return [
        `[${formatDateTime(item.createdAt)}] ${DIAGNOSTIC_LOG_CATEGORY_LABELS[item.category]} / ${DIAGNOSTIC_LOG_LEVEL_LABELS[item.level]}`,
        item.title,
        item.message,
        ...details,
      ].join("\n");
    })
    .join("\n\n");
}

/** 清空诊断日志，执行前要求二次点击确认。 */
function clearDiagnosticLogs(button: HTMLButtonElement): void {
  if (!readDiagnosticLogs().length) {
    showHubNotice("当前没有诊断日志。", "idle");
    return;
  }
  if (!confirmDangerousAction("clearDiagnosticLog", button, "再次点击清空", "再次点击将清空本机诊断日志。")) {
    return;
  }
  localStorage.removeItem(DIAGNOSTIC_LOG_STORAGE_KEY);
  resetPendingConfirmation();
  renderHub();
  showHubNotice("诊断日志已清空。", "success");
}

/** 让危险按钮进入短暂确认态；同一按钮在确认窗口内再次点击才返回 true。 */
function confirmDangerousAction(id: string, button: HTMLButtonElement, confirmLabel: string, message: string): boolean {
  if (pendingConfirmation?.id === id) {
    return true;
  }
  resetPendingConfirmation();
  const originalLabel = button.textContent || "";
  button.textContent = confirmLabel;
  button.dataset.confirming = "true";
  showHubNotice(message, "busy");
  pendingConfirmation = {
    id,
    button,
    originalLabel,
    timeoutHandle: window.setTimeout(resetPendingConfirmation, 3200),
  };
  return false;
}

/** 退出危险按钮确认态，并恢复按钮原文案和视觉状态。 */
function resetPendingConfirmation(): void {
  if (!pendingConfirmation) {
    return;
  }
  window.clearTimeout(pendingConfirmation.timeoutHandle);
  pendingConfirmation.button.textContent = pendingConfirmation.originalLabel;
  delete pendingConfirmation.button.dataset.confirming;
  pendingConfirmation = null;
}

/** 读取本地历史并按保留策略清理。 */
function readHistory(): HistoryItem[] {
  const raw = localStorage.getItem(HISTORY_STORAGE_KEY);
  if (!raw) {
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as Partial<HistoryItem>[];
    const items = parsed
      .map(normalizeHistoryItem)
      .filter((item): item is HistoryItem => Boolean(item))
      .sort((left, right) => right.createdAt - left.createdAt);
    return applyHistoryRetention(items);
  } catch {
    return [];
  }
}

/** 对历史记录做类型兜底。 */
function normalizeHistoryItem(value: Partial<HistoryItem>): HistoryItem | null {
  if (typeof value.id !== "string" || typeof value.outputText !== "string") {
    return null;
  }
  return {
    id: value.id,
    mode: normalizeMode(value.mode),
    sourceText: typeof value.sourceText === "string" ? value.sourceText : value.outputText,
    outputText: value.outputText,
    createdAt: typeof value.createdAt === "number" ? value.createdAt : Date.now(),
    recordElapsedMs: typeof value.recordElapsedMs === "number" ? value.recordElapsedMs : 0,
    transcribeElapsedMs: typeof value.transcribeElapsedMs === "number" ? value.transcribeElapsedMs : 0,
    processElapsedMs: typeof value.processElapsedMs === "number" ? value.processElapsedMs : 0,
    model: typeof value.model === "string" ? value.model : "",
    contextApp: typeof value.contextApp === "string" ? value.contextApp : "",
  };
}

/** 按设置中的保留策略过滤历史。 */
function applyHistoryRetention(items: HistoryItem[]): HistoryItem[] {
  const retention = readSavedConfig().historyRetention;
  if (retention === "never") {
    return [];
  }
  if (retention === "forever") {
    return items;
  }
  const days = Number(retention);
  const threshold = Date.now() - days * 24 * 60 * 60 * 1000;
  return items.filter((item) => item.createdAt >= threshold);
}

/** 保存历史记录并返回写入后的当前条目。 */
function saveHistory(item: HistoryItem): HistoryItem {
  if (readSavedConfig().historyRetention === "never") {
    return item;
  }
  const history = [item, ...readHistory()].slice(0, 300);
  writeHistory(history);
  return item;
}

/** 写入历史记录。 */
function writeHistory(history: HistoryItem[]): void {
  localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(history));
}

/** 读取本地字幕历史，异常时返回空数组。 */
function readSubtitleHistory(): SubtitleHistoryItem[] {
  const raw = localStorage.getItem(SUBTITLE_HISTORY_STORAGE_KEY);
  if (!raw) {
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as Partial<SubtitleHistoryItem>[];
    const items = parsed
      .map(normalizeSubtitleHistoryItem)
      .filter((item): item is SubtitleHistoryItem => Boolean(item))
      .sort((left, right) => right.createdAt - left.createdAt);
    return applySubtitleHistoryRetention(items);
  } catch {
    return [];
  }
}

/** 对字幕历史记录做类型兜底。 */
function normalizeSubtitleHistoryItem(value: Partial<SubtitleHistoryItem>): SubtitleHistoryItem | null {
  if (typeof value.id !== "string" || typeof value.text !== "string" || !value.text.trim()) {
    return null;
  }
  return {
    id: value.id,
    text: value.text,
    createdAt: typeof value.createdAt === "number" ? value.createdAt : Date.now(),
    source: normalizeSubtitleAudioSource(value.source),
    elapsedMs: typeof value.elapsedMs === "number" ? value.elapsedMs : 0,
    model: typeof value.model === "string" ? value.model : "",
  };
}

/** 规范化字幕音频来源，避免旧数据破坏渲染。 */
function normalizeSubtitleAudioSource(value: unknown): SubtitleAudioSource {
  if (value === "system" || value === "mixed") {
    return value;
  }
  return "microphone";
}

/** 按通用历史保留策略过滤字幕历史。 */
function applySubtitleHistoryRetention(items: SubtitleHistoryItem[]): SubtitleHistoryItem[] {
  const retention = readSavedConfig().historyRetention;
  if (retention === "never") {
    return [];
  }
  if (retention === "forever") {
    return items;
  }
  const days = Number(retention);
  const threshold = Date.now() - days * 24 * 60 * 60 * 1000;
  return items.filter((item) => item.createdAt >= threshold);
}

/** 保存一条字幕历史，并过滤短时间内完全相同的重复记录。 */
function saveSubtitleHistory(item: SubtitleHistoryItem): SubtitleHistoryItem {
  if (readSavedConfig().historyRetention === "never") {
    return item;
  }
  const latest = readSubtitleHistory()[0];
  if (latest && normalizeSubtitleText(latest.text) === normalizeSubtitleText(item.text) && item.createdAt - latest.createdAt < 3000) {
    return item;
  }
  writeSubtitleHistory([item, ...readSubtitleHistory()]);
  return item;
}

/** 写入本地字幕历史。 */
function writeSubtitleHistory(items: SubtitleHistoryItem[]): void {
  localStorage.setItem(SUBTITLE_HISTORY_STORAGE_KEY, JSON.stringify(items.slice(0, MAX_SUBTITLE_HISTORY_ITEMS)));
}

/** 渲染最近结果。 */
function updateRecentResult(item: HistoryItem | null): void {
  if (!item) {
    hubResultTextarea.value = "";
    latestResultMeta.textContent = "等待第一次语音输入";
    copyHubResultButton.disabled = true;
    retryHubResultButton.disabled = true;
    return;
  }
  hubResultTextarea.value = item.outputText;
  latestResultMeta.textContent = `${MODE_LABELS[item.mode]} · ${formatTimingSummary(item)}`;
  copyHubResultButton.disabled = !item.outputText.trim();
  retryHubResultButton.disabled = !item.sourceText.trim();
}

/** 渲染词典列表。 */
function renderDictionary(): void {
  if (windowMode !== "hub") {
    return;
  }
  document.querySelectorAll<HTMLButtonElement>("[data-dictionary-filter]").forEach((button) => {
    button.classList.toggle("isActive", button.dataset.dictionaryFilter === dictionaryFilter);
  });
  const keyword = dictionarySearchInput.value.trim().toLowerCase();
  const dictionaryItems = readDictionaryItems();
  exportDictionaryButton.disabled = dictionaryItems.length === 0;
  const items = dictionaryItems
    .filter((item) => dictionaryFilter === "all" || item.source === dictionaryFilter)
    .filter((item) => item.word.toLowerCase().includes(keyword));
  if (!items.length) {
    dictionaryList.innerHTML = '<div class="emptyState">词典为空。</div>';
    return;
  }
  dictionaryList.innerHTML = items
    .map(
      (item) => `
        <span class="wordTag">
          ${escapeHtml(item.word)}
          <em>${item.source === "auto" ? "自动" : "手动"}</em>
          <button type="button" data-dictionary-action="delete" data-word="${escapeHtml(item.word)}">删除</button>
        </span>`,
    )
    .join("");
}

/** 添加词典词条。 */
function addDictionaryWord(event: SubmitEvent): void {
  event.preventDefault();
  const words = splitInputList(dictionaryInput.value, []);
  if (!words.length) {
    showHubNotice("请输入要添加的词条。", "error");
    return;
  }
  upsertDictionaryWords(words, "manual");
  dictionaryInput.value = "";
  renderHub();
  showHubNotice("词条已加入本地词典。", "success");
}

/** 处理词典删除操作。 */
function handleDictionaryAction(event: MouseEvent): void {
  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return;
  }
  const button = target.closest<HTMLButtonElement>("[data-dictionary-action]");
  if (!button) {
    return;
  }
  const word = button.dataset.word || "";
  writeDictionaryItems(readDictionaryItems().filter((item) => item.word !== word));
  renderHub();
}

/** 读取本地词典。 */
function readDictionary(): string[] {
  return readDictionaryItems().map((item) => item.word);
}

/** 读取本地词典完整条目，并兼容早期 string[] 数据。 */
function readDictionaryItems(): DictionaryItem[] {
  const raw = localStorage.getItem(DICTIONARY_STORAGE_KEY);
  if (!raw) {
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .map(normalizeDictionaryItem)
      .filter((item): item is DictionaryItem => Boolean(item))
      .sort((left, right) => left.word.localeCompare(right.word));
  } catch {
    return [];
  }
}

/** 对读取到的词典条目做兼容和类型兜底。 */
function normalizeDictionaryItem(value: unknown): DictionaryItem | null {
  if (typeof value === "string") {
    const word = value.trim();
    return word ? { id: createId(), word, source: "manual", createdAt: Date.now() } : null;
  }
  if (!value || typeof value !== "object") {
    return null;
  }
  const item = value as Partial<DictionaryItem>;
  if (typeof item.word !== "string" || !item.word.trim()) {
    return null;
  }
  return {
    id: typeof item.id === "string" ? item.id : createId(),
    word: item.word.trim(),
    source: item.source === "auto" ? "auto" : "manual",
    createdAt: typeof item.createdAt === "number" ? item.createdAt : Date.now(),
  };
}

/** 批量加入词典词条，已存在的词条不重复写入。 */
function upsertDictionaryWords(words: string[], source: DictionaryItem["source"]): void {
  const existing = readDictionaryItems();
  const wordSet = new Set(existing.map((item) => item.word.toLowerCase()));
  const nextItems = [...existing];
  words
    .map((word) => word.trim())
    .filter(Boolean)
    .forEach((word) => {
      if (wordSet.has(word.toLowerCase())) {
        return;
      }
      wordSet.add(word.toLowerCase());
      nextItems.push({ id: createId(), word, source, createdAt: Date.now() });
    });
  writeDictionaryItems(nextItems);
}

/** 写入本地词典完整条目。 */
function writeDictionaryItems(items: DictionaryItem[]): void {
  localStorage.setItem(DICTIONARY_STORAGE_KEY, JSON.stringify(items));
}

/** 处理托盘菜单加入词典动作，来源是系统剪贴板里的真实文本。 */
function addDictionaryWordsFromTray(words: string[]): void {
  const normalizedWords = words.map((word) => word.trim()).filter(Boolean);
  switchHubView("dictionary");
  if (!normalizedWords.length) {
    showHubNotice("剪贴板里没有可加入词典的词汇。", "error");
    return;
  }
  upsertDictionaryWords(normalizedWords, "manual");
  dictionaryFilter = "all";
  dictionarySearchInput.value = "";
  renderHub();
  showHubNotice(`已加入 ${normalizedWords.length} 个词条。`, "success");
}

/** 从 CSV 或纯文本文件导入词典词条。 */
async function importDictionaryCsv(): Promise<void> {
  const file = dictionaryImportInput.files?.[0];
  if (!file) {
    return;
  }
  try {
    const content = await file.text();
    upsertDictionaryWords(parseDictionaryCsv(content), "manual");
    dictionaryImportInput.value = "";
    renderHub();
    showHubNotice("词典已导入。", "success");
  } catch (error) {
    showHubNotice(`导入词典失败：${formatError(error)}`, "error");
  }
}

/** 导出当前词典为 CSV 文件。 */
function exportDictionaryCsv(): void {
  if (!readDictionaryItems().length) {
    showHubNotice("当前词典为空，不能导出。", "error");
    return;
  }
  const rows = ["word,source,createdAt", ...readDictionaryItems().map(dictionaryItemToCsvRow)];
  const blob = new Blob([rows.join("\n")], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `typesass-dictionary-${new Date().toISOString().slice(0, 10)}.csv`;
  link.click();
  URL.revokeObjectURL(url);
  showHubNotice("词典 CSV 已导出。", "success");
}

/** 将词典条目转成 CSV 一行。 */
function dictionaryItemToCsvRow(item: DictionaryItem): string {
  return [item.word, item.source, new Date(item.createdAt).toISOString()].map(csvEscape).join(",");
}

/** 从 CSV 内容中提取第一列词条，兼容逗号、换行和纯文本。 */
function parseDictionaryCsv(content: string): string[] {
  return content
    .split(/\r?\n/)
    .flatMap((line) => parseCsvLine(line).slice(0, 1))
    .map((word) => word.trim())
    .filter((word) => Boolean(word) && word.toLowerCase() !== "word");
}

/** 解析单行 CSV，支持基础引号转义。 */
function parseCsvLine(line: string): string[] {
  const cells: string[] = [];
  let current = "";
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    const next = line[index + 1];
    if (char === '"' && quoted && next === '"') {
      current += '"';
      index += 1;
    } else if (char === '"') {
      quoted = !quoted;
    } else if (char === "," && !quoted) {
      cells.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  cells.push(current);
  return cells;
}

/** 转义 CSV 字段。 */
function csvEscape(value: string): string {
  if (!/[",\n]/.test(value)) {
    return value;
  }
  return `"${value.replace(/"/g, '""')}"`;
}

/** 拆分逗号、顿号、换行分隔的输入。 */
function splitInputList(value: string, fallback: string[]): string[] {
  const items = value
    .split(/[,，、\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
  return items.length ? items : fallback;
}

/** 复制文本到剪贴板。 */
async function copyText(text: string): Promise<void> {
  const normalizedText = text.trim();
  if (!normalizedText) {
    showHubNotice("没有可复制内容。", "error");
    return;
  }
  try {
    await navigator.clipboard.writeText(normalizedText);
  } catch {
    resultTextarea.value = normalizedText;
    resultTextarea.select();
    document.execCommand("copy");
  }
  showHubNotice("已复制到剪贴板。", "success");
}

/** 启动录音计时器。 */
function startTimer(): void {
  stopTimer();
  const tick = (): void => {
    recordTimer.textContent = formatDuration(Date.now() - recordStartedAt);
  };
  tick();
  timerHandle = window.setInterval(tick, 250);
}

/** 停止录音计时器。 */
function stopTimer(): void {
  if (timerHandle !== null) {
    window.clearInterval(timerHandle);
    timerHandle = null;
  }
}

/** 停止麦克风流并释放浏览器录音资源。 */
function stopStream(): void {
  disconnectAudioNodes();
  if (recordingStream) {
    recordingStream.getTracks().forEach((track) => track.stop());
    recordingStream = null;
  }
}

/** 断开 WebAudio 节点。 */
function disconnectAudioNodes(): void {
  audioProcessor?.disconnect();
  audioSource?.disconnect();
  audioSink?.disconnect();
  audioProcessor = null;
  audioSource = null;
  audioSink = null;
  void audioContext?.close();
  audioContext = null;
}

/** 更新悬浮条状态，并在错误时触发顶部气泡。 */
function setStatus(message: string, state: StatusState): void {
  statusText.textContent = message;
  floatShell.dataset.state = state;
  soundStage.dataset.state = state;
  if (state !== "recording") {
    resetVoiceLevelVisual();
  }
  if (state === "error") {
    void showRemoteErrorBubble(message);
  }
}

/** 给悬浮胶囊一个轻微反馈，表示按键已收到但当前状态不能立刻切换。 */
function flashFloatingNudge(): void {
  floatShell.dataset.nudge = "true";
  window.setTimeout(() => {
    delete floatShell.dataset.nudge;
  }, 240);
}

/** 通过 Tauri 独立窗口显示顶部错误气泡。 */
async function showRemoteErrorBubble(message: string): Promise<void> {
  if (!isTauriRuntime() || windowMode === "toast") {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("show_error_bubble", { message });
  } catch {
    // 顶部气泡失败时，悬浮条状态文本仍保留错误原因。
  }
}

/** 在当前窗口展示错误气泡内容。 */
function showLocalErrorBubble(message: string): void {
  statusBubble.textContent = message;
  statusBubble.dataset.visible = "true";
  if (bubbleTimerHandle !== null) {
    window.clearTimeout(bubbleTimerHandle);
  }
  bubbleTimerHandle = window.setTimeout(() => {
    statusBubble.dataset.visible = "false";
    window.setTimeout(() => void hideToastWindow(), 220);
  }, 5200);
}

/** 隐藏独立错误提示窗口，避免透明窗口停留在屏幕顶部。 */
async function hideToastWindow(): Promise<void> {
  if (!isTauriRuntime() || windowMode !== "toast") {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("hide_toast_window");
  } catch {
    // 提示窗口自动隐藏失败时，不影响主流程。
  }
}

/** 格式化异常对象为用户可读文本。 */
function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "操作失败";
}

/** 格式化毫秒耗时。 */
function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return "0ms";
  }
  if (ms < 1000) {
    return `${Math.round(ms)}ms`;
  }
  if (ms < 60_000) {
    const seconds = ms / 1000;
    return `${seconds < 10 ? seconds.toFixed(1) : Math.round(seconds).toString()}s`;
  }
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** 汇总一次语音处理链路的关键耗时，优先展示毫秒级速度感。 */
function formatTimingSummary(item: HistoryItem): string {
  const aiTiming = item.processElapsedMs ? formatDuration(item.processElapsedMs) : "未润色";
  return `录音 ${formatDuration(item.recordElapsedMs)} · 转写 ${formatDuration(item.transcribeElapsedMs)} · AI润色 ${aiTiming}`;
}

/** 把历史记录耗时渲染成独立标签，便于对比转写和 AI 润色速度。 */
function formatHistoryTimingChips(item: HistoryItem): string {
  const aiTiming = item.processElapsedMs ? formatDuration(item.processElapsedMs) : "未润色";
  return [
    ["录音", formatDuration(item.recordElapsedMs)],
    ["转写", formatDuration(item.transcribeElapsedMs)],
    ["AI润色", aiTiming],
  ]
    .map(
      ([label, value]) => `
        <span class="historyTimingChip">
          <em>${label}</em>
          <strong>${value}</strong>
        </span>`,
    )
    .join("");
}

/** 格式化字节数。 */
function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** 格式化紧凑数字。 */
function formatCompactNumber(value: number): string {
  if (value >= 10000) {
    return `${(value / 10000).toFixed(1)} 万`;
  }
  if (value >= 1000) {
    return `${(value / 1000).toFixed(1)}K`;
  }
  return String(value);
}

/** 格式化历史记录时间。 */
function formatDateTime(timestamp: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestamp));
}

/** 判断两个时间戳是否属于同一个本地自然日。 */
function isSameLocalDate(leftTimestamp: number, rightTimestamp: number): boolean {
  const left = new Date(leftTimestamp);
  const right = new Date(rightTimestamp);
  return (
    left.getFullYear() === right.getFullYear() &&
    left.getMonth() === right.getMonth() &&
    left.getDate() === right.getDate()
  );
}

/** 统计中文和英文混合文本的近似字数。 */
function countTextUnits(text: string): number {
  return Array.from(text.trim()).filter((char) => !/\s/.test(char)).length;
}

/** 生成本地历史记录 ID。 */
function createId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** 转义 HTML，避免本地历史内容影响页面结构。 */
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}
