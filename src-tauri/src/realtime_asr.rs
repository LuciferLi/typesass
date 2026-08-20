use std::collections::BTreeMap;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha1::Sha1;
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header::{AUTHORIZATION, CONTENT_TYPE};
use tungstenite::{connect, Message, WebSocket};
use url::Url;

use crate::app_audio::{AppAudioPcmCallback, AppAudioPcmChunk};
use crate::private_models::PrivateModelRuntimeRecord;

const REALTIME_ASR_TARGET_SAMPLE_RATE: u32 = 16_000;
const REALTIME_ASR_CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const REALTIME_ASR_READ_TIMEOUT: Duration = Duration::from_millis(260);
const REALTIME_ASR_STREAM_READ_TIMEOUT: Duration = Duration::from_millis(2);
const REALTIME_ASR_FINAL_TIMEOUT: Duration = Duration::from_secs(10);
const REALTIME_ASR_PARTIAL_POLL_FRAME_INTERVAL: usize = 8;

type HmacSha1 = Hmac<Sha1>;

/// 实时 ASR 分片发送器；录音回调只负责投递 PCM，不直接访问网络。
pub struct RealtimeAsrSession {
    /// 录音回调使用的 PCM 投递函数。
    pub pcm_callback: AppAudioPcmCallback,
    /// 后台 provider worker 控制句柄。
    worker: RealtimeAsrWorker,
}

/// 实时 ASR worker 句柄，用于停止录音后收尾并取得最终文本。
pub struct RealtimeAsrWorker {
    /// 关闭发送端后 provider worker 才会发送结束帧。
    sender: Sender<AppAudioPcmChunk>,
    /// provider worker 线程。
    handle: thread::JoinHandle<Result<RealtimeAsrResult, String>>,
}

/// 实时 ASR 结果，包含最终识别文本。
pub struct RealtimeAsrResult {
    /// 识别文本。
    pub text: String,
}

/// 实时 ASR partial 文本回调，用于浮窗展示边录边识别反馈。
pub type RealtimeAsrPartialCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// 实时 ASR 诊断回调，用于把 provider 阶段状态写入本机日志。
pub type RealtimeAsrDiagnosticCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// 启动实时 ASR provider worker。
/// 流程：根据模型 provider 创建对应 WebSocket worker，并返回可挂到录音层的 PCM callback。
/// 参数：model 为本机运行时模型配置，partial_callback 为可选临时文本回调，diagnostic_callback 为可选诊断回调。
/// 返回：可交给录音层的 session。
/// 异常/边界：未知 provider 显式拒绝，避免误走 OpenAI-compatible 或错误厂商协议。
pub fn start_realtime_asr_session(
    model: PrivateModelRuntimeRecord,
    partial_callback: Option<RealtimeAsrPartialCallback>,
    diagnostic_callback: Option<RealtimeAsrDiagnosticCallback>,
) -> Result<RealtimeAsrSession, String> {
    if !is_realtime_asr_provider(&model.provider) {
        return Err("当前模型不是实时 ASR provider。".to_string());
    }
    let (sender, receiver) = mpsc::channel::<AppAudioPcmChunk>();
    let worker_model = model.clone();
    let handle = thread::spawn(move || {
        run_provider_worker(
            worker_model,
            receiver,
            partial_callback,
            diagnostic_callback,
        )
    });
    let callback_sender = sender.clone();
    let pcm_callback: AppAudioPcmCallback = Arc::new(move |chunk| {
        let _ = callback_sender.send(chunk);
    });
    Ok(RealtimeAsrSession {
        pcm_callback,
        worker: RealtimeAsrWorker { sender, handle },
    })
}

/// 判断 provider 是否属于实时 ASR。
/// 流程：只允许明确内置的三家 provider 进入实时链路。
/// 参数：provider 为模型配置中的运行协议。
/// 返回：实时 ASR provider 返回 true。
/// 异常/边界：自定义中转站和 OpenAI-compatible 都返回 false，继续走批量 ASR。
pub fn is_realtime_asr_provider(provider: &str) -> bool {
    matches!(
        provider.trim(),
        "aliyun-realtime-asr" | "tencent-realtime-asr" | "iflytek-realtime-asr"
    )
}

impl RealtimeAsrSession {
    /// 结束 provider 音频流并等待最终识别结果。
    /// 流程：关闭 PCM 发送端，让后台 worker 发结束帧并读取 final 文本。
    /// 参数：无。
    /// 返回：实时 ASR 最终文本。
    /// 异常/边界：worker panic、鉴权失败、连接断开或空结果都会返回明确错误。
    pub fn finish(self) -> Result<RealtimeAsrResult, String> {
        let RealtimeAsrSession {
            pcm_callback,
            worker,
        } = self;
        drop(pcm_callback);
        drop(worker.sender);
        worker
            .handle
            .join()
            .map_err(|_| "实时 ASR worker 异常退出。".to_string())?
    }
}

/// 分发实时 ASR provider worker。
/// 流程：按 provider 调用不同厂商协议实现，所有实现统一输出最终文本。
/// 参数：model 为运行模型，receiver 为录音 PCM 分片，partial_callback 为 UI 回调，diagnostic_callback 为诊断回调。
/// 返回：最终识别结果。
/// 异常/边界：任何 provider 错误都转成不含密钥的中文错误。
fn run_provider_worker(
    model: PrivateModelRuntimeRecord,
    receiver: Receiver<AppAudioPcmChunk>,
    partial_callback: Option<RealtimeAsrPartialCallback>,
    diagnostic_callback: Option<RealtimeAsrDiagnosticCallback>,
) -> Result<RealtimeAsrResult, String> {
    emit_realtime_diagnostic(
        diagnostic_callback.as_ref(),
        &format!(
            "realtime asr worker started provider={} model={}",
            model.provider, model.model_name
        ),
    );
    let provider_result = match model.provider.as_str() {
        "aliyun-realtime-asr" => run_aliyun_realtime_asr(
            &model,
            receiver,
            partial_callback,
            diagnostic_callback.as_ref(),
        ),
        "tencent-realtime-asr" => run_tencent_realtime_asr(
            &model,
            receiver,
            partial_callback,
            diagnostic_callback.as_ref(),
        ),
        "iflytek-realtime-asr" => run_iflytek_realtime_asr(
            &model,
            receiver,
            partial_callback,
            diagnostic_callback.as_ref(),
        ),
        _ => {
            emit_realtime_diagnostic(
                diagnostic_callback.as_ref(),
                "realtime asr worker failed unsupported provider",
            );
            return Err("不支持的实时 ASR provider。".to_string());
        }
    };
    let text = provider_result.map_err(|error| {
        emit_realtime_diagnostic(
            diagnostic_callback.as_ref(),
            &format!(
                "realtime asr worker failed provider={} error={}",
                model.provider, error
            ),
        );
        error
    })?;
    let normalized_text = text.trim().to_string();
    if normalized_text.is_empty() {
        emit_realtime_diagnostic(
            diagnostic_callback.as_ref(),
            "realtime asr worker finished with empty text",
        );
        return Err("实时 ASR 没有返回有效文本。".to_string());
    }
    emit_realtime_diagnostic(
        diagnostic_callback.as_ref(),
        &format!(
            "realtime asr worker finished text_chars={}",
            normalized_text.chars().count()
        ),
    );
    Ok(RealtimeAsrResult {
        text: normalized_text,
    })
}

