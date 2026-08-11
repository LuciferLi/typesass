/** 任务状态类型；前端只展示 HTTP 服务从 Rust 任务库返回的真实值。 */
export type SessionTaskStatusType = 'created' | 'queued' | 'running' | 'waiting_acceptance' | 'completed' | 'failed';

/** 会话状态类型；会话只在任务被调度器真实领取后创建，不预留未实现的预创建状态。 */
export type SessionStatusType = 'running' | 'waiting_acceptance' | 'completed' | 'failed';

/** 会话来源类型；首发版只接受已接通的真实 CodeX 执行器。 */
export type SessionProviderType = 'codex';

/** 任务标题字符上限；必须与 Rust `TASK_TITLE_MAX_CHARS` 协议保持一致。 */
export const SESSION_TASK_TITLE_MAX_CHARS = 200;

/** 任务提示词字符上限；必须与 Rust `TASK_PROMPT_MAX_CHARS` 协议保持一致。 */
export const SESSION_TASK_PROMPT_MAX_CHARS = 50_000;

/** HTTP 服务返回的任务项目元数据。 */
export interface SessionProjectModel {
    /** Rust 权威任务库返回的稳定主键。 */
    id: string;
    /** 项目展示名称。 */
    name: string;
    /** 绑定的真实工作空间绝对路径。 */
    workspacePath: string;
    /** Rust 权威任务库统计的任务数量。 */
    taskCount: number;
    /** Rust 权威任务库统计的会话数量。 */
    sessionCount: number;
    /** 创建时间。 */
    createdAt: string;
    /** 更新时间。 */
    updatedAt: string;
}

/** 任务记录，所有状态字段均以 HTTP 服务委托 Rust 返回的结果为准。 */
export interface SessionTaskModel {
    /** 任务稳定主键。 */
    id: string;
    /** 所属项目 ID。 */
    projectId: string;
    /** 任务标题。 */
    title: string;
    /** 交给 CodeX 的真实提示词。 */
    prompt: string;
    /** Rust 权威任务状态机确认的当前状态。 */
    status: SessionTaskStatusType;
    /** 当前本地会话 ID，尚未执行时为空。 */
    currentSessionId: string;
    /** CodeX thread ID，尚未创建成功时为空。 */
    externalThreadId: string;
    /** 最近失败原因，正常状态为空。 */
    lastError: string;
    /** 可靠终态结果 JSON；执行前或失败时为 `{}`，最大 32 KiB。 */
    resultJson: string;
    /** 创建时间。 */
    createdAt: string;
    /** 更新时间。 */
    updatedAt: string;
}

/** HTTP 服务返回的任务关联真实会话记录。 */
export interface SessionRecordModel {
    /** 本地会话主键。 */
    id: string;
    /** 所属项目 ID。 */
    projectId: string;
    /** 关联任务 ID。 */
    taskId: string;
    /** 会话执行器。 */
    provider: SessionProviderType;
    /** 工作空间绝对路径。 */
    workspacePath: string;
    /** 会话标题。 */
    title: string;
    /** Rust 权威任务状态机确认的会话状态。 */
    status: SessionStatusType;
    /** 外部 CodeX thread ID。 */
    externalThreadId: string;
    /** 创建时间。 */
    createdAt: string;
    /** 更新时间。 */
    updatedAt: string;
}

/** 任务 HTTP 聚合响应，用于原子刷新项目、任务和会话。 */
export interface SessionWorkspaceDataModel {
    /** HTTP 服务返回的真实任务项目列表。 */
    projects: SessionProjectModel[];
    /** 当前项目的真实任务列表。 */
    tasks: SessionTaskModel[];
    /** 当前项目的真实会话列表。 */
    sessions: SessionRecordModel[];
}

/**
 * 创建任务 HTTP 响应。
 * 业务含义：在权威聚合数据之外，明确返回本次事务创建的任务 ID，避免并发同名任务时由标题猜测。
 */
export interface CreateSessionTaskResponseModel extends SessionWorkspaceDataModel {
    /** 本次 Rust 事务创建的唯一任务 ID。 */
    createdTaskId: string;
}

/** 创建任务项目的 HTTP 请求。 */
export interface CreateSessionProjectRequestModel {
    /** 项目展示名称。 */
    name: string;
    /** 绑定的真实工作空间绝对路径。 */
    workspacePath: string;
}

/** 编辑任务项目的 HTTP 请求。 */
export interface UpdateSessionProjectRequestModel {
    /** 项目稳定主键。 */
    id: string;
    /** 项目新展示名称。 */
    name: string;
    /** 后续任务使用的真实工作空间绝对路径。 */
    workspacePath: string;
}

/** 创建真实任务卡片的 HTTP 请求。 */
export interface CreateSessionTaskRequestModel {
    /** 所属项目 ID。 */
    projectId: string;
    /** 任务标题。 */
    title: string;
    /** 交给 CodeX 的完整提示词。 */
    prompt: string;
}

/** 更新真实任务卡片的 HTTP 请求。 */
export interface UpdateSessionTaskRequestModel {
    /** 任务稳定主键。 */
    id: string;
    /** 任务标题。 */
    title: string;
    /** 交给 CodeX 的完整提示词。 */
    prompt: string;
}

// 会话管理持久化配置，用于客户端 JSON 保存用户最后一次选择的真实 CodeX 工作空间。
export type SessionManagePersistedStateModel = {
    // 最近一次选中的任务项目 ID；仅用于恢复页面上下文，不代表任务状态。
    selectedProjectId?: string;
    // 最近一次选中的工作空间绝对路径。
    selectedWorkspaceCwd: string;
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
    // 父级 CodeX thread ID；普通用户会话为空字符串。
    parentThreadId: string;
    // 子任务深度；普通用户会话为 0，子 Agent 会话通常从 1 开始。
    depth: number;
    // 子 Agent 昵称；普通用户会话为空字符串。
    agentNickname: string;
    // 子 Agent 角色；普通用户会话为空字符串。
    agentRole: string;
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
