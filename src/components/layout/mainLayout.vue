<template>
    <sidebar-provider class="h-full min-h-0 bg-transparent text-foreground">
        <sidebar
            collapsible="none"
            :class="[
                'windowDragRegion hidden w-[212px] bg-transparent px-4 pb-5 md:flex',
                isClientRuntime ? 'pt-12' : 'pt-4'
            ]"
            data-tauri-drag-region="deep">
            <sidebar-header
                class="mb-4 cursor-default select-none p-0"
                data-tauri-drag-region="deep">
                <div
                    class="flex items-center gap-2.5"
                    data-tauri-drag-region="deep">
                    <img
                        class="h-8 w-8 rounded-md bg-background/10 ring-1 ring-sidebar-border"
                        :src="brandLogoUrl"
                        alt=""
                        data-tauri-drag-region />
                    <div
                        class="min-w-0 -translate-y-0.5"
                        data-tauri-drag-region>
                        <div
                            class="truncate text-[16px] font-normal leading-none text-sidebar-foreground"
                            data-tauri-drag-region>
                            CodexMan
                        </div>
                        <div
                            class="mt-1 truncate text-[10px] leading-none text-sidebar-foreground/55"
                            data-tauri-drag-region>
                            Vibe Coding 助手
                        </div>
                    </div>
                </div>
            </sidebar-header>
            <sidebar-content class="gap-1 overflow-hidden">
                <sidebar-menu>
                    <sidebar-menu-item
                        v-for="item in navItems"
                        :key="item.routeName">
                        <sidebar-menu-button
                            :is-active="isNavItemActive(item.routeName)"
                            class="h-8 px-2 text-[13px] font-normal text-sidebar-foreground data-[active=true]:font-normal [&>span:last-child]:leading-5"
                            :tooltip="item.label"
                            @click="handleNavigate(item.routeName)">
                            <component
                                :is="item.icon"
                                theme="outline"
                                size="15" />
                            <span>{{ item.label }}</span>
                        </sidebar-menu-button>
                    </sidebar-menu-item>
                </sidebar-menu>
            </sidebar-content>
            <sidebar-footer class="windowNoDrag gap-2 p-0">
                <sidebar-menu>
                    <sidebar-menu-item>
                        <sidebar-menu-button
                            :is-active="route.name === HubRouteName.Settings"
                            class="h-8 px-2 text-[13px] font-normal text-sidebar-foreground data-[active=true]:font-normal [&>span:last-child]:leading-5"
                            tooltip="系统设置"
                            @click="handleNavigate(HubRouteName.Settings)">
                            <setting
                                theme="outline"
                                size="15" />
                            <span>系统设置</span>
                        </sidebar-menu-button>
                    </sidebar-menu-item>
                </sidebar-menu>
                <div
                    class="flex items-center justify-between rounded-md px-2 py-2 text-sidebar-foreground"
                    data-disable-window-drag>
                    <div class="flex items-center gap-2 text-[12px]">
                        <component
                            :is="settingsStore.settings.themeMode === 'dark' ? Moon : SunOne"
                            theme="outline"
                            size="14" />
                        <span>{{ settingsStore.settings.themeMode === 'dark' ? '深色' : '浅色' }}</span>
                    </div>
                    <ui-switch
                        class="data-[state=checked]:bg-sidebar-accent data-[state=unchecked]:bg-sidebar-border [&>span]:bg-sidebar-foreground"
                        :model-value="settingsStore.settings.themeMode === 'dark'"
                        @update:model-value="settingsStore.toggleThemeMode" />
                </div>
                <button
                    type="button"
                    class="flex min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12px] outline-none ring-sidebar-ring transition-colors hover:bg-sidebar-accent focus-visible:ring-2"
                    :class="codexConnectionTextClass"
                    data-disable-window-drag
                    @click="codexConnectionStore.openDialog">
                    <span
                        class="h-1.5 w-1.5 shrink-0 rounded-full"
                        :class="codexConnectionDotClass"></span>
                    <span class="truncate">{{ codexConnectionText }}</span>
                </button>
                <button
                    type="button"
                    class="flex min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12px] outline-none ring-sidebar-ring transition-colors hover:bg-sidebar-accent focus-visible:ring-2"
                    :class="clientBridgeHealthTextClass"
                    data-disable-window-drag
                    @click="handleOpenHttpApiDoc">
                    <span
                        class="h-1.5 w-1.5 shrink-0 rounded-full"
                        :class="clientBridgeHealthDotClass"></span>
                    <span class="truncate">{{ publicApiHealthText }}</span>
                </button>
            </sidebar-footer>
        </sidebar>
        <sidebar-inset
            :class="[
                'min-h-0 overflow-hidden bg-transparent p-2 md:pb-4 md:pr-4',
                isClientRuntime ? 'md:pt-12' : 'md:pt-4'
            ]">
            <nav
                class="windowNoDrag mb-2 flex h-11 shrink-0 items-center gap-1 overflow-x-auto rounded-lg border border-border/70 bg-card/65 px-2 md:hidden"
                aria-label="移动端主导航">
                <img
                    class="mr-1 h-7 w-7 shrink-0 rounded-md"
                    :src="brandLogoUrl"
                    alt="CodexMan" />
                <button
                    v-for="item in navItems"
                    :key="`mobile-${item.routeName}`"
                    type="button"
                    class="grid h-8 w-8 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-ring"
                    :class="isNavItemActive(item.routeName) ? 'bg-accent text-accent-foreground' : ''"
                    :aria-label="item.label"
                    :title="item.label"
                    @click="handleNavigate(item.routeName)">
                    <component
                        :is="item.icon"
                        theme="outline"
                        size="17" />
                </button>
                <button
                    type="button"
                    class="grid h-8 w-8 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-ring"
                    :class="route.name === HubRouteName.Settings ? 'bg-accent text-accent-foreground' : ''"
                    aria-label="系统设置"
                    title="系统设置"
                    @click="handleNavigate(HubRouteName.Settings)">
                    <setting
                        theme="outline"
                        size="17" />
                </button>
                <button
                    type="button"
                    class="grid h-8 w-8 shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    :class="codexConnectionTextClass"
                    :aria-label="codexConnectionText"
                    :title="codexConnectionText"
                    @click="codexConnectionStore.openDialog">
                    <terminal
                        theme="outline"
                        size="17" />
                </button>
                <button
                    type="button"
                    class="ml-auto grid h-8 w-8 shrink-0 place-items-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    :class="clientBridgeHealthTextClass"
                    aria-label="HTTP API 文档"
                    title="HTTP API 文档"
                    @click="handleOpenHttpApiDoc">
                    <link-break
                        theme="outline"
                        size="17" />
                </button>
            </nav>
            <div
                v-if="isClientRuntime"
                class="windowDragRegion absolute inset-x-0 top-0 z-20 hidden h-12 cursor-default select-none md:block"
                data-tauri-drag-region="deep"></div>
            <div
                v-if="isClientRuntime"
                class="windowDragRegion absolute bottom-0 right-0 top-0 w-4"
                data-tauri-drag-region="deep"></div>
            <div
                v-if="isClientRuntime"
                class="windowDragRegion absolute inset-x-0 bottom-0 h-4"
                data-tauri-drag-region="deep"></div>
            <div
                class="windowNoDrag relative min-h-0 flex-1 rounded-lg border border-border/70 bg-card/65 p-3 backdrop-blur md:p-6"
                :class="route.name === HubRouteName.TaskManage ? 'overflow-hidden' : 'overflow-y-auto'">
                <router-view v-slot="{ Component, route: viewRoute }">
                    <transition
                        mode="out-in"
                        name="pageSwitch">
                        <component
                            :is="Component"
                            :key="viewRoute.name ?? viewRoute.path" />
                    </transition>
                </router-view>
                <transition name="clientBridgeOverlay">
                    <div
                        v-if="showClientBridgeOverlay"
                        class="absolute inset-0 z-30 grid place-items-center overflow-hidden rounded-lg bg-background/30 p-6 backdrop-blur-2xl">
                        <div
                            class="absolute inset-0 bg-[radial-gradient(circle_at_22%_0%,hsl(var(--primary)/0.16),transparent_28rem),radial-gradient(circle_at_82%_8%,hsl(var(--accent)/0.18),transparent_26rem)]"></div>
                        <div
                            class="absolute inset-0 border border-white/10 bg-card/25 shadow-[inset_0_1px_0_hsl(var(--foreground)/0.08)]"></div>
                        <section
                            class="relative grid max-w-[520px] place-items-center gap-3 rounded-lg border border-border/70 bg-card/55 px-8 py-7 text-center shadow-2xl shadow-background/30 backdrop-blur-2xl">
                            <span
                                class="grid h-11 w-11 place-items-center rounded-lg border border-primary/30 bg-primary/10 text-primary">
                                <link-break
                                    theme="outline"
                                    size="22" />
                            </span>
                            <div class="grid gap-1.5">
                                <h2 class="text-[17px] font-semibold leading-7 text-foreground">HTTP 服务未连接</h2>
                                <p class="text-[13px] leading-6 text-muted-foreground">
                                    当前 Web 页面无法连接 CodexMan HTTP 服务。服务恢复后，这层提示会自动消失。
                                </p>
                            </div>
                        </section>
                    </div>
                </transition>
            </div>
        </sidebar-inset>
        <transition name="startupSplash">
            <section
                v-if="showStartupSplash"
                class="windowDragRegion fixed inset-0 z-50 grid cursor-default select-none place-items-center overflow-hidden bg-background text-foreground"
                data-tauri-drag-region="deep"
                aria-live="polite">
                <div
                    class="absolute inset-0 bg-[linear-gradient(135deg,hsl(var(--background))_0%,hsl(var(--card))_54%,hsl(var(--muted))_100%)]"></div>
                <div class="absolute inset-0 border border-white/5"></div>
                <div
                    class="relative grid w-full max-w-[360px] place-items-center gap-5 px-8 text-center"
                    data-tauri-drag-region>
                    <img
                        class="h-20 w-20 rounded-2xl border border-border/70 bg-card p-2 shadow-xl shadow-background/35"
                        :src="brandLogoUrl"
                        alt="CodexMan"
                        data-tauri-drag-region />
                    <div
                        class="grid gap-2"
                        data-tauri-drag-region>
                        <h1
                            class="text-[23px] font-semibold leading-8 tracking-normal text-foreground"
                            data-tauri-drag-region>
                            CodexMan
                        </h1>
                        <p
                            class="text-[13px] leading-6 text-muted-foreground"
                            data-tauri-drag-region>
                            正在加载本机服务，请稍后
                        </p>
                    </div>
                    <div
                        class="h-1 w-36 overflow-hidden rounded-full bg-muted"
                        data-tauri-drag-region>
                        <span class="startupSplash__progress block h-full w-1/2 rounded-full bg-primary"></span>
                    </div>
                </div>
            </section>
        </transition>
        <codex-connection-dialog />
        <Dialog
            :open="approvalDialogVisible"
            @update:open="handleApprovalDialogOpenChange">
            <DialogContent class="windowNoDrag sm:max-w-[440px]">
                <DialogHeader>
                    <DialogTitle>是否确认授权</DialogTitle>
                    <DialogDescription>
                        浏览器插件正在申请 codexMan 授权码。确认后，插件可以读取任务项目并创建浏览器标注任务。
                    </DialogDescription>
                </DialogHeader>
                <div class="grid gap-3 rounded-md border border-border bg-muted/35 p-3 text-[13px] leading-6">
                    <div class="flex items-center justify-between gap-3">
                        <span class="text-muted-foreground">申请方</span>
                        <span class="min-w-0 truncate text-foreground">
                            {{ accessTokenApprovalRequest?.name || '-' }}
                        </span>
                    </div>
                    <div class="flex items-center justify-between gap-3">
                        <span class="text-muted-foreground">有效期</span>
                        <span class="text-foreground">{{ accessTokenApprovalExpiresText }}</span>
                    </div>
                    <div class="flex items-center justify-between gap-3">
                        <span class="text-muted-foreground">请求 ID</span>
                        <span class="min-w-0 truncate font-mono text-[12px] text-foreground">
                            {{ accessTokenApprovalRequest?.requestId || '-' }}
                        </span>
                    </div>
                </div>
                <DialogFooter>
                    <button
                        class="inline-flex h-9 items-center justify-center rounded-md border border-input bg-background px-3 text-[13px] font-medium text-foreground shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground disabled:pointer-events-none disabled:opacity-50"
                        type="button"
                        :disabled="approvalSubmitting"
                        @click="handleRespondAccessTokenApproval(false)">
                        拒绝
                    </button>
                    <button
                        class="inline-flex h-9 items-center justify-center rounded-md bg-primary px-3 text-[13px] font-medium text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50"
                        type="button"
                        :disabled="approvalSubmitting"
                        @click="handleRespondAccessTokenApproval(true)">
                        确认授权
                    </button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    </sidebar-provider>
