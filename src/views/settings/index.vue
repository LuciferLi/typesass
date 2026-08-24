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
            <ui-field class="rounded-md border border-border bg-muted/30 p-4">
                <ui-field-content>
                    <ui-field-label>用户英文名</ui-field-label>
                    <ui-field-description>
                        英文名会作为 HTTP API 外网访问域名前缀；每个“我的应用”仍可在创建或编辑时自定义自己的二级域名。
                    </ui-field-description>
                </ui-field-content>
                <ui-input
                    class="mt-3 h-9 max-w-[360px]"
                    maxlength="32"
                    placeholder="例如 lucifer"
                    :model-value="store.settings.userEnglishName"
                    @change="handleUserEnglishNameChanged" />
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
            <ui-field
                orientation="horizontal"
                class="rounded-md border border-border bg-muted/30 p-4">
                <ui-field-content>
                    <ui-field-label>任务并发上限</ui-field-label>
                    <ui-field-description>
                        同时提交到 Codex 执行的任务数量，调整后下一轮调度生效。
                    </ui-field-description>
                </ui-field-content>
                <ui-input
                    class="h-9 w-24 text-right"
                    type="number"
                    inputmode="numeric"
                    :min="SETTINGS_TASK_CONCURRENCY_MIN"
                    :max="SETTINGS_TASK_CONCURRENCY_MAX"
                    :model-value="String(store.settings.taskConcurrencyLimit)"
                    @change="handleTaskConcurrencyChanged" />
            </ui-field>
            <section class="grid gap-3 rounded-md border border-border bg-muted/30 p-4">
                <div class="flex flex-wrap items-start justify-between gap-3">
                    <div class="grid min-w-0 gap-1">
                        <h2 class="text-[14px] font-medium leading-6 text-foreground">App 授权码</h2>
                        <p class="text-[13px] leading-5 text-muted-foreground">
                            所有公网接入方都使用这里维护的授权码；没有设备码、短期 Token 或换 Token 流程。
                        </p>
                    </div>
                    <ui-button
                        variant="outline"
                        size="sm"
                        type="button"
                        :disabled="accessTokenLoading"
                        @click="loadAccessTokens">
                        <refresh
                            theme="outline"
                            size="15" />
                        <span>{{ accessTokenLoading ? '刷新中' : '刷新' }}</span>
                    </ui-button>
                </div>

                <form
                    class="grid gap-2 rounded-md border border-border bg-background/60 p-3 lg:grid-cols-[minmax(180px,1fr)_180px_auto]"
                    @submit.prevent="handleCreateAccessToken">
                    <ui-input
                        v-model="accessTokenName"
                        class="h-9"
                        maxlength="100"
                        placeholder="授权码名称，例如 Chrome 插件" />
                    <ui-input
                        v-model="accessTokenExpiresAt"
                        class="h-9"
                        placeholder="到期时间，可留空"
                        type="datetime-local" />
                    <ui-button
                        class="h-9"
                        type="submit"
                        :disabled="accessTokenCreating || !accessTokenName.trim()">
                        <plus
                            theme="outline"
                            size="15" />
                        <span>{{ accessTokenCreating ? '创建中' : '创建授权码' }}</span>
                    </ui-button>
                </form>

                <div
                    v-if="accessTokenLoading"
                    class="grid gap-2">
                    <ui-skeleton class="h-14 w-full" />
                    <ui-skeleton class="h-14 w-full" />
                </div>
                <div
                    v-else-if="!accessTokens.length"
                    class="rounded-md border border-dashed border-border p-6 text-center text-[13px] text-muted-foreground">
                    暂无授权码。创建后，公网来源业务接口可使用 Authorization: Bearer 授权码访问。
                </div>
                <div
                    v-else
                    class="overflow-hidden rounded-md border border-border bg-background/60">
                    <div
                        class="hidden grid-cols-[140px_minmax(220px,1fr)_150px_92px_150px_150px_132px] border-b border-border bg-muted/50 px-3 py-2 text-[12px] font-medium text-muted-foreground lg:grid">
                        <span>名称</span>
                        <span>授权码</span>
                        <span>有效期</span>
                        <span>状态</span>
                        <span>创建时间</span>
                        <span>最近使用</span>
                        <span class="text-right">操作</span>
                    </div>
                    <div class="divide-y divide-border">
                        <div
                            v-for="token in accessTokens"
                            :key="token.id"
                            class="grid gap-2 px-3 py-3 text-[13px] lg:grid-cols-[140px_minmax(220px,1fr)_150px_92px_150px_150px_132px] lg:items-center lg:gap-3">
                            <div class="min-w-0">
                                <span class="mb-1 block text-[11px] text-muted-foreground lg:hidden">名称</span>
                                <span class="block truncate font-medium text-foreground">{{ token.name }}</span>
                            </div>
                            <div class="min-w-0">
                                <span class="mb-1 block text-[11px] text-muted-foreground lg:hidden">授权码</span>
                                <code
                                    class="block min-w-0 truncate rounded bg-muted px-2 py-1.5 text-[12px] leading-5 text-foreground"
                                    :title="token.token">
                                    {{ token.token }}
                                </code>
                            </div>
                            <div class="min-w-0 text-muted-foreground">
                                <span class="mb-1 block text-[11px] lg:hidden">有效期</span>
                                <span class="block truncate">{{ formatAccessTokenTime(token.expiresAt) }}</span>
                            </div>
                            <div>
                                <span class="mb-1 block text-[11px] text-muted-foreground lg:hidden">状态</span>
                                <ui-badge
                                    variant="outline"
                                    :class="accessTokenStatusClass(token.status)">
                                    {{ accessTokenStatusText(token.status) }}
                                </ui-badge>
                            </div>
                            <div class="min-w-0 text-muted-foreground">
                                <span class="mb-1 block text-[11px] lg:hidden">创建时间</span>
                                <span class="block truncate">{{ formatAccessTokenTime(token.createdAt) }}</span>
                            </div>
                            <div class="min-w-0 text-muted-foreground">
                                <span class="mb-1 block text-[11px] lg:hidden">最近使用</span>
                                <span class="block truncate">{{ formatAccessTokenTime(token.lastUsedAt) }}</span>
                            </div>
                            <div class="flex justify-start gap-2 lg:justify-end">
                                <ui-button
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    title="复制授权码"
                                    @click="handleCopyAccessToken(token.token)">
                                    <copy
                                        theme="outline"
                                        size="15" />
                                    <span class="sr-only">复制授权码</span>
                                </ui-button>
                                <ui-button
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    title="撤销授权码"
                                    :disabled="token.status === 'revoked' || revokingAccessTokenId === token.id"
                                    @click="handleRevokeAccessToken(token.id)">
                                    <close-one
                                        theme="outline"
                                        size="15" />
                                    <span class="sr-only">撤销授权码</span>
                                </ui-button>
                            </div>
                        </div>
                    </div>
                </div>
            </section>
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
    import { CloseOne, Copy, Plus, Refresh } from '@icon-park/vue-next';
    import { toast } from 'vue-sonner';

    import { Alert as UiAlert } from '@/components/ui/alert';
    import { Badge as UiBadge } from '@/components/ui/badge';
    import { Button as UiButton } from '@/components/ui/button';
    import {
        Field as UiField,
        FieldContent as UiFieldContent,
        FieldDescription as UiFieldDescription,
        FieldLabel as UiFieldLabel
    } from '@/components/ui/field';
    import { Input as UiInput } from '@/components/ui/input';
    import { Skeleton as UiSkeleton } from '@/components/ui/skeleton';
    import { Switch as UiSwitch } from '@/components/ui/switch';
    import { SETTINGS_TASK_CONCURRENCY_MAX, SETTINGS_TASK_CONCURRENCY_MIN } from '@/model/settings';
    import {
        createPublicApiAccessToken,
        listPublicApiAccessTokens,
        type PublicApiAccessTokenModel,
        type PublicApiAccessTokenStatus,
        revokePublicApiAccessToken
    } from '@/service/tauri/command';
    import { useSettingsStore } from '@/stores/settings';

    defineOptions({
        name: 'SettingsView'
    });

    const store = useSettingsStore();
    const accessTokens = ref<PublicApiAccessTokenModel[]>([]);
    const accessTokenLoading = ref(false);
    const accessTokenCreating = ref(false);
    const accessTokenName = ref('');
    const accessTokenExpiresAt = ref('');
    const revokingAccessTokenId = ref('');

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

    /**
     * 保存用户英文名。
     * 流程：读取输入框值，交给 Store 统一规范化并持久化；若公网访问已开启，则同步重启为新域名。
     * 参数：event 为输入框 change 事件。
     * 返回：无返回值。
     * 边界：英文名会影响 HTTP API 固定域名，不会修改已有应用自定义域名。
     */
    function handleUserEnglishNameChanged(event: Event): void {
        const { target } = event;
        if (!(target instanceof HTMLInputElement)) return;
        void store
            .updateUserEnglishName(target.value)
            .then((publicUrl) => {
                toast.success('已更新用户英文名', {
                    description: publicUrl ? `HTTP API 外网地址已更新为 ${publicUrl}` : undefined
                });
            })
            .catch((error: unknown) => {
                showSettingsOperationError('更新用户英文名失败', error, '用户英文名保存失败。');
            });
    }

    /**
     * 保存任务执行并发上限。
     * 流程：读取数字输入框当前值，交给 Store 统一收敛范围并持久化，再给出短反馈。
     * 参数：event 为输入框 change 事件。
     * 返回：无返回值。
     * 边界：空值或非法值会回落到默认并发上限，不把无效字符串写入配置文件。
     */
    function handleTaskConcurrencyChanged(event: Event): void {
        const { target } = event;
        if (!(target instanceof HTMLInputElement)) return;
        store.updateTaskConcurrencyLimit(Number(target.value));
        toast.success('已更新任务并发上限');
    }

    /**
     * 读取系统设置页授权码列表。
     * 流程：调用公共 HTTP 授权码管理接口刷新本地列表；失败时保留旧列表并提示 requestId。
     * 参数：无。
     * 返回：刷新完成 Promise。
     * 边界：HTTP 服务未启动或当前来源未授权时不伪造空列表。
     */
    async function loadAccessTokens(): Promise<void> {
        accessTokenLoading.value = true;
        try {
            accessTokens.value = await listPublicApiAccessTokens();
        } catch (error) {
            showSettingsOperationError('读取授权码失败', error, '授权码列表读取失败。');
        } finally {
            accessTokenLoading.value = false;
        }
    }

    /**
     * 创建系统设置页授权码。
     * 流程：校验名称和可选到期时间，再调用授权码创建接口，成功后插入列表顶部。
     * 参数：无。
     * 返回：创建完成 Promise。
     * 边界：到期时间为空表示永久有效；datetime-local 自动补成本地时区 ISO 时间。
     */
    async function handleCreateAccessToken(): Promise<void> {
        const name = accessTokenName.value.trim();
        if (!name || accessTokenCreating.value) return;
        accessTokenCreating.value = true;
        try {
            const token = await createPublicApiAccessToken(name, normalizeAccessTokenExpiresAt());
            accessTokens.value = [token, ...accessTokens.value.filter((item) => item.id !== token.id)];
            accessTokenName.value = '';
            accessTokenExpiresAt.value = '';
            toast.success('已创建授权码');
        } catch (error) {
            showSettingsOperationError('创建授权码失败', error, '授权码创建失败。');
        } finally {
            accessTokenCreating.value = false;
        }
    }

    /**
     * 撤销系统设置页授权码。
     * 流程：按 ID 调用撤销接口，成功后用服务端返回记录替换当前列表项。
     * 参数：tokenId 为授权码稳定 ID。
     * 返回：撤销完成 Promise。
     * 边界：重复点击同一授权码时只保留一个进行中的撤销请求。
     */
    async function handleRevokeAccessToken(tokenId: string): Promise<void> {
        if (revokingAccessTokenId.value) return;
        revokingAccessTokenId.value = tokenId;
        try {
            const revokedToken = await revokePublicApiAccessToken(tokenId);
            accessTokens.value = accessTokens.value.map((item) => (item.id === tokenId ? revokedToken : item));
            toast.success('已撤销授权码');
        } catch (error) {
            showSettingsOperationError('撤销授权码失败', error, '授权码撤销失败。');
        } finally {
            revokingAccessTokenId.value = '';
        }
    }

    /**
     * 复制授权码明文。
     * 流程：优先使用浏览器剪贴板 API，成功后给出短提示。
     * 参数：token 为授权码明文。
     * 返回：复制完成 Promise。
     * 边界：剪贴板权限不可用时展示错误，不尝试写入隐藏输入框绕过权限。
     */
    async function handleCopyAccessToken(token: string): Promise<void> {
        try {
            if (!navigator.clipboard?.writeText) throw new Error('当前环境不支持剪贴板写入。');
            await navigator.clipboard.writeText(token);
            toast.success('已复制授权码');
        } catch (error) {
            showSettingsOperationError('复制授权码失败', error, '授权码复制失败。');
        }
    }

    /**
     * 规范化授权码到期时间。
     * 流程：把 datetime-local 输入转为 ISO 字符串；空输入保持永久有效。
     * 返回：ISO 到期时间或 null。
     * 边界：非法时间交给后端字段校验兜底，不在前端静默改写为永久有效。
     */
    function normalizeAccessTokenExpiresAt(): string | null {
        if (!accessTokenExpiresAt.value) return null;
        const expiresAt = new Date(accessTokenExpiresAt.value);
        if (Number.isNaN(expiresAt.getTime())) return accessTokenExpiresAt.value;
        return expiresAt.toISOString();
    }

    /**
     * 格式化授权码时间字段。
     * 流程：空值显示永久或从未使用；合法时间按本机区域展示。
     * 参数：time 为服务端 ISO 时间或空值。
     * 返回：页面展示文案。
     * 边界：异常时间原样返回，避免隐藏服务端数据问题。
     */
    function formatAccessTokenTime(time: string | null): string {
        if (!time) return '无';
        const date = new Date(time);
        if (Number.isNaN(date.getTime())) return time;
        return date.toLocaleString();
    }

    /**
     * 读取授权码状态展示文案。
     * 流程：把服务端稳定状态映射为用户可读中文。
     * 参数：status 为授权码状态。
     * 返回：状态展示文案。
     * 边界：状态枚举扩展时通过默认分支保持可见。
     */
    function accessTokenStatusText(status: PublicApiAccessTokenStatus): string {
        if (status === 'active') return '有效';
        if (status === 'expired') return '已过期';
        return '已撤销';
    }

    /**
     * 读取授权码状态样式。
     * 流程：按状态返回现有 Badge 可叠加的颜色类。
     * 参数：status 为授权码状态。
     * 返回：Tailwind class 字符串。
     * 边界：不修改 Badge 基础样式，避免影响其它页面。
     */
    function accessTokenStatusClass(status: PublicApiAccessTokenStatus): string {
        if (status === 'active') return 'border-primary/40 bg-primary/10 text-primary';
        if (status === 'expired') return 'border-amber-500/40 bg-amber-500/10 text-amber-600';
        return 'border-destructive/40 bg-destructive/10 text-destructive';
    }

    onMounted(() => {
        void store.initSettings();
        void loadAccessTokens();
    });
</script>