/// 调用阿里百炼实时 ASR。
/// 流程：Bearer 鉴权建立 WebSocket，发送 run-task JSON，等待 task-started 后发送 PCM，最后发送 finish-task 并读取结果。
/// 参数：model 为阿里实时模型配置，receiver 为 PCM 分片，partial_callback 为临时文本回调。
/// 返回：最终识别文本。
/// 异常/边界：如果服务端返回 task-failed 或连接提前关闭，返回脱敏错误。
fn run_aliyun_realtime_asr(
    model: &PrivateModelRuntimeRecord,
    receiver: Receiver<AppAudioPcmChunk>,
    partial_callback: Option<RealtimeAsrPartialCallback>,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<String, String> {
    emit_realtime_diagnostic(diagnostic_callback, "aliyun realtime asr url normalizing");
    let url = normalize_ws_url(&model.base_url)?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "aliyun realtime asr websocket connecting",
    );
    let task_id = uuid::Uuid::new_v4().to_string();
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|error| format!("创建阿里实时 ASR WebSocket 请求失败：{}", error))?;
    request.headers_mut().insert(
        AUTHORIZATION,
        format!("Bearer {}", model.api_key.trim())
            .parse()
            .map_err(|_| "阿里实时 ASR API Key 格式无效。".to_string())?,
    );
    request.headers_mut().insert(
        CONTENT_TYPE,
        "application/json"
            .parse()
            .map_err(|_| "构造阿里实时 ASR 请求头失败。".to_string())?,
    );
    let (mut socket, _) =
        connect(request).map_err(|error| format!("连接阿里实时 ASR 失败：{}", error))?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "aliyun realtime asr websocket connected",
    );
    configure_socket_timeout(&socket)?;
    emit_realtime_diagnostic(diagnostic_callback, "aliyun realtime asr run-task sending");
    socket
        .send(Message::Text(
            json!({
                "header": {
                    "action": "run-task",
                    "task_id": task_id,
                    "streaming": "duplex"
                },
                "payload": {
                    "task_group": "audio",
                    "task": "asr",
                    "function": "recognition",
                    "model": model.model_name,
                    "parameters": {
                        "format": "pcm",
                        "sample_rate": REALTIME_ASR_TARGET_SAMPLE_RATE
                    },
                    "input": {}
                }
            })
            .to_string()
            .into(),
        ))
        .map_err(|error| format!("启动阿里实时 ASR 任务失败：{}", error))?;
    emit_realtime_diagnostic(diagnostic_callback, "aliyun realtime asr run-task sent");
    wait_for_aliyun_started(&mut socket, diagnostic_callback)?;
    let mut collector = RealtimeTextCollector::default();
    stream_pcm_frames(
        &mut socket,
        receiver,
        Some(&mut collector),
        partial_callback.as_ref(),
        diagnostic_callback,
    )?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "aliyun realtime asr finish-task sending",
    );
    socket
        .send(Message::Text(
            json!({
                "header": {
                    "action": "finish-task",
                    "task_id": task_id,
                    "streaming": "duplex"
                },
                "payload": {
                    "input": {}
                }
            })
            .to_string()
            .into(),
        ))
        .map_err(|error| format!("结束阿里实时 ASR 任务失败：{}", error))?;
    emit_realtime_diagnostic(diagnostic_callback, "aliyun realtime asr finish-task sent");
    read_json_until_final(
        &mut socket,
        &mut collector,
        partial_callback.as_ref(),
        "阿里实时 ASR",
        diagnostic_callback,
    )?;
    Ok(collector.final_text())
}

/// 调用腾讯云实时 ASR。
/// 流程：解析 AppID/SecretId/SecretKey，按腾讯云 URL 签名规则建连，发送 PCM 二进制帧，结束时发送 `type=end`。
/// 参数：model 为腾讯云实时模型配置，receiver 为 PCM 分片，partial_callback 为临时文本回调。
/// 返回：最终识别文本。
/// 异常/边界：凭证缺字段、签名失败或服务端返回非 0 code 都显式失败。
fn run_tencent_realtime_asr(
    model: &PrivateModelRuntimeRecord,
    receiver: Receiver<AppAudioPcmChunk>,
    partial_callback: Option<RealtimeAsrPartialCallback>,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<String, String> {
    emit_realtime_diagnostic(
        diagnostic_callback,
        "tencent realtime asr credential parsing",
    );
    let credential = RealtimeCredential::parse(&model.api_key)?;
    let app_id = credential.required("appId")?;
    let secret_id = credential.required("secretId")?;
    let secret_key = credential.required("secretKey")?;
    let url = build_tencent_url(
        &model.base_url,
        &model.model_name,
        app_id,
        secret_id,
        secret_key,
    )?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "tencent realtime asr websocket connecting",
    );
    let (mut socket, _) =
        connect(url.as_str()).map_err(|error| format!("连接腾讯云实时 ASR 失败：{}", error))?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "tencent realtime asr websocket connected",
    );
    configure_socket_timeout(&socket)?;
    let mut collector = RealtimeTextCollector::default();
    stream_pcm_frames(
        &mut socket,
        receiver,
        Some(&mut collector),
        partial_callback.as_ref(),
        diagnostic_callback,
    )?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "tencent realtime asr end frame sending",
    );
    socket
        .send(Message::Text(json!({"type": "end"}).to_string().into()))
        .map_err(|error| format!("结束腾讯云实时 ASR 失败：{}", error))?;
    emit_realtime_diagnostic(diagnostic_callback, "tencent realtime asr end frame sent");
    read_json_until_final(
        &mut socket,
        &mut collector,
        partial_callback.as_ref(),
        "腾讯云实时 ASR",
        diagnostic_callback,
    )?;
    Ok(collector.final_text())
}

