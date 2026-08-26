use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

/// 首发任务管理业务库结构标识；当前版本允许受控补充软删除字段。
const INITIAL_SCHEMA_VERSION: i64 = 1;

/// 首发 schema 元数据名称；已登记库必须精确匹配，只允许登记过的首发结构迁移到当前结构。
const INITIAL_SCHEMA_NAME: &str = "初始化会话与任务管理表";
/// 首发 schema 内容校验值；已登记库必须精确匹配，禁止把未知结构当作当前版本继续运行。
const INITIAL_SCHEMA_CHECKSUM: &str = "003-session-task-project-base-prompt";
/// 上一版首发 schema 内容校验值；仅用于把项目记录升级为支持基础提示词。
const LEGACY_SCHEMA_CHECKSUM_PROJECT_SOFT_DELETE: &str = "002-session-task-project-soft-delete";
/// 上一版首发 schema 内容校验值；仅用于把项目物理删除协议升级为软删除协议。
const LEGACY_SCHEMA_CHECKSUM_SESSION_TASK: &str = "001-session-task-schema";

/// 任务标题最大 Unicode 字符数，兼顾看板识别与 IPC/SQLite 有界存储。
pub const TASK_TITLE_MAX_CHARS: usize = 200;
/// 任务提示词最大 Unicode 字符数，避免单条任务无限占用前端、IPC、数据库与 Codex 请求内存。
pub const TASK_PROMPT_MAX_CHARS: usize = 50_000;
/// 单个任务最多携带的图片附件数量，避免 HTTP/RPC/SQLite 聚合响应被截图无限放大。
const TASK_ATTACHMENT_LIMIT: usize = 4;
/// 单个图片附件 data URL 最大字符数，当前插件会压缩截图，Rust 仍执行最终边界校验。
const TASK_ATTACHMENT_DATA_URL_MAX_CHARS: usize = 200_000;
/// 可靠终态结果 JSON 最大 UTF-8 字节数，防止外部响应放大本地数据库和 IPC 返回。
const TASK_RESULT_JSON_MAX_BYTES: usize = 32 * 1024;
/// 项目名称最大 Unicode 字符数，确保项目列表的单条记录具备可证明的序列化上限。
const PROJECT_NAME_MAX_CHARS: usize = 100;
/// 项目基础提示词最大 Unicode 字符数，确保项目列表和任务发送内容具备有界容量。
pub const PROJECT_BASE_PROMPT_MAX_CHARS: usize = 20_000;
/// 首发最多允许创建的项目数，避免项目列表在没有分页字段的首版协议中无界增长。
const WORKSPACE_PROJECT_LIMIT: i64 = 200;
/// 工作区业务 JSON 的内部预算，给私有 RPC 8 MiB envelope、错误结构和长度前缀保留至少 1 MiB 余量。
pub(crate) const WORKSPACE_RESPONSE_BUDGET_BYTES: usize = 7 * 1024 * 1024;

/// 任务状态枚举，用于前端看板和调度器共同判断任务流转。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskStatus {
    /// 已创建但尚未进入执行队列，用户可手动推入排队。
    Created,
    /// 已进入执行队列，调度器会自动捞取。
    Queued,
    /// 已创建外部会话并正在执行。
    Running,
    /// 外部会话执行完成，等待人工验收。
    WaitingAcceptance,
    /// 人工验收通过，任务闭环完成。
    Completed,
    /// 执行过程失败，需要用户重新排队或检查错误。
    Failed,
}

impl TaskStatus {
    /// 返回写入 SQLite 和前端任务看板的稳定状态协议值。
    /// 流程：把当前枚举分支映射为唯一小写字符串；参数为当前状态引用；返回静态字符串。
    /// 异常/边界：枚举只包含首发版真实可达状态，不为未开放功能预留协议值。
    fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingAcceptance => "waiting_acceptance",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// 会话状态枚举，用于记录本地调度器与外部执行器的生命周期。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionStatus {
    /// 外部执行器正在处理任务。
    Running,
    /// 外部执行器已接受任务，当前等待人工验收。
    WaitingAcceptance,
    /// 会话对应任务已完成验收。
    Completed,
    /// 会话启动或执行失败。
    Failed,
}

impl SessionStatus {
    /// 返回写入 SQLite 和前端会话列表的稳定状态协议值。
    /// 流程：把当前枚举分支映射为唯一小写字符串；参数为当前状态引用；返回静态字符串。
    /// 异常/边界：只包含 session 表真实写入的首发状态，不保留未实现的预创建状态。
    fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::WaitingAcceptance => "waiting_acceptance",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// 创建项目请求，用于把本地业务项目绑定到一个 CodeX 工作空间。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    /// 项目名称，展示在任务管理和会话管理左侧列表。
    pub name: String,
    /// 项目绑定的工作空间绝对路径，后续任务默认在该目录创建会话。
    pub workspace_path: String,
    /// 项目基础提示词，后续任务执行时自动追加在任务正文前。
    #[serde(default)]
    pub base_prompt: String,
}

/// 编辑项目请求，用于更新展示名称和后续任务使用的 CodeX 工作空间。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    /// 项目稳定 ID。
    pub id: String,
    /// 项目新展示名称。
    pub name: String,
    /// 项目新工作空间绝对路径；已有会话仍保留各自执行时的路径快照。
    pub workspace_path: String,
    /// 项目基础提示词；为空时后续任务只发送任务自身内容。
    #[serde(default)]
    pub base_prompt: String,
}

/// 创建任务请求，用于在指定项目下登记一个待处理任务。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    /// 所属项目 ID，决定任务使用哪个工作空间。
    pub project_id: String,
    /// 任务标题，用于看板卡片展示和会话标题辅助识别。
    pub title: String,
    /// 任务首条提示词，执行时发送给 CodeX。
    pub prompt: String,
    /// 任务图片附件，执行时以文件附件形式发送给 CodeX，不拼入提示词。
    #[serde(default)]
    pub attachments: Vec<TaskAttachmentRecord>,
}

/// 更新任务请求，用于修改尚未执行完成任务的名称和描述。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    /// 任务稳定 ID。
    pub id: String,
    /// 任务新标题，用于看板卡片展示。
    pub title: String,
    /// 任务新提示词，后续执行时发送给 CodeX。
    pub prompt: String,
    /// 任务图片附件；更新任务时整体替换附件列表。
    #[serde(default)]
    pub attachments: Vec<TaskAttachmentRecord>,
}

/// 任务图片附件记录，用于 HTTP/RPC、任务看板和 Codex 执行上传。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttachmentRecord {
    /// 附件稳定 ID，用于前端渲染和排障。
    pub id: String,
    /// 附件文件名，执行前会再次收敛为安全文件名。
    pub name: String,
    /// 附件媒体类型；当前仅支持图片。
    pub mime_type: String,
    /// 图片 data URL；仅用于本机任务转发，不写入日志。
    pub data_url: String,
}

/// 项目列表项模型，用于前端项目选择与工作空间展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    /// 项目 ID，本地生成并作为稳定业务主键。
    pub id: String,
    /// 项目名称。
    pub name: String,
    /// 绑定的工作空间绝对路径。
    pub workspace_path: String,
    /// 项目基础提示词，后续任务执行时自动携带。
    pub base_prompt: String,
    /// 项目下任务总数。
    pub task_count: i64,
    /// 项目下会话总数。
    pub session_count: i64,
    /// 创建时间，使用 SQLite CURRENT_TIMESTAMP 字符串。
    pub created_at: String,
    /// 更新时间，使用 SQLite CURRENT_TIMESTAMP 字符串。
    pub updated_at: String,
}

/// 任务看板卡片模型，用于前端按状态分组展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    /// 任务 ID，本地生成并作为稳定业务主键。
    pub id: String,
    /// 所属项目 ID。
    pub project_id: String,
    /// 任务标题。
    pub title: String,
    /// 任务提示词。
    pub prompt: String,
    /// 任务图片附件，独立于 prompt 文本。
    pub attachments: Vec<TaskAttachmentRecord>,
    /// 当前任务状态协议值。
    pub status: String,
    /// 当前绑定的会话 ID；未执行前为空。
    pub current_session_id: String,
    /// 当前绑定的 CodeX thread ID；未创建会话前为空。
    pub external_thread_id: String,
    /// 最近失败原因；正常状态为空。
    pub last_error: String,
    /// app-server 可靠终态携带的结果 JSON；执行前或失败时为空对象。
    pub result_json: String,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
}

/// 会话列表项模型，用于会话管理页面展示工作空间下的 CodeX 会话。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    /// 会话 ID，本地生成并作为稳定业务主键。
    pub id: String,
    /// 所属项目 ID。
    pub project_id: String,
    /// 关联任务 ID；从会话管理手动导入的会话可为空。
    pub task_id: String,
    /// 外部执行器类型，当前为 codex。
    pub provider: String,
    /// 工作空间绝对路径。
    pub workspace_path: String,
    /// 会话标题。
    pub title: String,
    /// 当前会话状态协议值。
    pub status: String,
    /// CodeX thread ID。
    pub external_thread_id: String,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
}

/// 任务与会话管理页面的聚合数据，减少前端多次请求导致的状态抖动。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDataResponse {
    /// 本地项目列表。
    pub projects: Vec<ProjectRecord>,
    /// 当前项目下的任务列表。
    pub tasks: Vec<TaskRecord>,
    /// 当前项目下的会话列表。
    pub sessions: Vec<SessionRecord>,
}

/// 创建任务专用响应，用于在并发同名任务场景精确标识本次事务创建的记录。
/// 该结构只用于 createTask 私有 RPC，保持 projects/tasks/sessions 与既有聚合同级，避免其它聚合接口出现无意义空字段。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskResponse {
    /// 本次 Immediate 事务生成并已成功提交的任务 ID。
    pub created_task_id: String,
    /// 本地项目列表，与创建后的真实工作区聚合一致。
    pub projects: Vec<ProjectRecord>,
    /// 当前项目全部任务，包含 created_task_id 指向的新任务。
    pub tasks: Vec<TaskRecord>,
    /// 当前项目全部会话；新建 created 任务不会提前生成会话。
    pub sessions: Vec<SessionRecord>,
}

/// 后台任务状态事件的增量数据库快照，避免前端为单条事件重新加载整个项目。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdateSnapshot {
    /// 已提交状态后的真实任务卡片。
    pub task: TaskRecord,
    /// 已重新统计任务/会话数量的所属项目。
    pub project: ProjectRecord,
    /// 当前任务会话；任务尚未被调度器领取时为空。
    pub session: Option<SessionRecord>,
}

/// 队列调度候选任务，供 Rust 后台线程创建 CodeX 会话。
#[derive(Debug, Clone)]
pub struct QueuedTaskRecord {
    /// 任务 ID。
    pub id: String,
    /// 所属项目 ID。
    pub project_id: String,
    /// 任务标题。
    pub title: String,
    /// 任务提示词；读取队列上下文时已按项目配置拼接基础提示词。
    pub prompt: String,
    /// 任务图片附件，执行时上传给 CodeX。
    pub attachments: Vec<TaskAttachmentRecord>,
    /// 任务所属工作空间。
    pub workspace_path: String,
}

/// 重启对账所需的运行中任务快照，用于查询 Codex thread/turn 的真实状态。
#[derive(Debug, Clone)]
pub struct RunningTaskRecord {
    /// 所属项目 ID，状态提交后用于通知前端刷新当前看板。
    pub project_id: String,
    /// 本地任务 ID。
    pub task_id: String,
    /// 本地会话 ID。
    pub session_id: String,
    /// Codex thread ID；为空表示进程在绑定 thread 前崩溃，不能安全重放。
    pub thread_id: String,
    /// Codex turn ID；为空表示 turn/start 结果尚未可靠落库，只能从任务专用 thread 的唯一 turn 恢复，不能重放 prompt。
    pub turn_id: String,
}

/// 进程在 CDP Enter 后、thread 绑定前退出时所需的提交恢复上下文。
#[derive(Debug, Clone)]
pub struct PendingSubmissionRecord {
    /// 本地任务 ID。
    pub task_id: String,
    /// 所属项目 ID，用于恢复后广播真实状态。
    pub project_id: String,
    /// 本地 running 会话 ID。
    pub session_id: String,
    /// 任务执行时固定的 canonical 工作目录快照。
    pub workspace_path: String,
    /// 首条用户消息原文，仅在 Rust 内存用于精确匹配，不进入日志或响应。
    pub prompt: String,
    /// Enter 前已事务提交的 Unix 毫秒水位。
    pub submitted_at_ms: i64,
    /// Enter 前生成并持久化的请求关联 ID。
    pub client_user_message_id: String,
    /// Enter 前从权威 Codex 状态库读取并原子持久化的旧 thread ID 完整快照。
    pub known_thread_ids: Vec<String>,
}

/// 持久化在 session 内部字段中的 CDP 提交恢复状态。
/// 该对象只由 Rust 写入和读取，不属于公开 HTTP 字段；版本固定为 1，首发不接受旧字符串格式兜底。
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingSubmissionState {
    /// 严格结构版本，防止后续字段变化被旧恢复逻辑静默误读。
    version: u8,
    /// Enter 前的精确 Unix 毫秒水位，必须大于 0。
    submitted_at_ms: i64,
    /// Enter 前同一 canonical cwd 下全部旧 thread ID；空数组代表权威查询确认当时不存在旧 thread。
    known_thread_ids: Vec<String>,
}

/// 初始化并读取当前任务管理数据。
/// 流程：打开业务 SQLite，执行迁移，再按当前项目查询任务与会话。
/// 参数：app 用于定位用户级 app data 目录，project_id 为可选当前项目。
/// 返回：项目、任务、会话聚合数据。
/// 边界：项目为空时自动返回空任务和空会话列表。
pub fn load_workspace_data(
    app: &AppHandle,
    project_id: Option<String>,
) -> Result<WorkspaceDataResponse, String> {
    let connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    load_workspace_data_with_connection(&connection, project_id.as_deref())
}

/// 判断错误是否属于可安全返回 HTTP 调用方的任务业务契约错误。
/// 流程：只匹配本模块固定文案产生的白名单错误码；参数为内部错误字符串；返回是否允许在记录诊断后保留原始稳定码。
/// 异常/边界：数据库、路径、prompt 正文或未知错误均返回 false，调用方必须只返回统一脱敏诊断文案。
pub fn is_public_task_contract_error(error: &str) -> bool {
    [
        "TASK_ATTACHMENT_COUNT_TOO_LONG",
        "TASK_ATTACHMENT_DATA_TOO_LONG",
        "TASK_ATTACHMENT_DATA_URL_INVALID",
        "TASK_ATTACHMENT_MIME_UNSUPPORTED",
        "TASK_ATTACHMENT_NAME_REQUIRED",
        "TASK_ATTACHMENT_NAME_TOO_LONG",
        "TASK_TITLE_REQUIRED",
        "TASK_TITLE_TOO_LONG",
        "TASK_PROMPT_REQUIRED",
        "TASK_PROMPT_TOO_LONG",
        "TASK_PROJECT_NAME_REQUIRED",
        "TASK_PROJECT_NAME_EXISTS",
        "TASK_PROJECT_NAME_TOO_LONG",
        "TASK_PROJECT_WORKSPACE_REQUIRED",
        "TASK_PROJECT_WORKSPACE_ABSOLUTE_REQUIRED",
        "TASK_PROJECT_WORKSPACE_UNAVAILABLE",
        "TASK_PROJECT_WORKSPACE_DIR_REQUIRED",
        "TASK_PROJECT_LIMIT_REACHED",
        "TASK_PROJECT_NOT_FOUND",
        "TASK_NOT_FOUND",
        "TASK_UPDATE_STATUS_FORBIDDEN",
        "TASK_DELETE_STATUS_FORBIDDEN",
        "TASK_PROJECT_CAPACITY_INVALID",
        "TASK_WORKSPACE_SERIALIZATION_FAILED",
        "TASK_WORKSPACE_RESPONSE_TOO_LARGE",
        "CODEX_DESKTOP_NOT_CONNECTED",
        "CODEX_SEND_UNCERTAIN",
    ]
    .iter()
    .any(|code| error.contains(code))
}

/// 读取任务事件所需的真实增量快照。
/// 流程：打开任务库后分别读取任务、重新聚合的项目和当前会话，所有字段均来自同一 SQLite 连接。
/// 参数：app 用于定位数据库，task_id/project_id 为事件稳定标识；返回任务、项目与可选会话快照。
/// 异常/边界：任一必需记录不存在或任务归属不匹配时返回稳定错误，不回退全量工作区加载，也不伪造状态。
pub fn load_task_update_snapshot(
    app: &AppHandle,
    task_id: &str,
    project_id: &str,
) -> Result<TaskUpdateSnapshot, String> {
    let connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let task = load_task_with_connection(&connection, task_id)?
        .ok_or_else(|| "任务不存在或已被删除（错误码：TASK_NOT_FOUND）".to_string())?;
    if task.project_id != project_id {
        return Err("任务与事件项目不匹配（错误码：TASK_PROJECT_MISMATCH）".to_string());
    }
    let project = load_project_with_connection(&connection, project_id)?
        .ok_or_else(|| "任务项目不存在或已被删除（错误码：TASK_PROJECT_NOT_FOUND）".to_string())?;
    let session = if task.current_session_id.is_empty() {
        None
    } else {
        load_session_with_connection(&connection, &task.current_session_id)?
    };
    Ok(TaskUpdateSnapshot {
        task,
        project,
        session,
    })
}

