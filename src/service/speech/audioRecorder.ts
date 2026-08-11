// 录音结果模型，用于语音转写。
export type AudioRecordResultModel = {
    // 音频 Blob 对象。
    blob: Blob;
    // 音频 MIME 类型。
    contentType: string;
    // 录音时长，毫秒。
    durationMs: number;
};

// 录音增强选项模型，用于控制麦克风采集时是否请求浏览器或 WebView 提供的人声优化能力。
export type AudioRecordEnhancementOptionModel = {
    // 是否启用智能识音增强；开启时优先请求降噪、回声消除、自动增益和单声道采集。
    enabled: boolean;
};

/**
 * 创建一次性音频录制器。
 * 流程：先按用户设置请求麦克风音频流，再用 MediaRecorder 分片录制，达到最大时长后停止并返回 Blob。
 * 参数：maxDurationMs 为单次录音最长时长，enhancementOption 控制是否启用智能识音增强。
 * 返回：包含音频 Blob、MIME 类型和录音时长的结果。
 * 异常/边界：如果增强约束在当前 WebView 不可用，会自动退回普通麦克风录音；录音失败时抛出麦克风权限提示。
 */
export async function recordAudioOnce(
    maxDurationMs: number,
    enhancementOption: AudioRecordEnhancementOptionModel = { enabled: true }
): Promise<AudioRecordResultModel> {
    const audioConstraints: MediaTrackConstraints | boolean = enhancementOption.enabled
        ? {
              echoCancellation: true,
              noiseSuppression: true,
              autoGainControl: true,
              channelCount: 1
          }
        : true;
    const stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints }).catch((error: unknown) => {
        if (!enhancementOption.enabled) throw error;
        return navigator.mediaDevices.getUserMedia({ audio: true });
    });
    const recorder = new MediaRecorder(stream);
    const chunks: BlobPart[] = [];
    const startedAt = Date.now();

    return new Promise((resolve, reject) => {
        const stopTimer = window.setTimeout(() => {
            if (recorder.state !== 'inactive') recorder.stop();
        }, maxDurationMs);

        recorder.addEventListener('dataavailable', (event) => {
            if (event.data.size > 0) chunks.push(event.data);
        });
        recorder.addEventListener('stop', () => {
            window.clearTimeout(stopTimer);
            stream.getTracks().forEach((track) => track.stop());
            const contentType = recorder.mimeType || 'audio/webm';
            resolve({
                blob: new Blob(chunks, { type: contentType }),
                contentType,
                durationMs: Date.now() - startedAt
            });
        });
        recorder.addEventListener('error', () => {
            window.clearTimeout(stopTimer);
            stream.getTracks().forEach((track) => track.stop());
            reject(new Error('录音失败，请检查麦克风权限。'));
        });
        recorder.start();
    });
}

// 将音频 Blob 转成 base64 内容。
export async function blobToBase64(blob: Blob): Promise<string> {
    const buffer = await blob.arrayBuffer();
    const bytes = new Uint8Array(buffer);
    let binary = '';
    bytes.forEach((byte) => {
        binary += String.fromCharCode(byte);
    });
    return btoa(binary);
}