</template>

<script setup lang="ts">
    import {
        ApplicationMenu,
        Keyboard,
        KeyboardOne,
        LinkBreak,
        List,
        Magic,
        Microphone,
        Moon,
        Permissions,
        Setting,
        SunOne,
        Terminal
    } from '@icon-park/vue-next';

    import brandLogoUrl from '@/assets/codexManLogo.png';
    import CodexConnectionDialog from '@/components/layout/codexConnectionDialog.vue';
    import {
        Dialog,
        DialogContent,
        DialogDescription,
        DialogFooter,
        DialogHeader,
        DialogTitle
    } from '@/components/ui/dialog';
    import {
        Sidebar,
        SidebarContent,
        SidebarFooter,
        SidebarHeader,
        SidebarInset,
        SidebarMenu,
        SidebarMenuButton,
        SidebarMenuItem,
        SidebarProvider
    } from '@/components/ui/sidebar';
    import { Switch as UiSwitch } from '@/components/ui/switch';
    import { HubRouteName } from '@/router';
    import type { PublicApiAccessTokenApprovalEventModel } from '@/service/tauri/command';
    import {
        checkPublicApiHealth,
        isTauriRuntime,
        listenEvent,
        respondPublicApiAccessTokenRequest
    } from '@/service/tauri/command';
    import { useCodexConnectionStore } from '@/stores/codexConnection';
    import { usePermissionStore } from '@/stores/permission';
    import { useSettingsStore } from '@/stores/settings';
    import { useTextPolishStore } from '@/stores/textPolish';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'MainLayout'
    });

    const permissionStore = usePermissionStore();
    const codexConnectionStore = useCodexConnectionStore();
    const settingsStore = useSettingsStore();
    const textPolishStore = useTextPolishStore();
    const voicePolishStore = useVoicePolishStore();
    const route = useRoute();
    const router = useRouter();
    const isClientRuntime = isTauriRuntime();
    /** 公共 HTTP 服务启动期健康检查间隔；用于 sidecar 后台启动后尽快恢复首屏功能。 */
    const PUBLIC_API_STARTUP_HEALTH_INTERVAL_MS = 1_000;
    /** 公共 HTTP 健康检查稳定间隔；服务已连接后降低后台请求频率。 */
    const PUBLIC_API_HEALTH_INTERVAL_MS = 30_000;
    const clientBridgeHealthy = ref(false);
    const hasClientBridgeEverReady = ref(false);
    const approvalSubmitting = ref(false);
    const accessTokenApprovalRequest = ref<PublicApiAccessTokenApprovalEventModel | null>(null);
    let clientBridgeHealthTimer: number | undefined;
    let unlistenAccessTokenApproval: (() => void) | undefined;
    let stopRoutePermissionWatcher: (() => void) | undefined;

    const navItems = [
        { routeName: HubRouteName.VoicePolish, label: '语音转文字润色', icon: Microphone },
        { routeName: HubRouteName.TextPolish, label: '润色', icon: Magic },
        { routeName: HubRouteName.ShortcutBinding, label: '快捷键绑定', icon: Keyboard },
        { routeName: HubRouteName.SessionManage, label: '会话管理', icon: Terminal },
        { routeName: HubRouteName.TaskManage, label: '任务管理', icon: List },
        { routeName: HubRouteName.Permission, label: '权限管理', icon: Permissions },
        { routeName: HubRouteName.ModelManage, label: '模型管理', icon: KeyboardOne },
        { routeName: HubRouteName.MyApp, label: '我的应用', icon: ApplicationMenu }
    ] as const;

    const showClientBridgeOverlay = computed(() => {
        return !isClientRuntime && !clientBridgeHealthy.value && route.name !== HubRouteName.HttpApiDoc;
    });
    const showStartupSplash = computed(() => isClientRuntime && !hasClientBridgeEverReady.value);
    const publicApiHealthText = computed(() => (clientBridgeHealthy.value ? 'HTTP 服务已连接' : 'HTTP 服务未连接'));
    const clientBridgeHealthDotClass = computed(() =>
        clientBridgeHealthy.value ? 'bg-emerald-500' : 'bg-sidebar-foreground/35'
    );
    const clientBridgeHealthTextClass = computed(() =>
        clientBridgeHealthy.value ? 'text-sidebar-foreground' : 'text-sidebar-foreground/45'
    );
    const approvalDialogVisible = computed(() => accessTokenApprovalRequest.value !== null);
    const accessTokenApprovalExpiresText = computed(() => accessTokenApprovalRequest.value?.expiresAt || '永久有效');
    /** Codex 连接状态行的实时中文文案。 */
    const codexConnectionText = computed<string>(() => {
        if (codexConnectionStore.connectionState === 'checking') return 'Codex 检测中';
        if (codexConnectionStore.connectionState === 'connected') return 'Codex 已连接';
        if (codexConnectionStore.connectionState === 'disconnected') return 'Codex 未连接';
        if (codexConnectionStore.connectionState === 'restarting') return 'Codex 重启中';
        if (codexConnectionStore.connectionState === 'blocked') return 'Codex 连接受阻';
        if (codexConnectionStore.connectionState === 'unsupported') return 'Codex 不受支持';
        return 'Codex 状态未知';
    });
    /** Codex 状态圆点颜色；检测和重启使用脉冲提示状态仍在变化。 */
    const codexConnectionDotClass = computed<string>(() => {
        if (codexConnectionStore.connectionState === 'connected') return 'bg-emerald-500';
        if (
            codexConnectionStore.connectionState === 'disconnected' ||
            codexConnectionStore.connectionState === 'blocked'
        ) {
            return 'bg-destructive';
        }
        if (
            codexConnectionStore.connectionState === 'checking' ||
            codexConnectionStore.connectionState === 'restarting'
        ) {
            return 'animate-pulse bg-primary';
        }
        return 'bg-sidebar-foreground/35';
    });
    /** Codex 状态入口文字颜色，明确断连使用危险色，其余状态保持侧栏层级。 */
    const codexConnectionTextClass = computed<string>(() => {
        if (
            codexConnectionStore.connectionState === 'disconnected' ||
            codexConnectionStore.connectionState === 'blocked'
        ) {
            return 'text-destructive';
        }
        if (
            codexConnectionStore.connectionState === 'unknown' ||
            codexConnectionStore.connectionState === 'unsupported'
        ) {
            return 'text-sidebar-foreground/45';
        }
        return 'text-sidebar-foreground';
    });

    /**
     * Tauri Hub 事件传入的视图键与真实页面路由的映射。
     * 业务含义：原生托盘或快捷键只能通知当前真实存在的页面，首发版本不兼容已下线的旧入口。
     */
    const routeNameByHubView = {
        voicePolish: HubRouteName.VoicePolish,
        textPolish: HubRouteName.TextPolish,
        shortcutBinding: HubRouteName.ShortcutBinding,
        sessionManage: HubRouteName.SessionManage,
        taskManage: HubRouteName.TaskManage,
        permission: HubRouteName.Permission,
        modelManage: HubRouteName.ModelManage,
        httpApiDoc: HubRouteName.HttpApiDoc,
        settings: HubRouteName.Settings,
        dictionary: HubRouteName.VoicePolishDictionary
    } as const;

    /**
     * Tauri Hub 事件视图键类型。
     * 业务含义：约束外部事件只允许跳转到已登记的 Hub 页面，避免非法字符串进入路由跳转。
     */
    type HubSwitchViewKey = keyof typeof routeNameByHubView;

    /**
     * 判断外部事件传入的视图键是否可路由。
     * 流程：用对象自有属性判断 view 是否存在于映射表中。
     * 参数：view 为 Tauri 事件传入的原始视图键。
     * 返回：如果 view 已登记则返回 true，并收窄为 HubSwitchViewKey。
     * 边界：未知 view 不触发跳转，避免外部事件把页面带到不可达状态。
     */
    function isHubSwitchViewKey(view: string): view is HubSwitchViewKey {
        return Object.prototype.hasOwnProperty.call(routeNameByHubView, view);
    }

    /**
     * 判断当前路由是否需要读取最新权限状态。
     * 流程：权限管理页展示全部系统授权，语音页直接依赖麦克风授权，两类页面都需要避免使用旧 Store 缓存。
     * 参数：routeName 为 Vue Router 当前路由名称。
     * 返回：命中权限敏感页面时返回 true。
     * 边界：未命名路由或其它页面不触发额外刷新，减少无关页面的系统诊断请求。
     */
    function isPermissionSensitiveRoute(routeName: typeof route.name): boolean {
        return routeName === HubRouteName.Permission || routeName === HubRouteName.VoicePolish;
    }

    /**
     * 在权限敏感页面刷新当前系统权限状态。
     * 流程：仅当当前路由需要权限诊断时调用权限 Store，并允许一次真实麦克风可用性探测。
     * 参数：无。
     * 返回：无返回值。
     * 边界：刷新失败由 Store 写入 message，不阻塞路由或窗口焦点恢复。
     */
    function refreshPermissionsForActiveRoute(): void {
        if (!isPermissionSensitiveRoute(route.name)) return;
        void permissionStore.refreshPermissions({ probeMicrophoneAccess: true });
    }

    /**
     * 跳转到指定 Hub 页面。
     * 流程：根据路由名称执行 router.push，让 URL、选中态和页面内容保持一致。
     * 参数：routeName 为 HubRouteName 中登记的页面名称。
     * 返回：无返回值。
     * 边界：重复点击当前页面时由 Vue Router 自身忽略重复导航。
     */
    function handleNavigate(routeName: (typeof HubRouteName)[keyof typeof HubRouteName]): void {
        void router.push({ name: routeName });
    }

    /**
     * 打开公共 HTTP API 文档页。
     * 流程：点击侧边栏底部服务状态时跳转到文档路由，由文档页读取 `/openapi.json` 渲染。
     * 参数：无。
     * 返回：无返回值。
     * 边界：HTTP 服务未连接时仍允许进入页面，页面会展示文档读取失败原因。
     */
    function handleOpenHttpApiDoc(): void {
        handleNavigate(HubRouteName.HttpApiDoc);
    }

    /**
     * 判断侧边栏菜单是否应展示选中态。
     * 流程：语音润色的词典子页归入语音润色模块，其余页面按自身路由名称精确匹配。
     * 参数：routeName 为侧边栏菜单绑定的目标路由名称。
     * 返回：当前路由属于该菜单业务模块时返回 true。
     * 边界：未命名路由不会命中任何菜单，避免错误高亮。
     */
    function isNavItemActive(routeName: (typeof HubRouteName)[keyof typeof HubRouteName]): boolean {
        if (routeName === HubRouteName.VoicePolish) {
            return route.name === HubRouteName.VoicePolish || route.name === HubRouteName.VoicePolishDictionary;
        }
        return route.name === routeName;
    }

    /**
     * 刷新公共 HTTP 服务健康状态。
     * 流程：请求独立服务 `/health` 端点，并把成功结果同步到侧边栏状态行。
     * 参数：无。
     * 返回：刷新完成 Promise。
     * 边界：客户端未启动、端口不可达或响应异常时统一显示未连接，不打断页面其它功能。
     */
    async function refreshClientBridgeHealth(): Promise<void> {
        clientBridgeHealthy.value = await checkPublicApiHealth();
        if (clientBridgeHealthy.value) {
            hasClientBridgeEverReady.value = true;
        }
    }

    /**
     * 启动公共 HTTP 服务健康状态轮询。
     * 流程：页面挂载后立即检查一次；未连接时按启动期间隔快速恢复，连接后改用稳定间隔。
     * 参数：无。
     * 返回：无返回值。
     * 边界：组件卸载时清理旧定时器，避免重复轮询；单次失败不会打断页面展示。
     */
    function startClientBridgeHealthPolling(): void {
        const runPolling = async (): Promise<void> => {
            await refreshClientBridgeHealth();
            const intervalMs = clientBridgeHealthy.value
                ? PUBLIC_API_HEALTH_INTERVAL_MS
                : PUBLIC_API_STARTUP_HEALTH_INTERVAL_MS;
            clientBridgeHealthTimer = window.setTimeout(() => {
                void runPolling();
            }, intervalMs);
        };
        void runPolling();
    }

    /**
     * 记录浏览器插件发起的授权申请并打开确认弹窗。
     * 流程：保存事件载荷，主布局通过 Dialog 展示；若已有申请正在处理，则以后到事件为准刷新弹窗内容。
     * 参数：payload 为 Rust 通过私有 RPC 转发的授权申请。
     * 返回：无返回值。
     * 边界：不在前端生成或保存授权码，确认后仍由后端创建。
     */
    function handleAccessTokenApprovalRequested(payload: PublicApiAccessTokenApprovalEventModel): void {
        accessTokenApprovalRequest.value = payload;
        approvalSubmitting.value = false;
    }

    /**
     * 响应授权确认弹窗。
     * 流程：读取当前 requestId，调用 Tauri 命令唤醒等待中的 HTTP 请求，然后关闭弹窗。
     * 参数：approved 表示用户是否同意创建授权码。
     * 返回：异步完成 Promise。
     * 边界：命令失败时展示侧栏通知文案并关闭本次弹窗，避免前端状态卡住。
     */
    async function handleRespondAccessTokenApproval(approved: boolean): Promise<void> {
        const request = accessTokenApprovalRequest.value;
        if (!request || approvalSubmitting.value) return;
        approvalSubmitting.value = true;
        try {
            await respondPublicApiAccessTokenRequest(request.requestId, approved);
        } catch (error) {
            permissionStore.message = error instanceof Error ? error.message : '授权确认失败，请重新发起。';
        } finally {
            approvalSubmitting.value = false;
            accessTokenApprovalRequest.value = null;
        }
    }

    /**
     * 处理授权弹窗显隐变化。
     * 流程：用户点遮罩或 Esc 关闭时按拒绝处理，保证 HTTP 请求不会一直等待。
     * 参数：open 为 Dialog 新显隐状态。
     * 返回：无返回值。
     * 边界：程序主动关闭弹窗时当前申请已清空，不会重复发送拒绝。
     */
    function handleApprovalDialogOpenChange(open: boolean): void {
        if (!open && accessTokenApprovalRequest.value && !approvalSubmitting.value) {
            void handleRespondAccessTokenApproval(false);
        }
    }

    onMounted(async () => {
        settingsStore.applyThemeMode(settingsStore.settings.themeMode);
        startClientBridgeHealthPolling();
        codexConnectionStore.startPolling();
        await permissionStore.refreshPermissions({ probeMicrophoneAccess: true });
        window.addEventListener('focus', refreshPermissionsForActiveRoute);
        stopRoutePermissionWatcher = watch(
            () => route.name,
            () => {
                refreshPermissionsForActiveRoute();
            }
        );
        await listenEvent<string>('hub-switch-view', (view) => {
            if (isHubSwitchViewKey(view)) {
                handleNavigate(routeNameByHubView[view]);
            }
        });
        await listenEvent<{ message: string; state: string }>('hub-show-notice', (payload) => {
            permissionStore.message = payload.message;
        });
        await listenEvent<string>('hub-start-mode', (mode) => {
            if (mode === 'polish') void textPolishStore.polishSelectedText();
            if (mode === 'asr') void voicePolishStore.runVoicePolish('', 'asr');
            if (mode === 'dictate') void voicePolishStore.runVoicePolish('');
        });
        unlistenAccessTokenApproval = await listenEvent<PublicApiAccessTokenApprovalEventModel>(
            'public-api-access-token-requested',
            handleAccessTokenApprovalRequested
        );
    });

    onUnmounted(() => {
        codexConnectionStore.stopPolling();
        stopRoutePermissionWatcher?.();
        window.removeEventListener('focus', refreshPermissionsForActiveRoute);
        if (clientBridgeHealthTimer !== undefined) {
            window.clearTimeout(clientBridgeHealthTimer);
        }
        unlistenAccessTokenApproval?.();
    });