/// 创建项目并返回刷新后的聚合数据。
/// 流程：校验项目名称和工作空间，写入 project 表，再返回当前项目数据。
/// 参数：app 用于定位数据库，request 为前端表单。
/// 返回：以新项目为当前项目的聚合数据。
/// 边界：项目名称必须唯一；工作目录只校验存在性，允许多个任务项目绑定同一仓库或父级工作区。
pub fn create_project(
    app: &AppHandle,
    request: CreateProjectRequest,
) -> Result<WorkspaceDataResponse, String> {
    let name = request.name.trim();
    let base_prompt = request.base_prompt.trim();
    let workspace_path = canonical_workspace_path(request.workspace_path.trim())?;
    if name.is_empty() {
        return Err("项目名称不能为空（错误码：TASK_PROJECT_NAME_REQUIRED）".to_string());
    }
    validate_project_name(name)?;
    validate_project_base_prompt(base_prompt)?;
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let project_id = create_project_record(&mut connection, name, &workspace_path, base_prompt)?;
    load_workspace_data_with_connection(&connection, Some(&project_id))
}

/// 在一个 Immediate 事务内校验项目名称唯一性并创建项目记录。
/// 流程：串行化写事务，检查项目名称冲突，随后插入新项目并提交；参数为数据库连接、名称和规范路径；返回新项目 ID。
/// 异常/边界：任一冲突或插入失败都会整体回滚；不依赖前端预检查，避免并发创建重复项目。
fn create_project_record(
    connection: &mut Connection,
    name: &str,
    workspace_path: &Path,
    base_prompt: &str,
) -> Result<String, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let project_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM project WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if project_count >= WORKSPACE_PROJECT_LIMIT {
        return Err(format!(
            "项目数量不能超过 {} 个（错误码：TASK_PROJECT_LIMIT_REACHED）",
            WORKSPACE_PROJECT_LIMIT
        ));
    }
    ensure_project_name_unique(&transaction, name, "")?;
    let project_id = next_id("proj");
    transaction
        .execute(
            "INSERT INTO project (id, name, workspace_path, base_prompt) VALUES (?1, ?2, ?3, ?4)",
            params![
                project_id,
                name,
                workspace_path.to_string_lossy(),
                base_prompt
            ],
        )
        .map_err(database_error)?;
    transaction.commit().map_err(database_error)?;
    Ok(project_id)
}

/// 编辑项目并返回刷新后的聚合数据。
/// 流程：校验名称和 canonical 工作目录存在性，在 Immediate 事务内按 ID 更新项目；参数为 AppHandle 与编辑请求；返回当前项目数据。
/// 异常/边界：项目不存在或并发删除时拒绝更新；已有 session 的工作目录快照不会被追溯修改。
pub fn update_project(
    app: &AppHandle,
    request: UpdateProjectRequest,
) -> Result<WorkspaceDataResponse, String> {
    let project_id = request.id.trim();
    let name = request.name.trim();
    let base_prompt = request.base_prompt.trim();
    if project_id.is_empty() {
        return Err("项目 ID 不能为空".to_string());
    }
    if name.is_empty() {
        return Err("项目名称不能为空".to_string());
    }
    validate_project_name(name)?;
    validate_project_base_prompt(base_prompt)?;
    let workspace_path = canonical_workspace_path(request.workspace_path.trim())?;
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    ensure_project_name_unique(&transaction, name, project_id)?;
    let changed = transaction
        .execute(
            "UPDATE project SET name = ?1, workspace_path = ?2, base_prompt = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?4 AND deleted_at IS NULL",
            params![name, workspace_path.to_string_lossy(), base_prompt, project_id],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("项目不存在或已被删除".to_string());
    }
    transaction.commit().map_err(database_error)?;
    load_workspace_data_with_connection(&connection, Some(project_id))
}

/// 校验项目名称在首版项目表中唯一。
/// 流程：查询名称冲突，编辑时通过 excluded_project_id 排除当前项目；参数为连接、名称和可空排除 ID；返回无。
/// 异常/边界：调用方必须处于 Immediate 写事务内，确保检查至提交之间没有其它写入插队；冲突时不写任何数据。
fn ensure_project_name_unique(
    connection: &Connection,
    name: &str,
    excluded_project_id: &str,
) -> Result<(), String> {
    let duplicate_name: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project WHERE name = ?1 AND deleted_at IS NULL AND (?2 = '' OR id <> ?2))",
            params![name, excluded_project_id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if duplicate_name {
        return Err("项目名称已存在（错误码：TASK_PROJECT_NAME_EXISTS）".to_string());
    }
    Ok(())
}

/// 校验项目名称的首发容量契约。
/// 流程：按 Unicode 字符数检查名称上限；参数为已 trim 的项目名称；成功返回空值。
/// 异常/边界：空值仍由调用入口返回必填错误，超过上限时在任何数据库写入前返回稳定错误码。
fn validate_project_name(name: &str) -> Result<(), String> {
    if name.chars().count() > PROJECT_NAME_MAX_CHARS {
        return Err(format!(
            "项目名称不能超过 {} 个字符（错误码：TASK_PROJECT_NAME_TOO_LONG）",
            PROJECT_NAME_MAX_CHARS
        ));
    }
    Ok(())
}

/// 校验项目基础提示词的容量边界。
/// 流程：允许空提示词，非空时按 Unicode 字符数检查上限；参数为已 trim 的基础提示词。
/// 返回：校验通过时无返回值。
/// 异常/边界：只校验项目字段自身容量，最终发送内容仍由任务 prompt 上限和 Codex 提交上限共同保护。
fn validate_project_base_prompt(base_prompt: &str) -> Result<(), String> {
    if base_prompt.chars().count() > PROJECT_BASE_PROMPT_MAX_CHARS {
        return Err(format!(
            "项目基础提示词不能超过 {} 个字符（错误码：TASK_PROJECT_BASE_PROMPT_TOO_LONG）",
            PROJECT_BASE_PROMPT_MAX_CHARS
        ));
    }
    Ok(())
}

/// 生成任务最终发送给 CodeX 的提示词。
/// 流程：项目基础提示词为空时返回任务正文；非空时用固定分隔标题拼接基础提示词和任务正文。
/// 参数：base_prompt 为项目配置的基础提示词，task_prompt 为任务自身内容。
/// 返回：用于执行和崩溃恢复匹配的完整提示词。
/// 异常/边界：调用方已在项目和任务写入时分别完成容量校验，本函数不再截断正文，避免恢复匹配与真实发送内容不一致。
fn build_task_execution_prompt(base_prompt: &str, task_prompt: &str) -> String {
    if base_prompt.trim().is_empty() {
        return task_prompt.to_string();
    }
    format!(
        "项目基础提示词：\n{}\n\n任务内容：\n{}",
        base_prompt.trim(),
        task_prompt
    )
}

/// 软删除任务项目并返回剩余聚合数据。
/// 流程：Immediate 事务内只标记项目删除时间；参数为 AppHandle 与项目 ID；返回默认剩余未删除项目数据。
/// 异常/边界：任务、会话历史不参与删除校验且不会级联变更；重复删除或未知项目返回稳定不存在错误。
pub fn delete_project(app: &AppHandle, project_id: &str) -> Result<WorkspaceDataResponse, String> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err("项目 ID 不能为空".to_string());
    }
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    delete_project_record(&mut connection, project_id)?;
    load_workspace_data_with_connection(&connection, None)
}

/// 在一个 Immediate 事务内软删除项目。
/// 流程：只更新项目 deleted_at 与 updated_at，不检查任务或会话关联；参数为数据库连接与项目 ID；返回无。
/// 异常/边界：任何历史任务、会话均保留原 project_id；错误返回时事务自动回滚。
fn delete_project_record(connection: &mut Connection, project_id: &str) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "UPDATE project SET deleted_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?1 AND deleted_at IS NULL",
            params![project_id],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("项目不存在或已被删除".to_string());
    }
    transaction.commit().map_err(database_error)
}

/// 创建任务并返回刷新后的项目数据。
/// 流程：校验项目、标题和提示词，写入已创建任务状态，记录创建事件。
/// 参数：app 用于定位数据库，request 为任务表单。
/// 返回：本次已提交任务 ID 与当前项目刷新后的扁平聚合数据。
/// 边界：并发同名任务依靠 createdTaskId 精确区分；任务创建后不会自动执行，只有进入排队中后调度器才处理。
pub fn create_task(
    app: &AppHandle,
    request: CreateTaskRequest,
) -> Result<CreateTaskResponse, String> {
    let project_id = request.project_id.trim();
    let title = request.title.trim();
    let prompt = request.prompt.trim();
    let attachments = validate_task_attachments(request.attachments)?;
    if project_id.is_empty() {
        return Err("请选择项目".to_string());
    }
    validate_task_content(title, prompt)?;
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let created_task_id =
        create_task_record(&mut connection, project_id, title, prompt, &attachments)?;
    let workspace = load_workspace_data_with_connection(&connection, Some(project_id))?;
    Ok(CreateTaskResponse {
        created_task_id,
        projects: workspace.projects,
        tasks: workspace.tasks,
        sessions: workspace.sessions,
    })
}

/// 更新尚未执行完成的任务并返回刷新后的项目数据。
/// 流程：校验任务 ID、标题和提示词，在 Immediate 事务内仅允许 created 或 queued 状态更新任务内容并追加事件。
/// 参数：app 用于定位数据库，request 为任务 ID 与新内容。
/// 返回：更新后的当前项目聚合数据。
/// 异常/边界：running、waiting_acceptance、completed、failed 均拒绝修改，避免篡改已经执行过的任务历史。
pub fn update_task(
    app: &AppHandle,
    request: UpdateTaskRequest,
) -> Result<WorkspaceDataResponse, String> {
    let task_id = request.id.trim();
    let title = request.title.trim();
    let prompt = request.prompt.trim();
    let attachments = validate_task_attachments(request.attachments)?;
    if task_id.is_empty() {
        return Err("任务 ID 不能为空（错误码：TASK_NOT_FOUND）".to_string());
    }
    validate_task_content(title, prompt)?;
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let project_id = update_task_record(&mut connection, task_id, title, prompt, &attachments)?;
    load_workspace_data_with_connection(&connection, Some(&project_id))
}

/// 删除非执行中的任务并返回刷新后的项目数据。
/// 流程：在 Immediate 事务内读取任务状态，running 状态拒绝删除，其余状态删除任务和关联事件后提交。
/// 参数：app 用于定位数据库，task_id 为待删除任务稳定 ID。
/// 返回：删除后的当前项目聚合数据。
/// 异常/边界：running 任务可能正在被调度器或 Codex 写回，必须保留；其它状态的关联 session 先清理后再删除任务。
pub fn delete_task(app: &AppHandle, task_id: &str) -> Result<WorkspaceDataResponse, String> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err("任务 ID 不能为空（错误码：TASK_NOT_FOUND）".to_string());
    }
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let project_id = delete_task_record(&mut connection, task_id)?;
    load_workspace_data_with_connection(&connection, Some(&project_id))
}

/// 校验任务标题与提示词的稳定输入边界。
/// 流程：先拒绝 trim 后空值，再按 Unicode 字符数检查标题与 prompt 上限；参数为已 trim 的业务文本；返回无。
/// 异常/边界：超限错误携带稳定错误码，不按 UTF-8 字节误伤中文；调用方必须在任何 SQLite 或 Codex 写入前执行。
fn validate_task_content(title: &str, prompt: &str) -> Result<(), String> {
    if title.is_empty() {
        return Err("任务标题不能为空（错误码：TASK_TITLE_REQUIRED）".to_string());
    }
    if title.chars().count() > TASK_TITLE_MAX_CHARS {
        return Err(format!(
            "任务标题不能超过 {} 个字符（错误码：TASK_TITLE_TOO_LONG）",
            TASK_TITLE_MAX_CHARS
        ));
    }
    if prompt.is_empty() {
        return Err("任务内容不能为空（错误码：TASK_PROMPT_REQUIRED）".to_string());
    }
    if prompt.chars().count() > TASK_PROMPT_MAX_CHARS {
        return Err(format!(
            "任务内容不能超过 {} 个字符（错误码：TASK_PROMPT_TOO_LONG）",
            TASK_PROMPT_MAX_CHARS
        ));
    }
    Ok(())
}

/// 校验任务图片附件并返回已收敛的附件列表。
/// 流程：限制数量、文件名、媒体类型和 data URL 长度/前缀，避免附件进入 prompt 或撑爆本机转发链路。
/// 异常/边界：只做结构和大小校验，不在 HTTP/RPC 日志中回显图片内容。
fn validate_task_attachments(
    attachments: Vec<TaskAttachmentRecord>,
) -> Result<Vec<TaskAttachmentRecord>, String> {
    if attachments.len() > TASK_ATTACHMENT_LIMIT {
        return Err(format!(
            "任务附件不能超过 {} 个（错误码：TASK_ATTACHMENT_COUNT_TOO_LONG）",
            TASK_ATTACHMENT_LIMIT
        ));
    }
    let mut normalized = Vec::with_capacity(attachments.len());
    for (index, attachment) in attachments.into_iter().enumerate() {
        let id = attachment.id.trim();
        let name = attachment.name.trim();
        let mime_type = attachment.mime_type.trim();
        let data_url = attachment.data_url.trim();
        if name.is_empty() {
            return Err(
                "任务附件名称不能为空（错误码：TASK_ATTACHMENT_NAME_REQUIRED）".to_string(),
            );
        }
        if name.chars().count() > 120 {
            return Err(
                "任务附件名称不能超过 120 个字符（错误码：TASK_ATTACHMENT_NAME_TOO_LONG）"
                    .to_string(),
            );
        }
        if !matches!(mime_type, "image/png" | "image/jpeg" | "image/webp") {
            return Err(
                "任务附件只支持 PNG、JPEG 或 WebP 图片（错误码：TASK_ATTACHMENT_MIME_UNSUPPORTED）"
                    .to_string(),
            );
        }
        let expected_prefix = format!("data:{};base64,", mime_type);
        if !data_url.starts_with(&expected_prefix) {
            return Err(
                "任务附件 data URL 格式无效（错误码：TASK_ATTACHMENT_DATA_URL_INVALID）"
                    .to_string(),
            );
        }
        if data_url.chars().count() > TASK_ATTACHMENT_DATA_URL_MAX_CHARS {
            return Err(format!(
                "单个任务附件不能超过 {} 个字符（错误码：TASK_ATTACHMENT_DATA_TOO_LONG）",
                TASK_ATTACHMENT_DATA_URL_MAX_CHARS
            ));
        }
        normalized.push(TaskAttachmentRecord {
            id: if id.is_empty() {
                format!("attachment-{}", index + 1)
            } else {
                id.to_string()
            },
            name: name.to_string(),
            mime_type: mime_type.to_string(),
            data_url: data_url.to_string(),
        });
    }
    Ok(normalized)
}

/// 序列化任务附件事件载荷。
/// 流程：使用稳定 JSON 对象包裹附件列表，解析方只读取 attachments 字段。
/// 异常/边界：序列化失败返回稳定业务错误，不把附件正文拼入错误。
fn serialize_task_attachments_payload(
    attachments: &[TaskAttachmentRecord],
) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({ "attachments": attachments }))
        .map_err(|_| "任务附件序列化失败（错误码：TASK_ATTACHMENT_DATA_URL_INVALID）".to_string())
}

/// 在一个 Immediate 事务内创建任务和唯一创建事件。
/// 流程：锁定写事务、确认项目存在、插入 created 任务并追加 created 事件，最后一次性提交；参数为连接及任务业务字段；返回新任务 ID。
/// 异常/边界：任务或事件任一步失败均整体回滚，不会留下无事件任务；任务创建后不会自动排队。
fn create_task_record(
    connection: &mut Connection,
    project_id: &str,
    title: &str,
    prompt: &str,
    attachments: &[TaskAttachmentRecord],
) -> Result<String, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    ensure_project_exists(&transaction, project_id)?;
    let task_id = next_id("task");
    transaction
        .execute(
            "INSERT INTO task (id, project_id, title, prompt, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                task_id,
                project_id,
                title,
                prompt,
                TaskStatus::Created.as_str()
            ],
        )
        .map_err(database_error)?;
    append_task_event(
        &transaction,
        &task_id,
        "created",
        "",
        TaskStatus::Created.as_str(),
        "任务已创建",
        &serialize_task_attachments_payload(attachments)?,
    )?;
    transaction.commit().map_err(database_error)?;
    Ok(task_id)
}

/// 在一个 Immediate 事务内更新可编辑任务内容。
/// 流程：读取任务所属项目与当前状态，确认状态仍为 created 或 queued 后更新标题和 prompt，并追加 task_updated 事件。
/// 异常/边界：状态不允许或并发状态变化时整笔回滚；queued 任务更新后仍保持队列位置，不重写 queued_at。
fn update_task_record(
    connection: &mut Connection,
    task_id: &str,
    title: &str,
    prompt: &str,
    attachments: &[TaskAttachmentRecord],
) -> Result<String, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let (project_id, status): (String, String) = transaction
        .query_row(
            "SELECT project_id, status FROM task WHERE id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "任务不存在或已被删除（错误码：TASK_NOT_FOUND）".to_string())?;
    if !matches!(status.as_str(), "created" | "queued") {
        return Err(
            "只有已创建或等待中的任务可以修改（错误码：TASK_UPDATE_STATUS_FORBIDDEN）".to_string(),
        );
    }
    let changed = transaction
        .execute(
            "UPDATE task SET title = ?1, prompt = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3 AND status = ?4",
            params![title, prompt, task_id, status],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("任务更新 CAS 未命中，已拒绝覆盖当前任务".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "task_updated",
        &status,
        &status,
        "任务名称和描述已更新",
        &serialize_task_attachments_payload(attachments)?,
    )?;
    transaction.commit().map_err(database_error)?;
    Ok(project_id)
}

/// 在一个 Immediate 事务内删除非运行中任务。
/// 流程：读取任务所属项目和状态，running 立即拒绝；其它状态先写删除事件，再清理事件、关联会话和任务记录。
/// 异常/边界：删除任务会移除其本地执行历史；running 任务拒绝删除以避免破坏执行回写和重启对账。
fn delete_task_record(connection: &mut Connection, task_id: &str) -> Result<String, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let (project_id, status): (String, String) = transaction
        .query_row(
            "SELECT project_id, status FROM task WHERE id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "任务不存在或已被删除（错误码：TASK_NOT_FOUND）".to_string())?;
    if status == TaskStatus::Running.as_str() {
        return Err("进行中的任务不能删除（错误码：TASK_DELETE_STATUS_FORBIDDEN）".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "task_deleted",
        &status,
        "",
        "任务已删除",
        "{}",
    )?;
    transaction
        .execute(
            "DELETE FROM task_event WHERE task_id = ?1",
            params![task_id],
        )
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM session WHERE task_id = ?1", params![task_id])
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "DELETE FROM task WHERE id = ?1 AND status <> ?2",
            params![task_id, TaskStatus::Running.as_str()],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("任务删除 CAS 未命中，已拒绝删除当前任务".to_string());
    }
    transaction.commit().map_err(database_error)?;
    Ok(project_id)
}

/// 将可安全重试的已创建或失败任务推入排队中，并返回可执行的队列任务。
/// 流程：在同一 Immediate 事务读取任务状态及当前会话提交状态；只允许 created 或明确未发送的 failed 进入 queued，再返回完整执行上下文。
/// 参数：app 用于定位数据库，task_id 为目标任务。
/// 返回：队列任务记录，供调用方启动 CodeX 会话。
/// 边界：running、queued、waiting_acceptance、completed 不能重复排队；发送结果不确定的 failed 必须人工对账，禁止重排造成重复 prompt。
pub fn queue_task(app: &AppHandle, task_id: &str) -> Result<QueuedTaskRecord, String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    queue_task_with_connection(&mut connection, task_id)
}

