use std::fs;
use std::path::PathBuf;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 私有模型元数据文件名；当前未发布版本直接保存 API Key，避免开发期 Keychain 反复弹窗。
const PRIVATE_MODEL_FILE_NAME: &str = "private-models.json";
/// 禁用模型注入 sidecar 时使用的非敏感占位凭据；服务会先校验 enabled，绝不会用该值请求上游。
const DISABLED_MODEL_API_KEY_PLACEHOLDER: &str = "disabled-model-without-runtime-credential";

/// 模型配置变更前的内存快照，用于 JSON 写入或 sidecar 重启失败时完整补偿。
pub struct PrivateModelSnapshot {
    /// 变更前完整模型配置。
    records: Vec<PrivateModelRecord>,
}

/// 私有模型能力类型，限制 sidecar 注册表只能接收语音识别或文本生成模型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PrivateModelCapability {
    /// ASR 语音转文字模型。
    Asr,
    /// 文本润色或生成模型。
    Text,
}

/// 前端保存或测试私有模型时提交的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePrivateModelRequest {
    /// 已有模型 ID；新增时为空并由 Rust 生成稳定 ID。
    pub id: Option<String>,
    /// 用户可识别的模型名称。
    pub display_name: String,
    /// 模型用途能力。
    pub capability: PrivateModelCapability,
    /// 是否允许 sidecar 调度该模型。
    pub enabled: bool,
    /// 是否为相同能力的默认模型。
    pub is_default: bool,
    /// 模型供应商协议标识。
    pub provider: String,
    /// 供应商 HTTPS API 基础地址。
    pub base_url: String,
    /// 供应商实际模型名称。
    pub model_name: String,
    /// 新增或轮换时提供的 API Key；更新其它字段时可省略以保留 JSON 现值。
    pub api_key: Option<String>,
}

/// 本地私有模型配置；落盘时包含 API Key，返回给 WebView 前必须清空密钥原文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateModelRecord {
    /// 本地稳定模型 ID。
    pub id: String,
    /// 用户可识别名称。
    pub display_name: String,
    /// 模型用途能力。
    pub capability: PrivateModelCapability,
    /// 是否启用。
    pub enabled: bool,
    /// 是否为能力默认项。
    pub is_default: bool,
    /// 供应商协议标识。
    pub provider: String,
    /// API 基础地址。
    pub base_url: String,
    /// 实际模型名称。
    pub model_name: String,
    /// 上游 API Key；只允许落盘和 sidecar stdin 使用，IPC 返回前必须清空。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// JSON 配置中是否存在对应 API Key，仅用于 UI 展示配置完整性。
    #[serde(default)]
    pub has_api_key: bool,
}

/// 安全模型目录项；只暴露业务选择所需字段，禁止携带上游地址、模型名或密钥。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicModelCatalogRecord {
    /// 本地稳定模型 ID，业务调用只传递该不透明 ID。
    pub id: String,
    /// 用户可识别名称。
    pub display_name: String,
    /// 模型用途能力。
    pub capability: PrivateModelCapability,
    /// 是否在运行时可用；只有启用且已有 API Key 的模型才可用。
    pub enabled: bool,
    /// 是否为能力默认项；不可用模型不会被标记为默认项。
    pub is_default: bool,
}

/// 传入 sidecar 子进程的完整模型项；该类型只用于进程环境序列化，禁止作为 IPC 返回值。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarModelRecord {
    /// 本地模型 ID。
    pub id: String,
    /// 展示名称。
    pub display_name: String,
    /// 能力类型。
    pub capability: PrivateModelCapability,
    /// 是否启用。
    pub enabled: bool,
    /// 是否为默认项。
    pub is_default: bool,
    /// 供应商协议。
    pub provider: String,
    /// API 基础地址。
    pub base_url: String,
    /// 实际模型名。
    pub model_name: String,
    /// 从 JSON 配置读取的 API Key，仅存在于 Rust 临时值和 sidecar stdin bootstrap。
    pub api_key: String,
}

