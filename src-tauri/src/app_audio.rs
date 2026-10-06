use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use uuid::Uuid;

const RECORDING_READY_FRAME_COUNT: usize = 12;
const RECORDING_READY_PREROLL_TIMEOUT_MS: u64 = 1_600;
const SPEECH_SAMPLE_THRESHOLD: i32 = 1_147;
const AUTO_GAIN_TARGET_RMS: f64 = 1_650.0;
const AUTO_GAIN_MAX: f64 = 6.0;
const AUTO_GAIN_MIN_PEAK: i32 = 420;
const AUTO_GAIN_OUTPUT_PEAK_LIMIT: f64 = 28_000.0;

/// App 主进程录音结果；音频只保存在内存中，不写入持久配置。
pub struct AppAudioRecord {
    /// WAV 音频字节。
    pub bytes: Vec<u8>,
    /// 音频 MIME 类型。
    pub content_type: String,
    /// 实际采样得到的时长，毫秒。
    pub duration_ms: u64,
    /// 录音波形诊断摘要；不包含原始音频内容。
    pub diagnostics: AppAudioDiagnostics,
}

/// App 录音波形诊断信息，用于判断是否真的收到了麦克风声音。
pub struct AppAudioDiagnostics {
    /// 系统默认输入设备名称。
    pub device_name: String,
    /// 系统输入设备采样率。
    pub sample_rate: u32,
    /// 系统输入设备声道数。
    pub channel_count: usize,
    /// 系统输入设备采样格式。
    pub sample_format: String,
    /// 单声道样本数。
    pub sample_count: usize,
    /// 样本峰值，范围为 0 到 32768。
    pub peak_amplitude: i16,
    /// 均方根音量，范围约为 0 到 32768。
    pub rms_amplitude: f64,
    /// 超过人声粗略阈值的样本占比，范围为 0 到 1。
    pub active_sample_ratio: f64,
    /// 本次录音是否由用户再次按快捷键停止。
    pub stopped_by_request: bool,
    /// 本次录音过程中使用过的最大自动增益倍数。
    pub max_auto_gain: f64,
    /// 本次录音所有输入帧的平均自动增益倍数。
    pub average_auto_gain: f64,
}

/// App 录音实时音量回调；只传输归一化统计值，不传输原始音频。
pub type AppAudioLevelCallback = Arc<dyn Fn(AppAudioLevel) + Send + Sync + 'static>;

/// App 录音首帧就绪回调；用于确保 UI 只在真实收到麦克风采样后进入录音态。
pub type AppAudioReadyCallback = Arc<dyn Fn() + Send + Sync + 'static>;

/// App 录音 PCM 分片回调；只在实时 ASR 链路中把内存音频交给 provider worker。
pub type AppAudioPcmCallback = Arc<dyn Fn(AppAudioPcmChunk) + Send + Sync + 'static>;

/// App 录音实时音量事件，用于驱动悬浮窗波纹反馈。
#[derive(Clone, Copy)]
pub struct AppAudioLevel {
    /// 归一化 RMS 音量，范围 0 到 1。
    pub rms_level: f64,
    /// 归一化峰值音量，范围 0 到 1。
    pub peak_level: f64,
}

/// App 录音 PCM 分片；样本为单声道 i16，采样率沿用系统输入设备。
pub struct AppAudioPcmChunk {
    /// 单声道 PCM 样本。
    pub samples: Vec<i16>,
    /// 当前分片采样率。
    pub sample_rate: u32,
}

