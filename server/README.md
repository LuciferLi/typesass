# CodexMan AI API

仅监听本机回环地址的 FastAPI sidecar，当前 v1 只开放：

- `GET /health`
- `POST /v1/access-tokens/request`
- `POST /v1/access-tokens`
- `GET /v1/access-tokens`
- `POST /v1/access-tokens/{tokenId}/revoke`
- `GET /v1/models`
- `POST /v1/audio/transcriptions`
- `POST /v1/text/process`
- `GET /v1/codex/connection`
- `POST /v1/codex/connection/restart`
- `GET /v1/codex/workspaces`
- `POST /v1/codex/threads/search`
- `POST /v1/codex/threads/{threadId}/open`
- `POST /v1/task-workspace/query`
- `POST /v1/projects`
- `POST /v1/projects/{projectId}/update`
- `POST /v1/projects/{projectId}/delete`
- `POST /v1/tasks`
- `POST /v1/tasks/{taskId}/queue`
- `POST /v1/tasks/{taskId}/complete`

桌面端、本机 WebView 和任意浏览器页面都固定调用 `http://127.0.0.1:18080`。本项目不支持远程服务器、公网反向代理或可配置域名拓扑。上游 API Key、URL 和模型名只通过受信 sidecar stdin bootstrap 进入运行内存，客户端只能提交 opaque `modelId`，不能覆盖私有连接参数。

HTTP 服务不按浏览器 `Origin` 做 CORS 拦截，CORS 预检允许任意来源、方法和请求头。Chrome/Edge 等浏览器可能在公网 HTTPS 页面首次访问 `127.0.0.1` 时弹出本地网络访问授权；若浏览器在请求到达 sidecar 前拦截，服务端不会产生 requestId 或访问日志。`/health` 和授权码申请接口无需授权码；业务接口按 `Origin` 判定来源，内网来源可免授权码，公网来源必须携带 `Authorization: Bearer <App 授权码>`，缺失 `Origin` 的业务请求直接拒绝。

## 鉴权流程

1. App 系统设置维护明文授权码，支持创建、查询和撤销；授权码可长期查看，不做只展示一次。
2. `/health` 和 `POST /v1/access-tokens/request` 不需要授权码；申请接口会通知桌面 App 弹出“是否确认授权”弹窗，用户确认后才创建并返回授权码，拒绝或超时不会创建授权码。
3. 业务接口必须带 `Origin`。`localhost`、`127.0.0.1`、`::1`、私有网段 IP 和 `tauri.localhost` 视为内网来源，可免授权码；其它公网 IP 或域名必须携带 `Authorization: Bearer <App 授权码>`。
4. 授权码不做 scope、refresh token、短期 session token 或 Basic 换 token。撤销或过期后再次访问业务接口统一返回 `401 UNAUTHORIZED`。
5. 开发环境可启用固定授权码，使用 `AITOOL_ENABLE_DEV_BEARER_TOKEN=1` 和 `AITOOL_DEV_ACCESS_TOKEN=<至少 32 字符>`；该固定授权码不写入授权码列表，生产环境不得启用。

示例：

```bash
curl -X POST 'http://127.0.0.1:18080/v1/access-tokens/request' \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-auth-001' \
  -d '{"name":"Chrome 插件","expiresAt":null}'
```

执行该命令后，CodexMan 桌面 App 会弹出确认授权窗口；只有点击“确认授权”才会生成授权码。

成功返回：

```json
{
  "status": "approved",
  "accessToken": "typesass_xxx",
  "expiresAt": null
}
```

公网来源访问业务接口：

```bash
curl 'http://127.0.0.1:18080/v1/models' \
  -H 'Origin: https://public.example' \
  -H 'Authorization: Bearer typesass_xxx' \
  -H 'X-Request-ID: models-001'
```

- 缺失 `Origin` 的业务请求返回 `401 ORIGIN_REQUIRED`。
- 公网来源缺失、错误、过期或已撤销授权码返回 `401 UNAUTHORIZED`。
- 授权码校验成功会更新列表中的 `lastUsedAt`。

## 本地启动

本地直接启动要求 Python 3.9；正式 Sidecar 构建固定使用 CPython 3.9 和带哈希依赖锁。