/// 私有模型探针失败，区分可展示业务失败与必须通过 IPC 抛出的内部错误。
#[derive(Debug)]
pub struct PrivateModelTestFailure {
    /// 稳定诊断码，供页面展示和问题检索。
    pub code: &'static str,
    /// 已脱敏失败说明，不包含 API Key、请求正文或上游响应正文。
    pub message: String,
    /// 是否属于客户端构造等内部故障；内部故障不得伪装成普通探测结果。
    pub is_internal: bool,
}

/// 列出私有模型元数据并标记 API Key 是否存在。
/// 流程：读取 app_data_dir JSON，基于本地字段计算 hasApiKey，再清空密钥原文后返回；参数为 Tauri AppHandle；返回不含密钥的列表。
/// 异常/边界：JSON 损坏会显式报错，禁止把失败伪装成空配置。
pub fn list_private_models(app: &AppHandle) -> Result<Vec<PrivateModelRecord>, String> {
    let mut records = read_metadata(app)?;
    for record in &mut records {
        record.has_api_key = !record.api_key.trim().is_empty();
        record.api_key.clear();
    }
    Ok(records)
}

/// 列出本机业务页可使用的安全模型目录。
/// 流程：读取包含密钥状态的私有元数据，按 sidecar 运行时规则计算 enabled/default，再剥离上游连接信息和密钥后返回。
/// 参数：app 为 Tauri AppHandle。
/// 返回：只包含不透明 ID、展示名、能力和运行时可用状态的目录。
/// 异常/边界：JSON 损坏或磁盘读取失败会显式报错；缺少 API Key 的模型保留在目录中但不可选。
pub fn list_public_model_catalog(app: &AppHandle) -> Result<Vec<PublicModelCatalogRecord>, String> {
    let records = read_metadata(app)?;
    Ok(build_public_model_catalog(records))
}

/// 保存私有模型元数据及可选 API Key。
/// 流程：校验字段和 HTTPS 地址，直接把密钥随模型配置写入 JSON；参数为 AppHandle 与表单；返回刷新后元数据。
/// 异常/边界：更新未携带 apiKey 时保留旧密钥；默认项会取消同能力其它默认标记；任何错误直接返回。
pub fn save_private_model(
    app: &AppHandle,
    request: SavePrivateModelRequest,
) -> Result<Vec<PrivateModelRecord>, String> {
    validate_request(&request)?;
    let snapshot = capture_snapshot(app)?;
    let mut records = snapshot.records.clone();
    let id = request
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("model_{}", uuid::Uuid::new_v4()));
    if request.id.is_some() && !records.iter().any(|record| record.id == id) {
        return Err("待编辑的私有模型不存在".to_string());
    }
    if records
        .iter()
        .find(|record| record.id == id)
        .is_some_and(|record| record.capability != request.capability)
    {
        return Err("模型能力创建后不可修改；请新增并重新测试对应能力模型".to_string());
    }
    let existing_api_key = records
        .iter()
        .find(|record| record.id == id)
        .map(|record| record.api_key.trim().to_string())
        .unwrap_or_default();
    let api_key = match request.api_key.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value.to_string(),
        Some(_) => return Err("API Key 不能为空；如需保留已有凭证请不要传该字段".to_string()),
        None => existing_api_key,
    };
    if request.is_default {
        for record in &mut records {
            if record.capability == request.capability {
                record.is_default = false;
            }
        }
    }
    let has_api_key = !api_key.is_empty();
    let record = PrivateModelRecord {
        id: id.clone(),
        display_name: request.display_name.trim().to_string(),
        capability: request.capability.clone(),
        enabled: request.enabled,
        is_default: request.is_default,
        provider: request.provider.trim().to_string(),
        base_url: request.base_url.trim_end_matches('/').to_string(),
        model_name: request.model_name.trim().to_string(),
        api_key,
        has_api_key,
    };
    if record.enabled && !record.has_api_key {
        return Err("请提供模型 API Key".to_string());
    }
    if let Some(index) = records.iter().position(|item| item.id == id) {
        records[index] = record;
    } else {
        records.push(record);
    }
    normalize_defaults(&mut records, &request.capability);
    if let Err(error) = write_metadata(app, &records) {
        restore_snapshot(app, snapshot)?;
        return Err(error);
    }
    list_private_models(app)
}

