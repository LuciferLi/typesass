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
  Translate,
  VoiceInput,
  setConfig,
} from "@icon-park/svg";
import brandLogoUrl from "./assets/typesass-logo.png";

const DEFAULT_BASE_URL = "https://token-plan-cn.xiaomimimo.com/v1";
const DEFAULT_ASR_MODEL = "mimo-v2.5-asr";
const DEFAULT_TEXT_MODEL = "mimo-v2.5";
const CONFIG_STORAGE_KEY = "aiToolVoiceConfigV2";
const LEGACY_CONFIG_STORAGE_KEY = "aiToolVoiceConfig";
const HISTORY_STORAGE_KEY = "aiToolVoiceHistoryV1";
const DICTIONARY_STORAGE_KEY = "aiToolDictionaryV1";
const DEFAULT_HUB_NOTICE = "所有设置和历史都只保存在本机。";
const MIN_RECORDING_MS = 800;
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
  translate: Translate,
  voice: VoiceInput,
} as const;

type VoiceMode = "dictate" | "translate" | "ask";
type StatusState = "idle" | "ready" | "recording" | "busy" | "error";
type HubNoticeState = "idle" | "busy" | "success" | "error";
type WindowMode = "main" | "hub" | "toast" | "result";
type HistoryRetention = "forever" | "30" | "7" | "never";
type DictionaryFilter = "all" | "auto" | "manual";
type DiagnosticState = "idle" | "success" | "warning" | "error";
type IconName = keyof typeof ICON_RENDERERS;
type ReadinessAction = "apiKey" | "microphone" | "accessibility" | "shortcut" | "start" | "refresh";

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
  /** 当前正在录制的语音模式。 */
  mode: VoiceMode;
  /** 进入录制态之前输入框展示的快捷键文本。 */
  label: string;
}

interface ShortcutTriggerPayload {
  /** 快捷键触发的语音模式。 */
  mode: VoiceMode;
  /** 按下快捷键瞬间的前台目标 App。 */
  targetApp: string;
}

