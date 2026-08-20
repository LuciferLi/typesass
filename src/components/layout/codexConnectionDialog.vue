<template>
    <ui-dialog
        :open="store.dialogOpen"
        @update:open="store.setDialogOpen">
        <ui-dialog-content class="max-w-[520px]">
            <ui-dialog-header class="gap-2">
                <div
                    class="mb-1 grid h-10 w-10 place-items-center rounded-md border"
                    :class="iconContainerClass"
                    :role="isFailure ? 'alert' : 'status'"
                    aria-live="polite">
                    <loading
                        v-if="isBusy"
                        class="animate-spin"
                        theme="outline"
                        size="19" />
                    <check-one
                        v-else-if="isSuccess || store.connectionState === 'connected'"
                        theme="outline"
                        size="19" />
                    <terminal
                        v-else
                        theme="outline"
                        size="19" />
                </div>
                <ui-dialog-title>{{ dialogTitle }}</ui-dialog-title>
                <ui-dialog-description class="leading-6">{{ dialogDescription }}</ui-dialog-description>
            </ui-dialog-header>

            <div
                v-if="secondaryDescription"
                class="rounded-md border border-border/70 bg-muted/45 px-3 py-2.5 text-[13px] leading-5 text-muted-foreground">
                {{ secondaryDescription }}
            </div>

            <ui-dialog-footer class="mt-2">
                <template v-if="isBusy">
                    <ui-button
                        type="button"
                        disabled>
                        <loading
                            class="animate-spin"
                            theme="outline"
                            size="15" />
                        重启中
                    </ui-button>
                </template>
                <template v-else-if="isSuccess">
                    <ui-button
                        type="button"
                        @click="handleClose">
                        完成
                    </ui-button>
                </template>
                <template v-else-if="store.connectionState === 'connected'">
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="!store.canRestart"
                        @click="handleRestart">
                        重启 Codex
                    </ui-button>
                    <ui-button
                        type="button"
                        @click="handleClose">
                        知道了
                    </ui-button>
                </template>
                <template v-else-if="isFailure">
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="handleClose">
                        取消
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="!store.canRestart"
                        @click="handleRestart">
                        重试
                    </ui-button>
                </template>
                <template v-else-if="store.connectionState === 'unknown' || store.connectionState === 'checking'">
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="handleClose">
                        关闭
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="store.requestInFlight"
                        @click="handleRefresh">
                        {{ store.requestInFlight ? '检测中' : '重新检测' }}
                    </ui-button>
                </template>
                <template v-else-if="store.connectionState === 'unsupported'">
                    <ui-button
                        type="button"
                        @click="handleClose">
                        知道了
                    </ui-button>
                </template>
                <template v-else>
                    <ui-button
                        variant="outline"
                        type="button"
                        @click="handleClose">
                        稍后处理
                    </ui-button>
                    <ui-button
                        type="button"
                        :disabled="!store.canRestart"
                        @click="handleRestart">
                        重启 Codex
                    </ui-button>
                </template>
            </ui-dialog-footer>
        </ui-dialog-content>
    </ui-dialog>
</template>