/// 删除私有模型配置。
/// 流程：确认 ID 存在后从 JSON 中移除记录并归一化默认项；参数为 AppHandle 和模型 ID；返回剩余列表。
/// 异常/边界：未知 ID 显式报错，写入失败会恢复原配置。
pub fn delete_private_model(app: &AppHandle, id: &str) -> Result<Vec<PrivateModelRecord>, String> {
    let snapshot = capture_snapshot(app)?;
    let normalized = id.trim();
    let mut records = read_metadata(app)?;
    let deleted_capability = records
        .iter()
        .find(|record| record.id == normalized)
        .map(|record| record.capability.clone());
    let before = records.len();
    records.retain(|record| record.id != normalized);
    if records.len() == before {
        return Err("私有模型不存在".to_string());
    }
    if let Some(capability) = deleted_capability {
        normalize_defaults(&mut records, &capability);
    }
    if let Err(error) = write_metadata(app, &records) {
        restore_snapshot(app, snapshot)?;
        return Err(error);
    }
    list_private_models(app)
}

/// 测试未保存或已保存的模型表单，不写入任何本地状态。
/// 流程：校验表单，优先使用本次 apiKey，否则按 id 从 JSON 配置读取，再按真实业务协议调用 `chat/completions`；返回可读成功说明。
/// 异常/边界：只允许 HTTPS；配置、连接、超时、状态码和协议失败返回稳定分类；请求与错误中不包含密钥原文或上游正文。
pub fn test_private_model(
    app: Option<&AppHandle>,
    request: SavePrivateModelRequest,
) -> Result<String, PrivateModelTestFailure> {
    validate_request(&request).map_err(|message| PrivateModelTestFailure {
        code: "MODEL_CONFIG_INVALID",
        message,
        is_internal: false,
    })?;
    let api_key = match request.api_key.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => {
            let id = request
                .id
                .as_deref()
                .ok_or_else(|| PrivateModelTestFailure {
                    code: "MODEL_API_KEY_REQUIRED",
                    message: "测试未保存模型时必须提供 API Key".to_string(),
                    is_internal: false,
                })?;
            let app = app.ok_or_else(|| PrivateModelTestFailure {
                code: "MODEL_API_KEY_REQUIRED",
                message: "测试未保存模型时必须提供 API Key".to_string(),
                is_internal: false,
            })?;
            let records = read_metadata(app).map_err(|message| PrivateModelTestFailure {
                code: "MODEL_CREDENTIAL_UNAVAILABLE",
                message,
                is_internal: true,
            })?;
            records
                .into_iter()
                .find(|record| record.id == id)
                .map(|record| record.api_key.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| PrivateModelTestFailure {
                    code: "MODEL_API_KEY_REQUIRED",
                    message: "请先填写 API Key。".to_string(),
                    is_internal: true,
                })
        }?,
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| PrivateModelTestFailure {
            code: "MODEL_TEST_CLIENT_UNAVAILABLE",
            message: format!("创建模型测试客户端失败：{}", error),
            is_internal: true,
        })?;
    let base_url = request.base_url.trim_end_matches('/');
    let response = match request.capability {
        PrivateModelCapability::Text => client
            .post(format!("{}/chat/completions", base_url))
            .bearer_auth(&api_key)
            .json(&serde_json::json!({
                "model": request.model_name,
                "messages": [{"role": "user", "content": "ping"}],
                "max_completion_tokens": 1
            }))
            .send(),
        PrivateModelCapability::Asr => client
            .post(format!("{}/chat/completions", base_url))
            .bearer_auth(&api_key)
            .json(&build_asr_probe_body(&request.model_name))
            .send(),
    }
    .map_err(|error| PrivateModelTestFailure {
        code: if error.is_timeout() {
            "MODEL_UPSTREAM_TIMEOUT"
        } else {
            "MODEL_CONNECTION_FAILED"
        },
        message: if error.is_timeout() {
            "模型连通性测试失败：上游请求超过 15 秒".to_string()
        } else {
            format!("模型连通性测试失败：{}", error)
        },
        is_internal: false,
    })?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(PrivateModelTestFailure {
            code: upstream_http_error_code(status),
            message: format!("模型连通性测试失败：上游返回 HTTP {}", status),
            is_internal: false,
        });
    }
    let payload = response
        .json::<serde_json::Value>()
        .map_err(|_| PrivateModelTestFailure {
            code: "MODEL_UPSTREAM_RESPONSE_INVALID",
            message: "模型连通性测试失败：上游成功响应不是合法 JSON".to_string(),
            is_internal: false,
        })?;
    let contract_valid = match request.capability {
        PrivateModelCapability::Text => payload.pointer("/choices/0/message/content").is_some(),
        PrivateModelCapability::Asr => payload.pointer("/choices/0/message/content").is_some(),
    };
    if !contract_valid {
        return Err(PrivateModelTestFailure {
            code: "MODEL_UPSTREAM_CONTRACT_INVALID",
            message: "模型连通性测试失败：上游响应不符合对应能力协议".to_string(),
            is_internal: false,
        });
    }
    Ok("模型连通性测试通过".to_string())
}