interface ShortcutConfig {
  /** 听写模式全局快捷键。 */
  dictate: string;
  /** 翻译模式全局快捷键。 */
  translate: string;
  /** 随便问模式全局快捷键。 */
  ask: string;
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
  /** 选定的麦克风设备 ID，default 表示系统默认设备。 */
  microphoneDeviceId: string;
  /** 是否播放开始和停止录音提示音。 */
  interactionSounds: boolean;
  /** 录音期间是否临时静音系统输出。 */
  muteWhileDictating: boolean;
  /** 是否开机后自动启动。 */
  launchAtLogin: boolean;
  /** 是否在 Dock 中展示图标。 */
  showInDock: boolean;
  /** 三种语音模式的快捷键配置。 */
  shortcuts: ShortcutConfig;
  /** 用户对输出风格的本地偏好。 */
  personalStyle: string;
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
  /** 是否已经触发系统粘贴。 */
  pasted: boolean;
  /** 自动粘贴后的状态说明。 */
  message: string;
  /** 是否需要用户授予辅助功能权限。 */
  requiresAccessibility: boolean;
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

interface RuntimeDiagnostics {
  /** 当前会话内存里是否已有 Mimo Key。 */
  hasSessionApiKey: boolean;
  /** macOS 钥匙串里是否已保存 Mimo Key。 */
  hasKeychainApiKey: boolean;
  /** 启动环境变量里是否已有 Mimo Key。 */
  hasEnvApiKey: boolean;
  /** macOS 辅助功能权限是否已授权。 */
  accessibilityTrusted: boolean;
  /** Rust 侧当前注册的三种快捷键。 */
  shortcuts: ShortcutConfig;
  /** 当前全局快捷键是否已成功注册到系统。 */
  shortcutRegistrationReady: boolean;
  /** 最近一次全局快捷键注册结果说明。 */
  shortcutRegistrationMessage: string;
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

interface TauriWindow extends Window {
  /** Tauri 运行时注入对象，浏览器预览模式不存在。 */
  __TAURI_INTERNALS__?: unknown;
  /** Rust 快捷键直达前端的处理函数。 */
  __AIToolHandleShortcutMode?: (mode: VoiceMode, targetApp?: string) => void;
  /** 前端尚未加载完成时暂存的快捷键触发模式。 */
  __AIToolPendingShortcutMode?: VoiceMode | ShortcutTriggerPayload;
  /** Rust 结果窗口直达前端的渲染函数。 */
  __AIToolRenderResult?: (payload: ResultWindowPayload) => void;
}

const MODE_LABELS: Record<VoiceMode, string> = {
  dictate: "口述",
  translate: "翻译",
  ask: "随便问",
};

const MODE_ACTION_ICONS: Record<VoiceMode, IconName> = {
  dictate: "microphone",
  translate: "translate",
  ask: "message",
};

const VIEW_TITLES: Record<string, { eyebrow: string; title: string }> = {
  home: { eyebrow: "说话，不要打字", title: "仪表盘" },
  modes: { eyebrow: "选择真实语音流程", title: "语音模式" },
  shortcuts: { eyebrow: "按一次开始，再按一次结束", title: "快捷键" },
  history: { eyebrow: "只保存在本机", title: "历史记录" },
  dictionary: { eyebrow: "专有名词更准确", title: "词典" },
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
const targetLanguagesInput = getElement<HTMLInputElement>("targetLanguagesInput");
const historyRetentionSelect = getElement<HTMLSelectElement>("historyRetentionSelect");
const postProcessDictationInput = getElement<HTMLInputElement>("postProcessDictationInput");
const quickAiPolishInput = getElement<HTMLInputElement>("quickAiPolishInput");
const microphoneSelect = getElement<HTMLSelectElement>("microphoneSelect");
const refreshMicrophonesButton = getElement<HTMLButtonElement>("refreshMicrophonesButton");
const interactionSoundsInput = getElement<HTMLInputElement>("interactionSoundsInput");
const muteWhileDictatingInput = getElement<HTMLInputElement>("muteWhileDictatingInput");
const launchAtLoginInput = getElement<HTMLInputElement>("launchAtLoginInput");
const showInDockInput = getElement<HTMLInputElement>("showInDockInput");
const personalStyleInput = getElement<HTMLTextAreaElement>("personalStyleInput");
const dictateShortcutInput = getElement<HTMLInputElement>("dictateShortcutInput");
const translateShortcutInput = getElement<HTMLInputElement>("translateShortcutInput");
const askShortcutInput = getElement<HTMLInputElement>("askShortcutInput");
const shortcutValidationText = getElement<HTMLElement>("shortcutValidationText");
const dictateShortcutText = getElement<HTMLElement>("dictateShortcutText");
const translateShortcutText = getElement<HTMLElement>("translateShortcutText");
const askShortcutText = getElement<HTMLElement>("askShortcutText");
const homeDictateShortcutText = getElement<HTMLElement>("homeDictateShortcutText");
const homeTranslateShortcutText = getElement<HTMLElement>("homeTranslateShortcutText");
const homeAskShortcutText = getElement<HTMLElement>("homeAskShortcutText");
const saveConfigButton = getElement<HTMLButtonElement>("saveConfigButton");
const saveShortcutButton = getElement<HTMLButtonElement>("saveShortcutButton");
const clearConfigButton = getElement<HTMLButtonElement>("clearConfigButton");
const startDictateButton = getElement<HTMLButtonElement>("startDictateButton");
const quickStartButton = getElement<HTMLButtonElement>("quickStartButton");
const operationHint = getElement<HTMLElement>("operationHint");
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
let isStartingRecording = false;
let lastShortcutAt = 0;
let activeMode: VoiceMode = "dictate";
let historyFilter: VoiceMode | "all" = "all";
let dictionaryFilter: DictionaryFilter = "all";
let shortcutRecordingMode: VoiceMode | null = null;
let previousSystemMuteState: boolean | null = null;
let recordingTargetApp = "";
let resultCopyFeedbackTimer: number | null = null;
let nextReadinessAction: ReadinessAction = "apiKey";
let pendingConfirmation: PendingConfirmation | null = null;
let shortcutRecordingSnapshot: ShortcutRecordingSnapshot | null = null;
let accessibilityWatchHandle: number | null = null;

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
  if (mode === "hub" || mode === "toast" || mode === "result") {
    return mode;
  }
  return "main";
}

/** 初始化悬浮录音条窗口。 */
function initFloatingWindow(): void {
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
  bindHubEvents();
  void bindHubControlEvents();
  void populateMicrophones();
  void syncDesktopPreferences(readSavedConfig());
  renderHub();
  void refreshDiagnostics();
  window.addEventListener("storage", renderHub);
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
        handleShortcutMode(pendingShortcut.mode, pendingShortcut.targetApp);
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
    await listen<VoiceMode>("hub-start-mode", (event) => {
      handleShortcutMode(event.payload);
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
  document.querySelectorAll<HTMLButtonElement>("[data-mode-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const mode = normalizeMode(button.dataset.modeAction);
      selectVoiceMode(mode);
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-mode-start]").forEach((button) => {
    button.addEventListener("click", () => void requestFloatingMode(normalizeMode(button.dataset.modeStart)));
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
    button.addEventListener("click", () => startShortcutRecording(normalizeMode(button.dataset.shortcutRecord)));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-shortcut-reset]").forEach((button) => {
    button.addEventListener("click", () => resetShortcutInput(normalizeMode(button.dataset.shortcutReset)));
  });
  startDictateButton.addEventListener("click", () => void requestFloatingMode(activeMode));
  quickStartButton.addEventListener("click", () => void requestFloatingMode(activeMode));
  quickAiPolishInput.addEventListener("change", () => setDictationPolishEnabled(quickAiPolishInput.checked));
  postProcessDictationInput.addEventListener("change", () => setDictationPolishEnabled(postProcessDictationInput.checked));
  refreshStatusButton.addEventListener("click", () => void refreshHubRuntimeState());
  saveConfigButton.addEventListener("click", () => void saveConfigFromForm());
  saveShortcutButton.addEventListener("click", () => void saveConfigFromForm("快捷键已保存并重新生效。"));
  clearConfigButton.addEventListener("click", () => clearSavedConfig(clearConfigButton));
  clearApiKeyButton.addEventListener("click", () => void clearSavedApiKey());
  clearHistoryButton.addEventListener("click", () => clearHistory(clearHistoryButton));
  copyHubResultButton.addEventListener("click", () => void copyText(hubResultTextarea.value));
  retryHubResultButton.addEventListener("click", () => void retryLatestHistory());
  authorizeMicrophoneButton.addEventListener("click", () => void authorizeMicrophoneAccess());
  refreshMicrophonesButton.addEventListener("click", () => void populateMicrophones());
  importDictionaryButton.addEventListener("click", () => dictionaryImportInput.click());
  exportDictionaryButton.addEventListener("click", exportDictionaryCsv);
  refreshDiagnosticsButton.addEventListener("click", () => void refreshDiagnostics());
  openAccessibilityButton.addEventListener("click", () => void openAccessibilitySettings());
  nextStepPrimaryButton.addEventListener("click", () => void handleNextStepAction());
  nextStepRefreshButton.addEventListener("click", () => void refreshHubRuntimeState());
  dictionaryImportInput.addEventListener("change", () => void importDictionaryCsv());
  dictionaryForm.addEventListener("submit", addDictionaryWord);
  dictionarySearchInput.addEventListener("input", renderDictionary);
  historyList.addEventListener("click", handleHistoryAction);
  dictionaryList.addEventListener("click", handleDictionaryAction);
  window.addEventListener("keydown", captureShortcutKeys, true);
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
  if (value === "translate" || value === "ask") {
    return value;
  }
  return "dictate";
}

/** 根据字符串恢复历史筛选值，非法值回落到全部。 */
function normalizeHistoryFilter(value: string | undefined): VoiceMode | "all" {
  if (value === "dictate" || value === "translate" || value === "ask") {
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
function handleShortcutMode(mode: VoiceMode, targetApp = ""): void {
  const now = Date.now();
  if (now - lastShortcutAt < 500) {
    flashFloatingNudge();
    if (isRecording) {
      setStatus("已经在录音，说完后再按一次停止。", "recording");
    } else if (isProcessing) {
      setStatus("正在处理上一段语音，请稍等。", "busy");
    } else {
      setStatus("正在准备麦克风，请稍等。", "busy");
    }
    return;
  }
  lastShortcutAt = now;
  void toggleRecording(mode, targetApp);
}

/** 从本地存储读取配置并回填设置表单。 */
function loadConfigToForm(): void {
  const config = readSavedConfig();
  apiKeyInput.value = "";
  baseUrlInput.value = config.baseUrl;
  modelInput.value = config.asrModel;
  textModelInput.value = config.textModel;
  languageSelect.value = config.language;
  targetLanguagesInput.value = config.targetLanguages.join(", ");
  historyRetentionSelect.value = config.historyRetention;
  postProcessDictationInput.checked = config.postProcessDictation;
  quickAiPolishInput.checked = config.postProcessDictation;
  microphoneSelect.value = config.microphoneDeviceId;
  interactionSoundsInput.checked = config.interactionSounds;
  muteWhileDictatingInput.checked = config.muteWhileDictating;
  launchAtLoginInput.checked = config.launchAtLogin;
  showInDockInput.checked = config.showInDock;
  personalStyleInput.value = config.personalStyle;
  dictateShortcutInput.value = formatShortcutLabel(config.shortcuts.dictate);
  translateShortcutInput.value = formatShortcutLabel(config.shortcuts.translate);
  askShortcutInput.value = formatShortcutLabel(config.shortcuts.ask);
  renderShortcutLabels(config.shortcuts);
  validateShortcutInputs();
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
    targetLanguages: ["简体中文"],
    historyRetention: "forever",
    postProcessDictation: true,
    microphoneDeviceId: "default",
    interactionSounds: true,
    muteWhileDictating: false,
    launchAtLogin: false,
    showInDock: false,
    shortcuts: { ...DEFAULT_SHORTCUTS },
    personalStyle: "",
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
    microphoneDeviceId:
      typeof value.microphoneDeviceId === "string" && value.microphoneDeviceId.trim()
        ? value.microphoneDeviceId
        : fallback.microphoneDeviceId,
    interactionSounds:
      typeof value.interactionSounds === "boolean" ? value.interactionSounds : fallback.interactionSounds,
    muteWhileDictating:
      typeof value.muteWhileDictating === "boolean" ? value.muteWhileDictating : fallback.muteWhileDictating,
    launchAtLogin: typeof value.launchAtLogin === "boolean" ? value.launchAtLogin : fallback.launchAtLogin,
    showInDock: typeof value.showInDock === "boolean" ? value.showInDock : fallback.showInDock,
    shortcuts: normalizeShortcuts(value.shortcuts, fallback.shortcuts),
    personalStyle: typeof value.personalStyle === "string" ? value.personalStyle : fallback.personalStyle,
  };
}

/** 对三种全局快捷键配置做兜底和去重保护。 */
function normalizeShortcuts(value: unknown, fallback: ShortcutConfig): ShortcutConfig {
  const source = isShortcutConfigLike(value) ? value : fallback;
  return {
    dictate: normalizeShortcutText(source.dictate, fallback.dictate),
    translate: normalizeShortcutText(source.translate, fallback.translate),
    ask: normalizeShortcutText(source.ask, fallback.ask),
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
    .map(normalizeShortcutPart)
    .join("+");
  return normalized || fallback;
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
  return part;
}

/** 检查三个模式是否配置了重复快捷键，避免保存后系统注册失败。 */
function hasShortcutConflict(shortcuts: ShortcutConfig): boolean {
  const values = [shortcuts.dictate, shortcuts.translate, shortcuts.ask].map((shortcut) =>
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

/** 从表单收集当前配置。 */
function readConfigFromForm(): VoiceConfig {
  return {
    baseUrl: baseUrlInput.value.trim() || DEFAULT_BASE_URL,
    asrModel: modelInput.value.trim() || DEFAULT_ASR_MODEL,
    textModel: textModelInput.value.trim() || DEFAULT_TEXT_MODEL,
    language: languageSelect.value || "auto",
    targetLanguages: splitInputList(targetLanguagesInput.value, ["简体中文"]),
    historyRetention: normalizeRetention(historyRetentionSelect.value),
    postProcessDictation: postProcessDictationInput.checked,
    microphoneDeviceId: microphoneSelect.value || "default",
    interactionSounds: interactionSoundsInput.checked,
    muteWhileDictating: muteWhileDictatingInput.checked,
    launchAtLogin: launchAtLoginInput.checked,
    showInDock: showInDockInput.checked,
    shortcuts: {
      dictate: normalizeShortcutText(dictateShortcutInput.value, DEFAULT_SHORTCUTS.dictate),
      translate: normalizeShortcutText(translateShortcutInput.value, DEFAULT_SHORTCUTS.translate),
      ask: normalizeShortcutText(askShortcutInput.value, DEFAULT_SHORTCUTS.ask),
    },
    personalStyle: personalStyleInput.value.trim(),
  };
}

/** 保存配置；Mimo Key 只写入 macOS 钥匙串，不进入 localStorage。 */
async function saveConfigFromForm(successMessage = "设置已保存，快捷键已重新生效。"): Promise<void> {
  const config = readConfigFromForm();
  if (!validateShortcutInputs() || hasShortcutConflict(config.shortcuts)) {
    showHubNotice("快捷键配置需要处理后才能保存。", "error");
    return;
  }
  showHubNotice("正在保存设置。", "busy");
  const apiKeyReady = await syncSavedApiKey();
  if (!apiKeyReady) {
    return;
  }
  const desktopReady = await syncDesktopPreferences(config);
  if (!desktopReady) {
    await refreshDiagnostics();
    showHubNotice("设置未保存，部分系统设置需要检查权限。", "error");
    return;
  }
  localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(config));
  syncDictationPolishSwitches(config.postProcessDictation);
  renderHub();
  await refreshDiagnostics();
  showHubNotice(successMessage, "success");
}

/** 只切换口述 AI 润色开关，不触碰 Mimo Key、快捷键和桌面系统偏好。 */
function setDictationPolishEnabled(enabled: boolean): void {
  const config = readSavedConfig();
  config.postProcessDictation = enabled;
  localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(config));
  syncDictationPolishSwitches(enabled);
  renderHub();
  showHubNotice(
    enabled ? "AI 润色已开启，口述后会润色再粘贴。" : "AI 润色已关闭，口述后会直接粘贴原始转写。",
    "success",
  );
}

/** 同步顶部快捷开关和设置页开关，确保同一配置没有两个状态。 */
function syncDictationPolishSwitches(enabled: boolean): void {
  quickAiPolishInput.checked = enabled;
  postProcessDictationInput.checked = enabled;
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
  homeDictateShortcutText.textContent = formatShortcutLabel(shortcuts.dictate);
  homeTranslateShortcutText.textContent = formatShortcutLabel(shortcuts.translate);
  homeAskShortcutText.textContent = formatShortcutLabel(shortcuts.ask);
  updateFloatingShortcutTitle(shortcuts);
}

/** 刷新设置页里所有桌面能力的真实状态。 */
async function refreshDiagnostics(): Promise<void> {
  if (windowMode !== "hub") {
    return;
  }
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
    updateNextStepPanel("error", "重新检查运行状态", `诊断失败：${formatError(error)}`, "重新检查", "refresh", "refresh");
  }
}

/** 读取 Tauri 桌面端真实运行诊断，供设置页和授权轮询复用。 */
async function readRuntimeDiagnostics(): Promise<RuntimeDiagnostics> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<RuntimeDiagnostics>("get_runtime_diagnostics");
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
      "保存后 Key 会进入 macOS 钥匙串，不会写入本地配置文件。",
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

/** 所有准备项就绪时，把首页下一步切成当前模式的真实开始动作。 */
function updateReadyNextStepPanel(): void {
  const shortcuts = readSavedConfig().shortcuts;
  const shortcutLabel = formatShortcutLabel(shortcuts[activeMode] || shortcuts.dictate);
  updateNextStepPanel(
    "success",
    `可以开始${MODE_LABELS[activeMode]}`,
    `点击开始或按 ${shortcutLabel}，说完再按一次停止并处理。`,
    `开始${MODE_LABELS[activeMode]}`,
    "start",
    "play",
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
    await requestFloatingMode(activeMode);
    return;
  }
  if (nextReadinessAction === "refresh") {
    await refreshHubRuntimeState();
    return;
  }
  await handleReadinessAction(nextReadinessAction);
}

/** 处理首页准备状态上的可行动按钮，减少用户寻找设置项的路径。 */
async function handleReadinessAction(action: ReadinessAction | string): Promise<void> {
  if (action === "apiKey") {
    switchHubView("settings");
    focusSettingControl(apiKeyInput);
    showHubNotice("在这里粘贴 Mimo Key，保存后会进入 macOS 钥匙串。", "busy");
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
    switchHubView("shortcuts");
    focusSettingControl(dictateShortcutInput);
  }
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

/** 进入某个模式的快捷键录制状态。 */
function startShortcutRecording(mode: VoiceMode): void {
  restoreShortcutRecordingSnapshot();
  shortcutRecordingMode = mode;
  clearShortcutRecordingState();
  const input = getShortcutInput(mode);
  shortcutRecordingSnapshot = { mode, label: input.value };
  input.value = "请按新的组合键";
  input.dataset.recording = "true";
  setShortcutValidation("按下包含 Control、Command、Option 或 Shift 的组合键；Esc 可取消。", "busy", true);
  showHubNotice(`正在录制${MODE_LABELS[mode]}快捷键。`, "busy");
}

/** 将某个模式的快捷键恢复为默认值。 */
function resetShortcutInput(mode: VoiceMode): void {
  shortcutRecordingMode = null;
  shortcutRecordingSnapshot = null;
  clearShortcutRecordingState();
  getShortcutInput(mode).value = formatShortcutLabel(DEFAULT_SHORTCUTS[mode]);
  renderShortcutLabels(readConfigFromForm().shortcuts);
  const isValid = validateShortcutInputs();
  showHubNotice(
    isValid ? `${MODE_LABELS[mode]}快捷键已恢复默认，保存后生效。` : "恢复默认后出现快捷键冲突，请调整后保存。",
    isValid ? "success" : "error",
  );
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
      ? `${MODE_LABELS[mode]}快捷键已设为 ${formatShortcutLabel(shortcut)}，保存后生效。`
      : "这个快捷键和其它模式冲突，请重新录制后保存。",
    isValid ? "success" : "error",
  );
}

/** 清除快捷键输入框的录制态。 */
function clearShortcutRecordingState(): void {
  dictateShortcutInput.removeAttribute("data-recording");
  translateShortcutInput.removeAttribute("data-recording");
  askShortcutInput.removeAttribute("data-recording");
}

/** 取消当前快捷键录制并恢复进入录制态前的展示值。 */
function cancelShortcutRecording(): void {
  restoreShortcutRecordingSnapshot();
  shortcutRecordingMode = null;
  clearShortcutRecordingState();
  validateShortcutInputs();
  showHubNotice("已取消快捷键录制。", "idle");
}

/** 如果存在快捷键录制草稿，则恢复对应输入框的原值。 */
function restoreShortcutRecordingSnapshot(): void {
  if (!shortcutRecordingSnapshot) {
    return;
  }
  getShortcutInput(shortcutRecordingSnapshot.mode).value = shortcutRecordingSnapshot.label;
  shortcutRecordingSnapshot = null;
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
  const entries: Array<[VoiceMode, string]> = [
    ["dictate", shortcuts.dictate],
    ["translate", shortcuts.translate],
    ["ask", shortcuts.ask],
  ];
  const invalidEntry = entries.find(([, shortcut]) => !isValidShortcutText(shortcut));
  if (invalidEntry) {
    setShortcutValidation(`${MODE_LABELS[invalidEntry[0]]}快捷键不完整，请重新录制。`, "error", true);
    return false;
  }
  const repeated = entries.find(([, shortcut], index) =>
    entries.some(([, compareShortcut], compareIndex) => compareIndex !== index && compareShortcut === shortcut),
  );
  if (repeated) {
    setShortcutValidation(`${formatShortcutLabel(repeated[1])} 已被多个模式使用，请换一个组合键。`, "error", true);
    return false;
  }
  setShortcutValidation("快捷键没有冲突，保存后会立即重新注册。", "success", false);
  return true;
}

/** 更新快捷键校验提示，并控制保存快捷键按钮是否可用。 */
function setShortcutValidation(message: string, state: HubNoticeState, shouldDisableSave: boolean): void {
  shortcutValidationText.textContent = message;
  shortcutValidationText.dataset.state = state;
  saveShortcutButton.disabled = shouldDisableSave;
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
function getShortcutInput(mode: VoiceMode): HTMLInputElement {
  if (mode === "translate") {
    return translateShortcutInput;
  }
  if (mode === "ask") {
    return askShortcutInput;
  }
  return dictateShortcutInput;
}

/** 从浏览器枚举麦克风设备并填充选择器。 */
async function populateMicrophones(): Promise<void> {
  const config = readSavedConfig();
  const currentValue = microphoneSelect.value || config.microphoneDeviceId;
  microphoneSelect.innerHTML = '<option value="default">系统默认麦克风</option>';
  if (!navigator.mediaDevices?.enumerateDevices) {
    microphoneSelect.value = "default";
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
      });
    microphoneSelect.value = Array.from(microphoneSelect.options).some((option) => option.value === currentValue)
      ? currentValue
      : "default";
  } catch {
    microphoneSelect.value = "default";
  }
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
  selectVoiceMode(mode);
  if (windowMode === "hub" && nextReadinessAction !== "start") {
    await handleNextStepAction();
    return;
  }
  if (!isTauriRuntime()) {
    switchHubView("home");
    showHubNotice("网页预览模式不能触发系统悬浮录音。", "error");
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const { emitTo } = await import("@tauri-apps/api/event");
    await invoke("show_main_window");
    await emitTo("main", "hub-start-mode", mode);
  } catch (error) {
    showHubNotice(`无法唤起悬浮录音：${formatError(error)}`, "error");
  }
}

/** 只切换当前语音模式，不触发录音。 */
function selectVoiceMode(mode: VoiceMode): void {
  activeMode = mode;
  renderActiveModeButtons();
  updateFloatingShortcutTitle(readConfigFromForm().shortcuts);
  if (nextReadinessAction === "start") {
    updateReadyNextStepPanel();
  }
}

/** 开始或停止录音。 */
async function toggleRecording(mode: VoiceMode, targetApp = ""): Promise<void> {
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
  updateFloatingShortcutTitle(readSavedConfig().shortcuts);
  await startRecording(mode, targetApp);
}

/** 请求麦克风权限并开始录音。 */
async function startRecording(mode: VoiceMode, targetApp = ""): Promise<void> {
  if (!navigator.mediaDevices?.getUserMedia) {
    setStatus("当前环境不支持浏览器录音能力。", "error");
    return;
  }

  isStartingRecording = true;
  try {
    const config = readSavedConfig();
    if (!(await ensureReadyForRecording())) {
      return;
    }
    recordingTargetApp = normalizeRecordingTargetApp(targetApp) || normalizeRecordingTargetApp(await readFrontmostApp());
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
    stopStream();
    void restoreSystemMute();
    const message = formatRecordingError(error);
    setStatus(message, "error");
    if (isMicrophonePermissionError(error)) {
      await showHubWindow();
      await switchHubWindowToSettings();
    }
  } finally {
    isStartingRecording = false;
  }
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

/** 录音前检查桌面端必要配置，避免用户说完后才发现无法转写。 */
async function ensureReadyForRecording(): Promise<boolean> {
  if (!isTauriRuntime()) {
    return true;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const diagnostics = await invoke<RuntimeDiagnostics>("get_runtime_diagnostics");
    if (!diagnostics.hasSessionApiKey && !diagnostics.hasKeychainApiKey && !diagnostics.hasEnvApiKey) {
      setStatus("请先在设置里填写 Mimo API Key 并保存。", "error");
      await showHubWindow();
      await switchHubWindowToSettings();
      window.setTimeout(() => void hideFloatingWindow(), 1800);
      return false;
    }
    return true;
  } catch (error) {
    setStatus(`录音前检查失败：${formatError(error)}`, "error");
    return false;
  }
}

/** 请求 Hub 切换到设置页，用于权限或配置缺失时减少用户操作。 */
async function switchHubWindowToSettings(): Promise<void> {
  if (windowMode === "hub") {
    switchHubView("settings");
    return;
  }
  if (!isTauriRuntime()) {
    return;
  }
  try {
    const { emitTo } = await import("@tauri-apps/api/event");
    await emitTo("hub", "hub-switch-view", "settings");
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
    recordButton.title = "开始录音";
    recordButton.setAttribute("aria-label", "开始录音");
    playInteractionSound("stop", readSavedConfig());
    if (recordElapsedMs < MIN_RECORDING_MS) {
      resultMeta.textContent = "录音太短";
      copyButton.disabled = true;
      setStatus("录音太短了，请说完一句话后再停止。", "error");
      return;
    }
    await transcribeAudio(audioBlob, activeMode, recordElapsedMs);
  } catch (error) {
    await restoreSystemMute();
    setStatus(formatError(error), "error");
  } finally {
    isProcessing = false;
    setFloatingDisabled(false);
    isRecording = false;
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
    recordButton.title = "开始录音";
    recordButton.setAttribute("aria-label", "开始录音");
    recordDurationText.textContent = "--";
    audioSizeText.textContent = "--";
    resultMeta.textContent = "已取消";
    setStatus("已取消录音。", "ready");
    playInteractionSound("stop", readSavedConfig());
    void hideFloatingWindow();
    return;
  }
  resultTextarea.value = "";
  recordingTargetApp = "";
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

  const response = await callTranscribe({
    apiKey: "",
    baseUrl: config.baseUrl,
    asrModel: config.asrModel,
    language: config.language,
    contentType: "audio/wav",
    audioBase64: await blobToBase64(audioBlob),
  });

  const sourceText = response.text.trim();
  transcribeDurationText.textContent = formatDuration(response.elapsedMs);
  if (!isMeaningfulTranscription(sourceText)) {
    resultMeta.textContent = "没有识别到语音";
    copyButton.disabled = true;
    setStatus("没有识别到有效语音，请靠近麦克风后再试。", "error");
    return;
  }

  const contextApp = recordingTargetApp || (await readFrontmostApp());
  const shouldProcess = mode !== "dictate" || config.postProcessDictation;
  let usedSourceFallback = false;
  let processed: { text: string; elapsedMs: number; model: string };
  if (shouldProcess) {
    try {
      processed = await processRecognizedText(sourceText, mode, config, contextApp);
      if (mode === "dictate" && !processed.text.trim()) {
        usedSourceFallback = true;
        processed = { text: sourceText, elapsedMs: 0, model: response.model };
      }
    } catch (error) {
      if (mode !== "dictate") {
        throw error;
      }
      usedSourceFallback = true;
      processed = { text: sourceText, elapsedMs: 0, model: response.model };
    }
  } else {
    processed = { text: sourceText, elapsedMs: 0, model: response.model };
  }

  const outputText = processed.text.trim();
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
    setStatus("AI 润色没有及时返回，已先使用原始转写。", "error");
  }
  await pasteTranscription(outputText, contextApp);
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
  setStatus(mode === "dictate" ? "正在 AI 润色。" : `正在执行${MODE_LABELS[mode]}处理。`, "busy");
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
    styleInstruction: buildStyleInstruction(config),
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

/** 生成本地个性化提示，不改写事实，只影响表达形态。 */
function buildStyleInstruction(config: VoiceConfig): string {
  const pieces: string[] = [];
  if (config.personalStyle.trim()) {
    pieces.push(`用户偏好：${config.personalStyle.trim()}。`);
  }
  return pieces.join("\n");
}

/** 转写完成后把结果自动粘贴到当前焦点输入框。 */
async function pasteTranscription(text: string, targetApp = ""): Promise<void> {
  if (!isTauriRuntime()) {
    setStatus("转写完成，可以复制结果。", "ready");
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await activateTargetApp(targetApp);
    const response = await invoke<PasteResponse>("paste_text", { text });
    if (response.pasted) {
      setStatus(response.message, "ready");
      return;
    }
    setStatus(response.message, "error");
    await showResultWindow(text, response.message, response.requiresAccessibility);
  } catch (error) {
    const message = `${formatError(error)}。结果已保留，可手动复制。`;
    setStatus(message, "error");
    await showResultWindow(text, message, false);
  }
}

/** 自动粘贴前切回录音触发时的目标 App，降低焦点被处理窗口抢走的概率。 */
async function activateTargetApp(appName: string): Promise<void> {
  const normalizedAppName = normalizeRecordingTargetApp(appName);
  if (!normalizedAppName) {
    return;
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("activate_app", { appName: normalizedAppName });
    await wait(220);
  } catch {
    // 切回目标 App 失败时仍继续走剪贴板兜底，不中断转写结果。
  }
}

/** 等待一小段时间，让 macOS 完成前台 App 切换。 */
function wait(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
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
  resultReason.textContent = payload.reason || "自动粘贴没有完成，结果已写入剪贴板。";
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

/** 切换 Hub 当前视图。 */
function switchHubView(view: string): void {
  const title = VIEW_TITLES[view] || VIEW_TITLES.home;
  hubTitle.textContent = title.title;
  hubEyebrow.textContent = title.eyebrow;
  resetHubNotice();
  document.querySelectorAll<HTMLElement>("[data-view]").forEach((element) => {
    element.classList.toggle("isActive", element.dataset.view === view);
  });
  document.querySelectorAll<HTMLButtonElement>("[data-view-target]").forEach((button) => {
    button.classList.toggle("isActive", button.dataset.viewTarget === view);
  });
  renderHub();
}

/** 同步 Hub 顶部三种语音模式按钮的选中态。 */
function renderActiveModeButtons(): void {
  document.querySelectorAll<HTMLElement>("[data-mode-action]").forEach((element) => {
    element.classList.toggle("isActive", element.dataset.modeAction === activeMode);
  });
  syncStartActionButtons();
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
  const isReady = nextReadinessAction === "start";
  const sharedLabel = isReady ? `开始${MODE_LABELS[activeMode]}` : "继续配置";
  const sharedIcon = isReady ? "play" : "setting";
  operationHint.textContent = isReady
    ? "点击卡片切换模式，点击开始会唤起屏幕顶部的录音悬浮条。"
    : "点击卡片切换模式，先完成必要配置后再开始录音。";
  [startDictateButton, quickStartButton].forEach((button) => {
    updateActionButtonLabel(button, sharedLabel);
    updateActionButtonIcon(button, sharedIcon);
    button.title = isReady ? sharedLabel : "继续完成必要配置";
  });
  document.querySelectorAll<HTMLButtonElement>("[data-mode-start]").forEach((button) => {
    const mode = normalizeMode(button.dataset.modeStart);
    updateActionButtonLabel(button, isReady ? `开始${MODE_LABELS[mode]}` : "继续配置");
    updateActionButtonIcon(button, isReady ? MODE_ACTION_ICONS[mode] : "setting");
    button.title = isReady ? `开始${MODE_LABELS[mode]}` : "继续完成必要配置";
  });
}

/** 渲染 Hub 中所有本地数据。 */
function renderHub(): void {
  if (windowMode !== "hub") {
    return;
  }
  renderActiveModeButtons();
  renderStats();
  renderHistory();
  renderDictionary();
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
  metricPersonalization.textContent = `${readDictionary().length} 词条${readSavedConfig().personalStyle ? " + 偏好" : ""}`;
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