```bash
cd aiTool/server
python3.9 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt

export AITOOL_ACCESS_TOKEN_DATABASE_FILE='data/aitool-access-tokens.sqlite3'
export AITOOL_QUOTA_DATABASE_FILE='data/aitool-quota.sqlite3'
# 可选：开发环境固定授权码，生产环境不要启用。
export AITOOL_ENABLE_DEV_BEARER_TOKEN=1
export AITOOL_DEV_ACCESS_TOKEN='typesass-dev-access-token-000000000001'

uvicorn app.main:app --host 127.0.0.1 --port 18080 --workers 1
```

直接 Uvicorn 启动不具备桌面 App 注入的运行配置，因此模型目录为空、健康检查和鉴权可用，会话/任务接口返回 `503 PRIVATE_SERVICE_UNAVAILABLE`。生产桌面模式必须由 App 启动 sidecar；第三方只使用公开 HTTP 地址和 App 授权码，不需要也不能配置内部业务桥接。

## 会话与任务 HTTP 流程

会话管理和任务管理的所有页面操作都必须调用上述公共 HTTP 路由。FastAPI 是唯一公开入口，负责 Origin 与授权码鉴权、严格 DTO、requestId、CORS、统一错误 envelope 和 OpenAPI；它不会打开任务 SQLite、不会启动 CodeX、不会复制任务状态机。鉴权通过后，HTTP 服务把请求交给桌面业务核心，由桌面业务核心继续负责路径校验、事务、CAS 状态转换、任务调度和系统打开动作；第三方不得绕过 HTTP 入口直接访问内部实现。

桌面 App 内部业务服务不属于公开接入面。第三方不得探测或依赖其地址、凭据、帧格式、方法名、并发拓扑或生命周期；这些实现会随桌面版本变化。公开稳定契约只有本 README 和 OpenAPI 中的 HTTP 路由、响应 schema、错误码、requestId 与重试语义。

第三方接入流程为：先取得 App 授权码，再携带 `Origin`、`Authorization: Bearer <授权码>` 和每次尝试唯一的 `X-Request-ID` 调用会话/任务接口。每条接口的 OpenAPI `x-error-codes` 会列出公开稳定错误码、HTTP 状态、是否可重试和处理动作；不要只按 HTTP 状态猜测业务原因。字段/schema 错误返回 `422 VALIDATION_ERROR`。显式查询未知项目和打开未知会话分别返回 `TASK_PROJECT_NOT_FOUND`、`CODEX_THREAD_NOT_FOUND`；update/delete/queue/complete 的未知 ID 当前由 Rust 收敛为各操作固定 409 错误码，不会回退操作其它资源。内部协议、鉴权、方法或序列化故障统一返回 `502 PRIVATE_SERVICE_PROTOCOL_ERROR`，不会透出内部错误码或响应正文；未就绪或 `RPC_BUSY` 过载返回 503，超时返回 504。

任务接口的 `projectId/taskId/threadId` 只允许 1 到 128 字符的字母、数字、点、下划线、冒号和短横线；项目名最多 100 个 Unicode 字符，任务标题最多 200 个 Unicode 字符，提示词最多 50000 个 Unicode 字符。CodeX 会话搜索每页 1 到 60 条，offset 不得为负且最大 1000000，keyword 最多 200 字符。项目路径和工作空间路径最大 4096 字符，HTTP 的长度校验不替代 Rust 的绝对路径、存在性、普通文件/目录和访问边界校验。

以下连接管理和完整业务流程示例统一使用：

```bash
BASE_URL='http://127.0.0.1:18080'
TOKEN='<APP_ACCESS_TOKEN>'
```

### CodeX Desktop 连接与显式重启

`GET /v1/codex/connection` 是唯一公开连接探针。它返回 200 和当前脱敏快照；`connected=false` 是正常、可判定的业务状态，不是空成功，也不应被改写成 HTTP 故障。响应只包含：

```json
{
  "state": "disconnected",
  "connected": false,
  "desktopRunning": true,
  "canRestart": true,
  "reasonCode": "CODEX_CDP_NOT_READY",
  "message": "Codex 已启动，但任务页面尚未准备完成。",
  "checkedAt": "1786406400000"
}
```