/// 调用讯飞实时转写。
/// 流程：解析 APPID/APIKey/APISecret，按讯飞 signa 规则建连，发送 PCM 二进制帧，结束时发送二进制 `{"end": true}`。
/// 参数：model 为讯飞实时模型配置，receiver 为 PCM 分片，partial_callback 为临时文本回调。
/// 返回：最终识别文本。
/// 异常/边界：讯飞结果 JSON 中的 data 为字符串，需要二次解析后归并候选词。
fn run_iflytek_realtime_asr(
    model: &PrivateModelRuntimeRecord,
    receiver: Receiver<AppAudioPcmChunk>,
    partial_callback: Option<RealtimeAsrPartialCallback>,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<String, String> {
    emit_realtime_diagnostic(
        diagnostic_callback,
        "iflytek realtime asr credential parsing",
    );
    let credential = RealtimeCredential::parse(&model.api_key)?;
    let app_id = credential.required("appId")?;
    let api_key = credential.required("apiKey")?;
    let api_secret = credential.optional("apiSecret").unwrap_or(api_key);
    let url = build_iflytek_url(&model.base_url, app_id, api_key, api_secret)?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "iflytek realtime asr websocket connecting",
    );
    let (mut socket, _) =
        connect(url.as_str()).map_err(|error| format!("连接讯飞实时转写失败：{}", error))?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "iflytek realtime asr websocket connected",
    );
    configure_socket_timeout(&socket)?;
    let mut collector = RealtimeTextCollector::default();
    stream_pcm_frames(
        &mut socket,
        receiver,
        Some(&mut collector),
        partial_callback.as_ref(),
        diagnostic_callback,
    )?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        "iflytek realtime asr end frame sending",
    );
    socket
        .send(Message::Binary(br#"{"end": true}"#.to_vec().into()))
        .map_err(|error| format!("结束讯飞实时转写失败：{}", error))?;
    emit_realtime_diagnostic(diagnostic_callback, "iflytek realtime asr end frame sent");
    read_json_until_final(
        &mut socket,
        &mut collector,
        partial_callback.as_ref(),
        "讯飞实时转写",
        diagnostic_callback,
    )?;
    Ok(collector.final_text())
}

/// 等待阿里实时 ASR task-started 事件。
/// 流程：持续读取 JSON 消息，直到 event 为 task-started；task-failed 立即失败。
/// 参数：socket 为已连接 WebSocket。
/// 返回：启动成功为空。
/// 异常/边界：超过最终超时时间仍未收到启动事件则失败。
fn wait_for_aliyun_started(
    socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<(), String> {
    emit_realtime_diagnostic(
        diagnostic_callback,
        "aliyun realtime asr waiting task-started",
    );
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < REALTIME_ASR_FINAL_TIMEOUT {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let value = serde_json::from_str::<Value>(&text).map_err(|_| {
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!(
                            "aliyun realtime asr startup invalid json text_bytes={}",
                            text.len()
                        ),
                    );
                    "阿里实时 ASR 启动响应不是合法 JSON。".to_string()
                })?;
                let event = value
                    .pointer("/header/event")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !event.is_empty() {
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!("aliyun realtime asr startup event={}", event),
                    );
                }
                if event == "task-started" {
                    return Ok(());
                }
                if event == "task-failed" {
                    let summary = summarize_provider_error_message(&value);
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!("aliyun realtime asr startup failed task-failed {}", summary),
                    );
                    return Err(format!("阿里实时 ASR 启动任务失败：{}", summary));
                }
            }
            Ok(Message::Close(_)) => return Err("阿里实时 ASR 连接在启动阶段关闭。".to_string()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(tungstenite::Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(format!("读取阿里实时 ASR 启动响应失败：{}", error)),
        }
    }
    Err("等待阿里实时 ASR 启动超时。".to_string())
}

