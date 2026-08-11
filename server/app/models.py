"""HTTP 请求、响应和统一错误模型。"""

from typing import Annotated, List, Literal, Optional

from pydantic import BaseModel, ConfigDict, Field, StringConstraints


StrictText = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]
DictionaryText = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]
SafeBusinessId = Annotated[
    str,
    StringConstraints(
        strip_whitespace=True,
        min_length=1,
        max_length=128,
        pattern=r"^[A-Za-z0-9][A-Za-z0-9._:-]*$",
    ),
]


class StrictRequestModel(BaseModel):
    """严格请求基类。

    用途：禁止静默忽略客户端误传的 provider、API Key、baseUrl、modelName 或拼错字段。
    流程：Pydantic 去除字符串首尾空白并拒绝模型未声明字段。
    边界：字段缺失、纯空白或额外字段统一进入 422 错误契约。
    """

    model_config = ConfigDict(
        extra="forbid", str_strip_whitespace=True, populate_by_name=True
    )


class CodexThreadSearchRequest(StrictRequestModel):
    """Codex 会话搜索请求。

    流程：校验绝对工作空间字符串、分页边界和关键词长度后原样转发 Rust，实际路径权限由 Rust 统一校验。
    边界：单页最多 60 条，offset 不得为负，关键词最多 200 字符。
    """

    workspace_cwd: StrictText = Field(
        alias="workspaceCwd",
        max_length=4096,
        description="从工作空间列表取得的 CodeX 工作空间绝对路径；Rust 会再次校验路径边界。",
        examples=["/Users/demo/Documents/project-a"],
    )
    limit: int = Field(
        ge=1, le=60, description="本页最大记录数，范围 1 到 60。", examples=[20]
    )
    offset: int = Field(
        ge=0, le=1_000_000, description="从零开始的分页偏移量。", examples=[0]
    )
    keyword: str = Field(
        default="",
        max_length=200,
        description="可选标题或 thread ID 关键词；空字符串表示不筛选。",
        examples=["接口文档"],
    )


class WorkspaceQueryRequest(StrictRequestModel):
    """任务工作区聚合查询请求。

    流程：传 projectId 时严格查询该项目；省略时由 Rust 选择默认项目上下文。
    边界：显式传入未知 ID 不会回退到其它项目，调用方应刷新项目列表后重新选择。
    """

    project_id: Optional[SafeBusinessId] = Field(
        default=None,
        alias="projectId",
        description="可选任务项目稳定 ID；省略表示读取项目列表及默认项目上下文。",
        examples=["proj_01J00000000000000000000000"],
    )


class ProjectWriteRequest(StrictRequestModel):
    """任务项目创建或更新请求。

    流程：HTTP 只校验展示字段边界；Rust 对真实路径、重复项目和关联状态执行最终校验与事务。
    """

    name: StrictText = Field(
        max_length=100,
        description="项目展示名称，去除首尾空白后不可为空，最多 100 个 Unicode 字符。",
        examples=["AI 工具接口接入"],
    )
    workspace_path: StrictText = Field(
        alias="workspacePath",
        max_length=4096,
        description="项目绑定的真实工作空间绝对路径；必须存在且可访问，Rust 会规范化并校验唯一性。",
        examples=["/Users/demo/Documents/project-a"],
    )


class TaskCreateRequest(StrictRequestModel):
    """任务创建请求。

    流程：HTTP 校验字段边界后交给 Rust 原子创建 ``created`` 任务；创建不会自动排队或预建会话。
    边界：状态初始化、项目存在性、落库和后续调度完全由 Rust 状态机负责。
    """

    project_id: SafeBusinessId = Field(
        alias="projectId",
        description="所属任务项目稳定 ID。",
        examples=["proj_01J00000000000000000000000"],
    )
    title: StrictText = Field(
        max_length=200,
        description="任务标题，最多 200 个 Unicode 字符。",
        examples=["完善 HTTP 接口文档"],
    )
    prompt: StrictText = Field(
        max_length=50_000,
        description="提交给 CodeX 的完整提示词，最多 50000 个 Unicode 字符；正文不会写入 HTTP 日志。",
        examples=["检查现有接口契约，补齐请求示例、响应示例和稳定错误码。"],
    )


