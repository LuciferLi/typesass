<template>
    <ui-dialog v-model:open="open">
        <ui-dialog-content>
            <ui-dialog-header>
                <ui-dialog-title>语音转文字快捷键</ui-dialog-title>
                <ui-dialog-description>只设置当前语音转文字润色模块的快捷键。</ui-dialog-description>
            </ui-dialog-header>
            <div class="grid gap-3 py-2">
                <div
                    v-for="shortcut in shortcutItems"
                    :key="shortcut.key"
                    class="grid gap-3 rounded-lg border border-border bg-muted/30 px-4 py-3">
                    <div class="flex items-start justify-between gap-4">
                        <div class="min-w-0">
                            <div class="text-[14px] font-medium text-foreground">{{ shortcut.title }}</div>
                            <div class="mt-1 text-[12px] leading-5 text-muted-foreground">
                                {{ shortcut.description }}
                            </div>
                        </div>
                        <div class="flex shrink-0 items-center gap-2">
                            <ui-kbd-group>
                                <template
                                    v-for="(part, index) in shortcut.parts"
                                    :key="`${shortcut.key}-${part}-${index}`">
                                    <ui-kbd>{{ part }}</ui-kbd>
                                    <span
                                        v-if="index < shortcut.parts.length - 1"
                                        class="text-[12px] text-muted-foreground"
                                        >+</span
                                    >
                                </template>
                            </ui-kbd-group>
                            <ui-button
                                variant="outline"
                                size="sm"
                                type="button"
                                :disabled="saving"
                                @click="handleStartEdit(shortcut.key)">
                                编辑
                            </ui-button>
                        </div>
                    </div>
                    <div
                        v-if="editingKey === shortcut.key"
                        class="grid gap-2 border-t border-border/70 pt-3">
                        <div class="text-[12px] font-medium text-muted-foreground">新快捷键</div>
                        <div class="flex flex-wrap items-center gap-2">
                            <ui-input
                                class="h-9 w-full min-w-[220px] flex-1"
                                :model-value="recordingShortcut || '按下新的组合键'"
                                readonly
                                autofocus
                                @keydown.prevent="handleShortcutKeydown" />
                            <ui-button
                                size="sm"
                                type="button"
                                :disabled="!recordingShortcut || saving"
                                @click="handleSaveShortcut">
                                保存
                            </ui-button>
                            <ui-button
                                variant="outline"
                                size="sm"
                                type="button"
                                :disabled="saving"
                                @click="handleCancelEdit">
                                取消
                            </ui-button>
                        </div>
                    </div>
                </div>
            </div>
            <p
                v-if="message"
                :class="[
                    'text-[12px] leading-5',
                    messageType === 'error' ? 'text-destructive' : 'text-muted-foreground'
                ]">
                {{ message }}
            </p>
            <ui-dialog-footer>
                <ui-button
                    type="button"
                    @click="handleCloseDialog"
                    >完成</ui-button
                >
            </ui-dialog-footer>
        </ui-dialog-content>
    </ui-dialog>
</template>

