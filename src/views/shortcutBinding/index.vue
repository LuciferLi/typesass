<template>
    <section class="grid min-h-0 gap-5">
        <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="grid gap-1">
                <h1 class="text-[18px] font-semibold leading-7 text-foreground">快捷键绑定</h1>
                <p class="max-w-[640px] text-[13px] leading-6 text-muted-foreground">
                    为常用应用绑定全局快捷键，按下后直接打开对应 App。
                </p>
            </div>
            <ui-button
                type="button"
                @click="handleOpenCreateDialog">
                <plus
                    theme="outline"
                    size="15" />
                创建
            </ui-button>
        </div>

        <ui-page-state
            v-if="!shortcutProfile.appBindings.length"
            :icon="KeyboardOne"
            title="还没有快捷键绑定"
            description="创建后会以横向卡片展示在这里。">
            <template #action>
                <ui-button
                    type="button"
                    @click="handleOpenCreateDialog">
                    创建
                </ui-button>
            </template>
        </ui-page-state>

        <div
            v-else
            class="flex gap-3 overflow-x-auto pb-2">
            <article
                v-for="binding in shortcutProfile.appBindings"
                :key="binding.id"
                class="grid w-[280px] shrink-0 gap-4 rounded-lg border border-border bg-card p-4 shadow-sm">
                <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0">
                        <div class="truncate text-[15px] font-semibold text-foreground">{{ binding.appName }}</div>
                        <div class="mt-1 text-[12px] text-muted-foreground">打开应用</div>
                    </div>
                    <ui-button
                        variant="outline"
                        size="sm"
                        type="button"
                        :disabled="saving"
                        @click="handleDeleteBinding(binding.id)">
                        删除
                    </ui-button>
                </div>
                <ui-kbd-group>
                    <template
                        v-for="(part, index) in splitShortcutParts(binding.shortcut)"
                        :key="`${binding.id}-${part}-${index}`">
                        <ui-kbd>{{ part }}</ui-kbd>
                        <span
                            v-if="index < splitShortcutParts(binding.shortcut).length - 1"
                            class="text-[12px] text-muted-foreground"
                            >+</span
                        >
                    </template>
                </ui-kbd-group>
                <div class="grid gap-1">
                    <span class="text-[12px] text-muted-foreground">目标路径</span>
                    <span class="truncate text-[12px] text-foreground/80">{{ binding.appPath }}</span>
                </div>
            </article>
        </div>

        <p
            v-if="message"
            :class="['text-[12px] leading-5', messageType === 'error' ? 'text-destructive' : 'text-muted-foreground']">
            {{ message }}
        </p>

        <ui-dialog
            :open="createDialogOpen"
            @update:open="handleCreateDialogOpenChange">
            <ui-dialog-content class="max-w-[520px]">
                <ui-dialog-header>
                    <ui-dialog-title>创建快捷键绑定</ui-dialog-title>
                    <ui-dialog-description>当前只支持将快捷键绑定到打开应用动作。</ui-dialog-description>
                </ui-dialog-header>
                <div class="grid gap-4 py-2">
                    <div class="grid gap-2">
                        <ui-label>输入快捷键</ui-label>
                        <ui-input
                            :model-value="recordingShortcut || '按下组合键'"
                            readonly
                            autofocus
                            @keydown.prevent="handleShortcutKeydown" />
                    </div>
                    <div class="grid gap-2">
                        <ui-label>动作类型</ui-label>
                        <ui-select
                            :model-value="formActionType"
                            disabled>
                            <ui-select-trigger>
                                <ui-select-value />
                            </ui-select-trigger>
                            <ui-select-content>
                                <ui-select-item value="openApp">打开应用</ui-select-item>
                            </ui-select-content>
                        </ui-select>
                    </div>
                    <div
                        v-if="formActionType === 'openApp'"
                        class="grid gap-2">
                        <ui-label>选择 APP</ui-label>
                        <div class="grid gap-2">
                            <ui-input
                                v-model="applicationKeyword"
                                :placeholder="applicationSelectPlaceholder"
                                :disabled="applicationLoading || !applicationOptions.length"
                                @focus="applicationSearchFocused = true"
                                @input="handleApplicationKeywordInput" />
                            <div
                                v-if="shouldShowApplicationMatches"
                                class="max-h-[220px] overflow-y-auto rounded-md border border-border bg-popover p-1 shadow-md">
                                <button
                                    v-for="application in filteredApplicationOptions"
                                    :key="application.path"
                                    type="button"
                                    class="grid w-full gap-0.5 rounded-sm px-2 py-2 text-left text-[13px] outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent"
                                    @click="handleSelectApplication(application)">
                                    <span class="truncate font-medium">{{ application.name }}</span>
                                    <span class="truncate text-[11px] text-muted-foreground">{{
                                        application.path
                                    }}</span>
                                </button>
                                <div
                                    v-if="!filteredApplicationOptions.length"
                                    class="px-2 py-6 text-center text-[12px] text-muted-foreground">
                                    没有匹配的 APP
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
                <p
                    v-if="dialogMessage"
                    :class="[
                        'text-[12px] leading-5',
                        dialogMessageType === 'error' ? 'text-destructive' : 'text-muted-foreground'
                    ]">
                    {{ dialogMessage }}
                </p>
                <ui-dialog-footer>
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="saving"
                        @click="handleCancelCreate">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="!canSaveBinding || saving"
                        @click="handleSaveBinding">
                        保存
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
    </section>