class TaskUpdateRequest(StrictRequestModel):
    """任务更新请求。

    流程：HTTP 校验标题和描述边界后交给 Rust 事务，Rust 仅允许 ``created`` 和 ``queued`` 任务更新。
    边界：已执行过的任务状态不可修改，避免改变历史执行语义。
    """

    title: StrictText = Field(
        max_length=200,
        description="任务新标题，去除首尾空白后不可为空，最多 200 个 Unicode 字符。",
        examples=["完善任务管理接口文档"],
    )
    prompt: StrictText = Field(
        max_length=50_000,
        description="任务新描述或提示词，最多 50000 个 Unicode 字符；正文不会写入 HTTP 日志。",
        examples=["补充修改任务和删除任务接口说明，并同步界面操作。"],
    )


class CodexWorkspaceResponse(BaseModel):
    """CodeX 工作空间摘要响应。

    流程：Rust 从 CodeX 权威来源读取并按最近活跃度汇总，HTTP 仅验证并透传安全字段。
    边界：无会话时列表可为空；更新时间无法取得时允许返回空字符串。
    """

    cwd: str = Field(
        description="工作空间绝对路径。", examples=["/Users/demo/Documents/project-a"]
    )
    title: str = Field(description="工作空间展示名称。", examples=["project-a"])
    thread_count: int = Field(
        alias="threadCount", ge=0, description="已索引会话数量。", examples=[12]
    )
    updated_at: str = Field(
        alias="updatedAt",
        pattern=r"^$|^[0-9]+$",
        description="最近更新时间：十进制 Unix epoch 毫秒字符串；来源无法提供时间时为空字符串。",
        examples=["1786406400000"],
    )


class CodexThreadResponse(BaseModel):
    """CodeX 会话搜索摘要响应。

    流程：Rust 按工作空间、关键词和分页读取真实 CodeX thread，并透传子 Agent 父子关系元数据。
    边界：只返回摘要；普通会话的层级字段为空或 0，更新时间无法取得时允许为空字符串。
    """

    id: str = Field(
        description="CodeX thread 稳定 ID；打开会话时原样放入路径参数。",
        examples=["0198f25a-1111-7000-8000-000000000001"],
    )
    title: str = Field(description="会话标题。", examples=["完善 HTTP 接口文档"])
    parent_thread_id: str = Field(
        default="",
        alias="parentThreadId",
        description="父级 CodeX thread ID；普通用户会话为空字符串。",
        examples=["0198f25a-1111-7000-8000-000000000000"],
    )
    depth: int = Field(
        default=0,
        ge=0,
        description="子任务深度；普通用户会话为 0，子 Agent 会话通常从 1 开始。",
        examples=[1],
    )
    agent_nickname: str = Field(
        default="",
        alias="agentNickname",
        description="子 Agent 昵称；普通用户会话为空字符串。",
        examples=["Dirac"],
    )
    agent_role: str = Field(
        default="",
        alias="agentRole",
        description="子 Agent 角色；普通用户会话为空字符串。",
        examples=["worker"],
    )
    updated_at: str = Field(
        alias="updatedAt",
        pattern=r"^$|^[0-9]+$",
        description="最近更新时间：十进制 Unix epoch 毫秒字符串；来源无法提供时间时为空字符串。",
        examples=["1786406400000"],
    )


