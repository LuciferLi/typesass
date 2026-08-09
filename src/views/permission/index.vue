<template>
    <section class="grid gap-6">
        <div class="flex flex-wrap items-center justify-between gap-4">
            <div class="grid gap-1">
                <p class="max-w-[640px] text-[13px] leading-6 text-muted-foreground">
                    检查 typesass 在这台电脑上录音、读取选中文本、自动粘贴和响应全局快捷键所需的系统授权。
                </p>
            </div>
            <ui-button
                variant="outline"
                size="icon"
                type="button"
                title="系统说明"
                @click="permissionInfoDialogOpen = true">
                <info
                    theme="outline"
                    size="17" />
                <span class="sr-only">系统说明</span>
            </ui-button>
        </div>

        <div
            v-if="store.loading"
            class="grid gap-6">
            <section
                v-for="group in permissionGroups"
                :key="group.title"
                class="grid gap-2">
                <div class="px-1 text-[13px] font-medium text-muted-foreground">{{ group.title }}</div>
                <div class="overflow-hidden rounded-lg border border-border bg-card">
                    <ui-table>
                        <ui-table-body>
                            <ui-table-row
                                v-for="item in group.keys"
                                :key="item">
                                <ui-table-cell>
                                    <div class="flex items-center gap-3">
                                        <ui-skeleton class="h-9 w-9 rounded-md" />
                                        <div class="grid flex-1 gap-2">
                                            <ui-skeleton class="h-4 w-[160px]" />
                                            <ui-skeleton class="h-3 w-[260px]" />
                                        </div>
                                    </div>
                                </ui-table-cell>
                                <ui-table-cell class="w-[140px]">
                                    <ui-skeleton class="h-5 w-[72px]" />
                                </ui-table-cell>
                                <ui-table-cell class="w-[160px] text-right">
                                    <ui-skeleton class="ml-auto h-8 w-[96px]" />
                                </ui-table-cell>
                            </ui-table-row>
                        </ui-table-body>
                    </ui-table>
                </div>
            </section>
        </div>

        <div
            v-else
            class="grid gap-6">
            <section
                v-for="group in visiblePermissionGroups"
                :key="group.title"
                class="grid gap-2">
                <div class="px-1 text-[13px] font-medium text-muted-foreground">{{ group.title }}</div>
                <div class="overflow-hidden rounded-lg border border-border bg-card">
                    <ui-table>
                        <ui-table-body>
                            <ui-table-row
                                v-for="item in group.items"
                                :key="item.key">
                                <ui-table-cell>
                                    <div class="flex items-center gap-3">
                                        <div
                                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-muted text-muted-foreground">
                                            <component
                                                :is="permissionIconByKey[item.key]"
                                                theme="outline"
                                                size="18" />
                                        </div>
                                        <div class="min-w-0">
                                            <div class="text-[14px] font-semibold text-foreground">{{ item.name }}</div>
                                            <div class="mt-1 truncate text-[12px] text-muted-foreground">
                                                {{ item.description }}
                                            </div>
                                        </div>
                                    </div>
                                </ui-table-cell>
                                <ui-table-cell class="w-[220px]">
                                    <ui-badge
                                        :class="
                                            item.ready ? 'w-fit border-primary/70 bg-primary/10 text-primary' : 'w-fit'
                                        "
                                        variant="outline">
                                        {{ item.ready ? '已授权' : '未授权' }}
                                    </ui-badge>
                                </ui-table-cell>
                                <ui-table-cell class="w-[160px] text-right">
                                    <ui-button
                                        v-if="
                                            isClientRuntime &&
                                            (item.key === 'microphone' || item.key === 'accessibility')
                                        "
                                        variant="outline"
                                        size="sm"
                                        type="button"
                                        @click="store.openPermission(item.key)">
                                        打开系统设置
                                    </ui-button>
                                    <ui-tooltip
                                        v-else-if="item.key === 'microphone' || item.key === 'accessibility'"
                                        :open="disabledPermissionTipKey === item.key">
                                        <ui-tooltip-trigger as-child>
                                            <span
                                                class="inline-flex cursor-not-allowed"
                                                @mouseenter="disabledPermissionTipKey = item.key"
                                                @mouseleave="disabledPermissionTipKey = null"
                                                @pointerdown.prevent.stop="disabledPermissionTipKey = item.key"
                                                @click.prevent.stop="disabledPermissionTipKey = item.key">
                                                <ui-button
                                                    variant="outline"
                                                    size="sm"
                                                    type="button"
                                                    disabled>
                                                    打开系统设置
                                                </ui-button>
                                            </span>
                                        </ui-tooltip-trigger>
                                        <ui-tooltip-content>请打开客户端设置</ui-tooltip-content>
                                    </ui-tooltip>
                                    <span
                                        v-else
                                        class="text-[12px] text-muted-foreground"
                                        >-</span
                                    >
                                </ui-table-cell>
                            </ui-table-row>
                        </ui-table-body>
                    </ui-table>
                </div>
            </section>

            <section class="grid gap-2">
                <div class="px-1 text-[13px] font-medium text-muted-foreground">声音与画面</div>
                <div class="overflow-hidden rounded-lg border border-border bg-card">
                    <ui-table>
                        <ui-table-body>
                            <ui-table-row>
                                <ui-table-cell>
                                    <div class="flex items-center gap-3">
                                        <div
                                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-muted text-muted-foreground">
                                            <microphone
                                                theme="outline"
                                                size="18" />
                                        </div>
                                        <div class="min-w-0">
                                            <div class="text-[14px] font-semibold text-foreground">智能识音增强</div>
                                            <div class="mt-1 truncate text-[12px] text-muted-foreground">
                                                应用于语音转文字和语音转文字润色，优先提升人声清晰度。
                                            </div>
                                        </div>
                                    </div>
                                </ui-table-cell>
                                <ui-table-cell class="w-[160px] text-right">
                                    <ui-switch
                                        :model-value="settingsStore.settings.smartVoiceEnhancement"
                                        @update:model-value="settingsStore.toggleSmartVoiceEnhancement" />
                                </ui-table-cell>
                            </ui-table-row>
                        </ui-table-body>
                    </ui-table>
                </div>
            </section>
        </div>

        <ui-dialog v-model:open="permissionInfoDialogOpen">
            <ui-dialog-content class="max-w-[560px] overflow-hidden">
                <ui-dialog-header class="gap-1.5">
                    <ui-dialog-title>系统权限说明</ui-dialog-title>
                    <p class="text-[13px] leading-5 text-muted-foreground">
                        typesass 只在本机使用这些系统能力，用来完成语音输入、读取选区和快捷操作。
                    </p>
                    <ui-dialog-description class="sr-only"
                        >说明 typesass 需要本机系统权限的原因。</ui-dialog-description
                    >
                </ui-dialog-header>
                <div class="grid gap-3.5 text-[14px] leading-6 text-muted-foreground">
                    <section
                        v-for="item in permissionInfoSections"
                        :key="item.title"
                        class="grid gap-1">
                        <h2 class="flex items-center gap-1.5 text-[14px] font-semibold text-foreground">
                            <span
                                class="flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                                <component
                                    :is="item.icon"
                                    theme="outline"
                                    size="12" />
                            </span>
                            <span>{{ item.title }}</span>
                        </h2>
                        <p>{{ item.description }}</p>
                    </section>
                </div>
                <ui-dialog-footer>
                    <ui-button
                        type="button"
                        @click="permissionInfoDialogOpen = false"
                        >知道了</ui-button
                    >
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
    </section>
