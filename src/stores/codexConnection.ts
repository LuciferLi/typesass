import { defineStore } from 'pinia';

import type {
    CodexConnectionDialogResultType,
    CodexConnectionStateType,
    CodexConnectionStatusModel
} from '@/model/codexConnection';
import { CODEX_DESKTOP_NOT_CONNECTED_ERROR_CODE, CODEX_RESTART_IN_PROGRESS_ERROR_CODE } from '@/model/codexConnection';
import {
    getCodexConnectionStatus,
    hasPublicApiToken,
    isPublicApiRequestErrorCode,
    restartCodexConnection
} from '@/service/tauri/command';

/** 页面可见时的 Codex 连接轮询间隔，满足侧栏近实时反馈。 */
const VISIBLE_CONNECTION_POLL_INTERVAL_MS = 2_000;
/** 页面隐藏时的 Codex 连接轮询间隔，降低后台资源消耗。 */
const HIDDEN_CONNECTION_POLL_INTERVAL_MS = 30_000;
/** 重启请求被接受后等待 Codex 恢复连接的最长时间，防止网络异常永久锁住弹窗。 */
const RESTART_RESULT_TIMEOUT_MS = 90_000;

let connectionPollingTimer: number | undefined;
let restartResultDeadlineTimer: number | undefined;
let connectionVisibilityListener: (() => void) | undefined;
let connectionPollingCallback: (() => void) | undefined;
let activeConnectionRequest: Promise<void> | null = null;

/** Codex 连接全局状态，由主布局和任务写操作共享同一个弹窗与轮询结果。 */
interface CodexConnectionState {
    /** 当前前端连接状态。 */
    connectionState: CodexConnectionStateType;
    /** 最近一次服务端是否确认连接成功。 */
    connected: boolean;
    /** 最近一次服务端是否确认桌面程序正在运行。 */
    desktopRunning: boolean;
    /** 最近一次服务端是否允许重启。 */
    canRestart: boolean;
    /** 最近一次服务端稳定原因码。 */
    reasonCode: string;
    /** 最近一次安全状态说明或前端轮询错误提示。 */
    message: string;
    /** 最近一次服务端探测时间，使用十进制字符串保留完整精度。 */
    checkedAt: string;
    /** 是否正在执行连接状态 HTTP 请求。 */
    requestInFlight: boolean;
    /** 当前 Hub 生命周期是否已经自动展示过断连提示。 */
    outageDialogShown: boolean;
    /** Codex 连接说明弹窗是否打开。 */
    dialogOpen: boolean;
    /** 弹窗当前展示普通状态、重启成功或重启失败。 */
    dialogResult: CodexConnectionDialogResultType;
    /** 重启失败时展示的安全错误说明。 */
    restartErrorMessage: string;
    /** 是否已接受重启并等待状态轮询给出最终结果。 */
    restartAwaitingResult: boolean;
    /** 当前重启等待的绝对截止时间；没有重启流程时为 null。 */
    restartDeadlineAt: number | null;
    /** 当前重启是否已超过第一方等待期限，用于阻止重复 restarting 快照重新锁住弹窗。 */
    restartWaitTimedOut: boolean;
    /** 是否正在提交重启 HTTP 请求，用于阻止旧轮询响应覆盖重启中状态。 */
    restartSubmitting: boolean;
    /** 当前主布局是否已经启动轮询。 */
    pollingStarted: boolean;
}

/**
 * 按当前页面可见性重新创建 Codex 连接轮询定时器。
 * 流程：清理旧定时器，再以可见 2 秒、隐藏 30 秒的间隔调用共享刷新回调。
 * 参数：无。
 * 返回：无返回值。
 * 边界：轮询尚未启动时只清理旧定时器，不创建空回调定时器。
 */
function resetConnectionPollingTimer(): void {
    if (connectionPollingTimer !== undefined) window.clearInterval(connectionPollingTimer);
    connectionPollingTimer = undefined;
    if (!connectionPollingCallback) return;
    const intervalMs = document.hidden ? HIDDEN_CONNECTION_POLL_INTERVAL_MS : VISIBLE_CONNECTION_POLL_INTERVAL_MS;
    connectionPollingTimer = window.setInterval(connectionPollingCallback, intervalMs);
}