</script>

<style scoped>
    .windowDragRegion {
        -webkit-app-region: drag;
    }

    .windowNoDrag {
        -webkit-app-region: no-drag;
    }

    .pageSwitch-enter-active,
    .pageSwitch-leave-active {
        transition:
            opacity 150ms ease,
            transform 150ms ease;
    }

    .pageSwitch-enter-from,
    .pageSwitch-leave-to {
        opacity: 0;
        transform: translateY(6px);
    }

    .clientBridgeOverlay-enter-active,
    .clientBridgeOverlay-leave-active {
        transition:
            opacity 180ms ease,
            transform 180ms ease;
    }

    .clientBridgeOverlay-enter-from,
    .clientBridgeOverlay-leave-to {
        opacity: 0;
        transform: scale(0.985);
    }

    .startupSplash-enter-active,
    .startupSplash-leave-active {
        transition: opacity 260ms ease;
    }

    .startupSplash-enter-from,
    .startupSplash-leave-to {
        opacity: 0;
    }

    .startupSplash__progress {
        animation: startupSplashProgress 1.15s ease-in-out infinite;
        transform-origin: left center;
    }

    @keyframes startupSplashProgress {
        0% {
            transform: translateX(-120%) scaleX(0.65);
        }

        50% {
            transform: translateX(55%) scaleX(1);
        }

        100% {
            transform: translateX(220%) scaleX(0.65);
        }
    }
</style>