- `state` 固定为 `connected`、`disconnected`、`restarting`、`blocked` 或 `unsupported`。调用方按枚举驱动交互，不解析 `message` 猜状态。
- `connected=true` 才表示可以新建并发送任务；`desktopRunning=true` 单独出现不等于连接可用。
- `canRestart=true` 只表示当前状态允许展示“重启连接”操作，不能据此自动调用重启接口。
- `reasonCode` 是稳定原因码，例如 `CODEX_CONNECTED`、`CODEX_DESKTOP_NOT_RUNNING`、`CODEX_CDP_NOT_ENABLED`、`CODEX_CDP_NOT_READY`、`CODEX_CDP_TARGET_AMBIGUOUS`、`CODEX_CDP_PORT_IN_USE`、`CODEX_RESTART_IN_PROGRESS`、`CODEX_RESTART_FAILED`、`CODEX_RESTART_TIMEOUT`、`CODEX_PLATFORM_UNSUPPORTED`。第三方只展示安全 `message` 和保存原因码/requestId，不得尝试取得或展示内部连接细节。
- `checkedAt` 是本次真实探针完成时的十进制 Unix epoch 毫秒字符串，不是缓存过期时间。

只有用户看见当前连接状态并明确确认后，客户端才能调用 `POST /v1/codex/connection/restart`。该接口没有请求正文，成功固定返回 HTTP 202：

```bash
curl "$BASE_URL/v1/codex/connection" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-codex-connection-001'

# 仅在 canRestart=true 且用户明确确认后执行；已连接时也会真正重启。
curl -X POST "$BASE_URL/v1/codex/connection/restart" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-codex-restart-001'
# HTTP 202: {"accepted":true,"state":"restarting"}
```

202 只表示请求通过门禁并已被接受，固定返回 `{"accepted":true,"state":"restarting"}`，不表示 CodeX 已重启完成。明确重启不做“已连接就直接返回”的幂等处理：Rust 会在任何副作用前固化受信主进程和固定 CDP listener/helper 的出生时间、真实可执行文件与严格签名身份，随后依次尝试正常退出、`SIGTERM` 和受限 `SIGKILL`。每次发送信号前都会重新核对同一份快照；只要出现新监听者、PID 复用、路径或签名变化就立即停止，绝不结束未知进程。旧监听全部消失且固定端口不可连接后才启动新实例。调用方应以低频率（建议每 1 至 2 秒一次）轮询 `GET /v1/codex/connection`，直到出现以下任一结果，并设置有限的客户端等待期限：

- `connected=true/state=connected`：连接恢复，可以由用户继续 queue。
- `state=blocked`：停止轮询，展示 `reasonCode/message` 和最近 requestId，等待人工处理。
- `state=unsupported`：停止轮询，当前平台不支持该操作。
- 达到客户端等待期限仍未连接：停止轮询并保留当前状态；不得自动再次 POST restart。

下面示例使用 90 秒客户端等待期限；这是示例调用方的有限等待策略，不改变服务端状态。每次 GET 使用新的 requestId，且任何非 `restarting/disconnected` 状态都会停止循环交给人工判断：

```bash
CONNECTION_DEADLINE="$(( $(date +%s) + 90 ))"
while [ "$(date +%s)" -lt "$CONNECTION_DEADLINE" ]; do
  CONNECTION_RESPONSE="$(curl -fsS "$BASE_URL/v1/codex/connection" \
    -H "Authorization: Bearer $TOKEN" \
    -H "X-Request-ID: partner-codex-restart-poll-$(date +%s)")"
  CONNECTED="$(printf '%s' "$CONNECTION_RESPONSE" | jq -r '.connected')"
  STATE="$(printf '%s' "$CONNECTION_RESPONSE" | jq -r '.state')"
  [ "$CONNECTED" = 'true' ] && [ "$STATE" = 'connected' ] && break
  case "$STATE" in
    restarting|disconnected) sleep 2 ;;
    *) printf '%s\n' "$CONNECTION_RESPONSE"; exit 1 ;;
  esac
done
[ "$CONNECTED" = 'true' ] && [ "$STATE" = 'connected' ] \
  || { echo 'Codex connection wait timeout'; exit 1; }
```

