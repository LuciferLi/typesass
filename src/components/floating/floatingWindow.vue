<template>
    <main
        class="floatingWindow"
        data-tauri-drag-region>
        <section
            class="voicePill"
            :class="{ isNudging }"
            :data-state="legacyState"
            :title="stateTitle"
            role="status"
            aria-label="语音输入悬浮条"
            aria-live="polite"
            data-tauri-drag-region>
            <button
                class="pillButton cancelButton"
                type="button"
                aria-label="取消录音"
                :title="cancelTitle"
                @click="handleCancelVoiceTask">
                <span
                    class="pillIcon pillIconClose"
                    aria-hidden="true"></span>
            </button>
            <button
                class="dotsTrack"
                type="button"
                aria-label="停止录音并开始转文字"
                :title="confirmTitle"
                :data-state="legacyState"
                @click="handleStopRecording">
                <span
                    v-for="item in dotItems"
                    :key="item"
                    :style="dotStyleList[item]"></span>
            </button>
            <button
                class="pillButton confirmButton"
                type="button"
                aria-label="停止录音并开始转文字"
                :title="confirmTitle"
                @click="handleStopRecording">
                <span
                    class="pillIcon pillIconCheck"
                    aria-hidden="true"></span>
            </button>
        </section>
        <p
            v-if="errorHintMessage"
            class="errorHint"
            role="alert">
            {{ errorHintMessage }}
        </p>
    </main>
</template>

