/** Codex Desktop 连接接口返回的稳定状态。 */
export type CodexConnectionApiStateType = 'connected' | 'disconnected' | 'restarting' | 'blocked' | 'unsupported';

/** 前端展示使用的 Codex 连接状态，额外包含首次检测和 HTTP 状态未知。 */
export type CodexConnectionStateType = CodexConnectionApiStateType | 'checking' | 'unknown';

/** Codex 连接弹窗当前展示的操作结果。 */
export type CodexConnectionDialogResultType = 'status' | 'success' | 'failure';

/** Codex Desktop 公开连接状态响应。 */
export interface CodexConnectionStatusModel {
    /** 服务端确认的连接状态。 */
    state: CodexConnectionApiStateType;
    /** 当前是否可由 Codex Desktop 原生创建新会话并发送任务。 */
    connected: boolean;
    /** 当前是否检测到 Codex Desktop 主程序正在运行。 */
    desktopRunning: boolean;
    /** 当前状态是否允许用户发起重启。 */
    canRestart: boolean;
    /** 当前状态对应的稳定原因码。 */
    reasonCode: string;
    /** 服务端提供的安全诊断说明，不包含端口、进程或本机路径。 */
    message: string;
    /** 服务端完成本次探测的 Unix 毫秒十进制字符串，避免跨语言整数精度损失。 */
    checkedAt: string;
}

/** Codex Desktop 重启请求的异步接受响应。 */
export interface CodexConnectionRestartAcceptedModel {
    /** 服务端是否已接受本次重启请求。 */
    accepted: boolean;
    /** 接受请求后的连接状态，通常为重启中。 */
    state: CodexConnectionApiStateType;
}

/** Codex Desktop 未连接时由任务写接口返回的稳定业务错误码。 */
export const CODEX_DESKTOP_NOT_CONNECTED_ERROR_CODE = 'CODEX_DESKTOP_NOT_CONNECTED';

/** Codex Desktop 已有重启流程执行时返回的稳定业务错误码。 */
export const CODEX_RESTART_IN_PROGRESS_ERROR_CODE = 'CODEX_RESTART_IN_PROGRESS';