/// 发送实时 PCM 音频帧并穿插读取 partial 文本。
/// 流程：把任意系统采样率单声道 PCM 线性重采样到 16k，再按二进制帧写入 WebSocket；流式阶段使用极短读取超时，避免每帧等待服务端响应拖慢发送。
/// 参数：socket 为 provider 连接，receiver 为录音分片，collector 为可选文本归并器，partial_callback 为 UI 回调。
/// 返回：音频发送完成。
/// 异常/边界：录音结束由 channel 关闭表示；中途服务端关闭连接视为失败。
fn stream_pcm_frames(
    socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    receiver: Receiver<AppAudioPcmChunk>,
    mut collector: Option<&mut RealtimeTextCollector>,
    partial_callback: Option<&RealtimeAsrPartialCallback>,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<(), String> {
    configure_socket_read_timeout(socket, REALTIME_ASR_STREAM_READ_TIMEOUT)?;
    let mut resampler = Pcm16kResampler::default();
    let mut input_chunks = 0usize;
    let mut sent_frames = 0usize;
    let mut sent_bytes = 0usize;
    let mut source_sample_rate = 0u32;
    let started_at = std::time::Instant::now();
    emit_realtime_diagnostic(diagnostic_callback, "realtime asr pcm stream started");
    while let Ok(chunk) = receiver.recv() {
        input_chunks += 1;
        source_sample_rate = chunk.sample_rate;
        let pcm_bytes = resampler.push_chunk(&chunk);
        if !pcm_bytes.is_empty() {
            sent_frames += 1;
            sent_bytes += pcm_bytes.len();
            if let Err(error) = socket.send(Message::Binary(pcm_bytes.into())) {
                emit_realtime_diagnostic(
                    diagnostic_callback,
                    &format!(
                        "realtime asr pcm frame send failed frames={} bytes={} error={}",
                        sent_frames,
                        sent_bytes,
                        sanitize_diagnostic_value(&error.to_string())
                    ),
                );
                return Err(format!("发送实时 ASR 音频帧失败：{}", error));
            }
            if sent_frames == 1 || sent_frames % 25 == 0 {
                emit_realtime_diagnostic(
                    diagnostic_callback,
                    &format!(
                        "realtime asr pcm frame sent frames={} bytes={} source_sample_rate={}",
                        sent_frames, sent_bytes, source_sample_rate
                    ),
                );
            }
        }
        if sent_frames == 1 || sent_frames % REALTIME_ASR_PARTIAL_POLL_FRAME_INTERVAL == 0 {
            while let Some(value) = try_read_json(socket, diagnostic_callback)? {
                if let Some(collector) = collector.as_deref_mut() {
                    collector.accept(&value, partial_callback);
                }
            }
        }
    }
    let pcm_bytes = resampler.finish();
    if !pcm_bytes.is_empty() {
        sent_frames += 1;
        sent_bytes += pcm_bytes.len();
        if let Err(error) = socket.send(Message::Binary(pcm_bytes.into())) {
            emit_realtime_diagnostic(
                diagnostic_callback,
                &format!(
                    "realtime asr final pcm frame send failed frames={} bytes={} error={}",
                    sent_frames,
                    sent_bytes,
                    sanitize_diagnostic_value(&error.to_string())
                ),
            );
            return Err(format!("发送实时 ASR 最后一段音频失败：{}", error));
        }
    }
    emit_realtime_diagnostic(
        diagnostic_callback,
        &format!(
            "realtime asr pcm stream finished chunks={} frames={} bytes={} source_sample_rate={} elapsed_ms={}",
            input_chunks,
            sent_frames,
            sent_bytes,
            source_sample_rate,
            started_at.elapsed().as_millis()
        ),
    );
    Ok(())
}

/// 读取 provider JSON 消息直到 final 或超时。
/// 流程：持续读取文本消息，归并可识别字段；连接关闭且已有文本时视为成功。
/// 参数：socket 为 provider 连接，collector 为文本归并器，partial_callback 为 UI 回调，provider_label 为错误文案厂商名。
/// 返回：读取完成。
/// 异常/边界：超时但已有文本时允许返回，完全无文本时失败。
fn read_json_until_final(
    socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    collector: &mut RealtimeTextCollector,
    partial_callback: Option<&RealtimeAsrPartialCallback>,
    provider_label: &str,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<(), String> {
    configure_socket_read_timeout(socket, REALTIME_ASR_READ_TIMEOUT)?;
    emit_realtime_diagnostic(
        diagnostic_callback,
        &format!(
            "realtime asr final result waiting provider={}",
            provider_label
        ),
    );
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < REALTIME_ASR_FINAL_TIMEOUT {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let value = serde_json::from_str::<Value>(&text).map_err(|_| {
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!(
                            "realtime asr invalid json provider={} text_bytes={}",
                            provider_label,
                            text.len()
                        ),
                    );
                    format!("{} 返回了非法 JSON。", provider_label)
                })?;
                collector.accept(&value, partial_callback);
                if is_final_message(&value) && !collector.final_text().is_empty() {
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!(
                            "realtime asr final message received provider={} text_chars={}",
                            provider_label,
                            collector.final_text().chars().count()
                        ),
                    );
                    return Ok(());
                }
                if is_provider_error_message(&value) {
                    let summary = summarize_provider_error_message(&value);
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!(
                            "realtime asr provider error message provider={} {}",
                            provider_label, summary
                        ),
                    );
                    return Err(format!("{} 返回识别错误：{}", provider_label, summary));
                }
            }
            Ok(Message::Close(_)) => {
                if !collector.final_text().is_empty() {
                    emit_realtime_diagnostic(
                        diagnostic_callback,
                        &format!(
                            "realtime asr socket closed with text provider={} text_chars={}",
                            provider_label,
                            collector.final_text().chars().count()
                        ),
                    );
                    return Ok(());
                }
                emit_realtime_diagnostic(
                    diagnostic_callback,
                    &format!(
                        "realtime asr socket closed without text provider={}",
                        provider_label
                    ),
                );
                return Err(format!("{} 连接已关闭但没有返回文本。", provider_label));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(tungstenite::Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut => {
                if !collector.final_text().is_empty() {
                    return Ok(());
                }
            }
            Err(error) => return Err(format!("读取{}识别结果失败：{}", provider_label, error)),
        }
    }
    if collector.final_text().is_empty() {
        emit_realtime_diagnostic(
            diagnostic_callback,
            &format!(
                "realtime asr final result timeout empty provider={} elapsed_ms={}",
                provider_label,
                started_at.elapsed().as_millis()
            ),
        );
        Err(format!("等待{}最终识别结果超时。", provider_label))
    } else {
        emit_realtime_diagnostic(
            diagnostic_callback,
            &format!(
                "realtime asr final result timeout with text provider={} text_chars={} elapsed_ms={}",
                provider_label,
                collector.final_text().chars().count(),
                started_at.elapsed().as_millis()
            ),
        );
        Ok(())
    }
}

/// 输出实时 ASR 诊断日志。
/// 流程：存在回调时投递不含凭证和原始响应的阶段消息。
/// 参数：diagnostic_callback 为可选诊断回调，message 为已脱敏消息。
/// 返回：无返回值。
/// 异常/边界：回调内部失败不影响实时识别链路。
fn emit_realtime_diagnostic(
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
    message: &str,
) {
    if let Some(callback) = diagnostic_callback {
        callback(message.to_string());
    }
}

/// 生成 provider 错误摘要。
/// 流程：只读取错误码、事件、状态和短错误文案，不保留原始响应体、签名、凭证或音频内容。
/// 参数：value 为 provider 返回的 JSON。
/// 返回：适合写入本机诊断日志的脱敏摘要。
/// 异常/边界：未知结构返回固定占位，避免把完整 JSON 写进日志。
fn summarize_provider_error_message(value: &Value) -> String {
    let mut parts = Vec::new();
    for (label, pointer) in [
        ("event", "/header/event"),
        ("header_code", "/header/error_code"),
        ("header_message", "/header/error_message"),
        ("status", "/header/status"),
        ("code", "/code"),
        ("message", "/message"),
        ("error", "/error"),
        ("reason", "/reason"),
        ("desc", "/desc"),
        ("action", "/action"),
    ] {
        push_json_field_summary(&mut parts, label, value.pointer(pointer));
    }
    if let Some(data) = parse_iflytek_data(value) {
        for (label, pointer) in [
            ("iflytek_code", "/code"),
            ("iflytek_desc", "/desc"),
            ("iflytek_message", "/message"),
            ("iflytek_action", "/action"),
        ] {
            push_json_field_summary(&mut parts, label, data.pointer(pointer));
        }
    }
    if parts.is_empty() {
        "provider_error=unknown".to_string()
    } else {
        parts.join(" ")
    }
}

/// 追加一个安全 JSON 字段摘要。
/// 流程：只接受字符串、数字、布尔值并做长度截断与换行清理。
/// 参数：parts 为摘要集合，label 为字段标签，value 为可选 JSON 值。
/// 返回：无返回值。
/// 异常/边界：对象、数组和空值不写入，防止日志膨胀。
fn push_json_field_summary(parts: &mut Vec<String>, label: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    let raw = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        _ => String::new(),
    };
    if raw.is_empty() {
        return;
    }
    parts.push(format!("{}={}", label, sanitize_diagnostic_value(&raw)));
}

