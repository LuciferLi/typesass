<template>
    <main
        class="floatingWindow"
        data-tauri-drag-region>
        <div
            class="voiceCapsule"
            :class="phaseClass"
            :title="state.message"
            role="status"
            aria-live="polite">
            <span
                class="pillIcon pillIconCancel"
                aria-hidden="true"
                >×</span
            >
            <span
                class="dotRail"
                aria-hidden="true">
                <i
                    v-for="item in dotItems"
                    :key="item"></i>
            </span>
            <span
                class="pillIcon pillIconConfirm"
                aria-hidden="true"
                >✓</span
            >
        </div>
    </main>
</template>

<script setup lang="ts">
    import { listenEvent } from '@/service/tauri/command';

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

    const dotItems = Array.from({ length: 10 }, (_, index) => index);
    const state = reactive<FloatingVoiceStateModel>({
        phase: 'idle',
        message: '按快捷键开始录音，再按一次停止。'
    });
    const phaseClass = computed(() => `voiceCapsule--${state.phase}`);
    let removeFloatingVoiceStateListener: (() => void) | undefined;

    onMounted(async () => {
        removeFloatingVoiceStateListener = await listenEvent<FloatingVoiceStateModel>(
            'floating-voice-state',
            (payload) => {
                state.phase = payload.phase;
                state.message = payload.message || '语音输入处理中。';
            }
        );
    });

    onUnmounted(() => {
        removeFloatingVoiceStateListener?.();
    });
</script>

<style scoped>
    .floatingWindow {
        display: grid;
        width: 100%;
        height: 100%;
        place-items: center;
        background: transparent;
    }

    .voiceCapsule {
        position: relative;
        display: grid;
        width: 132px;
        height: 40px;
        grid-template-columns: 30px minmax(0, 1fr) 30px;
        align-items: center;
        gap: 7px;
        padding: 4px;
        overflow: hidden;
        border: 1px solid rgba(255, 255, 255, 0.16);
        border-radius: 999px;
        background: #050507;
        box-shadow:
            0 14px 34px rgba(0, 0, 0, 0.42),
            inset 0 0 0 1px rgba(255, 255, 255, 0.04);
        transition:
            border-color 180ms ease,
            box-shadow 180ms ease;
    }

    .voiceCapsule::after {
        position: absolute;
        inset: 0;
        background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.11), transparent);
        content: '';
        transform: translateX(-100%);
        animation: capsuleScan 1.18s ease-in-out infinite;
    }

    .pillIcon {
        position: relative;
        z-index: 1;
        display: grid;
        width: 30px;
        height: 30px;
        place-items: center;
        border: 0;
        border-radius: 999px;
        font-size: 22px;
        font-weight: 500;
        line-height: 1;
        user-select: none;
    }

    .pillIconCancel {
        background: #272832;
        color: rgba(255, 255, 255, 0.84);
    }

    .pillIconConfirm {
        background: #f8fafc;
        color: #050507;
        font-size: 21px;
    }

    .dotRail {
        position: relative;
        z-index: 1;
        display: flex;
        min-width: 0;
        align-items: center;
        justify-content: center;
        gap: 4px;
    }

    .dotRail i {
        display: block;
        width: 3px;
        height: 3px;
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.44);
        animation: dotBreathe 1.05s ease-in-out infinite;
        animation-delay: calc(var(--dotIndex, 0) * 80ms);
    }

    .dotRail i:nth-child(1) {
        --dotIndex: 1;
    }

    .dotRail i:nth-child(2) {
        --dotIndex: 2;
    }

    .dotRail i:nth-child(3) {
        --dotIndex: 3;
    }

    .dotRail i:nth-child(4) {
        --dotIndex: 4;
    }

    .dotRail i:nth-child(5) {
        --dotIndex: 5;
    }

    .dotRail i:nth-child(6) {
        --dotIndex: 6;
    }

    .dotRail i:nth-child(7) {
        --dotIndex: 7;
    }

    .dotRail i:nth-child(8) {
        --dotIndex: 8;
    }

    .dotRail i:nth-child(9) {
        --dotIndex: 9;
    }

    .dotRail i:nth-child(10) {
        --dotIndex: 10;
    }

    .voiceCapsule--preparing,
    .voiceCapsule--stopping,
    .voiceCapsule--transcribing,
    .voiceCapsule--polishing,
    .voiceCapsule--processing {
        border-color: rgba(245, 158, 11, 0.8);
        box-shadow:
            0 14px 34px rgba(0, 0, 0, 0.42),
            0 0 0 1px rgba(245, 158, 11, 0.18),
            0 0 22px rgba(245, 158, 11, 0.18);
    }

    .voiceCapsule--preparing .dotRail i,
    .voiceCapsule--stopping .dotRail i,
    .voiceCapsule--transcribing .dotRail i,
    .voiceCapsule--polishing .dotRail i,
    .voiceCapsule--processing .dotRail i {
        background: rgba(245, 158, 11, 0.95);
    }

    .voiceCapsule--recording {
        border-color: rgba(255, 255, 255, 0.22);
    }

    .voiceCapsule--recording .dotRail i {
        background: rgba(255, 255, 255, 0.72);
    }

    .voiceCapsule--success {
        border-color: rgba(34, 197, 94, 0.78);
        box-shadow:
            0 14px 34px rgba(0, 0, 0, 0.42),
            0 0 24px rgba(34, 197, 94, 0.22);
    }

    .voiceCapsule--success .dotRail i {
        background: rgba(34, 197, 94, 0.9);
    }

    .voiceCapsule--error {
        border-color: rgba(248, 113, 113, 0.86);
        box-shadow:
            0 14px 34px rgba(0, 0, 0, 0.42),
            0 0 24px rgba(248, 113, 113, 0.2);
    }

    .voiceCapsule--error .dotRail i {
        background: rgba(248, 113, 113, 0.9);
    }

    @keyframes capsuleScan {
        0% {
            transform: translateX(-100%);
        }

        55%,
        100% {
            transform: translateX(100%);
        }
    }

    @keyframes dotBreathe {
        0%,
        100% {
            opacity: 0.42;
            transform: scaleY(1);
        }

        50% {
            opacity: 1;
            transform: scaleY(1.65);
        }
    }
</style>