重启是有副作用的操作。POST 返回 202 前若连接中断或调用方未收到响应，不得自动重放；先查询连接状态，只有 `canRestart=true` 且用户再次明确确认时，才允许产生新的重启请求。已连接状态也可人工重启，但仍不得自动调用。稳定错误处理如下：

用户确认文案必须说明：正常退出失败时服务端可能强制结束已验证的官方 CodeX 旧进程，未发送草稿和尚未完成的手工任务可能丢失。任务数据库中存在 `running` 任务时接口会在任何进程副作用前拒绝；这只能保护 CodeXMan 管理的任务，无法推断用户在 CodeX 窗口中手工启动的工作。

- `409 CODEX_RESTART_IN_PROGRESS`：已有重启请求在执行，停止重复提交并轮询连接状态。
- `409 CODEX_RESTART_TASK_ACTIVE`：仍有任务执行，等待任务进入终态；之后必须重新展示确认，不得自动重启。
- `409 CODEX_CDP_PORT_IN_USE`：本机连接资源冲突，停止重启并提示人工处理；不得结束、替换或探测未知进程。
- `409 CODEX_RESTART_FAILED`：本次请求未被安全接受或无法启动流程；保留 requestId，重新查询状态，再由用户决定后续操作。
- `500 CODEX_CONNECTION_STATE_FAILED`：连接状态不可判定，禁止假报接受或自动重试，携带 requestId 排查日志。
- `501 CODEX_PLATFORM_UNSUPPORTED`：当前平台不支持重启，永久停止该操作。

### 第三方 curl 完整流程

下面示例假设已经完成前述授权，并继续使用上一节定义的 `BASE_URL/TOKEN`。每个命令使用不同的 `X-Request-ID`，便于服务方根据用户提供的信息定位对应日志。

1. 搜索工作空间、搜索会话并请求桌面打开。工作空间和搜索是只读操作，遇到 `RPC_BUSY`、`PRIVATE_SERVICE_UNAVAILABLE` 或 `PRIVATE_SERVICE_TIMEOUT` 可指数退避后重试；`CODEX_UNAVAILABLE` 需要先修复本机 CodeX 环境。打开成功的 `{"ok":true}` 只表示 Rust 已验证 thread 存在并向操作系统提交打开请求，不保证 CodeX UI 已完成页面切换。

```bash
curl "$BASE_URL/v1/codex/workspaces" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-workspaces-001'

curl -X POST "$BASE_URL/v1/codex/threads/search" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-thread-search-001' \
  -d '{"workspaceCwd":"/Users/demo/Documents/project-a","limit":20,"offset":0,"keyword":"接口文档"}'

curl -X POST "$BASE_URL/v1/codex/threads/0198f25a-1111-7000-8000-000000000001/open" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-thread-open-001'
```

2. 读取任务聚合、创建项目并更新项目。省略 `projectId` 时读取项目列表和默认项目上下文；显式传入未知 ID 时返回 `TASK_PROJECT_NOT_FOUND`，绝不静默回退到其它项目。以下使用 `jq` 从创建响应提取本次项目 ID；生产客户端也应使用响应 ID，禁止自行生成。

```bash
curl -X POST "$BASE_URL/v1/task-workspace/query" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-workspace-query-001' \
  -d '{}'

PROJECT_RESPONSE="$(curl -fsS -X POST "$BASE_URL/v1/projects" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-project-create-001' \
  -d '{"name":"AI 工具接口接入","workspacePath":"/Users/demo/Documents/project-a"}')"
PROJECT_ID="$(printf '%s' "$PROJECT_RESPONSE" | jq -r '.projects[] | select(.name == "AI 工具接口接入") | .id')"

curl -X POST "$BASE_URL/v1/projects/$PROJECT_ID/update" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-project-update-001' \
  -d '{"name":"AI 工具接口接入 v2","workspacePath":"/Users/demo/Documents/project-a-v2"}'
```

3. 创建任务、确认连接、进入队列并轮询。创建只生成 `created` 任务，不预建假会话；queue 会再次在服务端强制检查连接、任务状态、发送不确定状态和项目 session 容量。调用方应每次重新读取响应/聚合中的真实状态，不得在前端自行推断 `queued`、`running` 或 `waiting_acceptance`。

