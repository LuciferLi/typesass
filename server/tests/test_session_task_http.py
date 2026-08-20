"""会话与任务公共 HTTP 网关测试。"""

import json
import re
from typing import Dict, List

import pytest

from app import main
from tests.test_main_api import api_client, assert_error, session_headers


EMPTY_WORKSPACE = {"projects": [], "tasks": [], "sessions": []}


def workspace_task(index: int) -> Dict[str, object]:
    """构造满足公开工作区响应契约的任务记录。

    流程：按序号生成稳定 ID、标题和时间字段，供响应模型容量回归测试复用。
    参数：index 为任务序号。
    返回：完整任务响应字典。
    异常边界：仅用于测试响应模型，不触发真实 SQLite 或 Codex 执行。
    """

    return {
        "id": "task-{0}".format(index),
        "projectId": "project-1",
        "title": "任务 {0}".format(index),
        "prompt": "执行 {0}".format(index),
        "attachments": [],
        "status": "created",
        "currentSessionId": "",
        "externalThreadId": "",
        "lastError": "",
        "resultJson": "{}",
        "createdAt": "2026-08-13 10:00:00",
        "updatedAt": "2026-08-13 10:00:00",
    }


class FakePrivateRpcClient:
    """记录公共路由到私有 RPC 的精确方法和参数映射。"""

    def __init__(self) -> None:
        """初始化空调用列表。"""

        self.calls: List[tuple] = []

    async def call(
        self, method: str, request_id: str, params: Dict[str, object]
    ) -> object:
        """记录调用并返回满足对应公开响应 schema 的固定结果。"""

        self.calls.append((method, request_id, params))
        if method == "getCodexConnection":
            return {
                "state": "connected",
                "connected": True,
                "desktopRunning": True,
                "canRestart": True,
                "reasonCode": "CODEX_CONNECTED",
                "message": "CodeX 已连接，可以创建并发送任务。",
                "checkedAt": "1786406400000",
                "port": 9333,
                "pid": 1234,
                "webSocketDebuggerUrl": "ws://127.0.0.1/private",
                "dom": "private-dom",
                "cwd": "/private/workspace",
            }
        if method == "restartCodex":
            return {"accepted": True, "state": "restarting", "pid": 1234}
        if method == "listCodexWorkspaces":
            return [
                {
                    "cwd": "/tmp/work",
                    "title": "work",
                    "threadCount": 1,
                    "updatedAt": "1786406400000",
                }
            ]
        if method == "listCodexThreads":
            return [{"id": "thread-1", "title": "会话", "updatedAt": "1786406400000"}]
        if method == "openCodexThread":
            return None
        if method == "listMyApps":
            return []
        if method == "allocateMyAppPort":
            return {"port": 18123}
        if method in {"createMyApp", "updateMyApp", "restartMyApp"}:
            return {
                "id": "app-1",
                "name": "应用",
                "logoDataUrl": "",
                "accessType": "local",
                "port": 18123,
                "remoteUrl": None,
                "localUrl": "http://127.0.0.1:18123",
                "lanUrl": "http://192.168.1.2:18123",
                "openUrl": "http://127.0.0.1:18123",
                "serviceStatus": "running",
                "serviceMessage": "服务已启动。",
                "createdAt": "2026-08-20T00:00:00Z",
                "updatedAt": "2026-08-20T00:00:00Z",
            }
        if method in {"deleteMyApp", "openMyApp"}:
            return {"ok": True}
        if method == "createTask":
            return {"createdTaskId": "task-created-1", **EMPTY_WORKSPACE}
        return EMPTY_WORKSPACE