/// 清理诊断字段值。
/// 流程：移除换行和制表符，再把过长内容截断到短摘要长度。
/// 参数：value 为准备写入日志的值。
/// 返回：单行短文本。
/// 异常/边界：该函数不处理原始响应体，只用于已筛选字段的二次保护。
fn sanitize_diagnostic_value(value: &str) -> String {
    const MAX_DIAGNOSTIC_VALUE_CHARS: usize = 180;
    let normalized = value
        .chars()
        .map(|item| {
            if matches!(item, '\r' | '\n' | '\t') {
                ' '
            } else {
                item
            }
        })
        .collect::<String>();
    let trimmed = normalized.trim();
    if trimmed.chars().count() <= MAX_DIAGNOSTIC_VALUE_CHARS {
        return trimmed.to_string();
    }
    let prefix = trimmed
        .chars()
        .take(MAX_DIAGNOSTIC_VALUE_CHARS)
        .collect::<String>();
    format!("{}...", prefix)
}

/// 非阻塞读取一个 JSON 消息。
/// 流程：读取 WebSocket 文本消息并解析为 JSON；没有消息时返回 None。
/// 参数：socket 为 provider 连接。
/// 返回：可选 JSON。
/// 异常/边界：二进制或 ping/pong 消息忽略；连接关闭视为无消息。
fn try_read_json(
    socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    diagnostic_callback: Option<&RealtimeAsrDiagnosticCallback>,
) -> Result<Option<Value>, String> {
    match socket.read() {
        Ok(Message::Text(text)) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => Ok(Some(value)),
            Err(_) => {
                emit_realtime_diagnostic(
                    diagnostic_callback,
                    &format!(
                        "realtime asr partial invalid json text_bytes={}",
                        text.len()
                    ),
                );
                Ok(None)
            }
        },
        Ok(Message::Close(_)) => Err("实时 ASR 连接已关闭。".to_string()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Ok(None)
        }
        Err(tungstenite::Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut => {
            Ok(None)
        }
        Err(error) => Err(format!("读取实时 ASR 消息失败：{}", error)),
    }
}

/// 构建腾讯云实时 ASR 签名 URL。
/// 流程：按固定参数生成排序查询串，对 `host/path?query` 做 HMAC-SHA1 后写入 signature。
/// 参数：base_url 为腾讯 ASR WebSocket 根地址，model_name 为 engine_model_type，其余为腾讯云凭证。
/// 返回：完整 WebSocket URL。
/// 异常/边界：URL 主机或 AppID 缺失时失败；返回值不包含 SecretKey。
fn build_tencent_url(
    base_url: &str,
    model_name: &str,
    app_id: &str,
    secret_id: &str,
    secret_key: &str,
) -> Result<Url, String> {
    let mut base = normalize_ws_url(base_url)?;
    let host = base
        .host_str()
        .ok_or_else(|| "腾讯云实时 ASR 地址缺少主机。".to_string())?
        .to_string();
    base.set_path(&format!("/asr/v2/{}", app_id));
    let now = unix_seconds();
    let mut params = BTreeMap::new();
    params.insert("engine_model_type", model_name.trim().to_string());
    params.insert("expired", (now + 24 * 60 * 60).to_string());
    params.insert("filter_dirty", "0".to_string());
    params.insert("filter_modal", "0".to_string());
    params.insert("filter_punc", "0".to_string());
    params.insert("needvad", "1".to_string());
    params.insert("nonce", now.to_string());
    params.insert("secretid", secret_id.trim().to_string());
    params.insert("timestamp", now.to_string());
    params.insert("voice_format", "1".to_string());
    params.insert("voice_id", uuid::Uuid::new_v4().to_string());
    let query = encode_query(&params);
    let sign_source = format!("{}{}?{}", host, base.path(), query);
    let signature = hmac_sha1_base64(secret_key.trim().as_bytes(), sign_source.as_bytes())?;
    params.insert("signature", signature);
    base.set_query(Some(&encode_query(&params)));
    Ok(base)
}

/// 构建讯飞实时转写签名 URL。
/// 流程：signa = base64(hmac_sha1(secret, md5(appid + ts)))，并把 appid、ts、signa 写入查询串。
/// 参数：base_url 为讯飞 WebSocket 地址，app_id/api_key/api_secret 为讯飞凭证字段。
/// 返回：完整 WebSocket URL。
/// 异常/边界：不在错误中输出签名或密钥。
fn build_iflytek_url(
    base_url: &str,
    app_id: &str,
    api_key: &str,
    api_secret: &str,
) -> Result<Url, String> {
    let mut url = normalize_ws_url(base_url)?;
    let ts = unix_seconds().to_string();
    let md5_source = format!("{}{}", app_id.trim(), ts);
    let checksum = format!("{:x}", md5::compute(md5_source.as_bytes()));
    let signa = hmac_sha1_base64(api_secret.trim().as_bytes(), checksum.as_bytes())
        .or_else(|_| hmac_sha1_base64(api_key.trim().as_bytes(), checksum.as_bytes()))?;
    let mut params = BTreeMap::new();
    params.insert("appid", app_id.trim().to_string());
    params.insert("ts", ts);
    params.insert("signa", signa);
    url.set_query(Some(&encode_query(&params)));
    Ok(url)
}

/// 归一化并校验 WebSocket URL。
/// 流程：解析地址并只允许 ws/wss scheme。
/// 参数：value 为模型配置的 baseUrl。
/// 返回：合法 WebSocket URL。
/// 异常/边界：HTTP Base URL 会被拒绝，防止实时 provider 错用批量接口地址。
fn normalize_ws_url(value: &str) -> Result<Url, String> {
    let url =
        Url::parse(value.trim()).map_err(|_| "实时 ASR WebSocket 地址格式无效。".to_string())?;
    if !matches!(url.scheme(), "ws" | "wss") {
        return Err("实时 ASR provider 必须使用 ws 或 wss 地址。".to_string());
    }
    Ok(url)
}

/// 设置 WebSocket 底层 TCP 读写超时。
/// 流程：尽量从 plain/rustls 底层连接取出 TCP 并设置超时，避免 worker 无限挂起。
/// 参数：socket 为已经连接的 WebSocket。
/// 返回：设置成功为空。
/// 异常/边界：未来新增底层类型时保守跳过，由 provider 总超时兜底。
fn configure_socket_timeout(
    socket: &WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
) -> Result<(), String> {
    match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(stream) => configure_tcp_timeout(stream),
        tungstenite::stream::MaybeTlsStream::Rustls(stream) => {
            configure_tcp_timeout(stream.get_ref())
        }
        _ => Ok(()),
    }
}

