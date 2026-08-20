<template>
    <div class="grid w-full gap-5">
        <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="flex flex-wrap items-center gap-3">
                <div class="flex items-center gap-2 text-[13px] text-muted-foreground">
                    <button
                        class="font-medium text-foreground"
                        type="button"
                        @click="handleBackHome">
                        CodexMan
                    </button>
                    <span>/</span>
                    <span>语音转文字润色</span>
                </div>
                <div class="flex items-center gap-1.5">
                    <ui-tooltip>
                        <ui-tooltip-trigger as-child>
                            <button
                                :class="voiceRequirementIconClass(isMicrophoneReady)"
                                type="button"
                                aria-label="查看麦克风授权状态"
                                @click="handleOpenMicrophoneRequirement">
                                <permissions
                                    theme="outline"
                                    size="15" />
                                <span
                                    v-if="!isMicrophoneReady"
                                    class="absolute -right-1 -top-1 flex h-3.5 min-w-3.5 items-center justify-center rounded-full border border-background bg-destructive px-0.5 text-[10px] font-bold leading-none text-destructive-foreground">
                                    !
                                </span>
                            </button>
                        </ui-tooltip-trigger>
                        <ui-tooltip-content>{{ microphoneTooltip }}</ui-tooltip-content>
                    </ui-tooltip>
                </div>
            </div>
            <div class="flex items-center gap-2">
                <ui-dropdown-menu v-model:open="settingMenuOpen">
                    <ui-dropdown-menu-trigger as-child>
                        <ui-button
                            variant="outline"
                            size="icon"
                            type="button"
                            aria-label="打开设置菜单">
                            <setting-two
                                theme="outline"
                                size="16" />
                        </ui-button>
                    </ui-dropdown-menu-trigger>
                    <ui-dropdown-menu-content class="w-44">
                        <ui-dropdown-menu-item @select="shortcutDialogOpen = true">
                            <enter-the-keyboard
                                theme="outline"
                                size="16" />
                            <span>快捷键</span>
                        </ui-dropdown-menu-item>
                    </ui-dropdown-menu-content>
                </ui-dropdown-menu>
                <ui-button
                    variant="outline"
                    type="button"
                    @click="handleOpenDictionaryList">
                    <list
                        theme="outline"
                        size="16" />
                    <span>词典列表</span>
                </ui-button>
            </div>
        </div>

        <section class="grid gap-3">
            <ui-alert class="p-4">
                <div class="flex items-start gap-3">
                    <span
                        class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
                        <setting-two
                            theme="outline"
                            size="16" />
                    </span>
                    <div class="min-w-0">
                        <div class="text-[14px] font-semibold text-foreground">{{ requirementAlertTitle }}</div>
                        <p class="mt-1 text-[12px] leading-5 text-muted-foreground">
                            {{ requirementAlertDescription }}
                        </p>
                        <p
                            v-if="store.message"
                            class="mt-2 text-[12px] leading-5 text-muted-foreground">
                            {{ store.message }}
                        </p>
                    </div>
                </div>
            </ui-alert>
            <div class="grid gap-3 md:grid-cols-2">
                <label class="grid gap-1.5">
                    <span class="text-[12px] font-medium text-foreground">语音识别模型</span>
                    <ui-select-root v-model="selectedAsrModelId">
                        <ui-select-trigger>
                            <ui-select-value placeholder="暂无可用 ASR 模型" />
                        </ui-select-trigger>
                        <ui-select-content>
                            <ui-select-item
                                v-for="model in asrModels"
                                :key="model.id"
                                :value="model.id">
                                {{ model.displayName }}
                            </ui-select-item>
                        </ui-select-content>
                    </ui-select-root>
                </label>
                <label class="grid gap-1.5">
                    <span class="text-[12px] font-medium text-foreground">润色模型</span>
                    <ui-select-root v-model="selectedTextModelId">
                        <ui-select-trigger>
                            <ui-select-value placeholder="暂无可用文本模型" />
                        </ui-select-trigger>
                        <ui-select-content>
                            <ui-select-item
                                v-for="model in textModels"
                                :key="model.id"
                                :value="model.id">
                                {{ model.displayName }}
                            </ui-select-item>
                        </ui-select-content>
                    </ui-select-root>
                </label>
            </div>
            <div
                v-if="isVoiceAsrReady"
                class="flex flex-wrap items-center gap-2">
                <ui-button
                    variant="outline"
                    type="button"
                    :disabled="store.running"
                    @click="handleStartVoice('asr')">
                    <microphone
                        theme="outline"
                        size="16" />
                    <span>{{ store.running ? '处理中' : '语音转文字' }}</span>
                </ui-button>
                <ui-tooltip>
                    <ui-tooltip-trigger as-child>
                        <span
                            class="inline-flex"
                            tabindex="0">
                            <ui-button
                                type="button"
                                :disabled="store.running || !isVoicePolishReady"
                                @click="handleStartVoice('polish')">
                                <magic
                                    theme="outline"
                                    size="16" />
                                <span>{{ store.running ? '处理中' : '转文字并润色' }}</span>
                            </ui-button>
                        </span>
                    </ui-tooltip-trigger>
                    <ui-tooltip-content>{{ polishAvailabilityTooltip }}</ui-tooltip-content>
                </ui-tooltip>
                <ui-button
                    v-if="!isVoicePolishReady"
                    variant="ghost"
                    type="button"
                    @click="handleOpenModelManage">
                    配置润色模型
                </ui-button>
            </div>
        </section>

        <section>
            <div class="grid gap-3">
                <ui-page-state
                    v-if="!isVoiceAsrReady"
                    :icon="SettingTwo"
                    :title="voiceEmptyStateTitle"
                    :description="voiceEmptyStateDescription">
                    <template #action>
                        <ui-button
                            type="button"
                            @click="handlePrimarySetupAction">
                            <setting-two
                                theme="outline"
                                size="16" />
                            <span>{{ voiceEmptyStateActionLabel }}</span>
                        </ui-button>
                    </template>
                </ui-page-state>
                <ui-alert
                    v-for="item in visibleHistory"
                    :key="item.id"
                    class="p-4">
                    <div class="text-[12px] text-muted-foreground">{{ formatHistoryCreatedAt(item.createdAt) }}</div>
                    <div class="mt-2 text-[13px] text-muted-foreground">原文：{{ item.sourceText }}</div>
                    <div class="mt-2 whitespace-pre-wrap text-[14px] font-semibold leading-6 text-foreground">
                        {{ item.outputText }}
                    </div>
                </ui-alert>
                <ui-page-state
                    v-if="isVoiceAsrReady && !visibleHistory.length"
                    :icon="Empty"
                    title="还没有语音处理历史"
                    description="完成一次语音转文字或转文字并润色后，原文、结果和生成时间会在这里形成历史记录。" />
            </div>
        </section>

        <ui-dialog v-model:open="permissionPromptOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>需要麦克风授权</ui-dialog-title>
                    <ui-dialog-description>
                        语音转文字润色需要先开启麦克风权限。前往权限管理页完成授权后，就可以回到这里继续使用。
                    </ui-dialog-description>
                </ui-dialog-header>
                <ui-dialog-footer class="mt-5">
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="permissionPromptOpen = false">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        @click="handleGoPermissionPage">
                        去授权
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
        <voice-polish-shortcut-dialog v-model:open="shortcutDialogOpen" />
    </div>
