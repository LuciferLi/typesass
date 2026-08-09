use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 当前任务管理业务库结构版本；每次新增迁移时必须递增。
const CURRENT_SCHEMA_VERSION: i64 = 1;

/// 数据库迁移记录名称，用于排查本地用户库升级状态。
const INITIAL_SCHEMA_NAME: &str = "初始化会话与任务管理表";

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
    /// 用户取消任务，调度器不再处理；当前 UI 暂未开放取消入口，但数据库状态协议先保留。
    #[allow(dead_code)]
    Cancelled,
}

impl TaskStatus {
    /// 返回写入 SQLite 的稳定协议值。
    fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingAcceptance => "waiting_acceptance",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// 会话状态枚举，用于记录本地调度器与外部执行器的生命周期。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionStatus {
    /// 会话已创建但外部执行尚未启动；后续拆分会话预创建流程时使用。
    #[allow(dead_code)]
    Created,
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
    /// 返回写入 SQLite 的稳定协议值。
    fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
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
    /// 当前任务状态协议值。
    pub status: String,
    /// 当前绑定的会话 ID；未执行前为空。
    pub current_session_id: String,
    /// 当前绑定的 CodeX thread ID；未创建会话前为空。
    pub external_thread_id: String,
    /// 最近失败原因；正常状态为空。
    pub last_error: String,
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

/// 队列调度候选任务，供 Rust 后台线程创建 CodeX 会话。
#[derive(Debug, Clone)]
pub struct QueuedTaskRecord {
    /// 任务 ID。
    pub id: String,
    /// 所属项目 ID。
    pub project_id: String,
    /// 任务标题。
    pub title: String,
    /// 任务提示词。
    pub prompt: String,
    /// 任务所属工作空间。
    pub workspace_path: String,
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
    migrate_database(&connection)?;
    load_workspace_data_with_connection(&connection, project_id.as_deref())
}

/// 创建项目并返回刷新后的聚合数据。
/// 流程：校验项目名称和工作空间，写入 project 表，再返回当前项目数据。
/// 参数：app 用于定位数据库，request 为前端表单。
/// 返回：以新项目为当前项目的聚合数据。
/// 边界：同一路径可创建多个项目，但项目名称不能为空。
pub fn create_project(
    app: &AppHandle,
    request: CreateProjectRequest,
) -> Result<WorkspaceDataResponse, String> {
    let name = request.name.trim();
    let workspace_path = request.workspace_path.trim();
    if name.is_empty() {
        return Err("项目名称不能为空".to_string());
    }
    if workspace_path.is_empty() {
        return Err("工作空间不能为空".to_string());
    }
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    let project_id = next_id("proj");
    connection
        .execute(
            "INSERT INTO project (id, name, workspace_path) VALUES (?1, ?2, ?3)",
            params![project_id, name, workspace_path],
        )
        .map_err(database_error)?;
    load_workspace_data_with_connection(&connection, Some(&project_id))
}

/// 创建任务并返回刷新后的项目数据。
/// 流程：校验项目、标题和提示词，写入已创建任务状态，记录创建事件。
/// 参数：app 用于定位数据库，request 为任务表单。
/// 返回：当前项目下刷新后的聚合数据。
/// 边界：任务创建后不会自动执行，只有进入排队中后调度器才处理。
pub fn create_task(
    app: &AppHandle,
    request: CreateTaskRequest,
) -> Result<WorkspaceDataResponse, String> {
    let project_id = request.project_id.trim();
    let title = request.title.trim();
    let prompt = request.prompt.trim();
    if project_id.is_empty() {
        return Err("请选择项目".to_string());
    }
    if title.is_empty() {
        return Err("任务标题不能为空".to_string());
    }
    if prompt.is_empty() {
        return Err("任务内容不能为空".to_string());
    }
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    ensure_project_exists(&connection, project_id)?;
    let task_id = next_id("task");
    connection
        .execute(
            "INSERT INTO task (id, project_id, title, prompt, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, project_id, title, prompt, TaskStatus::Created.as_str()],
        )
        .map_err(database_error)?;
    append_task_event(
        &connection,
        &task_id,
        "created",
        "",
        TaskStatus::Created.as_str(),
        "任务已创建",
        "{}",
    )?;
    load_workspace_data_with_connection(&connection, Some(project_id))
}

/// 将已创建或失败任务推入排队中，并返回可执行的队列任务。
/// 流程：校验任务状态，只允许 created/failed/cancelled 进入 queued，再返回完整执行上下文。
/// 参数：app 用于定位数据库，task_id 为目标任务。
/// 返回：队列任务记录，供调用方启动 CodeX 会话。
/// 边界：running、queued、waiting_acceptance、completed 不能重复排队。
pub fn queue_task(app: &AppHandle, task_id: &str) -> Result<QueuedTaskRecord, String> {
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    let task = read_queued_task_context(&connection, task_id)?;
    let current_status: String = connection
        .query_row(
            "SELECT status FROM task WHERE id = ?1 AND archived_at IS NULL",
            params![task_id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if !matches!(
        current_status.as_str(),
        "created" | "failed" | "cancelled"
    ) {
        return Err("当前状态不能进入排队中".to_string());
    }
    update_task_status(
        &connection,
        task_id,
        &current_status,
        TaskStatus::Queued.as_str(),
        "",
    )?;
    Ok(task)
}

/// 将待验收任务标记为已完成，并同步会话状态。
/// 流程：校验任务必须为 waiting_acceptance，再把任务和关联会话一起置为 completed。
/// 参数：app 用于定位数据库，task_id 为目标任务。
/// 返回：刷新后的聚合数据。
/// 边界：非待验收任务不能直接完成，避免绕过执行链路。
pub fn complete_task(app: &AppHandle, task_id: &str) -> Result<WorkspaceDataResponse, String> {
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    let (project_id, status, session_id): (String, String, String) = connection
        .query_row(
            "SELECT project_id, status, COALESCE(current_session_id, '') FROM task WHERE id = ?1 AND archived_at IS NULL",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(database_error)?;
    if status != TaskStatus::WaitingAcceptance.as_str() {
        return Err("只有待验收任务可以标记为已完成".to_string());
    }
    update_task_status(
        &connection,
        task_id,
        &status,
        TaskStatus::Completed.as_str(),
        "",
    )?;
    if !session_id.is_empty() {
        update_session_status(
            &connection,
            &session_id,
            SessionStatus::Completed.as_str(),
            "",
        )?;
    }
    load_workspace_data_with_connection(&connection, Some(&project_id))
}

/// 记录任务已经开始运行并绑定本地会话。
/// 流程：创建 session 记录，将任务状态从 queued 改为 running 并保存 current_session_id。
/// 参数：app 用于定位数据库，task 为调度候选任务。
/// 返回：新建的本地会话 ID。
/// 边界：只有 queued 状态会被更新，避免重复执行覆盖已有会话。
pub fn mark_task_running(app: &AppHandle, task: &QueuedTaskRecord) -> Result<String, String> {
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    let session_id = next_id("sess");
    connection
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
    connection
        .execute(
            "UPDATE task SET status = ?1, current_session_id = ?2, started_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?3 AND status = ?4",
            params![TaskStatus::Running.as_str(), session_id, task.id, TaskStatus::Queued.as_str()],
        )
        .map_err(database_error)?;
    append_task_event(
        &connection,
        &task.id,
        "status_changed",
        TaskStatus::Queued.as_str(),
        TaskStatus::Running.as_str(),
        "任务开始执行",
        "{}",
    )?;
    Ok(session_id)
}

/// 记录 CodeX 会话创建成功并进入待验收。
/// 流程：保存外部 thread ID，将任务和会话流转为 waiting_acceptance。
/// 参数：app 用于定位数据库，task_id/session_id/thread_id 为本地与外部绑定关系。
/// 返回：无返回值。
/// 边界：如果任务已经被用户取消，仍保留会话绑定但不强制完成取消任务。
pub fn mark_task_waiting_acceptance(
    app: &AppHandle,
    task_id: &str,
    session_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    connection
        .execute(
            "UPDATE session SET status = ?1, external_thread_id = ?2, external_url = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?4",
            params![
                SessionStatus::WaitingAcceptance.as_str(),
                thread_id,
                format!("codex://threads/{}", thread_id),
                session_id
            ],
        )
        .map_err(database_error)?;
    update_task_status(
        &connection,
        task_id,
        TaskStatus::Running.as_str(),
        TaskStatus::WaitingAcceptance.as_str(),
        "",
    )
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
    let connection = open_database(app)?;
    migrate_database(&connection)?;
    let message = limit_text(error, 500);
    update_session_status(
        &connection,
        session_id,
        SessionStatus::Failed.as_str(),
        &message,
    )?;
    update_task_status(
        &connection,
        task_id,
        TaskStatus::Running.as_str(),
        TaskStatus::Failed.as_str(),
        &message,
    )
}

/// 重建业务表结构并清空项目、任务、会话和事件数据。
/// 流程：删除业务表和迁移记录，再重新执行当前最新迁移。
/// 参数：app 用于定位数据库。
/// 返回：清空后的聚合数据。
/// 边界：只影响任务管理业务库，不清理客户端 JSON 设置。
pub fn reset_schema(app: &AppHandle) -> Result<WorkspaceDataResponse, String> {
    let connection = open_database(app)?;
    drop_business_tables(&connection)?;
    migrate_database(&connection)?;
    load_workspace_data_with_connection(&connection, None)
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

/// 执行当前版本所需数据库迁移。
fn migrate_database(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_migrations (
              version INTEGER PRIMARY KEY,
              name TEXT NOT NULL,
              applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              checksum TEXT NOT NULL
            );
            ",
        )
        .map_err(database_error)?;
    let current_version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if current_version < CURRENT_SCHEMA_VERSION {
        apply_initial_schema(connection)?;
    }
    Ok(())
}

/// 应用首版任务管理表结构。
fn apply_initial_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS project (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              workspace_path TEXT NOT NULL,
              provider_scope TEXT NOT NULL DEFAULT 'local',
              description TEXT NOT NULL DEFAULT '',
              color TEXT NOT NULL DEFAULT '',
              sort_order INTEGER NOT NULL DEFAULT 0,
              metadata_json TEXT NOT NULL DEFAULT '{}',
              archived_at TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS session (
              id TEXT PRIMARY KEY,
              project_id TEXT NOT NULL,
              task_id TEXT,
              provider TEXT NOT NULL,
              workspace_path TEXT NOT NULL,
              title TEXT NOT NULL DEFAULT '',
              status TEXT NOT NULL,
              external_thread_id TEXT NOT NULL DEFAULT '',
              external_url TEXT NOT NULL DEFAULT '',
              external_status TEXT NOT NULL DEFAULT '',
              last_error TEXT NOT NULL DEFAULT '',
              metadata_json TEXT NOT NULL DEFAULT '{}',
              archived_at TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              FOREIGN KEY(project_id) REFERENCES project(id),
              FOREIGN KEY(task_id) REFERENCES task(id)
            );

            CREATE TABLE IF NOT EXISTS task (
              id TEXT PRIMARY KEY,
              project_id TEXT NOT NULL,
              title TEXT NOT NULL,
              description TEXT NOT NULL DEFAULT '',
              prompt TEXT NOT NULL,
              status TEXT NOT NULL,
              priority INTEGER NOT NULL DEFAULT 0,
              task_type TEXT NOT NULL DEFAULT 'general',
              current_session_id TEXT NOT NULL DEFAULT '',
              last_error TEXT NOT NULL DEFAULT '',
              metadata_json TEXT NOT NULL DEFAULT '{}',
              archived_at TEXT,
              queued_at TEXT,
              started_at TEXT,
              finished_at TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              FOREIGN KEY(project_id) REFERENCES project(id)
            );

            CREATE TABLE IF NOT EXISTS task_event (
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

            CREATE TABLE IF NOT EXISTS session_event (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              event_type TEXT NOT NULL,
              from_status TEXT NOT NULL DEFAULT '',
              to_status TEXT NOT NULL DEFAULT '',
              message TEXT NOT NULL DEFAULT '',
              payload_json TEXT NOT NULL DEFAULT '{}',
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              FOREIGN KEY(session_id) REFERENCES session(id)
            );

            CREATE INDEX IF NOT EXISTS idx_project_workspace ON project(workspace_path);
            CREATE INDEX IF NOT EXISTS idx_task_project_status ON task(project_id, status, priority, created_at);
            CREATE INDEX IF NOT EXISTS idx_task_current_session ON task(current_session_id);
            CREATE INDEX IF NOT EXISTS idx_session_project_status ON session(project_id, provider, status, updated_at);
            CREATE INDEX IF NOT EXISTS idx_session_external_thread ON session(provider, external_thread_id);
            CREATE INDEX IF NOT EXISTS idx_task_event_task_time ON task_event(task_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_session_event_session_time ON session_event(session_id, created_at);
            ",
        )
        .map_err(database_error)?;
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![CURRENT_SCHEMA_VERSION, INITIAL_SCHEMA_NAME, "001-session-task-schema"],
        )
        .map_err(database_error)?;
    Ok(())
}

/// 删除任务管理业务表，用于系统设置里的调试恢复入口。
fn drop_business_tables(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = OFF;
            DROP TABLE IF EXISTS session_event;
            DROP TABLE IF EXISTS task_event;
            DROP TABLE IF EXISTS session;
            DROP TABLE IF EXISTS task;
            DROP TABLE IF EXISTS project;
            DROP TABLE IF EXISTS schema_migrations;
            PRAGMA foreign_keys = ON;
            ",
        )
        .map_err(database_error)
}

/// 读取指定项目的聚合数据。
fn load_workspace_data_with_connection(
    connection: &Connection,
    project_id: Option<&str>,
) -> Result<WorkspaceDataResponse, String> {
    let projects = list_projects(connection)?;
    let selected_project_id = project_id
        .filter(|id| projects.iter().any(|project| project.id == *id))
        .map(ToString::to_string)
        .or_else(|| projects.first().map(|project| project.id.clone()));
    let tasks = if let Some(id) = selected_project_id.as_deref() {
        list_tasks(connection, id)?
    } else {
        Vec::new()
    };
    let sessions = if let Some(id) = selected_project_id.as_deref() {
        list_sessions(connection, id)?
    } else {
        Vec::new()
    };
    Ok(WorkspaceDataResponse {
        projects,
        tasks,
        sessions,
    })
}

/// 查询项目列表并聚合任务和会话数量。
fn list_projects(connection: &Connection) -> Result<Vec<ProjectRecord>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT p.id,
                   p.name,
                   p.workspace_path,
                   COUNT(DISTINCT t.id) AS task_count,
                   COUNT(DISTINCT s.id) AS session_count,
                   p.created_at,
                   p.updated_at
              FROM project p
         LEFT JOIN task t ON t.project_id = p.id AND t.archived_at IS NULL
         LEFT JOIN session s ON s.project_id = p.id AND s.archived_at IS NULL
             WHERE p.archived_at IS NULL
          GROUP BY p.id
          ORDER BY p.sort_order ASC, p.updated_at DESC, p.created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ProjectRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                workspace_path: row.get(2)?,
                task_count: row.get(3)?,
                session_count: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 查询项目下任务列表。
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
                   t.created_at,
                   t.updated_at
              FROM task t
         LEFT JOIN session s ON s.id = t.current_session_id
             WHERE t.project_id = ?1
               AND t.archived_at IS NULL
          ORDER BY t.updated_at DESC, t.created_at DESC
            ",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                prompt: row.get(3)?,
                status: row.get(4)?,
                current_session_id: row.get(5)?,
                external_thread_id: row.get(6)?,
                last_error: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(database_error)?;
    collect_rows(rows)
}

/// 查询项目下会话列表。
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
               AND archived_at IS NULL
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

/// 读取任务执行需要的项目和工作空间上下文。
fn read_queued_task_context(
    connection: &Connection,
    task_id: &str,
) -> Result<QueuedTaskRecord, String> {
    connection
        .query_row(
            "
            SELECT t.id, t.project_id, t.title, t.prompt, p.workspace_path
              FROM task t
              JOIN project p ON p.id = t.project_id
             WHERE t.id = ?1
               AND t.archived_at IS NULL
               AND p.archived_at IS NULL
            ",
            params![task_id],
            |row| {
                Ok(QueuedTaskRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    prompt: row.get(3)?,
                    workspace_path: row.get(4)?,
                })
            },
        )
        .map_err(database_error)
}

/// 确认项目存在，避免前端传入过期项目 ID 后创建孤儿任务。
fn ensure_project_exists(connection: &Connection, project_id: &str) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM project WHERE id = ?1 AND archived_at IS NULL",
            params![project_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(database_error)?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err("项目不存在或已归档".to_string())
    }
}