/// 使用 CodexMan 主进程录制麦克风 WAV，并可同步把 PCM 分片交给实时 ASR。
/// 流程：沿用标准录音链路采集、诊断和返回 WAV，同时在输入回调内把已混合的单声道 PCM 复制给实时 provider。
/// 参数：除标准录音参数外，pcm_callback 用于实时 ASR worker 接收分片。
/// 返回：包含 WAV 字节、MIME 和实际时长的录音结果。
/// 异常/边界：PCM 回调内部不得阻塞 CoreAudio 输入线程；录音仍以标准 WAV 结果作为最终诊断依据。
pub fn record_microphone_wav_with_pcm_callback(
    max_duration_ms: u64,
    stop_requested: Arc<AtomicBool>,
    level_callback: Option<AppAudioLevelCallback>,
    ready_callback: Option<AppAudioReadyCallback>,
    pcm_callback: Option<AppAudioPcmCallback>,
) -> Result<AppAudioRecord, String> {
    if max_duration_ms == 0 || max_duration_ms > 600_000 {
        return Err("录音时长必须在 1 毫秒到 10 分钟之间。".to_string());
    }

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "没有找到可用麦克风，请检查系统输入设备。".to_string())?;
    let supported_config = device
        .default_input_config()
        .map_err(|error| format!("读取默认麦克风配置失败：{}", error))?;
    let device_name = device.name().unwrap_or_else(|_| "未知麦克风".to_string());
    let sample_rate = supported_config.sample_rate().0;
    let channel_count = usize::from(supported_config.channels());
    if sample_rate == 0 || channel_count == 0 {
        return Err("麦克风返回了无效的采样配置。".to_string());
    }

    let max_sample_count = ((u64::from(sample_rate) * max_duration_ms) / 1_000) as usize;
    let samples = Arc::new(Mutex::new(Vec::<i16>::with_capacity(max_sample_count)));
    let stream_error = Arc::new(Mutex::new(None::<String>));
    let ready_state = Arc::new(Mutex::new(AppAudioReadyState::default()));
    let processing_state = Arc::new(Mutex::new(AppAudioProcessingState::default()));
    let stream_config: cpal::StreamConfig = supported_config.clone().into();
    let sample_format = supported_config.sample_format();
    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(
            &device,
            &stream_config,
            channel_count,
            max_sample_count,
            &samples,
            &stream_error,
            level_callback.as_ref(),
            ready_callback.as_ref(),
            pcm_callback.as_ref(),
            &ready_state,
            &processing_state,
        ),
        cpal::SampleFormat::I16 => build_input_stream::<i16>(
            &device,
            &stream_config,
            channel_count,
            max_sample_count,
            &samples,
            &stream_error,
            level_callback.as_ref(),
            ready_callback.as_ref(),
            pcm_callback.as_ref(),
            &ready_state,
            &processing_state,
        ),
        cpal::SampleFormat::U16 => build_input_stream::<u16>(
            &device,
            &stream_config,
            channel_count,
            max_sample_count,
            &samples,
            &stream_error,
            level_callback.as_ref(),
            ready_callback.as_ref(),
            pcm_callback.as_ref(),
            &ready_state,
            &processing_state,
        ),
        unsupported => Err(format!("当前麦克风采样格式暂不支持：{:?}", unsupported)),
    }?;

    stream.play().map_err(|error| {
        format!(
            "启动麦克风录音失败，请确认 CodexMan 已获得麦克风权限：{}",
            error
        )
    })?;
    let poll_interval = Duration::from_millis(30);
    let max_duration = Duration::from_millis(max_duration_ms);
    let started_at = std::time::Instant::now();
    let mut stopped_by_request = false;
    while started_at.elapsed() < max_duration {
        if stop_requested.load(Ordering::Acquire) {
            stopped_by_request = true;
            break;
        }
        thread::sleep(poll_interval);
    }
    drop(stream);

    if let Some(error) = stream_error
        .lock()
        .map_err(|_| "读取录音错误状态失败：状态锁已损坏".to_string())?
        .clone()
    {
        return Err(error);
    }

    let captured_samples = samples
        .lock()
        .map_err(|_| "读取录音缓存失败：状态锁已损坏".to_string())?
        .clone();
    if captured_samples.is_empty() {
        return Err(
            "没有录到有效音频，请确认麦克风未被静音并允许 CodexMan 访问麦克风。".to_string(),
        );
    }
    let processing_diagnostics = processing_state
        .lock()
        .map_err(|_| "读取录音处理状态失败：状态锁已损坏".to_string())?
        .diagnostics();
    let duration_ms = (captured_samples.len() as u64 * 1_000) / u64::from(sample_rate);
    let diagnostics = build_audio_diagnostics(
        &captured_samples,
        stopped_by_request,
        AppAudioInputInfo {
            device_name,
            sample_rate,
            channel_count,
            sample_format: format!("{:?}", sample_format),
        },
        processing_diagnostics,
    );
    let bytes = write_wav_bytes(sample_rate, &captured_samples)?;
    Ok(AppAudioRecord {
        bytes,
        content_type: "audio/wav".to_string(),
        duration_ms,
        diagnostics,
    })
}