</template>

<script setup lang="ts">
    import { KeyboardOne, Plus } from '@icon-park/vue-next';

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
    import { Label as UiLabel } from '@/components/ui/label';
    import { PageState as UiPageState } from '@/components/ui/pageState';
    import {
        Select as UiSelect,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import { StorageKey } from '@/config/storageKey';
    import type { ShortcutProfileModel } from '@/model/permission';
    import type {
        ApplicationOptionModel,
        OpenAppShortcutBindingModel,
        ShortcutBindingActionType
    } from '@/model/shortcutBinding';
    import {
        DefaultShortcutProfile,
        hasShortcutProfileValue,
        normalizeKeyboardEvent,
        normalizeShortcutProfileValue,
        splitShortcutParts
    } from '@/service/shortcut/shortcutProfile';
    import { readClientJson, writeClientJson } from '@/service/storage/clientJsonStorage';
    import {
        getRuntimeDiagnostics,
        isTauriRuntime,
        listInstalledApplications,
        registerShortcuts,
        suspendShortcutsForRecording
    } from '@/service/tauri/command';

    defineOptions({
        name: 'ShortcutBindingView'
    });

    const shortcutProfile = ref<ShortcutProfileModel>({ ...DefaultShortcutProfile, appBindings: [] });
    const applicationOptions = ref<ApplicationOptionModel[]>([]);
    const createDialogOpen = ref(false);
    const recordingShortcut = ref('');
    const selectedAppPath = ref('');
    const applicationKeyword = ref('');
    const applicationSearchFocused = ref(false);
    const saving = ref(false);
    const applicationLoading = ref(false);
    const message = ref('');
    const messageType = ref<'info' | 'error'>('info');
    const dialogMessage = ref('');
    const dialogMessageType = ref<'info' | 'error'>('info');
    const formActionType: ShortcutBindingActionType = 'openApp';

    const selectedApplication = computed<ApplicationOptionModel | undefined>(() => {
        return applicationOptions.value.find((application) => application.path === selectedAppPath.value);
    });
    const filteredApplicationOptions = computed<ApplicationOptionModel[]>(() => {
        const keyword = applicationKeyword.value.trim().toLowerCase();
        if (!keyword) return applicationOptions.value.slice(0, 8);
        return applicationOptions.value
            .filter((application) => {
                return (
                    application.name.toLowerCase().includes(keyword) || application.path.toLowerCase().includes(keyword)
                );
            })
            .slice(0, 12);
    });
    const canSaveBinding = computed<boolean>(() => {
        return Boolean(recordingShortcut.value && selectedApplication.value);
    });
    const shouldShowApplicationMatches = computed<boolean>(() => {
        return applicationSearchFocused.value && !applicationLoading.value && applicationOptions.value.length > 0;
    });
    const applicationSelectPlaceholder = computed<string>(() => {
        if (applicationLoading.value) return '正在读取应用列表';
        return applicationOptions.value.length ? '输入 APP 名称搜索' : '未读取到可选 APP';
    });

    onMounted(() => {
        void hydrateShortcutProfile();
    });

    /**
     * 同步桌面端或本地保存的快捷键配置。
     * 流程：优先读取客户端 JSON；没有本地保存值时读取原生运行诊断，最后回退默认配置。
     * 参数：无。
     * 返回：无返回值。
     * 边界：读取失败时保留默认配置并展示错误，不阻塞页面继续创建绑定。
     */
    async function hydrateShortcutProfile(): Promise<void> {
        try {
            const savedProfile = await readShortcutProfileConfig();
            if (hasShortcutProfileValue(savedProfile)) {
                shortcutProfile.value = normalizeShortcutProfileValue(savedProfile, shortcutProfile.value);
                return;
            }
            const diagnostics = await getRuntimeDiagnostics();
            shortcutProfile.value = normalizeShortcutProfileValue(
                diagnostics?.shortcuts ?? shortcutProfile.value,
                shortcutProfile.value
            );
        } catch (error) {
            messageType.value = 'error';
            message.value = error instanceof Error ? error.message : '读取快捷键绑定失败。';
        }
    }

    /**
     * 打开创建弹窗。
     * 流程：重置表单、暂停已有全局快捷键，再异步读取本机应用列表供选择。
     * 参数：无。
     * 返回：无返回值。
     * 边界：暂停或读取失败时仍打开弹窗并展示错误，避免用户误以为按钮无响应。
     */
    async function handleOpenCreateDialog(): Promise<void> {
        resetCreateForm();
        createDialogOpen.value = true;
        try {
            await suspendShortcutsForRecording();
            dialogMessage.value = '请按下新的组合键。';
            dialogMessageType.value = 'info';
        } catch (error) {
            dialogMessageType.value = 'error';
            dialogMessage.value = error instanceof Error ? error.message : '暂停快捷键失败。';
        }
        await hydrateApplicationOptions();
    }

    /**
     * 接收 Dialog 请求的开关变化。
     * 流程：关闭时走取消创建逻辑以恢复旧快捷键注册；打开由创建按钮统一处理。
     * 参数：open 为 Dialog 组件请求的新状态。
     * 返回：无返回值。
     * 边界：外部关闭如果恢复失败会重新保持打开，让错误对用户可见。
     */
    function handleCreateDialogOpenChange(open: boolean): void {
        if (open) {
            createDialogOpen.value = true;
            return;
        }
        void handleCancelCreate();
    }

    /**
     * 读取本机应用选项。
     * 流程：桌面端调用 Tauri 扫描 .app；网页预览提供少量开发兜底数据方便 UI 验证。
     * 参数：无。
     * 返回：无返回值。
     * 边界：扫描失败不清空已有选项，只在弹窗内提示错误。
     */
    async function hydrateApplicationOptions(): Promise<void> {
        applicationLoading.value = true;
        try {
            applicationOptions.value = isTauriRuntime()
                ? await listInstalledApplications()
                : [
                      { name: 'Safari', path: '/Applications/Safari.app' },
                      { name: 'Google Chrome', path: '/Applications/Google Chrome.app' },
                      { name: 'ChatGPT', path: '/Applications/ChatGPT.app' }
                  ];
        } catch (error) {
            dialogMessageType.value = 'error';
            dialogMessage.value = error instanceof Error ? error.message : '读取 APP 列表失败。';
        } finally {
            applicationLoading.value = false;
        }
    }

    /**
     * 记录用户按下的新快捷键。
     * 流程：从 KeyboardEvent 中生成统一快捷键字符串，并写入当前创建表单。
     * 参数：event 为输入框 keydown 事件。
     * 返回：无返回值。
     * 边界：只按修饰键时不记录，避免保存无效快捷键。
     */
    function handleShortcutKeydown(event: KeyboardEvent): void {
        const shortcut = normalizeKeyboardEvent(event);
        if (!shortcut) return;
        recordingShortcut.value = shortcut;
        dialogMessage.value = '已录入，选择 APP 后可保存。';
        dialogMessageType.value = 'info';
    }

    /**
     * 处理应用搜索输入。
     * 流程：输入内容变化时清空已选应用，并保持匹配列表可见，等待用户点击具体 App。
     * 参数：无。
     * 返回：无返回值。
     * 边界：如果输入内容仍等于已选应用名称，不清空选择，避免点击结果后的 input 事件误清状态。
     */
    function handleApplicationKeywordInput(): void {
        if (selectedApplication.value?.name === applicationKeyword.value) return;
        selectedAppPath.value = '';
        applicationSearchFocused.value = true;
    }

    /**
     * 选择目标应用。
     * 流程：保存应用路径并把输入框回填为应用名称，同时收起匹配列表。
     * 参数：application 为用户从模糊匹配结果中点击的目标 App。
     * 返回：无返回值。
     * 边界：只允许来自扫描列表的应用进入保存流程，避免保存不可打开的任意文本。
     */
    function handleSelectApplication(application: ApplicationOptionModel): void {
        selectedAppPath.value = application.path;
        applicationKeyword.value = application.name;
        applicationSearchFocused.value = false;
    }

    /**
     * 保存新的打开应用快捷键绑定。
     * 流程：创建绑定记录、提交完整快捷键配置给原生注册，成功后持久化并刷新横向卡片。
     * 参数：无。
     * 返回：无返回值。
     * 边界：重复快捷键或 App 路径异常由原生侧拒绝；失败时保持弹窗打开以便修正。
     */
    async function handleSaveBinding(): Promise<void> {
        const application = selectedApplication.value;
        if (!recordingShortcut.value || !application) return;
        saving.value = true;
        dialogMessage.value = '';
        const binding: OpenAppShortcutBindingModel = {
            id: createBindingId(),
            shortcut: recordingShortcut.value,
            actionType: 'openApp',
            appName: application.name,
            appPath: application.path,
            createdAt: new Date().toISOString()
        };
        const nextProfile: ShortcutProfileModel = {
            ...shortcutProfile.value,
            appBindings: [binding, ...shortcutProfile.value.appBindings]
        };
        try {
            const savedProfile = await registerShortcuts(nextProfile);
            shortcutProfile.value = savedProfile;
            await persistShortcutProfileConfig(savedProfile);
            createDialogOpen.value = false;
            resetCreateForm();
            messageType.value = 'info';
            message.value = '快捷键绑定已保存。';
        } catch (error) {
            dialogMessageType.value = 'error';
            dialogMessage.value = error instanceof Error ? error.message : '保存快捷键绑定失败。';
        } finally {
            saving.value = false;
        }
    }

    /**
     * 删除指定快捷键绑定。
     * 流程：从完整配置中过滤目标 ID，重新注册剩余快捷键，并同步持久化结果。
     * 参数：bindingId 为待删除绑定 ID。
     * 返回：无返回值。
     * 边界：注册失败时不更新页面，避免 UI 展示与系统实际快捷键不一致。
     */
    async function handleDeleteBinding(bindingId: string): Promise<void> {
        saving.value = true;
        message.value = '';
        const nextProfile: ShortcutProfileModel = {
            ...shortcutProfile.value,
            appBindings: shortcutProfile.value.appBindings.filter((binding) => binding.id !== bindingId)
        };
        try {
            const savedProfile = await registerShortcuts(nextProfile);
            shortcutProfile.value = savedProfile;
            await persistShortcutProfileConfig(savedProfile);
            messageType.value = 'info';
            message.value = '快捷键绑定已删除。';
        } catch (error) {
            messageType.value = 'error';
            message.value = error instanceof Error ? error.message : '删除快捷键绑定失败。';
        } finally {
            saving.value = false;
        }
    }

    /**
     * 取消创建并恢复已保存的快捷键注册。
     * 流程：把当前页面配置重新提交给原生侧注册，成功后关闭弹窗并清空表单。
     * 参数：无。
     * 返回：无返回值。
     * 边界：恢复失败时保留弹窗并展示错误，避免全局快捷键停留在暂停状态。
     */
    async function handleCancelCreate(): Promise<void> {
        try {
            shortcutProfile.value = await registerShortcuts(shortcutProfile.value);
            createDialogOpen.value = false;
            resetCreateForm();
        } catch (error) {
            createDialogOpen.value = true;
            dialogMessageType.value = 'error';
            dialogMessage.value = error instanceof Error ? error.message : '恢复快捷键失败。';
        }
    }

    /**
     * 重置创建表单。
     * 流程：清空快捷键、应用选择和弹窗提示。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不清空应用列表，避免用户连续创建时重复等待扫描。
     */
    function resetCreateForm(): void {
        recordingShortcut.value = '';
        selectedAppPath.value = '';
        applicationKeyword.value = '';
        applicationSearchFocused.value = false;
        dialogMessage.value = '';
        dialogMessageType.value = 'info';
    }

    /**
     * 创建绑定 ID。
     * 流程：优先使用浏览器安全随机 UUID，不可用时用时间戳兜底。
     * 参数：无。
     * 返回：前端唯一 ID 字符串。
     * 边界：兜底 ID 只用于极旧运行时，仍带随机片段降低冲突概率。
     */
    function createBindingId(): string {
        if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
            return crypto.randomUUID();
        }
        return `binding-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    }

    /**
     * 读取已保存的快捷键配置。
     * 流程：客户端环境读取客户端 JSON 配置；网页预览环境读取浏览器本地存储。
     * 参数：无。
     * 返回：保存过的快捷键配置片段。
     * 边界：本地存储损坏时清理该项并返回空对象，避免阻塞页面打开。
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
</script>
