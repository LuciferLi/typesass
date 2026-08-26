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

/** 项目基础提示词字符上限；必须与 Rust `PROJECT_BASE_PROMPT_MAX_CHARS` 协议保持一致。 */
export const SESSION_PROJECT_BASE_PROMPT_MAX_CHARS = 20_000;

/** 任务图片附件，独立于 prompt 文本发送给 CodeX。 */
export interface SessionTaskAttachmentModel {
    /** 附件稳定 ID。 */
    id: string;
    /** 附件文件名。 */
    name: string;
    /** 附件图片类型。 */
    mimeType: 'image/png' | 'image/jpeg' | 'image/webp';
    /** 图片 data URL，用于前端预览和本机执行上传。 */
    dataUrl: string;
}

/** HTTP 服务返回的任务项目元数据。 */
export interface SessionProjectModel {
    /** Rust 权威任务库返回的稳定主键。 */
    id: string;
    /** 项目展示名称。 */
    name: string;
    /** 绑定的真实工作空间绝对路径。 */
    workspacePath: string;
    /** 项目基础提示词，任务执行时自动追加到任务内容前。 */
    basePrompt: string;
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
    /** 交给 CodeX 的图片附件，不拼入 prompt 文本。 */
    attachments: SessionTaskAttachmentModel[];
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
    /** 项目基础提示词，为空时不参与任务发送。 */
    basePrompt: string;
}

/** 编辑任务项目的 HTTP 请求。 */
export interface UpdateSessionProjectRequestModel {
    /** 项目稳定主键。 */
    id: string;
    /** 项目新展示名称。 */
    name: string;
    /** 后续任务使用的真实工作空间绝对路径。 */
    workspacePath: string;
    /** 后续任务执行时自动携带的项目基础提示词。 */
    basePrompt: string;
}

/** 创建真实任务卡片的 HTTP 请求。 */
export interface CreateSessionTaskRequestModel {
    /** 所属项目 ID。 */
    projectId: string;
    /** 任务标题。 */
    title: string;
    /** 交给 CodeX 的完整提示词。 */
    prompt: string;
    /** 交给 CodeX 的图片附件。 */
    attachments?: SessionTaskAttachmentModel[];
}

/** 更新真实任务卡片的 HTTP 请求。 */
export interface UpdateSessionTaskRequestModel {
    /** 任务稳定主键。 */
    id: string;
    /** 任务标题。 */
    title: string;
    /** 交给 CodeX 的完整提示词。 */
    prompt: string;
    /** 交给 CodeX 的图片附件；更新时整体替换。 */
    attachments?: SessionTaskAttachmentModel[];
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

// CodeX 会话运行状态，用于左侧列表展示实时执行态。
export type CodexThreadStatusType = 'running' | 'completed' | 'failed' | 'unknown';

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
    // 会话运行状态；running 时左侧列表展示加载图标。
    status: CodexThreadStatusType;
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

/** CodeX 会话消息角色类型。 */
export type CodexThreadMessageRoleType = 'user' | 'assistant';

/** CodeX 会话消息结构化展示类型。 */
export type CodexThreadMessageKindType =
    | 'user'
    | 'assistant'
    | 'commentary'
    | 'finalAnswer'
    | 'reasoning'
    | 'toolCall'
    | 'toolResult'
    | 'status';

/** CodeX 会话正文中的单条可展示消息。 */
export interface CodexThreadMessageModel {
    /** 历史分页和排序使用的消息顺序，不与 SSE seq 混用。 */
    messageOrder: number;
    /** 消息角色。 */
    role: CodexThreadMessageRoleType;
    /** 结构化消息类型，用于区分助手正文、思考、工具调用、工具结果和状态。 */
    kind: CodexThreadMessageKindType;
    /** 消息块标题；普通正文为空，工具和状态块展示折叠标题。 */
    title: string;
    /** 已由服务端做长度保护的消息正文。 */
    content: string;
    /** 执行状态；普通正文为空，工具和状态块可能为 running/completed/failed。 */
    status: string;
    /** 消息创建时间；来源不可用时为空字符串。 */
    createdAt: string;
}

/** CodeX 会话消息窗口范围。 */
export interface CodexThreadMessageRangeModel {
    /** 当前窗口第一条消息顺序。 */
    startMessageOrder: number;
    /** 当前窗口最后一条消息顺序。 */
    endMessageOrder: number;
    /** 窗口前方是否还有更早历史。 */
    hasMoreBefore: boolean;
    /** 窗口后方是否还有更新消息。 */
    hasMoreAfter: boolean;
}

/** CodeX 会话正文窗口响应。 */
export interface CodexThreadMessagesResponseModel {
    /** CodeX thread 稳定 ID。 */
    threadId: string;
    /** 会话标题。 */
    title: string;
    /** 会话更新时间；来源不可用时为空字符串。 */
    updatedAt: string;
    /** 当前窗口内可展示消息。 */
    messages: CodexThreadMessageModel[];
    /** 当前消息窗口范围。 */
    range: CodexThreadMessageRangeModel;
}

/** CodeX 会话 SSE snapshot 事件。 */
export interface CodexThreadSnapshotEventModel {
    /** SSE 增量事件序号。 */
    seq: number;
    /** CodeX thread 稳定 ID。 */
    threadId: string;
    /** 固定为 snapshot。 */
    type: 'snapshot';
    /** 首包窗口消息。 */
    messages: CodexThreadMessageModel[];
    /** 首包消息范围。 */
    range: CodexThreadMessageRangeModel;
}

/** CodeX 会话 SSE 心跳事件。 */
export interface CodexThreadHeartbeatEventModel {
    /** SSE 增量事件序号。 */
    seq: number;
    /** CodeX thread 稳定 ID。 */
    threadId: string;
    /** 固定为 heartbeat。 */
    type: 'heartbeat';
}

/** CodeX 会话 SSE 消息增量事件。 */
export interface CodexThreadMessageDeltaEventModel {
    /** SSE 增量事件序号。 */
    seq: number;
    /** CodeX thread 稳定 ID。 */
    threadId: string;
    /** 固定为 messageDelta。 */
    type: 'messageDelta';
    /** 发生变化的消息。 */
    message: CodexThreadMessageModel;
}

/** CodeX 会话流事件联合类型。 */
export type CodexThreadStreamEventModel =
    | CodexThreadSnapshotEventModel
    | CodexThreadHeartbeatEventModel
    | CodexThreadMessageDeltaEventModel;
