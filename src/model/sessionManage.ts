// 任务状态类型，用于任务看板分栏和状态流转控制。
export type SessionTaskStatusType =
    | 'created'
    | 'queued'
    | 'running'
    | 'waiting_acceptance'
    | 'completed'
    | 'failed'
    | 'cancelled';

// 会话状态类型，用于会话管理列表展示本地执行生命周期。
export type SessionStatusType = 'created' | 'running' | 'waiting_acceptance' | 'completed' | 'failed';

// 会话来源类型，用于区分当前 CodeX 和未来 Cloud 等执行器。
export type SessionProviderType = 'codex' | 'cloud';

// 项目记录模型，用于左侧项目列表和工作空间绑定展示。
export type SessionProjectModel = {
    // 项目 ID，本地 SQLite 生成的稳定主键。
    id: string;
    // 项目名称。
    name: string;
    // 项目绑定的工作空间绝对路径。
    workspacePath: string;
    // 项目下任务数量。
    taskCount: number;
    // 项目下会话数量。
    sessionCount: number;
    // 创建时间，来自 SQLite。
    createdAt: string;
    // 更新时间，来自 SQLite。
    updatedAt: string;
};

// 任务记录模型，用于任务管理看板卡片展示。
export type SessionTaskModel = {
    // 任务 ID，本地 SQLite 生成的稳定主键。
    id: string;
    // 所属项目 ID。
    projectId: string;
    // 任务标题。
    title: string;
    // 任务执行提示词。
    prompt: string;
    // 当前任务状态。
    status: SessionTaskStatusType;
    // 当前绑定的本地会话 ID，未执行前为空。
    currentSessionId: string;
    // 当前绑定的 CodeX thread ID，未创建成功前为空。
    externalThreadId: string;
    // 最近失败原因，正常状态为空。
    lastError: string;
    // 创建时间，来自 SQLite。
    createdAt: string;
    // 更新时间，来自 SQLite。
    updatedAt: string;
};

// 会话记录模型，用于会话管理页面展示 CodeX 会话和工作空间。
export type SessionRecordModel = {
    // 会话 ID，本地 SQLite 生成的稳定主键。
    id: string;
    // 所属项目 ID。
    projectId: string;
    // 关联任务 ID，手动导入会话时可能为空。
    taskId: string;
    // 会话来源，当前主要为 codex。
    provider: SessionProviderType;
    // 会话所属工作空间绝对路径。
    workspacePath: string;
    // 会话标题。
    title: string;
    // 当前会话状态。
    status: SessionStatusType;
    // CodeX thread ID。
    externalThreadId: string;
    // 创建时间，来自 SQLite。
    createdAt: string;
    // 更新时间，来自 SQLite。
    updatedAt: string;
};

// 会话与任务工作区聚合模型，用于页面一次性刷新项目、任务和会话。
export type SessionWorkspaceDataModel = {
    // 本地项目列表。
    projects: SessionProjectModel[];
    // 当前项目下的任务列表。
    tasks: SessionTaskModel[];
    // 当前项目下的会话列表。
    sessions: SessionRecordModel[];
};

// 会话与任务管理持久化配置，用于客户端 JSON 保存用户最后一次选择的工作空间。
export type SessionManagePersistedStateModel = {
    // 最近一次选中的本地项目 ID，用于下次打开任务管理页时恢复看板上下文。
    selectedProjectId: string;
    // 最近一次选中的工作空间绝对路径，用于项目被删除或重建时按目录兜底恢复。
    selectedWorkspaceCwd: string;
};

// 创建项目请求模型，用于把业务项目绑定到工作空间。
export type CreateSessionProjectRequestModel = {
    // 项目名称。
    name: string;
    // 绑定的工作空间绝对路径。
    workspacePath: string;
};

// 创建任务请求模型，用于登记一张任务看板卡片。
export type CreateSessionTaskRequestModel = {
    // 所属项目 ID。
    projectId: string;
    // 任务标题。
    title: string;
    // 任务执行提示词。
    prompt: string;
};

// CodeX 工作空间模型，用于会话管理展示外部已有工作空间。
export type CodexWorkspaceModel = {
    // 工作空间绝对路径。
    cwd: string;
    // 工作空间展示名称。
    title: string;
    // CodeX 侧已有会话数量。
    threadCount: number;
    // 最近更新时间。
    updatedAt: string;
};

// CodeX 会话摘要模型，用于展示外部已有会话。
export type CodexThreadSummaryModel = {
    // CodeX thread ID。
    id: string;
    // CodeX 会话标题。
    title: string;
    // 最近更新时间。
    updatedAt: string;
};

// CodeX 会话列表请求模型，用于按工作空间分页和搜索会话。
export type CodexThreadListRequestModel = {
    // CodeX 工作空间绝对路径。
    workspaceCwd: string;
    // 本次读取的最大会话数量。
    limit: number;
    // 从第几条开始读取，用于加载更多。
    offset: number;
    // 搜索关键词，可匹配会话标题或 thread ID。
    keyword: string;
};