/// App 录音输入设备信息。
struct AppAudioInputInfo {
    /// 系统默认输入设备名称。
    device_name: String,
    /// 输入采样率。
    sample_rate: u32,
    /// 输入声道数。
    channel_count: usize,
    /// 输入采样格式。
    sample_format: String,
}

/// App 录音自动增益处理摘要。
#[derive(Clone, Copy)]
struct AppAudioProcessingDiagnostics {
    /// 录音期间最大增益。
    max_auto_gain: f64,
    /// 所有输入帧的平均增益。
    average_auto_gain: f64,
}

/// 生成录音质量诊断，只记录波形统计，不记录或落盘音频内容。
/// 流程：计算峰值、RMS 和有效样本占比，辅助判断用户侧是权限问题、设备静音还是上游识别空结果。
/// 参数：samples 为单声道 PCM 样本，stopped_by_request 表示是否由第二次快捷键停止，input_info 和 processing 记录安全诊断摘要。
/// 返回：可安全写入诊断日志的音频统计信息。
/// 异常/边界：空样本由调用方提前拦截；这里仍用 1 作为分母兜底，避免除零。
fn build_audio_diagnostics(
    samples: &[i16],
    stopped_by_request: bool,
    input_info: AppAudioInputInfo,
    processing: AppAudioProcessingDiagnostics,
) -> AppAudioDiagnostics {
    let mut peak_amplitude = 0_i32;
    let mut square_sum = 0_f64;
    let mut active_sample_count = 0_usize;
    for sample in samples {
        let amplitude = i32::from(*sample).abs();
        peak_amplitude = peak_amplitude.max(amplitude);
        square_sum += f64::from(amplitude * amplitude);
        if amplitude >= SPEECH_SAMPLE_THRESHOLD {
            active_sample_count += 1;
        }
    }
    let sample_count = samples.len().max(1);
    AppAudioDiagnostics {
        device_name: input_info.device_name,
        sample_rate: input_info.sample_rate,
        channel_count: input_info.channel_count,
        sample_format: input_info.sample_format,
        sample_count: samples.len(),
        peak_amplitude: peak_amplitude.min(i32::from(i16::MAX)) as i16,
        rms_amplitude: (square_sum / sample_count as f64).sqrt(),
        active_sample_ratio: active_sample_count as f64 / sample_count as f64,
        stopped_by_request,
        max_auto_gain: processing.max_auto_gain,
        average_auto_gain: processing.average_auto_gain,
    }
}

/// 构建类型化输入流，把多声道音频实时混合为单声道 i16。
/// 流程：按帧读取所有声道并取平均值，超过最大样本数后丢弃后续数据。
/// 参数：device/config 为 cpal 输入设备配置，channel_count 为输入声道数，max_sample_count 为单声道样本上限。
/// 返回：尚未启动的输入流。
/// 异常/边界：CoreAudio 建流失败时返回系统错误；回调内锁异常会转成异步错误状态。
fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channel_count: usize,
    max_sample_count: usize,
    samples: &Arc<Mutex<Vec<i16>>>,
    stream_error: &Arc<Mutex<Option<String>>>,
    level_callback: Option<&AppAudioLevelCallback>,
    ready_callback: Option<&AppAudioReadyCallback>,
    pcm_callback: Option<&AppAudioPcmCallback>,
    ready_state: &Arc<Mutex<AppAudioReadyState>>,
    processing_state: &Arc<Mutex<AppAudioProcessingState>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::Sample + cpal::SizedSample + AppSampleToI16,
{
    let callback_samples = Arc::clone(samples);
    let callback_error = Arc::clone(stream_error);
    let error_state = Arc::clone(stream_error);
    let callback_level = level_callback.cloned();
    let callback_ready = ready_callback.cloned();
    let callback_pcm = pcm_callback.cloned();
    let callback_ready_state = Arc::clone(ready_state);
    let callback_processing_state = Arc::clone(processing_state);
    let level_state = Arc::new(Mutex::new(AppAudioLevelThrottle::default()));
    let sample_rate = config.sample_rate.0;
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if let Err(error) = push_mono_samples(
                    data,
                    channel_count,
                    max_sample_count,
                    &callback_samples,
                    callback_level.as_ref(),
                    callback_ready.as_ref(),
                    callback_pcm.as_ref(),
                    sample_rate,
                    &callback_ready_state,
                    &level_state,
                    &callback_processing_state,
                ) {
                    if let Ok(mut stored_error) = callback_error.lock() {
                        *stored_error = Some(error);
                    }
                }
            },
            move |error| {
                if let Ok(mut stored_error) = error_state.lock() {
                    *stored_error = Some(format!("麦克风录音流异常：{}", error));
                }
            },
            None,
        )
        .map_err(|error| format!("创建麦克风录音流失败：{}", error))
}