class CodexConnectionResponse(BaseModel):
    """CodeX Desktop 本机连接状态响应。

    流程：Rust 探测真实 CodeX Desktop renderer 后返回脱敏状态，HTTP 只校验并透传稳定业务字段。
    边界：公开响应不包含端口、PID、WebSocket 地址、DOM、工作目录或其它内部探针细节。
    """

    state: Literal[
        "connected", "disconnected", "restarting", "blocked", "unsupported"
    ] = Field(
        description="当前连接状态；前端按稳定枚举展示已连接、未连接、重启中、受阻或平台不支持。",
        examples=["connected"],
    )
    connected: bool = Field(
        description="Rust 是否已验证真实 CodeX Desktop renderer 可用。", examples=[True]
    )
    desktop_running: bool = Field(
        alias="desktopRunning",
        description="CodeX Desktop 主进程是否正在运行；不公开进程 ID 或启动参数。",
        examples=[True],
    )
    can_restart: bool = Field(
        alias="canRestart",
        description="当前状态是否允许用户显式请求重启；false 时调用方不得自动提交重启。",
        examples=[True],
    )
    reason_code: str = Field(
        alias="reasonCode",
        min_length=1,
        max_length=128,
        pattern=r"^[A-Z][A-Z0-9_]*$",
        description="稳定原因码；调用方据此选择交互，不解析 message 文案。",
        examples=["CODEX_CONNECTED"],
    )
    message: str = Field(
        min_length=1,
        max_length=500,
        description="不含端口、PID、WebSocket、DOM、工作目录或登录信息的用户可读说明。",
        examples=["Codex 已连接，可以由 Desktop 原生创建新会话并发送首次任务。"],
    )
    checked_at: str = Field(
        alias="checkedAt",
        pattern=r"^[0-9]+$",
        description="Rust 完成本次探针的 Unix epoch 毫秒字符串。",
        examples=["1786406400000"],
    )


class CodexRestartAcceptedResponse(BaseModel):
    """CodeX Desktop 异步重启接受响应。

    流程：Rust 完成幂等或单飞门禁后立即返回，实际后台结果由连接状态接口继续轮询。
    边界：HTTP 202 只表示请求已接受；state 为 restarting 时不代表重启已经完成。
    """

    accepted: Literal[True] = Field(
        description="固定为 true，表示 Rust 已接受本次请求或确认当前已连接而无需重复重启。",
        examples=[True],
    )
    state: Literal["connected", "restarting"] = Field(
        description="接受后的状态；connected 表示幂等成功，restarting 表示后台重启正在进行。",
        examples=["restarting"],
    )


class ProjectResponse(BaseModel):
    """任务项目响应。

    流程：Rust 从真实任务数据库读取项目并聚合任务、会话计数，HTTP 不缓存或重新计算。
    边界：时间统一为 SQLite UTC ``YYYY-MM-DD HH:MM:SS``，项目列表单次最多返回 200 条。
    """

    id: str = Field(
        description="项目稳定 ID。", examples=["proj_01J00000000000000000000000"]
    )
    name: str = Field(description="项目展示名称。", examples=["AI 工具接口接入"])
    workspace_path: str = Field(
        alias="workspacePath",
        description="项目当前绑定的规范化绝对工作空间路径。",
        examples=["/Users/demo/Documents/project-a"],
    )
    task_count: int = Field(
        alias="taskCount",
        ge=0,
        le=16,
        description="该项目任务总数，范围 0 到 16。",
        examples=[3],
    )
    session_count: int = Field(
        alias="sessionCount",
        ge=0,
        le=16,
        description="该项目会话总数，范围 0 到 16。",
        examples=[2],
    )
    created_at: str = Field(
        alias="createdAt",
        description="创建时间，SQLite UTC 格式 YYYY-MM-DD HH:MM:SS。",
        examples=["2026-08-11 09:30:00"],
    )
    updated_at: str = Field(
        alias="updatedAt",
        description="最后更新时间，SQLite UTC 格式 YYYY-MM-DD HH:MM:SS。",
        examples=["2026-08-11 10:15:00"],
    )


