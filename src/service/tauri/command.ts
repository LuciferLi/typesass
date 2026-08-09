import type { HttpApiOpenApiDocumentModel } from '@/model/httpApiDoc';
import type { RuntimeDiagnosticsModel, ShortcutProfileModel } from '@/model/permission';
import type {
    CodexThreadListRequestModel,
    CodexThreadSummaryModel,
    CodexWorkspaceModel,
    CreateSessionProjectRequestModel,
    CreateSessionTaskRequestModel,
    SessionWorkspaceDataModel
} from '@/model/sessionManage';
import type { SubtitleHistoryUpdatePayloadModel, SubtitleMessagePayloadModel } from '@/model/subtitle';
import type { SelectedTextResponseModel } from '@/model/textPolish';
import type {
    ProcessTextRequestModel,
    ProcessTextResponseModel,
    TranscribeRequestModel,
    TranscribeResponseModel
} from '@/model/voicePolish';

// 非客户端环境触发快捷键语音转换时的统一提示文案。
export const CLIENT_UNAVAILABLE_VOICE_MESSAGE = '当前不在客户端，无法使用快捷键转换语音';
// 未连接到客户端桥接时，模型相关能力无法转交给本机客户端执行。
export const CLIENT_BRIDGE_UNAVAILABLE_MESSAGE = '未连接到 typesass 客户端，无法把请求转交给客户端执行。';
// 客户端为网页预览提供的本地 HTTP 桥接地址，只监听 127.0.0.1。
const CLIENT_HTTP_BRIDGE_BASE_URL = 'http://127.0.0.1:25818';
// 健康检查最大等待时间，避免 App 未启动时长期占用页面轮询。
const CLIENT_HTTP_HEALTH_TIMEOUT_MS = 1200;

// 判断当前是否运行在 Tauri 桌面端。
export function isTauriRuntime(): boolean {
    return Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

/**
 * 检查 typesass 客户端 HTTP 桥接是否健康。
 * 流程：请求 App 启动的 `/health` 端点，成功且返回 ok=true 时认为已连接。
 * 参数：无。
 * 返回：客户端 HTTP 桥接是否可请求。
 * 边界：客户端未启动、超时、端口被占用或响应内容不匹配时返回 false。
 */
export async function checkClientHttpBridgeHealth(): Promise<boolean> {
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), CLIENT_HTTP_HEALTH_TIMEOUT_MS);
    try {
        const response = await fetch(`${CLIENT_HTTP_BRIDGE_BASE_URL}/health`, {
            method: 'GET',
            cache: 'no-store',
            signal: controller.signal
        });
        if (!response.ok) return false;
        const responseJson = (await response.json()) as { ok?: unknown; name?: unknown };
        return responseJson.ok === true && responseJson.name === 'typesass-client-bridge';
    } catch {
        return false;
    } finally {
        window.clearTimeout(timer);
    }
}

/**
 * 读取 App HTTP 桥接 OpenAPI 文档。
 * 流程：请求客户端 `/openapi.json`，拿到当前 App 真实支持的 HTTP 接口清单。
 * 参数：无。
 * 返回：OpenAPI 文档模型。
 * 边界：客户端未启动、端口不可达或文档端点异常时抛出统一桥接错误。
 */
export async function readClientHttpBridgeOpenApi(): Promise<HttpApiOpenApiDocumentModel> {
    return requestClientHttpBridge<HttpApiOpenApiDocumentModel>('/openapi.json');
}

// 调用原生诊断命令读取当前权限和快捷键状态。
export async function getRuntimeDiagnostics(): Promise<RuntimeDiagnosticsModel | null> {
    return requestClientHttpBridge<RuntimeDiagnosticsModel>('/runtime-diagnostics');
}

// 注册新的全局快捷键配置；客户端保存并立即重新注册。
export async function registerShortcuts(shortcuts: ShortcutProfileModel): Promise<ShortcutProfileModel> {
    return requestClientHttpBridge<ShortcutProfileModel>('/register-shortcuts', shortcuts);
}

// 临时暂停全局快捷键注册，避免录制新快捷键时被系统级快捷键抢先触发。
export async function suspendShortcutsForRecording(): Promise<void> {
    await requestClientHttpBridge<void>('/suspend-shortcuts-for-recording');
}

// 打开系统麦克风权限设置。
export async function openMicrophoneSettings(): Promise<void> {
    await requestClientHttpBridge<void>('/open-microphone-settings');
}

// 打开系统辅助功能权限设置。
export async function openAccessibilitySettings(): Promise<void> {
    await requestClientHttpBridge<void>('/open-accessibility-settings');
}

// 设置开机自动启动。
export async function setLoginLaunch(enabled: boolean): Promise<void> {
    await requestClientHttpBridge<void>('/set-login-launch', { enabled });
}

// 查询开机自动启动状态。
export async function getLoginLaunch(): Promise<boolean> {
    return requestClientHttpBridge<boolean>('/get-login-launch');
}

// 保存 API Key 到原生会话和钥匙串。
export async function saveApiKey(apiKey: string): Promise<void> {
    await requestClientHttpBridge<void>('/save-api-key', { apiKey });
}

// 执行音频转写；由 Web 通过客户端本地 HTTP 桥接交给 App 执行。
export async function transcribeAudio(request: TranscribeRequestModel): Promise<TranscribeResponseModel> {
    return requestClientHttpBridge<TranscribeResponseModel>('/transcribe-audio', request);
}