class ManyTasksPrivateRpcClient:
    """返回超过旧 16 条上限的任务列表，回归工作区响应模型不应误拦真实历史数据。"""

    async def call(
        self, method: str, request_id: str, params: Dict[str, object]
    ) -> object:
        """返回 17 条任务的工作区聚合数据。"""

        return {
            "projects": [
                {
                    "id": "project-1",
                    "name": "项目",
                    "workspacePath": "/tmp/work",
                    "basePrompt": "",
                    "taskCount": 17,
                    "sessionCount": 0,
                    "createdAt": "2026-08-13 10:00:00",
                    "updatedAt": "2026-08-13 10:00:00",
                }
            ],
            "tasks": [workspace_task(index) for index in range(17)],
            "sessions": [
                {
                    "id": "session-1",
                    "projectId": "project-1",
                    "taskId": "task-1",
                    "provider": "codex",
                    "workspacePath": "/tmp/work",
                    "title": "会话",
                    "status": "running",
                    "externalThreadId": "thread-1",
                    "createdAt": "2026-08-13T10:00:00Z",
                    "updatedAt": "2026-08-13T18:00:00+08:00",
                }
            ],
        }


@pytest.mark.asyncio
async def test_tc_session_http_001_all_routes_require_bearer_and_map_rpc(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """全部会话/任务路由复用 Bearer，精确映射 Rust 方法并过滤连接探针私有字段。"""

    async with api_client() as client:
        unauthorized_responses = [
            await client.get("/v1/codex/connection"),
            await client.post("/v1/codex/connection/restart"),
            await client.get("/v1/codex/workspaces"),
            await client.get("/v1/my-apps"),
        ]
        for unauthorized in unauthorized_responses:
            assert_error(unauthorized, 401, "ORIGIN_REQUIRED")
        headers = await session_headers(client, "session-http-001")
        fake = FakePrivateRpcClient()
        main.app.state.private_rpc = fake
        requests = [
            ("GET", "/v1/codex/connection", None),
            ("POST", "/v1/codex/connection/restart", None),
            ("GET", "/v1/codex/workspaces", None),
            (
                "POST",
                "/v1/codex/threads/search",
                {"workspaceCwd": "/tmp/work", "limit": 20, "offset": 0, "keyword": ""},
            ),
            ("POST", "/v1/codex/threads/thread-1/open", None),
            ("POST", "/v1/task-workspace/query", {"projectId": "project-1"}),
            (
                "POST",
                "/v1/projects",
                {"name": "项目", "workspacePath": "/tmp/work", "basePrompt": "项目规则"},
            ),
            (
                "POST",
                "/v1/projects/project-1/update",
                {"name": "新项目", "workspacePath": "/tmp/new", "basePrompt": "新项目规则"},
            ),
            ("POST", "/v1/projects/project-1/delete", None),
            (
                "POST",
                "/v1/tasks",
                {"projectId": "project-1", "title": "任务", "prompt": "执行"},
            ),
            (
                "POST",
                "/v1/tasks/task-1/update",
                {"title": "新任务", "prompt": "新执行说明"},
            ),
            ("POST", "/v1/tasks/task-1/delete", None),
            ("POST", "/v1/tasks/task-1/queue", None),
            ("POST", "/v1/tasks/task-1/complete", None),
            ("GET", "/v1/my-apps", None),
            ("POST", "/v1/my-apps/allocate-port", None),
            (
                "POST",
                "/v1/my-apps",
                {"name": "应用", "logoDataUrl": "", "accessType": "remote", "remoteUrl": "https://example.com"},
            ),
            (
                "POST",
                "/v1/my-apps/app-1/update",
                {"name": "应用", "logoDataUrl": "", "accessType": "remote", "remoteUrl": "https://example.com"},
            ),
            ("POST", "/v1/my-apps/app-1/delete", None),
            ("POST", "/v1/my-apps/app-1/start", None),
            ("POST", "/v1/my-apps/app-1/open", {"target": "codexman"}),
        ]
        responses = []
        for method, path, payload in requests:
            responses.append(
                await client.request(method, path, headers=headers, json=payload)
            )
    assert [response.status_code for response in responses] == [
        200,
        202,
        *([200] * 19),
    ]
    assert [call[0] for call in fake.calls] == [
        "getCodexConnection",
        "restartCodex",
        "listCodexWorkspaces",
        "listCodexThreads",
        "openCodexThread",
        "loadWorkspaceData",
        "createProject",
        "updateProject",
        "deleteProject",
        "createTask",
        "updateTask",
        "deleteTask",
        "queueTask",
        "completeTask",
        "listMyApps",
        "allocateMyAppPort",
        "createMyApp",
        "updateMyApp",
        "deleteMyApp",
        "restartMyApp",
        "openMyApp",
    ]
    assert all(call[1] == "session-http-001" for call in fake.calls)
    assert fake.calls[3][2] == {
        "workspaceCwd": "/tmp/work",
        "limit": 20,
        "offset": 0,
        "keyword": "",
    }
    assert fake.calls[7][2] == {
        "name": "新项目",
        "workspacePath": "/tmp/new",
        "basePrompt": "新项目规则",
        "id": "project-1",
    }
    assert responses[0].json() == {
        "state": "connected",
        "connected": True,
        "desktopRunning": True,
        "canRestart": True,
        "reasonCode": "CODEX_CONNECTED",
        "message": "CodeX 已连接，可以创建并发送任务。",
        "checkedAt": "1786406400000",
    }
    assert responses[1].status_code == 202
    assert responses[1].json() == {"accepted": True, "state": "restarting"}
    assert responses[4].json() == {"ok": True}
    assert fake.calls[10][2] == {
        "title": "新任务",
        "prompt": "新执行说明",
        "attachments": [],
        "id": "task-1",
    }
    assert responses[9].json() == {"createdTaskId": "task-created-1", **EMPTY_WORKSPACE}
    assert all(
        private_value not in caplog.text
        for private_value in (
            "9333",
            "1234",
            "ws://127.0.0.1/private",
            "private-dom",
            "/private/workspace",
        )
    )


@pytest.mark.asyncio
async def test_tc_session_http_001a_workspace_allows_more_than_sixteen_tasks() -> None:
    """工作区读取允许返回超过 16 条历史任务，避免响应模型把真实项目误判为 500。"""

    async with api_client() as client:
        headers = await session_headers(client, "session-http-001a")
        main.app.state.private_rpc = ManyTasksPrivateRpcClient()
        response = await client.post(
            "/v1/task-workspace/query", headers=headers, json={}
        )
    assert response.status_code == 200
    data = response.json()
    assert len(data["tasks"]) == 17
    assert data["projects"][0]["taskCount"] == 17
    assert data["projects"][0]["createdAt"] == "2026-08-13T10:00:00Z"
    assert data["tasks"][0]["updatedAt"] == "2026-08-13T10:00:00Z"
    assert data["sessions"][0]["createdAt"] == "2026-08-13T10:00:00Z"
    assert data["sessions"][0]["updatedAt"] == "2026-08-13T10:00:00Z"


@pytest.mark.asyncio
async def test_tc_session_http_002_strict_dto_and_openapi() -> None:
    """请求拒绝额外字段、不安全 ID 和越界分页，OpenAPI 声明全部真实路由且不泄露私有配置。"""

    async with api_client() as client:
        headers = await session_headers(client)
        invalid_responses = [
            await client.post(
                "/v1/codex/threads/search",
                headers=headers,
                json={
                    "workspaceCwd": "/tmp/work",
                    "limit": 61,
                    "offset": 0,
                    "keyword": "",
                },
            ),
            await client.post(
                "/v1/tasks",
                headers=headers,
                json={
                    "projectId": "project-1",
                    "title": "任务",
                    "prompt": "执行",
                    "extra": True,
                },
            ),
            await client.post("/v1/tasks/bad%2Fid/queue", headers=headers),
            await client.post(
                "/v1/tasks/task-1/update",
                headers=headers,
                json={"title": "任务", "prompt": "执行", "extra": True},
            ),
        ]
        schema_response = await client.get("/openapi.json")
    assert invalid_responses[0].status_code == 422
    assert invalid_responses[1].status_code == 422
    assert invalid_responses[2].status_code in (404, 422)
    serialized_schema = schema_response.text
    assert "privateRpc" not in serialized_schema
    assert "socketPath" not in serialized_schema
    assert schema_response.json()["paths"]["/v1/tasks/{taskId}/queue"]["post"][
        "security"
    ] == [{"AppAccessToken": []}]


@pytest.mark.contract
@pytest.mark.asyncio
async def test_tc_session_http_003_route_specific_errors_and_examples() -> None:
    """验证公开路由的专属错误契约、示例与任务排队判定顺序。

    流程：读取真实 OpenAPI，逐路由核对错误码和示例；随后检查 queue 明确先做
    CODEX_SEND_UNCERTAIN 只读预检，再检查桌面连接，并验证两类错误分别固定为 409 和 503。
    边界：公开 schema 不得出现已废弃的内部进程探测错误码，避免调用方依赖私有实现细节。
    """

    async with api_client() as client:
        schema_response = await client.get("/openapi.json")
    schema = schema_response.json()
    route_contracts = {
        ("/v1/codex/connection", "get"): (
            "CODEX_CONNECTION_STATE_FAILED",
            None,
            {
                "state",
                "connected",
                "desktopRunning",
                "canRestart",
                "reasonCode",
                "message",
                "checkedAt",
            },
        ),
        ("/v1/codex/connection/restart", "post"): (
            "CODEX_RESTART_IN_PROGRESS",
            None,
            {"accepted", "state"},
        ),
        ("/v1/codex/workspaces", "get"): (
            "CODEX_UNAVAILABLE",
            None,
            {"cwd", "title", "threadCount", "updatedAt"},
        ),
        ("/v1/codex/threads/search", "post"): (
            "CODEX_UNAVAILABLE",
            {"workspaceCwd", "limit", "offset", "keyword"},
            {
                "id",
                "title",
                "parentThreadId",
                "depth",
                "agentNickname",
                "agentRole",
                "updatedAt",
            },
        ),
        ("/v1/codex/threads/{threadId}/open", "post"): (
            "CODEX_UNAVAILABLE",
            None,
            {"ok"},
        ),
        ("/v1/task-workspace/query", "post"): (
            "TASK_WORKSPACE_LOAD_FAILED",
            {"projectId"},
            None,
        ),
        ("/v1/projects", "post"): (
            "TASK_PROJECT_CREATE_FAILED",
            {"name", "workspacePath", "basePrompt"},
            None,
        ),
        ("/v1/projects/{projectId}/update", "post"): (
            "TASK_PROJECT_UPDATE_FAILED",
            {"name", "workspacePath", "basePrompt"},
            None,
        ),
        ("/v1/projects/{projectId}/delete", "post"): (
            "TASK_PROJECT_DELETE_FAILED",
            None,
            None,
        ),
        ("/v1/tasks", "post"): (
            "TASK_CREATE_FAILED",
            {"projectId", "title", "prompt", "attachments"},
            None,
        ),
        ("/v1/tasks/{taskId}/update", "post"): (
            "TASK_UPDATE_FAILED",
            {"title", "prompt", "attachments"},
            None,
        ),
        ("/v1/tasks/{taskId}/delete", "post"): ("TASK_DELETE_FAILED", None, None),
        ("/v1/tasks/{taskId}/queue", "post"): ("TASK_QUEUE_FAILED", None, None),
        ("/v1/tasks/{taskId}/complete", "post"): ("TASK_ACCEPTANCE_FAILED", None, None),
        ("/v1/my-apps", "get"): (
            "MY_APP_LIST_FAILED",
            None,
            {
                "id",
                "name",
                "logoDataUrl",
                "accessType",
                "port",
                "remoteUrl",
                "localUrl",
                "lanUrl",
                "openUrl",
                "serviceStatus",
                "serviceMessage",
                "createdAt",
                "updatedAt",
            },
        ),
        ("/v1/my-apps/allocate-port", "post"): ("MY_APP_PORT_ALLOCATE_FAILED", None, {"port"}),
        ("/v1/my-apps", "post"): (
            "MY_APP_CREATE_FAILED",
            {"name", "logoDataUrl", "accessType", "port", "remoteUrl", "zipDataUrl"},
            None,
        ),
        ("/v1/my-apps/{appId}/update", "post"): (
            "MY_APP_UPDATE_FAILED",
            {"name", "logoDataUrl", "accessType", "port", "remoteUrl", "zipDataUrl"},
            None,
        ),
        ("/v1/my-apps/{appId}/delete", "post"): ("MY_APP_NOT_FOUND", None, {"ok"}),
        ("/v1/my-apps/{appId}/start", "post"): ("MY_APP_RESTART_FAILED", None, None),
        ("/v1/my-apps/{appId}/open", "post"): ("MY_APP_OPEN_FAILED", {"target"}, {"ok"}),
    }
    forbidden_generic_codes = {
        "INVALID_REQUEST",
        "RESOURCE_NOT_FOUND",
        "STATE_CONFLICT",
    }
    forbidden_internal_codes = {
        "RPC_INTERNAL_ERROR",
        "RPC_INVALID_PARAMS",
        "RPC_INVALID_REQUEST",
        "RPC_METHOD_NOT_ALLOWED",
        "RPC_REQUEST_TOO_LARGE",
        "RPC_RESPONSE_TOO_LARGE",
        "RPC_SERIALIZATION_FAILED",
        "RPC_UNAUTHORIZED",
    }
    workspace_item_fields = {
        "id",
        "name",
        "workspacePath",
        "basePrompt",
        "taskCount",
        "sessionCount",
        "createdAt",
        "updatedAt",
    }
    task_item_fields = {
        "id",
        "projectId",
        "title",
        "prompt",
        "attachments",
        "status",
        "currentSessionId",
        "externalThreadId",
        "lastError",
        "resultJson",
        "createdAt",
        "updatedAt",
    }
    session_item_fields = {
        "id",
        "projectId",
        "taskId",
        "provider",
        "workspacePath",
        "title",
        "status",
        "externalThreadId",
        "createdAt",
        "updatedAt",
    }
    my_app_item_fields = {
        "id",
        "name",
        "logoDataUrl",
        "accessType",
        "port",
        "remoteUrl",
        "localUrl",
        "lanUrl",
        "openUrl",
        "serviceStatus",
        "serviceMessage",
        "createdAt",
        "updatedAt",
    }

    for (path, method), (
        endpoint_code,
        request_fields,
        list_item_fields,
    ) in route_contracts.items():
        operation = schema["paths"][path][method]
        documented_codes = {
            entry["code"]
            for entries in operation["x-error-codes"].values()
            for entry in entries
        }
        assert endpoint_code in documented_codes
        assert "RPC_BUSY" in documented_codes
        assert documented_codes.isdisjoint(forbidden_internal_codes)
        assert documented_codes.isdisjoint(forbidden_generic_codes)
        if request_fields is not None:
            request_example = operation["requestBody"]["content"]["application/json"][
                "example"
            ]
            assert set(request_example) == request_fields
        success_status = "202" if path == "/v1/codex/connection/restart" else "200"
        success_example = operation["responses"][success_status]["content"][
            "application/json"
        ]["example"]
        if list_item_fields is not None:
            if path.endswith("/open") or path in {
                "/v1/codex/connection",
                "/v1/codex/connection/restart",
                "/v1/my-apps/allocate-port",
                "/v1/my-apps/{appId}/delete",
            }:
                assert set(success_example) == list_item_fields
            else:
                assert success_example and set(success_example[0]) == list_item_fields
            continue
        if (path, method) in {
            ("/v1/my-apps", "post"),
            ("/v1/my-apps/{appId}/update", "post"),
            ("/v1/my-apps/{appId}/start", "post"),
        }:
            assert set(success_example) == my_app_item_fields
            continue
        if path == "/v1/my-apps/allocate-port":
            assert set(success_example) == {"port"}
            continue
        if path in {"/v1/my-apps/{appId}/delete", "/v1/my-apps/{appId}/open"}:
            assert set(success_example) == {"ok"}
            continue
        expected_root_fields = {"projects", "tasks", "sessions"}
        if path == "/v1/tasks":
            expected_root_fields.add("createdTaskId")
            assert success_example["createdTaskId"] == success_example["tasks"][0]["id"]
        assert set(success_example) == expected_root_fields
        assert success_example["projects"] and set(success_example["projects"][0]) == workspace_item_fields
        assert success_example["tasks"] and set(success_example["tasks"][0]) == task_item_fields
        assert success_example["sessions"] and set(success_example["sessions"][0]) == session_item_fields

    public_contract = json.dumps(
        [schema["paths"][path][method] for path, method in route_contracts],
        ensure_ascii=False,
    ).lower()
    assert all(
        secret_name not in public_contract
        for secret_name in ('"socketpath"', '"secret"', "privaterpc")
    )
    connection_contract = json.dumps(
        [
            schema["paths"]["/v1/codex/connection"]["get"],
            schema["paths"]["/v1/codex/connection/restart"]["post"],
        ],
        ensure_ascii=False,
    )
    assert all(
        private_field not in connection_contract
        for private_field in (
            '"port"',
            '"pid"',
            '"webSocketDebuggerUrl"',
            '"dom"',
            '"cwd"',
        )
    )


@pytest.mark.contract
@pytest.mark.asyncio
async def test_tc_session_http_004_models_document_wire_formats() -> None:
    """相关模型逐字段描述并举例，准确声明两类时间格式、resultJson 二次解析和打开动作语义。"""

    async with api_client() as client:
        schema_response = await client.get("/openapi.json")
    schema = schema_response.json()
    components = schema["components"]["schemas"]
    documented_models = (
        "CodexThreadSearchRequest",
        "CodexConnectionResponse",
        "CodexRestartAcceptedResponse",
        "WorkspaceQueryRequest",
        "ProjectWriteRequest",
        "TaskCreateRequest",
        "TaskUpdateRequest",
        "TaskCreateResponse",
        "CodexWorkspaceResponse",
        "CodexThreadResponse",
        "ProjectResponse",
        "TaskResponse",
        "SessionResponse",
        "OperationResponse",
    )
    for model_name in documented_models:
        for field in components[model_name]["properties"].values():
            assert field["description"]
            assert field.get("examples")

    assert components["ProjectWriteRequest"]["properties"]["name"]["maxLength"] == 100
    assert components["ProjectWriteRequest"]["properties"]["basePrompt"]["maxLength"] == 20_000
    connection = components["CodexConnectionResponse"]
    assert set(connection["required"]) == {
        "state",
        "connected",
        "desktopRunning",
        "canRestart",
        "reasonCode",
        "message",
        "checkedAt",
    }
    assert set(connection["properties"]["state"]["enum"]) == {
        "connected",
        "disconnected",
        "restarting",
        "blocked",
        "unsupported",
    }
    assert connection["properties"]["checkedAt"]["pattern"] == "^[0-9]+$"
    restart = schema["paths"]["/v1/codex/connection/restart"]["post"]
    assert "200" not in restart["responses"]
    assert restart["responses"]["202"]["content"]["application/json"]["example"] == {
        "accepted": True,
        "state": "restarting",
    }
    assert set(
        components["CodexRestartAcceptedResponse"]["properties"]["state"]["enum"]
    ) == {
        "connected",
        "restarting",
    }
    task_create_response = components["TaskCreateResponse"]
    assert set(task_create_response["required"]) == {
        "createdTaskId",
        "projects",
        "tasks",
        "sessions",
    }
    assert (
        "唯一任务稳定 ID"
        in task_create_response["properties"]["createdTaskId"]["description"]
    )
    for model_name in ("CodexWorkspaceResponse", "CodexThreadResponse"):
        updated_at = components[model_name]["properties"]["updatedAt"]
        assert updated_at["pattern"] == "^$|^[0-9]+$"
        assert (
            "Unix epoch" in updated_at["description"]
            and "毫秒" in updated_at["description"]
        )
        assert re.fullmatch(r"[0-9]{13}", updated_at["examples"][0])
    for model_name in ("ProjectResponse", "TaskResponse", "SessionResponse"):
        for field_name in ("createdAt", "updatedAt"):
            timestamp = components[model_name]["properties"][field_name]
            assert "ISO 8601 UTC" in timestamp["description"]
            assert "Z" in timestamp["description"]
            assert re.fullmatch(
                r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z",
                timestamp["examples"][0],
            )

    result_json = components["TaskResponse"]["properties"]["resultJson"]
    assert "双层编码" in result_json["description"]
    assert "JSON.parse" in result_json["description"]
    assert "二次解析失败" in result_json["description"]
    assert isinstance(json.loads(result_json["examples"][0]), dict)
    task_status_description = components["TaskResponse"]["properties"]["status"][
        "description"
    ]
    assert "externalStatus=sendUncertain" in task_status_description
    assert "CODEX_SEND_UNCERTAIN" in task_status_description
    queue_operation = schema["paths"]["/v1/tasks/{taskId}/queue"]["post"]
    queue_errors_by_status = queue_operation["x-error-codes"]
    queue_error_codes = {
        item["code"]
        for items in queue_errors_by_status.values()
        for item in items
    }
    assert {
        "CODEX_SEND_UNCERTAIN",
        "CODEX_DESKTOP_NOT_CONNECTED",
    } <= queue_error_codes
    send_uncertain = next(
        item
        for item in queue_errors_by_status["409"]
        if item["code"] == "CODEX_SEND_UNCERTAIN"
    )
    desktop_disconnected = next(
        item
        for item in queue_errors_by_status["503"]
        if item["code"] == "CODEX_DESKTOP_NOT_CONNECTED"
    )
    assert "只读预检优先于 Codex Desktop 连接检查" in send_uncertain["action"]
    assert "即使当前断连也稳定返回 409" in send_uncertain["action"]
    assert "仅在 CODEX_SEND_UNCERTAIN 只读预检通过后检查连接" in desktop_disconnected[
        "action"
    ]
    queue_description = queue_operation["description"]
    assert queue_description.index("先执行只读 CODEX_SEND_UNCERTAIN 预检") < (
        queue_description.index("再检查 Codex Desktop 连接")
    )
    assert "即使当前断连也不会降级为 503" in queue_description
    assert "仅在该预检通过后，未连接才返回 503 CODEX_DESKTOP_NOT_CONNECTED" in (
        queue_description
    )
    queue_conflict_description = queue_operation["responses"]["409"]["description"]
    assert "TASK_PROJECT_SESSION_LIMIT_REACHED" not in queue_conflict_description
    operation = schema["paths"]["/v1/codex/threads/{threadId}/open"]["post"]
    assert "已确认 thread 存在" in operation["description"]
    assert "向操作系统提交" in operation["description"]
    assert "不保证 CodeX UI 已完成切换" in operation["description"]
    assert components["OperationResponse"]["properties"]["ok"]["const"] is True
    assert "CODEX_PROCESS_CHECK_FAILED" not in schema_response.text