class TaskResponse(BaseModel):
    """真实任务记录响应。

    流程：Rust 状态机持久化并返回任务全量字段，HTTP 只执行响应 schema 验证。
    边界：生命周期不得由 HTTP 层推断；每个项目最多保留 16 条任务，第 17 条在写入前拒绝。
    """

    id: str = Field(
        description="任务稳定 ID。", examples=["task_01J00000000000000000000000"]
    )
    project_id: str = Field(
        alias="projectId",
        description="所属项目稳定 ID。",
        examples=["proj_01J00000000000000000000000"],
    )
    title: str = Field(description="任务标题。", examples=["完善 HTTP 接口文档"])
    prompt: str = Field(
        description="任务创建时提交的完整提示词。", examples=["检查并补齐接口契约。"]
    )
    status: Literal[
        "created", "queued", "running", "waiting_acceptance", "completed", "failed"
    ] = Field(
        description=(
            "Rust 状态机当前值：created -> queued -> running -> waiting_acceptance -> completed；执行失败进入 failed。"
            "failed 仅表示本地终态，不保证可重新 queue；内部 externalStatus=sendUncertain 表示发送结果不确定并禁止重排。"
            "该内部字段未公开时，调用方必须以 queue 接口的 CODEX_SEND_UNCERTAIN 拒绝为准，禁止自动重放 prompt。"
        ),
        examples=["waiting_acceptance"],
    )
    current_session_id: str = Field(
        alias="currentSessionId",
        description="当前关联本地会话 ID；尚未创建会话时为空字符串。",
        examples=["session_01J00000000000000000000000"],
    )
    external_thread_id: str = Field(
        alias="externalThreadId",
        description="关联 CodeX thread ID；尚未绑定时为空字符串。",
        examples=["0198f25a-1111-7000-8000-000000000001"],
    )
    last_error: str = Field(
        alias="lastError",
        description="最近一次执行失败的安全错误信息；无错误时为空字符串。",
        examples=[""],
    )
    result_json: str = Field(
        alias="resultJson",
        description=(
            "双层编码字段：HTTP 外层值是字符串，调用方必须再执行一次 JSON.parse；内层 JSON 的 UTF-8 最大 32 KiB。"
            '尚无终态结果或执行失败时为字符串 "{}"。二次解析失败表示记录契约损坏，不得展示为成功；应保留原始值和 requestId 供排障。'
        ),
        examples=['{"summary":"接口文档已补齐","filesChanged":3}'],
    )
    created_at: str = Field(
        alias="createdAt",
        description="创建时间，SQLite UTC 格式 YYYY-MM-DD HH:MM:SS。",
        examples=["2026-08-11 09:30:00"],
    )
    updated_at: str = Field(
        alias="updatedAt",
        description="最后更新时间，SQLite UTC 格式 YYYY-MM-DD HH:MM:SS。",
        examples=["2026-08-11 10:15:00"],
    )


class SessionResponse(BaseModel):
    """任务关联的 CodeX 会话记录响应。

    流程：Rust 在真实执行开始后创建记录，并随任务执行和人工验收更新状态。
    边界：每个项目最多保留 16 条会话，第 17 条在写入前拒绝；时间统一为 SQLite UTC ``YYYY-MM-DD HH:MM:SS``。
    """

    id: str = Field(
        description="本地会话稳定 ID。", examples=["session_01J00000000000000000000000"]
    )
    project_id: str = Field(
        alias="projectId",
        description="所属项目稳定 ID。",
        examples=["proj_01J00000000000000000000000"],
    )
    task_id: str = Field(
        alias="taskId",
        description="触发该会话的任务稳定 ID。",
        examples=["task_01J00000000000000000000000"],
    )
    provider: Literal["codex"] = Field(
        description="首发版固定为 codex。", examples=["codex"]
    )
    workspace_path: str = Field(
        alias="workspacePath",
        description="会话创建时使用的工作空间路径快照；项目后续改路径不会追溯改写。",
        examples=["/Users/demo/Documents/project-a"],
    )
    title: str = Field(description="会话标题。", examples=["完善 HTTP 接口文档"])
    status: Literal["running", "waiting_acceptance", "completed", "failed"] = Field(
        description="会话真实状态，由 Rust 执行器更新。",
        examples=["waiting_acceptance"],
    )
    external_thread_id: str = Field(
        alias="externalThreadId",
        description="真实 CodeX thread ID。",
        examples=["0198f25a-1111-7000-8000-000000000001"],
    )
    created_at: str = Field(
        alias="createdAt",
        description="创建时间，SQLite UTC 格式 YYYY-MM-DD HH:MM:SS。",
        examples=["2026-08-11 09:31:00"],
    )
    updated_at: str = Field(
        alias="updatedAt",
        description="最后更新时间，SQLite UTC 格式 YYYY-MM-DD HH:MM:SS。",
        examples=["2026-08-11 10:14:00"],
    )