// 执行文本润色或语音文本整理；由 Web 通过客户端本地 HTTP 桥接交给 App 执行。
export async function processText(request: ProcessTextRequestModel): Promise<ProcessTextResponseModel> {
    return requestClientHttpBridge<ProcessTextResponseModel>('/process-text', request);
}

/**
 * 通过本地 HTTP 桥接请求已启动的 typesass 客户端。
 * 流程：网页预览环境把模型请求 POST 到客户端本地端口，客户端再执行原生命令和真实大模型请求。
 * 参数：path 为桥接端点路径；payload 为要交给客户端的业务请求。
 * 返回：客户端桥接返回的业务响应。
 * 边界：客户端未启动、端口不可达或客户端返回错误时抛出可展示的错误文案。
 */
export async function requestClientHttpBridge<ResponseModel>(path: string, payload?: unknown): Promise<ResponseModel> {
    try {
        const response = await fetch(`${CLIENT_HTTP_BRIDGE_BASE_URL}${path}`, {
            method: payload === undefined ? 'GET' : 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: payload === undefined ? undefined : JSON.stringify(payload)
        });
        const responseText = await response.text();
        const responseJson = responseText ? (JSON.parse(responseText) as { error?: string }) : {};
        if (!response.ok) {
            throw new Error(responseJson.error || CLIENT_BRIDGE_UNAVAILABLE_MESSAGE);
        }
        return responseJson as ResponseModel;
    } catch (error) {
        if (error instanceof Error && error.message !== 'Failed to fetch') {
            throw error;
        }
        throw new Error(CLIENT_BRIDGE_UNAVAILABLE_MESSAGE);
    }
}

// 读取系统当前选中文本。
export async function readSelectedText(): Promise<SelectedTextResponseModel> {
    return requestClientHttpBridge<SelectedTextResponseModel>('/read-selected-text', {});
}

// 将处理结果粘贴回目标应用。
export async function pasteText(text: string, targetApp: string): Promise<void> {
    await requestClientHttpBridge<void>('/paste-text', { text, targetApp });
}

// 显示独立字幕窗口。
export async function showSubtitleWindows(): Promise<void> {
    await requestClientHttpBridge<void>('/show-subtitle-windows');
}

// 隐藏独立字幕窗口。
export async function hideSubtitleWindows(): Promise<void> {
    await requestClientHttpBridge<void>('/hide-subtitle-windows');
}

// 读取本地会话管理聚合数据。
export async function loadSessionWorkspaceData(projectId?: string): Promise<SessionWorkspaceDataModel> {
    return requestClientHttpBridge<SessionWorkspaceDataModel>('/load-session-workspace-data', { projectId });
}

// 创建本地项目并绑定工作空间。
export async function createSessionProject(
    request: CreateSessionProjectRequestModel
): Promise<SessionWorkspaceDataModel> {
    return requestClientHttpBridge<SessionWorkspaceDataModel>('/create-session-project', request);
}

// 创建本地任务卡片，初始状态保持已创建。
export async function createSessionTask(request: CreateSessionTaskRequestModel): Promise<SessionWorkspaceDataModel> {
    return requestClientHttpBridge<SessionWorkspaceDataModel>('/create-session-task', request);
}

// 将任务推入排队并由客户端自动创建 CodeX 会话。
export async function queueSessionTask(taskId: string): Promise<SessionWorkspaceDataModel> {
    return requestClientHttpBridge<SessionWorkspaceDataModel>('/queue-session-task', { taskId });
}

// 将待验收任务标记为已完成。
export async function completeSessionTask(taskId: string): Promise<SessionWorkspaceDataModel> {
    return requestClientHttpBridge<SessionWorkspaceDataModel>('/complete-session-task', { taskId });
}

// 恢复任务管理最新表结构，并清空任务、会话和项目业务数据。
export async function resetSessionTaskSchema(): Promise<SessionWorkspaceDataModel> {
    return requestClientHttpBridge<SessionWorkspaceDataModel>('/reset-session-task-schema', {});
}

// 打开任务或会话绑定的 CodeX thread。
export async function openSessionExternalThread(threadId: string): Promise<string> {
    return requestClientHttpBridge<string>('/open-session-external-thread', { threadId });
}

// 读取 CodeX 当前可见工作空间，用于会话管理和新建项目快捷绑定。
export async function listCodexWorkspaces(): Promise<CodexWorkspaceModel[]> {
    return requestClientHttpBridge<CodexWorkspaceModel[]>('/list-codex-workspaces');
}

// 读取 CodeX 指定工作空间下已有会话。
export async function listCodexThreads(request: CodexThreadListRequestModel): Promise<CodexThreadSummaryModel[]> {
    return requestClientHttpBridge<CodexThreadSummaryModel[]>('/list-codex-threads', request);
}

// 广播字幕窗口当前展示文本。
export async function emitSubtitleMessage(payload: SubtitleMessagePayloadModel): Promise<void> {
    window.dispatchEvent(new CustomEvent('subtitle-message', { detail: payload }));
}

// 广播字幕历史窗口更新。
export async function emitSubtitleHistory(payload: SubtitleHistoryUpdatePayloadModel): Promise<void> {
    window.dispatchEvent(new CustomEvent('subtitle-history-updated', { detail: payload }));
}

// 监听窗口广播事件；跨窗口原生事件不属于本次四模块范围。
export async function listenEvent<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
    const listener = (message: Event) => {
        handler((message as CustomEvent<T>).detail);
    };
    window.addEventListener(event, listener);
    return () => window.removeEventListener(event, listener);
}