/// 把一批输入样本追加到单声道缓存。
/// 流程：按声道帧取平均，写入 i16 单声道缓存并执行硬上限。
/// 参数：data 为输入回调数据，其余参数描述声道数、样本上限和共享缓存。
/// 返回：成功写入或达到上限。
/// 异常/边界：缓存锁损坏时返回错误，空帧直接跳过。
fn push_mono_samples<T>(
    data: &[T],
    channel_count: usize,
    max_sample_count: usize,
    samples: &Arc<Mutex<Vec<i16>>>,
    level_callback: Option<&AppAudioLevelCallback>,
    ready_callback: Option<&AppAudioReadyCallback>,
    pcm_callback: Option<&AppAudioPcmCallback>,
    sample_rate: u32,
    ready_state: &Arc<Mutex<AppAudioReadyState>>,
    level_state: &Arc<Mutex<AppAudioLevelThrottle>>,
    processing_state: &Arc<Mutex<AppAudioProcessingState>>,
) -> Result<(), String>
where
    T: AppSampleToI16,
{
    if data.is_empty() || channel_count == 0 {
        return Ok(());
    }
    let mut level_samples = Vec::with_capacity(data.len() / channel_count.max(1));
    for frame in data.chunks(channel_count) {
        let mono_sample = select_strongest_channel_sample(frame);
        level_samples.push(mono_sample);
    }
    apply_auto_gain(&mut level_samples, processing_state)?;
    if level_samples.is_empty() {
        return Ok(());
    }
    let mut stored_samples = samples
        .lock()
        .map_err(|_| "写入录音缓存失败：状态锁已损坏".to_string())?;
    if stored_samples.len() >= max_sample_count {
        return Ok(());
    }
    let mut stored_count = 0usize;
    for mono_sample in &level_samples {
        if stored_samples.len() >= max_sample_count {
            break;
        }
        stored_samples.push(*mono_sample);
        stored_count += 1;
    }
    drop(stored_samples);
    let stored_level_samples = &level_samples[..stored_count];
    maybe_emit_recording_ready(stored_level_samples, ready_callback, ready_state)?;
    maybe_emit_audio_level(stored_level_samples, level_callback, level_state)?;
    maybe_emit_pcm_chunk(stored_level_samples, sample_rate, pcm_callback);
    Ok(())
}

/// 从一帧多声道采样中选择能量最高的声道。
/// 流程：逐声道转换为 i16，保留绝对值最大的样本及其符号，避免左右声道相位或空声道被平均后抵消。
/// 参数：frame 为同一时刻的多声道采样。
/// 返回：最能代表当前输入人声强度的单声道样本。
/// 异常/边界：空帧返回 0。
fn select_strongest_channel_sample<T>(frame: &[T]) -> i16
where
    T: AppSampleToI16,
{
    let mut strongest = 0_i16;
    let mut strongest_amplitude = 0_i32;
    for sample in frame {
        let value = sample.to_i16_sample();
        let amplitude = i32::from(value).abs();
        if amplitude > strongest_amplitude {
            strongest = value;
            strongest_amplitude = amplitude;
        }
    }
    strongest
}

