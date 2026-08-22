<template>
    <aside class="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] bg-transparent">
        <div class="flex min-w-0 items-center gap-2 border-b border-border p-3">
            <ui-select-root
                :model-value="selectedWorkspaceCwd"
                :disabled="loading || !workspaces.length"
                @update:model-value="handleSelectWorkspace">
                <ui-select-trigger
                    class="h-9 w-[140px] min-w-[140px] text-sm"
                    :title="selectedWorkspaceTitle">
                    <ui-select-value placeholder="工作空间" />
                </ui-select-trigger>
                <ui-select-content>
                    <ui-select-item
                        v-for="workspace in workspaces"
                        :key="workspace.cwd"
                        :value="workspace.cwd">
                        <span
                            class="truncate"
                            :title="workspace.cwd">
                            {{ workspace.title || workspace.cwd }}
                        </span>
                    </ui-select-item>
                </ui-select-content>
            </ui-select-root>
            <form
                class="relative min-w-0 flex-1"
                @submit.prevent="handleSearchThreads">
                <search
                    class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
                    theme="outline"
                    size="15" />
                <ui-input
                    v-model="searchValue"
                    class="h-9 pl-8 pr-3 text-sm"
                    placeholder="搜索会话标题或 ID"
                    type="search" />
            </form>
            <ui-button
                variant="outline"
                size="icon"
                type="button"
                title="刷新会话"
                :disabled="loading"
                @click="emit('refresh')">
                <refresh
                    theme="outline"
                    size="15" />
                <span class="sr-only">刷新会话</span>
            </ui-button>
        </div>

        <ui-scroll-area class="min-h-0">
            <div class="grid gap-1 p-2">
                <template
                    v-for="group in threadGroups"
                    :key="group.parent.id">
                    <ui-accordion
                        v-if="group.children.length"
                        type="multiple"
                        collapsible>
                        <ui-accordion-item
                            :value="group.parent.id"
                            class="overflow-hidden rounded-md border border-border/80 bg-background/45"
                            :class="selectedThreadId === group.parent.id ? 'border-primary/60 bg-primary/10' : ''">
                            <div
                                class="flex min-w-0 items-start gap-2 p-2"
                                @click="emit('select', group.parent.id)">
                                <div class="flex min-w-0 flex-1 items-start gap-2">
                                    <span
                                        class="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border bg-muted/45 text-muted-foreground">
                                        <loading
                                            v-if="isThreadRunning(group.parent.id)"
                                            class="animate-spin"
                                            theme="outline"
                                            size="14" />
                                        <terminal
                                            v-else
                                            theme="outline"
                                            size="14" />
                                    </span>
                                    <div class="grid min-w-0 flex-1 gap-1">
                                        <span class="truncate text-[13px] font-medium leading-5 text-foreground">
                                            {{ group.parent.title || '未命名会话' }}
                                        </span>
                                        <span class="truncate text-[11px] leading-4 text-muted-foreground">
                                            {{ group.parent.id }}
                                        </span>
                                        <div
                                            class="flex min-w-0 items-center gap-2 text-[11px] leading-4 text-muted-foreground">
                                            <span class="truncate">{{ formatTime(group.parent.updatedAt) }}</span>
                                            <ui-accordion-trigger
                                                class="flex-none gap-1 rounded p-0 text-[11px] font-normal leading-none text-primary hover:no-underline focus-visible:ring-1 focus-visible:ring-offset-0 [&>.i-icon-down]:h-3.5 [&>.i-icon-down]:w-3.5">
                                                <span
                                                    class="shrink-0 rounded border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-primary">
                                                    {{ group.children.length }} 个子会话
                                                </span>
                                            </ui-accordion-trigger>
                                        </div>
                                    </div>
                                </div>
                                <ui-button
                                    variant="ghost"
                                    size="icon-sm"
                                    type="button"
                                    title="定位 CodeX 会话"
                                    @click.stop="emit('open', group.parent.id)">
                                    <focus
                                        theme="outline"
                                        size="14" />
                                    <span class="sr-only">定位 CodeX 会话</span>
                                </ui-button>
                            </div>
                            <ui-accordion-content class="border-t border-border/70 data-[state=closed]:hidden">
                                <div class="ml-5 grid gap-1 border-l border-primary/30 py-2 pl-3">
                                    <button
                                        v-for="child in group.children"
                                        :key="child.id"
                                        type="button"
                                        class="group grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-2 rounded-md px-2 py-2 text-left hover:bg-muted/45"
                                        :class="selectedThreadId === child.id ? 'bg-primary/10' : ''"
                                        @click="emit('select', child.id)"
                                        @dblclick="emit('open', child.id)">
                                        <span
                                            class="mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded border border-primary/25 bg-primary/10 text-primary">
                                            <loading
                                                v-if="isThreadRunning(child.id)"
                                                class="animate-spin"
                                                theme="outline"
                                                size="13" />
                                            <branch-one
                                                v-else
                                                theme="outline"
                                                size="13" />
                                        </span>
                                        <span class="grid min-w-0 gap-0.5">
                                            <span class="truncate text-[12px] font-medium leading-5 text-foreground">
                                                {{ child.title || '未命名会话' }}
                                            </span>
                                            <span class="truncate text-[11px] leading-4 text-muted-foreground">{{
                                                child.id
                                            }}</span>
                                            <span
                                                v-if="threadMetaText(child)"
                                                class="truncate text-[11px] leading-4 text-muted-foreground">
                                                {{ threadMetaText(child) }}
                                            </span>
                                        </span>
                                        <focus
                                            class="mt-1 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
                                            theme="outline"
                                            size="13" />
                                    </button>
                                </div>
                            </ui-accordion-content>
                        </ui-accordion-item>
                    </ui-accordion>

                    <button
                        v-else
                        type="button"
                        class="group grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-2 rounded-md border border-border/75 bg-background/45 p-2 text-left hover:bg-muted/45"
                        :class="[
                            group.parent.parentThreadId ? 'border-dashed' : '',
                            selectedThreadId === group.parent.id ? 'border-primary/60 bg-primary/10' : ''
                        ]"
                        @click="emit('select', group.parent.id)"
                        @dblclick="emit('open', group.parent.id)">
                        <span
                            class="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border bg-muted/45 text-muted-foreground">
                            <loading
                                v-if="isThreadRunning(group.parent.id)"
                                class="animate-spin"
                                theme="outline"
                                size="14" />
                            <branch-one
                                v-else-if="group.parent.parentThreadId"
                                theme="outline"
                                size="14" />
                            <terminal
                                v-else
                                theme="outline"
                                size="14" />
                        </span>
                        <span class="grid min-w-0 gap-1">
                            <span class="flex min-w-0 items-center gap-2">
                                <span class="truncate text-[13px] font-medium leading-5 text-foreground">
                                    {{ group.parent.title || '未命名会话' }}
                                </span>
                                <span
                                    v-if="group.parent.parentThreadId"
                                    class="shrink-0 rounded border border-border px-1.5 py-0.5 text-[11px] leading-none text-muted-foreground">
                                    子任务
                                </span>
                            </span>
                            <span class="truncate text-[11px] leading-4 text-muted-foreground">{{
                                group.parent.id
                            }}</span>
                            <span
                                v-if="threadMetaText(group.parent)"
                                class="truncate text-[11px] leading-4 text-muted-foreground">
                                {{ threadMetaText(group.parent) }}
                            </span>
                            <span class="truncate text-[11px] leading-4 text-muted-foreground">
                                {{ formatTime(group.parent.updatedAt) }}
                            </span>
                        </span>
                        <focus
                            class="mt-1 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
                            theme="outline"
                            size="14" />
                    </button>
                </template>

                <div
                    v-if="!codexThreads.length"
                    class="rounded-lg border border-dashed border-border p-8 text-center text-[13px] text-muted-foreground">
                    {{ hasWorkspace ? '暂无会话' : '请先选择工作空间' }}
                </div>
                <div
                    v-if="codexThreads.length"
                    class="flex justify-center py-3">
                    <ui-button
                        v-if="hasMore"
                        variant="outline"
                        size="sm"
                        type="button"
                        :disabled="loadingMore"
                        @click="emit('loadMore')">
                        <loading
                            v-if="loadingMore"
                            theme="outline"
                            size="14" />
                        <down
                            v-else
                            theme="outline"
                            size="14" />
                        <span>{{ loadingMore ? '加载中' : '加载更多' }}</span>
                    </ui-button>
                    <span
                        v-else
                        class="text-[12px] text-muted-foreground">
                        没有更多会话了
                    </span>
                </div>
            </div>
        </ui-scroll-area>
    </aside>