```bash
TASK_RESPONSE="$(curl -fsS -X POST "$BASE_URL/v1/tasks" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-task-create-001' \
  -d "{\"projectId\":\"$PROJECT_ID\",\"title\":\"完善 HTTP 接口文档\",\"prompt\":\"检查现有接口契约，补齐请求示例、响应示例和稳定错误码。\"}")"
TASK_ID="$(printf '%s' "$TASK_RESPONSE" | jq -er '.createdTaskId')"

# queue 前先读取真实连接状态；未连接时停止，并按上一节让用户决定是否重启。
CONNECTION_RESPONSE="$(curl -fsS "$BASE_URL/v1/codex/connection" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-codex-connection-before-queue-001')"
printf '%s' "$CONNECTION_RESPONSE" | jq -e '.connected == true and .state == "connected"' >/dev/null \
  || { printf '%s\n' "$CONNECTION_RESPONSE"; exit 1; }

curl -X POST "$BASE_URL/v1/tasks/$TASK_ID/queue" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-task-queue-001'

POLL_DEADLINE="$(( $(date +%s) + 43200 ))"
while [ "$(date +%s)" -lt "$POLL_DEADLINE" ]; do
  TASK_STATE="$(curl -fsS -X POST "$BASE_URL/v1/task-workspace/query" \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Content-Type: application/json' \
    -H "X-Request-ID: partner-task-poll-$(date +%s)" \
    -d "{\"projectId\":\"$PROJECT_ID\"}" \
    | jq -c --arg taskId "$TASK_ID" '.tasks[] | select(.id == $taskId)')"
  STATUS="$(printf '%s' "$TASK_STATE" | jq -r '.status')"
  [ "$STATUS" = 'waiting_acceptance' ] && break
  if [ "$STATUS" = 'failed' ] || [ "$STATUS" = 'completed' ] || [ -z "$STATUS" ]; then
    printf '%s\n' "$TASK_STATE"
    exit 1
  fi
  sleep 5
done
[ "$STATUS" = 'waiting_acceptance' ] || { echo 'poll timeout'; exit 1; }
```

轮询可能观察到 `queued -> running -> waiting_acceptance`；执行失败则进入终态 `failed`，此时先展示 `lastError` 和 requestId，再判断是否允许用户采取后续操作，绝不能看到 `failed` 就自动 queue。queue 的状态检查和状态变更在同一个写事务内完成；状态不允许时，任何任务状态/事件/session 写入前同步返回 `409 TASK_STATE_CONFLICT`、`retryable=false`，任务保持原来的 `created` 或 `failed` 状态。只有 `waiting_acceptance` 允许调用 complete；`created/queued/running/completed/failed` 调用 complete 都会失败，不得通过重试绕过状态机。

queue 的两个连接相关错误必须单独处理：

- `503 CODEX_DESKTOP_NOT_CONNECTED`、`retryable=false`：服务端在任何任务或事件写入前拒绝，不能把相同 queue 请求自动重试或重放 prompt。先调用连接查询；如允许重启，必须由用户明确确认，收到 202 后持续轮询至 `connected=true`，最后再由用户发起一次新的 queue 操作。
- `409 CODEX_SEND_UNCERTAIN`、`retryable=false`：此前 prompt 可能已经发送成功，只是本地无法确定结果。任务会保持不可安全重排的 failed 状态；禁止自动重试、重新 queue、复制 prompt 创建新任务或以刷新页面规避。应保留任务 ID、requestId 和 `lastError`，让用户在 CodeX 中人工核对后处理。

4. 人工验收完成任务。响应中的 `resultJson` 是 JSON 字符串，不是已展开对象：浏览器需要再执行一次 `JSON.parse(task.resultJson)`。内层 UTF-8 最大 32 KiB；尚无终态结果或执行失败时为字符串 `"{}"`。二次解析失败代表持久化记录违反契约，客户端不得显示“完成成功”，应保留原始字段和本次 requestId 交给服务方排查。

```bash
curl -X POST "$BASE_URL/v1/tasks/$TASK_ID/complete" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-task-complete-001'
```

5. 删除项目为软删除，只在本机数据库中标记项目已删除；已有任务或会话历史不会级联删除，也不会阻止删除。删除后该项目不再出现在当前项目列表中，不能继续作为新建任务目标。