class WorkspaceDataResponse(BaseModel):
    """任务与会话管理原子聚合响应。

    流程：Rust 在同一业务调用中返回项目全集和选中项目的最近任务、会话，避免拆分请求导致状态撕裂。
    边界：项目最多 200 条，每项目任务和会话各最多 16 条，内部序列化预算为 7 MiB；超限不会截断伪成功。
    """

    projects: List[ProjectResponse] = Field(
        max_length=200,
        description="全部项目，最多 200 条；没有项目时为空数组。",
        examples=[[]],
    )
    tasks: List[TaskResponse] = Field(
        max_length=16,
        description="选中或默认项目的全部任务，最多 16 条；没有项目或任务时为空数组。",
        examples=[[]],
    )
    sessions: List[SessionResponse] = Field(
        max_length=16,
        description="选中或默认项目的全部会话，最多 16 条；没有项目或会话时为空数组。",
        examples=[[]],
    )


class TaskCreateResponse(WorkspaceDataResponse):
    """创建任务专用成功响应。

    流程：Rust 在创建事务提交后返回新任务稳定 ID，并附带同一次业务调用读取的项目聚合快照。
    边界：``createdTaskId`` 只存在于创建任务接口，调用方必须使用该字段执行后续 queue，禁止按非唯一标题猜测 ID。
    """

    created_task_id: str = Field(
        alias="createdTaskId",
        description="本次事务新建的唯一任务稳定 ID；后续 queue、轮询和 complete 必须使用此值。",
        examples=["task_01J00000000000000000000000"],
    )


class OperationResponse(BaseModel):
    """不返回业务实体的成功操作响应。

    流程：具体操作已由权威业务层验证并提交后，HTTP 返回固定确认值。
    边界：用于打开 CodeX thread 时，只表示 Rust 已确认会话存在并向操作系统提交打开请求，不保证 CodeX UI 已完成切换。
    """

    ok: Literal[True] = Field(
        default=True,
        description="固定为 true；打开会话时表示 Rust 已验证 thread 存在并已提交 OS 打开请求，不代表 CodeX 界面已经打开完成。",
        examples=[True],
    )


class AudioTranscriptionRequest(StrictRequestModel):
    """音频转写请求。

    用途：接收 opaque modelId、前端生成的 base64 音频及其媒体类型。
    流程：FastAPI 先校验字段长度，再由服务层解析受信目录、严格解码并提交已登记 ASR 模型。
    边界：只允许客户端传目录 ID，不允许传 API Key、上游地址或上游模型名称。
    """

    model_id: StrictText = Field(
        alias="modelId",
        max_length=128,
        description="从 GET /v1/models 取得的 opaque ID；服务端会校验存在、启用和 ASR 能力。",
        examples=["model_01J00000000000000000000000"],
    )
    audio_base64: str = Field(
        alias="audioBase64",
        description="不含 data URL 头的标准 base64 音频；服务层严格解码并限制为最大 8 MiB。",
        examples=["UklGRiQAAABXQVZFZm10IBAAAAABAAEA..."],
    )
    content_type: str = Field(
        alias="contentType",
        description="调用方声明的 MIME 类型；仅支持 audio/wav、audio/webm、audio/mpeg、audio/mp4、audio/ogg。推荐单声道、16 kHz。",
        examples=["audio/wav"],
    )
    language: Optional[str] = Field(
        default="auto",
        max_length=32,
        description="推荐传 auto 自动识别；也可传 BCP 47 代码，例如 zh-CN，不受支持时返回 UPSTREAM_REJECTED。",
        examples=["auto"],
    )