/// 构造与正式语音转写完全一致的 ASR 模型探针请求体。
/// 流程：把最小合法 WAV 编码为历史已验证的 data URL，再按 OpenAI Compatible chat/completions 音频结构组装请求。
/// 参数：model_name 为上游实际模型名；返回只含 model/messages 的 JSON 请求体。
/// 异常/边界：探针固定使用 audio/wav 与 auto 语言，不增加正式链路未发送的 format、提示词或生成参数，避免测试成功但业务协议不一致。
fn build_asr_probe_body(model_name: &str) -> serde_json::Value {
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(minimal_wav());
    serde_json::json!({
        "model": model_name,
        "messages": [{
            "role": "user",
            "content": [{
                "type": "input_audio",
                "input_audio": {
                    "data": format!("data:audio/wav;base64,{}", audio_base64)
                }
            }]
        }]
    })
}

/// 把上游 HTTP 状态映射为稳定模型探针诊断码。
/// 流程：优先区分鉴权、限流和服务端故障，其它非成功状态归入通用 HTTP 错误。
/// 参数：status 为未读取响应正文的上游状态码；返回可向页面公开的静态错误码。
/// 异常/边界：本方法不读取或记录响应正文，调用方只在非 2xx 分支使用。
fn upstream_http_error_code(status: reqwest::StatusCode) -> &'static str {
    match status.as_u16() {
        401 | 403 => "MODEL_UPSTREAM_AUTH_FAILED",
        429 => "MODEL_UPSTREAM_RATE_LIMITED",
        _ if status.is_server_error() => "MODEL_UPSTREAM_UNAVAILABLE",
        _ => "MODEL_UPSTREAM_HTTP_ERROR",
    }
}

/// 构建 sidecar 注册表 JSON。
/// 流程：读取 JSON 模型配置，为启用且已有密钥的模型传入密钥；禁用或缺密钥模型注入固定非密占位值。
/// 参数：app 为 Tauri AppHandle；返回仅供匿名 stdin 管道使用的 JSON。
/// 异常/边界：缺密钥模型不会阻止 App 启动，但不会作为可用模型进入 sidecar；公开目录仍不泄露历史密钥。
pub fn sidecar_catalog_json(app: &AppHandle) -> Result<String, String> {
    let records = read_metadata(app)?;
    let catalog = build_sidecar_catalog(records)?;
    serde_json::to_string(&catalog)
        .map_err(|error| format!("序列化 sidecar 模型注册表失败：{}", error))
}