/// 在连接门禁前只读预检任务是否处于发送不确定状态。
/// 流程：打开现有业务库，不执行 schema 初始化；表存在时读取当前 session externalStatus，sendUncertain 返回稳定 409 业务码；参数为 AppHandle 和任务 ID。
/// 异常/边界：首发空库或表尚不存在直接放行给后续连接/事务门禁；数据库异常显式失败，事务内 queue_task 仍会再次检查以关闭并发窗口。
pub fn ensure_task_queue_retry_allowed(app: &AppHandle, task_id: &str) -> Result<(), String> {
    let connection = open_database(app)?;
    let has_task_table: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'task')",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if !has_task_table {
        return Ok(());
    }
    let external_status = connection
        .query_row(
            "SELECT COALESCE(s.external_status, '') FROM task t LEFT JOIN session s ON s.id = t.current_session_id WHERE t.id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(database_error)?;
    if external_status.as_deref() == Some("sendUncertain") {
        return Err(
            "任务发送结果不确定，必须先人工对账，禁止再次入队（错误码：CODEX_SEND_UNCERTAIN）"
                .to_string(),
        );
    }
    Ok(())
}

/// 在已初始化连接上事务化执行任务入队。
/// 流程：Immediate 事务读取执行上下文、任务状态和当前 session 外部状态，安全状态才 CAS 为 queued 并提交；参数为连接和任务 ID；返回队列上下文。
/// 异常/边界：sendUncertain 在任何状态或事件写入前拒绝；失败事务自动回滚，供生产入口和内存测试共用同一逻辑。
fn queue_task_with_connection(
    connection: &mut Connection,
    task_id: &str,
) -> Result<QueuedTaskRecord, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let task = read_queued_task_context(&transaction, task_id)?;
    let external_status: String = transaction
        .query_row(
            "SELECT COALESCE(s.external_status, '') FROM task t LEFT JOIN session s ON s.id = t.current_session_id WHERE t.id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if external_status == "sendUncertain" {
        return Err(
            "任务发送结果不确定，必须先人工对账，禁止再次入队（错误码：CODEX_SEND_UNCERTAIN）"
                .to_string(),
        );
    }
    let current_status: String = transaction
        .query_row(
            "SELECT status FROM task WHERE id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if !matches!(current_status.as_str(), "created" | "failed") {
        return Err("当前状态不能进入排队中".to_string());
    }
    update_task_status(
        &transaction,
        task_id,
        &current_status,
        TaskStatus::Queued.as_str(),
        "",
    )?;
    transaction.commit().map_err(database_error)?;
    Ok(task)
}

/// 将待验收任务标记为已完成，并同步会话状态。
/// 流程：校验任务必须为 waiting_acceptance，再把任务和关联会话一起置为 completed。
/// 参数：app 用于定位数据库，task_id 为目标任务。
/// 返回：刷新后的聚合数据。
/// 边界：非待验收任务不能直接完成，避免绕过执行链路。
pub fn complete_task(app: &AppHandle, task_id: &str) -> Result<WorkspaceDataResponse, String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let (project_id, status, session_id): (String, String, String) = transaction
        .query_row(
            "SELECT project_id, status, COALESCE(current_session_id, '') FROM task WHERE id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(database_error)?;
    if status != TaskStatus::WaitingAcceptance.as_str() {
        return Err("只有待验收任务可以标记为已完成".to_string());
    }
    update_task_status(
        &transaction,
        task_id,
        &status,
        TaskStatus::Completed.as_str(),
        "",
    )?;
    if !session_id.is_empty() {
        let changed = transaction
            .execute(
                "UPDATE session SET status = ?1, last_error = '', updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND task_id = ?3 AND status = ?4",
                params![SessionStatus::Completed.as_str(), session_id, task_id, SessionStatus::WaitingAcceptance.as_str()],
            )
            .map_err(database_error)?;
        if changed != 1 {
            return Err("会话验收状态 CAS 未命中，已拒绝部分完成".to_string());
        }
    }
    transaction.commit().map_err(database_error)?;
    load_workspace_data_with_connection(&connection, Some(&project_id))
}

/// 记录任务已经开始运行并绑定本地会话。
/// 流程：创建 session 记录，将任务状态从 queued 改为 running 并保存 current_session_id。
/// 参数：app 用于定位数据库，task 为调度候选任务。
/// 返回：新建的本地会话 ID。
/// 边界：只有 queued 状态会被更新，避免重复执行覆盖已有会话。
pub fn mark_task_running(app: &AppHandle, task: &QueuedTaskRecord) -> Result<String, String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let session_id = next_id("sess");
    let changed = transaction
        .execute(
            "UPDATE task SET status = ?1, current_session_id = ?2, started_at = CURRENT_TIMESTAMP, finished_at = NULL, last_error = '', result_json = '{}', updated_at = CURRENT_TIMESTAMP WHERE id = ?3 AND status = ?4",
            params![TaskStatus::Running.as_str(), session_id, task.id, TaskStatus::Queued.as_str()],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("任务已被其它调度器领取，已拒绝重复执行".to_string());
    }
    transaction
        .execute(
            "INSERT INTO session (id, project_id, task_id, provider, workspace_path, title, status) VALUES (?1, ?2, ?3, 'codex', ?4, ?5, ?6)",
            params![
                session_id,
                task.project_id,
                task.id,
                task.workspace_path,
                task.title,
                SessionStatus::Running.as_str()
            ],
        )
        .map_err(database_error)?;
    append_task_event(
        &transaction,
        &task.id,
        "status_changed",
        TaskStatus::Queued.as_str(),
        TaskStatus::Running.as_str(),
        "任务开始执行",
        "{}",
    )?;
    transaction.commit().map_err(database_error)?;
    Ok(session_id)
}

/// 持久化 app-server 创建的 thread/turn 标识，但保持任务 running。
/// 流程：Immediate 事务内以 task=running 且 session 匹配为 CAS 条件，同时写 session 与事件；参数为本地/外部 ID；返回无。
/// 异常/边界：任一标识为空或状态已变化会拒绝覆盖，避免迟到响应污染新一轮执行。
pub fn bind_task_execution(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    bind_task_execution_with_connection(&mut connection, task_id, session_id, thread_id, turn_id)
}

/// 在现有数据库连接上原子绑定 Codex turn，供正常响应与崩溃恢复共用同一 CAS 边界。
/// 流程：校验全部标识后开启 Immediate 事务，只允许 running 会话从空 turnId 绑定一次，并同步追加任务事件。
/// 参数：connection 为已完成 schema 初始化的连接，其余参数标识当前本地执行与 Codex thread/turn；返回事务提交结果。
/// 异常/边界：任一标识为空、thread 不匹配、turn 已绑定或状态已变化均拒绝写入，避免迟到响应覆盖恢复结果。
fn bind_task_execution_with_connection(
    connection: &mut Connection,
    task_id: &str,
    session_id: &str,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), String> {
    if [task_id, session_id, thread_id, turn_id]
        .iter()
        .any(|value| value.trim().is_empty())
    {
        return Err("绑定任务执行时本地或 Codex 标识不能为空".to_string());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "UPDATE session SET external_turn_id = ?1, external_status = 'inProgress', updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND task_id = ?3 AND external_thread_id = ?4 AND external_client_message_id <> '' AND external_turn_id = '' AND status = 'running'",
            params![turn_id, session_id, task_id, thread_id],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("任务执行绑定已过期，已拒绝覆盖当前会话".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "codex_execution_bound",
        TaskStatus::Running.as_str(),
        TaskStatus::Running.as_str(),
        "已绑定 Codex thread 与 turn，继续等待可靠终态",
        &serde_json::json!({"threadId": thread_id, "turnId": turn_id}).to_string(),
    )?;
    transaction.commit().map_err(database_error)
}

/// 在 turn/start 前持久化任务专用 thread 与请求关联 ID。
/// 流程：以 running session 为 CAS 条件写入外部 thread 和 clientUserMessageId 并记录事件；参数为本地任务/会话及外部标识；返回无。
/// 异常/边界：必须先于 turn/start 提交；Codex 0.147.0 的 thread/read 不支持按 clientUserMessageId 查询，因此恢复只依赖专用 thread 的唯一 turn，绝不重发 prompt。
pub fn bind_task_thread(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
    thread_id: &str,
    client_user_message_id: &str,
) -> Result<(), String> {
    if thread_id.trim().is_empty() || client_user_message_id.trim().is_empty() {
        return Err("绑定 Codex thread 时外部标识不能为空".to_string());
    }
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "UPDATE session SET external_thread_id = ?1, external_client_message_id = ?2, external_url = ?3, external_status = 'threadCreated', updated_at = CURRENT_TIMESTAMP WHERE id = ?4 AND task_id = ?5 AND status = 'running' AND external_thread_id = ''",
            params![thread_id, client_user_message_id, format!("codex://threads/{}", thread_id), session_id, task_id],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("Codex thread 绑定 CAS 未命中".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "codex_thread_bound",
        "running",
        "running",
        "已持久化 Codex 任务专用 thread 和请求关联 ID",
        &serde_json::json!({"threadId": thread_id, "clientUserMessageId": client_user_message_id})
            .to_string(),
    )?;
    transaction.commit().map_err(database_error)
}

/// 根据 app-server 的可靠 turn 终态完成或失败任务。
/// 流程：只接受 completed/failed/interrupted，以 thread/turn/session/task 全量 CAS 更新两表和事件；参数含安全结果或错误；返回无。
/// 异常/边界：inProgress 等非终态会拒绝；completed 进入 waiting_acceptance；interrupted 仅在已有最终回复时进入 waiting_acceptance，否则仍失败。
pub fn finish_task_execution(
    app: &AppHandle,
    running: &RunningTaskRecord,
    turn_status: &str,
    result_json: &str,
    error: &str,
) -> Result<(), String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    finish_task_execution_with_connection(&mut connection, running, turn_status, result_json, error)
}

/// 在现有数据库连接上提交 Codex turn 终态，供运行态对账和单测共用同一 CAS 边界。
/// 流程：先校验状态与结果 JSON，再根据终态和 finalText 判断本地任务是否可验收，最后原子更新 session/task 并追加事件。
/// 参数：connection 为已初始化连接，running 为绑定快照，turn_status/result_json/error 来自只读对账。
/// 异常/边界：failed 永远失败；interrupted 没有可展示回复时失败且清空结果，避免把空中断伪装成完成。
fn finish_task_execution_with_connection(
    connection: &mut Connection,
    running: &RunningTaskRecord,
    turn_status: &str,
    result_json: &str,
    error: &str,
) -> Result<(), String> {
    validate_task_result_json(result_json)?;
    let has_usable_result = task_result_has_final_text(result_json)?;
    let (task_status, session_status, message) = match turn_status {
        "completed" => (
            TaskStatus::WaitingAcceptance.as_str(),
            SessionStatus::WaitingAcceptance.as_str(),
            "Codex turn 已可靠完成，等待人工验收",
        ),
        "interrupted" if has_usable_result => (
            TaskStatus::WaitingAcceptance.as_str(),
            SessionStatus::WaitingAcceptance.as_str(),
            "Codex turn 已返回可验收内容，等待人工验收",
        ),
        "failed" | "interrupted" => (
            TaskStatus::Failed.as_str(),
            SessionStatus::Failed.as_str(),
            "Codex turn 执行失败",
        ),
        _ => return Err(format!("Codex turn 状态 {} 不是可靠终态", turn_status)),
    };
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let safe_error = limit_text(error, 1_000);
    let safe_result = if task_status == TaskStatus::WaitingAcceptance.as_str() {
        result_json
    } else {
        "{}"
    };
    let session_changed = transaction
        .execute(
            "UPDATE session SET status = ?1, external_status = ?2, last_error = ?3, result_json = ?4, updated_at = CURRENT_TIMESTAMP WHERE id = ?5 AND task_id = ?6 AND external_thread_id = ?7 AND external_turn_id = ?8 AND status = 'running'",
            params![session_status, turn_status, safe_error, safe_result, running.session_id, running.task_id, running.thread_id, running.turn_id],
        )
        .map_err(database_error)?;
    let task_changed = transaction
        .execute(
            "UPDATE task SET status = ?1, last_error = ?2, result_json = ?3, finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?4 AND current_session_id = ?5 AND status = 'running'",
            params![task_status, safe_error, safe_result, running.task_id, running.session_id],
        )
        .map_err(database_error)?;
    if session_changed != 1 || task_changed != 1 {
        return Err("任务终态 CAS 未命中，已拒绝重复或迟到的完成通知".to_string());
    }
    append_task_event(
        &transaction,
        &running.task_id,
        "codex_turn_terminal",
        TaskStatus::Running.as_str(),
        task_status,
        message,
        &serde_json::json!({"threadId": running.thread_id, "turnId": running.turn_id, "turnStatus": turn_status}).to_string(),
    )?;
    transaction.commit().map_err(database_error)
}

/// 判断任务结果中是否已有可展示的助手最终回复。
/// 流程：解析内部 result JSON，只认非空 finalText 字符串为可验收内容；参数为待落库结果；返回是否可展示。
/// 异常/边界：畸形 JSON 由调用方统一校验，这里仍 fail closed；纯空白回复不视为成功。
fn task_result_has_final_text(result_json: &str) -> Result<bool, String> {
    let value = serde_json::from_str::<Value>(result_json)
        .map_err(|_| "任务结果不是有效 JSON（错误码：TASK_RESULT_INVALID）".to_string())?;
    Ok(value
        .get("finalText")
        .and_then(Value::as_str)
        .map(|text| !text.trim().is_empty())
        .unwrap_or(false))
}

/// 校验写入 task/session 的可靠终态结果 JSON。
/// 流程：先按 UTF-8 字节检查数据库与 IPC 大小上限，再解析 JSON 确认结构可读；参数为内部生成的结果字符串；返回无。
/// 异常/边界：失败与中断仍传入 `{}`；任何超限或畸形结果都拒绝落库并携带稳定错误码，由终态对账保留重试诊断。
fn validate_task_result_json(result_json: &str) -> Result<(), String> {
    if result_json.len() > TASK_RESULT_JSON_MAX_BYTES {
        return Err(format!(
            "任务结果超过 {} 字节上限（错误码：TASK_RESULT_TOO_LARGE）",
            TASK_RESULT_JSON_MAX_BYTES
        ));
    }
    serde_json::from_str::<serde_json::Value>(result_json)
        .map(|_| ())
        .map_err(|_| "任务结果不是有效 JSON（错误码：TASK_RESULT_INVALID）".to_string())
}

/// 查询重启后仍为 running 的任务，供 App setup 与 Codex 真实状态对账。
/// 流程：读取任务当前会话和外部标识；参数为 AppHandle；返回运行中快照列表。
/// 异常/边界：不修改状态，不擅自重放 prompt；缺少 turnId 由调用方从任务专用 thread 的唯一 turn 只读恢复。
pub fn list_running_tasks(app: &AppHandle) -> Result<Vec<RunningTaskRecord>, String> {
    let connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let mut statement = connection
        .prepare("SELECT t.id, t.project_id, s.id, s.external_thread_id, s.external_turn_id FROM task t JOIN session s ON s.id = t.current_session_id WHERE t.status = 'running' AND s.status = 'running'")
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(RunningTaskRecord {
                task_id: row.get(0)?,
                project_id: row.get(1)?,
                session_id: row.get(2)?,
                thread_id: row.get(3)?,
                turn_id: row.get(4)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 判断任务库中是否存在真实执行中的任务。
/// 流程：初始化首版结构后使用 EXISTS 匹配任一 running task 或任一 running session；参数为 AppHandle；返回是否存在活动或不一致执行记录。
/// 异常/边界：task/session 任一侧仍为 running 都按活动处理；数据库读取失败显式返回，重启调用方必须 fail closed，不能把未知状态当作空闲。
pub fn has_running_task(app: &AppHandle) -> Result<bool, String> {
    let connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    has_running_task_with_connection(&connection)
}

/// 在已初始化连接上执行 fail-closed 活动任务判断。
/// 流程：用单条 EXISTS 同时检查 task 和 session 任一侧 running；参数为数据库连接；返回是否存在活动或不一致记录。
/// 异常/边界：不要求两表 join 一致，任一侧残留 running 都阻止重启；查询异常显式返回。
fn has_running_task_with_connection(connection: &Connection) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM task WHERE status = 'running' UNION ALL SELECT 1 FROM session WHERE status = 'running' LIMIT 1)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database_error)
}

