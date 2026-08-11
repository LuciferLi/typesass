<template>
    <div class="grid gap-3">
        <ui-alert v-if="store.initializing">
            <ui-skeleton class="h-5 w-[34%]" />
            <ui-skeleton class="mt-3 h-4 w-[62%]" />
        </ui-alert>
        <template v-else>
            <ui-field
                orientation="horizontal"
                class="rounded-md border border-border bg-muted/30 p-4">
                <ui-field-content>
                    <ui-field-label>界面主题</ui-field-label>
                    <ui-field-description>在浅色和深色主题之间切换。</ui-field-description>
                </ui-field-content>
                <ui-switch
                    :model-value="store.settings.themeMode === 'dark'"
                    @update:model-value="store.toggleThemeMode" />
            </ui-field>
            <ui-field
                orientation="horizontal"
                class="rounded-md border border-border bg-muted/30 p-4">
                <ui-field-content>
                    <ui-field-label>开机自动启动</ui-field-label>
                    <ui-field-description>登录 macOS 后自动启动 CodexMan。</ui-field-description>
                </ui-field-content>
                <ui-switch
                    :model-value="store.settings.launchAtLogin"
                    :disabled="store.saving"
                    @update:model-value="handleToggleLaunchAtLogin" />
            </ui-field>
            <p
                v-if="store.message"
                class="text-[13px] leading-5 text-muted-foreground"
                role="status">
                {{ store.message }}
            </p>
        </template>
    </div>
</template>

<script setup lang="ts">
    import { toast } from 'vue-sonner';

    import { Alert as UiAlert } from '@/components/ui/alert';
    import {
        Field as UiField,
        FieldContent as UiFieldContent,
        FieldDescription as UiFieldDescription,
        FieldLabel as UiFieldLabel
    } from '@/components/ui/field';
    import { Skeleton as UiSkeleton } from '@/components/ui/skeleton';
    import { Switch as UiSwitch } from '@/components/ui/switch';
    import { useSettingsStore } from '@/stores/settings';

    defineOptions({
        name: 'SettingsView'
    });

    const store = useSettingsStore();

    /**
     * 弹出设置操作失败提示。
     * 流程：优先展示 Error 中的安全错误说明；未知异常使用兜底文案。
     * 参数：title 为短提示标题，error 为捕获异常，fallbackDescription 为兜底说明。
     * 返回：无返回值。
     * 边界：初始化读取失败仍保留页面级 message，不转为短提示。
     */
    function showSettingsOperationError(title: string, error: unknown, fallbackDescription: string): void {
        toast.error(title, {
            description: error instanceof Error ? error.message : fallbackDescription
        });
    }

    /**
     * 切换开机自动启动设置。
     * 流程：委托 Store 调用系统设置，完成后用 Sonner 给出短反馈。
     * 参数：enabled 为目标开关状态。
     * 返回：无返回值。
     * 边界：失败时 Store 保留原开关状态，页面只弹出失败说明。
     */
    function handleToggleLaunchAtLogin(enabled: boolean): void {
        void store
            .toggleLaunchAtLogin(enabled)
            .then(() => {
                toast.success(enabled ? '已开启开机自动启动' : '已关闭开机自动启动');
            })
            .catch((error: unknown) => {
                showSettingsOperationError('保存系统设置失败', error, '开机自动启动设置保存失败。');
            });
    }

    onMounted(() => {
        void store.initSettings();
    });
</script>