/**
 * 创建 Codex Desktop 连接 Store。
 * 流程：通过公开 HTTP 接口维护单飞轮询、断连周期提示和异步重启结果。
 * 返回：Pinia Store 定义。
 * 边界：HTTP 请求异常只进入未知状态，不把网络故障伪报为 Codex 明确断连。
 */
export const useCodexConnectionStore = defineStore('codexConnection', {
    state: (): CodexConnectionState => ({
        connectionState: 'checking',
        connected: false,
        desktopRunning: false,
        canRestart: false,
        reasonCode: '',
        message: '',
        checkedAt: '',
        requestInFlight: false,
        outageDialogShown: false,
        dialogOpen: false,
        dialogResult: 'status',
        restartErrorMessage: '',
        restartAwaitingResult: false,
        restartDeadlineAt: null,
        restartWaitTimedOut: false,
        restartSubmitting: false,
        pollingStarted: false
    }),
    actions: {
        /**
         * 启动一次有界的 Codex 重启结果等待。
         * 流程：保留首次重启的绝对截止时间，创建独立计时器；到期仍未收到成功或明确失败时转为可关闭、可重试的失败态。
         * 参数：无。
         * 返回：无返回值。
         * 边界：重复的 restarting 轮询不会延长截止时间；旧计时器会在创建新一轮重启前清理。
         */
        startRestartResultDeadline(): void {
            if (this.restartAwaitingResult && this.restartDeadlineAt !== null) return;
            if (restartResultDeadlineTimer !== undefined) window.clearTimeout(restartResultDeadlineTimer);
            this.restartAwaitingResult = true;
            this.restartDeadlineAt = Date.now() + RESTART_RESULT_TIMEOUT_MS;
            this.restartWaitTimedOut = false;
            restartResultDeadlineTimer = window.setTimeout(() => {
                restartResultDeadlineTimer = undefined;
                if (!this.restartAwaitingResult || this.restartDeadlineAt === null) return;
                this.restartAwaitingResult = false;
                this.restartDeadlineAt = null;
                this.restartWaitTimedOut = true;
                this.restartSubmitting = false;
                this.connectionState = 'unknown';
                this.connected = false;
                this.dialogResult = 'failure';
                this.restartErrorMessage = 'Codex 重启超过 90 秒仍未恢复连接，请重新检测状态或再次重试。';
                this.dialogOpen = true;
                this.outageDialogShown = true;
            }, RESTART_RESULT_TIMEOUT_MS);
        },

        /**
         * 结束当前 Codex 重启结果等待并释放截止计时器。
         * 流程：清除模块级计时器、绝对截止时间和等待标记，恢复弹窗关闭与后续重试能力。
         * 参数：无。
         * 返回：无返回值。
         * 边界：允许在没有活动计时器时重复调用，不改变当前连接结果展示。
         */
        clearRestartResultDeadline(): void {
            if (restartResultDeadlineTimer !== undefined) window.clearTimeout(restartResultDeadlineTimer);
            restartResultDeadlineTimer = undefined;
            this.restartAwaitingResult = false;
            this.restartDeadlineAt = null;
        },

        /**
         * 应用服务端确认的 Codex 连接快照。
         * 流程：同步全部安全字段；重启等待态根据最终连接或断连转为成功或失败。
         * 参数：status 为公开 HTTP 接口返回的完整连接状态；allowAutoOpen 控制明确断连时是否允许自动弹窗。
         * 返回：无返回值。
         * 边界：只有 connected=true 才认定连接成功；重启中不会触发断连弹窗。
         */
        applyConnectionStatus(status: CodexConnectionStatusModel, allowAutoOpen: boolean): void {
            this.connectionState = status.connected ? 'connected' : status.state;
            this.connected = status.connected;
            this.desktopRunning = status.desktopRunning;
            this.canRestart = status.canRestart;
            this.reasonCode = status.reasonCode;
            this.message = status.message;
            this.checkedAt = status.checkedAt;

            if (status.connected) {
                if (this.restartAwaitingResult) {
                    this.clearRestartResultDeadline();
                    this.dialogResult = 'success';
                    this.dialogOpen = true;
                } else if (this.restartWaitTimedOut) {
                    this.restartWaitTimedOut = false;
                    this.dialogResult = 'success';
                    this.restartErrorMessage = '';
                    this.dialogOpen = true;
                }
                return;
            }

            if (status.state === 'restarting') {
                if (this.restartWaitTimedOut) return;
                this.startRestartResultDeadline();
                return;
            }

            if (this.restartAwaitingResult) {
                this.clearRestartResultDeadline();
                this.dialogResult = 'failure';
                this.restartErrorMessage = status.message || 'Codex 重启后仍未恢复连接。';
                this.dialogOpen = true;
                this.outageDialogShown = true;
                return;
            }

            if (allowAutoOpen && !this.outageDialogShown) {
                this.dialogResult = 'status';
                this.dialogOpen = true;
                this.outageDialogShown = true;
            }
        },

        /**
         * 执行一次 Codex 连接状态 HTTP 请求。
         * 流程：先复用公共请求层检查当前运行会话是否持有 Token；未授权时停止在本地，不产生 401 轮询；已授权才读取公开连接接口并应用权威快照。
         * 参数：allowAutoOpen 控制本次明确断连是否允许自动展示弹窗。
         * 返回：请求完成 Promise。
         * 边界：无 Token 或异常均不会清空最近一次桌面运行和可重启信息；设备授权成功后下一轮会自动恢复 HTTP 查询。
         */
        async performConnectionRefresh(allowAutoOpen: boolean): Promise<void> {
            this.requestInFlight = true;
            try {
                if (!(await hasPublicApiToken())) {
                    this.connectionState = 'unknown';
                    this.connected = false;
                    this.message = '当前 Web 会话尚未授权，暂时无法获取 Codex 连接状态。';
                    return;
                }
                this.applyConnectionStatus(await getCodexConnectionStatus(), allowAutoOpen);
            } catch (error) {
                this.connectionState = 'unknown';
                this.connected = false;
                this.message = error instanceof Error ? error.message : '暂时无法获取 Codex 连接状态。';
            } finally {
                this.requestInFlight = false;
            }
        },

        /**
         * 单飞刷新 Codex 连接状态。
         * 流程：已有请求时复用同一个 Promise，否则启动一次真实 HTTP 请求并在结束后释放单飞槽。
         * 参数：allowAutoOpen 默认为 true，用于启动和轮询时自动提示明确断连。
         * 返回：当前单飞请求完成 Promise。
         * 边界：并发的页面恢复、定时轮询和业务错误刷新不会产生重叠 HTTP 请求。
         */
        async refreshConnection(allowAutoOpen = true): Promise<void> {
            if (this.restartSubmitting) return;
            if (activeConnectionRequest) {
                await activeConnectionRequest;
                return;
            }
            activeConnectionRequest = this.performConnectionRefresh(allowAutoOpen);
            try {
                await activeConnectionRequest;
            } finally {
                activeConnectionRequest = null;
            }
        },

        /**
         * 启动主布局生命周期内的 Codex 连接轮询。
         * 流程：立即检查一次，注册页面可见性监听，并按前台 2 秒、后台 30 秒持续单飞刷新。
         * 参数：无。
         * 返回：无返回值。
         * 边界：重复调用不会注册多个监听器或定时器。
         */
        startPolling(): void {
            if (this.pollingStarted) return;
            this.pollingStarted = true;
            connectionPollingCallback = () => {
                void this.refreshConnection(true);
            };
            connectionVisibilityListener = () => {
                resetConnectionPollingTimer();
                if (!document.hidden) void this.refreshConnection(true);
            };
            document.addEventListener('visibilitychange', connectionVisibilityListener);
            resetConnectionPollingTimer();
            void this.refreshConnection(true);
        },

        /**
         * 停止 Codex 连接轮询。
         * 流程：清理定时器和页面可见性监听，释放模块级回调引用。
         * 参数：无。
         * 返回：无返回值。
         * 边界：不会中断已经发出的 HTTP 请求，请求完成后只更新仍存活的 Pinia 状态。
         */
        stopPolling(): void {
            if (connectionPollingTimer !== undefined) window.clearInterval(connectionPollingTimer);
            if (connectionVisibilityListener) {
                document.removeEventListener('visibilitychange', connectionVisibilityListener);
            }
            connectionPollingTimer = undefined;
            connectionVisibilityListener = undefined;
            connectionPollingCallback = undefined;
            this.pollingStarted = false;
        },

        /**
         * 打开 Codex 连接说明弹窗。
         * 流程：用户点击侧栏状态时始终回到当前连接状态说明；重启等待期间保留重启中视图。
         * 参数：无。
         * 返回：无返回值。
         * 边界：不会发起状态修改或重启请求。
         */
        openDialog(): void {
            if (!this.restartAwaitingResult) this.dialogResult = 'status';
            this.dialogOpen = true;
        },

        /**
         * 更新 Codex 连接弹窗开关。
         * 流程：普通状态允许关闭；正在等待重启结果时拒绝关闭，防止用户误以为重启已结束。
         * 参数：open 为 Dialog 组件请求的新开关状态。
         * 返回：无返回值。
         * 边界：重启完成或失败后恢复正常关闭能力。
         */
        setDialogOpen(open: boolean): void {
            if (!open && this.restartAwaitingResult) return;
            this.dialogOpen = open;
        },

        /**
         * 请求 HTTP 服务异步重启 Codex Desktop。
         * 流程：先进入最长 90 秒的重启等待并锁定重复操作，提交 POST 接口后立即刷新状态，最终结果由后续轮询或截止计时器确认。
         * 参数：无。
         * 返回：重启请求和首次状态确认完成 Promise。
         * 边界：服务端报告已有重启时继续等待同一有界流程；其它错误进入失败态且不自动重放写请求。
         */
        async restartConnection(): Promise<void> {
            if (this.restartAwaitingResult || !this.canRestart) return;
            if (activeConnectionRequest) await activeConnectionRequest;
            if (this.restartAwaitingResult || !this.canRestart) return;
            this.restartSubmitting = true;
            this.startRestartResultDeadline();
            this.connectionState = 'restarting';
            this.connected = false;
            this.dialogResult = 'status';
            this.restartErrorMessage = '';
            try {
                const response = await restartCodexConnection();
                if (!response.accepted) throw new Error('Codex 重启请求未被服务端接受。');
                this.restartSubmitting = false;
                this.connectionState = response.state;
                await this.refreshConnection(false);
            } catch (error) {
                this.restartSubmitting = false;
                if (isPublicApiRequestErrorCode(error, CODEX_RESTART_IN_PROGRESS_ERROR_CODE)) {
                    this.connectionState = 'restarting';
                    return;
                }
                this.clearRestartResultDeadline();
                this.connectionState = 'unknown';
                this.dialogResult = 'failure';
                this.restartErrorMessage = error instanceof Error ? error.message : 'Codex 重启失败，请稍后重试。';
                this.dialogOpen = true;
            }
        },

        /**
         * 处理任务写接口确认的 Codex Desktop 未连接错误。
         * 流程：立即把侧栏切到未连接并打开共享弹窗，再单飞刷新连接详情和可重启能力。
         * 参数：message 为 HTTP 错误 envelope 生成的安全说明。
         * 返回：无返回值。
         * 边界：不修改任务列表或任务状态，原写请求也不会自动重放。
         */
        markDisconnectedFromBusinessError(message: string): void {
            this.connectionState = 'disconnected';
            this.connected = false;
            this.reasonCode = CODEX_DESKTOP_NOT_CONNECTED_ERROR_CODE;
            this.message = message;
            this.dialogResult = 'status';
            this.dialogOpen = true;
            this.outageDialogShown = true;
            void this.refreshConnection(false);
        }
    }
});