/// 更新任务状态并写入事件表。
fn update_task_status(
    connection: &Connection,
    task_id: &str,
    from_status: &str,
    to_status: &str,
    error: &str,
) -> Result<(), String> {
    let finished_at_sql = if matches!(to_status, "completed" | "failed" | "cancelled") {
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
        "UPDATE task SET status = ?1, last_error = ?2, updated_at = CURRENT_TIMESTAMP{}{} WHERE id = ?3",
        queued_at_sql, finished_at_sql
    );
    connection
        .execute(&sql, params![to_status, error, task_id])
        .map_err(database_error)?;
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

/// 更新会话状态，并记录最近错误。
fn update_session_status(
    connection: &Connection,
    session_id: &str,
    status: &str,
    error: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE session SET status = ?1, last_error = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
            params![status, error, session_id],
        )
        .map_err(database_error)?;
    Ok(())
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
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}_{}", prefix, nanos)
}

/// 返回状态变化事件的用户可读说明。
fn status_message(status: &str) -> &'static str {
    match status {
        "queued" => "任务已进入排队中",
        "running" => "任务开始执行",
        "waiting_acceptance" => "任务执行完成，等待验收",
        "completed" => "任务已完成",
        "failed" => "任务执行失败",
        "cancelled" => "任务已取消",
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