</template>

<script setup lang="ts">
    import { Empty, EnterTheKeyboard, List, Magic, Microphone, Permissions, SettingTwo } from '@icon-park/vue-next';
    import { toast } from 'vue-sonner';

    import { Alert as UiAlert } from '@/components/ui/alert';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import {
        DropdownMenu as UiDropdownMenu,
        DropdownMenuContent as UiDropdownMenuContent,
        DropdownMenuItem as UiDropdownMenuItem,
        DropdownMenuTrigger as UiDropdownMenuTrigger
    } from '@/components/ui/dropdownMenu';
    import { PageState as UiPageState } from '@/components/ui/pageState';
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import {
        Tooltip as UiTooltip,
        TooltipContent as UiTooltipContent,
        TooltipTrigger as UiTooltipTrigger
    } from '@/components/ui/tooltip';
    import VoicePolishShortcutDialog from '@/components/voicePolish/shortcutDialog.vue';
    import type { VoicePolishRunModeType } from '@/model/voicePolish';
    import { HubRouteName } from '@/router';
    import { isTauriRuntime } from '@/service/tauri/command';
    import { useModelManageStore } from '@/stores/modelManage';
    import { usePermissionStore } from '@/stores/permission';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'VoicePolishView'
    });

    const router = useRouter();
    const store = useVoicePolishStore();
    const permissionStore = usePermissionStore();
    const modelManageStore = useModelManageStore();
    const settingMenuOpen = ref(false);
    const permissionPromptOpen = ref(false);
    const shortcutDialogOpen = ref(false);
    const isClientRuntime = isTauriRuntime();
    const historyTimeFormatter = new Intl.DateTimeFormat('zh-CN', {
        timeZone: 'Asia/Shanghai',
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false
    });
    const asrModels = computed(() => modelManageStore.enabledServiceModels('asr'));
    const textModels = computed(() => modelManageStore.enabledServiceModels('text'));
    const selectedAsrModelId = computed({
        get: () => store.asrModelId,
        set: (modelId: string) => {
            store.asrModelId = modelId;
            store.persistVoicePolish();
        }
    });
    const selectedTextModelId = computed({
        get: () => store.textModelId,
        set: (modelId: string) => {
            store.textModelId = modelId;
            store.persistVoicePolish();
        }
    });
    const microphonePermission = computed(() => permissionStore.items.find((item) => item.key === 'microphone'));
    const isMicrophoneReady = computed(() => Boolean(microphonePermission.value?.ready));
    const isVoiceAsrConfigured = computed(() =>
        Boolean(permissionStore.items.find((item) => item.key === 'httpApi')?.ready)
    );
    const isVoiceAsrReady = computed(
        () => isMicrophoneReady.value && isVoiceAsrConfigured.value && Boolean(store.asrModelId)
    );
    const isVoicePolishReady = computed(() => isVoiceAsrReady.value && Boolean(store.textModelId));
    const visibleHistory = computed(() => store.history);
    const requirementAlertTitle = computed(() => {
        if (!isMicrophoneReady.value) return '还缺少麦克风权限';
        if (!isVoiceAsrConfigured.value) return '还缺少 HTTP 服务授权';
        if (!store.asrModelId) return '还缺少可用 ASR 模型';
        if (!store.textModelId) return '语音转文字已就绪';
        return '语音处理已准备好';
    });
    const requirementAlertDescription = computed(() => {
        if (!isMicrophoneReady.value) {
            return '麦克风权限用于录音收音；公共 HTTP 服务负责语音识别，并可继续整理口语、补齐标点和修正明显误识别。';
        }
        if (!isVoiceAsrConfigured.value) {
            return 'HTTP 服务需要先连接并完成设备码授权，授权成功后才能提交录音。';
        }
        if (!store.asrModelId) {
            return '服务目录中需要已启用的 ASR 模型，才能使用语音转文字。';
        }
        if (!store.textModelId) {
            return 'ASR 模型可用，可以正常语音转文字；配置文本模型后还可继续自动润色。';
        }
        return '服务端语音识别和文本处理能力已就绪：可以只转文字，也可以转文字后继续自动整理和润色。';
    });
    const voiceEmptyStateTitle = computed(() => {
        if (!isMicrophoneReady.value) return '请先开启麦克风权限';
        if (!store.asrModelId) return '请先配置可用 ASR 模型';
        return '语音服务暂不可用';
    });
    const voiceEmptyStateDescription = computed(() => {
        if (!isMicrophoneReady.value && !isClientRuntime) {
            return '网页只负责配置和历史展示，请在 CodexMan App 中授权麦克风并使用全局快捷键。';
        }
        if (!isMicrophoneReady.value) {
            return '语音转文字由 CodexMan App 录音，需要先给 App 开启麦克风权限。';
        }
        if (!store.asrModelId) return '模型目录缺少已启用的 ASR 模型，请前往模型管理配置。';
        return '公共 HTTP 服务未提供语音识别能力，请联系服务管理员检查部署配置。';
    });
    const voiceEmptyStateActionLabel = computed(() => {
        if (!isMicrophoneReady.value && !isClientRuntime) return '查看权限说明';
        if (!isMicrophoneReady.value) return '去开启权限';
        if (!store.asrModelId) return '去模型管理';
        return '检查 HTTP 服务';
    });
    const microphoneTooltip = computed(() => {
        if (isMicrophoneReady.value) return '麦克风权限已开启。';
        return '未开启 CodexMan App 麦克风权限，语音转文字润色无法收音。';
    });
    const polishAvailabilityTooltip = computed(() => {
        if (isVoicePolishReady.value) return '使用当前文本模型整理并润色语音识别结果。';
        return '缺少已启用的文本模型；语音转文字仍可正常使用。';
    });
    let permissionRefreshTimer: number | null = null;

    onMounted(() => {
        void refreshVoicePermission();
        void initializeModelSelections();
    });

    /**
     * 初始化语音页模型选择。
     * 流程：读取服务安全目录，分别校正 ASR 与文本模型 ID，并保存回退后的不透明 ID。
     * 参数：无。
     * 返回：初始化完成 Promise。
     * 边界：目录不可达或没有对应能力时保留空选择并展示明确错误，不使用前端预设猜测模型。
     */
    async function initializeModelSelections(): Promise<void> {
        await modelManageStore.hydrateModelManage();
        const asrSelection = modelManageStore.resolveSelection('asr', store.asrModelId, '语音转文字');
        const textSelection = modelManageStore.resolveSelection('text', store.textModelId, '语音转文字润色');
        store.asrModelId = asrSelection.modelId;
        store.textModelId = textSelection.modelId;
        store.persistVoicePolish();
        store.message = [modelManageStore.message, asrSelection.message, textSelection.message]
            .filter(Boolean)
            .join(' ');
    }

    onUnmounted(() => {
        stopPermissionRefreshTimer();
    });

    /**
     * 从面包屑回到语音转文字润色首页。
     * 流程：使用当前模块路由名称导航，保持桌面端和网页预览 URL 一致。
     * 参数：无。
     * 返回：无返回值。
     * 边界：当前已在首页时由 Vue Router 自身忽略重复导航。
     */
    function handleBackHome(): void {
        void router.push({ name: HubRouteName.VoicePolish });
    }

    /**
     * 构造顶部前置条件图标样式。
     * 流程：根据当前前置条件是否满足返回成功或警告状态类名，供权限、ASR 和文本模型三个图标复用。
     * 参数：ready 表示对应前置条件是否已满足。
     * 返回：图标外层 class 字符串。
     * 边界：只影响视觉状态，不触发任何业务操作。
     */
    function voiceRequirementIconClass(ready: boolean): string {
        return [
            'relative flex h-8 w-8 items-center justify-center rounded-md border transition-colors hover:bg-muted focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2',
            ready
                ? 'border-primary/35 bg-primary/10 text-primary'
                : 'border-destructive/40 bg-destructive/10 text-destructive'
        ].join(' ');
    }

    /**
     * 刷新语音润色所需权限状态。
     * 流程：进入页面时刷新一次权限，并通过临时音频流确认麦克风真实可用；如果仍未满足，则启动 10 秒轮询，授权后停止轮询。
     * 参数：无。
     * 返回：无返回值。
     * 边界：轮询只刷新权限 Store，不读取或修改 ASR、文本模型状态。
     */
    async function refreshVoicePermission(): Promise<void> {
        await permissionStore.refreshPermissions({ probeMicrophoneAccess: true });
        if (isMicrophoneReady.value) {
            stopPermissionRefreshTimer();
            return;
        }
        startPermissionRefreshTimer();
    }

    /**
     * 启动权限刷新定时器。
     * 流程：如果当前没有定时器，则每 10 秒刷新一次权限；刷新到麦克风已授权后自动停止。
     * 参数：无。
     * 返回：无返回值。
     * 边界：只允许存在一个定时器，避免重复进入页面时多次轮询。
     */
    function startPermissionRefreshTimer(): void {
        if (permissionRefreshTimer !== null) return;
        permissionRefreshTimer = window.setInterval(() => {
            void refreshVoicePermission();
        }, 10000);
    }

    /**
     * 停止权限刷新定时器。
     * 流程：清理当前定时器并重置引用，避免离开页面后继续刷新权限。
     * 参数：无。
     * 返回：无返回值。
     * 边界：没有定时器时直接返回。
     */
    function stopPermissionRefreshTimer(): void {
        if (permissionRefreshTimer === null) return;
        window.clearInterval(permissionRefreshTimer);
        permissionRefreshTimer = null;
    }

    /**
     * 打开语音润色词典列表页。
     * 流程：跳转到同模块词典子路由，由词典页负责展示和维护词条。
     * 参数：无。
     * 返回：无返回值。
     * 边界：路由跳转失败时不影响当前历史列表展示。
     */
    function handleOpenDictionaryList(): void {
        void router.push({ name: HubRouteName.VoicePolishDictionary });
    }

    /**
     * 打开麦克风前置条件提示。
     * 流程：普通 Web 只展示产品提示；客户端委托权限 Store 打开系统麦克风设置。
     * 参数：无。
     * 返回：无返回值。
     * 边界：页面本身不调用浏览器麦克风，授权后通过 App 原生诊断刷新状态。
     */
    async function handleOpenMicrophoneRequirement(): Promise<void> {
        if (!isClientRuntime) {
            toast.info('语音由 CodexMan App 录音', {
                description: '请打开 CodexMan App，并在 macOS 系统设置中允许 CodexMan 使用麦克风。'
            });
            return;
        }
        await permissionStore.openPermission('microphone');
        await refreshVoicePermission();
        if (!isMicrophoneReady.value) {
            permissionPromptOpen.value = true;
        }
    }

    /**
     * 前往权限管理页面。
     * 流程：打开 App 麦克风系统设置，再按最新状态决定是否跳转权限管理路由。
     * 参数：无。
     * 返回：无返回值。
     * 边界：授权成功时停留在当前页面继续使用；失败时进入权限管理页展示诊断和系统设置入口。
     */
    async function handleGoPermissionPage(): Promise<void> {
        permissionPromptOpen.value = false;
        await permissionStore.openPermission('microphone');
        await refreshVoicePermission();
        if (!isMicrophoneReady.value) {
            void router.push({ name: HubRouteName.Permission });
        }
    }

    /**
     * 处理空态主按钮点击。
     * 流程：优先处理麦克风权限；能力异常时进入 HTTP API 文档页查看服务地址、鉴权与错误码。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不会在浏览器预览中尝试申请系统权限，避免给用户造成可以网页授权的误解。
     */
    function handlePrimarySetupAction(): void {
        if (!isMicrophoneReady.value) {
            if (!isClientRuntime) {
                void router.push({ name: HubRouteName.Permission });
                return;
            }
            void handleGoPermissionPage();
            return;
        }
        if (!store.asrModelId) {
            handleOpenModelManage();
            return;
        }
        void router.push({ name: HubRouteName.HttpApiDoc });
    }

    /**
     * 前往模型管理页面。
     * 流程：从语音页的 ASR 空态或润色缺失引导跳转到统一模型管理路由。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不会修改当前选择，用户返回后页面会按服务目录重新校正。
     */
    function handleOpenModelManage(): void {
        void router.push({ name: HubRouteName.ModelManage });
    }

    /**
     * 格式化语音历史创建时间。
     * 流程：兼容旧版毫秒/秒时间戳和新版 ISO 字符串，统一按北京时间展示。
     * 参数：createdAt 为历史记录创建时间。
     * 返回：用户可读的北京时间；无法解析时显示兜底文案。
     * 边界：不展示 Invalid Date，避免历史脏数据直接暴露给用户。
     */
    function formatHistoryCreatedAt(createdAt: string): string {
        if (!createdAt) return '时间未记录';
        const normalizedCreatedAt = createdAt.trim();
        const numericTimestamp = Number(normalizedCreatedAt);
        const date =
            Number.isFinite(numericTimestamp) && numericTimestamp > 0
                ? new Date(numericTimestamp < 1_000_000_000_000 ? numericTimestamp * 1000 : numericTimestamp)
                : new Date(normalizedCreatedAt);
        if (Number.isNaN(date.getTime())) return '时间未记录';
        return `${historyTimeFormatter.format(date)} 北京时间`;
    }

    /**
     * 手动开始语音处理。
     * 流程：桌面端通过 App 主进程执行录音、ASR、可选润色和粘贴链路。
     * 参数：mode 为本次语音处理模式，asr 只转文字，polish 会继续调用文本模型润色。
     * 返回：无返回值。
     * 边界：普通 Web 不录音，只展示需要使用 CodexMan App 的提示。
     */
    async function handleStartVoice(mode: VoicePolishRunModeType): Promise<void> {
        if (!isClientRuntime) {
            await store.runVoicePolish('', mode);
            return;
        }
        await refreshVoicePermission();
        if (!isMicrophoneReady.value) {
            permissionPromptOpen.value = true;
            return;
        }
        void store.runVoicePolish('', mode);
    }
</script>
