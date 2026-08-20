<template>
    <div class="grid w-full gap-5">
        <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="flex items-center gap-2 text-[13px] text-muted-foreground">
                <button
                    class="font-medium text-foreground"
                    type="button"
                    @click="handleBackVoicePolish">
                    语音转文字润色
                </button>
                <span>/</span>
                <span>词典列表</span>
            </div>
            <ui-button
                variant="outline"
                type="button"
                @click="dictionaryDialogOpen = true">
                <plus
                    theme="outline"
                    size="16" />
                <span>添加</span>
            </ui-button>
        </div>

        <section class="grid gap-3">
            <ui-alert
                v-for="item in store.dictionary"
                :key="item.word"
                class="flex items-center justify-between gap-4 p-4">
                <div class="min-w-0">
                    <div class="truncate text-[14px] font-semibold text-foreground">{{ item.word }}</div>
                    <div class="mt-1 text-[12px] text-muted-foreground">
                        {{ new Date(item.createdAt).toLocaleString() }}
                    </div>
                </div>
                <ui-button
                    variant="outline"
                    size="sm"
                    type="button"
                    @click="store.removeDictionaryWord(item.word)">
                    删除
                </ui-button>
            </ui-alert>
            <ui-page-state
                v-if="!store.dictionary.length"
                :icon="Book"
                title="还没有词典词条"
                description="词典用于提前告诉语音润色需要重点保留的专有名词、产品名、人名或常用术语。添加后，语音转文字和文本润色会优先参考这些词条，减少同音误写，让输出更贴近你的工作语境。">
                <template #action>
                    <ui-button
                        type="button"
                        @click="dictionaryDialogOpen = true">
                        <plus
                            theme="outline"
                            size="16" />
                        <span>添加词条</span>
                    </ui-button>
                </template>
            </ui-page-state>
        </section>

        <voice-polish-dictionary-dialog
            v-model:open="dictionaryDialogOpen"
            @submit="handleAddWords" />
    </div>
</template>

<script setup lang="ts">
    import { Book, Plus } from '@icon-park/vue-next';

    import { Alert as UiAlert } from '@/components/ui/alert';
    import { Button as UiButton } from '@/components/ui/button';
    import { PageState as UiPageState } from '@/components/ui/pageState';
    import VoicePolishDictionaryDialog from '@/components/voicePolish/dictionaryDialog.vue';
    import { HubRouteName } from '@/router';
    import { useVoicePolishStore } from '@/stores/voicePolish';

    defineOptions({
        name: 'VoicePolishDictionaryListView'
    });

    const router = useRouter();
    const store = useVoicePolishStore();
    const dictionaryDialogOpen = ref(false);

    /**
     * 返回语音转文字润色历史页。
     * 流程：通过命名路由回到同模块首页，保持侧边栏选中态和 URL 同步。
     * 参数：无。
     * 返回：无返回值。
     * 边界：如果当前路由已变化，仍以命名路由为准回到模块首页。
     */
    function handleBackVoicePolish(): void {
        void router.push({ name: HubRouteName.VoicePolish });
    }

    /**
     * 添加词典词条。
     * 流程：复用语音润色 Store 的词条拆分、去重和持久化能力。
     * 参数：input 为弹窗提交的原始多词条文本。
     * 返回：无返回值。
     * 边界：空文本由弹窗侧拦截，重复词条由 Store 去重。
     */
    function handleAddWords(input: string): void {
        store.addDictionaryWords(input);
    }
</script>