```bash
EMPTY_PROJECT_RESPONSE="$(curl -fsS -X POST "$BASE_URL/v1/projects" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'X-Request-ID: partner-empty-project-create-001' \
  -d '{"name":"待删除项目","workspacePath":"/Users/demo/Documents/deleted-project"}')"
EMPTY_PROJECT_ID="$(printf '%s' "$EMPTY_PROJECT_RESPONSE" | jq -r '.projects[] | select(.name == "待删除项目") | .id')"

curl -X POST "$BASE_URL/v1/projects/$EMPTY_PROJECT_ID/delete" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Request-ID: partner-project-delete-001'
```

### 重试、未知 ID 与容量边界

- 只读 `GET /v1/codex/workspaces`、会话搜索和任务聚合查询可在 `retryable=true` 时指数退避重试。建议从 500 ms 开始并加入随机抖动，最多重试 2 次；仍失败时向用户展示 `error.code` 和 `error.requestId`。
- 创建项目、更新项目、删除项目、创建任务、queue、complete 都不是幂等接口，HTTP 服务不会替调用方自动重试。写操作遇到 503/504 或连接中断时，先用聚合查询核对项目/任务真实状态，再决定是否发起新的用户操作，禁止原样盲目重放。
- `RPC_BUSY` 表示本机业务服务过载，返回 503 且可退避；它不是业务状态冲突。聚合业务 JSON 的公开容量预算为 7 MiB。
- 显式未知 `projectId/taskId/threadId` 从不回退到默认或首条资源。项目和任务操作先刷新 `/v1/task-workspace/query`；CodeX 操作先刷新工作空间和 thread 搜索结果。
- 最多创建并返回 200 个项目；每个项目最多保留 16 个任务，session 历史不再按条数设上限，但仍受聚合业务 JSON 7 MiB 预算保护。第 17 个任务由创建接口同步返回 `TASK_PROJECT_TASK_LIMIT_REACHED`，不会静默截断成看似成功。项目总量达到 200 时创建返回 `TASK_PROJECT_LIMIT_REACHED`。
- CodeX thread 搜索每页最多 60 条；会话扫描最多检查最近 180 个文件，索引最多 500 项，目录枚举最多 1024 个目录/500 个文件，精确查找最多 4096 个条目。单会话文件最大 64 MiB、单次总扫描最大 512 MiB、会话索引最大 8 MiB；触发容量错误时不会返回伪造空结果。
- v1 没有取消任务、SSE、会话详情、删除历史任务或批量操作 API。第三方不得在客户端伪造这些入口，也不得通过 Tauri command、私有 socket、SQLite 文件或客户端直连 CodeX 绕过 HTTP 服务。

前端固定连接该本机端口，不读取 IP 或域名环境变量。端口被占用时 Uvicorn 会直接启动失败；由用户人工确认并处理冲突。应用不自动结束未知进程、不自动漂移端口，否则前端地址、OpenAPI `servers` 和监控目标会失配。