</template>

<script setup lang="ts">
    import { Info, KeyboardOne, Microphone, Permissions, Shield, TextRecognition } from '@icon-park/vue-next';
    import type { Component } from 'vue';

    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import { Skeleton as UiSkeleton } from '@/components/ui/skeleton';
    import { Switch as UiSwitch } from '@/components/ui/switch';
    import {
        Table as UiTable,
        TableBody as UiTableBody,
        TableCell as UiTableCell,
        TableRow as UiTableRow
    } from '@/components/ui/table';
    import {
        Tooltip as UiTooltip,
        TooltipContent as UiTooltipContent,
        TooltipTrigger as UiTooltipTrigger
    } from '@/components/ui/tooltip';
    import type { PermissionItemModel, PermissionKeyType } from '@/model/permission';
    import { isTauriRuntime } from '@/service/tauri/command';
    import { usePermissionStore } from '@/stores/permission';
    import { useSettingsStore } from '@/stores/settings';

    defineOptions({
        name: 'PermissionView'
    });

    /**
     * 权限分组模型。
     * 业务含义：将本机权限按系统授权和运行能力分组展示，保证页面是一组列表而不是独立卡片。
     */
    type PermissionGroupModel = {
        // 分组标题，展示在列表容器外部。
        title: string;
        // 该分组包含的权限稳定键。
        keys: PermissionDisplayKeyType[];
    };

    /**
     * 权限页展示键类型。
     * 业务含义：权限页只展示电脑授权相关状态，API Key 属于账号/模型配置能力，不在本页展示。
     */
    type PermissionDisplayKeyType = Exclude<PermissionKeyType, 'apiKey'>;

    /**
     * 权限页展示项模型。
     * 业务含义：从完整权限诊断列表中过滤出本机权限相关项，供当前页面按行展示。
     */
    type PermissionDisplayItemModel = PermissionItemModel & {
        // 当前页面允许展示的本机权限键。
        key: PermissionDisplayKeyType;
    };

    /**
     * 带权限项的分组模型。
     * 业务含义：根据权限诊断结果把分组键映射为真实权限行，用于模板渲染。
     */
    type VisiblePermissionGroupModel = {
        // 分组标题，来自权限分组配置。
        title: string;
        // 当前分组内可展示的权限行。
        items: PermissionDisplayItemModel[];
    };

    /**
     * 权限说明弹窗信息模型。
     * 业务含义：为系统权限说明弹窗提供标题、说明和图标组件，避免说明内容只有大段文字。
     */
    type PermissionInfoSectionModel = {
        // 说明项标题，对应用户需要理解的权限或能力。
        title: string;
        // 说明项正文，解释该权限缺失时的业务影响。
        description: string;
        // 说明项图标组件，用于辅助识别权限类型。
        icon: Component;
    };

    const store = usePermissionStore();
    const settingsStore = useSettingsStore();
    const permissionInfoDialogOpen = ref(false);
    const disabledPermissionTipKey = ref<PermissionDisplayKeyType | null>(null);
    const isClientRuntime = isTauriRuntime();
    const permissionGroups: PermissionGroupModel[] = [
        { title: '系统权限', keys: ['microphone', 'accessibility'] },
        { title: '运行能力', keys: ['shortcut'] }
    ];
    const permissionIconByKey: Record<PermissionDisplayKeyType, Component> = {
        microphone: Microphone,
        accessibility: Permissions,
        shortcut: KeyboardOne
    };
    const permissionInfoSections: PermissionInfoSectionModel[] = [
        {
            title: '为什么需要系统权限',
            description:
                'typesass 是桌面端输入辅助工具，需要在本机完成录音、读取当前选中文本、把结果写回目标应用，以及在后台响应快捷键。',
            icon: Shield
        },
        {
            title: '麦克风权限',
            description: '用于语音转文字和语音转文字润色。没有麦克风权限时，应用无法采集你的语音输入。',
            icon: Microphone
        },
        {
            title: '辅助功能权限',
            description:
                '用于读取选中文本、恢复输入焦点和自动粘贴结果。没有辅助功能权限时，文字润色和自动写回能力会受限。',
            icon: TextRecognition
        },
        {
            title: '全局快捷键',
            description:
                '用于在应用处于后台时快速开始语音、字幕或润色动作。快捷键不可用时，只能回到应用窗口内手动操作。',
            icon: KeyboardOne
        }
    ];
    const visiblePermissionGroups = computed<VisiblePermissionGroupModel[]>(() => {
        return permissionGroups
            .map((group) => ({
                title: group.title,
                items: group.keys
                    .map((key) => store.items.find((item) => item.key === key))
                    .filter((item): item is PermissionDisplayItemModel => Boolean(item))
            }))
            .filter((group) => group.items.length > 0);
    });

    onMounted(() => {
        void store.refreshPermissions();
    });
</script>