class AudioTranscriptionResponse(BaseModel):
    """音频转写成功响应。

    用途：向前端返回识别文本、服务端耗时和实际使用的目录 modelId。
    流程：由上游成功响应映射为稳定 camelCase 字段。
    边界：上游缺少有效文本时不会构造该响应，而是返回统一错误 envelope。
    """

    text: str = Field(description="识别后的非空文本。", examples=["今天下午三点开会。"])
    elapsed_ms: int = Field(
        alias="elapsedMs", ge=0, description="服务端调用上游并解析结果的耗时毫秒数。"
    )
    model_id: str = Field(
        alias="modelId", description="服务端本次实际使用的 opaque 模型目录 ID。"
    )


class TextProcessRequest(StrictRequestModel):
    """文本处理请求。

    用途：支持按 opaque modelId 执行听写整理和文字润色两种固定业务模式。
    流程：校验目录模型、文本与上下文后，由服务端生成提示词并调用已登记文本模型。
    边界：模式只允许 dictate/polish，字典与说明字段均有长度上限。
    """

    model_id: StrictText = Field(
        alias="modelId",
        max_length=128,
        description="从 GET /v1/models 取得的 opaque ID；服务端会校验存在、启用和 text 能力。",
        examples=["model_01J00000000000000000000001"],
    )
    mode: Literal["dictate", "polish"] = Field(
        description="dictate 整理口述；polish 润色现有文本。"
    )
    text: StrictText = Field(
        description="待处理正文；去除首尾空白后必须非空，服务层限制为最多 20000 字符。"
    )
    audio_duration_ms: int = Field(
        alias="audioDurationMs",
        ge=0,
        le=24 * 60 * 60 * 1000,
        description="来源音频时长毫秒；非语音来源传 0。",
    )
    dictionary: List[DictionaryText] = Field(
        default_factory=list,
        max_length=100,
        description="需保护的专有词列表；最多 100 项，服务层限制每项最多 100 字符。",
    )
    context_app: str = Field(
        alias="contextApp",
        default="",
        max_length=200,
        description="可选调用场景名称，不应包含隐私正文。",
    )
    style_instruction: str = Field(
        alias="styleInstruction",
        default="",
        max_length=2000,
        description="可选输出语气、长度或格式要求。",
    )


class TextProcessResponse(BaseModel):
    """文本处理成功响应。

    用途：返回模型处理后的文本、服务端耗时和实际使用的目录 modelId。
    流程：从上游首个 assistant message 提取并去除首尾空白。
    边界：空输出按上游无效响应处理，不返回伪成功。
    """

    processed_text: str = Field(
        alias="processedText",
        description="处理后的非空最终文本。",
        examples=["请于今天下午三点参会。"],
    )
    elapsed_ms: int = Field(
        alias="elapsedMs", ge=0, description="服务端调用上游并解析结果的耗时毫秒数。"
    )
    model_id: str = Field(
        alias="modelId", description="服务端本次实际使用的 opaque 模型目录 ID。"
    )