/// 设置 WebSocket 读取超时。
/// 流程：流式发送阶段切到极短超时读取 partial，最终结果阶段再恢复常规超时。
/// 参数：socket 为 provider 连接，timeout 为目标读取超时。
/// 返回：设置成功为空。
/// 异常/边界：未来新增底层类型时保守跳过，由 provider 总超时兜底。
fn configure_socket_read_timeout(
    socket: &WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> Result<(), String> {
    match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(stream) => stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| format!("设置实时 ASR 读取超时失败：{}", error)),
        tungstenite::stream::MaybeTlsStream::Rustls(stream) => stream
            .get_ref()
            .set_read_timeout(Some(timeout))
            .map_err(|error| format!("设置实时 ASR 读取超时失败：{}", error)),
        _ => Ok(()),
    }
}

/// 设置 TCP 读写超时。
/// 流程：分别写入 read_timeout 和 write_timeout。
/// 参数：stream 为 WebSocket 底层 TCP 连接。
/// 返回：设置成功为空。
/// 异常/边界：系统拒绝设置时返回脱敏错误。
fn configure_tcp_timeout(stream: &TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(REALTIME_ASR_READ_TIMEOUT))
        .map_err(|error| format!("设置实时 ASR 读取超时失败：{}", error))?;
    stream
        .set_write_timeout(Some(REALTIME_ASR_CONNECT_TIMEOUT))
        .map_err(|error| format!("设置实时 ASR 写入超时失败：{}", error))?;
    Ok(())
}

/// 对查询参数做稳定编码。
/// 流程：按 BTreeMap 排序顺序写入 query，保证签名串稳定。
/// 参数：params 为查询参数。
/// 返回：URL query 字符串。
/// 异常/边界：不会输出空 key。
fn encode_query(params: &BTreeMap<&str, String>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

/// 计算 HMAC-SHA1 并 Base64 编码。
/// 流程：使用指定密钥签名原文，再输出标准 Base64。
/// 参数：key 为密钥字节，payload 为待签名原文。
/// 返回：Base64 签名。
/// 异常/边界：空密钥由 hmac crate 接受，调用方已做必填校验。
fn hmac_sha1_base64(key: &[u8], payload: &[u8]) -> Result<String, String> {
    let mut mac =
        HmacSha1::new_from_slice(key).map_err(|_| "初始化实时 ASR 签名失败。".to_string())?;
    mac.update(payload);
    Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

/// 获取当前 Unix 秒。
/// 流程：从系统时间转换为秒数，失败时回退为 0。
/// 参数：无。
/// 返回：Unix 时间戳秒。
/// 异常/边界：系统时间早于 epoch 时返回 0。
fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// 实时 provider 凭证解析器。
/// 流程：优先解析 JSON；失败时支持 key=value 多行或分号格式，便于用户在单一密钥框中粘贴多字段。
/// 参数：raw 为模型管理中保存的凭证字段。
/// 返回：归一化凭证映射。
/// 异常/边界：密钥原文不会写入错误。
struct RealtimeCredential {
    /// 归一化后的凭证字段。
    fields: BTreeMap<String, String>,
}

impl RealtimeCredential {
    /// 解析实时 ASR 凭证。
    /// 流程：JSON 对象优先，其次解析 `key=value` 文本，字段名按大小写不敏感处理。
    /// 参数：raw 为用户填写的凭证。
    /// 返回：凭证结构。
    /// 异常/边界：无法解析时返回字段格式提示，不泄露原文。
    fn parse(raw: &str) -> Result<Self, String> {
        let normalized = raw.trim();
        if normalized.is_empty() {
            return Err("实时 ASR 凭证不能为空。".to_string());
        }
        if normalized.starts_with('{') {
            let value = serde_json::from_str::<Value>(normalized)
                .map_err(|_| "实时 ASR 凭证 JSON 格式无效。".to_string())?;
            let object = value
                .as_object()
                .ok_or_else(|| "实时 ASR 凭证必须是 JSON 对象。".to_string())?;
            let fields = object
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|text| (normalize_credential_key(key), text.trim().to_string()))
                })
                .filter(|(_, value)| !value.is_empty())
                .collect::<BTreeMap<_, _>>();
            return Ok(Self { fields });
        }
        let mut fields = BTreeMap::new();
        for line in normalized.split(['\n', ';', ',']) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let normalized_key = normalize_credential_key(key);
            let normalized_value = value.trim().to_string();
            if !normalized_key.is_empty() && !normalized_value.is_empty() {
                fields.insert(normalized_key, normalized_value);
            }
        }
        if fields.is_empty() {
            return Err("实时 ASR 凭证格式无效，请使用 JSON 或 key=value 多字段格式。".to_string());
        }
        Ok(Self { fields })
    }

    /// 读取必填凭证字段。
    /// 流程：按归一化字段名查找并返回字符串引用。
    /// 参数：key 为业务字段名。
    /// 返回：字段值。
    /// 异常/边界：缺字段时只提示字段名，不暴露其它凭证内容。
    fn required(&self, key: &str) -> Result<&str, String> {
        self.fields
            .get(&normalize_credential_key(key))
            .map(String::as_str)
            .ok_or_else(|| format!("实时 ASR 凭证缺少 {} 字段。", key))
    }

    /// 读取可选凭证字段。
    /// 流程：按归一化字段名查找并返回字符串引用。
    /// 参数：key 为业务字段名。
    /// 返回：可选字段值。
    /// 异常/边界：缺字段返回 None。
    fn optional(&self, key: &str) -> Option<&str> {
        self.fields
            .get(&normalize_credential_key(key))
            .map(String::as_str)
    }
}

