use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use uuid::Uuid;

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
}

/// 使用 CodexMan 主进程录制一段可被外部停止的麦克风 WAV。
/// 流程：打开系统默认输入设备，持续采集 PCM；达到最长时长或收到停止信号后，混合为单声道 16-bit WAV 返回。
/// 参数：max_duration_ms 为单次最长录音时长，stop_requested 为快捷键停止信号。
/// 返回：包含 WAV 字节、MIME 和实际时长的录音结果。
/// 异常/边界：无输入设备、系统拒绝麦克风、采样流错误、空音频或临时文件写入失败都会显式返回错误。
pub fn record_microphone_wav(
    max_duration_ms: u64,
    stop_requested: Arc<AtomicBool>,
) -> Result<AppAudioRecord, String> {
    if max_duration_ms == 0 || max_duration_ms > 120_000 {
        return Err("录音时长必须在 1 毫秒到 120 秒之间。".to_string());
    }

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "没有找到可用麦克风，请检查系统输入设备。".to_string())?;
    let supported_config = device
        .default_input_config()
        .map_err(|error| format!("读取默认麦克风配置失败：{}", error))?;
    let sample_rate = supported_config.sample_rate().0;
    let channel_count = usize::from(supported_config.channels());
    if sample_rate == 0 || channel_count == 0 {
        return Err("麦克风返回了无效的采样配置。".to_string());
    }

    let max_sample_count = ((u64::from(sample_rate) * max_duration_ms) / 1_000) as usize;
    let samples = Arc::new(Mutex::new(Vec::<i16>::with_capacity(max_sample_count)));
    let stream_error = Arc::new(Mutex::new(None::<String>));
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
        ),
        cpal::SampleFormat::I16 => build_input_stream::<i16>(
            &device,
            &stream_config,
            channel_count,
            max_sample_count,
            &samples,
            &stream_error,
        ),
        cpal::SampleFormat::U16 => build_input_stream::<u16>(
            &device,
            &stream_config,
            channel_count,
            max_sample_count,
            &samples,
            &stream_error,
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
    let duration_ms = (captured_samples.len() as u64 * 1_000) / u64::from(sample_rate);
    let diagnostics = build_audio_diagnostics(&captured_samples, stopped_by_request);
    let bytes = write_wav_bytes(sample_rate, &captured_samples)?;
    Ok(AppAudioRecord {
        bytes,
        content_type: "audio/wav".to_string(),
        duration_ms,
        diagnostics,
    })
}

/// 生成录音质量诊断，只记录波形统计，不记录或落盘音频内容。
/// 流程：计算峰值、RMS 和有效样本占比，辅助判断用户侧是权限问题、设备静音还是上游识别空结果。
/// 参数：samples 为单声道 PCM 样本，stopped_by_request 表示是否由第二次快捷键停止。
/// 返回：可安全写入诊断日志的音频统计信息。
/// 异常/边界：空样本由调用方提前拦截；这里仍用 1 作为分母兜底，避免除零。
fn build_audio_diagnostics(samples: &[i16], stopped_by_request: bool) -> AppAudioDiagnostics {
    const ACTIVE_SAMPLE_THRESHOLD: i32 = 500;
    let mut peak_amplitude = 0_i32;
    let mut square_sum = 0_f64;
    let mut active_sample_count = 0_usize;
    for sample in samples {
        let amplitude = i32::from(*sample).abs();
        peak_amplitude = peak_amplitude.max(amplitude);
        square_sum += f64::from(amplitude * amplitude);
        if amplitude >= ACTIVE_SAMPLE_THRESHOLD {
            active_sample_count += 1;
        }
    }
    let sample_count = samples.len().max(1);
    AppAudioDiagnostics {
        sample_count: samples.len(),
        peak_amplitude: peak_amplitude.min(i32::from(i16::MAX)) as i16,
        rms_amplitude: (square_sum / sample_count as f64).sqrt(),
        active_sample_ratio: active_sample_count as f64 / sample_count as f64,
        stopped_by_request,
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
) -> Result<cpal::Stream, String>
where
    T: cpal::Sample + cpal::SizedSample + AppSampleToI16,
{
    let callback_samples = Arc::clone(samples);
    let callback_error = Arc::clone(stream_error);
    let error_state = Arc::clone(stream_error);
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if let Err(error) =
                    push_mono_samples(data, channel_count, max_sample_count, &callback_samples)
                {
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
) -> Result<(), String>
where
    T: AppSampleToI16,
{
    if data.is_empty() || channel_count == 0 {
        return Ok(());
    }
    let mut stored_samples = samples
        .lock()
        .map_err(|_| "写入录音缓存失败：状态锁已损坏".to_string())?;
    if stored_samples.len() >= max_sample_count {
        return Ok(());
    }
    for frame in data.chunks(channel_count) {
        if stored_samples.len() >= max_sample_count {
            break;
        }
        let frame_sum: i32 = frame
            .iter()
            .map(|sample| i32::from(sample.to_i16_sample()))
            .sum();
        stored_samples.push((frame_sum / frame.len() as i32) as i16);
    }
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
