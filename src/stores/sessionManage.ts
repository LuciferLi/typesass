import { defineStore } from 'pinia';

import { StorageKey } from '@/config/storageKey';
import type {
    CodexThreadSummaryModel,
    CodexWorkspaceModel,
    CreateSessionProjectRequestModel,
    CreateSessionTaskRequestModel,
    SessionManagePersistedStateModel,
    SessionProjectModel,
    SessionRecordModel,
    SessionTaskModel,
    SessionTaskStatusType,
    SessionWorkspaceDataModel
} from '@/model/sessionManage';
import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
import {
    completeSessionTask,
    createSessionProject,
    createSessionTask,
    listCodexThreads,
    listCodexWorkspaces,
    listenEvent,
    loadSessionWorkspaceData,
    openSessionExternalThread,
    queueSessionTask
} from '@/service/tauri/command';

// CodeX 会话列表每页读取数量，用于右侧会话列表加载更多。
const CODEX_THREAD_PAGE_SIZE = 30;

interface SessionManageState {
    // 本地项目列表。
    projects: SessionProjectModel[];
    // 当前项目下的任务列表。
    tasks: SessionTaskModel[];
    // 当前项目下的会话列表。
    sessions: SessionRecordModel[];
    // CodeX 侧工作空间列表。
    codexWorkspaces: CodexWorkspaceModel[];
    // 当前工作空间下 CodeX 侧会话列表。
    codexThreads: CodexThreadSummaryModel[];
    // 当前 CodeX 会话搜索关键词。
    codexThreadKeyword: string;
    // 当前 CodeX 会话列表已读取数量。
    codexThreadOffset: number;
    // 是否还有更多 CodeX 会话可加载。
    hasMoreCodexThreads: boolean;
    // 当前选中的 CodeX 工作空间路径。
    selectedWorkspaceCwd: string;
    // 当前选中的项目 ID。
    selectedProjectId: string;
    // 是否正在初始化本地数据。
    loading: boolean;
    // 是否正在追加加载 CodeX 会话。
    loadingMoreThreads: boolean;
    // 是否正在提交写操作。
    saving: boolean;
    // 页面提示文案。
    message: string;
    // 本地任务库是否已成功读取，用于避免客户端断开时展示热更新遗留数据。
    workspaceDataReady: boolean;
}

