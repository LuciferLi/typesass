import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import { CODEX_DESKTOP_NOT_CONNECTED_ERROR_CODE } from '@/model/codexConnection';
import type {
    CodexThreadSummaryModel,
    CodexWorkspaceModel,
    CreateSessionProjectRequestModel,
    CreateSessionTaskRequestModel,
    SessionManagePersistedStateModel,
    SessionProjectModel,
    SessionRecordModel,
    SessionTaskModel,
    SessionWorkspaceDataModel,
    UpdateSessionProjectRequestModel,
    UpdateSessionTaskRequestModel
} from '@/model/sessionManage';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import {
    completeSessionTask,
    createSessionProject,
    createSessionTask,
    deleteSessionProject,
    deleteSessionTask,
    isPublicApiRequestErrorCode,
    isTauriRuntime,
    listCodexThreads,
    listCodexWorkspaces,
    loadSessionWorkspaceData,
    openSessionExternalThread,
    queueSessionTask,
    updateSessionProject,
    updateSessionTask
} from '@/service/tauri/command';
import { useCodexConnectionStore } from '@/stores/codexConnection';

const CODEX_THREAD_PAGE_SIZE = 30;
const TASK_WORKSPACE_REFRESH_INTERVAL_MS = 1_500;

/** 会话管理页面状态，只承载可从 CodeX 本地状态真实读取的数据。 */
interface SessionManageState {
    /** HTTP 服务返回的真实任务项目列表。 */
    projects: SessionProjectModel[];
    /** 当前项目的真实任务列表。 */
    tasks: SessionTaskModel[];
    /** 当前项目关联的真实会话列表。 */
    sessions: SessionRecordModel[];
    /** CodeX 工作空间列表。 */
    codexWorkspaces: CodexWorkspaceModel[];
    /** 当前工作空间的会话列表。 */
    codexThreads: CodexThreadSummaryModel[];
    /** 当前搜索关键词。 */
    codexThreadKeyword: string;
    /** 下一页偏移量。 */
    codexThreadOffset: number;
    /** 是否还有下一页。 */
    hasMoreCodexThreads: boolean;
    /** 当前工作空间路径。 */
    selectedWorkspaceCwd: string;
    /** 当前任务项目 ID，仅用于读取上下文。 */
    selectedProjectId: string;
    /** 首屏加载状态。 */
    loading: boolean;
    /** 追加加载状态。 */
    loadingMoreThreads: boolean;
    /** 是否正在提交任务写操作。 */
    saving: boolean;
    /** HTTP 服务是否已成功返回 Rust 权威任务数据。 */
    workspaceDataReady: boolean;
    /** 每次采用权威任务聚合后递增，用于丢弃晚到的旧轮询响应。 */
    workspaceDataRevision: number;
    /** 可排障提示，仅用于加载失败或状态不可用等需要持续展示的页面状态。 */
    message: string;
}

/**
 * 创建真实 CodeX 会话浏览 Store。
 * 流程：只读工作空间和会话，允许搜索、分页与打开，不创建无法观测终态的自动任务。
 * 返回：Pinia Store 定义。
 * 边界：CodeX 本地状态不可读时显示空列表和明确错误，不伪造项目或任务状态。
 */