/// 读取 CDP Enter 已开始但尚未绑定 thread 的崩溃恢复上下文。
/// 流程：精确连接 running task/session/project，要求 externalStatus=cdpSubmitStarted、thread 为空并解析内部 submission 毫秒水位；参数为任务和会话 ID；返回可选上下文。
/// 异常/边界：只读不改变状态；字段缺失或水位非法显式失败，普通未提交 running 会话返回 None，调用方不得据此重放 prompt。
pub fn load_pending_submission(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
) -> Result<Option<PendingSubmissionRecord>, String> {
    let connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    load_pending_submission_with_connection(&connection, task_id, session_id)
}

/// 在已初始化连接上读取并严格解析一次待恢复 CDP 提交。
/// 流程：连接 running task/session 后解析版本化提交状态、正水位、请求 ID 和完整旧 thread 快照；参数为连接及本地任务/会话 ID；返回可选恢复上下文。
/// 异常/边界：旧字符串格式、零水位、空 thread ID 或缺失关联 ID 均显式失败；供生产入口与事务调用链测试复用同一解析实现。
fn load_pending_submission_with_connection(
    connection: &Connection,
    task_id: &str,
    session_id: &str,
) -> Result<Option<PendingSubmissionRecord>, String> {
    let record = connection
        .query_row(
            "SELECT t.id, t.project_id, s.id, s.workspace_path, t.prompt, p.base_prompt, s.external_url, s.external_client_message_id FROM task t JOIN session s ON s.id = t.current_session_id JOIN project p ON p.id = t.project_id WHERE t.id = ?1 AND s.id = ?2 AND t.status = 'running' AND s.status = 'running' AND s.external_status = 'cdpSubmitStarted' AND s.external_thread_id = ''",
            params![task_id, session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((
        task_id,
        project_id,
        session_id,
        workspace_path,
        prompt,
        base_prompt,
        state_json,
        client_id,
    )) = record
    else {
        return Ok(None);
    };
    let state = serde_json::from_str::<PendingSubmissionState>(&state_json)
        .map_err(|_| "CDP 提交恢复状态无效".to_string())?;
    if state.version != 1 || state.submitted_at_ms <= 0 {
        return Err("CDP 提交恢复时间水位无效".to_string());
    }
    if state
        .known_thread_ids
        .iter()
        .any(|thread_id| thread_id.trim().is_empty())
    {
        return Err("CDP 提交恢复旧 thread 快照无效".to_string());
    }
    if client_id.trim().is_empty() {
        return Err("CDP 提交恢复请求关联 ID 缺失".to_string());
    }
    Ok(Some(PendingSubmissionRecord {
        task_id,
        project_id,
        session_id,
        workspace_path,
        prompt: build_task_execution_prompt(&base_prompt, &prompt),
        submitted_at_ms: state.submitted_at_ms,
        client_user_message_id: client_id,
        known_thread_ids: state.known_thread_ids,
    }))
}

/// 把 CDP 提交结果不确定的任务原子转为不可自动重排的失败态。
/// 流程：在 Immediate 事务内同时把 running session 标记为 sendUncertain、task 标记为 failed，并追加稳定事件；参数标识唯一任务和本地会话。
/// 异常/边界：只接受当前 running 配对，任一 CAS 未命中整笔回滚；错误正文受限且不保存 prompt、DOM 或 CDP 地址。
pub fn mark_task_send_uncertain(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
    error: &str,
) -> Result<(), String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    mark_task_send_uncertain_with_connection(&mut connection, task_id, session_id, error)
}

/// 在已初始化连接上原子写入发送不确定终态。
/// 流程：Immediate 事务 CAS 更新 session/task 并追加单条事件；参数为连接、任务、会话和安全错误；返回提交结果。
/// 异常/边界：任一 CAS 未命中整笔回滚，供生产入口与内存事务测试复用。
fn mark_task_send_uncertain_with_connection(
    connection: &mut Connection,
    task_id: &str,
    session_id: &str,
    error: &str,
) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let safe_error = limit_text(error, 1_000);
    let session_changed = transaction
        .execute(
            "UPDATE session SET status = 'failed', external_status = 'sendUncertain', last_error = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND task_id = ?3 AND status = 'running'",
            params![safe_error, session_id, task_id],
        )
        .map_err(database_error)?;
    let task_changed = transaction
        .execute(
            "UPDATE task SET status = 'failed', last_error = ?1, finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND current_session_id = ?3 AND status = 'running'",
            params![safe_error, task_id, session_id],
        )
        .map_err(database_error)?;
    if session_changed != 1 || task_changed != 1 {
        return Err("发送不确定状态 CAS 未命中，已拒绝部分更新".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "codex_send_uncertain",
        TaskStatus::Running.as_str(),
        TaskStatus::Failed.as_str(),
        "Codex Desktop 提交结果不确定，禁止自动重排",
        "{}",
    )?;
    transaction.commit().map_err(database_error)
}

/// 在向 Codex Desktop 发送 Enter 前持久化提交阶段和时间水位。
/// 流程：Immediate 事务把正毫秒水位、请求 ID 与权威旧 thread 快照作为版本化 JSON 原子写入并标记 cdpSubmitStarted，再追加不含 prompt 的事件。
/// 参数：任务、会话、请求关联 ID、提交水位和完整旧 thread 列表；异常/边界：非法快照或 CAS 失败时绝不能继续执行 Enter。
pub fn mark_task_submission_started(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
    client_user_message_id: &str,
    submitted_at_ms: i64,
    known_thread_ids: &[String],
) -> Result<(), String> {
    if task_id.trim().is_empty()
        || session_id.trim().is_empty()
        || client_user_message_id.trim().is_empty()
        || submitted_at_ms <= 0
    {
        return Err("Codex Desktop 提交阶段参数无效".to_string());
    }
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    mark_task_submission_started_with_connection(
        &mut connection,
        task_id,
        session_id,
        client_user_message_id,
        submitted_at_ms,
        known_thread_ids,
    )
}

/// 在已初始化连接上事务化持久化 CDP 提交水位。
/// 流程：校验提交身份与旧 thread 快照，序列化严格版本对象后执行 Immediate CAS 并追加事件；参数为连接和完整提交身份；返回提交结果。
/// 异常/边界：只允许空 thread/空 client ID 的 running session 写一次；空数组只代表权威确认没有旧 thread，旧字符串格式绝不兼容。
fn mark_task_submission_started_with_connection(
    connection: &mut Connection,
    task_id: &str,
    session_id: &str,
    client_user_message_id: &str,
    submitted_at_ms: i64,
    known_thread_ids: &[String],
) -> Result<(), String> {
    if task_id.trim().is_empty()
        || session_id.trim().is_empty()
        || client_user_message_id.trim().is_empty()
        || submitted_at_ms <= 0
    {
        return Err("Codex Desktop 提交阶段参数无效".to_string());
    }
    if known_thread_ids
        .iter()
        .any(|thread_id| thread_id.trim().is_empty())
    {
        return Err("Codex Desktop 提交旧 thread 快照无效".to_string());
    }
    let submission_state = serde_json::to_string(&PendingSubmissionState {
        version: 1,
        submitted_at_ms,
        known_thread_ids: known_thread_ids.to_vec(),
    })
    .map_err(|_| "Codex Desktop 提交恢复状态序列化失败".to_string())?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "UPDATE session SET external_client_message_id = ?1, external_status = 'cdpSubmitStarted', external_url = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3 AND task_id = ?4 AND status = 'running' AND external_thread_id = '' AND external_client_message_id = ''",
            params![client_user_message_id, submission_state, session_id, task_id],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err("Codex Desktop 提交阶段 CAS 未命中".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "codex_submission_started",
        TaskStatus::Running.as_str(),
        TaskStatus::Running.as_str(),
        "已持久化 Codex Desktop 提交时间水位，后续禁止自动重放",
        &serde_json::json!({"submittedAtMs": submitted_at_ms}).to_string(),
    )?;
    transaction.commit().map_err(database_error)
}

/// 查询 App 重启前已持久化但尚未领取的 queued 任务。
/// 流程：连接 project 补齐 canonical workspace 上下文并按排队时间顺序返回；参数为 AppHandle；返回队列记录。
/// 异常/边界：只读不改状态，实际领取仍必须经过 mark_task_running 的 CAS，避免多调度器重复执行。
pub fn list_queued_tasks(app: &AppHandle) -> Result<Vec<QueuedTaskRecord>, String> {
    let connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    list_queued_tasks_with_connection(&connection)
}

/// 按真实入队事件顺序查询尚未领取的任务。
/// 流程：连接 project 补齐工作空间，以最近一次 queued 事件的 SQLite rowid 作为事务提交顺序，再用创建时间和 ID 稳定兜底。
/// 参数：connection 为已完成首版 schema 初始化的数据库连接；返回可由唯一调度器领取的队列记录。
/// 异常/边界：失败重排会生成新的 queued 事件并排到队尾；本方法只读不领取，领取仍由 mark_task_running 的 CAS 保证唯一。
fn list_queued_tasks_with_connection(
    connection: &Connection,
) -> Result<Vec<QueuedTaskRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.project_id, t.title, t.prompt, p.workspace_path, p.base_prompt
               FROM task t
               JOIN project p ON p.id = t.project_id
              WHERE t.status = 'queued'
                AND p.deleted_at IS NULL
           ORDER BY (SELECT MAX(e.rowid) FROM task_event e WHERE e.task_id = t.id AND e.to_status = 'queued') ASC,
                    t.created_at ASC,
                    t.id ASC",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            let task_id: String = row.get(0)?;
            Ok(QueuedTaskRecord {
                id: task_id.clone(),
                project_id: row.get(1)?,
                title: row.get(2)?,
                prompt: build_task_execution_prompt(
                    row.get::<_, String>(5)?.as_str(),
                    row.get::<_, String>(3)?.as_str(),
                ),
                attachments: latest_task_attachments(connection, &task_id)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?,
                workspace_path: row.get(4)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 记录任务执行失败，供前端卡片展示错误并允许重新排队。
/// 流程：把任务和会话一起置为 failed，并记录错误事件。
/// 参数：app 用于定位数据库，task_id/session_id/error 为失败上下文。
/// 返回：无返回值。
/// 边界：错误信息会截断，避免外部响应体撑大本地数据库。
pub fn mark_task_failed(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
    error: &str,
) -> Result<(), String> {
    let mut connection = open_database(app)?;
    initialize_database_schema(&connection)?;
    let message = limit_text(error, 500);
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let session_changed = transaction
        .execute(
            "UPDATE session SET status = 'failed', external_status = CASE WHEN external_status = '' THEN 'localFailure' ELSE external_status END, last_error = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND task_id = ?3 AND status = 'running'",
            params![message, session_id, task_id],
        )
        .map_err(database_error)?;
    let task_changed = transaction
        .execute(
            "UPDATE task SET status = 'failed', last_error = ?1, result_json = '{}', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND current_session_id = ?3 AND status = 'running'",
            params![message, task_id, session_id],
        )
        .map_err(database_error)?;
    if session_changed != 1 || task_changed != 1 {
        return Err("任务失败状态 CAS 未命中，已拒绝重复或迟到更新".to_string());
    }
    append_task_event(
        &transaction,
        task_id,
        "execution_failed",
        TaskStatus::Running.as_str(),
        TaskStatus::Failed.as_str(),
        "任务执行失败",
        &serde_json::json!({"error": message}).to_string(),
    )?;
    transaction.commit().map_err(database_error)
}

/// 打开用户级业务数据库，确保升级应用不会覆盖用户数据。
fn open_database(app: &AppHandle) -> Result<Connection, String> {
    let database_path = database_file_path(app)?;
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建数据目录失败：{}", error))?;
    }
    let connection = Connection::open(database_path).map_err(database_error)?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(database_error)?;
    Ok(connection)
}

/// 计算业务数据库文件路径。
fn database_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败：{}", error))?;
    path.push("data");
    path.push("aitool.sqlite");
    Ok(path)
}

/// 幂等初始化首发数据库结构。
/// 流程：先枚举全部用户 schema 对象；真正空库原子创建首发结构，非空库则严格校验当前元数据与完整表/索引定义。
/// 参数：connection 为已打开且启用外键约束的业务库连接；返回无。
/// 异常/边界：只接受零对象空库或精确当前首发结构；高版本、错误元数据、缺表、多表、异常列/索引均 fail closed，不执行 ALTER、数据转换或历史兼容兜底。
fn initialize_database_schema(connection: &Connection) -> Result<(), String> {
    if read_schema_objects(connection)?.is_empty() {
        apply_initial_schema(connection)?;
    }
    validate_current_schema(connection)
}

/// 应用首版任务管理表结构。
/// 流程：在单一事务内一次性创建项目、任务、会话、任务事件及必要索引，再登记唯一 schema 元数据并提交。
/// 参数：connection 为已确认零用户对象的数据库连接；返回无。
/// 异常/边界：项目使用 deleted_at 软删除，不级联任务或会话；任一 SQLite 错误整体回滚，调用方不得在非空库上调用或伪装初始化成功。
fn apply_initial_schema(connection: &Connection) -> Result<(), String> {
    let transaction = connection.unchecked_transaction().map_err(database_error)?;
    transaction
        .execute_batch(
            "
            CREATE TABLE schema_metadata (
              version INTEGER PRIMARY KEY,
              name TEXT NOT NULL,
              applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              checksum TEXT NOT NULL
            );

            CREATE TABLE project (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              workspace_path TEXT NOT NULL,
              base_prompt TEXT NOT NULL DEFAULT '',
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              deleted_at TEXT
            );

            CREATE TABLE session (
              id TEXT PRIMARY KEY,
              project_id TEXT NOT NULL,
              task_id TEXT,
              provider TEXT NOT NULL,
              workspace_path TEXT NOT NULL,
              title TEXT NOT NULL DEFAULT '',
              status TEXT NOT NULL,
              external_thread_id TEXT NOT NULL DEFAULT '',
              external_turn_id TEXT NOT NULL DEFAULT '',
              external_client_message_id TEXT NOT NULL DEFAULT '',
              external_url TEXT NOT NULL DEFAULT '',
              external_status TEXT NOT NULL DEFAULT '',
              last_error TEXT NOT NULL DEFAULT '',
              result_json TEXT NOT NULL DEFAULT '{}',
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              FOREIGN KEY(project_id) REFERENCES project(id),
              FOREIGN KEY(task_id) REFERENCES task(id)
            );

            CREATE TABLE task (
              id TEXT PRIMARY KEY,
              project_id TEXT NOT NULL,
              title TEXT NOT NULL,
              prompt TEXT NOT NULL,
              status TEXT NOT NULL,
              current_session_id TEXT NOT NULL DEFAULT '',
              last_error TEXT NOT NULL DEFAULT '',
              result_json TEXT NOT NULL DEFAULT '{}',
              queued_at TEXT,
              started_at TEXT,
              finished_at TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              FOREIGN KEY(project_id) REFERENCES project(id)
            );

            CREATE TABLE task_event (
              id TEXT PRIMARY KEY,
              task_id TEXT NOT NULL,
              event_type TEXT NOT NULL,
              from_status TEXT NOT NULL DEFAULT '',
              to_status TEXT NOT NULL DEFAULT '',
              message TEXT NOT NULL DEFAULT '',
              payload_json TEXT NOT NULL DEFAULT '{}',
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              FOREIGN KEY(task_id) REFERENCES task(id)
            );

            CREATE INDEX idx_project_workspace ON project(workspace_path);
            CREATE INDEX idx_task_project_status ON task(project_id, status, created_at);
            CREATE INDEX idx_task_current_session ON task(current_session_id);
            CREATE INDEX idx_session_project_status ON session(project_id, provider, status, updated_at);
            CREATE INDEX idx_session_external_thread ON session(provider, external_thread_id);
            CREATE INDEX idx_task_event_task_time ON task_event(task_id, created_at);
            ",
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "INSERT INTO schema_metadata (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![
                INITIAL_SCHEMA_VERSION,
                INITIAL_SCHEMA_NAME,
                INITIAL_SCHEMA_CHECKSUM
            ],
        )
        .map_err(database_error)?;
    transaction.commit().map_err(database_error)
}