/// 归一化凭证字段名。
/// 流程：去掉空格、下划线和短横线后转小写。
/// 参数：key 为用户输入字段名。
/// 返回：稳定字段名。
/// 异常/边界：未知字段仍保留，具体 provider 再判断是否必填。
fn normalize_credential_key(key: &str) -> String {
    key.trim()
        .chars()
        .filter(|ch| !matches!(*ch, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// 16k PCM 重采样器。
/// 流程：缓存输入样本并使用线性插值从系统采样率转换到 16k，输出小端 i16 PCM 字节。
/// 参数：通过 push_chunk 持续追加输入。
/// 返回：可直接发给实时 ASR 的 PCM bytes。
/// 异常/边界：采样率变化时重置缓存，避免错用旧 ratio。
#[derive(Default)]
struct Pcm16kResampler {
    /// 当前输入采样率。
    source_sample_rate: u32,
    /// 待重采样缓存。
    pending_samples: Vec<i16>,
    /// 下一个输出采样点在 pending_samples 中的浮点位置。
    cursor: f64,
}

impl Pcm16kResampler {
    /// 追加一个系统 PCM 分片并输出 16k PCM。
    /// 流程：必要时重置状态，随后线性插值并排出已消费样本。
    /// 参数：chunk 为系统录音分片。
    /// 返回：16k little-endian PCM 字节。
    /// 异常/边界：空分片或采样率异常返回空字节。
    fn push_chunk(&mut self, chunk: &AppAudioPcmChunk) -> Vec<u8> {
        if chunk.samples.is_empty() || chunk.sample_rate == 0 {
            return Vec::new();
        }
        if self.source_sample_rate != chunk.sample_rate {
            self.source_sample_rate = chunk.sample_rate;
            self.pending_samples.clear();
            self.cursor = 0.0;
        }
        self.pending_samples.extend_from_slice(&chunk.samples);
        self.drain_available(false)
    }

    /// 输出最后可用的一段重采样 PCM。
    /// 流程：尽可能消费剩余输入样本，不额外补零。
    /// 参数：无。
    /// 返回：16k little-endian PCM 字节。
    /// 异常/边界：剩余样本少于两个时返回空字节。
    fn finish(&mut self) -> Vec<u8> {
        self.drain_available(true)
    }

    /// 根据当前缓存执行重采样。
    /// 流程：以 source/target ratio 推进 cursor，线性插值得到输出样本，再丢弃安全消费过的输入。
    /// 参数：finishing 为是否录音结束。
    /// 返回：16k little-endian PCM 字节。
    /// 异常/边界：为保证插值需要保留最后一个输入样本到下一轮。
    fn drain_available(&mut self, finishing: bool) -> Vec<u8> {
        if self.source_sample_rate == 0 || self.pending_samples.len() < 2 {
            return Vec::new();
        }
        let step = f64::from(self.source_sample_rate) / f64::from(REALTIME_ASR_TARGET_SAMPLE_RATE);
        let end = if finishing {
            self.pending_samples.len().saturating_sub(1) as f64
        } else {
            self.pending_samples.len().saturating_sub(2) as f64
        };
        let mut bytes = Vec::new();
        while self.cursor < end {
            let left_index = self.cursor.floor() as usize;
            let right_index = (left_index + 1).min(self.pending_samples.len() - 1);
            let fraction = self.cursor - left_index as f64;
            let left = f64::from(self.pending_samples[left_index]);
            let right = f64::from(self.pending_samples[right_index]);
            let sample = (left + (right - left) * fraction)
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
            self.cursor += step;
        }
        let consumed = self.cursor.floor() as usize;
        if consumed > 0 {
            let keep_from = consumed.min(self.pending_samples.len().saturating_sub(1));
            self.pending_samples.drain(0..keep_from);
            self.cursor -= keep_from as f64;
        }
        bytes
    }
}

/// 实时 ASR 文本归并器。
/// 流程：从各厂商 JSON 中提取候选文本；final 文本优先，partial 文本用于 UI 反馈。
/// 参数：通过 accept 持续接收 provider JSON。
/// 返回：final_text 汇总最终结果。
#[derive(Default)]
struct RealtimeTextCollector {
    /// 已确认文本片段。
    final_segments: Vec<String>,
    /// 最近一次临时文本。
    partial_text: String,
}

impl RealtimeTextCollector {
    /// 接收一条 provider JSON。
    /// 流程：优先识别腾讯/讯飞结构化结果，再回退扫描常见 text 字段；final 消息追加，partial 消息覆盖。
    /// 参数：value 为 provider JSON，partial_callback 为 UI 回调。
    /// 返回：无返回值。
    /// 异常/边界：无法识别文本时静默忽略，不中断流。
    fn accept(&mut self, value: &Value, partial_callback: Option<&RealtimeAsrPartialCallback>) {
        if let Some(text) = extract_iflytek_text(value).or_else(|| extract_common_text(value)) {
            let normalized = text.trim();
            if normalized.is_empty() {
                return;
            }
            if is_final_message(value) {
                if self
                    .final_segments
                    .last()
                    .is_none_or(|last| last != normalized)
                {
                    self.final_segments.push(normalized.to_string());
                }
            } else {
                self.partial_text = normalized.to_string();
                if let Some(callback) = partial_callback {
                    callback(self.preview_text());
                }
            }
        }
    }

    /// 获取最终文本。
    /// 流程：优先拼接 final 片段，没有 final 时返回最近 partial。
    /// 参数：无。
    /// 返回：识别文本。
    /// 异常/边界：不会返回首尾空白。
    fn final_text(&self) -> String {
        if self.final_segments.is_empty() {
            return self.partial_text.trim().to_string();
        }
        self.final_segments.join("").trim().to_string()
    }

    /// 获取预览文本。
    /// 流程：final 片段加最新 partial，供浮窗 title 更新。
    /// 参数：无。
    /// 返回：临时展示文本。
    /// 异常/边界：不修改 final 数据。
    fn preview_text(&self) -> String {
        format!("{}{}", self.final_segments.join(""), self.partial_text)
            .trim()
            .to_string()
    }
}

/// 判断 provider 消息是否为 final。
/// 流程：兼容阿里 event、腾讯 final/slice_type/type 和讯飞 cn.st.type 字段。
/// 参数：value 为 provider JSON。
/// 返回：确认最终片段或结束消息时返回 true。
fn is_final_message(value: &Value) -> bool {
    let event = value
        .pointer("/header/event")
        .and_then(Value::as_str)
        .unwrap_or("");
    if matches!(event, "task-finished" | "result-generated") {
        if value
            .pointer("/payload/output/sentence/sentence_end")
            .and_then(Value::as_bool)
            .unwrap_or(event == "task-finished")
        {
            return true;
        }
    }
    if value
        .get("final")
        .and_then(Value::as_i64)
        .is_some_and(|item| item == 1)
    {
        return true;
    }
    if value
        .get("slice_type")
        .and_then(Value::as_i64)
        .is_some_and(|item| item == 2)
    {
        return true;
    }
    if value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|item| item == "end")
    {
        return true;
    }
    if let Some(data) = parse_iflytek_data(value) {
        return data
            .pointer("/cn/st/type")
            .and_then(Value::as_str)
            .is_some_and(|item| item == "0");
    }
    false
}

/// 判断 provider 是否返回错误消息。
/// 流程：检查常见 code/status/event 字段。
/// 参数：value 为 provider JSON。
/// 返回：明确错误返回 true。
fn is_provider_error_message(value: &Value) -> bool {
    if value.pointer("/header/event").and_then(Value::as_str) == Some("task-failed") {
        return true;
    }
    if let Some(code) = value.get("code").and_then(Value::as_i64) {
        return code != 0;
    }
    false
}

/// 提取通用实时 ASR 文本字段。
/// 流程：优先读取三家常见路径，最后递归查找字段名为 text 或 voice_text_str 的字符串。
/// 参数：value 为 provider JSON。
/// 返回：候选文本。
fn extract_common_text(value: &Value) -> Option<String> {
    for pointer in [
        "/payload/output/sentence/text",
        "/payload/output/text",
        "/result/voice_text_str",
        "/voice_text_str",
        "/text",
    ] {
        if let Some(text) = value.pointer(pointer).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }
    }
    find_text_by_key(value, &["voice_text_str", "text"])
}

/// 提取讯飞实时转写 data 字段中的文本。
/// 流程：讯飞外层 data 通常是 JSON 字符串，解析后拼接 cn.st.rt.ws.cw.w。
/// 参数：value 为讯飞外层 JSON。
/// 返回：候选文本。
fn extract_iflytek_text(value: &Value) -> Option<String> {
    let data = parse_iflytek_data(value)?;
    let words = data
        .pointer("/cn/st/rt")
        .and_then(Value::as_array)?
        .iter()
        .flat_map(|rt| rt.get("ws").and_then(Value::as_array).into_iter().flatten())
        .flat_map(|ws| ws.get("cw").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|cw| cw.get("w").and_then(Value::as_str))
        .collect::<String>();
    if words.trim().is_empty() {
        None
    } else {
        Some(words)
    }
}

/// 解析讯飞 data 字符串。
/// 流程：读取外层 data 字段并按 JSON 二次解析。
/// 参数：value 为讯飞外层 JSON。
/// 返回：内层 JSON。
fn parse_iflytek_data(value: &Value) -> Option<Value> {
    let data = value.get("data")?.as_str()?;
    serde_json::from_str::<Value>(data).ok()
}

/// 在 JSON 树中按字段名递归查找文本。
/// 流程：深度优先遍历对象和数组，命中指定字段且值为字符串时返回。
/// 参数：value 为 JSON，keys 为允许的字段名。
/// 返回：第一个非空文本。
fn find_text_by_key(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(text) = object.get(*key).and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
            object
                .values()
                .find_map(|item| find_text_by_key(item, keys))
        }
        Value::Array(items) => items.iter().find_map(|item| find_text_by_key(item, keys)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 腾讯云实时 ASR 签名 URL 必须包含模型、voice_id 与 signature 参数。
    #[test]
    fn tencent_url_contains_required_signed_parameters() {
        let url = build_tencent_url(
            "wss://asr.cloud.tencent.com/asr/v2",
            "16k_zh",
            "123456",
            "sid",
            "skey",
        )
        .expect("腾讯签名 URL 应可生成");
        let query = url.query().expect("腾讯 URL 必须包含查询参数");
        assert!(url
            .as_str()
            .starts_with("wss://asr.cloud.tencent.com/asr/v2/123456?"));
        assert!(query.contains("engine_model_type=16k_zh"));
        assert!(query.contains("voice_id="));
        assert!(query.contains("signature="));
    }

    /// 讯飞实时转写签名 URL 必须包含 appid、ts 与 signa。
    #[test]
    fn iflytek_url_contains_required_signed_parameters() {
        let url = build_iflytek_url(
            "wss://rtasr.xfyun.cn/v1/ws",
            "appid",
            "api_key",
            "api_secret",
        )
        .expect("讯飞签名 URL 应可生成");
        let query = url.query().expect("讯飞 URL 必须包含查询参数");
        assert!(query.contains("appid=appid"));
        assert!(query.contains("ts="));
        assert!(query.contains("signa="));
    }

    /// 实时 ASR 凭证支持 JSON 和 key=value 两种格式。
    #[test]
    fn realtime_credential_accepts_json_and_key_value() {
        let json_credential =
            RealtimeCredential::parse(r#"{"appId":"a","secretId":"b","secretKey":"c"}"#)
                .expect("JSON 凭证应可解析");
        assert_eq!(json_credential.required("appId").unwrap(), "a");
        let text_credential = RealtimeCredential::parse("appId=a;secretId=b;secretKey=c")
            .expect("key=value 凭证应可解析");
        assert_eq!(text_credential.required("secretKey").unwrap(), "c");
    }

    /// 讯飞 data 字符串应能归并候选词。
    #[test]
    fn iflytek_text_can_be_extracted_from_nested_data_string() {
        let value = json!({
            "code": 0,
            "data": "{\"cn\":{\"st\":{\"type\":\"0\",\"rt\":[{\"ws\":[{\"cw\":[{\"w\":\"你好\"}]},{\"cw\":[{\"w\":\"世界\"}]}]}]}}}"
        });
        assert_eq!(extract_iflytek_text(&value).unwrap(), "你好世界");
        assert!(is_final_message(&value));
    }

    /// provider 错误摘要只保留安全字段，不写入完整响应体或无关字段。
    #[test]
    fn provider_error_summary_keeps_only_sanitized_fields() {
        let value = json!({
            "header": {
                "event": "task-failed",
                "error_code": "InvalidApiKey",
                "error_message": "key expired\nplease rotate"
            },
            "apiKey": "should-not-appear",
            "payload": {
                "raw": "should-not-appear"
            }
        });
        let summary = summarize_provider_error_message(&value);

        assert!(summary.contains("event=task-failed"));
        assert!(summary.contains("header_code=InvalidApiKey"));
        assert!(summary.contains("header_message=key expired please rotate"));
        assert!(!summary.contains("should-not-appear"));
        assert!(!summary.contains('\n'));
    }

    /// 任意系统采样率输入都应转为 16k little-endian PCM。
    #[test]
    fn pcm_resampler_outputs_little_endian_16k_samples() {
        let mut resampler = Pcm16kResampler::default();
        let bytes = resampler.push_chunk(&AppAudioPcmChunk {
            sample_rate: 48_000,
            samples: vec![0; 4_800],
        });
        assert!(bytes.len() > 1_000);
        assert_eq!(bytes.len() % 2, 0);
    }
}
