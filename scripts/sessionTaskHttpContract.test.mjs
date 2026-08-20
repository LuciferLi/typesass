import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const SCRIPT_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const PROJECT_DIRECTORY = join(SCRIPT_DIRECTORY, '..');
const COMMAND_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src', 'service', 'tauri', 'command.ts');
const SESSION_STORE_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src', 'stores', 'sessionManage.ts');
const SESSION_MODEL_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src', 'model', 'sessionManage.ts');
const CODEX_CONNECTION_STORE_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src', 'stores', 'codexConnection.ts');
const CODEX_DESKTOP_RUST_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src-tauri', 'src', 'codex_desktop.rs');
const CODEX_CDP_RUST_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src-tauri', 'src', 'codex_cdp.rs');
const TAURI_LIB_RUST_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src-tauri', 'src', 'lib.rs');
const DESKTOP_ERROR_RUST_SOURCE_PATH = join(PROJECT_DIRECTORY, 'src-tauri', 'src', 'desktop_error.rs');

/**
 * 会话与任务 HTTP 导出函数的固定源码契约。
 * 字段：name 为前端导出函数名；pathPattern 为允许动态 ID 但必须精确匹配的 HTTP 路径；method 为固定请求方法；legacyCommand 为禁止恢复的旧 Tauri 命令名。
 */