/// 对当前输入分片做轻量自动增益。
/// 流程：仅在检测到非静音输入时把偏小的人声抬到稳定 RMS，最大增益和输出峰值都有限制，避免把环境底噪过度放大。
/// 参数：samples 为本批将送入缓存和实时 ASR 的单声道样本，processing_state 保存平滑增益。
/// 返回：成功或状态锁损坏错误。
/// 异常/边界：静音、极低峰值或已经足够响的分片保持原样。
fn apply_auto_gain(
    samples: &mut [i16],
    processing_state: &Arc<Mutex<AppAudioProcessingState>>,
) -> Result<(), String> {
    if samples.is_empty() {
        return Ok(());
    }
    let mut peak_amplitude = 0_i32;
    let mut square_sum = 0_f64;
    for sample in samples.iter() {
        let amplitude = i32::from(*sample).abs();
        peak_amplitude = peak_amplitude.max(amplitude);
        square_sum += f64::from(amplitude * amplitude);
    }
    let rms_amplitude = (square_sum / samples.len().max(1) as f64).sqrt();
    let mut desired_gain = 1.0;
    if peak_amplitude >= AUTO_GAIN_MIN_PEAK
        && rms_amplitude > 0.0
        && rms_amplitude < AUTO_GAIN_TARGET_RMS
    {
        let rms_gain = AUTO_GAIN_TARGET_RMS / rms_amplitude;
        let peak_gain = AUTO_GAIN_OUTPUT_PEAK_LIMIT / f64::from(peak_amplitude);
        desired_gain = rms_gain.min(peak_gain).clamp(1.0, AUTO_GAIN_MAX);
    }
    let gain = {
        let mut state = processing_state
            .lock()
            .map_err(|_| "更新录音增益状态失败：状态锁已损坏".to_string())?;
        state.next_gain(desired_gain, samples.len())
    };
    if gain <= 1.01 {
        return Ok(());
    }
    for sample in samples {
        *sample = (f64::from(*sample) * gain)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
    }
    Ok(())
}

/// 将录音 PCM 分片交给实时 ASR worker。
/// 流程：复制当前输入回调内已混合的单声道样本并标记采样率，交给调用方异步处理。
/// 参数：mono_samples 为当前输入帧，sample_rate 为系统输入采样率，pcm_callback 为可选分片消费者。
/// 返回：无返回值。
/// 异常/边界：空分片、采样率异常或无实时消费者时直接跳过。
fn maybe_emit_pcm_chunk(
    mono_samples: &[i16],
    sample_rate: u32,
    pcm_callback: Option<&AppAudioPcmCallback>,
) {
    let Some(callback) = pcm_callback else {
        return;
    };
    if mono_samples.is_empty() || sample_rate == 0 {
        return;
    }
    callback(AppAudioPcmChunk {
        samples: mono_samples.to_vec(),
        sample_rate,
    });
}

/// 录音预热状态，用于复刻旧静态页“首帧后再等待 12 个采样帧”的提示时机。
struct AppAudioReadyState {
    /// 已收到的非空输入回调次数。
    frame_count: usize,
    /// 开始等待预热的时间。
    started_at: Instant,
    /// 是否已经通知 UI 可以进入录音态。
    emitted: bool,
}

impl Default for AppAudioReadyState {
    fn default() -> Self {
        Self {
            frame_count: 0,
            started_at: Instant::now(),
            emitted: false,
        }
    }
}

/// 在预热采样达到旧静态页阈值后只通知一次录音就绪。
/// 流程：输入回调写入单声道样本后累计帧数；达到 12 帧或等待超过 1600ms 时触发 ready。
/// 参数：mono_samples 为本批样本，ready_callback 为可选 UI 回调，ready_state 为预热门禁。
/// 返回：成功或锁损坏错误。
/// 异常/边界：空采样或无回调时跳过，不影响录音主流程。
fn maybe_emit_recording_ready(
    mono_samples: &[i16],
    ready_callback: Option<&AppAudioReadyCallback>,
    ready_state: &Arc<Mutex<AppAudioReadyState>>,
) -> Result<(), String> {
    if mono_samples.is_empty() {
        return Ok(());
    }
    let Some(callback) = ready_callback else {
        return Ok(());
    };
    let mut state = ready_state
        .lock()
        .map_err(|_| "更新录音预热状态失败：状态锁已损坏".to_string())?;
    if state.emitted {
        return Ok(());
    }
    state.frame_count = state.frame_count.saturating_add(1);
    let should_emit = state.frame_count >= RECORDING_READY_FRAME_COUNT
        || state.started_at.elapsed() >= Duration::from_millis(RECORDING_READY_PREROLL_TIMEOUT_MS);
    if !should_emit {
        return Ok(());
    }
    state.emitted = true;
    drop(state);
    callback();
    Ok(())
}