/// 构造 sidecar 运行目录。
/// 流程：逐项保留完整模型元数据；启用且已有密钥的记录使用 JSON 中的 API Key，其它记录使用固定非敏感占位值。
/// 参数：records 为磁盘元数据；返回 sidecar 专用目录。
/// 异常/边界：缺密钥记录会按禁用注入，避免历史半配置阻断 App 启动。
fn build_sidecar_catalog(
    records: Vec<PrivateModelRecord>,
) -> Result<Vec<SidecarModelRecord>, String> {
    let mut catalog = Vec::with_capacity(records.len());
    for record in records {
        let normalized_api_key = record.api_key.trim();
        let is_runtime_enabled = record.enabled && !normalized_api_key.is_empty();
        let api_key = if is_runtime_enabled {
            normalized_api_key.to_string()
        } else {
            DISABLED_MODEL_API_KEY_PLACEHOLDER.to_string()
        };
        catalog.push(SidecarModelRecord {
            id: record.id,
            display_name: record.display_name,
            capability: record.capability,
            enabled: is_runtime_enabled,
            is_default: record.is_default && is_runtime_enabled,
            provider: record.provider,
            base_url: record.base_url,
            model_name: record.model_name,
            api_key,
        });
    }
    Ok(catalog)
}

/// 构造业务页安全模型目录。
/// 流程：复用 sidecar 的运行时可用规则，但只保留业务选择字段。
/// 参数：records 为包含密钥状态的磁盘元数据。
/// 返回：不包含上游连接参数和密钥的模型目录。
/// 异常/边界：缺密钥模型返回 enabled=false，默认项也随之失效。
fn build_public_model_catalog(records: Vec<PrivateModelRecord>) -> Vec<PublicModelCatalogRecord> {
    records
        .into_iter()
        .map(|record| {
            let is_runtime_enabled = record.enabled && !record.api_key.trim().is_empty();
            PublicModelCatalogRecord {
                id: record.id,
                display_name: record.display_name,
                capability: record.capability,
                enabled: is_runtime_enabled,
                is_default: record.is_default && is_runtime_enabled,
            }
        })
        .collect()
}