const SESSION_TASK_HTTP_CONTRACTS = [
    {
        name: 'openSessionExternalThread',
        pathPattern: /`\/v1\/codex\/threads\/\$\{encodeURIComponent\(threadId\)\}\/open`/,
        method: 'POST',
        legacyCommand: 'open_codex_desktop_thread'
    },
    {
        name: 'listCodexWorkspaces',
        pathPattern: /['"]\/v1\/codex\/workspaces['"]/,
        method: 'GET',
        legacyCommand: 'list_codex_workspaces'
    },
    {
        name: 'listCodexThreads',
        pathPattern: /['"]\/v1\/codex\/threads\/search['"]/,
        method: 'POST',
        legacyCommand: 'list_codex_threads'
    },
    {
        name: 'loadSessionWorkspaceData',
        pathPattern: /['"]\/v1\/task-workspace\/query['"]/,
        method: 'POST',
        legacyCommand: 'load_session_workspace_data'
    },
    {
        name: 'createSessionProject',
        pathPattern: /['"]\/v1\/projects['"]/,
        method: 'POST',
        legacyCommand: 'create_session_project'
    },
    {
        name: 'updateSessionProject',
        pathPattern: /`\/v1\/projects\/\$\{encodeURIComponent\(request\.id\)\}\/update`/,
        method: 'POST',
        legacyCommand: 'update_session_project'
    },
    {
        name: 'deleteSessionProject',
        pathPattern: /`\/v1\/projects\/\$\{encodeURIComponent\(projectId\)\}\/delete`/,
        method: 'POST',
        legacyCommand: 'delete_session_project'
    },
    {
        name: 'createSessionTask',
        pathPattern: /['"]\/v1\/tasks['"]/,
        method: 'POST',
        legacyCommand: 'create_session_task'
    },
    {
        name: 'updateSessionTask',
        pathPattern: /`\/v1\/tasks\/\$\{encodeURIComponent\(request\.id\)\}\/update`/,
        method: 'POST',
        legacyCommand: 'update_session_task'
    },
    {
        name: 'deleteSessionTask',
        pathPattern: /`\/v1\/tasks\/\$\{encodeURIComponent\(taskId\)\}\/delete`/,
        method: 'POST',
        legacyCommand: 'delete_session_task'
    },
    {
        name: 'queueSessionTask',
        pathPattern: /`\/v1\/tasks\/\$\{encodeURIComponent\(taskId\)\}\/queue`/,
        method: 'POST',
        legacyCommand: 'queue_session_task'
    },
    {
        name: 'completeSessionTask',
        pathPattern: /`\/v1\/tasks\/\$\{encodeURIComponent\(taskId\)\}\/complete`/,
        method: 'POST',
        legacyCommand: 'complete_session_task'
    }
];

/**
 * 从 TypeScript 源码提取指定导出异步函数的完整声明片段。
 * 流程：定位稳定的 export async function 起点，再以后一项导出异步函数或文件结尾作为终点，保留签名与函数体供静态契约断言。
 * 参数：source 为完整 TypeScript 源码，functionName 为目标导出函数名。
 * 返回：包含目标签名和函数体的源码片段。
 * 异常/边界：函数被删除、改为非导出异步函数或出现重复声明时立即抛出，让迁移契约显式失败而不是误检其它函数。
 */
function extractExportedAsyncFunction(source, functionName) {
    const declaration = `export async function ${functionName}`;
    const startIndex = source.indexOf(declaration);
    assert.notEqual(startIndex, -1, `缺少导出函数 ${functionName}`);
    assert.equal(source.indexOf(declaration, startIndex + declaration.length), -1, `导出函数 ${functionName} 重复声明`);
    const nextFunctionIndex = source.indexOf('\nexport async function ', startIndex + declaration.length);
    return source.slice(startIndex, nextFunctionIndex === -1 ? source.length : nextFunctionIndex);
}

/**
 * 从 Pinia Store 源码提取指定 action 的声明片段。
 * 流程：定位 action 名和同步或异步签名，再以同级下一个文档注释作为边界，隔离当前 action 的实现。
 * 参数：source 为完整 Store 源码， actionName 为目标 action 名。
 * 返回：目标 action 的源码片段。
 * 异常/边界：action 不存在时直接失败；该辅助方法仅用于当前遵循统一 JSDoc 分隔格式的 Store。
 */
function extractStoreAction(source, actionName) {
    const declarationPattern = new RegExp(
        `^ {8}(?:async\\s+)?${actionName}\\s*\\([^\\n]*\\)(?:\\s*:\\s*[^\\{\\n]+)?\\s*\\{`,
        'm'
    );
    const match = declarationPattern.exec(source);
    assert.ok(match, `缺少 Store action ${actionName}`);
    const nextActionCommentIndex = source.indexOf('\n\n        /**', match.index + match[0].length);
    return source.slice(match.index, nextActionCommentIndex === -1 ? source.length : nextActionCommentIndex);
}

test('SESSION-TASK-HTTP-001 十二个会话任务函数固定使用公共 HTTP 契约', /**
     * 验证会话浏览与任务管理的十二个前端入口全部通过公共 HTTP 服务访问指定路由。
 * 流程：读取 command.ts，逐项提取函数体并断言 requestPublicApi、精确路径和 GET/POST 方法。
 * 参数：无，契约表提供函数名、路径正则和方法。
 * 返回：十项源码契约全部匹配后完成 Promise。
 * 异常/边界：函数改名、路径漂移、方法变化或绕开统一 HTTP 客户端都会使测试失败。
 */ async () => {
    const commandSource = await readFile(COMMAND_SOURCE_PATH, 'utf8');

    assert.equal(SESSION_TASK_HTTP_CONTRACTS.length, 12);
    for (const contract of SESSION_TASK_HTTP_CONTRACTS) {
        const functionSource = extractExportedAsyncFunction(commandSource, contract.name);
        assert.match(functionSource, /requestPublicApi\s*</, `${contract.name} 必须调用 requestPublicApi`);
        assert.match(functionSource, contract.pathPattern, `${contract.name} HTTP 路径不符合契约`);
        assert.match(
            functionSource,
            new RegExp(`method\\s*:\\s*['"]${contract.method}['"]`),
            `${contract.name} 必须使用 ${contract.method}`
        );
    }
});

test('SESSION-TASK-HTTP-002 会话任务函数禁止回退 Tauri IPC 或开启瞬时重试', /**
 * 验证迁移后的业务函数不会通过旧 IPC 绕过 HTTP，也不会对会产生副作用的请求自动重试。
 * 流程：逐项提取十二个函数体，拒绝 invoke、invokeSessionDesktop、旧命令名和 retryTransientErrors 配置。
 * 参数：无，使用固定源码契约表中的函数名与旧命令名。
 * 返回：所有禁止模式均不存在后完成 Promise。
 * 异常/边界：即使旧调用只出现在单个函数分支或错误兜底中也会失败，防止桌面与 Web 行为再次分叉。
 */ async () => {
    const commandSource = await readFile(COMMAND_SOURCE_PATH, 'utf8');

    for (const contract of SESSION_TASK_HTTP_CONTRACTS) {
        const functionSource = extractExportedAsyncFunction(commandSource, contract.name);
        assert.doesNotMatch(functionSource, /\binvoke(?:Desktop|SessionDesktop)?\s*</, `${contract.name} 禁止调用 IPC`);
        assert.doesNotMatch(functionSource, /\binvokeSessionDesktop\b/, `${contract.name} 禁止调用旧会话 IPC 封装`);
        assert.ok(!functionSource.includes(contract.legacyCommand), `${contract.name} 禁止使用 ${contract.legacyCommand}`);
        assert.doesNotMatch(
            functionSource,
            /\bretryTransientErrors\s*:/,
            `${contract.name} 禁止开启自动瞬时错误重试`
        );
    }
});

test('SESSION-TASK-HTTP-003 活动任务状态仅通过 HTTP 有界轮询刷新', /**
 * 验证任务看板仅在排队中或执行中任务存在时，使用 loadSessionWorkspaceData 获取 Rust 权威状态，并彻底移除旧 Tauri 事件刷新链路。
 * 流程：读取 sessionManage Store，隔离 listenTaskUpdates action，断言存在活动任务门禁、有界轮询与 HTTP 数据加载，同时全文件禁止 session-task-updated 事件名。
 * 参数：无。
 * 返回：轮询加载和事件禁用契约均满足后完成 Promise。
 * 异常/边界：删除刷新、恢复事件监听或改为其它非权威数据源都会使测试失败。
 */ async () => {
    const storeSource = await readFile(SESSION_STORE_SOURCE_PATH, 'utf8');
    const refreshActionSource = extractStoreAction(storeSource, 'listenTaskUpdates');

    assert.match(refreshActionSource, /window\.setInterval\s*\(/, '任务状态刷新必须使用有界轮询');
    assert.match(
        refreshActionSource,
        /task\.status\s*===\s*['"]queued['"]\s*\|\|\s*task\.status\s*===\s*['"]running['"]/,
        '只有排队中或执行中任务需要轮询'
    );
    assert.match(refreshActionSource, /!hasActiveTask/, '没有活动任务时必须跳过 HTTP 轮询');
    assert.match(refreshActionSource, /requestInFlight/, '轮询必须使用单飞标记防止慢请求重叠');
    assert.match(
        refreshActionSource,
        /this\.selectedProjectId\s*!==\s*requestedProjectId/,
        '项目切换后必须丢弃旧项目响应'
    );
    assert.match(
        refreshActionSource,
        /this\.workspaceDataRevision\s*!==\s*requestedRevision/,
        '写操作或新权威数据到达后必须丢弃旧版本响应'
    );
    assert.match(refreshActionSource, /stopped\s*=\s*true/, '页面卸载后必须屏蔽在途响应');
    assert.match(
        refreshActionSource,
        /loadSessionWorkspaceData\s*\(\s*requestedProjectId\s*\)/,
        '任务状态刷新必须调用 loadSessionWorkspaceData'
    );
    assert.ok(!storeSource.includes('session-task-updated'), 'Store 禁止监听旧 session-task-updated 事件');
});

test('SESSION-TASK-HTTP-004 普通 Web 初始化跳过配置 IPC 并继续加载 HTTP 数据', /**
 * 验证普通 Web 初始化不会读取桌面客户端配置，同时会话和任务初始化仍进入统一 HTTP 数据链路。
 * 流程：分别提取会话与任务初始化 action，断言 readClientJson 只位于 isTauriRuntime 真值分支，并核对会话刷新和任务工作区 HTTP 加载调用。
 * 参数：无。
 * 返回：两个初始化 action 的运行时分流与业务加载契约均满足后完成 Promise。
 * 异常/边界：配置读取移出 Tauri guard、普通 Web 提前跳过业务加载或任务数据改走其它来源都会使测试失败。
 */ async () => {
    const storeSource = await readFile(SESSION_STORE_SOURCE_PATH, 'utf8');
    const sessionInitializationSource = extractStoreAction(storeSource, 'initSessionManage');
    const taskInitializationSource = extractStoreAction(storeSource, 'initTaskManage');
    const tauriGuardedConfigReadPattern =
        /const\s+saved\s*=\s*isTauriRuntime\(\)\s*\?\s*await\s+readClientJson<SessionManagePersistedStateModel>/;

    assert.match(
        sessionInitializationSource,
        tauriGuardedConfigReadPattern,
        '会话初始化只能在 Tauri 分支读取客户端配置'
    );
    assert.match(
        taskInitializationSource,
        tauriGuardedConfigReadPattern,
        '任务初始化只能在 Tauri 分支读取客户端配置'
    );
    assert.match(sessionInitializationSource, /await\s+this\.refreshCodexWorkspaces\(\)/, '会话初始化必须加载 HTTP 工作空间');
    assert.match(
        sessionInitializationSource,
        /await\s+this\.refreshCodexThreads\(undefined,\s*true\)/,
        '会话初始化必须加载 HTTP 会话列表'
    );
    assert.match(
        taskInitializationSource,
        /loadSessionWorkspaceData\(this\.selectedProjectId\s*\|\|\s*undefined\)/,
        '任务初始化必须加载 HTTP 任务工作区'
    );
});

test('SESSION-TASK-HTTP-005 Tauri 仅持久化选择且 Store 禁止业务 IPC', /**
 * 验证桌面端仍可保存页面选择，但项目、任务、会话等业务数据不会写入客户端配置或回退旧 Tauri 命令。
 * 流程：提取 persistSelection action，断言普通 Web 在写配置前立即返回、写入值仅含两个选择字段；随后扫描 Store 禁止 invoke 和全部旧业务命令。
 * 参数：无。
 * 返回：选择配置白名单与业务 IPC 黑名单全部满足后完成 Promise。
 * 异常/边界：新增业务数据副本、移除普通 Web guard、直接 invoke 或恢复任一旧命令都会使测试失败。
 */ async () => {
    const storeSource = await readFile(SESSION_STORE_SOURCE_PATH, 'utf8');
    const persistenceSource = extractStoreAction(storeSource, 'persistSelection');
    const forbiddenBusinessCommands = SESSION_TASK_HTTP_CONTRACTS.map((contract) => contract.legacyCommand);

    assert.match(
        persistenceSource,
        /if\s*\(\s*!isTauriRuntime\(\)\s*\)\s*return\s*;/,
        '普通 Web 必须在客户端配置写入前返回'
    );
    assert.match(
        persistenceSource,
        /writeClientJson<SessionManagePersistedStateModel>\(StorageKey\.sessionManage/,
        'Tauri 必须继续使用既有会话选择配置分区'
    );
    assert.match(persistenceSource, /selectedProjectId:\s*this\.selectedProjectId/, '配置只保存当前项目选择');
    assert.match(persistenceSource, /selectedWorkspaceCwd:\s*this\.selectedWorkspaceCwd/, '配置只保存当前工作空间选择');
    assert.doesNotMatch(persistenceSource, /\b(?:projects|tasks|sessions|codexThreads)\s*:/, '禁止持久化业务数据副本');
    assert.doesNotMatch(storeSource, /\binvoke(?:Desktop|SessionDesktop)?\s*</, 'Store 禁止直接调用 Tauri IPC');
    assert.doesNotMatch(storeSource, /\binvokeSessionDesktop\b/, 'Store 禁止恢复旧会话 IPC 封装');
    for (const commandName of forbiddenBusinessCommands) {
        assert.ok(!storeSource.includes(commandName), `Store 禁止使用旧业务命令 ${commandName}`);
    }
});

test('SESSION-TASK-HTTP-006 创建任务必须使用服务端返回的唯一任务 ID', /**
 * 验证创建任务的 HTTP 契约显式返回并校验 createdTaskId，禁止并发同名任务通过标题猜测本次创建结果。
 * 流程：读取前端模型、HTTP 导出函数和 Store action，断言专用响应类型包含 createdTaskId，服务层使用该类型，Store 在采用聚合数据前校验字段。
 * 参数：无。
 * 返回：三层源码契约全部满足后完成 Promise。
 * 异常/边界：字段被删除、接口退回通用聚合类型或页面忽略空 ID 时测试失败；本测试不允许用标题搜索作为兜底。
 */ async () => {
    const [modelSource, commandSource, storeSource] = await Promise.all([
        readFile(SESSION_MODEL_SOURCE_PATH, 'utf8'),
        readFile(COMMAND_SOURCE_PATH, 'utf8'),
        readFile(SESSION_STORE_SOURCE_PATH, 'utf8')
    ]);
    const createFunctionSource = extractExportedAsyncFunction(commandSource, 'createSessionTask');
    const createActionSource = extractStoreAction(storeSource, 'addTask');

    assert.match(
        modelSource,
        /interface\s+CreateSessionTaskResponseModel\s+extends\s+SessionWorkspaceDataModel[\s\S]*createdTaskId:\s*string/,
        '创建任务专用响应必须包含 createdTaskId'
    );
    assert.match(
        createFunctionSource,
        /Promise<CreateSessionTaskResponseModel>[\s\S]*requestPublicApi<CreateSessionTaskResponseModel>/,
        '创建任务 HTTP 函数必须使用专用响应类型'
    );
    assert.match(createActionSource, /if\s*\(\s*!response\.createdTaskId\s*\)\s*throw\s+new\s+Error/, 'Store 必须拒绝空任务 ID');
    assert.doesNotMatch(createActionSource, /(?:find|filter)\s*\([\s\S]*title/, 'Store 禁止按标题猜测本次创建任务');
});

test('SESSION-TASK-HTTP-007 Codex 断连自动弹窗每个 Hub 生命周期仅一次', /**
 * 验证 connected 与 disconnected 反复切换不会重置自动提示门禁，同时保留用户主动打开和任务 503 显式打开能力。
 * 流程：分别提取 applyConnectionStatus、openDialog 与 markDisconnectedFromBusinessError，断言自动打开使用生命周期门禁、连接恢复不清零门禁、两个显式入口直接打开弹窗。
 * 参数：无。
 * 返回：三个 Store action 的源码契约全部满足后完成 Promise。
 * 异常/边界：connected 分支重置 outageDialogShown、自动打开绕开门禁、侧栏入口或业务错误入口不再显式打开时测试失败。
 */ async () => {
    const storeSource = await readFile(CODEX_CONNECTION_STORE_SOURCE_PATH, 'utf8');
    const applyStatusSource = extractStoreAction(storeSource, 'applyConnectionStatus');
    const openDialogSource = extractStoreAction(storeSource, 'openDialog');
    const businessErrorSource = extractStoreAction(storeSource, 'markDisconnectedFromBusinessError');

    assert.match(applyStatusSource, /if\s*\(\s*status\.connected\s*\)/, '连接快照必须显式处理 connected 分支');
    assert.doesNotMatch(
        applyStatusSource,
        /outageDialogShown\s*=\s*false/,
        'connected 恢复不得重置 Hub 生命周期内已经展示过的断连提示门禁'
    );
    assert.match(
        applyStatusSource,
        /if\s*\(\s*allowAutoOpen\s*&&\s*!this\.outageDialogShown\s*\)/,
        '自动断连弹窗必须同时受允许标记和生命周期门禁约束'
    );
    assert.match(
        applyStatusSource,
        /this\.dialogOpen\s*=\s*true;[\s\S]*this\.outageDialogShown\s*=\s*true/,
        '首次自动打开后必须永久记录当前 Hub 生命周期已提示'
    );
    assert.match(openDialogSource, /this\.dialogOpen\s*=\s*true/, '用户点击侧栏状态必须仍可显式打开弹窗');
    assert.match(
        businessErrorSource,
        /this\.dialogOpen\s*=\s*true;[\s\S]*this\.outageDialogShown\s*=\s*true/,
        '任务 503 未连接错误必须显式打开弹窗并记录已提示'
    );
    assert.match(
        businessErrorSource,
        /this\.refreshConnection\(false\)/,
        '任务 503 后只允许静默刷新连接详情，禁止触发第二次自动弹窗'
    );
});

test('SESSION-TASK-HTTP-008 Codex 连接轮询必须先校验 Token 且无 Token 时可恢复', /**
 * 验证 Codex 连接刷新不会在未授权时反复发出 401 HTTP 请求，同时不会因一次无 Token 永久停止后续轮询。
 * 流程：提取 performConnectionRefresh 与 refreshConnection，断言 Token 检查位于状态接口之前，无 Token 分支直接结束当前轮且不调用状态接口，最终释放请求标记和单飞槽。
 * 参数：无。
 * 返回：授权门禁、无请求分支与下一轮恢复契约全部满足后完成 Promise。
 * 异常/边界：状态请求移到 Token 检查之前、无 Token 分支触发请求或轮询单飞状态未释放时测试失败。
 */ async () => {
    const storeSource = await readFile(CODEX_CONNECTION_STORE_SOURCE_PATH, 'utf8');
    const performRefreshSource = extractStoreAction(storeSource, 'performConnectionRefresh');
    const refreshSource = extractStoreAction(storeSource, 'refreshConnection');
    const tokenCheckIndex = performRefreshSource.indexOf('await hasPublicApiToken()');
    const statusRequestIndex = performRefreshSource.indexOf('await getCodexConnectionStatus()');
    const noTokenBranchMatch = performRefreshSource.match(
        /if\s*\(\s*!\(\s*await\s+hasPublicApiToken\(\)\s*\)\s*\)\s*\{([\s\S]*?)\n\s{16}\}/
    );

    assert.notEqual(tokenCheckIndex, -1, '连接刷新必须调用 hasPublicApiToken');
    assert.notEqual(statusRequestIndex, -1, '已授权分支必须调用 getCodexConnectionStatus');
    assert.ok(tokenCheckIndex < statusRequestIndex, '必须先校验 Token，再请求 Codex 连接状态');
    assert.ok(noTokenBranchMatch, '必须保留显式的无 Token 分支');
    assert.match(noTokenBranchMatch[1], /\breturn\s*;/, '无 Token 时必须结束当前刷新');
    assert.doesNotMatch(
        noTokenBranchMatch[1],
        /getCodexConnectionStatus\s*\(/,
        '无 Token 分支禁止请求 Codex 连接状态'
    );
    assert.match(
        performRefreshSource,
        /finally\s*\{[\s\S]*this\.requestInFlight\s*=\s*false/,
        '无 Token 返回后也必须释放当前请求标记'
    );
    assert.doesNotMatch(performRefreshSource, /(?:stopPolling\s*\(|pollingStarted\s*=\s*false)/, '无 Token 不得停止后续轮询');
    assert.match(
        refreshSource,
        /finally\s*\{[\s\S]*activeConnectionRequest\s*=\s*null/,
        '每轮刷新结束后必须释放单飞槽，使授权后下一轮可恢复'
    );
});

test('SESSION-TASK-HTTP-009 Codex 重启结果必须在固定 90 秒截止并清理计时器', /**
 * 验证重启等待使用固定 90 秒绝对截止时间，不会被重复 restarting 快照无限延长，且所有终态都释放独立计时器。
 * 流程：读取 Store 常量、状态和三个重启 action，断言截止时间、setTimeout 超时失败转换、重复快照门禁以及成功/明确失败/提交失败的清理路径。
 * 参数：无。
 * 返回：全部重启截止与计时器生命周期契约满足后完成 Promise。
 * 异常/边界：超时常量漂移、restarting 延长截止、超时后仍锁定弹窗或任一终态遗留计时器时测试失败。
 */ async () => {
    const storeSource = await readFile(CODEX_CONNECTION_STORE_SOURCE_PATH, 'utf8');
    const startDeadlineSource = extractStoreAction(storeSource, 'startRestartResultDeadline');
    const clearDeadlineSource = extractStoreAction(storeSource, 'clearRestartResultDeadline');
    const applyStatusSource = extractStoreAction(storeSource, 'applyConnectionStatus');
    const restartSource = extractStoreAction(storeSource, 'restartConnection');
    const duplicateRestartGuardIndex = startDeadlineSource.indexOf(
        'if (this.restartAwaitingResult && this.restartDeadlineAt !== null) return;'
    );
    const deadlineAssignmentIndex = startDeadlineSource.indexOf(
        'this.restartDeadlineAt = Date.now() + RESTART_RESULT_TIMEOUT_MS;'
    );

    assert.match(storeSource, /const\s+RESTART_RESULT_TIMEOUT_MS\s*=\s*90_000\s*;/, '重启结果超时必须固定为 90 秒');
    assert.match(storeSource, /let\s+restartResultDeadlineTimer:\s*number\s*\|\s*undefined\s*;/, '重启结果必须使用独立计时器');
    assert.match(storeSource, /restartDeadlineAt:\s*number\s*\|\s*null\s*;/, '状态必须保存重启绝对截止时间');
    assert.match(storeSource, /restartDeadlineAt:\s*null\s*,/, '初始状态不得伪造活动截止时间');
    assert.notEqual(duplicateRestartGuardIndex, -1, '重复 restarting 快照必须直接复用原截止时间');
    assert.ok(
        duplicateRestartGuardIndex < deadlineAssignmentIndex,
        '重复 restarting 的门禁必须位于截止时间赋值之前'
    );
    assert.match(
        startDeadlineSource,
        /restartResultDeadlineTimer\s*=\s*window\.setTimeout\([\s\S]*RESTART_RESULT_TIMEOUT_MS\s*\)/,
        '重启结果必须使用独立 setTimeout 执行截止'
    );
    assert.match(startDeadlineSource, /this\.restartAwaitingResult\s*=\s*false/, '超时必须解除重启等待锁定');
    assert.match(startDeadlineSource, /this\.restartDeadlineAt\s*=\s*null/, '超时必须清除绝对截止时间');
    assert.match(startDeadlineSource, /this\.dialogResult\s*=\s*['"]failure['"]/, '超时必须转为失败结果');
    assert.match(startDeadlineSource, /this\.dialogOpen\s*=\s*true/, '超时失败必须保持错误可见');
    assert.match(clearDeadlineSource, /window\.clearTimeout\(restartResultDeadlineTimer\)/, '统一清理 action 必须取消计时器');
    assert.match(clearDeadlineSource, /restartResultDeadlineTimer\s*=\s*undefined/, '统一清理 action 必须释放计时器引用');
    assert.match(
        applyStatusSource,
        /if\s*\(\s*status\.connected\s*\)[\s\S]*this\.clearRestartResultDeadline\(\)/,
        '连接成功必须清理重启截止计时器'
    );
    assert.match(
        applyStatusSource,
        /if\s*\(\s*this\.restartAwaitingResult\s*\)[\s\S]*this\.clearRestartResultDeadline\(\)[\s\S]*this\.dialogResult\s*=\s*['"]failure['"]/,
        '服务端明确失败必须清理计时器并转为失败态'
    );
    assert.match(
        restartSource,
        /catch\s*\([^)]*\)\s*\{[\s\S]*CODEX_RESTART_IN_PROGRESS_ERROR_CODE[\s\S]*return;[\s\S]*this\.clearRestartResultDeadline\(\)[\s\S]*this\.dialogResult\s*=\s*['"]failure['"]/,
        '非重启中的提交失败必须清理计时器并转为失败态'
    );
});

test('SESSION-TASK-HTTP-010 Rust 连接状态必须先返回重启中再探测主进程', /**
 * 验证重启期间的连接状态查询不会因旧进程已退出而误报断连或进程检查失败。
 * 流程：隔离 Rust connection_status 函数，对比 restarting 原子状态检查与 codex_main_pids 的源码顺序，并确认前者直接返回 Restarting。
 * 参数：无。
 * 返回：重启优先契约满足后完成 Promise。
 * 异常/边界：主进程探测移到 restarting 检查之前，或重启分支不再返回 Restarting 时测试失败。
 */ async () => {
    const rustSource = await readFile(CODEX_DESKTOP_RUST_SOURCE_PATH, 'utf8');
    const functionStartIndex = rustSource.indexOf('pub(crate) fn connection_status(');
    const functionEndIndex = rustSource.indexOf('\npub(crate) fn begin_restart(', functionStartIndex);

    assert.notEqual(functionStartIndex, -1, 'Rust 必须保留 connection_status 公开状态查询');
    assert.notEqual(functionEndIndex, -1, '必须能够隔离 connection_status 函数边界');
    const connectionStatusSource = rustSource.slice(functionStartIndex, functionEndIndex);
    const restartingCheckIndex = connectionStatusSource.indexOf('runtime.restarting.load(Ordering::Acquire)');
    const mainPidCheckIndex = connectionStatusSource.indexOf('codex_main_pids()');

    assert.notEqual(restartingCheckIndex, -1, 'connection_status 必须检查重启原子状态');
    assert.notEqual(mainPidCheckIndex, -1, 'connection_status 必须探测 Codex 主进程');
    assert.ok(restartingCheckIndex < mainPidCheckIndex, '重启状态检查必须位于 Codex 主进程探测之前');
    assert.match(
        connectionStatusSource.slice(restartingCheckIndex, mainPidCheckIndex),
        /return\s+Ok\(CodexConnectionStatus\s*\{[\s\S]*state:\s*CodexConnectionState::Restarting/,
        '重启中必须在主进程探测前直接返回 Restarting'
    );
});

test('SESSION-TASK-HTTP-011 CDP 新会话必须在工作空间切换前后分别拒绝草稿', /**
 * 验证新会话提交在调用 Codex Desktop 工作空间 bridge 前检查当前草稿，并在 bridge 切换后再检查目标工作空间草稿。
 * 流程：隔离 submit_new_chat 生产函数，定位 previous_draft 拒绝、electron-set-active-workspace-root bridge 和 selected_workspace_draft 拒绝的源码位置并断言严格顺序。
 * 参数：无。
 * 返回：两次草稿保护包围 bridge 的顺序契约满足后完成 Promise。
 * 异常/边界：当前草稿检查移到 bridge 之后、目标草稿检查移到 bridge 之前或任一拒绝分支被删除时测试失败。
 */ async () => {
    const cdpSource = await readFile(CODEX_CDP_RUST_SOURCE_PATH, 'utf8');
    const functionStartIndex = cdpSource.indexOf('pub(crate) fn submit_new_chat<F>(');
    const functionEndIndex = cdpSource.indexOf('\nfn wait_for_selected_workspace(', functionStartIndex);

    assert.notEqual(functionStartIndex, -1, 'CDP 必须保留 submit_new_chat 生产函数');
    assert.notEqual(functionEndIndex, -1, '必须能够隔离 submit_new_chat 函数边界');
    const submitSource = cdpSource.slice(functionStartIndex, functionEndIndex);
    const previousDraftGuardIndex = submitSource.indexOf('if !previous_draft.is_empty()');
    const workspaceBridgeIndex = submitSource.indexOf('electron-set-active-workspace-root');
    const selectedDraftGuardIndex = submitSource.indexOf('if !selected_workspace_draft.is_empty()');

    assert.notEqual(previousDraftGuardIndex, -1, '必须拒绝当前输入框的未发送草稿');
    assert.notEqual(workspaceBridgeIndex, -1, '必须使用受支持的 Codex Desktop 工作空间 bridge');
    assert.notEqual(selectedDraftGuardIndex, -1, '必须拒绝目标工作空间的未发送草稿');
    assert.ok(previousDraftGuardIndex < workspaceBridgeIndex, '当前草稿拒绝必须早于工作空间 bridge');
    assert.ok(workspaceBridgeIndex < selectedDraftGuardIndex, '目标工作空间草稿拒绝必须晚于 bridge');
});

test('SESSION-TASK-HTTP-012 任务调度器必须传递经白名单收敛的执行诊断码', /**
 * 验证任务执行失败时，调度器不会把所有 CDP 错误压平或直接信任错误正文，而是将 task_execution_diagnostic_code 的白名单结果传给日志入口。
 * 流程：隔离 run_codex_task_dispatcher，确认 execute_codex_task 失败分支先计算 diagnostic_code，随后将同一变量传入 record_desktop_task_error。
 * 参数：无。
 * 返回：诊断码收敛与传递契约满足后完成 Promise。
 * 异常/边界：调度器绕过 task_execution_diagnostic_code、传入固定通用码或直接把外部错误正文当作错误码时测试失败。
 */ async () => {
    const libSource = await readFile(TAURI_LIB_RUST_SOURCE_PATH, 'utf8');
    const dispatcherStartIndex = libSource.indexOf('fn run_codex_task_dispatcher(app: &AppHandle)');
    const dispatcherEndIndex = libSource.indexOf('\nfn task_execution_diagnostic_code(', dispatcherStartIndex);

    assert.notEqual(dispatcherStartIndex, -1, '必须保留 Codex 任务调度器');
    assert.notEqual(dispatcherEndIndex, -1, '必须能够隔离任务调度器函数边界');
    const dispatcherSource = libSource.slice(dispatcherStartIndex, dispatcherEndIndex);
    assert.match(
        dispatcherSource,
        /let\s+diagnostic_code\s*=\s*task_execution_diagnostic_code\(&error\)\s*;/,
        '任务执行失败必须先通过 task_execution_diagnostic_code 收敛诊断码'
    );
    assert.match(
        dispatcherSource,
        /record_desktop_task_error\(app,\s*&task\.id,\s*diagnostic_code,\s*&error\)/,
        'record_desktop_task_error 必须接收收敛后的 diagnostic_code'
    );
});

test('SESSION-TASK-HTTP-013 桌面错误白名单必须保留输入框草稿错误码', /**
 * 验证 CODEX_CDP_COMPOSER_NOT_EMPTY 能以稳定错误码和固定摘要记入桌面诊断日志，方便用户提供诊断 ID 排查草稿阻断。
 * 流程：隔离 safe_error_metadata 白名单函数，断言输入错误码分支原样返回 CODEX_CDP_COMPOSER_NOT_EMPTY 并使用固定安全摘要。
 * 参数：无。
 * 返回：错误码白名单契约满足后完成 Promise。
 * 异常/边界：错误码被删除、降级为未知错误或摘要改为外部正文时测试失败。
 */ async () => {
    const desktopErrorSource = await readFile(DESKTOP_ERROR_RUST_SOURCE_PATH, 'utf8');
    const metadataStartIndex = desktopErrorSource.indexOf('fn safe_error_metadata(');
    const metadataEndIndex = desktopErrorSource.indexOf('\nfn ', metadataStartIndex + 'fn safe_error_metadata('.length);

    assert.notEqual(metadataStartIndex, -1, '必须保留桌面错误白名单函数');
    assert.notEqual(metadataEndIndex, -1, '必须能够隔离 safe_error_metadata 函数边界');
    const metadataSource = desktopErrorSource.slice(metadataStartIndex, metadataEndIndex);
    assert.match(
        metadataSource,
        /['"]CODEX_CDP_COMPOSER_NOT_EMPTY['"]\s*=>\s*\(\s*['"]CODEX_CDP_COMPOSER_NOT_EMPTY['"]\s*,\s*['"]Codex 输入框存在未发送草稿，请处理后重试。['"]\s*,?\s*\)/,
        'desktop_error 白名单必须原样保留草稿错误码和固定安全摘要'
    );
});