<script setup lang="ts">
    import { cancelAppVoiceTask, listenEvent, stopAppVoiceRecording } from '@/service/tauri/command';

    defineOptions({
        name: 'FloatingWindow'
    });

    /** 悬浮录音胶囊支持的展示阶段。 */
    type FloatingVoicePhaseType =
        | 'idle'
        | 'preparing'
        | 'recording'
        | 'stopping'
        | 'transcribing'
        | 'polishing'
        | 'processing'
        | 'success'
        | 'error';

    /** 悬浮录音胶囊状态事件载荷。 */
    interface FloatingVoiceStateModel {
        /** 当前展示阶段，用于还原旧静态页悬浮条的录音、处理和完成状态。 */
        phase: FloatingVoicePhaseType;
        /** 状态说明文案，主要用于 title 与无障碍读屏。 */
        message: string;
    }

    /** 悬浮录音窗实时音量事件载荷，只包含安全统计值，不包含原始音频。 */
    interface FloatingVoiceLevelModel {
        /** RMS 归一化音量，范围 0 到 1。 */
        rmsLevel: number;
        /** 峰值归一化音量，范围 0 到 1。 */
        peakLevel: number;
    }

    const VOICE_DOT_FACTORS = [0.48, 0.72, 0.94, 0.66, 1, 0.78, 0.9, 0.58, 0.42] as const;
    const ERROR_HINT_VISIBLE_MS = 5_000;

    const dotItems = Array.from({ length: VOICE_DOT_FACTORS.length }, (_, index) => index);
    const state = reactive<FloatingVoiceStateModel>({
        phase: 'idle',
        message: '按快捷键开始录音，再按一次停止。'
    });
    const voiceLevel = reactive<FloatingVoiceLevelModel>({
        rmsLevel: 0,
        peakLevel: 0
    });
    const isNudging = ref(false);
    const errorHintMessage = ref('');
    const legacyState = computed<'ready' | 'recording' | 'busy' | 'error'>(() => {
        if (state.phase === 'recording') return 'recording';
        if (state.phase === 'error') return 'error';
        if (state.phase === 'idle' || state.phase === 'success') return 'ready';
        return 'busy';
    });
    const stateTitle = computed(() => {
        if (state.phase === 'error') return normalizeErrorHintMessage(state.message);
        return state.message;
    });
    const dotStyleList = computed<Record<string, string>[]>(() => {
        if (state.phase !== 'recording') return dotItems.map(() => ({}));
        const normalizedLevel = normalizeVoiceLevel(voiceLevel);
        return dotItems.map((item) => {
            const factor = VOICE_DOT_FACTORS[item] ?? 0.6;
            const height = 2 + normalizedLevel * 14 * factor;
            const opacity = 0.42 + normalizedLevel * 0.58;
            return {
                height: `${height.toFixed(1)}px`,
                opacity: Math.min(opacity, 1).toFixed(2)
            };
        });
    });
    const confirmTitle = computed(() => {
        if (state.phase === 'recording') return '停止录音并转文字';
        return '正在处理语音';
    });
    const cancelTitle = computed(() => {
        if (state.phase === 'recording') return '取消本次录音';
        return '取消本次语音输入';
    });
    let removeFloatingVoiceStateListener: (() => void) | undefined;
    let removeFloatingVoiceLevelListener: (() => void) | undefined;
    let removeFloatingVoiceNudgeListener: (() => void) | undefined;
    let nudgeTimer: number | undefined;
    let errorHintTimer: number | undefined;

    onMounted(async () => {
        removeFloatingVoiceStateListener = await listenEvent<FloatingVoiceStateModel>(
            'floating-voice-state',
            (payload) => {
                state.phase = payload.phase;
                state.message = payload.message || '语音输入处理中。';
                if (payload.phase === 'error') {
                    showErrorHint(payload.message);
                } else {
                    clearErrorHint();
                }
                if (payload.phase !== 'recording') {
                    voiceLevel.rmsLevel = 0;
                    voiceLevel.peakLevel = 0;
                }
            }
        );
        removeFloatingVoiceLevelListener = await listenEvent<FloatingVoiceLevelModel>(
            'floating-voice-level',
            (payload) => {
                if (state.phase !== 'recording') return;
                voiceLevel.rmsLevel = clampLevel(payload.rmsLevel);
                voiceLevel.peakLevel = clampLevel(payload.peakLevel);
            }
        );
        removeFloatingVoiceNudgeListener = await listenEvent<void>('floating-voice-nudge', () => {
            flashFloatingNudge();
        });
        window.addEventListener('keydown', handleFloatingKeydown);
    });

    onUnmounted(() => {
        removeFloatingVoiceStateListener?.();
        removeFloatingVoiceLevelListener?.();
        removeFloatingVoiceNudgeListener?.();
        if (nudgeTimer !== undefined) window.clearTimeout(nudgeTimer);
        if (errorHintTimer !== undefined) window.clearTimeout(errorHintTimer);
        window.removeEventListener('keydown', handleFloatingKeydown);
    });

    /**
     * 把实时 RMS 和峰值折算成适合悬浮窗展示的波纹强度。
     * 流程：沿用旧静态页的 RMS 乘 12 映射，让真实录音波形和原先调试过的视觉幅度一致。
     * 参数：level 为 Rust 侧传来的安全音量统计。
     * 返回：0 到 1 的视觉强度。
     * 异常/边界：异常数值统一夹紧，避免 CSS 变量出现 NaN 或超大值。
     */
    function normalizeVoiceLevel(level: FloatingVoiceLevelModel): number {
        return Math.max(0.08, Math.min(1, clampLevel(level.rmsLevel) * 12));
    }

    /**
     * 夹紧音量数值，保证跨进程事件异常时 UI 不会抖坏。
     * 参数：value 为任意来源的音量数值。
     * 返回：0 到 1 的安全数值。
     * 异常/边界：非有限数值按 0 处理。
     */
    function clampLevel(value: number): number {
        if (!Number.isFinite(value)) return 0;
        return Math.min(Math.max(value, 0), 1);
    }

    /**
     * 触发旧静态页一致的胶囊轻微抖动反馈。
     * 流程：先重置 class 再下一帧打开，确保连续快捷键也能重新播放动画。
     * 参数：无。
     * 返回：无。
     * 异常/边界：销毁前会清理定时器，避免卸载后继续写状态。
     */
    function flashFloatingNudge(): void {
        if (nudgeTimer !== undefined) {
            window.clearTimeout(nudgeTimer);
            nudgeTimer = undefined;
        }
        isNudging.value = false;
        window.requestAnimationFrame(() => {
            isNudging.value = true;
            nudgeTimer = window.setTimeout(() => {
                isNudging.value = false;
                nudgeTimer = undefined;
            }, 240);
        });
    }

    /**
     * 展示错误提示并在固定时间后自动隐藏。
     * 流程：把跨进程错误先归一成中文用户提示，再启动 5 秒清理定时器，避免英文错误码直接出现在悬浮窗。
     * 参数：message 为 Rust 或 IPC 返回的原始错误文案。
     * 返回：无。
     * 异常/边界：空文案、英文错误码或网络异常统一显示中文兜底提示。
     */
    function showErrorHint(message: string): void {
        if (errorHintTimer !== undefined) {
            window.clearTimeout(errorHintTimer);
            errorHintTimer = undefined;
        }
        errorHintMessage.value = normalizeErrorHintMessage(message);
        errorHintTimer = window.setTimeout(() => {
            errorHintMessage.value = '';
            errorHintTimer = undefined;
        }, ERROR_HINT_VISIBLE_MS);
    }

    /**
     * 清理当前错误提示。
     * 流程：非错误状态到来时立即隐藏提示并取消自动隐藏定时器。
     * 参数：无。
     * 返回：无。
     * 异常/边界：定时器不存在时直接忽略。
     */
    function clearErrorHint(): void {
        errorHintMessage.value = '';
        if (errorHintTimer === undefined) return;
        window.clearTimeout(errorHintTimer);
        errorHintTimer = undefined;
    }

    /**
     * 将错误消息归一成面向普通用户的中文提示。
     * 流程：保留明确中文业务提示；遇到英文、错误码、网络/上游异常时用中文兜底。
     * 参数：message 为任意来源错误文本。
     * 返回：不会包含英文错误码的中文提示。
     * 异常/边界：如果文案包含中英文混合诊断信息，为避免泄露技术细节也使用兜底文案。
     */
    function normalizeErrorHintMessage(message: string): string {
        const trimmedMessage = message.trim();
        if (!trimmedMessage) return '操作失败，请检查网络。';
        if (/[A-Za-z]/.test(trimmedMessage)) return '操作失败，请检查网络或模型配置。';
        if (/错误码|诊断|上游|模型服务|网络|连接|超时|请求|接口/.test(trimmedMessage)) {
            return '操作失败，请检查网络或模型配置。';
        }
        return trimmedMessage;
    }

    /**
     * 停止当前 App 原生录音并进入转写。
     * 流程：点击旧静态页右侧确认按钮后调用 Rust 停止信号；处理中阶段仅刷新状态，不重复提交录音。
     * 参数：无。
     * 返回：无。
     * 异常/边界：IPC 失败时把错误映射到悬浮窗错误态，避免用户点了按钮却完全无反馈。
     */
    async function handleStopRecording(): Promise<void> {
        try {
            state.message = await stopAppVoiceRecording();
        } catch (error) {
            state.phase = 'error';
            state.message = error instanceof Error ? error.message : '停止录音失败。';
            showErrorHint(state.message);
        }
    }

    /**
     * 取消当前 App 原生语音任务。
     * 流程：点击旧静态页左侧 X 后调用 Rust 取消信号；录音中会停止采样，处理中会丢弃后续结果。
     * 参数：无。
     * 返回：无。
     * 异常/边界：取消失败时展示错误态，不吞掉用户操作。
     */
    async function handleCancelVoiceTask(): Promise<void> {
        try {
            state.message = await cancelAppVoiceTask();
        } catch (error) {
            state.phase = 'error';
            state.message = error instanceof Error ? error.message : '取消语音输入失败。';
            showErrorHint(state.message);
        }
    }

    /**
     * 处理悬浮录音窗键盘取消。
     * 流程：按下 Escape 时复用旧静态页的取消入口；录音阶段停止采样，处理阶段丢弃后续结果。
     * 参数：event 为窗口键盘事件。
     * 返回：无。
     * 异常/边界：非 Escape 键不处理，避免影响目标 App 的普通输入。
     */
    function handleFloatingKeydown(event: KeyboardEvent): void {
        if (event.key !== 'Escape') return;
        event.preventDefault();
        void handleCancelVoiceTask();
    }
