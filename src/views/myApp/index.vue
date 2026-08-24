<template>
    <section class="grid gap-5">
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="grid gap-1.5">
                <h1 class="text-[18px] font-semibold leading-7 text-foreground">我的应用</h1>
                <p class="text-[13px] leading-5 text-muted-foreground">管理本地托管站点和远程 URL 应用。</p>
            </div>
            <div class="flex gap-2">
                <ui-button
                    variant="outline"
                    type="button"
                    :disabled="store.loading"
                    @click="handleRefresh">
                    <refresh
                        theme="outline"
                        size="15" />
                    <span>{{ store.loading ? '刷新中' : '刷新' }}</span>
                </ui-button>
                <ui-button
                    type="button"
                    @click="handleOpenCreateDialog">
                    <plus
                        theme="outline"
                        size="15" />
                    <span>创建应用</span>
                </ui-button>
            </div>
        </div>

        <p
            v-if="store.message"
            class="text-[13px] leading-5 text-muted-foreground"
            role="status">
            {{ store.message }}
        </p>

        <div
            v-if="store.loading && !store.apps.length"
            class="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
            <ui-card
                v-for="index in 6"
                :key="index">
                <ui-card-content class="grid gap-4 p-4">
                    <ui-skeleton class="h-12 w-12 rounded-lg" />
                    <ui-skeleton class="h-4 w-2/3" />
                    <ui-skeleton class="h-3 w-full" />
                    <ui-skeleton class="h-3 w-4/5" />
                </ui-card-content>
            </ui-card>
        </div>

        <ui-page-state
            v-else-if="!store.apps.length"
            :icon="ApplicationMenu"
            title="还没有应用"
            description="创建本地静态站点或远程 URL 后，会显示在这里。">
            <template #action>
                <ui-button
                    type="button"
                    @click="handleOpenCreateDialog">
                    <plus
                        theme="outline"
                        size="16" />
                    <span>创建应用</span>
                </ui-button>
            </template>
        </ui-page-state>

        <div
            v-else
            class="grid grid-cols-[repeat(auto-fill,minmax(270px,1fr))] gap-3">
            <ui-card
                v-for="app in store.apps"
                :key="app.id"
                class="group min-w-0 cursor-pointer transition-colors hover:border-primary/45"
                @click="handleOpenApp(app, 'codexman')"
                @contextmenu.prevent="handleOpenEditDialog(app)">
                <ui-card-content class="grid h-full min-h-[168px] gap-4 p-4">
                    <div class="flex min-w-0 items-start justify-between gap-3">
                        <div class="flex min-w-0 items-center gap-3">
                            <div
                                class="grid h-12 w-12 shrink-0 place-items-center overflow-hidden rounded-lg border border-border bg-muted">
                                <img
                                    v-if="app.logoDataUrl"
                                    class="h-full w-full object-cover"
                                    :src="app.logoDataUrl"
                                    alt="" />
                                <application-menu
                                    v-else
                                    theme="outline"
                                    size="22"
                                    class="text-muted-foreground" />
                            </div>
                            <div class="min-w-0">
                                <h2 class="truncate text-[15px] font-medium leading-6 text-foreground">
                                    {{ app.name }}
                                </h2>
                                <div class="mt-1 flex flex-wrap gap-1.5">
                                    <ui-badge variant="secondary">{{
                                        app.accessType === 'local' ? '本地托管' : '远程 URL'
                                    }}</ui-badge>
                                    <ui-badge :variant="serviceStatusBadgeVariant(app)">
                                        {{ serviceStatusText(app) }}
                                    </ui-badge>
                                </div>
                            </div>
                        </div>
                        <ui-dropdown-menu>
                            <ui-dropdown-menu-trigger as-child>
                                <ui-button
                                    class="shrink-0"
                                    variant="ghost"
                                    size="icon"
                                    type="button"
                                    title="打开应用"
                                    @click.stop>
                                    <play
                                        theme="outline"
                                        size="16" />
                                </ui-button>
                            </ui-dropdown-menu-trigger>
                            <ui-dropdown-menu-content align="end">
                                <ui-dropdown-menu-item @select="handleOpenApp(app, 'codexman')">
                                    <application-menu
                                        theme="outline"
                                        size="15" />
                                    <span>使用 CodexMan 打开</span>
                                </ui-dropdown-menu-item>
                                <ui-dropdown-menu-item @select="handleOpenApp(app, 'browser')">
                                    <browser
                                        theme="outline"
                                        size="15" />
                                    <span>使用默认浏览器打开</span>
                                </ui-dropdown-menu-item>
                            </ui-dropdown-menu-content>
                        </ui-dropdown-menu>
                    </div>

                    <div class="grid min-w-0 gap-2 text-[12px] leading-5">
                        <div class="truncate text-muted-foreground">{{ app.serviceMessage }}</div>
                    </div>

                    <div class="mt-auto flex flex-wrap items-center justify-between gap-2">
                        <ui-dropdown-menu>
                            <ui-dropdown-menu-trigger as-child>
                                <ui-button
                                    variant="ghost"
                                    size="icon"
                                    type="button"
                                    title="访问地址"
                                    @click.stop>
                                    <Link
                                        theme="outline"
                                        size="16" />
                                </ui-button>
                            </ui-dropdown-menu-trigger>
                            <ui-dropdown-menu-content
                                class="w-[min(330px,calc(100vw-32px))] p-2"
                                align="start">
                                <div class="grid gap-1">
                                    <div
                                        v-for="option in getLinkOptions(app)"
                                        :key="option.label"
                                        class="grid min-w-0 grid-cols-[1fr_auto] items-center gap-2 rounded-md px-2 py-1.5 hover:bg-accent/70">
                                        <div class="grid min-w-0 gap-0.5">
                                            <span class="text-[12px] text-muted-foreground">{{ option.label }}</span>
                                            <span class="truncate font-mono text-[12px] text-foreground">{{
                                                option.value || '-'
                                            }}</span>
                                        </div>
                                        <ui-button
                                            variant="ghost"
                                            size="icon"
                                            type="button"
                                            :disabled="!option.value"
                                            :title="`复制${option.label}`"
                                            @click.stop="handleCopyAddress(option.label, option.value)">
                                            <copy
                                                theme="outline"
                                                size="14" />
                                        </ui-button>
                                    </div>
                                </div>
                            </ui-dropdown-menu-content>
                        </ui-dropdown-menu>
                        <div class="flex flex-wrap justify-end gap-1">
                            <ui-button
                                v-if="app.accessType === 'local'"
                                variant="ghost"
                                size="icon"
                                type="button"
                                :disabled="isOperating(app.id)"
                                title="启动或重启服务"
                                @click.stop="handleRestartApp(app)">
                                <refresh
                                    theme="outline"
                                    size="16" />
                            </ui-button>
                            <ui-button
                                variant="ghost"
                                size="icon"
                                type="button"
                                title="编辑应用"
                                @click.stop="handleOpenEditDialog(app)">
                                <edit
                                    theme="outline"
                                    size="16" />
                            </ui-button>
                            <ui-button
                                variant="ghost"
                                size="icon"
                                type="button"
                                title="删除应用"
                                @click.stop="pendingRemoval = app">
                                <delete
                                    theme="outline"
                                    size="16" />
                            </ui-button>
                        </div>
                    </div>
                </ui-card-content>
            </ui-card>
        </div>

        <my-app-form-dialog
            v-model:open="formDialogOpen"
            :app="pendingEditing"
            :saving="store.saving"
            :allocating-port="store.allocatingPort"
            :allocate-port="store.allocatePort"
            @submit="handleSaveApp" />

        <ui-dialog v-model:open="deleteDialogOpen">
            <ui-dialog-content>
                <ui-dialog-header>
                    <ui-dialog-title>删除应用</ui-dialog-title>
                    <ui-dialog-description
                        >删除后，本地托管应用会停止服务并删除已解压的静态资源。</ui-dialog-description
                    >
                </ui-dialog-header>
                <div class="rounded-md border border-border bg-muted/35 p-3 text-[13px] text-muted-foreground">
                    {{ pendingRemoval?.name || '-' }}
                </div>
                <ui-dialog-footer class="mt-5">
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="pendingRemoval ? isOperating(pendingRemoval.id) : false"
                        @click="pendingRemoval = null">
                        取消
                    </ui-button>
                    <ui-button
                        variant="destructive"
                        type="button"
                        :disabled="pendingRemoval ? isOperating(pendingRemoval.id) : false"
                        @click="handleRemoveApp">
                        删除
                    </ui-button>
                </ui-dialog-footer>
            </ui-dialog-content>
        </ui-dialog>
    </section>
