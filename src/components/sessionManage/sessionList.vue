<template>
    <section class="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-3">
        <div class="flex items-center justify-between gap-2">
            <form
                class="relative min-w-0 flex-1"
                @submit.prevent="handleSearchThreads">
                <search
                    class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
                    theme="outline"
                    size="15" />
                <ui-input
                    v-model="searchValue"
                    class="h-8 pl-8 pr-3"
                    placeholder="搜索会话标题或 ID"
                    type="search" />
            </form>
            <ui-button
                variant="outline"
                size="sm"
                type="button"
                :disabled="loading"
                @click="emit('refresh')">
                <refresh
                    theme="outline"
                    size="15" />
                <span>刷新</span>
            </ui-button>
        </div>

        <ui-scroll-area class="min-h-0">
            <div class="pr-3">
                <ui-item-group>
                    <ui-item
                        v-for="thread in displayThreads"
                        :key="thread.id"
                        variant="outline"
                        :class="[
                            thread.level > 0 ? 'border-dashed bg-muted/30' : 'bg-card/70',
                            threadDepthClass(thread.level)
                        ]">
                        <ui-item-media
                            variant="icon"
                            :class="thread.level > 0 ? 'border-dashed' : ''">
                            <branch-one
                                v-if="thread.level > 0"
                                theme="outline"
                                size="15" />
                            <terminal
                                v-else
                                theme="outline"
                                size="15" />
                        </ui-item-media>
                        <ui-item-content>
                            <ui-item-title>
                                <span class="flex min-w-0 items-center gap-2">
                                    <span class="truncate">{{ thread.title || '未命名会话' }}</span>
                                    <span
                                        v-if="thread.level > 0"
                                        class="shrink-0 rounded border border-border px-1.5 py-0.5 text-[11px] font-normal leading-none text-muted-foreground">
                                        子任务
                                    </span>
                                </span>
                            </ui-item-title>
                            <ui-item-description class="truncate">{{ thread.id }}</ui-item-description>
                            <ui-item-description
                                v-if="threadMetaText(thread)"
                                class="truncate">
                                {{ threadMetaText(thread) }}
                            </ui-item-description>
                            <ui-item-footer>{{ formatTime(thread.updatedAt) }}</ui-item-footer>
                        </ui-item-content>
                        <ui-item-actions>
                            <ui-button
                                variant="ghost"
                                size="icon-sm"
                                type="button"
                                @click="emit('open', thread.id)">
                                <focus
                                    theme="outline"
                                    size="15" />
                                <span class="sr-only">定位 CodeX 会话</span>
                            </ui-button>
                        </ui-item-actions>
                    </ui-item>
                </ui-item-group>
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
                        class="text-[12px] text-muted-foreground"
                        >没有更多会话了</span
                    >
                </div>
            </div>
        </ui-scroll-area>
    </section>
</template>