export const useSessionManageStore = defineStore('sessionManage', {
    state: (): SessionManageState => {
        return {
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
            message: '',
            workspaceDataReady: false
        };
    },
    getters: {
        // 当前选中的项目，用于页面标题、任务创建和会话过滤。
        selectedProject: (state): SessionProjectModel | null => {
            return state.projects.find((project) => project.id === state.selectedProjectId) ?? null;
        },
        // 当前选中的 CodeX 工作空间，用于会话管理页面读取右侧列表。
        selectedCodexWorkspace: (state): CodexWorkspaceModel | null => {
            return (
                state.codexWorkspaces.find((workspace) => workspace.cwd === state.selectedWorkspaceCwd) ??
                state.codexWorkspaces[0] ??
                null
            );
        },
        // 按任务状态分组后的看板数据。
        taskListByStatus:
            (state) =>
            (status: SessionTaskStatusType): SessionTaskModel[] => {
                return state.tasks.filter((task) => task.status === status);
            }
    },
    actions: {
        /**
         * 初始化会话管理数据。
         * 流程：先读取客户端 JSON 中的上次选中项，再读取本地 SQLite 聚合数据，同时尝试读取 CodeX 工作空间列表。
         * 参数：无。
         * 返回：初始化完成 Promise。
         * 边界：CodeX 工作空间读取失败不影响本地任务数据展示。
         */
        async initSessionManage(): Promise<void> {
            this.loading = true;
            this.workspaceDataReady = false;
            try {
                await this.hydrateSessionManageConfig();
                this.applyWorkspaceData(await loadSessionWorkspaceData(this.selectedProjectId || undefined));
                await this.refreshCodexWorkspaces();
                await this.persistSessionManageConfig();
                await this.refreshCodexThreads(undefined, true);
            } catch (error) {
                this.projects = [];
                this.tasks = [];
                this.sessions = [];
                this.selectedProjectId = '';
                this.workspaceDataReady = false;
                this.message = error instanceof Error ? error.message : '读取会话管理数据失败。';
            } finally {
                this.loading = false;
            }
        },

        /**
         * 应用 Rust 返回的聚合数据。
         * 流程：替换项目、任务、会话列表，并按项目 ID 或工作空间路径自动校正当前选中项目。
         * 参数：data 为本地 SQLite 聚合数据。
         * 返回：无返回值。
         * 边界：当前项目被恢复表结构清空后，选中项会重置为空。
         */
        applyWorkspaceData(data: SessionWorkspaceDataModel): void {
            this.projects = data.projects;
            const requestedProjectId = this.selectedProjectId;
            if (!this.projects.some((project) => project.id === this.selectedProjectId)) {
                const matchedProject = requestedProjectId
                    ? this.projects.find((project) => project.workspacePath === this.selectedWorkspaceCwd)
                    : null;
                this.selectedProjectId = matchedProject?.id ?? (this.projects.length === 1 ? this.projects[0].id : '');
            }
            this.tasks = this.selectedProjectId ? data.tasks : [];
            this.sessions = this.selectedProjectId ? data.sessions : [];
            this.selectedWorkspaceCwd = this.selectedProject?.workspacePath ?? this.selectedWorkspaceCwd;
            this.workspaceDataReady = true;
        },

        /**
         * 切换当前项目。
         * 流程：更新选中项目 ID 后读取该项目任务与会话，保存选择配置，再刷新对应 CodeX 会话。
         * 参数：projectId 为目标项目 ID。
         * 返回：切换完成 Promise。
         * 边界：重复选择当前项目时仍刷新数据，保证后台状态变化可见。
         */
        async selectProject(projectId: string): Promise<void> {
            this.selectedProjectId = projectId;
            this.codexThreadKeyword = '';
            this.applyWorkspaceData(await loadSessionWorkspaceData(projectId));
            this.selectedWorkspaceCwd = this.selectedProject?.workspacePath ?? this.selectedWorkspaceCwd;
            await this.persistSessionManageConfig();
            await this.refreshCodexThreads(undefined, true);
        },

        /**
         * 切换当前 CodeX 工作空间。
         * 流程：保存工作空间 cwd，再按该目录刷新 CodeX 原生会话列表。
         * 参数：workspaceCwd 为 CodeX 工作空间绝对路径。
         * 返回：切换完成 Promise。
         * 边界：重复选择同一工作空间仍刷新，保证外部会话变化可见。
         */
        async selectCodexWorkspace(workspaceCwd: string): Promise<void> {
            this.selectedProjectId = '';
            this.tasks = [];
            this.sessions = [];
            this.codexThreadKeyword = '';
            this.selectedWorkspaceCwd = workspaceCwd;
            await this.persistSessionManageConfig();
            await this.refreshCodexThreads(workspaceCwd, true);
        },

        /**
         * 创建项目。
         * 流程：调用 Tauri 写入 SQLite，并把新项目设为当前项目。
         * 参数：request 为项目名称和工作空间路径。
         * 返回：创建完成 Promise。
         * 边界：失败时保留原列表并展示错误。
         */
        async addProject(request: CreateSessionProjectRequestModel): Promise<void> {
            this.saving = true;
            try {
                const previousProjectIds = new Set(this.projects.map((project) => project.id));
                const data = await createSessionProject(request);
                const createdProject =
                    data.projects.find((project) => !previousProjectIds.has(project.id)) ??
                    data.projects.find(
                        (project) =>
                            project.name === request.name.trim() &&
                            project.workspacePath === request.workspacePath.trim()
                    ) ??
                    null;
                this.projects = data.projects;
                this.selectedProjectId = createdProject?.id ?? (data.projects.length === 1 ? data.projects[0].id : '');
                this.tasks = this.selectedProjectId ? data.tasks : [];
                this.sessions = this.selectedProjectId ? data.sessions : [];
                this.selectedWorkspaceCwd = createdProject?.workspacePath ?? this.selectedWorkspaceCwd;
                this.workspaceDataReady = true;
                await this.persistSessionManageConfig();
                this.message = '项目已创建。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '创建项目失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 从客户端 JSON 配置中恢复任务管理选择。
         * 流程：通过 Tauri 配置命令读取上次选中的项目 ID 和工作空间路径，写回当前 Store 状态。
         * 参数：无。
         * 返回：恢复完成 Promise。
         * 边界：配置不存在或网页预览环境时保持当前空选择，由后续项目列表兜底选择第一项。
         */
        async hydrateSessionManageConfig(): Promise<void> {
            const saved = await readClientJson<SessionManagePersistedStateModel>(StorageKey.sessionManage, {
                selectedProjectId: '',
                selectedWorkspaceCwd: ''
            });
            this.selectedProjectId = saved.selectedProjectId;
            this.selectedWorkspaceCwd = saved.selectedWorkspaceCwd;
        },

        /**
         * 保存任务管理当前选择到客户端 JSON 配置。
         * 流程：把当前项目 ID 和工作空间路径写入 APP 本地配置文件，下次打开页面可回显。
         * 参数：无。
         * 返回：保存完成 Promise。
         * 边界：网页预览环境不会写盘；没有项目时保存空值保持幂等。
         */
        async persistSessionManageConfig(): Promise<void> {
            await writeClientJson<SessionManagePersistedStateModel>(StorageKey.sessionManage, {
                selectedProjectId: this.selectedProjectId,
                selectedWorkspaceCwd: this.selectedProject?.workspacePath ?? this.selectedWorkspaceCwd
            });
        },

        /**
         * 创建任务卡片。
         * 流程：调用 Tauri 写入已创建任务，然后刷新当前项目看板。
         * 参数：request 为任务标题和提示词。
         * 返回：创建完成 Promise。
         * 边界：任务创建后不会自动执行，必须由用户放入排队中。
         */
        async addTask(request: CreateSessionTaskRequestModel): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await createSessionTask(request));
                this.message = '任务已创建。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '创建任务失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 将任务推入排队并触发自动执行。
         * 流程：调用 Tauri 设置排队状态并启动后台 CodeX 会话创建，然后刷新看板。
         * 参数：taskId 为目标任务 ID。
         * 返回：操作完成 Promise。
         * 边界：只有已创建、失败或取消任务可进入排队。
         */
        async queueTask(taskId: string): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await queueSessionTask(taskId));
                this.message = '任务已进入排队中。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '任务排队失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 将待验收任务标记为已完成。
         * 流程：调用 Tauri 完成状态流转，再刷新当前看板。
         * 参数：taskId 为目标任务 ID。
         * 返回：操作完成 Promise。
         * 边界：只有待验收任务可完成。
         */
        async completeTask(taskId: string): Promise<void> {
            this.saving = true;
            try {
                this.applyWorkspaceData(await completeSessionTask(taskId));
                this.message = '任务已完成。';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '完成任务失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 打开任务绑定的 CodeX 会话。
         * 流程：把 CodeX thread ID 交给 Tauri 使用 deeplink 打开。
         * 参数：threadId 为 CodeX 会话 ID。
         * 返回：打开完成 Promise。
         * 边界：未绑定会话时抛错并展示提示。
         */
        async openExternalThread(threadId: string): Promise<void> {
            try {
                await openSessionExternalThread(threadId);
            } catch (error) {
                this.message = error instanceof Error ? error.message : '打开 CodeX 会话失败。';
                throw error;
            }
        },

        /**
         * 刷新本地当前项目数据。
         * 流程：按当前选中项目重新读取 SQLite 聚合数据。
         * 参数：无。
         * 返回：刷新完成 Promise。
         * 边界：后台执行完成事件会复用该方法更新待验收状态。
         */
        async refreshCurrentProject(): Promise<void> {
            this.applyWorkspaceData(await loadSessionWorkspaceData(this.selectedProjectId || undefined));
        },

        /**
         * 刷新 CodeX 工作空间列表。
         * 流程：优先按 MVP 方式读取 CodeX 本地状态库，再由客户端桥接兜底读取 app-server 摘要。
         * 参数：无。
         * 返回：刷新完成 Promise。
         * 边界：CodeX 不可用时只清空外部工作空间列表。
         */
        async refreshCodexWorkspaces(): Promise<void> {
            try {
                this.codexWorkspaces = await listCodexWorkspaces();
                if (this.selectedProject) {
                    this.selectedWorkspaceCwd = this.selectedProject.workspacePath;
                    return;
                }
                if (
                    !this.selectedWorkspaceCwd ||
                    !this.codexWorkspaces.some((workspace) => workspace.cwd === this.selectedWorkspaceCwd)
                ) {
                    this.selectedWorkspaceCwd = this.codexWorkspaces[0]?.cwd ?? '';
                }
            } catch {
                this.codexWorkspaces = [];
                if (!this.selectedProject) {
                    this.selectedWorkspaceCwd = '';
                }
            }
        },

        /**
         * 刷新当前工作空间下的 CodeX 会话。
         * 流程：优先使用传入工作空间，其次使用会话管理选中的 CodeX 工作空间，最后兼容任务管理选中的本地项目。
         * 参数：workspaceCwd 为可选工作空间路径。
         * 返回：刷新完成 Promise。
         * 边界：没有工作空间或 CodeX 不可用时返回空列表。
         */
        async refreshCodexThreads(workspaceCwd?: string, reset = true): Promise<void> {
            const workspacePath = workspaceCwd || this.selectedWorkspaceCwd || this.selectedProject?.workspacePath;
            if (!workspacePath) {
                this.codexThreads = [];
                this.codexThreadOffset = 0;
                this.hasMoreCodexThreads = false;
                return;
            }
            try {
                const offset = reset ? 0 : this.codexThreadOffset;
                const threads = await listCodexThreads({
                    workspaceCwd: workspacePath,
                    limit: CODEX_THREAD_PAGE_SIZE,
                    offset,
                    keyword: this.codexThreadKeyword
                });
                this.codexThreads = reset ? threads : [...this.codexThreads, ...threads];
                this.codexThreadOffset = offset + threads.length;
                this.hasMoreCodexThreads = threads.length >= CODEX_THREAD_PAGE_SIZE;
            } catch {
                this.codexThreads = [];
                this.codexThreadOffset = 0;
                this.hasMoreCodexThreads = false;
            }
        },

        /**
         * 搜索当前工作空间下的 CodeX 会话。
         * 流程：保存用户输入的关键词，重置分页后重新读取第一页会话。
         * 参数：keyword 为会话标题或 thread ID 关键词。
         * 返回：搜索完成 Promise。
         * 边界：空关键词等价于恢复当前工作空间默认会话列表。
         */
        async searchCodexThreads(keyword: string): Promise<void> {
            this.codexThreadKeyword = keyword.trim();
            await this.refreshCodexThreads(undefined, true);
        },

        /**
         * 追加加载当前工作空间下的 CodeX 会话。
         * 流程：沿用当前工作空间、搜索关键词和分页位置读取下一页，并追加到列表底部。
         * 参数：无。
         * 返回：加载完成 Promise。
         * 边界：没有更多数据或正在加载时直接返回，避免重复请求。
         */
        async loadMoreCodexThreads(): Promise<void> {
            if (!this.hasMoreCodexThreads || this.loadingMoreThreads) return;
            this.loadingMoreThreads = true;
            try {
                await this.refreshCodexThreads(undefined, false);
            } finally {
                this.loadingMoreThreads = false;
            }
        },

        /**
         * 监听后台执行状态变化。
         * 流程：Tauri 后台创建 CodeX 会话完成后广播项目 ID，前端命中当前项目则刷新。
         * 参数：无。
         * 返回：取消监听函数。
         * 边界：非客户端环境返回空取消函数。
         */
        async listenTaskUpdates(): Promise<() => void> {
            return listenEvent<string>('session-task-updated', (projectId) => {
                if (!this.selectedProjectId || this.selectedProjectId === projectId) {
                    void this.refreshCurrentProject();
                    void this.refreshCodexThreads(undefined, true);
                }
            });
        }
    }
});
