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
                        v-for="thread in codexThreads"
                        :key="thread.id"
                        variant="outline"
                        class="bg-card/70">
                        <ui-item-media variant="icon">
                            <terminal
                                theme="outline"
                                size="15" />
                        </ui-item-media>
                        <ui-item-content>
                            <ui-item-title>{{ thread.title || '未命名会话' }}</ui-item-title>
                            <ui-item-description class="truncate">{{ thread.id }}</ui-item-description>
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
    import { Down, Focus, Loading, Refresh, Search, Terminal } from '@icon-park/vue-next';

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