</template>

<script setup lang="ts">
    import { ApplicationMenu, Browser, Copy, Delete, Edit, Link, Play, Plus, Refresh } from '@icon-park/vue-next';
    import { toast } from 'vue-sonner';

    import MyAppFormDialog from '@/components/myApp/appFormDialog.vue';
    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Button as UiButton } from '@/components/ui/button';
    import { Card as UiCard, CardContent as UiCardContent } from '@/components/ui/card';
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
    import { Skeleton as UiSkeleton } from '@/components/ui/skeleton';
    import type { MyAppFormModel, MyAppModel, MyAppOpenTargetType } from '@/model/myApp';
    import { useMyAppStore } from '@/stores/myApp';

    defineOptions({
        name: 'MyAppView'
    });

    const store = useMyAppStore();
    const formDialogOpen = ref(false);
    const pendingEditing = ref<MyAppModel | null>(null);
    const pendingRemoval = ref<MyAppModel | null>(null);
    const deleteDialogOpen = computed({
        get: () => Boolean(pendingRemoval.value),
        set: (value: boolean) => {
            if (!value) pendingRemoval.value = null;
        }
    });

    /** 我的应用可复制访问地址选项。 */
    interface MyAppLinkOption {
        /** 地址展示标签。 */
        label: string;
        /** 可复制地址；为空时禁用复制按钮。 */
        value: string;
    }

    /**
     * 判断应用是否正在执行操作。
     * 流程：读取 store 操作 ID 列表。
     * 参数：appId 为应用 ID。
     * 返回：正在操作时 true。
     * 边界：仅用于禁用按钮和展示 loading，不推断服务状态。
     */
    function isOperating(appId: string): boolean {
        return store.operatingIds.includes(appId);
    }

    /**
     * 服务状态中文。
     * 流程：把 HTTP 返回的稳定状态映射为卡片文案。
     * 参数：app 为应用列表项。
     * 返回：状态中文。
     * 边界：远程 URL 应用没有本地服务。
     */
    function serviceStatusText(app: MyAppModel): string {
        if (app.serviceStatus === 'starting') return '启动中';
        if (app.serviceStatus === 'running') return '已启动';
        if (app.serviceStatus === 'paused') return '已暂停';
        if (app.serviceStatus === 'failed') return '启动失败';
        return '远程访问';
    }

    /**
     * 服务状态 Badge 样式。
     * 流程：按状态映射组件库 variant。
     * 参数：app 为应用列表项。
     * 返回：Badge variant。
     * 边界：失败状态使用 destructive，其它状态保持克制。
     */
    function serviceStatusBadgeVariant(app: MyAppModel): 'default' | 'secondary' | 'outline' | 'destructive' {
        if (app.serviceStatus === 'running') return 'default';
        if (app.serviceStatus === 'failed') return 'destructive';
        if (app.serviceStatus === 'unavailable') return 'outline';
        return 'secondary';
    }

    /**
     * 读取应用可复制地址选项。
     * 流程：本地托管返回本机和局域网地址；远程应用返回远程 URL。
     * 参数：app 为当前应用卡片。
     * 返回：下拉菜单内展示的地址列表。
     * 边界：地址为空时仍展示占位，复制按钮禁用，避免用户误以为入口丢失。
     */
    function getLinkOptions(app: MyAppModel): MyAppLinkOption[] {
        if (app.accessType === 'local') {
            return [
                { label: '公网访问地址', value: app.publicUrl },
                { label: '本地访问地址', value: app.localUrl },
                { label: '局域网访问地址', value: app.lanUrl }
            ];
        }
        return [{ label: '远程 URL', value: app.remoteUrl || '' }];
    }

    /**
     * 复制应用访问地址。
     * 流程：校验地址非空后写入系统剪贴板，并按地址类型展示提示。
     * 参数：label 为地址类型，address 为待复制地址。
     * 返回：无。
     * 异常：剪贴板权限不可用时提示失败，不影响卡片其它操作。
     */
    function handleCopyAddress(label: string, address: string): void {
        if (!address) return;
        void copyTextToClipboard(address).then((copied) => {
            if (copied) {
                toast.success(`${label}已复制。`);
                return;
            }
            toast.error(`${label}复制失败。`, {
                description: '当前环境不允许写入剪贴板。'
            });
        });
    }

    /**
     * 写入文本到系统剪贴板。
     * 流程：优先使用标准 Clipboard API；失败或不可用时创建临时 textarea 兜底复制。
     * 参数：text 为需要复制的文本。
     * 返回：复制成功时 true。
     * 异常/边界：无 DOM 或浏览器禁止写入时返回 false，不向上抛异常。
     */
    async function copyTextToClipboard(text: string): Promise<boolean> {
        try {
            if (navigator.clipboard?.writeText) {
                await navigator.clipboard.writeText(text);
                return true;
            }
        } catch {
            // 继续使用 textarea 兜底。
        }
        if (typeof document === 'undefined') return false;
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.setAttribute('readonly', 'true');
        textarea.className = 'fixed -left-[9999px] top-0 opacity-0';
        document.body.appendChild(textarea);
        textarea.select();
        try {
            return document.execCommand('copy');
        } catch {
            return false;
        } finally {
            document.body.removeChild(textarea);
        }
    }

    /**
     * 打开新增弹窗。
     * 流程：清空编辑对象并显示表单。
     * 参数：无。
     * 返回：无。
     * 边界：表单组件会在打开时重置内部 zip 选择。
     */
    function handleOpenCreateDialog(): void {
        pendingEditing.value = null;
        formDialogOpen.value = true;
    }

    /**
     * 打开编辑弹窗。
     * 流程：保存当前应用并显示表单。
     * 参数：app 为待编辑应用。
     * 返回：无。
     * 边界：右键卡片也进入同一编辑入口。
     */
    function handleOpenEditDialog(app: MyAppModel): void {
        pendingEditing.value = app;
        formDialogOpen.value = true;
    }

    /**
     * 保存表单。
     * 流程：把表单转换为 HTTP 请求模型，成功后关闭弹窗并提示。
     * 参数：form 为已校验表单。
     * 返回：无。
     * 异常：保存失败时弹窗保持打开。
     */
    async function handleSaveApp(form: MyAppFormModel): Promise<void> {
        try {
            const port = form.accessType === 'local' ? Number(form.port) : undefined;
            const request = {
                name: form.name.trim(),
                logoDataUrl: form.logoDataUrl,
                accessType: form.accessType,
                port,
                remoteUrl: form.accessType === 'remote' ? form.remoteUrl.trim() : undefined,
                publicSubdomain:
                    form.accessType === 'local' && form.publicSubdomain.trim()
                        ? form.publicSubdomain.trim().toLowerCase()
                        : undefined,
                zipDataUrl: form.zipDataUrl || undefined
            };
            await store.saveApp(form.id ? { id: form.id, ...request } : request);
            toast.success(form.id ? '应用已更新。' : '应用已创建。');
            formDialogOpen.value = false;
            pendingEditing.value = null;
        } catch (error) {
            toast.error(error instanceof Error ? error.message : '保存应用失败。');
        }
    }

    /**
     * 删除应用。
     * 流程：调用 HTTP 删除接口，成功后关闭确认框。
     * 参数：无。
     * 返回：无。
     * 异常：删除失败时保持确认框，方便用户重试或取消。
     */
    async function handleRemoveApp(): Promise<void> {
        const app = pendingRemoval.value;
        if (!app) return;
        try {
            await store.removeApp(app.id);
            toast.success('应用已删除。');
            pendingRemoval.value = null;
        } catch (error) {
            toast.error(error instanceof Error ? error.message : '删除应用失败。');
        }
    }

    /**
     * 启动或重启应用。
     * 流程：调用 HTTP 重启接口并展示结果提示。
     * 参数：app 为本地托管应用。
     * 返回：无。
     * 异常：失败时由 store 刷新列表并展示错误。
     */
    async function handleRestartApp(app: MyAppModel): Promise<void> {
        try {
            await store.restartApp(app.id);
            toast.success('服务已启动。');
        } catch (error) {
            toast.error(error instanceof Error ? error.message : '启动服务失败。');
        }
    }

    /**
     * 打开应用。
     * 流程：按用户选择的目标调用 HTTP 打开接口。
     * 参数：app 为应用，target 为打开目标。
     * 返回：无。
     * 异常：本地服务启动或窗口打开失败时提示。
     */
    async function handleOpenApp(app: MyAppModel, target: MyAppOpenTargetType): Promise<void> {
        if (isOperating(app.id)) return;
        try {
            await store.openApp(app.id, target);
            toast.success(target === 'codexman' ? '已使用 CodexMan 打开。' : '已使用默认浏览器打开。');
        } catch (error) {
            toast.error(error instanceof Error ? error.message : '打开应用失败。');
        }
    }

    /**
     * 刷新列表。
     * 流程：重新读取 HTTP 列表。
     * 参数：无。
     * 返回：无。
     * 异常：失败时提示错误。
     */
    async function handleRefresh(): Promise<void> {
        try {
            await store.loadApps();
            toast.success('应用列表已刷新。');
        } catch (error) {
            toast.error(error instanceof Error ? error.message : '刷新应用列表失败。');
        }
    }

    onMounted(() => {
        void store.loadApps().catch((error: unknown) => {
            toast.error(error instanceof Error ? error.message : '读取我的应用失败。');
        });
    });
</script>