/// 实时音量节流状态，避免输入回调高频刷新 UI。
struct AppAudioLevelThrottle {
    /// 上一次发出音量事件的时间。
    last_emit_at: Instant,
}

impl Default for AppAudioLevelThrottle {
    fn default() -> Self {
        Self {
            last_emit_at: Instant::now() - Duration::from_millis(120),
        }
    }
}

/// App 录音自动增益状态。
struct AppAudioProcessingState {
    /// 平滑后的当前增益。
    current_gain: f64,
    /// 录音期间出现过的最大增益。
    max_gain: f64,
    /// 按样本数加权的增益总和。
    gain_sample_sum: f64,
    /// 已处理样本数。
    processed_sample_count: usize,
}

impl Default for AppAudioProcessingState {
    fn default() -> Self {
        Self {
            current_gain: 1.0,
            max_gain: 1.0,
            gain_sample_sum: 0.0,
            processed_sample_count: 0,
        }
    }
}

impl AppAudioProcessingState {
    /// 计算下一批样本使用的平滑增益。
    /// 流程：增益升高时响应更快，降低时稍慢释放，减少忽大忽小的泵音；同时记录统计信息供日志排查。
    /// 参数：desired_gain 为当前分片按 RMS/峰值算出的目标增益，sample_count 为本分片样本数。
    /// 返回：用于处理当前分片的实际增益。
    /// 异常/边界：目标增益会被夹紧到安全范围，样本数为 0 时只更新当前增益。
    fn next_gain(&mut self, desired_gain: f64, sample_count: usize) -> f64 {
        let safe_desired = desired_gain.clamp(1.0, AUTO_GAIN_MAX);
        let ratio = if safe_desired > self.current_gain {
            0.45
        } else {
            0.12
        };
        self.current_gain += (safe_desired - self.current_gain) * ratio;
        self.current_gain = self.current_gain.clamp(1.0, AUTO_GAIN_MAX);
        self.max_gain = self.max_gain.max(self.current_gain);
        self.gain_sample_sum += self.current_gain * sample_count as f64;
        self.processed_sample_count = self.processed_sample_count.saturating_add(sample_count);
        self.current_gain
    }

    /// 输出录音处理摘要。
    /// 流程：把内部平滑增益状态压缩为最大值和平均值，避免日志记录任何原始音频。
    /// 参数：无。
    /// 返回：可安全写入诊断日志的处理摘要。
    /// 异常/边界：没有处理样本时按 1 倍增益返回。
    fn diagnostics(&self) -> AppAudioProcessingDiagnostics {
        let average_auto_gain = if self.processed_sample_count == 0 {
            1.0
        } else {
            self.gain_sample_sum / self.processed_sample_count as f64
        };
        AppAudioProcessingDiagnostics {
            max_auto_gain: self.max_gain,
            average_auto_gain,
        }
    }
}

/// 按固定频率发送录音实时音量。
/// 流程：从本批单声道样本计算 RMS 和峰值，最多约每 60ms 回调一次 UI。
/// 参数：mono_samples 为本批采样，level_callback 为可选 UI 回调，level_state 为节流状态。
/// 返回：成功或锁损坏错误。
/// 异常/边界：无回调、空样本或未到节流间隔时直接跳过，不影响录音缓存写入。
fn maybe_emit_audio_level(
    mono_samples: &[i16],
    level_callback: Option<&AppAudioLevelCallback>,
    level_state: &Arc<Mutex<AppAudioLevelThrottle>>,
) -> Result<(), String> {
    let Some(callback) = level_callback else {
        return Ok(());
    };
    if mono_samples.is_empty() {
        return Ok(());
    }
    let mut state = level_state
        .lock()
        .map_err(|_| "更新实时音量状态失败：状态锁已损坏".to_string())?;
    if state.last_emit_at.elapsed() < Duration::from_millis(60) {
        return Ok(());
    }
    state.last_emit_at = Instant::now();
    drop(state);

    let mut peak_amplitude = 0_i32;
    let mut square_sum = 0_f64;
    for sample in mono_samples {
        let amplitude = i32::from(*sample).abs();
        peak_amplitude = peak_amplitude.max(amplitude);
        square_sum += f64::from(amplitude * amplitude);
    }
    let sample_count = mono_samples.len().max(1) as f64;
    let rms_amplitude = (square_sum / sample_count).sqrt();
    callback(AppAudioLevel {
        rms_level: (rms_amplitude / f64::from(i16::MAX)).clamp(0.0, 1.0),
        peak_level: (f64::from(peak_amplitude) / f64::from(i16::MAX)).clamp(0.0, 1.0),
    });
    Ok(())
}

