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
        <codex-connection-dialog />
    </sidebar-provider>
</template>

<script setup lang="ts">
    import {
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
    import { checkPublicApiHealth, isTauriRuntime, listenEvent } from '@/service/tauri/command';
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
    /** 公共 HTTP 健康检查间隔；30 秒可及时恢复状态，同时避免高频请求淹没业务访问日志。 */
    const PUBLIC_API_HEALTH_INTERVAL_MS = 30_000;
    const clientBridgeHealthy = ref(false);
    let clientBridgeHealthTimer: number | undefined;

    const navItems = [
        { routeName: HubRouteName.VoicePolish, label: '语音转文字润色', icon: Microphone },
        { routeName: HubRouteName.TextPolish, label: '润色', icon: Magic },
        { routeName: HubRouteName.SessionManage, label: '会话管理', icon: Terminal },
        { routeName: HubRouteName.TaskManage, label: '任务管理', icon: List },
        { routeName: HubRouteName.Permission, label: '权限管理', icon: Permissions },
        { routeName: HubRouteName.ModelManage, label: '模型管理', icon: KeyboardOne }
    ] as const;

    const showClientBridgeOverlay = computed(() => {
        return !isClientRuntime && !clientBridgeHealthy.value && route.name !== HubRouteName.HttpApiDoc;
    });
    const publicApiHealthText = computed(() => (clientBridgeHealthy.value ? 'HTTP 服务已连接' : 'HTTP 服务未连接'));
    const clientBridgeHealthDotClass = computed(() =>
        clientBridgeHealthy.value ? 'bg-emerald-500' : 'bg-sidebar-foreground/35'
    );
    const clientBridgeHealthTextClass = computed(() =>
        clientBridgeHealthy.value ? 'text-sidebar-foreground' : 'text-sidebar-foreground/45'
    );
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
    }

    /**
     * 启动公共 HTTP 服务健康状态轮询。
     * 流程：页面挂载后立即检查一次，再按固定间隔持续刷新。
     * 参数：无。
     * 返回：无返回值。
     * 边界：组件卸载时清理旧定时器，避免重复轮询。
     */
    function startClientBridgeHealthPolling(): void {
        void refreshClientBridgeHealth();
        clientBridgeHealthTimer = window.setInterval(() => {
            void refreshClientBridgeHealth();
        }, PUBLIC_API_HEALTH_INTERVAL_MS);
    }

    onMounted(async () => {
        settingsStore.applyThemeMode(settingsStore.settings.themeMode);
        startClientBridgeHealthPolling();
        codexConnectionStore.startPolling();
        await permissionStore.refreshPermissions();
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
    });

    onUnmounted(() => {
        codexConnectionStore.stopPolling();
        if (clientBridgeHealthTimer !== undefined) {
            window.clearInterval(clientBridgeHealthTimer);
        }
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
</style>