</template>

<script setup lang="ts">
    import { BranchOne, Down, Focus, Loading, Refresh, Search, Terminal } from '@icon-park/vue-next';

    import {
        Accordion as UiAccordion,
        AccordionContent as UiAccordionContent,
        AccordionItem as UiAccordionItem,
        AccordionTrigger as UiAccordionTrigger
    } from '@/components/ui/accordion';
    import { Button as UiButton } from '@/components/ui/button';
    import { Input as UiInput } from '@/components/ui/input';
    import { ScrollArea as UiScrollArea } from '@/components/ui/scroll-area';
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import type { CodexThreadSummaryModel, CodexWorkspaceModel, SessionRecordModel } from '@/model/sessionManage';

    defineOptions({
        name: 'SessionManageSessionList'
    });

    const props = defineProps<{
        // CodeX 工作空间列表，用于在当前会话列表分栏内切换上下文。
        workspaces: CodexWorkspaceModel[];
        // 当前选中的工作空间路径。
        selectedWorkspaceCwd: string;
        // 当前搜索关键词。
        searchKeyword: string;
        // 是否正在刷新会话或工作空间数据。
        loading: boolean;
        // 当前工作空间下 CodeX 原生会话列表。
        codexThreads: CodexThreadSummaryModel[];
        // 当前右侧内容区选中的 CodeX thread ID。
        selectedThreadId: string;
        // 当前任务系统中已经绑定 CodeX thread 的真实会话记录。
        sessions: SessionRecordModel[];
        // 是否已选中 CodeX 工作空间。
        hasWorkspace: boolean;
        // 是否还有更多 CodeX 会话可加载。
        hasMore: boolean;
        // 是否正在追加加载更多会话。
        loadingMore: boolean;
    }>();

    const emit = defineEmits<{
        // 刷新当前会话列表。
        refresh: [];
        // 切换工作空间。
        selectWorkspace: [workspaceCwd: string];
        // 搜索当前工作空间下的会话。
        search: [keyword: string];
        // 追加加载更多会话。
        loadMore: [];
        // 切换右侧会话内容。
        select: [threadId: string];
        // 打开外部 CodeX 会话。
        open: [threadId: string];
    }>();

    const searchValue = ref(props.searchKeyword);

    /** 会话折叠组模型，父会话和当前页内直接子任务在同一个树节点中展示。 */
    interface CodexThreadGroupModel {
        /** 父级或孤立展示的会话。 */
        parent: CodexThreadSummaryModel;
        /** 当前页内归属于 parent 的直接子任务。 */
        children: CodexThreadSummaryModel[];
    }

    // 会话搜索仍按后端分页返回平铺数据，前端仅在当前页内把父子任务合并成树节点。
    const threadGroups = computed<CodexThreadGroupModel[]>(() => buildThreadGroups(props.codexThreads));

    /**
     * 当前选中工作空间名称。
     * 流程：根据选中的 cwd 从工作空间列表中匹配标题，只展示名称，不展示会话数量。
     * 参数：无显式参数，依赖 props.workspaces 与 props.selectedWorkspaceCwd。
     * 返回：工作空间标题或路径兜底。
     * 边界：工作空间尚未加载时返回空字符串，触发 Select 占位文案。
     */
    const selectedWorkspaceTitle = computed<string>(() => {
        const workspace = props.workspaces.find((item) => item.cwd === props.selectedWorkspaceCwd);
        return workspace?.title || workspace?.cwd || '';
    });

    /**
     * 构建执行中 CodeX thread ID 索引。
     * 流程：优先使用 CodeX 会话摘要自身的 running 状态，再合并任务聚合中绑定 externalThreadId 的 running 会话。
     * 参数：无显式参数，依赖 props.codexThreads 与 props.sessions。
     * 返回：执行中的 CodeX thread ID 集合。
     * 边界：未知状态不展示 loading，避免把历史会话误标成执行中。
     */
    const runningThreadIdSet = computed<Set<string>>(() => {
        const threadIdSet = props.codexThreads.reduce<Set<string>>((nextThreadIdSet, thread) => {
            if (thread.status === 'running') nextThreadIdSet.add(thread.id);
            return nextThreadIdSet;
        }, new Set<string>());
        return props.sessions.reduce<Set<string>>((nextThreadIdSet, session) => {
            if (session.status === 'running' && session.externalThreadId) nextThreadIdSet.add(session.externalThreadId);
            return nextThreadIdSet;
        }, threadIdSet);
    });

    // 需要监听父级关键词变化：切换工作空间时 Store 会清空搜索词，输入框必须同步清空但不触发请求。
    watch(
        () => props.searchKeyword,
        (keyword) => {
            searchValue.value = keyword;
        }
    );

    /**
     * 切换工作空间。
     * 流程：Select 组件可能返回字符串或数组，这里只接受单个工作空间路径并向父级抛出。
     * 参数：value 为 Select 组件返回值。
     * 返回：无返回值。
     * 边界：空值或非字符串值直接忽略，避免误清空当前工作空间。
     */
    function handleSelectWorkspace(value: string | string[]): void {
        if (typeof value !== 'string' || !value) return;
        emit('selectWorkspace', value);
    }

    /**
     * 提交会话搜索。
     * 流程：读取输入框当前关键词并通知父组件刷新第一页搜索结果。
     * 参数：无。
     * 返回：无返回值。
     * 边界：空关键词由父级按默认列表处理。
     */
    function handleSearchThreads(): void {
        emit('search', String(searchValue.value));
    }

    /**
     * 构建当前页会话树节点。
     * 流程：按 parentThreadId 建立直接子任务索引；父级在当前页时把子任务合并到同一个树节点；父级不在当前页的子任务按普通孤立节点展示。
     * 参数：threads 为后端当前页返回的真实会话摘要。
     * 返回：父级节点与子任务数组组成的树节点列表。
     * 边界：只在当前分页内组装，不跨页补查父级，避免改变后端分页语义。
     */
    function buildThreadGroups(threads: CodexThreadSummaryModel[]): CodexThreadGroupModel[] {
        const threadIdSet = new Set(threads.map((thread) => thread.id));
        const childMap = new Map<string, CodexThreadSummaryModel[]>();
        const groups: CodexThreadGroupModel[] = [];

        for (const thread of threads) {
            if (!thread.parentThreadId) continue;
            const children = childMap.get(thread.parentThreadId) ?? [];
            children.push(thread);
            childMap.set(thread.parentThreadId, children);
        }

        for (const thread of threads) {
            if (thread.parentThreadId && threadIdSet.has(thread.parentThreadId)) continue;
            groups.push({
                parent: thread,
                children: childMap.get(thread.id) ?? []
            });
        }

        return groups;
    }

    /**
     * 生成子任务辅助信息。
     * 流程：优先展示 Agent 昵称和角色；父会话不在当前页时追加父级 thread ID，方便定位来源。
     * 参数：thread 为当前展示的子任务会话。
     * 返回：用于列表次要信息的文本；普通会话返回空字符串。
     * 边界：缺少子任务元数据时只展示可确认字段，不编造名称或角色。
     */
    function threadMetaText(thread: CodexThreadSummaryModel): string {
        if (!thread.parentThreadId) return '';
        const agentText = [thread.agentNickname, thread.agentRole].filter(Boolean).join(' / ');
        const parentText = `父会话 ${thread.parentThreadId}`;
        return agentText ? `${agentText} · ${parentText}` : parentText;
    }

    /**
     * 判断当前 CodeX 会话是否正在执行。
     * 流程：优先直接读取当前 props.codexThreads 中的状态，再用 running 会话索引兜底查询。
     * 参数：threadId 为 CodeX 会话列表中的真实 thread ID。
     * 返回：处于执行中时为 true。
     * 边界：只展示后端已确认的 running，不根据更新时间或标题猜测状态。
     */
    function isThreadRunning(threadId: string): boolean {
        return (
            props.codexThreads.some((thread) => thread.id === threadId && thread.status === 'running') ||
            runningThreadIdSet.value.has(threadId)
        );
    }

    /**
     * 格式化时间用于树节点次要信息展示。
     * 流程：兼容毫秒时间戳字符串和 SQLite 时间字符串，转换失败时返回原值。
     * 参数：value 为后端返回的更新时间。
     * 返回：适合页面展示的短日期时间。
     * 边界：空值返回占位横线。
     */
    function formatTime(value: string): string {
        if (!value) return '-';
        const timestamp = Number(value);
        const date = Number.isFinite(timestamp) && timestamp > 0 ? new Date(timestamp) : new Date(value);
        if (Number.isNaN(date.getTime())) return value;
        return date.toLocaleString();
    }
</script>