<script setup lang="ts">
    import { CheckOne, Loading, Terminal } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import { useCodexConnectionStore } from '@/stores/codexConnection';

    defineOptions({
        name: 'CodexConnectionDialog'
    });

    const store = useCodexConnectionStore();

    /** 弹窗是否正在等待异步重启的最终连接结果。 */
    const isBusy = computed<boolean>(() => store.restartAwaitingResult || store.connectionState === 'restarting');
    /** 弹窗是否展示本次重启成功结果。 */
    const isSuccess = computed<boolean>(() => store.dialogResult === 'success');
    /** 弹窗是否展示本次重启失败结果。 */
    const isFailure = computed<boolean>(() => store.dialogResult === 'failure');

    /** 弹窗标题，优先表达当前操作结果，再回退实时连接状态。 */
    const dialogTitle = computed<string>(() => {
        if (isSuccess.value) return 'Codex 已重新连接';
        if (isFailure.value) return 'Codex 重启失败';
        if (isBusy.value) return '正在重启 Codex';
        if (store.connectionState === 'connected') return 'Codex 已连接';
        if (store.connectionState === 'disconnected') return 'Codex 未连接';
        if (store.connectionState === 'blocked') return 'Codex 连接受阻';
        if (store.connectionState === 'unsupported') return '当前平台不支持 Codex 连接';
        if (store.connectionState === 'checking') return '正在检测 Codex';
        return '暂时无法获取 Codex 状态';
    });

    /** 弹窗主说明，明确连接用途、阻断范围和当前可执行操作。 */
    const dialogDescription = computed<string>(() => {
        if (isSuccess.value) return '现在可以由 Codex Desktop 原生创建新会话并发送任务。';
        if (isFailure.value) return '未能重新连接 Codex。请确认 Codex 已安装并可正常启动，然后重试。';
        if (isBusy.value) return '重启期间无法创建新会话或发送任务，请稍候。';
        if (store.connectionState === 'connected') {
            return '连接正常。CodexMan 通过 Codex Desktop 原生创建新会话并发送首次任务，避免独立写入与 Desktop 状态冲突。';
        }
        if (store.connectionState === 'disconnected') {
            return '已有会话仍可查询；由任务排队创建 Codex 会话并发送首次任务需要先连接 Codex Desktop。';
        }
        if (store.connectionState === 'blocked') {
            return '已有会话仍可查询；Codex 当前无法建立安全连接，任务排队执行暂不可用。';
        }
        if (store.connectionState === 'unsupported') {
            return '当前操作系统不支持 Codex Desktop 连接和重启；已有会话仍可查询，任务排队执行不可用。';
        }
        if (store.connectionState === 'checking') return '正在通过本机 HTTP 服务确认 Codex 连接状态。';
        return '连接状态检查失败。CodexMan 不会在状态不明时创建 Codex 会话或发送任务。';
    });

    /** 仅在失败、未知或明确断连时展示的补充诊断，不重复健康态说明。 */
    const secondaryDescription = computed<string>(() => {
        if (isFailure.value) return store.restartErrorMessage;
        if (store.connectionState === 'unknown') return store.message;
        if (store.connectionState === 'connected') {
            return '如果会话页面或任务发送异常，可以手动重启。正常退出失败时，CodexMan 会强制结束已验证的官方 Codex 进程；未发送的草稿和尚未完成的手工任务可能丢失。';
        }
        if (store.connectionState === 'disconnected' || store.connectionState === 'blocked') {
            if (!store.canRestart) return store.message || '当前状态暂不允许重启，请稍后重新检测。';
            return '正常退出失败时，CodexMan 会强制结束已验证的官方 Codex 进程；未发送的草稿和尚未完成的手工任务可能丢失。只有点击“重启 Codex”后才会执行。';
        }
        if (store.connectionState === 'unsupported') return store.message;
        return '';
    });

    /** 状态图标容器颜色，保持与侧栏连接语义一致。 */
    const iconContainerClass = computed<string>(() => {
        if (isSuccess.value || store.connectionState === 'connected') {
            return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600';
        }
        if (isFailure.value || store.connectionState === 'disconnected' || store.connectionState === 'blocked') {
            return 'border-destructive/30 bg-destructive/10 text-destructive';
        }
        if (isBusy.value) return 'border-primary/30 bg-primary/10 text-primary';
        return 'border-border bg-muted text-muted-foreground';
    });

    /**
     * 关闭连接说明弹窗。
     * 流程：委托 Store 执行关闭，重启等待期间 Store 会拒绝关闭请求。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不会取消已经接受的后台重启。
     */
    function handleClose(): void {
        store.setDialogOpen(false);
    }

    /**
     * 手动重新检测 Codex 连接。
     * 流程：执行共享单飞 HTTP 刷新，本次明确断连不重复触发自动弹窗。
     * 参数：无。
     * 返回：无返回值。
     * 边界：按钮在请求期间禁用，避免重复操作。
     */
    function handleRefresh(): void {
        void store.refreshConnection(false);
    }

    /**
     * 确认重启 Codex Desktop。
     * 流程：委托 Store 通过 HTTP 提交异步重启，并由轮询确认最终状态。
     * 参数：无。
     * 返回：无返回值。
     * 边界：不可重启或已有重启流程时 Store 会忽略重复请求。
     */
    function handleRestart(): void {
        void store.restartConnection();
    }
</script>