export const useSessionManageStore = defineStore('sessionManage', {
    state: (): SessionManageState => ({
        projects: [],
        tasks: [],
        sessions: [],
        codexWorkspaces: [],
        codexThreads: [],
        codexThreadKeyword: '',
        codexThreadOffset: 0,
        hasMoreCodexThreads: false,
        selectedWorkspaceCwd: '',
        selectedProjectId: '',
        loading: false,
        loadingMoreThreads: false,
        saving: false,
        workspaceDataReady: false,
        workspaceDataRevision: 0,
        message: ''
    }),
    getters: {
        /** 读取当前任务项目；ID 为空代表全部视图，ID 不存在时返回 null，禁止猜测项目归属。 */
        selectedProject: (state): SessionProjectModel | null =>
            state.projects.find((project) => project.id === state.selectedProjectId) ?? null
    },
    actions: {
        /**
         * 初始化会话浏览。
         * 流程：桌面端读取上次工作空间，普通 Web 保留当前页面选择；随后刷新真实工作空间列表并读取第一页会话。
         * 返回：初始化完成 Promise。
         * 边界：普通 Web 不调用客户端配置 IPC；HTTP 失败会清空不可确认的数据并保存错误信息。
         */
        async initSessionManage(): Promise<void> {
            this.loading = true;
            try {
                const saved = isTauriRuntime()
                    ? await readClientJson<SessionManagePersistedStateModel>(StorageKey.sessionManage, {
                          selectedProjectId: '',
                          selectedWorkspaceCwd: ''
                      })
                    : { selectedProjectId: '', selectedWorkspaceCwd: this.selectedWorkspaceCwd };
                this.selectedWorkspaceCwd = saved.selectedWorkspaceCwd;
                await this.refreshCodexWorkspaces();
                await this.persistSelection();
                await this.refreshCodexThreads(undefined, true);
                this.message = '';
            } catch (error) {
                this.codexWorkspaces = [];
                this.codexThreads = [];
                this.message = error instanceof Error ? error.message : '读取 CodeX 会话失败。';
            } finally {
                this.loading = false;
            }
        },

        /**
         * 初始化任务管理真实数据。
         * 流程：桌面端读取上次项目选择，普通 Web 保留页面内存选择；再并行加载任务聚合和 CodeX 工作空间数据。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：HTTP 失败时清空任务数据并标记未就绪，不回退 IPC 或旧缓存。
         */
        async initTaskManage(): Promise<void> {
            this.loading = true;
            this.workspaceDataReady = false;
            try {
                const saved = isTauriRuntime()
                    ? await readClientJson<SessionManagePersistedStateModel>(StorageKey.sessionManage, {
                          selectedProjectId: '',
                          selectedWorkspaceCwd: this.selectedWorkspaceCwd
                      })
                    : { selectedProjectId: this.selectedProjectId, selectedWorkspaceCwd: this.selectedWorkspaceCwd };
                this.selectedProjectId = saved.selectedProjectId ?? '';
                const [workspaceData] = await Promise.all([
                    loadSessionWorkspaceData(this.selectedProjectId || undefined),
                    this.refreshCodexWorkspaces()
                ]);
                this.applyWorkspaceData(workspaceData);
                await this.persistSelection();
                this.message = '';
            } catch (error) {
                this.projects = [];
                this.tasks = [];
                this.sessions = [];
                this.selectedProjectId = '';
                this.workspaceDataReady = false;
                this.message = error instanceof Error ? error.message : '读取任务数据失败。';
            } finally {
                this.loading = false;
            }
        },

        /**
         * 应用 HTTP 返回的权威任务聚合数据。
         * 流程：原子替换三个列表；项目 ID 为空时保持“全部”视图，显式项目缺失时回到“全部”。
         * 参数：data 为 HTTP 服务从 Rust 任务库取得的真实数据。
         * 返回：无返回值。
         * 边界：不会在前端补造任务、会话或状态；“全部”视图不猜测任务创建归属。
         */
        applyWorkspaceData(data: SessionWorkspaceDataModel): void {
            this.projects = data.projects;
            if (!this.projects.some((project) => project.id === this.selectedProjectId)) {
                this.selectedProjectId = '';
            }
            this.tasks = data.tasks;
            this.sessions = data.sessions;
            const project = this.projects.find((item) => item.id === this.selectedProjectId);
            if (project) this.selectedWorkspaceCwd = project.workspacePath;
            this.workspaceDataReady = true;
            this.workspaceDataRevision += 1;
        },

        /**
         * 切换任务项目。
         * 流程：按项目 ID 重新请求 HTTP 聚合数据；项目 ID 为空时读取全部视图，再持久化当前页面选择。
         * 参数：projectId 为真实项目 ID，空字符串代表全部项目。
         * 返回：切换完成 Promise。
         * 异常：读取失败时透传，保留服务端之前确认的数据。
         */
        async selectProject(projectId: string): Promise<void> {
            const previousProjectId = this.selectedProjectId;
            try {
                this.selectedProjectId = projectId;
                this.applyWorkspaceData(await loadSessionWorkspaceData(projectId || undefined));
                await this.persistSelection();
                this.message = '';
            } catch (error) {
                this.selectedProjectId = previousProjectId;
                this.message = error instanceof Error ? error.message : '切换任务项目失败。';
                throw error;
            }
        },

        /**
         * 创建真实任务项目。
         * 流程：调用 HTTP，由 Rust 事务返回列表识别新增项目并保存当前选择。
         * 参数：request 为项目名称和工作空间路径。
         * 返回：创建完成 Promise。
         * 异常：HTTP 或 Rust 权威校验失败时透传，前端不插入临时项目。
         */
        async addProject(request: CreateSessionProjectRequestModel): Promise<void> {
            this.saving = true;
            try {
                const previousIds = new Set(this.projects.map((project) => project.id));
                const data = await createSessionProject(request);
                const created = data.projects.find((project) => !previousIds.has(project.id));
                this.selectedProjectId = created?.id ?? this.selectedProjectId;
                this.applyWorkspaceData(data);
                await this.persistSelection();
                this.message = '';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 编辑真实任务项目。
         * 流程：调用 HTTP 更新接口，由 Rust 事务提交后完整采用返回聚合数据并保存当前选择。
         * 参数：request 为项目 ID、名称和后续工作空间。
         * 返回：编辑完成 Promise。
         * 异常：HTTP 或 Rust 权威校验失败时透传，前端保留已确认项目内容。
         */
        async editProject(request: UpdateSessionProjectRequestModel): Promise<void> {
            this.saving = true;
            try {
                this.selectedProjectId = request.id;
                this.applyWorkspaceData(await updateSessionProject(request));
                await this.persistSelection();
                this.message = '';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 删除当前项目。
         * 流程：调用 HTTP 删除接口，由 Rust 事务软删除项目，再采用返回列表并持久化新的选择。
         * 参数：projectId 为待删除项目稳定 ID。
         * 返回：删除完成 Promise。
         * 异常：项目不存在、已删除或事务失败时透传，前端不移除项目。
         */
        async removeProject(projectId: string): Promise<void> {
            this.saving = true;
            try {
                if (this.selectedProjectId === projectId) this.selectedProjectId = '';
                this.applyWorkspaceData(await deleteSessionProject(projectId));
                await this.persistSelection();
                this.message = '';
            } catch (error) {
                this.selectedProjectId = projectId;
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 创建真实任务卡片。
         * 流程：调用 HTTP，校验本次 createdTaskId 并完整采用 Rust 返回聚合数据，初始状态由任务库决定。
         * 参数：request 为项目 ID、标题和提示词。
         * 返回：创建完成 Promise。
         * 异常：失败时不修改任务列表，防止页面出现未落库卡片。
         */
        async addTask(request: CreateSessionTaskRequestModel): Promise<void> {
            this.saving = true;
            try {
                const response = await createSessionTask(request);
                if (!response.createdTaskId) throw new Error('HTTP 服务没有返回本次创建的任务 ID。');
                this.applyWorkspaceData(response);
                this.message = '';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 编辑真实任务卡片。
         * 流程：调用 HTTP 更新接口，由 Rust 状态机确认任务仍处于已创建或等待中，再采用返回聚合数据刷新看板。
         * 参数：request 为任务 ID、标题和提示词。
         * 返回：编辑完成 Promise。
         * 异常：状态不允许或写入失败时保留原任务内容并透传错误。
         */
        async editTask(request: UpdateSessionTaskRequestModel): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await updateSessionTask(request));
                this.message = '';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 删除真实任务卡片。
         * 流程：调用 HTTP 删除接口，由 Rust 事务拒绝进行中任务并返回删除后的聚合数据。
         * 参数：taskId 为待删除任务稳定 ID。
         * 返回：删除完成 Promise。
         * 异常：任务进行中、不存在或事务失败时保留原任务列表并透传错误。
         */
        async removeTask(taskId: string): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await deleteSessionTask(taskId));
                this.message = '';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 请求任务进入排队。
         * 流程：调用 HTTP 并用 Rust CAS 返回状态刷新看板，不在前端先改成 queued。
         * 参数：taskId 为真实任务 ID。
         * 返回：排队操作完成 Promise。
         * 异常：状态不允许或调度失败时保留原状态并透传错误。
         */
        async queueTask(taskId: string): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await queueSessionTask(taskId));
                this.message = '';
            } catch (error) {
                if (isPublicApiRequestErrorCode(error, CODEX_DESKTOP_NOT_CONNECTED_ERROR_CODE)) {
                    useCodexConnectionStore().markDisconnectedFromBusinessError(error.message);
                }
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 验收待验收任务。
         * 流程：调用 HTTP 并用 Rust 返回状态刷新看板，不在前端提前标记完成。
         * 参数：taskId 为真实任务 ID。
         * 返回：验收完成 Promise。
         * 异常：任务状态不匹配或落库失败时保留原状态并透传错误。
         */
        async completeTask(taskId: string): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await completeSessionTask(taskId));
                this.message = '';
            } finally {
                this.saving = false;
            }
        },

        /**
         * 切换 CodeX 工作空间。
         * 参数：workspaceCwd 为真实工作空间绝对路径。
         * 流程：更新选择、清空搜索、持久化后读取第一页会话。
         * 返回：切换完成 Promise。
         */
        async selectCodexWorkspace(workspaceCwd: string): Promise<void> {
            this.selectedWorkspaceCwd = workspaceCwd;
            this.codexThreadKeyword = '';
            await this.persistSelection();
            await this.refreshCodexThreads(workspaceCwd, true);
        },

        /**
         * 保存当前工作空间选择。
         * 流程：桌面端写客户端 JSON 的会话分区和真实 CodeX 工作空间路径；普通 Web 仅保留 Pinia 页面状态。
         * 返回：保存完成 Promise。
         * 边界：普通 Web 不调用配置 IPC 或写 Web Storage；所有端都不保存项目、任务等业务数据副本。
         */
        async persistSelection(): Promise<void> {
            if (!isTauriRuntime()) return;
            await writeClientJson<SessionManagePersistedStateModel>(StorageKey.sessionManage, {
                selectedProjectId: this.selectedProjectId,
                selectedWorkspaceCwd: this.selectedWorkspaceCwd
            });
        },

        /**
         * 在任务页存活期间跟踪 Rust 任务状态。
         * 流程：仅在存在排队中或执行中任务时，以固定有界间隔单飞重读当前项目或全部视图；响应返回后核对项目与数据版本，再采用数据库真实聚合数据。
         * 参数：无。
         * 返回：取消监听函数 Promise。
         * 边界：没有活动任务、已有请求或正在写操作时跳过当次；页面卸载、项目切换或期间完成写操作后丢弃旧响应；失败只更新错误信息，不回退 Tauri event/IPC。
         */
        async listenTaskUpdates(): Promise<() => void> {
            let requestInFlight = false;
            let stopped = false;
            const timer = window.setInterval(() => {
                const hasActiveTask = this.tasks.some((task) => task.status === 'queued' || task.status === 'running');
                if (!hasActiveTask || requestInFlight || this.loading || this.saving) return;
                const requestedProjectId = this.selectedProjectId;
                const requestedRevision = this.workspaceDataRevision;
                requestInFlight = true;
                void loadSessionWorkspaceData(requestedProjectId || undefined)
                    .then((data) => {
                        if (
                            stopped ||
                            this.selectedProjectId !== requestedProjectId ||
                            this.workspaceDataRevision !== requestedRevision
                        ) {
                            return;
                        }
                        this.applyWorkspaceData(data);
                    })
                    .catch((error: unknown) => {
                        if (stopped || this.selectedProjectId !== requestedProjectId) return;
                        this.message = error instanceof Error ? error.message : '刷新任务状态失败。';
                    })
                    .finally(() => {
                        requestInFlight = false;
                    });
            }, TASK_WORKSPACE_REFRESH_INTERVAL_MS);
            return () => {
                stopped = true;
                window.clearInterval(timer);
            };
        },

        /**
         * 打开真实 CodeX 会话。
         * 参数：threadId 为 CodeX 返回的 thread ID。
         * 流程：通过 HTTP 交给 Rust 校验 ID 并打开 CodeX deeplink。
         * 返回：打开完成 Promise。
         */
        async openExternalThread(threadId: string): Promise<void> {
            await openSessionExternalThread(threadId);
        },

        /**
         * 刷新真实 CodeX 工作空间。
         * 流程：通过 HTTP 请求 Rust 只读状态，并确保当前选择仍存在。
         * 返回：刷新完成 Promise。
         * 边界：读取失败向上抛出，由初始化入口统一展示错误。
         */
        async refreshCodexWorkspaces(): Promise<void> {
            this.codexWorkspaces = await listCodexWorkspaces();
            if (!this.codexWorkspaces.some((item) => item.cwd === this.selectedWorkspaceCwd)) {
                this.selectedWorkspaceCwd = this.codexWorkspaces[0]?.cwd ?? '';
            }
        },

        /**
         * 刷新或追加 CodeX 会话。
         * 参数：workspaceCwd 为可选工作空间，reset 表示是否重置分页。
         * 流程：按当前搜索词读取真实会话摘要并更新偏移量。
         * 返回：读取完成 Promise。
         * 边界：未选工作空间时返回空列表；读取失败清空列表并保留错误信息。
         */
        async refreshCodexThreads(workspaceCwd?: string, reset = true): Promise<void> {
            const workspacePath = workspaceCwd || this.selectedWorkspaceCwd;
            if (!workspacePath) {
                this.codexThreads = [];
                this.codexThreadOffset = 0;
                this.hasMoreCodexThreads = false;
                return;
            }
            try {
                const offset = reset ? 0 : this.codexThreadOffset;
                const limit = reset
                    ? Math.max(CODEX_THREAD_PAGE_SIZE, this.codexThreadOffset || CODEX_THREAD_PAGE_SIZE)
                    : CODEX_THREAD_PAGE_SIZE;
                const threads = await listCodexThreads({
                    workspaceCwd: workspacePath,
                    limit,
                    offset,
                    keyword: this.codexThreadKeyword
                });
                this.codexThreads = reset ? threads : [...this.codexThreads, ...threads];
                this.codexThreadOffset = offset + threads.length;
                this.hasMoreCodexThreads = threads.length >= CODEX_THREAD_PAGE_SIZE;
                this.message = '';
            } catch (error) {
                this.codexThreads = [];
                this.codexThreadOffset = 0;
                this.hasMoreCodexThreads = false;
                this.message = error instanceof Error ? error.message : '读取 CodeX 会话失败。';
            }
        },

        /**
         * 搜索当前工作空间会话。
         * 参数：keyword 为标题或 thread ID 关键词。
         * 流程：规范空白后重置分页并重新读取。
         * 返回：搜索完成 Promise。
         */
        async searchCodexThreads(keyword: string): Promise<void> {
            this.codexThreadKeyword = keyword.trim();
            await this.refreshCodexThreads(undefined, true);
        },

        /**
         * 加载下一页会话。
         * 流程：在有下一页且未加载时读取并追加，finally 恢复状态。
         * 返回：加载完成 Promise。
         * 边界：重复触发或无更多数据时直接返回。
         */
        async loadMoreCodexThreads(): Promise<void> {
            if (!this.hasMoreCodexThreads || this.loadingMoreThreads) return;
            this.loadingMoreThreads = true;
            try {
                await this.refreshCodexThreads(undefined, false);
            } finally {
                this.loadingMoreThreads = false;
            }
        }
    }
});