/// 校验模型表单的必填字段和网络边界。
fn validate_request(request: &SavePrivateModelRequest) -> Result<(), String> {
    let display_name = request.display_name.trim();
    let provider = request.provider.trim();
    let model_name = request.model_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 80 {
        return Err("模型展示名称长度必须为 1 到 80 个字符".to_string());
    }
    if provider != "openai-compatible" {
        return Err("当前私有模型只支持 openai-compatible 协议".to_string());
    }
    if model_name.is_empty()
        || model_name.chars().count() > 160
        || model_name.chars().any(char::is_control)
    {
        return Err("模型名称、供应商和模型标识不能为空".to_string());
    }
    if let Some(id) = request.id.as_deref() {
        if !valid_model_id(id) {
            return Err("私有模型 ID 格式无效".to_string());
        }
    }
    if request.is_default && !request.enabled {
        return Err("默认模型必须处于启用状态".to_string());
    }
    let url = reqwest::Url::parse(request.base_url.trim())
        .map_err(|_| "模型 Base URL 不是合法 URL".to_string())?;
    let raw_url = request.base_url.trim().to_ascii_lowercase();
    let host = url
        .host_str()
        .ok_or_else(|| "模型 Base URL 缺少主机".to_string())?;
    let loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
    if !matches!(url.scheme(), "http" | "https")
        || (url.scheme() != "https" && !loopback)
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || raw_url.contains("%2e")
        || raw_url.contains('\\')
        || url.path().contains("//")
    {
        return Err(
            "公网模型 Base URL 必须为无凭据 HTTPS；仅本机回环地址允许 HTTP，且禁止 query/fragment"
                .to_string(),
        );
    }
    let segments = url
        .path_segments()
        .map(|items| items.filter(|item| !item.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    if segments
        .iter()
        .any(|segment| matches!(*segment, "." | ".."))
        || segments.ends_with(&["chat", "completions"])
        || segments.ends_with(&["audio", "transcriptions"])
        || segments.ends_with(&["models"])
    {
        return Err("模型 Base URL 包含路径穿越或固定 endpoint".to_string());
    }
    Ok(())
}

/// 验证原生生成的模型 ID，编辑操作只接受 `model_` 加 UUID。
fn valid_model_id(id: &str) -> bool {
    id.strip_prefix("model_")
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .is_some()
}

/// 生成仅含一个静音采样点的合法 PCM WAV，用于 ASR 真实协议探测。
fn minimal_wav() -> Vec<u8> {
    let mut bytes = Vec::from(*b"RIFF");
    bytes.extend_from_slice(&38_u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8_000_u32.to_le_bytes());
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&0_i16.to_le_bytes());
    bytes
}

/// 归一化同能力默认项，确保至多一个且默认项必为 enabled。
fn normalize_defaults(records: &mut [PrivateModelRecord], capability: &PrivateModelCapability) {
    let selected = records
        .iter()
        .position(|record| record.capability == *capability && record.enabled && record.is_default)
        .or_else(|| {
            records
                .iter()
                .position(|record| record.capability == *capability && record.enabled)
        });
    for (index, record) in records.iter_mut().enumerate() {
        if record.capability == *capability {
            record.is_default = Some(index) == selected;
        }
    }
}

/// 捕获模型元数据内存快照，供跨资源操作补偿。
pub fn capture_snapshot(app: &AppHandle) -> Result<PrivateModelSnapshot, String> {
    let records = read_metadata(app)?;
    Ok(PrivateModelSnapshot { records })
}

/// 恢复模型元数据快照。
pub fn restore_snapshot(app: &AppHandle, snapshot: PrivateModelSnapshot) -> Result<(), String> {
    write_metadata(app, &snapshot.records)
}

/// 读取包含密钥的模型元数据文件；文件不存在时返回空列表。
fn read_metadata(app: &AppHandle) -> Result<Vec<PrivateModelRecord>, String> {
    let path = metadata_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content =
        fs::read_to_string(path).map_err(|error| format!("读取私有模型元数据失败：{}", error))?;
    serde_json::from_str(&content).map_err(|error| format!("解析私有模型元数据失败：{}", error))
}

/// 原子写入包含密钥的模型元数据，避免进程中断产生半截 JSON。
fn write_metadata(app: &AppHandle, records: &[PrivateModelRecord]) -> Result<(), String> {
    let path = metadata_path(app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "私有模型元数据路径无父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建私有模型目录失败：{}", error))?;
    let temporary = path.with_extension("json.tmp");
    let content = serde_json::to_vec_pretty(records)
        .map_err(|error| format!("序列化私有模型元数据失败：{}", error))?;
    fs::write(&temporary, content)
        .map_err(|error| format!("写入私有模型临时文件失败：{}", error))?;
    fs::rename(temporary, path).map_err(|error| format!("替换私有模型元数据失败：{}", error))
}

