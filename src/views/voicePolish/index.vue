<template>
    <div class="grid w-full gap-5">
        <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="flex flex-wrap items-center gap-3">
                <div class="flex items-center gap-2 text-[13px] text-muted-foreground">
                    <button
                        class="font-medium text-foreground"
                        type="button"
                        @click="handleBackHome">
                        typesass
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
                    <ui-tooltip>
                        <ui-tooltip-trigger as-child>
                            <button
                                :class="voiceRequirementIconClass(Boolean(selectedAsrModel))"
                                type="button"
                                aria-label="选择 ASR 模型"
                                @click="handleOpenModelSetting('asr')">
                                <microphone
                                    theme="outline"
                                    size="15" />
                                <span
                                    v-if="!selectedAsrModel"
                                    class="absolute -right-1 -top-1 flex h-3.5 min-w-3.5 items-center justify-center rounded-full border border-background bg-destructive px-0.5 text-[10px] font-bold leading-none text-destructive-foreground">
                                    !
                                </span>
                            </button>
                        </ui-tooltip-trigger>
                        <ui-tooltip-content>{{ asrModelTooltip }}</ui-tooltip-content>
                    </ui-tooltip>
                    <ui-tooltip>
                        <ui-tooltip-trigger as-child>
                            <button
                                :class="voiceRequirementIconClass(Boolean(selectedTextModel))"
                                type="button"
                                aria-label="选择润色文本模型"
                                @click="handleOpenModelSetting('text')">
                                <magic
                                    theme="outline"
                                    size="15" />
                                <span
                                    v-if="!selectedTextModel"
                                    class="absolute -right-1 -top-1 flex h-3.5 min-w-3.5 items-center justify-center rounded-full border border-background bg-destructive px-0.5 text-[10px] font-bold leading-none text-destructive-foreground">
                                    !
                                </span>
                            </button>
                        </ui-tooltip-trigger>
                        <ui-tooltip-content>{{ textModelTooltip }}</ui-tooltip-content>
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
                        <ui-dropdown-menu-item @select="handleOpenModelSetting('asr')">
                            <microphone
                                theme="outline"
                                size="16" />
                            <span>ASR 模型设置</span>
                        </ui-dropdown-menu-item>
                        <ui-dropdown-menu-item @select="handleOpenModelSetting('text')">
                            <magic
                                theme="outline"
                                size="16" />
                            <span>润色模型设置</span>
                        </ui-dropdown-menu-item>
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
            <div
                v-if="isVoiceAsrConfigured"
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
                <ui-button
                    type="button"
                    :disabled="store.running"
                    @click="handleStartVoice('polish')">
                    <magic
                        theme="outline"
                        size="16" />
                    <span>{{ store.running ? '处理中' : '转文字并润色' }}</span>
                </ui-button>
            </div>
        </section>

        <section>
            <div class="grid gap-3">
                <ui-page-state
                    v-if="!isVoiceAsrConfigured"
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
                    <div class="text-[12px] text-muted-foreground">{{ new Date(item.createdAt).toLocaleString() }}</div>
                    <div class="mt-2 text-[13px] text-muted-foreground">原文：{{ item.sourceText }}</div>
                    <div class="mt-2 whitespace-pre-wrap text-[14px] font-semibold leading-6 text-foreground">
                        {{ item.outputText }}
                    </div>
                </ui-alert>
                <ui-page-state
                    v-if="isVoiceAsrConfigured && !visibleHistory.length"
                    :icon="Empty"
                    title="还没有语音处理历史"
                    description="完成一次语音转文字或转文字并润色后，原文、结果和生成时间会在这里形成历史记录。" />
            </div>
        </section>

        <voice-polish-model-setting-dialog
            v-model:open="modelSettingDialogOpen"
            :mode="modelSettingMode" />
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
        <model-manage-model-form-dialog
            v-model:open="modelGuideDialogOpen"
            :group="modelGuideGroup"
            :title="modelGuideDialogTitle"
            :save-model="handleAddGuidedModel" />
        <ui-dialog v-model:open="textPolishPromptOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>是否开启文本 AI 润色？</ui-dialog-title>
                    <ui-dialog-description>
                        文本 AI 润色可以在 ASR
                        识别后自动整理口语、修正明显误识别、补齐标点，让语音内容更适合直接发送或记录。
                    </ui-dialog-description>
                </ui-dialog-header>
                <ui-dialog-footer class="mt-5">
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="handleDeferTextModel">
                        以后添加
                    </ui-button>
                    <ui-button
                        type="button"
                        @click="handleAddTextModelNow">
                        立即添加
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

    import ModelManageModelFormDialog from '@/components/modelManage/modelFormDialog.vue';
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
        Tooltip as UiTooltip,
        TooltipContent as UiTooltipContent,
        TooltipTrigger as UiTooltipTrigger
    } from '@/components/ui/tooltip';
    import VoicePolishModelSettingDialog from '@/components/voicePolish/modelSettingDialog.vue';
    import VoicePolishShortcutDialog from '@/components/voicePolish/shortcutDialog.vue';
    import type { ModelFormModel, ModelGroupType, ModelItemModel } from '@/model/modelManage';
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
    const modelStore = useModelManageStore();
    const permissionStore = usePermissionStore();
    const settingMenuOpen = ref(false);
    const modelSettingDialogOpen = ref(false);
    const modelSettingMode = ref<VoicePolishModelSettingMode>('all');
    const modelGuideDialogOpen = ref(false);
    const modelGuideGroup = ref<ModelGroupType>('asr');
    const permissionPromptOpen = ref(false);
    const textPolishPromptOpen = ref(false);
    const shortcutDialogOpen = ref(false);
    const isClientRuntime = isTauriRuntime();
    const asrModels = computed(() => modelStore.groupModels('asr'));
    const textModels = computed(() => modelStore.groupModels('text'));
    const selectedAsrModel = computed(() => modelStore.modelById(store.selectedAsrModelId));
    const selectedTextModel = computed(() => modelStore.modelById(store.selectedTextModelId));
    const microphonePermission = computed(() => permissionStore.items.find((item) => item.key === 'microphone'));
    const isMicrophoneReady = computed(() => Boolean(microphonePermission.value?.ready));
    const isVoiceAsrConfigured = computed(() => Boolean(selectedAsrModel.value));
    const isDictationPolishConfigured = computed(() => Boolean(selectedAsrModel.value && selectedTextModel.value));
    const visibleHistory = computed(() => (isVoiceAsrConfigured.value ? store.history : []));
    const requirementAlertTitle = computed(() => {
        if (!isMicrophoneReady.value) return '还缺少麦克风权限';
        if (!selectedAsrModel.value) return '还缺少 ASR 模型';
        if (!selectedTextModel.value) return '可先语音转文字，润色模型还未配置';
        return '语音处理已准备好';
    });
    const requirementAlertDescription = computed(() => {
        if (!isMicrophoneReady.value) {
            return '麦克风权限用于录音收音；ASR 模型用于把语音识别成文字；润色模型用于在识别后整理口语、补齐标点和修正明显误识别。';
        }
        if (!selectedAsrModel.value) {
            return 'ASR 模型是语音转文字的必需配置，没有它就无法把录音识别成文字；润色模型只在需要整理口语和标点时使用。';
        }
        if (!selectedTextModel.value) {
            return '当前已经可以做语音转文字；如果希望识别后自动整理口语、补齐标点和润色表达，再配置文本润色模型。';
        }
        return '麦克风、ASR 模型和润色模型都已具备：可以只转文字，也可以转文字后继续自动整理和润色。';
    });
    const voiceEmptyStateTitle = computed(() => {
        if (!isMicrophoneReady.value) return '请先开启麦克风权限';
        return '请先设置 ASR 模型';
    });
    const voiceEmptyStateDescription = computed(() => {
        if (!isMicrophoneReady.value && !isClientRuntime) {
            return '浏览器预览无法开启本机系统权限。请在 typesass App 里打开权限管理，先开启麦克风权限，再回来配置 ASR 模型。';
        }
        if (!isMicrophoneReady.value) {
            return '语音转文字需要先获得麦克风权限用于录音收音。开启权限后，再配置 ASR 模型把语音识别成文字。';
        }
        return '语音转文字需要先配置可用的 ASR 模型。需要润色时，再补充文本大模型。完成一次处理后，历史记录会在这里展示。';
    });
    const voiceEmptyStateActionLabel = computed(() => {
        if (!isMicrophoneReady.value && !isClientRuntime) return '请用 App 打开';
        if (!isMicrophoneReady.value) return '去开启权限';
        return modelGuideActionLabel.value;
    });
    const microphoneTooltip = computed(() => {
        if (isMicrophoneReady.value) return '麦克风权限已开启。';
        return '未开启麦克风权限，语音转文字润色无法收音。';
    });
    const asrModelTooltip = computed(() => {
        if (selectedAsrModel.value) return `ASR 模型已选择：${selectedAsrModel.value.name}`;
        if (asrModels.value.length) return '已有 ASR 模型，请先选择语音润色要使用的模型。';
        return '还没有 ASR 模型，请先添加语音识别模型。';
    });
    const textModelTooltip = computed(() => {
        if (selectedTextModel.value) return `文本模型已选择：${selectedTextModel.value.name}`;
        if (textModels.value.length) return '已有文本模型，请先选择语音润色要使用的模型。';
        return '还没有文本模型，无法进行语音内容整理和润色。';
    });
    const modelGuideActionLabel = computed(() => {
        if (!asrModels.value.length) return '请添加 ASR 模型';
        if (!textModels.value.length) return '添加大模型（文本模型）';
        if (!selectedAsrModel.value) return '选择 ASR 模型';
        if (!selectedTextModel.value) return '选择大模型（文本模型）';
        return '设置模型';
    });
    const modelGuideDialogTitle = computed(() => {
        if (modelGuideGroup.value === 'asr') return '添加 ASR 模型';
        return '添加文本模型';
    });
    let permissionRefreshTimer: number | null = null;

    onMounted(() => {
        void refreshVoicePermission();
    });

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
     * 流程：进入页面时刷新一次权限；如果麦克风权限仍未满足，则启动 10 秒轮询，授权后停止轮询。
     * 参数：无。
     * 返回：无返回值。
     * 边界：轮询只刷新权限 Store，不读取或修改 ASR、文本模型状态。
     */
    async function refreshVoicePermission(): Promise<void> {
        await permissionStore.refreshPermissions();
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
     * 流程：网页预览环境提示用户改用 APP 打开；客户端环境弹出授权引导，用户确认后再进入权限管理页。
     * 参数：无。
     * 返回：无返回值。
     * 边界：麦克风已授权时仍允许进入权限管理页查看状态，不直接触发系统授权能力。
     */
    function handleOpenMicrophoneRequirement(): void {
        if (!isClientRuntime) {
            showClientRuntimeToast();
            return;
        }
        permissionPromptOpen.value = true;
    }

    /**
     * 前往权限管理页面。
     * 流程：关闭当前授权提示弹窗，再跳转到权限管理路由，由权限页负责展示和执行真实授权动作。
     * 参数：无。
     * 返回：无返回值。
     * 边界：路由跳转失败不会改变语音润色模型选择状态。
     */
    function handleGoPermissionPage(): void {
        permissionPromptOpen.value = false;
        void router.push({ name: HubRouteName.Permission });
    }

    /**
     * 打开指定类型的语音润色模型设置弹窗。
     * 流程：先记录弹窗模式，再打开弹窗，让下拉菜单的 ASR 与润色模型入口分别展示对应配置项。
     * 参数：mode 为模型设置弹窗模式，asr 只展示语音识别模型，text 只展示润色文本模型，all 展示完整配置。
     * 返回：无返回值。
     * 边界：完整设置入口用于未配置状态，仍保留同时配置两类模型的能力。
     */
    function handleOpenModelSetting(mode: VoicePolishModelSettingMode): void {
        settingMenuOpen.value = false;
        modelSettingMode.value = mode;
        modelSettingDialogOpen.value = true;
    }

    /**
     * 打开语音润色模型引导入口。
     * 流程：先判断本地模型仓库缺少 ASR 还是文本模型；缺模型时打开对应新增弹窗，已有模型但未选择时打开选择弹窗。
     * 参数：无。
     * 返回：无返回值。
     * 边界：ASR 和文本模型都存在但当前选择失效时，不新增模型，只让用户选择已有模型。
     */
    function handleOpenModelGuide(): void {
        if (!asrModels.value.length) {
            openGuidedModelDialog('asr');
            return;
        }
        if (!textModels.value.length) {
            openGuidedModelDialog('text');
            return;
        }
        handleOpenModelSetting('all');
    }

    /**
     * 处理空态主按钮点击。
     * 流程：优先处理麦克风权限；浏览器预览只提示必须使用 App，客户端则跳到权限管理；权限满足后再进入模型配置。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不会在浏览器预览中尝试申请系统权限，避免给用户造成可以网页授权的误解。
     */
    function handlePrimarySetupAction(): void {
        if (!isMicrophoneReady.value) {
            if (!isClientRuntime) {
                showClientRuntimeToast();
                return;
            }
            handleGoPermissionPage();
            return;
        }
        handleOpenModelGuide();
    }

    /**
     * 提示用户切换到客户端完成本机权限操作。
     * 流程：使用 Sonner 展示轻量反馈，避免为无需确认的信息弹出阻断式 Dialog。
     * 参数：无。
     * 返回：无返回值。
     * 边界：仅用于网页预览环境，不会尝试申请或读取系统权限。
     */
    function showClientRuntimeToast(): void {
        toast.warning('请用 APP 打开', {
            description:
                '浏览器预览无法读取本机麦克风权限。请在 typesass App 里打开权限管理，完成麦克风授权后再使用语音转文字润色。'
        });
    }

    /**
     * 手动开始语音处理。
     * 流程：按模式校验 ASR 或润色模型是否具备；缺配置时打开对应引导，配置完整时调用真实录音、ASR、润色和粘贴链路。
     * 参数：mode 为本次语音处理模式，asr 只转文字，polish 会继续调用文本模型润色。
     * 返回：无返回值。
     * 边界：非客户端环境由 Store 展示客户端不可用提示，不会写入假历史。
     */
    function handleStartVoice(mode: VoicePolishRunModeType): void {
        if (!selectedAsrModel.value) {
            handleOpenModelGuide();
            return;
        }
        if (mode === 'polish' && !isDictationPolishConfigured.value) {
            if (!textModels.value.length) {
                openGuidedModelDialog('text');
                return;
            }
            handleOpenModelSetting('text');
            return;
        }
        void store.runVoicePolish('', mode);
    }

    /**
     * 打开指定类型的模型新增弹窗。
     * 流程：记录当前引导新增的模型类型，再打开模型管理页复用的添加弹窗。
     * 参数：group 为需要新增的模型分组，asr 表示语音识别模型，text 表示文本润色模型。
     * 返回：无返回值。
     * 边界：弹窗内部仍负责字段校验和真实连通性测试。
     */
    function openGuidedModelDialog(group: ModelGroupType): void {
        modelGuideGroup.value = group;
        modelGuideDialogOpen.value = true;
    }

    /**
     * 处理引导流程新增模型成功。
     * 流程：把新增模型写入模型仓库并立即选中；ASR 新增后如果还没有文本模型，则弹出文本 AI 润色引导。
     * 参数：form 为模型添加弹窗提交的模型表单。
     * 返回：无返回值。
     * 边界：如果新增文本模型，则直接完成语音润色所需的第二类模型配置。
     */
    async function handleAddGuidedModel(form: ModelFormModel): Promise<void> {
        const model = await modelStore.addModel(form);
        if (model.group === 'asr') {
            store.updateModelSelection(model.id, store.selectedTextModelId);
            if (!textModels.value.length) {
                textPolishPromptOpen.value = true;
            }
            return;
        }
        applyTextModel(model);
    }

    /**
     * 暂后添加文本模型。
     * 流程：关闭文本 AI 润色说明弹窗，页面空状态会继续提示用户补充文本模型。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不会清除已经选中的 ASR 模型。
     */
    function handleDeferTextModel(): void {
        textPolishPromptOpen.value = false;
    }

    /**
     * 立即进入文本模型添加流程。
     * 流程：关闭说明弹窗后打开文本模型新增弹窗，用户添加成功后即可满足语音润色完整配置。
     * 参数：无。
     * 返回：无返回值。
     * 边界：如果用户关闭新增弹窗，页面仍保持“添加大模型（文本模型）”引导。
     */
    function handleAddTextModelNow(): void {
        textPolishPromptOpen.value = false;
        openGuidedModelDialog('text');
    }

    /**
     * 选中新添加的文本模型。
     * 流程：保留当前 ASR 选择，只更新语音润色使用的文本大模型。
     * 参数：model 为刚写入模型仓库的文本模型配置。
     * 返回：无返回值。
     * 边界：调用方保证传入模型属于 text 分组。
     */
    function applyTextModel(model: ModelItemModel): void {
        store.updateModelSelection(store.selectedAsrModelId, model.id);
    }

    /**
     * 语音润色模型设置弹窗模式。
     * 业务含义：约束不同入口只展示对应模型设置区域。
     */
    type VoicePolishModelSettingMode = 'asr' | 'text' | 'all';
</script>