</script>

<style scoped>
    .floatingWindow {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        flex-direction: column;
        gap: 7px;
        background: transparent;
    }

    .errorHint {
        box-sizing: border-box;
        max-width: 260px;
        min-height: 28px;
        margin: 0;
        border: 1px solid rgba(239, 68, 68, 0.24);
        border-radius: 999px;
        padding: 6px 12px;
        overflow: hidden;
        background: rgba(5, 5, 7, 0.9);
        box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22);
        color: #fee2e2;
        font-size: 12px;
        font-weight: 500;
        line-height: 16px;
        text-align: center;
        text-overflow: ellipsis;
        white-space: nowrap;
        animation: errorHintIn 140ms ease-out;
    }

    .voicePill {
        position: relative;
        display: grid;
        width: 122px;
        height: 36px;
        grid-template-columns: 26px minmax(0, 1fr) 26px;
        align-items: center;
        gap: 6px;
        padding: 4px;
        overflow: hidden;
        border: 1px solid rgba(255, 255, 255, 0.14);
        border-radius: 999px;
        background: #050507;
        transition:
            border-color 160ms ease,
            transform 160ms ease;
    }

    .voicePill::before {
        position: absolute;
        inset: 2px;
        border: 1px solid transparent;
        border-radius: inherit;
        opacity: 0;
        content: '';
        pointer-events: none;
    }

    .voicePill::after {
        position: absolute;
        inset: 0;
        opacity: 0;
        background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.08), transparent);
        content: '';
        transform: translateX(-80%);
        pointer-events: none;
    }

    .voicePill[data-state='busy']::after {
        opacity: 1;
        animation: pillSweep 920ms ease-in-out infinite;
    }

    .pillButton,
    .dotsTrack {
        cursor: pointer;
    }

    .pillButton {
        position: relative;
        z-index: 1;
        display: grid;
        width: 26px;
        height: 26px;
        place-items: center;
        border: 0;
        border-radius: 999px;
        padding: 0;
        cursor: pointer;
        user-select: none;
        transition:
            filter 150ms ease,
            transform 150ms ease;
        transform-origin: center;
    }

    .pillButton:focus-visible,
    .dotsTrack:focus-visible {
        outline: 2px solid rgba(255, 255, 255, 0.72);
        outline-offset: 2px;
    }

    .pillButton:hover {
        filter: brightness(1.08);
        transform: scale(1.13);
    }

    .pillButton:active {
        transform: scale(0.94);
    }

    .cancelButton {
        background: #272832;
        color: rgba(255, 255, 255, 0.84);
    }

    .confirmButton {
        background: #f8fafc;
        color: #050507;
    }

    .pillIcon {
        position: relative;
        display: inline-flex;
        width: 14px;
        height: 14px;
        align-items: center;
        justify-content: center;
    }

    .pillIconClose::before,
    .pillIconClose::after {
        position: absolute;
        width: 12px;
        height: 2px;
        border-radius: 999px;
        background: currentColor;
        content: '';
    }

    .pillIconClose::before {
        transform: rotate(45deg);
    }

    .pillIconClose::after {
        transform: rotate(-45deg);
    }

    .pillIconCheck::before {
        width: 11px;
        height: 6px;
        border-bottom: 2px solid currentColor;
        border-left: 2px solid currentColor;
        content: '';
        transform: translateY(-1px) rotate(-45deg);
    }

    .dotsTrack {
        position: relative;
        z-index: 1;
        display: flex;
        min-width: 0;
        height: 20px;
        align-items: center;
        justify-content: center;
        gap: 3px;
        border: 0;
        border-radius: 999px;
        padding: 0 1px;
        background: transparent;
    }

    .dotsTrack span {
        display: block;
        width: 2px;
        height: 2px;
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.42);
        transform-origin: center;
        transition:
            background 160ms ease,
            height 160ms ease,
            opacity 160ms ease;
    }

    .voicePill[data-state='recording'] {
        border-color: rgba(255, 255, 255, 0.28);
        transform: scale(1.02);
    }

    .voicePill[data-state='recording']::before {
        border-color: rgba(16, 185, 129, 0.42);
        opacity: 1;
        animation: pillBreath 1320ms ease-in-out infinite;
    }

    .voicePill[data-state='recording'] .dotsTrack span {
        height: 10px;
        background: #f8fafc;
        animation: none;
    }

    .voicePill[data-state='busy'] {
        border-color: rgba(245, 158, 11, 0.64);
    }

    .voicePill[data-state='busy'] .dotsTrack span {
        height: 8px;
        background: #f59e0b;
        animation: voiceLevel 680ms ease-in-out infinite;
    }

    .voicePill[data-state='error'] {
        border-color: rgba(239, 68, 68, 0.72);
        animation: errorNudge 260ms ease-out;
    }

    .voicePill[data-state='error'] .dotsTrack span {
        background: #ef4444;
    }

    .voicePill.isNudging {
        animation: softNudge 220ms ease-out;
    }

    .dotsTrack span:nth-child(2) {
        animation-delay: 80ms;
    }

    .dotsTrack span:nth-child(3) {
        animation-delay: 150ms;
    }

    .dotsTrack span:nth-child(4) {
        animation-delay: 30ms;
    }

    .dotsTrack span:nth-child(5) {
        animation-delay: 190ms;
    }

    .dotsTrack span:nth-child(6) {
        animation-delay: 120ms;
    }

    .dotsTrack span:nth-child(7) {
        animation-delay: 230ms;
    }

    .dotsTrack span:nth-child(8) {
        animation-delay: 60ms;
    }

    .dotsTrack span:nth-child(9) {
        animation-delay: 170ms;
    }

    @keyframes voiceLevel {
        0%,
        100% {
            opacity: 0.5;
            transform: scaleY(0.38);
        }

        50% {
            opacity: 1;
            transform: scaleY(1);
        }
    }

    @keyframes pillSweep {
        0% {
            transform: translateX(-90%);
        }

        100% {
            transform: translateX(90%);
        }
    }

    @keyframes pillBreath {
        0%,
        100% {
            opacity: 0.34;
            transform: scale(0.96);
        }

        50% {
            opacity: 1;
            transform: scale(1);
        }
    }

    @keyframes errorNudge {
        0%,
        100% {
            transform: translateX(0);
        }

        25% {
            transform: translateX(-2px);
        }

        60% {
            transform: translateX(2px);
        }
    }

    @keyframes softNudge {
        0%,
        100% {
            transform: translateY(0) scale(1);
        }

        45% {
            transform: translateY(-1px) scale(1.035);
        }
    }

    @keyframes errorHintIn {
        0% {
            opacity: 0;
            transform: translateY(-3px);
        }

        100% {
            opacity: 1;
            transform: translateY(0);
        }
    }
</style>
