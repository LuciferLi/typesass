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
                    <ui-field-description>登录 macOS 后自动启动 typesass。</ui-field-description>
                </ui-field-content>
                <ui-switch
                    :model-value="store.settings.launchAtLogin"
                    :disabled="store.saving"
                    @update:model-value="store.toggleLaunchAtLogin" />
            </ui-field>
            <ui-field
                orientation="horizontal"
                class="rounded-md border border-destructive/35 bg-destructive/5 p-4">
                <ui-field-content>
                    <ui-field-label>恢复表结构</ui-field-label>
                    <ui-field-description
                        >应用最新任务管理表结构，并清空项目、任务、会话和执行记录。</ui-field-description
                    >
                </ui-field-content>
                <ui-button
                    variant="destructive"
                    type="button"
                    :disabled="store.saving"
                    @click="handleResetSessionTaskSchema">
                    {{ store.saving ? '恢复中' : '恢复表结构' }}
                </ui-button>
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
    import { Alert as UiAlert } from '@/components/ui/alert';
    import { Button as UiButton } from '@/components/ui/button';
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
     * 触发任务管理业务表恢复。
     * 流程：调用设置 store 的恢复动作，Rust 端会重建 SQLite 业务表并清空任务数据。
     * 参数：无。
     * 返回：无返回值。
     * 边界：只影响会话和任务管理业务库，不会删除 JSON 设置。
     */
    function handleResetSessionTaskSchema(): void {
        void store.resetSessionTaskSchema();
    }

    onMounted(() => {
        void store.initSettings();
    });
</script>