/// 校验非空业务库是否精确等于当前首发 schema。
/// 流程：先确认 schema_metadata 表存在并读取全部登记，允许旧首发结构迁移项目软删除和基础提示词字段，再与当前内存库的完整用户表和索引定义逐项比较。
/// 参数：connection 为待校验业务库；返回无。
/// 异常/边界：触发器、额外表/索引、缺失对象、异常 SQL 定义或元数据列损坏均拒绝；错误只返回稳定诊断码，不泄漏本地 schema 正文。
fn validate_current_schema(connection: &Connection) -> Result<(), String> {
    let actual_objects = read_schema_objects(connection)?;
    if !actual_objects
        .iter()
        .any(|(_, name, _)| name == "schema_metadata")
    {
        return Err(
            "任务管理数据库缺少首发 schema 元数据（错误码：TASK_SCHEMA_METADATA_INVALID）"
                .to_string(),
        );
    }
    let mut statement = connection
        .prepare("SELECT version, name, checksum FROM schema_metadata ORDER BY version")
        .map_err(database_error)?;
    let metadata = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    if metadata
        .iter()
        .any(|(version, _, _)| *version > INITIAL_SCHEMA_VERSION)
    {
        return Err(
            "任务管理数据库版本高于当前应用（错误码：TASK_SCHEMA_VERSION_UNSUPPORTED）".to_string(),
        );
    }
    if metadata.len() == 1
        && metadata[0].0 == INITIAL_SCHEMA_VERSION
        && metadata[0].1 == INITIAL_SCHEMA_NAME
        && metadata[0].2 == LEGACY_SCHEMA_CHECKSUM_SESSION_TASK
    {
        migrate_legacy_project_soft_delete(connection)?;
        migrate_legacy_project_base_prompt(connection, LEGACY_SCHEMA_CHECKSUM_PROJECT_SOFT_DELETE)?;
    } else if metadata.len() == 1
        && metadata[0].0 == INITIAL_SCHEMA_VERSION
        && metadata[0].1 == INITIAL_SCHEMA_NAME
        && metadata[0].2 == LEGACY_SCHEMA_CHECKSUM_PROJECT_SOFT_DELETE
    {
        migrate_legacy_project_base_prompt(connection, LEGACY_SCHEMA_CHECKSUM_PROJECT_SOFT_DELETE)?;
    } else if metadata.len() != 1
        || metadata[0].0 != INITIAL_SCHEMA_VERSION
        || metadata[0].1 != INITIAL_SCHEMA_NAME
        || metadata[0].2 != INITIAL_SCHEMA_CHECKSUM
    {
        return Err(
            "任务管理数据库首发 schema 元数据不匹配（错误码：TASK_SCHEMA_METADATA_INVALID）"
                .to_string(),
        );
    }
    let actual_objects = read_schema_objects(connection)?;
    let expected_connection = Connection::open_in_memory().map_err(database_error)?;
    apply_initial_schema(&expected_connection)?;
    let expected_objects = read_schema_objects(&expected_connection)?;
    if actual_objects != expected_objects
        && !schema_objects_are_logically_equivalent(
            connection,
            &expected_connection,
            &actual_objects,
            &expected_objects,
        )?
    {
        return Err(
            "任务管理数据库结构与当前首发 schema 不一致（错误码：TASK_SCHEMA_STRUCTURE_INVALID）"
                .to_string(),
        );
    }
    Ok(())
}

/// 把旧项目记录迁移为支持基础提示词的结构。
/// 流程：在 Immediate 事务中追加 project.base_prompt 默认空字符串，并把指定旧 checksum 更新到当前 checksum。
/// 参数：connection 为已打开业务库，from_checksum 为当前元数据中允许迁移的旧校验值。
/// 返回：迁移成功无返回值。
/// 异常/边界：只允许登记过的旧 schema 迁移；字段已存在、元数据异常或 ALTER 失败均整体回滚。
fn migrate_legacy_project_base_prompt(
    connection: &Connection,
    from_checksum: &str,
) -> Result<(), String> {
    let transaction = connection.unchecked_transaction().map_err(database_error)?;
    transaction
        .execute(
            "ALTER TABLE project ADD COLUMN base_prompt TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "UPDATE schema_metadata SET checksum = ?1 WHERE version = ?2 AND name = ?3 AND checksum = ?4",
            params![
                INITIAL_SCHEMA_CHECKSUM,
                INITIAL_SCHEMA_VERSION,
                INITIAL_SCHEMA_NAME,
                from_checksum
            ],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err(
            "任务管理数据库基础提示词迁移元数据异常（错误码：TASK_SCHEMA_METADATA_INVALID）"
                .to_string(),
        );
    }
    transaction.commit().map_err(database_error)
}

/// 把首发物理删除项目协议迁移为软删除协议。
/// 流程：在 Immediate 事务中追加 project.deleted_at，并更新 schema checksum；参数为已打开业务库连接。
/// 返回：迁移成功无返回值。
/// 异常/边界：只允许从登记过的 001 schema 迁移；字段已存在、元数据异常或 ALTER 失败均整体回滚。
fn migrate_legacy_project_soft_delete(connection: &Connection) -> Result<(), String> {
    let transaction = connection.unchecked_transaction().map_err(database_error)?;
    transaction
        .execute("ALTER TABLE project ADD COLUMN deleted_at TEXT", [])
        .map_err(database_error)?;
    let changed = transaction
        .execute(
            "UPDATE schema_metadata SET checksum = ?1 WHERE version = ?2 AND name = ?3 AND checksum = ?4",
            params![
                LEGACY_SCHEMA_CHECKSUM_PROJECT_SOFT_DELETE,
                INITIAL_SCHEMA_VERSION,
                INITIAL_SCHEMA_NAME,
                LEGACY_SCHEMA_CHECKSUM_SESSION_TASK
            ],
        )
        .map_err(database_error)?;
    if changed != 1 {
        return Err(
            "任务管理数据库首发 schema 迁移元数据不匹配（错误码：TASK_SCHEMA_METADATA_INVALID）"
                .to_string(),
        );
    }
    transaction.commit().map_err(database_error)
}

/// 读取参与首发结构校验的全部用户 schema 对象。
/// 流程：从 sqlite_schema 读取非 sqlite 内部对象的类型、名称和规范 SQL，并按稳定键排序。
/// 参数：connection 为目标数据库连接；返回可直接进行严格相等比较的对象元组列表。
/// 异常/边界：表、索引、触发器和视图都会进入结果，任何额外对象都会导致上层 fail closed；内部自动索引因无用户 SQL 且名称以 sqlite_ 开头而排除。
fn read_schema_objects(connection: &Connection) -> Result<Vec<(String, String, String)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, COALESCE(sql, '') FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .map_err(database_error)?;
    let objects = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(objects)
}

/// 判断两个 schema 快照是否字段和对象层面等价。
/// 流程：先比较全部对象的类型和名称，再对 table 使用 PRAGMA 字段信息比较，对非 table 继续比较 SQL 原文。
/// 参数：actual_connection/expected_connection 为待比对库，actual_objects/expected_objects 为各自 sqlite_schema 快照。
/// 返回：逻辑等价为 true，否则 false。
/// 异常/边界：只放宽 CREATE TABLE 文本差异；额外对象、缺失对象、异常索引、异常字段仍返回不等价。
fn schema_objects_are_logically_equivalent(
    actual_connection: &Connection,
    expected_connection: &Connection,
    actual_objects: &[(String, String, String)],
    expected_objects: &[(String, String, String)],
) -> Result<bool, String> {
    let actual_keys = actual_objects
        .iter()
        .map(|(object_type, name, _)| (object_type.clone(), name.clone()))
        .collect::<Vec<_>>();
    let expected_keys = expected_objects
        .iter()
        .map(|(object_type, name, _)| (object_type.clone(), name.clone()))
        .collect::<Vec<_>>();
    if actual_keys != expected_keys {
        return Ok(false);
    }
    for ((object_type, name, actual_sql), (_, _, expected_sql)) in
        actual_objects.iter().zip(expected_objects.iter())
    {
        if object_type == "table" {
            if !table_columns_are_logically_equivalent(
                &read_table_columns(actual_connection, name)?,
                &read_table_columns(expected_connection, name)?,
            ) {
                return Ok(false);
            }
        } else if actual_sql != expected_sql {
            return Ok(false);
        }
    }
    Ok(true)
}

/// 判断表字段定义是否逻辑一致。
/// 流程：先按字段名排序，再比较字段名、类型、非空、默认值和主键属性；参数为实际库与期望库的 PRAGMA table_info 结果。
/// 返回：字段集合和字段属性完全一致时返回 true。
/// 异常/边界：允许 SQLite `ALTER TABLE ADD COLUMN` 导致的物理列顺序差异；缺字段、多字段、默认值或约束差异仍判定为不等价。
fn table_columns_are_logically_equivalent(
    actual_columns: &[(String, String, i64, Option<String>, i64)],
    expected_columns: &[(String, String, i64, Option<String>, i64)],
) -> bool {
    if actual_columns.len() != expected_columns.len() {
        return false;
    }
    let mut actual_sorted = actual_columns.to_vec();
    let mut expected_sorted = expected_columns.to_vec();
    actual_sorted.sort_by(|left, right| left.0.cmp(&right.0));
    expected_sorted.sort_by(|left, right| left.0.cmp(&right.0));
    actual_sorted == expected_sorted
}

/// 判断两个 schema 快照的原始 SQL 是否完全一致。
/// 流程：先比较对象数量、类型、名称和 SQL 正文；参数为实际库与期望库的 sqlite_schema 快照。
/// 返回：所有对象原始定义一致时返回 true。
/// 异常/边界：仅用于测试证明迁移库与新库存在物理定义差异，生产逻辑应使用字段级等价判断。
#[cfg(test)]
fn schema_objects_are_textually_identical(
    actual_objects: &[(String, String, String)],
    expected_objects: &[(String, String, String)],
) -> bool {
    actual_objects == expected_objects
}

/// 判断两个 schema 快照在对象名称和字段属性层面是否等价。
/// 流程：用于测试直接调用生产等价判断；参数为实际连接、期望连接和已读取的 schema 快照；返回逻辑等价判断。
/// 异常/边界：透传生产校验错误，避免测试使用另一套宽松标准。
#[cfg(test)]
fn schema_objects_are_logically_equivalent_for_test(
    actual_connection: &Connection,
    expected_connection: &Connection,
    actual_objects: &[(String, String, String)],
    expected_objects: &[(String, String, String)],
) -> Result<bool, String> {
    schema_objects_are_logically_equivalent(
        actual_connection,
        expected_connection,
        actual_objects,
        expected_objects,
    )
}

/// 读取表字段结构用于 schema 逻辑等价校验。
/// 流程：通过 PRAGMA table_info 读取字段名、类型、必填、默认值和主键位置；参数为连接与可信 schema 表名。
/// 返回：按 cid 排序的字段结构。
/// 异常/边界：表名来自 sqlite_schema 快照，仍使用双引号转义，避免特殊名称破坏 PRAGMA 语句。
fn read_table_columns(
    connection: &Connection,
    table_name: &str,
) -> Result<Vec<(String, String, i64, Option<String>, i64)>, String> {
    let quoted_table_name = table_name.replace('"', "\"\"");
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info(\"{}\")", quoted_table_name))
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 读取任务工作区的有界聚合数据。
/// 流程：先读取有限项目列表；显式 ID 必须命中，省略 ID 时返回全部未删除项目下的任务与会话；最后校验 7 MiB 内部预算。
/// 参数：connection 为已初始化数据库，project_id 为调用方显式选择或 None；返回首版工作区聚合。
/// 异常/边界：未知显式 ID 返回 TASK_PROJECT_NOT_FOUND，不回落其它项目；理论容量计算失配时拒绝返回超限成功响应。
fn load_workspace_data_with_connection(
    connection: &Connection,
    project_id: Option<&str>,
) -> Result<WorkspaceDataResponse, String> {
    let projects = list_projects(connection)?;
    let selected_project_id = match project_id {
        Some(id) => {
            if !projects.iter().any(|project| project.id == id) {
                return Err(
                    "指定项目不存在或已被删除（错误码：TASK_PROJECT_NOT_FOUND）".to_string()
                );
            }
            Some(id.to_string())
        }
        None => None,
    };
    let tasks = if let Some(id) = selected_project_id.as_deref() {
        list_tasks(connection, id)?
    } else {
        list_all_tasks(connection)?
    };
    let sessions = if let Some(id) = selected_project_id.as_deref() {
        list_sessions(connection, id)?
    } else {
        list_all_sessions(connection)?
    };
    let response = WorkspaceDataResponse {
        projects,
        tasks,
        sessions,
    };
    let serialized_bytes = serde_json::to_vec(&response).map_err(|_| {
        "任务工作区聚合序列化失败（错误码：TASK_WORKSPACE_SERIALIZATION_FAILED）".to_string()
    })?;
    if serialized_bytes.len() > WORKSPACE_RESPONSE_BUDGET_BYTES {
        return Err(format!(
            "任务工作区聚合超过 {} 字节预算（错误码：TASK_WORKSPACE_RESPONSE_TOO_LARGE）",
            WORKSPACE_RESPONSE_BUDGET_BYTES
        ));
    }
    Ok(response)
}