<script setup lang="ts">
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import { Input as UiInput } from '@/components/ui/input';
    import { Kbd as UiKbd, KbdGroup as UiKbdGroup } from '@/components/ui/kbd';
    import { StorageKey } from '@/config/storageKey';
    import type { ShortcutProfileModel } from '@/model/permission';
    import {
        DefaultShortcutProfile,
        hasShortcutProfileValue,
        normalizeShortcutProfileValue
    } from '@/service/shortcut/shortcutProfile';
    import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
    import {
        getRuntimeDiagnostics,
        isTauriRuntime,
        registerShortcuts,
        suspendShortcutsForRecording
    } from '@/service/tauri/command';

    defineOptions({
        name: 'VoicePolishShortcutDialog'
    });

    /**
     * 语音转文字模块快捷键字段。
     * 业务含义：asr 只转文本，dictate 表示 ASR 后继续润色。
     */
    type VoiceShortcutKey = 'asr' | 'dictate';

    /** 语音模块快捷键展示项，用于渲染两种语音输入动作。 */
    type ShortcutDisplayItem = {
        /** 快捷键配置字段。 */
        key: VoiceShortcutKey;
        /** 快捷键对应的业务动作名称。 */
        title: string;
        /** 快捷键触发后的效果说明。 */
        description: string;
        /** 按 Kbd 组件拆分后的组合键片段。 */
        parts: string[];
    };

    const open = defineModel<boolean>('open', { default: false });
    const shortcutProfile = ref<ShortcutProfileModel>({ ...DefaultShortcutProfile });
    const editingKey = ref<VoiceShortcutKey | ''>('');
    const recordingShortcut = ref('');
    const saving = ref(false);
    const message = ref('');
    const messageType = ref<'info' | 'error'>('info');
    const shortcutItems = computed<ShortcutDisplayItem[]>(() => [
        {
            key: 'asr',
            title: 'ASR 转文本',
            description: '录音后只把语音识别成文字，并直接回填到当前输入位置。',
            parts: splitShortcutParts(shortcutProfile.value.asr)
        },
        {
            key: 'dictate',
            title: 'ASR 转文本并润色',
            description: '录音转写后继续按当前润色模型和输出偏好处理，再回填到当前输入位置。',
            parts: splitShortcutParts(shortcutProfile.value.dictate)
        }
    ]);

    // 监听弹窗打开状态，用于每次进入设置时同步桌面端最新快捷键配置，避免展示过期值。
    watch(open, (isOpen) => {
        if (isOpen) {
            void hydrateShortcutProfile();
        } else if (editingKey.value) {
            void restoreShortcutRegistration();
        }
    });

    /**
     * 从桌面运行诊断和本地配置同步快捷键配置。
     * 流程：优先读取已保存的快捷键配置；没有保存值时再用原生运行诊断补齐运行态。
     * 参数：无。
     * 返回：无返回值。
     * 边界：读取失败时不阻塞弹窗展示，避免用户无法继续编辑快捷键。
     */
    async function hydrateShortcutProfile(): Promise<void> {
        message.value = '';
        editingKey.value = '';
        recordingShortcut.value = '';
        try {
            const savedProfile = await readShortcutProfileConfig();
            const diagnostics = await getRuntimeDiagnostics();
            if (hasShortcutProfileValue(savedProfile)) {
                shortcutProfile.value = normalizeShortcutProfileValue(savedProfile, shortcutProfile.value);
                return;
            }
            shortcutProfile.value = normalizeShortcutProfileValue(
                diagnostics?.shortcuts ?? shortcutProfile.value,
                shortcutProfile.value
            );
        } catch (error) {
            messageType.value = 'error';
            message.value = error instanceof Error ? error.message : '读取快捷键配置失败。';
        }
    }

    /**
     * 进入快捷键编辑态。
     * 流程：记录当前编辑字段，清空录制值，并请求原生侧临时暂停全局快捷键，避免按键被系统拦截。
     * 参数：key 为需要编辑的语音模块快捷键字段。
     * 返回：无返回值。
     * 边界：网页预览没有原生快捷键可暂停，仍可继续录制 UI 输入。
     */
    async function handleStartEdit(key: VoiceShortcutKey): Promise<void> {
        editingKey.value = key;
        recordingShortcut.value = '';
        message.value = '请按下新的组合键。';
        messageType.value = 'info';
        try {
            await suspendShortcutsForRecording();
        } catch (error) {
            messageType.value = 'error';
            message.value = error instanceof Error ? error.message : '暂停快捷键失败。';
        }
    }

    /**
     * 记录用户按下的新快捷键。
     * 流程：从 KeyboardEvent 中读取修饰键和主键，组合成原生侧可规范化的快捷键字符串。
     * 参数：event 为输入框 keydown 事件。
     * 返回：无返回值。
     * 边界：只按修饰键时不记录，避免保存成不可触发的快捷键。
     */
    function handleShortcutKeydown(event: KeyboardEvent): void {
        const shortcut = normalizeKeyboardEvent(event);
        if (shortcut) {
            recordingShortcut.value = shortcut;
            message.value = '已录入，点击保存后生效。';
            messageType.value = 'info';
        }
    }

    /**
     * 保存当前录制的快捷键。
     * 流程：把当前字段写入完整快捷键配置并提交给原生注册；成功后刷新本地展示并退出编辑态。
     * 参数：无。
     * 返回：无返回值。
     * 边界：重复快捷键会由原生侧拒绝；网页预览只更新本地展示。
     */
    async function handleSaveShortcut(): Promise<void> {
        if (!editingKey.value || !recordingShortcut.value) return;
        saving.value = true;
        message.value = '';
        const nextProfile = {
            ...shortcutProfile.value,
            [editingKey.value]: recordingShortcut.value
        };
        try {
            const savedProfile = await registerShortcuts(nextProfile);
            shortcutProfile.value = savedProfile;
            await persistShortcutProfileConfig(savedProfile);
            editingKey.value = '';
            recordingShortcut.value = '';
            messageType.value = 'info';
            message.value = '快捷键已保存。';
        } catch (error) {
            messageType.value = 'error';
            message.value = error instanceof Error ? error.message : '保存快捷键失败。';
        } finally {
            saving.value = false;
        }
    }

    /**
     * 取消当前快捷键编辑。
     * 流程：重新注册当前已保存配置，退出录制态且不提交本次录入值。
     * 参数：无。
     * 返回：无返回值。
     * 边界：恢复失败时保留编辑态并展示错误，避免全局快捷键保持暂停。
     */
    async function handleCancelEdit(): Promise<void> {
        await restoreShortcutRegistration();
    }

    /**
     * 关闭快捷键弹窗。
     * 流程：如果当前处于编辑态，先恢复原快捷键注册，再关闭弹窗。
     * 参数：无。
     * 返回：无返回值。
     * 边界：恢复失败会在弹窗内展示错误，不主动关闭，避免用户误以为快捷键已恢复。
     */
    async function handleCloseDialog(): Promise<void> {
        if (editingKey.value) {
            await restoreShortcutRegistration();
            if (messageType.value === 'error') return;
        }
        open.value = false;
    }

    /**
     * 恢复当前已保存的快捷键注册。
     * 流程：把当前配置重新提交给原生侧注册，并退出编辑态。
     * 参数：无。
     * 返回：恢复完成 Promise。
     * 边界：网页预览不会触发真实注册；恢复失败时保留编辑态并展示错误。
     */
    async function restoreShortcutRegistration(): Promise<void> {
        try {
            shortcutProfile.value = await registerShortcuts(shortcutProfile.value);
            editingKey.value = '';
            recordingShortcut.value = '';
            message.value = '';
            messageType.value = 'info';
        } catch (error) {
            messageType.value = 'error';
            message.value = error instanceof Error ? error.message : '恢复快捷键失败。';
        }
    }

    /**
     * 读取已保存的快捷键配置。
     * 流程：客户端环境读取客户端 JSON 配置；网页预览环境读取浏览器本地存储，保证开发预览中关闭重开也能回显。
     * 参数：无。
     * 返回：保存过的快捷键配置片段。
     * 边界：网页预览本地存储损坏时清理该项并返回空对象，避免阻塞弹窗打开。
     */
    async function readShortcutProfileConfig(): Promise<Partial<ShortcutProfileModel>> {
        if (isTauriRuntime()) {
            return readClientJson<Partial<ShortcutProfileModel>>(StorageKey.shortcuts, {});
        }
        if (typeof window === 'undefined') return {};
        const rawValue = window.localStorage.getItem(StorageKey.shortcuts);
        if (!rawValue) return {};
        try {
            const value = JSON.parse(rawValue) as unknown;
            return hasShortcutProfileValue(value) ? normalizeShortcutProfileValue(value, shortcutProfile.value) : {};
        } catch {
            window.localStorage.removeItem(StorageKey.shortcuts);
            return {};
        }
    }

    /**
     * 保存快捷键配置。
     * 流程：客户端环境写入客户端 JSON 配置；网页预览环境写入浏览器本地存储用于 UI 自测回显。
     * 参数：profile 为原生注册后返回的完整快捷键配置。
     * 返回：保存完成 Promise。
     * 边界：网页预览保存只影响当前浏览器，不会注册系统级全局快捷键。
     */
    async function persistShortcutProfileConfig(profile: ShortcutProfileModel): Promise<void> {
        if (isTauriRuntime()) {
            await writeClientJson(StorageKey.shortcuts, profile);
            return;
        }
        if (typeof window === 'undefined') return;
        window.localStorage.setItem(StorageKey.shortcuts, JSON.stringify(profile));
    }

    /**
     * 把快捷键字符串拆成展示片段。
     * 流程：按加号拆分后统一修饰键和主键大小写。
     * 参数：shortcut 为原生侧格式的快捷键字符串。
     * 返回：用于 Kbd 组件展示的片段列表。
     * 边界：空值时返回占位文本，避免 UI 空白。
     */
    function splitShortcutParts(shortcut: string): string[] {
        const parts = shortcut
            .split('+')
            .map((part) => formatShortcutPart(part))
            .filter(Boolean);
        return parts.length ? parts : ['未设置'];
    }

    /**
     * 格式化单个快捷键片段。
     * 流程：修饰键使用常见英文缩写，普通按键首字母大写。
     * 参数：part 为原始快捷键片段。
     * 返回：面向用户展示的按键文本。
     * 边界：未知按键保持原文本，避免误丢信息。
     */
    function formatShortcutPart(part: string): string {
        const normalized = part.trim().toLowerCase();
        const labelByPart: Record<string, string> = {
            ctrl: 'Ctrl',
            control: 'Ctrl',
            cmd: 'Cmd',
            meta: 'Cmd',
            alt: 'Alt',
            option: 'Alt',
            shift: 'Shift',
            space: 'Space'
        };
        if (labelByPart[normalized]) return labelByPart[normalized];
        return normalized ? normalized.slice(0, 1).toUpperCase() + normalized.slice(1) : '';
    }

    /**
     * 从键盘事件生成快捷键字符串。
     * 流程：按 Ctrl/Cmd/Alt/Shift 顺序收集修饰键，再追加主键。
     * 参数：event 为用户按下组合键的事件。
     * 返回：原生侧可规范化的快捷键字符串；只有修饰键时返回空字符串。
     * 边界：不允许把 Ctrl、Shift、Alt、Meta 单独作为主键。
     */
    function normalizeKeyboardEvent(event: KeyboardEvent): string {
        const key = normalizeEventKey(event);
        if (!key) return '';
        const parts: string[] = [];
        if (event.ctrlKey) parts.push('ctrl');
        if (event.metaKey) parts.push('cmd');
        if (event.altKey) parts.push('alt');
        if (event.shiftKey) parts.push('shift');
        parts.push(key);
        return parts.join('+');
    }

    /**
     * 规范化 KeyboardEvent 主键。
     * 流程：过滤修饰键，兼容空格、字母和常见符号。
     * 参数：event 为用户按键事件。
     * 返回：可用于快捷键配置的主键。
     * 边界：只按修饰键返回空字符串，避免无效组合。
     */
    function normalizeEventKey(event: KeyboardEvent): string {
        const key = event.key.toLowerCase();
        if (['control', 'shift', 'alt', 'meta'].includes(key)) return '';
        if (key === ' ') return 'space';
        if (key.length === 1) return key;
        return key.replace(/\s+/g, '');
    }
</script>