## 配置

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `AITOOL_ACCESS_TOKEN_DATABASE_FILE` | `data/aitool-access-tokens.sqlite3` | App 明文授权码 SQLite 存储文件；桌面升级时必须保留该文件 |
| `AITOOL_ENABLE_DEV_BEARER_TOKEN` | 关闭 | 开发环境固定授权码开关；生产环境禁止启用 |
| `AITOOL_DEV_ACCESS_TOKEN` | 无 | 开发环境固定授权码，至少 32 个 ASCII 字符；启用后不要求 Origin，也不会写入授权码列表 |
| `AITOOL_API_KEYS_JSON` | 无 | 旧 Basic 流程兼容配置；公开 HTTP 主链路不再需要 |
| `AITOOL_DEVICE_APPROVER_CLIENT_IDS` | 无 | 旧设备码流程兼容配置；公开 HTTP 主链路不再需要 |
| `AITOOL_TOKEN_SIGNING_KEY` | `unused-signing-key-for-removed-session-token` | 旧短 Token 兼容配置；公开 HTTP 主链路不再需要 |
| `AITOOL_CLIENT_RATE_LIMIT_PER_MINUTE` | `60` | 每 clientId 滚动一分钟业务请求上限 |
| `AITOOL_CLIENT_DAILY_QUOTA` | `10000` | 每 clientId 自然日业务请求上限 |
| `AITOOL_QUOTA_DATABASE_FILE` | `data/aitool-quota.sqlite3` | 持久化 UTC 日额度 SQLite；桌面升级时必须保留该文件 |
| sidecar stdin `modelCatalog` | 空目录 | Rust 一次性注入的模型数组；内部项固定为 `id/displayName/capability/enabled/isDefault/provider/baseUrl/modelName/apiKey`，禁止改用进程环境传密钥 |
| `AITOOL_SIDECAR_HOST` | `127.0.0.1` | PyInstaller 入口监听地址；仅接受 localhost 或回环 IP |
| `AITOOL_SIDECAR_PORT` | `18080` | PyInstaller 入口固定端口；范围 1024-65535，冲突时失败且不自动漂移 |
| `AITOOL_REQUEST_TIMEOUT_SECONDS` | `30` | 单次上游超时；网关读取超时建议 `65s` |
| `AITOOL_CONCURRENCY_LIMIT` | `8` | 单 worker 最大模型请求并发 |
| `AITOOL_CONCURRENCY_WAIT_SECONDS` | `1` | 等待全局并发额度秒数 |
| `AITOOL_LOG_FILE` | `logs/aitool-server.log` | 结构化轮转日志 |
| `AITOOL_LOG_MAX_BYTES` | `10485760` | 单文件轮转阈值 |
| `AITOOL_LOG_BACKUP_COUNT` | `5` | 轮转文件数量 |

服务公开地址固定为 `http://127.0.0.1:18080`，OpenAPI `servers` 使用该值。首发版不提供地址覆盖环境变量；端口被占用时直接返回可诊断错误，不自动漂移、不终止无关进程。

v1 公开契约固定为：请求体 12 MiB、base64 解码后音频 8 MiB、文本 20000 字符。这三个值不能通过环境变量覆盖，避免 OpenAPI 和运行时漂移。MIME 允许列表、音频解码上限、文本上限和词典单项 100 字符限制由 Service 返回稳定业务 code；字段缺失、类型错误、额外字段、非法 mode 和词典超过 100 项仍返回 `422 VALIDATION_ERROR`。

## 模型目录与请求契约

`GET /v1/models` 遵循统一 Origin 与授权码鉴权，返回数组项只包含：

```json
{
  "id": "model_01J00000000000000000000000",
  "displayName": "本地语音模型",
  "capability": "asr",
  "enabled": true,
  "isDefault": true
}
```

公开响应绝不包含 `provider`、`baseUrl`、`modelName` 或 `apiKey`。`capability` 是单值 `asr` 或 `text`，一个配置只对应一个已验证能力。音频和文本请求必须携带目录中的 `modelId`，成功响应通过 `modelId` 原样返回实际使用的目录项；服务不会根据默认项或上游响应静默切换模型。

```json
{
  "modelId": "model_01J00000000000000000000000",
  "audioBase64": "UklGRiQAAABXQVZFZm10IBAAAAABAAEA...",
  "contentType": "audio/wav",
  "language": "auto"
}
```

模型选择错误稳定映射为：空目录 `503 MODEL_NOT_CONFIGURED`、未知 ID `404 MODEL_NOT_FOUND`、已禁用 `409 MODEL_DISABLED`、能力不匹配 `409 MODEL_CAPABILITY_MISMATCH`。调用方应刷新目录或让用户修正配置，不应自动改用其它模型。

## 固定本机拓扑与浏览器访问

当前版本只支持单机、单 Uvicorn worker、固定 `127.0.0.1:18080`。分钟窗口和全局并发在单进程内执行，UTC 日额度用 SQLite `BEGIN IMMEDIATE` 持久化；桌面进程重启不会清零。禁止把本服务绑定公网、放到反向代理后或启动多个 worker；如未来需要远程服务或横向扩容，必须另行设计共享限流、额度、鉴权和 TLS 契约，不能复用当前部署说明。

浏览器来源参与业务接口鉴权判断。`/health` 和授权码申请可直接访问；模型、任务、Codex 状态等敏感接口按 Origin 区分内网免授权码与公网强制授权码。服务不使用 Cookie 鉴权，也不允许跨域凭据参与会话。