class ModelCatalogResponse(BaseModel):
    """公开模型目录中的安全模型项。

    用途：向已鉴权调用方提供可选择的 opaque ID、展示名、单一能力和当前状态。
    流程：路由从私有运行时目录逐项显式映射本类型，Pydantic 只序列化声明字段。
    字段：``id`` 为调用 ID；``display_name`` 为展示名；``capability`` 为 asr/text；
    ``enabled`` 表示能否调用；``is_default`` 表示该能力下的桌面默认选择。
    边界：绝不包含 provider、baseUrl、modelName、apiKey 或其它上游连接细节。
    """

    id: str = Field(description="opaque 模型目录 ID。")
    display_name: str = Field(
        alias="displayName", description="面向用户的模型展示名称。"
    )
    capability: Literal["asr", "text"] = Field(
        description="模型唯一能力：语音识别或文本处理。"
    )
    enabled: bool = Field(description="模型当前是否允许调用。")
    is_default: bool = Field(alias="isDefault", description="是否为该能力的默认模型。")


class HealthResponse(BaseModel):
    """服务健康响应。

    用途：供部署探针判断 HTTP 进程及配置初始化是否正常。
    流程：健康路由直接返回固定状态和服务名，不访问外部模型。
    边界：不暴露密钥、上游地址或运行环境详情。
    """

    ok: bool
    name: str


class AccessTokenResponse(BaseModel):
    """短期访问 Token 响应。

    用途：长期调用凭据验证通过后，只向当前会话返回有 TTL 的签名 Token。
    流程：服务端绑定 clientId 和过期时间签名，业务接口只接受该短期 Token。
    边界：不返回长期调用凭据；过期后必须重新交换，不支持刷新 Token。
    """

    access_token: str = Field(
        alias="accessToken",
        description="短期 Bearer Token；只应保存在当前页面或可信客户端进程内存。",
    )
    token_type: Literal["Bearer"] = Field(alias="tokenType", default="Bearer")
    expires_in: int = Field(
        alias="expiresIn", gt=0, description="Token 剩余有效期秒数。"
    )
    client_id: str = Field(alias="clientId", description="该 Token 绑定的调用方 ID。")


class AppAccessTokenWriteRequest(StrictRequestModel):
    """App 授权码创建请求。

    用途：由系统设置页或授权码申请流程创建一条可长期查看的明文授权码。
    流程：HTTP 校验名称和可选过期时间边界，具体时间语义由授权码服务规范化。
    边界：不包含权限范围、不换取短期 session token，也不生成 pending 授权申请。
    """

    name: StrictText = Field(
        max_length=100,
        description="授权码名称，用于在系统设置页区分调用方。",
        examples=["Chrome 插件"],
    )
    expires_at: Optional[str] = Field(
        default=None,
        alias="expiresAt",
        max_length=64,
        description="授权码到期时间；null 表示永久有效，非空时必须为带时区的 ISO 时间。",
        examples=[None, "2026-09-01T00:00:00Z"],
    )


class AppAccessTokenResponse(BaseModel):
    """App 授权码响应。

    用途：系统设置页展示和复制明文授权码，并让接口测试确认创建、撤销和最近使用时间。
    流程：由授权码服务从明文 SQLite 记录映射，状态按撤销和过期时间动态计算。
    边界：本响应会包含明文 token，仅用于 App 管理和授权码申请结果，不写入日志。
    """

    id: str = Field(description="授权码稳定 ID。", examples=["token_abc"])
    name: str = Field(description="授权码名称。", examples=["Chrome 插件"])
    token: str = Field(description="明文授权码，可长期查看和复制。", examples=["typesass_xxx"])
    expires_at: Optional[str] = Field(
        default=None,
        alias="expiresAt",
        description="到期时间；null 表示永久有效。",
        examples=[None, "2026-09-01T00:00:00Z"],
    )
    status: Literal["active", "expired", "revoked"] = Field(
        description="授权码状态：有效、已过期或已撤销。",
        examples=["active"],
    )
    created_at: str = Field(alias="createdAt", description="创建时间。")
    revoked_at: Optional[str] = Field(
        default=None, alias="revokedAt", description="撤销时间；未撤销时为 null。"
    )
    last_used_at: Optional[str] = Field(
        default=None,
        alias="lastUsedAt",
        description="最近使用时间；尚未使用时为 null。",
    )