/// 把 i16 PCM 写成 WAV 并读回内存。
/// 流程：写入系统临时目录，完成后立即读取并删除，避免音频留在用户磁盘上。
/// 参数：sample_rate 为采样率，samples 为单声道 PCM 样本。
/// 返回：完整 WAV 文件字节。
/// 异常/边界：写入、封口、读取或删除失败均返回明确错误。
fn write_wav_bytes(sample_rate: u32, samples: &[i16]) -> Result<Vec<u8>, String> {
    let path = std::env::temp_dir().join(format!("codexman-audio-{}.wav", Uuid::new_v4()));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    {
        let mut writer = hound::WavWriter::create(&path, spec)
            .map_err(|error| format!("创建 WAV 文件失败：{}", error))?;
        for sample in samples {
            writer
                .write_sample(*sample)
                .map_err(|error| format!("写入 WAV 样本失败：{}", error))?;
        }
        writer
            .finalize()
            .map_err(|error| format!("完成 WAV 文件失败：{}", error))?;
    }
    let bytes = fs::read(&path).map_err(|error| format!("读取 WAV 文件失败：{}", error))?;
    fs::remove_file(&path).map_err(|error| format!("清理临时 WAV 文件失败：{}", error))?;
    Ok(bytes)
}

/// 将 cpal 输入样本安全转换成 i16 PCM。
trait AppSampleToI16 {
    /// 转成 i16 样本。
    fn to_i16_sample(&self) -> i16;
}

impl AppSampleToI16 for f32 {
    fn to_i16_sample(&self) -> i16 {
        (self.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
    }
}

impl AppSampleToI16 for i16 {
    fn to_i16_sample(&self) -> i16 {
        *self
    }
}

impl AppSampleToI16 for u16 {
    fn to_i16_sample(&self) -> i16 {
        (*self as i32 - 32_768) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多声道混音必须保留能量最高的声道，避免平均值把有效人声抵消。
    #[test]
    fn strongest_channel_sample_preserves_louder_input() {
        assert_eq!(
            select_strongest_channel_sample(&[120_i16, -2_400_i16]),
            -2_400
        );
        assert_eq!(
            select_strongest_channel_sample(&[-3_200_i16, 900_i16]),
            -3_200
        );
    }

    /// 偏小但非静音的人声应被自动增益抬高，方便实时 ASR 获得更稳定的 PCM。
    #[test]
    fn auto_gain_amplifies_quiet_voice_like_samples() {
        let state = Arc::new(Mutex::new(AppAudioProcessingState::default()));
        let mut samples = vec![500_i16, -620_i16, 740_i16, -860_i16, 980_i16];
        let before_peak = samples.iter().map(|sample| sample.abs()).max().unwrap_or(0);
        apply_auto_gain(&mut samples, &state).expect("自动增益不应失败");
        let after_peak = samples.iter().map(|sample| sample.abs()).max().unwrap_or(0);
        assert!(after_peak > before_peak);
        assert!(state.lock().unwrap().diagnostics().max_auto_gain > 1.0);
    }

    /// 极低峰值的环境底噪不应被自动增益放大，避免把静音误处理成人声。
    #[test]
    fn auto_gain_keeps_tiny_noise_unchanged() {
        let state = Arc::new(Mutex::new(AppAudioProcessingState::default()));
        let mut samples = vec![20_i16, -30_i16, 40_i16, -50_i16];
        let original = samples.clone();
        apply_auto_gain(&mut samples, &state).expect("自动增益不应失败");
        assert_eq!(samples, original);
    }
}