/// 返回 app_data_dir 下模型元数据绝对路径。
fn metadata_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(PRIVATE_MODEL_FILE_NAME))
        .map_err(|error| format!("读取应用数据目录失败：{}", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小合法文本模型请求，供纯校验测试复用。
    fn valid_request(base_url: &str) -> SavePrivateModelRequest {
        SavePrivateModelRequest {
            id: None,
            display_name: "测试模型".to_string(),
            capability: PrivateModelCapability::Text,
            enabled: true,
            is_default: false,
            provider: "openai-compatible".to_string(),
            base_url: base_url.to_string(),
            model_name: "gpt-test".to_string(),
            api_key: Some("secret-value".to_string()),
        }
    }

    /// 公网 HTTP 必须被拒绝，避免模型密钥明文传输。
    #[test]
    fn public_http_base_url_is_rejected() {
        assert!(validate_request(&valid_request("http://api.example.com/v1")).is_err());
    }

    /// 本机回环 HTTP 允许用于受控开发服务。
    #[test]
    fn loopback_http_base_url_is_allowed() {
        assert!(validate_request(&valid_request("http://127.0.0.1:9000/v1")).is_ok());
    }

    /// 已包含固定 endpoint 的地址必须拒绝，避免重复拼接和请求错路由。
    #[test]
    fn fixed_endpoint_base_url_is_rejected() {
        assert!(validate_request(&valid_request(
            "https://api.example.com/v1/chat/completions"
        ))
        .is_err());
    }

    /// 当前私有模型协议只允许 OpenAI Compatible，拒绝未知 provider 进入持久化和探针链路。
    #[test]
    fn unsupported_provider_is_rejected() {
        let mut request = valid_request("https://api.example.com/v1");
        request.provider = "anthropic".to_string();
        assert!(validate_request(&request).is_err());
    }

    /// 探针配置校验失败必须返回稳定配置错误码，且不得进入网络请求。
    #[test]
    fn model_test_reports_invalid_configuration_code() {
        let failure = test_private_model(None, valid_request("http://api.example.com/v1"))
            .expect_err("公网 HTTP 配置必须被探针拒绝");
        assert_eq!(failure.code, "MODEL_CONFIG_INVALID");
        assert!(!failure.is_internal);
    }

    /// 未保存模型缺少 API Key 时必须返回可修正的稳定错误码。
    #[test]
    fn model_test_reports_missing_api_key_code() {
        let mut request = valid_request("http://127.0.0.1:9/v1");
        request.api_key = None;
        let failure = test_private_model(None, request).expect_err("缺少 API Key 必须阻断探针");
        assert_eq!(failure.code, "MODEL_API_KEY_REQUIRED");
        assert!(!failure.is_internal);
    }

    /// 上游状态必须按鉴权、限流、服务故障和其它 HTTP 错误稳定分类。
    #[test]
    fn upstream_http_statuses_have_stable_diagnostic_codes() {
        assert_eq!(
            upstream_http_error_code(reqwest::StatusCode::UNAUTHORIZED),
            "MODEL_UPSTREAM_AUTH_FAILED"
        );
        assert_eq!(
            upstream_http_error_code(reqwest::StatusCode::FORBIDDEN),
            "MODEL_UPSTREAM_AUTH_FAILED"
        );
        assert_eq!(
            upstream_http_error_code(reqwest::StatusCode::TOO_MANY_REQUESTS),
            "MODEL_UPSTREAM_RATE_LIMITED"
        );
        assert_eq!(
            upstream_http_error_code(reqwest::StatusCode::BAD_GATEWAY),
            "MODEL_UPSTREAM_UNAVAILABLE"
        );
        assert_eq!(
            upstream_http_error_code(reqwest::StatusCode::BAD_REQUEST),
            "MODEL_UPSTREAM_HTTP_ERROR"
        );
    }

    /// 原生模型 ID 只接受 model_ 前缀和合法 UUID。
    #[test]
    fn model_id_validation_rejects_forged_value() {
        assert!(valid_model_id("model_550e8400-e29b-41d4-a716-446655440000"));
        assert!(!valid_model_id("../../keychain"));
    }

    /// ASR 探针必须是带 RIFF/WAVE 头且尺寸字段一致的非空 WAV。
    #[test]
    fn minimal_asr_probe_is_valid_wav_container() {
        let wav = minimal_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 46);
    }

    /// ASR 探针请求必须复用正式转写的 data URL 协议，禁止出现历史链路未发送的 format 或额外提示词。
    #[test]
    fn asr_probe_uses_production_transcription_contract() {
        let body = build_asr_probe_body("mimo-asr-test");
        assert_eq!(body["model"], "mimo-asr-test");
        let content = body["messages"][0]["content"]
            .as_array()
            .expect("ASR content 必须是数组");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "input_audio");
        let input_audio = content[0]["input_audio"]
            .as_object()
            .expect("input_audio 必须是对象");
        assert_eq!(input_audio.len(), 1);
        assert!(input_audio
            .get("data")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.starts_with("data:audio/wav;base64,")));
        assert!(input_audio.get("format").is_none());
        assert!(body.get("max_completion_tokens").is_none());
    }

    /// 默认项被禁用后必须自动选择同能力首个 enabled，且不能保留多个 default。
    #[test]
    fn default_normalization_selects_one_enabled_model() {
        let mut records = vec![
            PrivateModelRecord {
                id: "disabled".to_string(),
                display_name: "禁用".to_string(),
                capability: PrivateModelCapability::Text,
                enabled: false,
                is_default: true,
                provider: "openai-compatible".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                model_name: "disabled".to_string(),
                api_key: String::new(),
                has_api_key: true,
            },
            PrivateModelRecord {
                id: "enabled".to_string(),
                display_name: "启用".to_string(),
                capability: PrivateModelCapability::Text,
                enabled: true,
                is_default: false,
                provider: "openai-compatible".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                model_name: "enabled".to_string(),
                api_key: "secret-value".to_string(),
                has_api_key: true,
            },
        ];
        normalize_defaults(&mut records, &PrivateModelCapability::Text);
        assert!(!records[0].is_default);
        assert!(records[1].is_default);
    }

    /// 禁用模型必须保留在 sidecar 目录中，同时不使用真实 API Key 并写入非敏感占位值。
    #[test]
    fn disabled_sidecar_model_does_not_require_api_key() {
        let records = vec![PrivateModelRecord {
            id: "model_550e8400-e29b-41d4-a716-446655440000".to_string(),
            display_name: "已禁用模型".to_string(),
            capability: PrivateModelCapability::Text,
            enabled: false,
            is_default: false,
            provider: "openai-compatible".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            model_name: "disabled-model".to_string(),
            api_key: String::new(),
            has_api_key: false,
        }];
        let catalog = build_sidecar_catalog(records).expect("禁用项缺少 API Key 时仍应生成目录");

        assert_eq!(catalog.len(), 1);
        assert!(!catalog[0].enabled);
        assert_eq!(catalog[0].api_key, DISABLED_MODEL_API_KEY_PLACEHOLDER);
        assert!(!catalog[0].api_key.contains("secret"));
    }

    /// 业务页安全目录必须按运行时规则计算可用状态，并且不能暴露上游连接字段或密钥。
    #[test]
    fn public_model_catalog_uses_runtime_enabled_state() {
        let records = vec![
            PrivateModelRecord {
                id: "model_550e8400-e29b-41d4-a716-446655440001".to_string(),
                display_name: "可用文本模型".to_string(),
                capability: PrivateModelCapability::Text,
                enabled: true,
                is_default: true,
                provider: "openai-compatible".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                model_name: "text-model".to_string(),
                api_key: "secret-value".to_string(),
                has_api_key: true,
            },
            PrivateModelRecord {
                id: "model_550e8400-e29b-41d4-a716-446655440002".to_string(),
                display_name: "缺密钥 ASR 模型".to_string(),
                capability: PrivateModelCapability::Asr,
                enabled: true,
                is_default: true,
                provider: "openai-compatible".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                model_name: "asr-model".to_string(),
                api_key: String::new(),
                has_api_key: false,
            },
        ];

        let catalog = build_public_model_catalog(records);

        assert_eq!(catalog.len(), 2);
        assert!(catalog[0].enabled);
        assert!(catalog[0].is_default);
        assert!(!catalog[1].enabled);
        assert!(!catalog[1].is_default);
    }
}