class AccessTokenRequestResponse(BaseModel):
    """请求授权码响应。

    用途：没有授权码的客户端在 App 用户确认后直接拿到授权码。
    流程：不创建 pending 记录，确认时立即创建授权码并返回 approved。
    边界：拒绝时返回 rejected 且不包含 token。
    """

    status: Literal["approved", "rejected"] = Field(
        description="授权申请结果；当前接口成功创建时返回 approved。",
        examples=["approved"],
    )
    access_token: Optional[str] = Field(
        default=None,
        alias="accessToken",
        description="用户确认后生成的明文授权码；拒绝时为空。",
        examples=["typesass_xxx"],
    )
    expires_at: Optional[str] = Field(
        default=None,
        alias="expiresAt",
        description="授权码到期时间；null 表示永久有效。",
        examples=[None],
    )


class DeviceAuthorizationResponse(BaseModel):
    """浏览器设备授权启动响应。

    用途：向无密钥前端返回一次性设备码、人工核对码、明确批准位置和轮询参数。
    流程：deviceCode 仅用于当前浏览器轮询，userCode 交给用户在本机 CodexMan App 内批准。
    边界：不包含 client secret 或短期 Token。
    """

    device_code: str = Field(
        alias="deviceCode", description="高熵一次性轮询凭据，不应发送给管理员。"
    )
    user_code: str = Field(
        alias="userCode", description="可交给管理员批准的短人工核对码。"
    )
    approval_method: Literal["codexman-app"] = Field(
        alias="approvalMethod",
        default="codexman-app",
        description="固定表示必须在本机 CodexMan App 内批准；公共网页和第三方服务不能代替桌面端批准。",
    )
    approval_instruction: str = Field(
        alias="approvalInstruction",
        default="打开本机 CodexMan App，进入 HTTP API 文档，在“批准第三方 Web 设备码”中输入 userCode。",
        description="可直接展示给最终用户的批准步骤；不是可点击网页地址。",
    )
    expires_in: int = Field(alias="expiresIn", gt=0, description="设备码有效期秒数。")
    interval: int = Field(gt=0, description="轮询最小间隔秒数。")


class DeviceApprovalRequest(StrictRequestModel):
    """管理员批准设备码请求。

    用途：让机密客户端把人工核对码绑定到自身 clientId。
    流程：HTTP Basic 验证管理员调用凭据后批准 userCode。
    边界：浏览器不调用该接口，也不持有 Basic secret。
    """

    user_code: str = Field(
        alias="userCode",
        min_length=9,
        max_length=9,
        pattern=r"^[A-F0-9]{4}-[A-F0-9]{4}$",
    )


class DeviceTokenRequest(StrictRequestModel):
    """浏览器轮询设备授权请求。

    用途：使用高熵 deviceCode 查询批准状态并一次性领取短期 Token。
    流程：未批准返回 428，批准后返回 AccessTokenResponse 并消费设备码。
    边界：deviceCode 最长 128 字符，错误或过期统一拒绝。
    """

    device_code: str = Field(alias="deviceCode", min_length=32, max_length=128)


class ErrorDetail(BaseModel):
    """统一错误详情。

    用途：为客户端提供稳定错误码、可展示信息和请求追踪 ID。
    流程：由全局异常处理器从业务异常或框架异常映射生成。
    边界：不得包含密钥、Authorization、完整上游响应或请求正文。
    """

    code: str
    message: str
    request_id: str = Field(alias="requestId")
    retryable: bool = Field(description="相同业务请求是否允许按文档约束重试。")


class ErrorEnvelope(BaseModel):
    """统一错误响应 envelope。

    用途：确保所有非成功响应使用一致的 ``error`` 根字段。
    流程：全局异常处理器把 ``ErrorDetail`` 放入该模型。
    边界：仅用于错误响应，成功响应保持接口约定的扁平结构。
    """

    error: ErrorDetail