/// 查询未删除项目列表并聚合任务和会话数量。
/// 流程：只读取 deleted_at 为空的项目，关联任务与会话表聚合真实记录数，并按最近更新时间排序；参数为数据库连接；返回项目列表。
/// 异常/边界：空库返回空列表；超过二百个未删除项目时 fail closed，不静默截断；软删除项目不参与当前工作台展示。
fn list_projects(connection: &Connection) -> Result<Vec<ProjectRecord>, String> {
    let project_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM project WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if project_count > WORKSPACE_PROJECT_LIMIT {
        return Err(format!(
            "项目数量超过 {} 个首发上限（错误码：TASK_PROJECT_CAPACITY_INVALID）",
            WORKSPACE_PROJECT_LIMIT
        ));
    }
    let mut statement = connection
        .prepare(
            "
            SELECT p.id,
                   p.name,
                   p.workspace_path,
                   p.base_prompt,
                   COUNT(DISTINCT t.id) AS task_count,
                   COUNT(DISTINCT s.id) AS session_count,
                   p.created_at,
                   p.updated_at
              FROM project p
         LEFT JOIN task t ON t.project_id = p.id
         LEFT JOIN session s ON s.project_id = p.id
             WHERE p.deleted_at IS NULL
          GROUP BY p.id
          ORDER BY p.updated_at DESC, p.created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ProjectRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                workspace_path: row.get(2)?,
                base_prompt: row.get(3)?,
                task_count: row.get(4)?,
                session_count: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 读取单个未删除项目并重新计算任务、会话数量。
/// 流程：按项目主键执行与项目列表一致的聚合；参数为数据库连接和项目 ID；返回可空项目。
/// 异常/边界：只读取一个项目且不受列表上限影响；项目不存在或已软删除返回 None，数据库异常显式返回。
fn load_project_with_connection(
    connection: &Connection,
    project_id: &str,
) -> Result<Option<ProjectRecord>, String> {
    connection
        .query_row(
            "
            SELECT p.id,
                   p.name,
                   p.workspace_path,
                   p.base_prompt,
                   COUNT(DISTINCT t.id),
                   COUNT(DISTINCT s.id),
                   p.created_at,
                   p.updated_at
              FROM project p
         LEFT JOIN task t ON t.project_id = p.id
         LEFT JOIN session s ON s.project_id = p.id
             WHERE p.id = ?1
               AND p.deleted_at IS NULL
          GROUP BY p.id
             LIMIT 1
            ",
            params![project_id],
            |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    workspace_path: row.get(2)?,
                    base_prompt: row.get(3)?,
                    task_count: row.get(4)?,
                    session_count: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

/// 查询项目下任务列表。
/// 流程：按项目 ID 查询全部任务并关联当前会话 threadId，最后按更新时间倒序；参数为连接和项目 ID；返回完整看板任务列表。
/// 异常/边界：项目无任务时返回空列表；任务条数不按旧首发上限截断，最终由聚合响应 JSON 预算兜底；查询失败不跳过损坏行。
fn list_tasks(connection: &Connection, project_id: &str) -> Result<Vec<TaskRecord>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT t.id,
                   t.project_id,
                   t.title,
                   t.prompt,
                   t.status,
                   COALESCE(t.current_session_id, ''),
                   COALESCE(s.external_thread_id, ''),
                   t.last_error,
                   t.result_json,
                   t.created_at,
                   t.updated_at
              FROM task t
         LEFT JOIN session s ON s.id = t.current_session_id
             WHERE t.project_id = ?1
          ORDER BY t.updated_at DESC, t.created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let task_id: String = row.get(0)?;
            Ok(TaskRecord {
                id: task_id.clone(),
                project_id: row.get(1)?,
                title: row.get(2)?,
                prompt: row.get(3)?,
                attachments: latest_task_attachments(connection, &task_id)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?,
                status: row.get(4)?,
                current_session_id: row.get(5)?,
                external_thread_id: row.get(6)?,
                last_error: row.get(7)?,
                result_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 查询全部未删除项目下的任务列表。
/// 流程：统计全部活跃任务量，再关联当前会话 threadId 并按更新时间倒序读取。
/// 参数：connection 为已初始化数据库连接。
/// 返回：全部项目聚合看板任务列表。
/// 异常/边界：只统计未删除项目；软删除项目的历史任务不进入当前“全部”视图；聚合过大时由响应预算返回稳定错误。
fn list_all_tasks(connection: &Connection) -> Result<Vec<TaskRecord>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT t.id,
                   t.project_id,
                   t.title,
                   t.prompt,
                   t.status,
                   COALESCE(t.current_session_id, ''),
                   COALESCE(s.external_thread_id, ''),
                   t.last_error,
                   t.result_json,
                   t.created_at,
                   t.updated_at
              FROM task t
              JOIN project p ON p.id = t.project_id
         LEFT JOIN session s ON s.id = t.current_session_id
             WHERE p.deleted_at IS NULL
          ORDER BY t.updated_at DESC, t.created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            let task_id: String = row.get(0)?;
            Ok(TaskRecord {
                id: task_id.clone(),
                project_id: row.get(1)?,
                title: row.get(2)?,
                prompt: row.get(3)?,
                attachments: latest_task_attachments(connection, &task_id)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?,
                status: row.get(4)?,
                current_session_id: row.get(5)?,
                external_thread_id: row.get(6)?,
                last_error: row.get(7)?,
                result_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 按任务 ID 读取单条看板记录。
/// 流程：复用任务列表的当前会话关联字段，但只命中一个稳定主键；参数为数据库连接和任务 ID；返回可空任务。
/// 异常/边界：数据库异常显式返回；不存在返回 None，由 IPC 层转换稳定 TASK_NOT_FOUND 错误。
fn load_task_with_connection(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<TaskRecord>, String> {
    connection
        .query_row(
            "
            SELECT t.id,
                   t.project_id,
                   t.title,
                   t.prompt,
                   t.status,
                   COALESCE(t.current_session_id, ''),
                   COALESCE(s.external_thread_id, ''),
                   t.last_error,
                   t.result_json,
                   t.created_at,
                   t.updated_at
              FROM task t
         LEFT JOIN session s ON s.id = t.current_session_id
             WHERE t.id = ?1
             LIMIT 1
            ",
            params![task_id],
            |row| {
                let task_id: String = row.get(0)?;
                Ok(TaskRecord {
                    id: task_id.clone(),
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    prompt: row.get(3)?,
                    attachments: latest_task_attachments(connection, &task_id)
                        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?,
                    status: row.get(4)?,
                    current_session_id: row.get(5)?,
                    external_thread_id: row.get(6)?,
                    last_error: row.get(7)?,
                    result_json: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

/// 查询项目下会话列表。
/// 流程：按项目 ID 读取全部真实 Codex 会话并按更新时间倒序；参数为连接和项目 ID；返回完整会话列表。
/// 异常/边界：没有会话时返回空列表；首版没有软删除或隐藏会话协议。
fn list_sessions(connection: &Connection, project_id: &str) -> Result<Vec<SessionRecord>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id,
                   project_id,
                   COALESCE(task_id, ''),
                   provider,
                   workspace_path,
                   title,
                   status,
                   external_thread_id,
                   created_at,
                   updated_at
              FROM session
             WHERE project_id = ?1
          ORDER BY updated_at DESC, created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                task_id: row.get(2)?,
                provider: row.get(3)?,
                workspace_path: row.get(4)?,
                title: row.get(5)?,
                status: row.get(6)?,
                external_thread_id: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 查询全部未删除项目下的会话列表。
/// 流程：关联 project 排除已删除项目，再按更新时间倒序返回真实 Codex 会话。
/// 参数：connection 为已初始化数据库连接。
/// 返回：全部项目聚合会话列表。
/// 异常/边界：首版不限制历史会话条数，最终仍由聚合响应总预算兜底。
fn list_all_sessions(connection: &Connection) -> Result<Vec<SessionRecord>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT s.id,
                   s.project_id,
                   COALESCE(s.task_id, ''),
                   s.provider,
                   s.workspace_path,
                   s.title,
                   s.status,
                   s.external_thread_id,
                   s.created_at,
                   s.updated_at
              FROM session s
              JOIN project p ON p.id = s.project_id
             WHERE p.deleted_at IS NULL
          ORDER BY s.updated_at DESC, s.created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                task_id: row.get(2)?,
                provider: row.get(3)?,
                workspace_path: row.get(4)?,
                title: row.get(5)?,
                status: row.get(6)?,
                external_thread_id: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 按本地会话 ID 读取单条会话记录。
/// 流程：查询任务事件需要的会话展示字段；参数为连接和会话 ID；返回可空会话。
/// 异常/边界：任务刚进入 created/queued 时没有会话并返回 None；数据库异常显式返回，不构造占位会话。
fn load_session_with_connection(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<SessionRecord>, String> {
    connection
        .query_row(
            "
            SELECT id,
                   project_id,
                   COALESCE(task_id, ''),
                   provider,
                   workspace_path,
                   title,
                   status,
                   external_thread_id,
                   created_at,
                   updated_at
              FROM session
             WHERE id = ?1
             LIMIT 1
            ",
            params![session_id],
            |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    task_id: row.get(2)?,
                    provider: row.get(3)?,
                    workspace_path: row.get(4)?,
                    title: row.get(5)?,
                    status: row.get(6)?,
                    external_thread_id: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

/// 读取任务最新附件列表。
/// 流程：优先取最近一次 `created` 或 `task_updated` 事件的 payload_json，解析其中 attachments 字段。
/// 参数：connection 为已初始化连接，task_id 为任务 ID。
/// 返回：任务当前附件；没有附件事件或旧数据无字段时返回空数组。
/// 异常/边界：payload 结构损坏或附件越界会显式失败，避免执行阶段上传错误图片。
fn latest_task_attachments(
    connection: &Connection,
    task_id: &str,
) -> Result<Vec<TaskAttachmentRecord>, String> {
    let payload_json = connection
        .query_row(
            "
            SELECT payload_json
              FROM task_event
             WHERE task_id = ?1
               AND event_type IN ('created', 'task_updated')
          ORDER BY rowid DESC
             LIMIT 1
            ",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(database_error)?;
    let Some(payload_json) = payload_json else {
        return Ok(Vec::new());
    };
    let value: serde_json::Value =
        serde_json::from_str(&payload_json).map_err(|_| "任务附件载荷损坏".to_string())?;
    let attachments = value
        .get("attachments")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let records: Vec<TaskAttachmentRecord> =
        serde_json::from_value(attachments).map_err(|_| "任务附件载荷结构无效".to_string())?;
    validate_task_attachments(records)
}

/// 读取任务执行需要的项目和工作空间上下文。
/// 流程：按任务 ID 关联项目并取得标题、项目基础提示词、prompt 和 canonical 工作目录；参数为连接和任务 ID；返回排队执行快照。
/// 异常/边界：任务或未删除项目不存在时返回数据库错误；本方法只读，不领取任务、不改变状态。
fn read_queued_task_context(
    connection: &Connection,
    task_id: &str,
) -> Result<QueuedTaskRecord, String> {
    connection
        .query_row(
            "
            SELECT t.id, t.project_id, t.title, t.prompt, p.workspace_path, p.base_prompt
             FROM task t
              JOIN project p ON p.id = t.project_id
             WHERE t.id = ?1
               AND p.deleted_at IS NULL
            ",
            params![task_id],
            |row| {
                let task_id: String = row.get(0)?;
                Ok(QueuedTaskRecord {
                    id: task_id.clone(),
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    prompt: build_task_execution_prompt(
                        row.get::<_, String>(5)?.as_str(),
                        row.get::<_, String>(3)?.as_str(),
                    ),
                    attachments: latest_task_attachments(connection, &task_id)
                        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?,
                    workspace_path: row.get(4)?,
                })
            },
        )
        .map_err(database_error)
}

/// 确认项目存在且未删除，避免前端传入过期项目 ID 后创建孤儿任务。
/// 流程：按项目主键和 deleted_at 过滤执行存在性查询；参数为连接和项目 ID；存在时返回无值成功。
/// 异常/边界：不存在或已删除时返回明确业务错误，不允许继续创建新任务。
fn ensure_project_exists(connection: &Connection, project_id: &str) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM project WHERE id = ?1 AND deleted_at IS NULL",
            params![project_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(database_error)?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("项目不存在".to_string())
    }
}

/// 更新任务状态并写入事件表。
/// 流程：按 from_status 执行 CAS，按目标状态补 queued/finished 时间，再追加同事务任务事件；参数为连接、任务 ID、前后状态和错误摘要；返回无。
/// 异常/边界：CAS 未命中或事件写入失败均返回错误，调用方事务负责整体回滚，避免重复或半状态更新。
fn update_task_status(
    connection: &Connection,
    task_id: &str,
    from_status: &str,
    to_status: &str,
    error: &str,
) -> Result<(), String> {
    let finished_at_sql = if matches!(to_status, "completed" | "failed") {
        ", finished_at = CURRENT_TIMESTAMP"
    } else {
        ""
    };
    let queued_at_sql = if to_status == "queued" {
        ", queued_at = CURRENT_TIMESTAMP"
    } else {
        ""
    };
    let sql = format!(
        "UPDATE task SET status = ?1, last_error = ?2, updated_at = CURRENT_TIMESTAMP{}{} WHERE id = ?3 AND status = ?4",
        queued_at_sql, finished_at_sql
    );
    let changed = connection
        .execute(&sql, params![to_status, error, task_id, from_status])
        .map_err(database_error)?;
    if changed != 1 {
        return Err("任务状态 CAS 未命中，已拒绝重复或迟到更新".to_string());
    }
    append_task_event(
        connection,
        task_id,
        "status_changed",
        from_status,
        to_status,
        status_message(to_status),
        "{}",
    )
}

/// 追加任务事件，保留状态流转历史。
fn append_task_event(
    connection: &Connection,
    task_id: &str,
    event_type: &str,
    from_status: &str,
    to_status: &str,
    message: &str,
    payload_json: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO task_event (id, task_id, event_type, from_status, to_status, message, payload_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![next_id("evt"), task_id, event_type, from_status, to_status, message, payload_json],
        )
        .map_err(database_error)?;
    Ok(())
}

/// 收集 rusqlite 查询迭代器并统一转换错误。
fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>, String>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(database_error)?);
    }
    Ok(items)
}

/// 生成本地字符串 ID，兼顾可读前缀和单机唯一性。
fn next_id(prefix: &str) -> String {
    format!("{}_{}", prefix, uuid::Uuid::new_v4())
}

/// 规范化并验证项目工作目录。
fn canonical_workspace_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() {
        return Err("工作空间不能为空（错误码：TASK_PROJECT_WORKSPACE_REQUIRED）".to_string());
    }
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(
            "工作空间必须是绝对路径（错误码：TASK_PROJECT_WORKSPACE_ABSOLUTE_REQUIRED）"
                .to_string(),
        );
    }
    let canonical = fs::canonicalize(path).map_err(|_| {
        "工作空间不存在或不可访问（错误码：TASK_PROJECT_WORKSPACE_UNAVAILABLE）".to_string()
    })?;
    if !canonical.is_dir() {
        return Err(
            "工作空间必须是已存在的目录（错误码：TASK_PROJECT_WORKSPACE_DIR_REQUIRED）".to_string(),
        );
    }
    Ok(canonical)
}

/// 返回状态变化事件的用户可读说明。
fn status_message(status: &str) -> &'static str {
    match status {
        "queued" => "任务已进入排队中",
        "running" => "任务开始执行",
        "waiting_acceptance" => "任务执行完成，等待验收",
        "completed" => "任务已完成",
        "failed" => "任务执行失败",
        _ => "任务状态已更新",
    }
}

/// 截断过长文本，避免错误详情无限写入本地库。
fn limit_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

/// 统一数据库错误文案，避免暴露过长的 SQLite 内部上下文。
fn database_error(error: rusqlite::Error) -> String {
    format!("任务管理数据库操作失败：{}", error)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 聚合预算测试使用的最坏字段样本数；该值不代表项目任务业务上限。
    const WORKSPACE_RESPONSE_BUDGET_SAMPLE_COUNT: i64 = 16;

    /// 首发初始 schema 必须直接创建可靠 turn、结果和事件字段。
    #[test]
    fn initial_schema_contains_reliable_execution_columns() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        let read_columns = |table: &str| {
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({})", table))
                .expect("应准备字段查询");
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .expect("应查询字段")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("应收集字段")
        };
        let session_columns = read_columns("session");
        let task_columns = read_columns("task");
        let project_columns = read_columns("project");
        assert!(session_columns.contains(&"external_turn_id".to_string()));
        assert!(session_columns.contains(&"external_client_message_id".to_string()));
        assert!(session_columns.contains(&"result_json".to_string()));
        assert!(task_columns.contains(&"result_json".to_string()));
        assert!(
            project_columns.contains(&"deleted_at".to_string()),
            "项目必须使用 deleted_at 支持软删除"
        );
        assert!(
            project_columns.contains(&"base_prompt".to_string()),
            "项目必须保存基础提示词"
        );
        for columns in [&project_columns, &task_columns, &session_columns] {
            assert!(
                !columns.contains(&"archived_at".to_string()),
                "项目软删除协议不应混用 archived_at"
            );
        }
        for unused in ["color", "metadata_json", "provider_scope", "sort_order"] {
            assert!(!project_columns.contains(&unused.to_string()));
        }
        for unused in ["description", "metadata_json", "priority", "task_type"] {
            assert!(!task_columns.contains(&unused.to_string()));
        }
        assert!(!session_columns.contains(&"metadata_json".to_string()));
        let session_event_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_event')",
                [],
                |row| row.get(0),
            )
            .expect("应检查未开放会话事件表");
        assert!(!session_event_exists);
    }

    /// 首发 schema 初始化必须只在真正空库执行，并允许精确当前结构重复打开。
    /// 流程：初始化零对象内存库后再次调用门禁，并核对唯一版本、名称和 checksum；返回无。
    /// 异常/边界：重复打开不得新增元数据、改变业务结构或误走历史迁移分支。
    #[test]
    fn initial_schema_accepts_only_empty_or_exact_current_database() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("空库应初始化首发结构");
        initialize_database_schema(&connection).expect("精确当前结构应可重复打开");
        let metadata: (i64, String, String, i64) = connection
            .query_row(
                "SELECT version, name, checksum, (SELECT COUNT(*) FROM schema_metadata) FROM schema_metadata",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("应读取唯一 schema 元数据");
        assert_eq!(metadata.0, INITIAL_SCHEMA_VERSION);
        assert_eq!(metadata.1, INITIAL_SCHEMA_NAME);
        assert_eq!(metadata.2, INITIAL_SCHEMA_CHECKSUM);
        assert_eq!(metadata.3, 1);
    }

    /// 旧版项目物理删除 schema 必须自动补齐 deleted_at 与 base_prompt 并升级 checksum。
    /// 流程：构造登记过的 001 schema，再重新执行初始化门禁；返回无。
    /// 异常/边界：只接受登记过的旧 checksum，迁移后字段层面必须等价于当前 schema。
    #[test]
    fn legacy_project_schema_migrates_to_soft_delete_column() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        apply_initial_schema(&connection).expect("应创建当前结构");
        connection
            .execute("ALTER TABLE project DROP COLUMN deleted_at", [])
            .expect("测试应模拟旧版项目表");
        connection
            .execute("ALTER TABLE project DROP COLUMN base_prompt", [])
            .expect("测试应模拟未支持基础提示词的旧版项目表");
        connection
            .execute(
                "UPDATE schema_metadata SET checksum = ?1",
                params![LEGACY_SCHEMA_CHECKSUM_SESSION_TASK],
            )
            .expect("测试应模拟旧版 checksum");

        initialize_database_schema(&connection).expect("旧版首发 schema 应迁移到当前结构");
        let migrated_checksum: String = connection
            .query_row("SELECT checksum FROM schema_metadata", [], |row| row.get(0))
            .expect("应读取迁移后 checksum");
        assert_eq!(migrated_checksum, INITIAL_SCHEMA_CHECKSUM);
        let has_deleted_at: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('project') WHERE name = 'deleted_at')",
                [],
                |row| row.get(0),
            )
            .expect("应检查 deleted_at 字段");
        assert!(has_deleted_at);
        let has_base_prompt: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('project') WHERE name = 'base_prompt')",
                [],
                |row| row.get(0),
            )
            .expect("应检查 base_prompt 字段");
        assert!(has_base_prompt);
    }

    /// 通过 ALTER TABLE 迁移出的项目表列顺序不同，但字段集合和约束一致时必须被接受。
    /// 流程：从 001 旧库迁移出真实历史列顺序，再与全新库 schema 比较；返回无。
    /// 异常/边界：原始 SQL 差异仅来自字段物理顺序，缺字段、多字段或索引差异仍由生产校验拒绝。
    #[test]
    fn migrated_project_schema_allows_altered_column_order() {
        let migrated_connection = Connection::open_in_memory().expect("应创建迁移库");
        apply_initial_schema(&migrated_connection).expect("应创建当前结构");
        migrated_connection
            .execute("ALTER TABLE project DROP COLUMN deleted_at", [])
            .expect("测试应移除软删除字段");
        migrated_connection
            .execute("ALTER TABLE project DROP COLUMN base_prompt", [])
            .expect("测试应移除基础提示词字段");
        migrated_connection
            .execute(
                "UPDATE schema_metadata SET checksum = ?1",
                params![LEGACY_SCHEMA_CHECKSUM_SESSION_TASK],
            )
            .expect("测试应模拟旧版 checksum");
        initialize_database_schema(&migrated_connection).expect("历史库应迁移成功");

        let expected_connection = Connection::open_in_memory().expect("应创建期望库");
        apply_initial_schema(&expected_connection).expect("应创建全新当前结构");
        let migrated_objects = read_schema_objects(&migrated_connection).expect("应读取迁移结构");
        let expected_objects = read_schema_objects(&expected_connection).expect("应读取当前结构");
        assert!(
            !schema_objects_are_textually_identical(&migrated_objects, &expected_objects),
            "迁移库的 CREATE TABLE 文本应保留 ALTER TABLE 追加顺序差异"
        );
        assert!(
            schema_objects_are_logically_equivalent_for_test(
                &migrated_connection,
                &expected_connection,
                &migrated_objects,
                &expected_objects,
            )
            .expect("逻辑等价检查应成功"),
            "迁移库字段属性应等价于全新当前结构"
        );
    }

    /// 首发 schema 门禁必须拒绝未来版本及任意不匹配的元数据。
    /// 流程：分别篡改版本、名称、checksum 及增加第二条登记后重新校验；返回无。
    /// 异常/边界：高于当前版本返回专用错误码，其余非唯一或不匹配记录返回元数据错误码，均不得自动覆盖修复。
    #[test]
    fn initial_schema_rejects_unsupported_or_inconsistent_metadata() {
        for (mutation, expected_code) in [
            (
                "UPDATE schema_metadata SET version = 2",
                "TASK_SCHEMA_VERSION_UNSUPPORTED",
            ),
            (
                "UPDATE schema_metadata SET name = 'unexpected'",
                "TASK_SCHEMA_METADATA_INVALID",
            ),
            (
                "UPDATE schema_metadata SET checksum = 'unexpected'",
                "TASK_SCHEMA_METADATA_INVALID",
            ),
            (
                "INSERT INTO schema_metadata (version, name, checksum) VALUES (0, 'unexpected', 'unexpected')",
                "TASK_SCHEMA_METADATA_INVALID",
            ),
        ] {
            let connection = Connection::open_in_memory().expect("应创建内存数据库");
            initialize_database_schema(&connection).expect("应先初始化合法首发结构");
            connection
                .execute_batch(mutation)
                .expect("测试应可篡改 schema 元数据");
            let error = initialize_database_schema(&connection)
                .expect_err("不匹配的 schema 元数据必须 fail closed");
            assert!(error.contains(expected_code), "未返回预期错误码：{error}");
        }
    }

    /// 首发 schema 门禁必须拒绝缺表、异常列、额外对象和非空未登记库。
    /// 流程：分别破坏合法结构或创建未登记对象，再执行严格对象快照校验；返回无。
    /// 异常/边界：任何偏离均不得用 CREATE IF NOT EXISTS 补齐或忽略，缺少元数据时返回元数据错误，其余返回结构错误。
    #[test]
    fn initial_schema_rejects_missing_or_anomalous_structure() {
        for mutation in [
            "DROP TABLE task_event",
            "ALTER TABLE task ADD COLUMN unexpected TEXT",
            "CREATE INDEX unexpected_index ON project(name)",
        ] {
            let connection = Connection::open_in_memory().expect("应创建内存数据库");
            initialize_database_schema(&connection).expect("应先初始化合法首发结构");
            connection
                .execute_batch(mutation)
                .expect("测试应可破坏 schema 结构");
            let error = initialize_database_schema(&connection)
                .expect_err("异常 schema 结构必须 fail closed");
            assert!(
                error.contains("TASK_SCHEMA_STRUCTURE_INVALID"),
                "未返回结构错误码：{error}"
            );
        }

        let unregistered = Connection::open_in_memory().expect("应创建内存数据库");
        unregistered
            .execute_batch("CREATE TABLE unexpected (id TEXT PRIMARY KEY)")
            .expect("应创建未登记业务表");
        let error =
            initialize_database_schema(&unregistered).expect_err("非空未登记库必须 fail closed");
        assert!(error.contains("TASK_SCHEMA_METADATA_INVALID"));
    }

    /// 队列读取必须遵循真实 queued 事件提交顺序，不能退化为任务创建顺序或不稳定时间戳排序。
    #[test]
    fn queued_tasks_follow_transactional_enqueue_order() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        for task_id in ["task-a", "task-b", "task-c"] {
            connection
                .execute(
                    "INSERT INTO task (id, project_id, title, prompt, status) VALUES (?1, 'p', ?1, 'P', 'created')",
                    params![task_id],
                )
                .expect("应插入任务");
        }
        for task_id in ["task-b", "task-a", "task-c"] {
            update_task_status(&connection, task_id, "created", "queued", "")
                .expect("任务应按测试顺序入队");
        }
        let queued = list_queued_tasks_with_connection(&connection).expect("应读取排队任务");
        assert_eq!(
            queued.into_iter().map(|task| task.id).collect::<Vec<_>>(),
            vec!["task-b", "task-a", "task-c"]
        );
    }

    /// 队列任务执行上下文必须自动追加项目基础提示词。
    #[test]
    fn queued_task_context_includes_project_base_prompt() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path, base_prompt) VALUES ('p', 'P', '/tmp', '基础规则')",
                [],
            )
            .expect("应插入项目");
        connection
            .execute(
                "INSERT INTO task (id, project_id, title, prompt, status) VALUES ('t', 'p', 'T', '执行任务', 'created')",
                [],
            )
            .expect("应插入任务");
        update_task_status(&connection, "t", "created", "queued", "").expect("任务应进入队列");

        let queued = list_queued_tasks_with_connection(&connection).expect("应读取排队任务");

        assert_eq!(
            queued.first().map(|task| task.prompt.as_str()),
            Some("项目基础提示词：\n基础规则\n\n任务内容：\n执行任务")
        );
    }

    /// 状态更新 CAS 只能命中预期 from 状态，重复更新必须报错且不追加伪事件。
    #[test]
    fn task_status_compare_and_swap_rejects_duplicate_transition() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        connection
            .execute(
                "INSERT INTO task (id, project_id, title, prompt, status) VALUES ('t', 'p', 'T', 'P', 'created')",
                [],
            )
            .expect("应插入任务");
        update_task_status(&connection, "t", "created", "queued", "").expect("首次 CAS 应成功");
        assert!(update_task_status(&connection, "t", "created", "queued", "").is_err());
        let event_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_event WHERE task_id = 't'",
                [],
                |row| row.get(0),
            )
            .expect("应读取事件数");
        assert_eq!(event_count, 1);
    }

    /// 任务编辑只能覆盖尚未执行的 created 或 queued 内容，并在状态不允许时保持原内容。
    #[test]
    fn task_update_only_allows_created_and_queued_statuses() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        let created_id = create_task_record(&mut connection, "p", "已创建", "原描述", &[])
            .expect("已创建任务应创建成功");
        let queued_id = create_task_record(&mut connection, "p", "等待中", "原描述", &[])
            .expect("等待中任务应创建成功");
        connection
            .execute(
                "UPDATE task SET status = 'queued' WHERE id = ?1",
                params![queued_id],
            )
            .expect("应置为等待中");
        let completed_id = create_task_record(&mut connection, "p", "已完成", "原描述", &[])
            .expect("已完成任务应创建成功");
        connection
            .execute(
                "UPDATE task SET status = 'completed' WHERE id = ?1",
                params![completed_id],
            )
            .expect("应置为已完成");

        update_task_record(&mut connection, &created_id, "新已创建", "新描述", &[])
            .expect("已创建任务应允许修改");
        update_task_record(&mut connection, &queued_id, "新等待中", "新描述", &[])
            .expect("等待中任务应允许修改");
        let error = update_task_record(&mut connection, &completed_id, "覆盖", "覆盖描述", &[])
            .expect_err("已执行过任务必须拒绝修改");
        assert!(error.contains("TASK_UPDATE_STATUS_FORBIDDEN"));
        let completed_title: String = connection
            .query_row(
                "SELECT title FROM task WHERE id = ?1",
                params![completed_id],
                |row| row.get(0),
            )
            .expect("应读取已完成任务标题");
        assert_eq!(completed_title, "已完成");
    }

    /// 任务删除必须拒绝 running，允许其它状态物理移除并同步清理本地关联记录。
    #[test]
    fn task_delete_rejects_running_and_removes_other_statuses() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        let created_id = create_task_record(&mut connection, "p", "已创建", "描述", &[])
            .expect("已创建任务应创建成功");
        let running_id = create_task_record(&mut connection, "p", "进行中", "描述", &[])
            .expect("进行中任务应创建成功");
        connection
            .execute(
                "UPDATE task SET status = 'running' WHERE id = ?1",
                params![running_id],
            )
            .expect("应置为进行中");

        delete_task_record(&mut connection, &created_id).expect("已创建任务应允许删除");
        let error =
            delete_task_record(&mut connection, &running_id).expect_err("进行中任务必须拒绝删除");
        assert!(error.contains("TASK_DELETE_STATUS_FORBIDDEN"));
        let remaining_task_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM task", [], |row| row.get(0))
            .expect("应读取任务数");
        assert_eq!(remaining_task_count, 1);
    }

    /// 崩溃恢复出的唯一 turn 必须复用正常绑定 CAS，只能成功一次且保留原 thread 归属。
    #[test]
    fn recovered_turn_binding_is_atomic_and_single_use() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp');
                INSERT INTO task (id, project_id, title, prompt, status, current_session_id)
                VALUES ('t', 'p', 'T', 'P', 'running', 's');
                INSERT INTO session (
                    id, project_id, task_id, provider, workspace_path, title, status,
                    external_thread_id, external_client_message_id, external_status
                ) VALUES ('s', 'p', 't', 'codex', '/tmp', 'T', 'running', 'thread-a', 'request-a', 'threadCreated');
                ",
            )
            .expect("应准备缺少 turnId 的运行任务");

        bind_task_execution_with_connection(&mut connection, "t", "s", "thread-a", "turn-a")
            .expect("专用 thread 恢复出的唯一 turn 应绑定成功");
        assert!(bind_task_execution_with_connection(
            &mut connection,
            "t",
            "s",
            "thread-a",
            "turn-b"
        )
        .is_err());
        assert!(bind_task_execution_with_connection(
            &mut connection,
            "t",
            "s",
            "thread-other",
            "turn-c"
        )
        .is_err());

        let bound: (String, String) = connection
            .query_row(
                "SELECT external_turn_id, external_status FROM session WHERE id = 's'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("应读取恢复绑定结果");
        assert_eq!(bound, ("turn-a".to_string(), "inProgress".to_string()));
        let event_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_event WHERE task_id = 't' AND event_type = 'codex_execution_bound'",
                [],
                |row| row.get(0),
            )
            .expect("应读取 turn 绑定事件数");
        assert_eq!(event_count, 1);
    }

    /// 任务标题、prompt 与结果 JSON 必须在边界内接受，超过一个字符或字节即返回稳定错误码。
    #[test]
    fn task_text_and_result_boundaries_are_enforced() {
        assert!(validate_task_content(
            &"标".repeat(TASK_TITLE_MAX_CHARS),
            &"内".repeat(TASK_PROMPT_MAX_CHARS)
        )
        .is_ok());
        assert!(
            validate_task_content(&"标".repeat(TASK_TITLE_MAX_CHARS + 1), "内容")
                .expect_err("标题超限应拒绝")
                .contains("TASK_TITLE_TOO_LONG")
        );
        assert!(
            validate_task_content("标题", &"内".repeat(TASK_PROMPT_MAX_CHARS + 1))
                .expect_err("prompt 超限应拒绝")
                .contains("TASK_PROMPT_TOO_LONG")
        );

        let exact_result = serde_json::json!({
            "value": "x".repeat(TASK_RESULT_JSON_MAX_BYTES - 12)
        })
        .to_string();
        assert_eq!(exact_result.len(), TASK_RESULT_JSON_MAX_BYTES);
        assert!(validate_task_result_json(&exact_result).is_ok());
        let oversized_result = format!("{} ", exact_result);
        assert!(validate_task_result_json(&oversized_result)
            .expect_err("结果 JSON 超限应拒绝")
            .contains("TASK_RESULT_TOO_LARGE"));
        assert!(validate_task_result_json("not-json")
            .expect_err("畸形 JSON 应拒绝")
            .contains("TASK_RESULT_INVALID"));
    }

    /// 历史任务超过旧首发十六条后仍必须完整返回，避免本地真实数据被读取层误判异常。
    #[test]
    fn workspace_tasks_are_not_limited_to_initial_cap() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        for index in 0..17 {
            connection
                .execute(
                    "INSERT INTO task (id, project_id, title, prompt, status, updated_at) VALUES (?1, 'p', ?1, 'P', 'created', ?2)",
                    params![format!("task-{index:04}"), format!("2026-01-01 00:{:02}:00", index % 60)],
                )
                .expect("应插入边界任务");
        }
        let tasks = list_tasks(&connection, "p").expect("超过十六条历史任务也应完整返回");
        assert_eq!(tasks.len(), 17);
    }

    /// 项目会话不再限制为十六条，查询应完整返回历史会话。
    #[test]
    fn workspace_sessions_are_not_limited_to_initial_cap() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        for index in 0..17 {
            connection
                .execute(
                    "INSERT INTO session (id, project_id, provider, workspace_path, title, status, updated_at) VALUES (?1, 'p', 'codex', '/tmp', ?1, 'running', ?2)",
                    params![format!("session-{index:04}"), format!("2026-01-01 00:{:02}:00", index % 60)],
                )
                .expect("应插入边界会话");
        }
        let sessions = list_sessions(&connection, "p").expect("超过十六条会话也应完整返回");
        assert_eq!(sessions.len(), 17);
    }

    /// 创建任务不再受旧十六条上限影响，真实容量由字段校验和聚合响应预算兜底。
    #[test]
    fn project_task_creation_is_not_limited_to_initial_cap() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        for index in 0..17 {
            create_task_record(
                &mut connection,
                "p",
                &format!("任务-{index}"),
                "提示词",
                &[],
            )
            .expect("边界内任务应创建");
        }
        let extra_id = create_task_record(&mut connection, "p", "额外任务", "提示词", &[])
            .expect("第十八条任务不应被旧上限拒绝");
        let task_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM task", [], |row| row.get(0))
            .expect("应读取任务数");
        let event_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM task_event", [], |row| row.get(0))
            .expect("应读取事件数");
        assert_eq!(task_count, 18);
        assert_eq!(event_count, 18);
        assert!(extra_id.starts_with("task_"));
    }

    /// 创建任务专用响应必须用事务 ID 区分同项目同名任务，并保持聚合字段扁平且不污染普通查询响应。
    #[test]
    fn create_task_response_identifies_same_name_records_without_polluting_workspace() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");

        let first_id = create_task_record(&mut connection, "p", "相同标题", "相同提示词", &[])
            .expect("首个同名任务应创建");
        let first_workspace = load_workspace_data_with_connection(&connection, Some("p"))
            .expect("应读取首次创建聚合");
        let first_response = CreateTaskResponse {
            created_task_id: first_id.clone(),
            projects: first_workspace.projects,
            tasks: first_workspace.tasks,
            sessions: first_workspace.sessions,
        };
        let first_json = serde_json::to_value(&first_response).expect("首次响应应可序列化");

        let second_id = create_task_record(&mut connection, "p", "相同标题", "相同提示词", &[])
            .expect("第二个同名任务应创建");
        let second_workspace = load_workspace_data_with_connection(&connection, Some("p"))
            .expect("应读取第二次创建聚合");
        let ordinary_workspace_json =
            serde_json::to_value(&second_workspace).expect("普通聚合应可序列化");
        let second_response = CreateTaskResponse {
            created_task_id: second_id.clone(),
            projects: second_workspace.projects,
            tasks: second_workspace.tasks,
            sessions: second_workspace.sessions,
        };
        let second_json = serde_json::to_value(&second_response).expect("第二次响应应可序列化");

        assert_ne!(first_id, second_id);
        assert_eq!(first_json["createdTaskId"], first_id);
        assert_eq!(second_json["createdTaskId"], second_id);
        assert!(second_json["tasks"]
            .as_array()
            .is_some_and(|tasks| tasks.iter().any(|task| task["id"] == second_id)));
        assert!(second_json.get("projects").is_some());
        assert!(second_json.get("tasks").is_some());
        assert!(second_json.get("sessions").is_some());
        assert!(second_json.get("workspace").is_none());
        assert!(ordinary_workspace_json.get("createdTaskId").is_none());
    }

    /// 每项目超过十六条历史会话后仍允许继续创建新执行会话。
    #[test]
    fn project_session_history_does_not_block_running_transition() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        for index in 0..17 {
            connection
                .execute(
                    "INSERT INTO session (id, project_id, provider, workspace_path, title, status) VALUES (?1, 'p', 'codex', '/tmp', ?1, 'failed')",
                    params![format!("session-{index}")],
                )
                .expect("边界内会话应插入");
        }
        let sessions = list_sessions(&connection, "p").expect("历史会话不应阻塞查询");
        let session_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM session", [], |row| row.get(0))
            .expect("应读取会话数");
        assert_eq!(session_count, 17);
        assert_eq!(sessions.len(), 17);
    }

    /// 显式未知项目必须稳定报错，省略项目 ID 时返回全部活跃项目任务。
    #[test]
    fn workspace_selection_distinguishes_missing_unknown_and_all_project_id() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('known-a', '已知项目 A', '/tmp/a');
                INSERT INTO project (id, name, workspace_path) VALUES ('known-b', '已知项目 B', '/tmp/b');
                INSERT INTO task (id, project_id, title, prompt, status) VALUES ('task-a', 'known-a', '任务 A', '提示 A', 'created');
                INSERT INTO task (id, project_id, title, prompt, status) VALUES ('task-b', 'known-b', '任务 B', '提示 B', 'created');
                ",
            )
            .expect("应插入项目和任务");

        let default_data = load_workspace_data_with_connection(&connection, None)
            .expect("省略 ID 应返回全部项目任务");
        assert_eq!(default_data.projects.len(), 2);
        assert_eq!(default_data.tasks.len(), 2);
        assert!(default_data
            .tasks
            .iter()
            .any(|task| task.project_id == "known-a"));
        assert!(default_data
            .tasks
            .iter()
            .any(|task| task.project_id == "known-b"));
        let error = load_workspace_data_with_connection(&connection, Some("missing"))
            .expect_err("显式未知 ID 不得回落首个项目");
        assert!(error.contains("TASK_PROJECT_NOT_FOUND"));
    }

    /// 首发聚合容量的最坏合法字段组合必须低于 7 MiB 业务预算，并为 8 MiB RPC envelope 留出余量。
    #[test]
    fn worst_case_legal_workspace_response_fits_internal_budget() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        for index in 0..WORKSPACE_PROJECT_LIMIT {
            connection
                .execute(
                    "INSERT INTO project (id, name, workspace_path) VALUES (?1, ?2, ?3)",
                    params![
                        format!("project-{index:03}"),
                        format!(
                            "{}{:03}",
                            "\u{0001}".repeat(PROJECT_NAME_MAX_CHARS - 3),
                            index
                        ),
                        format!("{}{:03}", "\u{0001}".repeat(997), index)
                    ],
                )
                .expect("应插入容量边界项目");
        }
        let result_json = serde_json::json!({
            "value": "x".repeat(TASK_RESULT_JSON_MAX_BYTES - 12)
        })
        .to_string();
        for index in 0..WORKSPACE_RESPONSE_BUDGET_SAMPLE_COUNT {
            connection
                .execute(
                    "INSERT INTO task (id, project_id, title, prompt, status, last_error, result_json) VALUES (?1, 'project-000', ?2, ?3, 'failed', ?4, ?5)",
                    params![
                        format!("task-{index:03}"),
                        "\u{0001}".repeat(TASK_TITLE_MAX_CHARS),
                        "\u{0001}".repeat(TASK_PROMPT_MAX_CHARS),
                        "\u{0001}".repeat(1_000),
                        result_json
                    ],
                )
                .expect("应插入最坏合法任务");
            connection
                .execute(
                    "INSERT INTO session (id, project_id, provider, workspace_path, title, status, external_thread_id) VALUES (?1, 'project-000', 'codex', ?2, ?3, 'failed', ?4)",
                    params![
                        format!("session-{index:03}"),
                        "\u{0001}".repeat(1_000),
                        "\u{0001}".repeat(TASK_TITLE_MAX_CHARS),
                        format!("thread-{index:03}")
                    ],
                )
                .expect("应插入最坏合法会话");
        }

        let response = load_workspace_data_with_connection(&connection, Some("project-000"))
            .expect("最坏合法聚合仍应成功");
        let bytes = serde_json::to_vec(&response).expect("聚合应可序列化");
        assert_eq!(response.projects.len(), WORKSPACE_PROJECT_LIMIT as usize);
        assert_eq!(
            response.tasks.len(),
            WORKSPACE_RESPONSE_BUDGET_SAMPLE_COUNT as usize
        );
        assert_eq!(
            response.sessions.len(),
            WORKSPACE_RESPONSE_BUDGET_SAMPLE_COUNT as usize
        );
        assert!(bytes.len() <= WORKSPACE_RESPONSE_BUDGET_BYTES);
    }

    /// 项目名称和项目总数必须在写入前受限，防止无分页项目列表突破聚合容量。
    #[test]
    fn project_capacity_is_rejected_before_insert() {
        assert!(validate_project_name(&"项".repeat(PROJECT_NAME_MAX_CHARS)).is_ok());
        assert!(
            validate_project_name(&"项".repeat(PROJECT_NAME_MAX_CHARS + 1))
                .expect_err("超长名称应拒绝")
                .contains("TASK_PROJECT_NAME_TOO_LONG")
        );

        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        for index in 0..WORKSPACE_PROJECT_LIMIT {
            connection
                .execute(
                    "INSERT INTO project (id, name, workspace_path) VALUES (?1, ?2, ?3)",
                    params![
                        format!("project-{index}"),
                        format!("项目-{index}"),
                        format!("/tmp/project-{index}")
                    ],
                )
                .expect("应插入边界项目");
        }
        let error = create_project_record(&mut connection, "额外项目", Path::new("/"), "")
            .expect_err("达到项目上限后必须在插入前拒绝");
        assert!(error.contains("TASK_PROJECT_LIMIT_REACHED"));
        let project_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM project", [], |row| row.get(0))
            .expect("应读取项目总数");
        assert_eq!(project_count, WORKSPACE_PROJECT_LIMIT);
    }

    /// 增量事件依赖的单条查询必须返回任务、最新项目计数与当前会话，确保前端无需全量重载。
    #[test]
    fn incremental_task_queries_preserve_workspace_state() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp');
                INSERT INTO task (id, project_id, title, prompt, status, current_session_id)
                VALUES ('t', 'p', 'T', 'P', 'running', 's');
                INSERT INTO session (
                    id, project_id, task_id, provider, workspace_path, title, status,
                    external_thread_id
                ) VALUES ('s', 'p', 't', 'codex', '/tmp', 'T', 'running', 'thread-a');
                ",
            )
            .expect("应准备增量快照数据");
        let task = load_task_with_connection(&connection, "t")
            .expect("应读取任务")
            .expect("任务应存在");
        let project = load_project_with_connection(&connection, "p")
            .expect("应读取项目")
            .expect("项目应存在");
        let session = load_session_with_connection(&connection, &task.current_session_id)
            .expect("应读取会话")
            .expect("会话应存在");
        assert_eq!(task.external_thread_id, "thread-a");
        assert_eq!(project.task_count, 1);
        assert_eq!(project.session_count, 1);
        assert_eq!(session.task_id, "t");
    }

    /// 工作目录必须存在且 canonicalize，符号路径或相对路径不能原样进入任务执行。
    #[test]
    fn workspace_path_requires_existing_absolute_directory() {
        assert!(canonical_workspace_path("relative/path").is_err());
        let canonical = canonical_workspace_path("/tmp").expect("系统临时目录应存在");
        assert!(canonical.is_absolute());
        assert!(canonical.is_dir());
    }

    /// 删除项目只软删除项目行，不能因为关联任务或会话而拒绝，也不能级联删除历史。
    #[test]
    fn project_delete_soft_deletes_project_without_touching_history() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('empty', '空项目', '/tmp'), ('used', '有任务项目', '/tmp')",
                [],
            )
            .expect("应插入项目");
        connection
            .execute(
                "INSERT INTO task (id, project_id, title, prompt, status) VALUES ('task', 'used', '任务', '提示', 'created')",
                [],
            )
            .expect("应插入任务");
        connection
            .execute(
                "INSERT INTO session (id, project_id, task_id, provider, workspace_path, title, status) VALUES ('session', 'used', 'task', 'codex', '/tmp', '会话', 'created')",
                [],
            )
            .expect("应插入会话");

        delete_project_record(&mut connection, "empty").expect("空项目应可删除");
        delete_project_record(&mut connection, "used").expect("有任务和会话历史的项目也应可软删除");
        let visible_projects = list_projects(&connection).expect("应读取未删除项目");
        assert!(visible_projects.is_empty());
        let history_counts: (i64, i64, i64) = connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM project WHERE deleted_at IS NOT NULL), (SELECT COUNT(*) FROM task WHERE project_id = 'used'), (SELECT COUNT(*) FROM session WHERE project_id = 'used')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("应读取软删除和历史数量");
        assert_eq!(history_counts, (2, 1, 1));
        assert!(ensure_project_exists(&connection, "used").is_err());
        assert!(delete_project_record(&mut connection, "used").is_err());
    }

    /// 项目创建和编辑必须拒绝重复名称，并允许多个项目绑定同一个 canonical 工作目录。
    #[test]
    fn project_identity_conflicts_leave_database_unchanged() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        let workspace_a = canonical_workspace_path("/tmp").expect("临时目录应存在");
        let workspace_b = canonical_workspace_path("/").expect("根目录应存在");
        let project_a = create_project_record(&mut connection, "项目 A", &workspace_a, "")
            .expect("首个项目应创建成功");

        assert!(create_project_record(&mut connection, "项目 A", &workspace_b, "").is_err());
        let project_b = create_project_record(&mut connection, "项目 B", &workspace_a, "")
            .expect("不同项目应允许绑定同一工作目录");
        let project_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM project", [], |row| row.get(0))
            .expect("应读取项目数");
        assert_eq!(project_count, 2);

        ensure_project_name_unique(&connection, "项目 A", &project_a)
            .expect("编辑项目应允许保留自身名称");
        assert!(ensure_project_name_unique(&connection, "项目 A", &project_b).is_err());
    }

    /// 创建事件写入失败时任务行必须随 Immediate 事务回滚，恢复后正常创建只产生一条事件。
    #[test]
    fn task_creation_rolls_back_when_event_insert_fails() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('project', '项目', '/tmp')",
                [],
            )
            .expect("应插入项目");
        connection
            .execute_batch(
                "CREATE TRIGGER reject_task_event BEFORE INSERT ON task_event BEGIN SELECT RAISE(ABORT, 'event rejected'); END;",
            )
            .expect("应创建失败触发器");

        assert!(create_task_record(&mut connection, "project", "任务", "提示词", &[]).is_err());
        let task_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM task", [], |row| row.get(0))
            .expect("应读取任务数");
        assert_eq!(task_count, 0);

        connection
            .execute_batch("DROP TRIGGER reject_task_event;")
            .expect("应移除失败触发器");
        let task_id = create_task_record(&mut connection, "project", "任务", "提示词", &[])
            .expect("事件恢复后应创建成功");
        let event_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_event WHERE task_id = ?1 AND event_type = 'created'",
                params![task_id],
                |row| row.get(0),
            )
            .expect("应读取创建事件数");
        assert_eq!(event_count, 1);
    }

    /// Codex 连接门禁和发送不确定状态必须作为稳定公开业务码保留，不能被统一日志入口吞成 TASK_QUEUE_FAILED。
    #[test]
    fn codex_queue_contract_errors_are_public() {
        assert!(is_public_task_contract_error(
            "未连接（错误码：CODEX_DESKTOP_NOT_CONNECTED）"
        ));
        assert!(is_public_task_contract_error(
            "禁止重排（错误码：CODEX_SEND_UNCERTAIN）"
        ));
    }

    /// 项目创建表单校验错误必须作为公开业务码返回，避免用户只看到脱敏后的泛化创建失败。
    #[test]
    fn project_form_contract_errors_are_public() {
        assert!(is_public_task_contract_error(
            "工作空间不存在或不可访问（错误码：TASK_PROJECT_WORKSPACE_UNAVAILABLE）"
        ));
        assert!(is_public_task_contract_error(
            "项目名称已存在（错误码：TASK_PROJECT_NAME_EXISTS）"
        ));
    }

    /// task/session 任一侧残留 running 都必须阻止重启，不能因两表不一致误判空闲。
    #[test]
    fn running_gate_fails_closed_for_partial_state() {
        let connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute(
                "INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp')",
                [],
            )
            .expect("应插入项目");
        connection
            .execute(
                "INSERT INTO task (id, project_id, title, prompt, status) VALUES ('t', 'p', 'T', 'P', 'running')",
                [],
            )
            .expect("应插入单侧 running task");
        assert!(has_running_task_with_connection(&connection).expect("应识别 task running"));
        connection
            .execute("UPDATE task SET status = 'failed' WHERE id = 't'", [])
            .expect("应清理 task running");
        connection
            .execute(
                "INSERT INTO session (id, project_id, provider, workspace_path, title, status) VALUES ('s', 'p', 'codex', '/tmp', 'S', 'running')",
                [],
            )
            .expect("应插入单侧 running session");
        assert!(has_running_task_with_connection(&connection).expect("应识别 session running"));
    }

    /// CDP 提交水位、发送不确定终态和禁止重排必须构成一个不可绕过的事务闭环。
    #[test]
    fn uncertain_submission_cannot_be_requeued_or_mutate_events() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp');
                INSERT INTO task (id, project_id, title, prompt, status, current_session_id)
                VALUES ('t', 'p', 'T', 'P', 'running', 's');
                INSERT INTO session (id, project_id, task_id, provider, workspace_path, title, status)
                VALUES ('s', 'p', 't', 'codex', '/tmp', 'T', 'running');
                ",
            )
            .expect("应准备 running 任务");
        mark_task_submission_started_with_connection(
            &mut connection,
            "t",
            "s",
            "client-message",
            1_765_000_000_000,
            &["thread-old".to_string()],
        )
        .expect("Enter 前水位应事务提交");
        let submission: (String, String, String) = connection
            .query_row(
                "SELECT external_status, external_client_message_id, external_url FROM session WHERE id = 's'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("应读取提交阶段");
        assert_eq!(submission.0, "cdpSubmitStarted");
        assert_eq!(submission.1, "client-message");
        let persisted_state = serde_json::from_str::<PendingSubmissionState>(&submission.2)
            .expect("提交恢复状态应为严格 JSON");
        assert_eq!(persisted_state.version, 1);
        assert_eq!(persisted_state.submitted_at_ms, 1_765_000_000_000);
        assert_eq!(persisted_state.known_thread_ids, ["thread-old"]);
        let pending = load_pending_submission_with_connection(&connection, "t", "s")
            .expect("应读取持久化恢复上下文")
            .expect("应存在待恢复提交");
        assert_eq!(pending.submitted_at_ms, 1_765_000_000_000);
        assert_eq!(pending.client_user_message_id, "client-message");
        assert_eq!(pending.known_thread_ids, ["thread-old"]);

        mark_task_send_uncertain_with_connection(&mut connection, "t", "s", "无法确认")
            .expect("发送不确定必须原子落库");
        let states: (String, String, String) = connection
            .query_row(
                "SELECT t.status, s.status, s.external_status FROM task t JOIN session s ON s.id = t.current_session_id WHERE t.id = 't'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("应读取不确定状态");
        assert_eq!(
            states,
            (
                "failed".to_string(),
                "failed".to_string(),
                "sendUncertain".to_string()
            )
        );
        let events_before: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_event WHERE task_id = 't'",
                [],
                |row| row.get(0),
            )
            .expect("应读取事件数");
        let error = queue_task_with_connection(&mut connection, "t")
            .expect_err("sendUncertain 必须禁止重排");
        assert!(error.contains("CODEX_SEND_UNCERTAIN"));
        let events_after: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_event WHERE task_id = 't'",
                [],
                |row| row.get(0),
            )
            .expect("应读取拒绝后事件数");
        assert_eq!(events_after, events_before);
        let status_after: String = connection
            .query_row("SELECT status FROM task WHERE id = 't'", [], |row| {
                row.get(0)
            })
            .expect("应读取拒绝后状态");
        assert_eq!(status_after, "failed");

        connection
            .execute("UPDATE task SET status = 'created' WHERE id = 't'", [])
            .expect("应模拟异常状态组合");
        let error = queue_task_with_connection(&mut connection, "t")
            .expect_err("任意任务状态只要关联 sendUncertain 都必须禁止入队");
        assert!(error.contains("CODEX_SEND_UNCERTAIN"));
        let events_after_corrupt_state: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_event WHERE task_id = 't'",
                [],
                |row| row.get(0),
            )
            .expect("应读取异常状态拒绝后的事件数");
        assert_eq!(events_after_corrupt_state, events_after);
    }

    /// 非正提交水位必须在事务写入前拒绝，session 和事件表都不得产生半状态。
    #[test]
    fn invalid_submission_watermark_has_zero_database_side_effects() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp');
                INSERT INTO task (id, project_id, title, prompt, status, current_session_id)
                VALUES ('t', 'p', 'T', 'P', 'running', 's');
                INSERT INTO session (id, project_id, task_id, provider, workspace_path, title, status)
                VALUES ('s', 'p', 't', 'codex', '/tmp', 'T', 'running');
                ",
            )
            .expect("应准备 running 任务");
        assert!(mark_task_submission_started_with_connection(
            &mut connection,
            "t",
            "s",
            "client-message",
            0,
            &[],
        )
        .is_err());
        let session: (String, String, String) = connection
            .query_row(
                "SELECT external_status, external_client_message_id, external_url FROM session WHERE id = 's'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("应读取未修改 session");
        assert_eq!(session, (String::new(), String::new(), String::new()));
        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM task_event", [], |row| row.get(0))
            .expect("应读取事件数");
        assert_eq!(events, 0);
    }

    /// interrupted 但已有助手最终回复时，应视为可人工验收并保留结果，避免把已产出内容的任务误标失败。
    #[test]
    fn interrupted_turn_with_final_text_enters_waiting_acceptance() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp');
                INSERT INTO task (id, project_id, title, prompt, status, current_session_id)
                VALUES ('t', 'p', 'T', 'P', 'running', 's');
                INSERT INTO session (
                    id, project_id, task_id, provider, workspace_path, title, status,
                    external_thread_id, external_turn_id
                )
                VALUES ('s', 'p', 't', 'codex', '/tmp', 'T', 'running', 'thread-1', 'turn-1');
                ",
            )
            .expect("应准备已绑定的 running 任务");
        let running = RunningTaskRecord {
            project_id: "p".to_string(),
            task_id: "t".to_string(),
            session_id: "s".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        };
        let result_json = serde_json::json!({
            "turnId": "turn-1",
            "status": "interrupted",
            "completedAt": null,
            "finalText": "已经生成的回复"
        })
        .to_string();

        finish_task_execution_with_connection(
            &mut connection,
            &running,
            "interrupted",
            &result_json,
            "",
        )
        .expect("有最终回复的 interrupted 应进入待验收");

        let persisted: (String, String, String, String) = connection
            .query_row(
                "SELECT t.status, s.status, s.external_status, t.result_json FROM task t JOIN session s ON s.id = t.current_session_id WHERE t.id = 't'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("应读取终态");
        assert_eq!(
            persisted,
            (
                "waiting_acceptance".to_string(),
                "waiting_acceptance".to_string(),
                "interrupted".to_string(),
                result_json
            )
        );
    }

    /// interrupted 没有最终回复时仍必须失败并清空结果，避免空中断被伪装成可验收内容。
    #[test]
    fn interrupted_turn_without_final_text_stays_failed() {
        let mut connection = Connection::open_in_memory().expect("应创建内存数据库");
        initialize_database_schema(&connection).expect("首版结构初始化应成功");
        connection
            .execute_batch(
                "
                INSERT INTO project (id, name, workspace_path) VALUES ('p', 'P', '/tmp');
                INSERT INTO task (id, project_id, title, prompt, status, current_session_id)
                VALUES ('t', 'p', 'T', 'P', 'running', 's');
                INSERT INTO session (
                    id, project_id, task_id, provider, workspace_path, title, status,
                    external_thread_id, external_turn_id
                )
                VALUES ('s', 'p', 't', 'codex', '/tmp', 'T', 'running', 'thread-1', 'turn-1');
                ",
            )
            .expect("应准备已绑定的 running 任务");
        let running = RunningTaskRecord {
            project_id: "p".to_string(),
            task_id: "t".to_string(),
            session_id: "s".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        };
        let result_json = serde_json::json!({
            "turnId": "turn-1",
            "status": "interrupted",
            "completedAt": null,
            "finalText": "   "
        })
        .to_string();

        finish_task_execution_with_connection(
            &mut connection,
            &running,
            "interrupted",
            &result_json,
            "用户中断",
        )
        .expect("没有最终回复的 interrupted 应保持失败");

        let persisted: (String, String, String, String, String) = connection
            .query_row(
                "SELECT t.status, s.status, s.external_status, t.result_json, t.last_error FROM task t JOIN session s ON s.id = t.current_session_id WHERE t.id = 't'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("应读取终态");
        assert_eq!(
            persisted,
            (
                "failed".to_string(),
                "failed".to_string(),
                "interrupted".to_string(),
                "{}".to_string(),
                "用户中断".to_string()
            )
        );
    }
}