<script setup lang="ts">
    import { BranchOne, Down, Focus, Loading, Refresh, Search, Terminal } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import { Input as UiInput } from '@/components/ui/input';
    import {
        Item as UiItem,
        ItemActions as UiItemActions,
        ItemContent as UiItemContent,
        ItemDescription as UiItemDescription,
        ItemFooter as UiItemFooter,
        ItemGroup as UiItemGroup,
        ItemMedia as UiItemMedia,
        ItemTitle as UiItemTitle
    } from '@/components/ui/item';
    import { ScrollArea as UiScrollArea } from '@/components/ui/scroll-area';
    import type { CodexThreadSummaryModel } from '@/model/sessionManage';

    defineOptions({
        name: 'SessionManageSessionList'
    });

    const props = defineProps<{
        // 当前工作空间下 CodeX 原生会话列表。
        codexThreads: CodexThreadSummaryModel[];
        // 是否已选中 CodeX 工作空间。
        hasWorkspace: boolean;
        // 是否还有更多 CodeX 会话可加载。
        hasMore: boolean;
        // 是否正在刷新数据。
        loading: boolean;
        // 是否正在追加加载更多会话。
        loadingMore: boolean;
        // 当前搜索关键词。
        searchKeyword: string;
    }>();

    const emit = defineEmits<{
        // 刷新会话列表。
        refresh: [];
        // 搜索会话列表。
        search: [keyword: string];
        // 追加加载更多会话。
        loadMore: [];
        // 打开外部 CodeX 会话。
        open: [threadId: string];
    }>();

    const searchValue = ref(props.searchKeyword);

    /** 会话树形展示行，保留原始 thread 字段并补充当前页内展示层级。 */
    interface CodexThreadDisplayItemModel extends CodexThreadSummaryModel {
        /** 当前页内用于缩进的展示层级，父级不在当前页时沿用 CodeX 原始 depth。 */
        level: number;
    }

    // 会话搜索仍按后端分页返回平铺数据，前端仅在当前页内把父子任务排成轻量树。
    const displayThreads = computed<CodexThreadDisplayItemModel[]>(() => buildThreadDisplayRows(props.codexThreads));

    // 需要监听父级关键词变化：切换工作空间时 Store 会清空搜索词，输入框必须同步清空但不触发请求。
    watch(
        () => props.searchKeyword,
        (keyword) => {
            searchValue.value = keyword;
        }
    );

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
     * 构建当前页会话展示树。
     * 流程：按 parentThreadId 建立当前页子节点索引，再从根会话开始深度追加；父会话不在当前页的子任务按原顺序追加。
     * 参数：threads 为后端当前页返回的真实会话摘要。
     * 返回：带展示层级的会话行数组。
     * 边界：只在当前分页内重排，不跨页补查父级，避免改变后端分页语义。
     */
    function buildThreadDisplayRows(threads: CodexThreadSummaryModel[]): CodexThreadDisplayItemModel[] {
        const threadIdSet = new Set(threads.map((thread) => thread.id));
        const childMap = new Map<string, CodexThreadSummaryModel[]>();
        const displayRows: CodexThreadDisplayItemModel[] = [];
        const appendedIds = new Set<string>();

        for (const thread of threads) {
            if (!thread.parentThreadId) continue;
            const children = childMap.get(thread.parentThreadId) ?? [];
            children.push(thread);
            childMap.set(thread.parentThreadId, children);
        }

        for (const thread of threads) {
            if (thread.parentThreadId && threadIdSet.has(thread.parentThreadId)) continue;
            appendThreadWithChildren(
                thread,
                thread.parentThreadId ? Math.max(thread.depth, 1) : 0,
                childMap,
                displayRows,
                appendedIds
            );
        }

        return displayRows;
    }

    /**
     * 递归追加会话和当前页内子任务。
     * 流程：先追加当前会话，再按接口原始顺序追加它的直接子任务，层级逐层加一。
     * 参数：thread 为当前会话；level 为展示层级；childMap 为父子索引；displayRows 为输出数组；appendedIds 防止异常环形数据重复展示。
     * 返回：无返回值。
     * 边界：CodeX 异常返回循环父子关系时跳过已追加节点，保证页面不会死循环。
     */
    function appendThreadWithChildren(
        thread: CodexThreadSummaryModel,
        level: number,
        childMap: Map<string, CodexThreadSummaryModel[]>,
        displayRows: CodexThreadDisplayItemModel[],
        appendedIds: Set<string>
    ): void {
        if (appendedIds.has(thread.id)) return;
        appendedIds.add(thread.id);
        displayRows.push({ ...thread, level: Math.min(level, 2) });
        for (const child of childMap.get(thread.id) ?? []) {
            appendThreadWithChildren(child, level + 1, childMap, displayRows, appendedIds);
        }
    }

    /**
     * 生成会话层级缩进类名。
     * 流程：把当前页内展示层级映射为固定 Tailwind padding，避免动态类名被构建裁剪。
     * 参数：level 为展示层级。
     * 返回：列表项左侧缩进类名。
     * 边界：超过二级的子任务按二级缩进展示，避免窄屏过度挤压。
     */
    function threadDepthClass(level: number): string {
        if (level <= 0) return '';
        if (level === 1) return 'ml-5';
        return 'ml-10';
    }

    /**
     * 生成子任务辅助信息。
     * 流程：优先展示 Agent 昵称和角色；父会话不在当前页时追加父级 thread ID，方便定位来源。
     * 参数：thread 为当前展示行。
     * 返回：用于列表次要信息的文本；普通会话返回空字符串。
     * 边界：缺少子任务元数据时只展示可确认字段，不编造名称或角色。
     */
    function threadMetaText(thread: CodexThreadDisplayItemModel): string {
        if (!thread.parentThreadId) return '';
        const agentText = [thread.agentNickname, thread.agentRole].filter(Boolean).join(' / ');
        const parentText = `父会话 ${thread.parentThreadId}`;
        return agentText ? `${agentText} · ${parentText}` : parentText;
    }

    /**
     * 格式化时间用于卡片次要信息展示。
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