## 错误、重试与排障

- 所有错误统一返回 `error.code/message/requestId/retryable`，其中 `retryable` 由稳定错误码映射决定；CORS 预检、正文中间件 413、框架错误和兜底 500 也遵循该 envelope。所有路由 OpenAPI 都声明实际可达的 413 和 500。
- 调用方提供的 `X-Request-ID` 必须严格匹配 `^[A-Za-z0-9._-]{1,128}$`。缺失、空值、非 ASCII、超长或其它不匹配值不会返回 422，而会被替换为 32 字符 UUID4 hex；最终值同时出现在响应 Header 和错误 `requestId`。
- 幂等转换请求只有 `RATE_LIMIT`、`CONCURRENCY_LIMIT`、`UPSTREAM_UNAVAILABLE`、`QUOTA_STORE_UNAVAILABLE`、`UPSTREAM_TIMEOUT` 标记为可自动重试。单次业务调用最多执行初次请求加 2 次重试，即最多 3 次 HTTP 尝试；三次尝试共享 60 秒总时限，每次尝试使用新的 `X-Request-ID`。响应存在 `Retry-After` 时必须优先按该值等待；仅在缺失时使用第 1 次 1 秒、第 2 次 2 秒并加入随机抖动的退避。
- `DAILY_QUOTA_EXCEEDED` 不进入上述短退避重试；调用方必须停止本次业务调用，等待 `Retry-After` 指定的下一 UTC 自然日后再发起新的业务请求。
- `UPSTREAM_REJECTED`、`UPSTREAM_INVALID_RESPONSE`、`UPSTREAM_EMPTY_RESULT`、所有 400/401/404/409/413/422 和 `INTERNAL_ERROR` 禁止自动重试，应修正输入、配置或携带 requestId 排查。
- `AUTHORIZATION_PENDING`、`DEVICE_POLLING_TOO_FAST` 和 `DEVICE_AUTHORIZATION_CAPACITY` 的 `retryable=true` 只表示设备授权状态推进或容量等待，不计入转换请求的 2 次重试上限；设备轮询按 `interval/Retry-After` 持续到成功或 600 秒过期。
- 日额度按已通过鉴权并进入业务门禁的尝试扣减，包括后续 4xx/5xx；周期统一为 UTC 自然日，Retry-After 精确到下一 UTC 日。
- 日志记录 requestId、clientId、errorCode、路径、状态和耗时，但不记录 Authorization、长期 secret、模型 API Key、音频、正文、私有目录字段或完整上游响应。
- 桌面 App 核心链路错误写入 App data 的 `logs/desktop-errors.log`，每条包含稳定错误码和唯一诊断 ID；达到 2 MiB 时轮转为 `.1`，超过 14 天的备份会在后续写入时清理。用户反馈问题时应同时提供页面错误码、诊断 ID 和 HTTP requestId（如有）。
- 告警至少覆盖：5xx 比例、502/504、429 按 code/clientId 分布、p95/p99 时延、日志写入失败、进程重启和磁盘使用率。

OpenAPI 由 FastAPI 路由单源生成：启动后访问 `/docs` 或 `/openapi.json`。

## Sidecar 构建依赖

PyInstaller 入口固定为 `sidecar_main.py`，运行时必须从 Rust 管道接收一次性模型 bootstrap。干净构建机必须使用 CPython 3.9 创建独立虚拟环境，并按完整传递依赖锁与 SHA-256 安装；不要复用开发机全局 site-packages：

```bash
cd aiTool/server
python3.9 -m venv .venv-build
. .venv-build/bin/activate
pip install --require-hashes --only-binary=:all: --requirement requirements-build.lock
```

`requirements-build.txt` 是人工维护的顶层输入；`requirements-build.lock` 固定全部传递依赖及发行包哈希，并由构建脚本强制校验。更新任一 Python 依赖时必须使用 CPython 3.9 和 `pip-compile --generate-hashes --allow-unsafe --strip-extras requirements-build.txt -o requirements-build.lock` 重新生成并复测，不得手工删改哈希。实际打包命令与 Tauri externalBin 登记由仓库构建脚本负责，构建产物必须以 `sidecar_main.py` 为唯一 Python 启动入口。
