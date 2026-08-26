<template>
    <section class="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] border-l border-border bg-background">
        <header class="flex min-w-0 items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div class="grid min-w-0 gap-0.5">
                <h2 class="truncate text-[14px] font-semibold text-foreground">
                    {{ selectedThread?.title || '请选择会话' }}
                </h2>
                <p class="truncate text-[11px] leading-4 text-muted-foreground">
                    {{ selectedThread?.id || '从左侧选择一个会话后查看内容' }}
                </p>
            </div>
            <div class="flex shrink-0 items-center gap-2">
                <span
                    v-if="messageRangeText"
                    class="hidden rounded border border-border bg-muted/35 px-2 py-1 text-[11px] leading-none text-muted-foreground xl:inline-flex">
                    {{ messageRangeText }}
                </span>
                <span
                    class="rounded-full border px-2.5 py-1 text-[11px] leading-none"
                    :class="connectionBadgeClass">
                    {{ connectionText }}
                </span>
            </div>
        </header>

        <div
            ref="scrollContainerRef"
            class="relative min-h-0 overflow-y-auto bg-[#111113] px-5 py-4 text-[#f4f4f5]"
            @scroll="handleMessageScroll">
            <div
                v-if="!selectedThread"
                class="grid h-full min-h-[360px] place-items-center text-center">
                <div class="grid max-w-sm justify-items-center gap-3">
                    <div
                        class="grid h-12 w-12 place-items-center rounded-xl border border-white/10 bg-white/[0.04] text-white/55">
                        <doc-detail
                            theme="outline"
                            size="20" />
                    </div>
                    <div class="grid gap-1">
                        <h3 class="text-[14px] font-medium text-white/90">会话内容区域</h3>
                        <p class="text-[13px] leading-5 text-white/50">
                            左侧切换会话后，这里会读取最近消息并持续接收会话流。
                        </p>
                    </div>
                </div>
            </div>

            <div
                v-else-if="loading && !messages.length"
                class="flex h-full min-h-[360px] items-center justify-center gap-2 text-[13px] text-white/50">
                <loading
                    class="animate-spin"
                    theme="outline"
                    size="16" />
                <span>读取会话内容中</span>
            </div>

            <div
                v-else-if="errorMessage"
                class="grid h-full min-h-[360px] place-items-center">
                <div class="grid max-w-md gap-2 rounded-xl border border-red-500/25 bg-red-500/10 p-4">
                    <h3 class="text-[14px] font-medium text-red-200">会话内容读取失败</h3>
                    <p class="text-[13px] leading-5 text-white/60">{{ errorMessage }}</p>
                    <ui-button
                        class="w-fit border-white/10 bg-white/[0.03] text-white hover:bg-white/[0.08]"
                        variant="outline"
                        size="sm"
                        type="button"
                        @click="handleRetry">
                        <refresh
                            theme="outline"
                            size="14" />
                        <span>重试</span>
                    </ui-button>
                </div>
            </div>

            <div
                v-else-if="!messages.length"
                class="grid h-full min-h-[360px] place-items-center text-[13px] text-white/45">
                当前会话暂无可展示消息
            </div>

            <div
                v-else
                class="mx-auto grid w-full max-w-[860px] gap-5 pb-10">
                <div
                    v-if="currentRuntime?.isLoadingMoreBefore"
                    class="mx-auto inline-flex items-center gap-2 rounded-full border border-white/10 bg-[#1f1f23]/95 px-3 py-1.5 text-[12px] text-white/55 shadow-lg shadow-black/20">
                    <loading
                        class="animate-spin"
                        theme="outline"
                        size="13" />
                    <span>加载更早消息中</span>
                </div>
                <div
                    v-if="loading"
                    class="sticky top-0 z-10 mx-auto mb-2 inline-flex items-center gap-2 rounded-full border border-white/10 bg-[#1f1f23]/95 px-3 py-1.5 text-[12px] text-white/60 shadow-lg shadow-black/20">
                    <loading
                        class="animate-spin"
                        theme="outline"
                        size="13" />
                    <span>同步最新内容中</span>
                </div>
                <article
                    v-for="message in renderedTurns"
                    :key="message.sourceKey"
                    class="grid min-w-0"
                    :class="message.source.role === 'user' ? 'justify-items-end' : 'justify-items-stretch'">
                    <div
                        v-if="message.source.role === 'user'"
                        class="max-w-[min(720px,82%)] rounded-[18px] bg-[#2d2d31] px-4 py-2.5 text-[14px] leading-6 text-white shadow-sm">
                        <template
                            v-for="(block, blockIndex) in message.blocks"
                            :key="`${message.sourceKey}-user-${blockIndex}`">
                            <p
                                v-if="block.type === 'paragraph'"
                                class="whitespace-pre-wrap break-words">
                                <template
                                    v-for="(segment, segmentIndex) in block.segments"
                                    :key="`${message.sourceKey}-user-${blockIndex}-${segmentIndex}`">
                                    <img
                                        v-if="segment.type === 'image' && segment.previewSrc"
                                        class="my-2 max-h-[260px] max-w-full rounded-lg border border-white/10 object-contain"
                                        :src="segment.previewSrc"
                                        :alt="segment.content || '会话图片'"
                                        :title="segment.title || segment.href" />
                                    <span
                                        v-else-if="segment.type === 'image'"
                                        class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.05] px-1.5 py-0.5 text-[12px] leading-5 text-white/70"
                                        :title="segment.title || segment.href">
                                        {{ segment.content || compactLinkText(segment.href) }}
                                    </span>
                                    <a
                                        v-else-if="segment.type === 'link' && isNavigableHref(segment.href)"
                                        class="font-medium text-sky-300 underline decoration-sky-300/35 underline-offset-4 hover:text-sky-200"
                                        :href="segment.href"
                                        :title="segment.title || segment.href"
                                        rel="noreferrer"
                                        target="_blank">
                                        {{ segment.content || compactLinkText(segment.href) }}
                                    </a>
                                    <span
                                        v-else-if="segment.type === 'link' && !isOpenableLocalFileHref(segment.href)"
                                        class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.05] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-white/75"
                                        :title="segment.title || segment.href">
                                        {{ segment.content || compactLinkText(segment.href) }}
                                    </span>
                                    <button
                                        v-else-if="segment.type === 'link'"
                                        class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.05] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-sky-200 hover:bg-white/[0.09] hover:text-sky-100"
                                        type="button"
                                        :title="`用系统默认应用打开：${segment.title || segment.href}`"
                                        @click="handleOpenLocalFile(segment.href)">
                                        {{ segment.content || compactLinkText(segment.href) }}
                                    </button>
                                    <code
                                        v-else-if="segment.type === 'code'"
                                        class="rounded bg-white/10 px-1 py-0.5 font-mono text-[12px] text-white/90"
                                        >{{ segment.content }}</code
                                    >
                                    <strong
                                        v-else-if="segment.type === 'strong'"
                                        class="font-semibold text-white"
                                        >{{ segment.content }}</strong
                                    >
                                    <span v-else>{{ segment.content }}</span>
                                </template>
                            </p>
                            <pre
                                v-else-if="block.type === 'code'"
                                class="mt-2 max-h-[360px] overflow-auto rounded-lg border border-white/10 bg-black/35 p-3 font-mono text-[12px] leading-5 text-white/80"><code>{{ block.content }}</code></pre>
                            <img
                                v-else-if="block.type === 'image' && block.previewSrc"
                                class="mt-2 max-h-[260px] max-w-full rounded-lg border border-white/10 object-contain"
                                :src="block.previewSrc"
                                :alt="block.alt || '会话图片'"
                                :title="block.title || block.href" />
                            <ul
                                v-else-if="block.type === 'list'"
                                class="mt-1 grid gap-1 pl-5">
                                <li
                                    v-for="(item, itemIndex) in block.items"
                                    :key="`${message.sourceKey}-user-li-${blockIndex}-${itemIndex}`"
                                    class="list-disc break-words pl-1">
                                    <template
                                        v-for="(segment, segmentIndex) in item"
                                        :key="`${message.sourceKey}-user-li-${blockIndex}-${itemIndex}-${segmentIndex}`">
                                        <img
                                            v-if="segment.type === 'image' && segment.previewSrc"
                                            class="my-2 max-h-[260px] max-w-full rounded-lg border border-white/10 object-contain"
                                            :src="segment.previewSrc"
                                            :alt="segment.content || '会话图片'"
                                            :title="segment.title || segment.href" />
                                        <span
                                            v-else-if="segment.type === 'image'"
                                            class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.05] px-1.5 py-0.5 text-[12px] leading-5 text-white/70"
                                            :title="segment.title || segment.href">
                                            {{ segment.content || compactLinkText(segment.href) }}
                                        </span>
                                        <a
                                            v-else-if="segment.type === 'link' && isNavigableHref(segment.href)"
                                            class="font-medium text-sky-300 underline decoration-sky-300/35 underline-offset-4 hover:text-sky-200"
                                            :href="segment.href"
                                            :title="segment.title || segment.href"
                                            rel="noreferrer"
                                            target="_blank">
                                            {{ segment.content || compactLinkText(segment.href) }}
                                        </a>
                                        <span
                                            v-else-if="
                                                segment.type === 'link' && !isOpenableLocalFileHref(segment.href)
                                            "
                                            class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.05] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-white/75"
                                            :title="segment.title || segment.href">
                                            {{ segment.content || compactLinkText(segment.href) }}
                                        </span>
                                        <button
                                            v-else-if="segment.type === 'link'"
                                            class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.05] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-sky-200 hover:bg-white/[0.09] hover:text-sky-100"
                                            type="button"
                                            :title="`用系统默认应用打开：${segment.title || segment.href}`"
                                            @click="handleOpenLocalFile(segment.href)">
                                            {{ segment.content || compactLinkText(segment.href) }}
                                        </button>
                                        <code
                                            v-else-if="segment.type === 'code'"
                                            class="rounded bg-white/10 px-1 py-0.5 font-mono text-[12px] text-white/90"
                                            >{{ segment.content }}</code
                                        >
                                        <strong
                                            v-else-if="segment.type === 'strong'"
                                            class="font-semibold text-white"
                                            >{{ segment.content }}</strong
                                        >
                                        <span v-else>{{ segment.content }}</span>
                                    </template>
                                </li>
                            </ul>
                            <p
                                v-else-if="block.type === 'heading' || block.type === 'quote' || block.type === 'tool'"
                                class="whitespace-pre-wrap break-words">
                                {{ block.content }}
                            </p>
                        </template>
                    </div>

                    <div
                        v-else
                        class="grid w-full min-w-0 grid-cols-[28px_minmax(0,1fr)] gap-3">
                        <div
                            class="mt-1 grid h-7 w-7 place-items-center rounded-md border border-white/10 bg-white/[0.04] text-white/55">
                            <terminal
                                theme="outline"
                                size="14" />
                        </div>
                        <div class="grid min-w-0 gap-3">
                            <div class="flex min-w-0 items-center gap-2 text-[12px] leading-5 text-white/45">
                                <span class="font-medium text-white/65">Codex</span>
                                <span v-if="message.timeText">{{ message.timeText }}</span>
                            </div>
                            <div class="grid min-w-0 gap-3 text-[14px] leading-7 text-[#f4f4f5]">
                                <template
                                    v-for="(block, blockIndex) in message.blocks"
                                    :key="`${message.sourceKey}-assistant-${blockIndex}`">
                                    <h3
                                        v-if="block.type === 'heading'"
                                        class="mt-1 text-[15px] font-semibold leading-7 text-white">
                                        {{ block.content }}
                                    </h3>

                                    <p
                                        v-else-if="block.type === 'paragraph'"
                                        class="whitespace-pre-wrap break-words">
                                        <template
                                            v-for="(segment, segmentIndex) in block.segments"
                                            :key="`${message.sourceKey}-p-${blockIndex}-${segmentIndex}`">
                                            <img
                                                v-if="segment.type === 'image' && segment.previewSrc"
                                                class="my-2 max-h-[360px] max-w-full rounded-lg border border-white/10 object-contain"
                                                :src="segment.previewSrc"
                                                :alt="segment.content || '会话图片'"
                                                :title="segment.title || segment.href" />
                                            <span
                                                v-else-if="segment.type === 'image'"
                                                class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.04] px-2 py-1 text-[12px] leading-5 text-white/65"
                                                :title="segment.title || segment.href">
                                                {{ segment.content || compactLinkText(segment.href) }}
                                            </span>
                                            <a
                                                v-else-if="segment.type === 'link' && isNavigableHref(segment.href)"
                                                class="font-medium text-sky-300 underline decoration-sky-300/35 underline-offset-4 hover:text-sky-200"
                                                :href="segment.href"
                                                :title="segment.title || segment.href"
                                                rel="noreferrer"
                                                target="_blank">
                                                {{ segment.content || compactLinkText(segment.href) }}
                                            </a>
                                            <span
                                                v-else-if="
                                                    segment.type === 'link' && !isOpenableLocalFileHref(segment.href)
                                                "
                                                class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.04] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-white/72"
                                                :title="segment.title || segment.href">
                                                {{ segment.content || compactLinkText(segment.href) }}
                                            </span>
                                            <button
                                                v-else-if="segment.type === 'link'"
                                                class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.04] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-sky-200 hover:bg-white/[0.08] hover:text-sky-100"
                                                type="button"
                                                :title="`用系统默认应用打开：${segment.title || segment.href}`"
                                                @click="handleOpenLocalFile(segment.href)">
                                                {{ segment.content || compactLinkText(segment.href) }}
                                            </button>
                                            <code
                                                v-else-if="segment.type === 'code'"
                                                class="rounded-md bg-white/10 px-1.5 py-0.5 font-mono text-[12px] leading-none text-white/90"
                                                >{{ segment.content }}</code
                                            >
                                            <strong
                                                v-else-if="segment.type === 'strong'"
                                                class="font-semibold text-white"
                                                >{{ segment.content }}</strong
                                            >
                                            <span v-else>{{ segment.content }}</span>
                                        </template>
                                    </p>

                                    <ul
                                        v-else-if="block.type === 'list'"
                                        class="grid gap-1.5 pl-5">
                                        <li
                                            v-for="(item, itemIndex) in block.items"
                                            :key="`${message.sourceKey}-li-${blockIndex}-${itemIndex}`"
                                            class="list-disc break-words pl-1">
                                            <template
                                                v-for="(segment, segmentIndex) in item"
                                                :key="`${message.sourceKey}-li-${blockIndex}-${itemIndex}-${segmentIndex}`">
                                                <img
                                                    v-if="segment.type === 'image' && segment.previewSrc"
                                                    class="my-2 max-h-[320px] max-w-full rounded-lg border border-white/10 object-contain"
                                                    :src="segment.previewSrc"
                                                    :alt="segment.content || '会话图片'"
                                                    :title="segment.title || segment.href" />
                                                <span
                                                    v-else-if="segment.type === 'image'"
                                                    class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.04] px-2 py-1 text-[12px] leading-5 text-white/65"
                                                    :title="segment.title || segment.href">
                                                    {{ segment.content || compactLinkText(segment.href) }}
                                                </span>
                                                <a
                                                    v-else-if="segment.type === 'link' && isNavigableHref(segment.href)"
                                                    class="font-medium text-sky-300 underline decoration-sky-300/35 underline-offset-4 hover:text-sky-200"
                                                    :href="segment.href"
                                                    :title="segment.title || segment.href"
                                                    rel="noreferrer"
                                                    target="_blank">
                                                    {{ segment.content || compactLinkText(segment.href) }}
                                                </a>
                                                <span
                                                    v-else-if="
                                                        segment.type === 'link' &&
                                                        !isOpenableLocalFileHref(segment.href)
                                                    "
                                                    class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.04] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-white/72"
                                                    :title="segment.title || segment.href">
                                                    {{ segment.content || compactLinkText(segment.href) }}
                                                </span>
                                                <button
                                                    v-else-if="segment.type === 'link'"
                                                    class="inline-flex max-w-full items-center rounded-md border border-white/10 bg-white/[0.04] px-1.5 py-0.5 font-mono text-[12px] leading-5 text-sky-200 hover:bg-white/[0.08] hover:text-sky-100"
                                                    type="button"
                                                    :title="`用系统默认应用打开：${segment.title || segment.href}`"
                                                    @click="handleOpenLocalFile(segment.href)">
                                                    {{ segment.content || compactLinkText(segment.href) }}
                                                </button>
                                                <code
                                                    v-else-if="segment.type === 'code'"
                                                    class="rounded-md bg-white/10 px-1.5 py-0.5 font-mono text-[12px] leading-none text-white/90"
                                                    >{{ segment.content }}</code
                                                >
                                                <strong
                                                    v-else-if="segment.type === 'strong'"
                                                    class="font-semibold text-white"
                                                    >{{ segment.content }}</strong
                                                >
                                                <span v-else>{{ segment.content }}</span>
                                            </template>
                                        </li>
                                    </ul>

                                    <blockquote
                                        v-else-if="block.type === 'quote'"
                                        class="border-l-2 border-white/15 pl-3 text-[13px] leading-6 text-white/60">
                                        {{ block.content }}
                                    </blockquote>

                                    <div
                                        v-else-if="block.type === 'processGroup'"
                                        class="grid gap-2">
                                        <button
                                            type="button"
                                            class="inline-flex w-fit items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.035] px-2.5 py-1.5 text-[12px] leading-none text-white/62 hover:bg-white/[0.07] hover:text-white/82"
                                            @click="handleToggleProcessGroup(message.sourceKey, blockIndex)">
                                            <span>{{ block.title }}</span>
                                            <down
                                                class="transition-transform"
                                                :class="
                                                    isProcessGroupExpanded(message.sourceKey, blockIndex)
                                                        ? 'rotate-180'
                                                        : ''
                                                "
                                                theme="outline"
                                                size="12" />
                                        </button>

                                        <div
                                            v-if="isProcessGroupExpanded(message.sourceKey, blockIndex)"
                                            class="grid gap-2 pl-0.5">
                                            <div
                                                v-for="(item, itemIndex) in block.items"
                                                :key="`${message.sourceKey}-process-${blockIndex}-${itemIndex}`"
                                                class="grid min-w-0 gap-1">
                                                <button
                                                    type="button"
                                                    class="group flex min-w-0 items-center gap-2 rounded-md px-1 py-1 text-left text-[13px] leading-5 text-white/58 hover:bg-white/[0.04] hover:text-white/78"
                                                    :class="item.content ? '' : 'cursor-default hover:bg-transparent'"
                                                    @click="
                                                        item.content
                                                            ? handleToggleProcessItem(
                                                                  message.sourceKey,
                                                                  blockIndex,
                                                                  itemIndex
                                                              )
                                                            : undefined
                                                    ">
                                                    <doc-detail
                                                        v-if="item.icon === 'file'"
                                                        class="shrink-0 text-white/48"
                                                        theme="outline"
                                                        size="14" />
                                                    <terminal
                                                        v-else-if="item.icon === 'command'"
                                                        class="shrink-0 text-white/48"
                                                        theme="outline"
                                                        size="14" />
                                                    <refresh
                                                        v-else-if="item.icon === 'browser'"
                                                        class="shrink-0 text-white/48"
                                                        theme="outline"
                                                        size="14" />
                                                    <check
                                                        v-else-if="item.icon === 'edit'"
                                                        class="shrink-0 text-white/48"
                                                        theme="outline"
                                                        size="14" />
                                                    <command
                                                        v-else
                                                        class="shrink-0 text-white/48"
                                                        theme="outline"
                                                        size="14" />
                                                    <span class="min-w-0 flex-1 truncate">{{ item.title }}</span>
                                                    <span
                                                        v-if="item.statusText"
                                                        class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] leading-none"
                                                        :class="item.statusClass">
                                                        {{ item.statusText }}
                                                    </span>
                                                    <down
                                                        v-if="item.content"
                                                        class="shrink-0 transition-transform"
                                                        :class="
                                                            isProcessItemExpanded(
                                                                message.sourceKey,
                                                                blockIndex,
                                                                itemIndex
                                                            )
                                                                ? 'rotate-180'
                                                                : ''
                                                        "
                                                        theme="outline"
                                                        size="12" />
                                                </button>
                                                <pre
                                                    v-if="
                                                        item.content &&
                                                        isProcessItemExpanded(message.sourceKey, blockIndex, itemIndex)
                                                    "
                                                    class="ml-6 max-h-[360px] overflow-auto rounded-lg border border-white/10 bg-black/25 p-3 font-mono text-[12px] leading-5 text-white/68"><code>{{ item.content }}</code></pre>
                                            </div>
                                        </div>
                                    </div>

                                    <img
                                        v-else-if="block.type === 'image' && block.previewSrc"
                                        class="max-h-[420px] max-w-full rounded-lg border border-white/10 object-contain"
                                        :src="block.previewSrc"
                                        :alt="block.alt || '会话图片'"
                                        :title="block.title || block.href" />

                                    <div
                                        v-else-if="block.type === 'tool'"
                                        class="overflow-hidden rounded-xl border border-white/10 bg-white/[0.035]">
                                        <button
                                            type="button"
                                            class="flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left text-[12px] text-white/60 hover:bg-white/[0.04]"
                                            @click="handleToggleBlock(message.sourceKey, blockIndex)">
                                            <command
                                                theme="outline"
                                                size="14" />
                                            <span class="min-w-0 flex-1 truncate">{{ block.title }}</span>
                                            <span
                                                v-if="block.statusText"
                                                class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] leading-none"
                                                :class="block.statusClass">
                                                {{ block.statusText }}
                                            </span>
                                            <down
                                                class="transition-transform"
                                                :class="
                                                    isBlockExpanded(message.sourceKey, blockIndex) ? 'rotate-180' : ''
                                                "
                                                theme="outline"
                                                size="13" />
                                        </button>
                                        <pre
                                            v-if="isBlockExpanded(message.sourceKey, blockIndex)"
                                            class="max-h-[420px] overflow-auto border-t border-white/10 bg-black/25 p-3 font-mono text-[12px] leading-5 text-white/70"><code>{{ block.content }}</code></pre>
                                    </div>

                                    <div
                                        v-else-if="block.type === 'code'"
                                        class="overflow-hidden rounded-xl border border-white/10 bg-[#17171a]">
                                        <div
                                            class="flex min-w-0 items-center justify-between gap-2 border-b border-white/10 px-3 py-2 text-[11px] text-white/45">
                                            <span class="truncate font-mono">{{ block.language || 'text' }}</span>
                                            <button
                                                type="button"
                                                class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-white/55 hover:bg-white/[0.06] hover:text-white"
                                                @click="handleCopyBlock(message.sourceKey, blockIndex, block.content)">
                                                <check
                                                    v-if="
                                                        copiedBlockKey === buildBlockKey(message.sourceKey, blockIndex)
                                                    "
                                                    theme="outline"
                                                    size="13" />
                                                <copy
                                                    v-else
                                                    theme="outline"
                                                    size="13" />
                                                <span>{{
                                                    copiedBlockKey === buildBlockKey(message.sourceKey, blockIndex)
                                                        ? '已复制'
                                                        : '复制'
                                                }}</span>
                                            </button>
                                        </div>
                                        <pre
                                            class="max-h-[460px] overflow-auto p-3 font-mono text-[12px] leading-5 text-white/78"><code>{{ block.content }}</code></pre>
                                    </div>
                                </template>
                            </div>
                            <button
                                v-if="message.isLong"
                                type="button"
                                class="mt-1 inline-flex w-fit items-center gap-1 rounded-md px-1.5 py-1 text-[12px] text-white/45 hover:bg-white/[0.05] hover:text-white/70"
                                @click="handleToggleTurn(message.longMessageOrders)">
                                <down
                                    class="transition-transform"
                                    :class="isTurnExpanded(message.longMessageOrders) ? 'rotate-180' : ''"
                                    theme="outline"
                                    size="13" />
                                <span>{{ isTurnExpanded(message.longMessageOrders) ? '收起' : '展开完整内容' }}</span>
                            </button>
                        </div>
                    </div>
                </article>
            </div>
        </div>
    </section>
</template>

<script setup lang="ts">
    import { Check, Command, Copy, DocDetail, Down, Refresh, Terminal } from '@icon-park/vue-next';
    import { convertFileSrc, isTauri } from '@tauri-apps/api/core';
    import { toast } from 'vue-sonner';

    import { Button as UiButton } from '@/components/ui/button';
    import type {
        CodexThreadMessageModel,
        CodexThreadMessageRangeModel,
        CodexThreadStreamEventModel,
        CodexThreadSummaryModel
    } from '@/model/sessionManage';
    import {
        buildLocalMarkdownImagePreviewUrl,
        openLocalFileWithDefaultApp,
        readCodexThreadMessages,
        streamCodexThreadEvents
    } from '@/service/tauri/command';

    defineOptions({
        name: 'SessionManageSessionThreadViewer'
    });

    /** 会话详情兜底刷新间隔；SSE 在浏览器或代理层被缓冲时，用短轮询保证右侧内容继续接近实时。 */
    const THREAD_DETAIL_FALLBACK_REFRESH_INTERVAL_MS = 2_000;
    /** 最多保活的会话监听数量；避免用户频繁切换后后台 SSE 和轮询无限增长。 */
    const MAX_KEEP_ALIVE_THREAD_COUNT = 4;

    const props = defineProps<{
        // 当前左侧选中的真实 CodeX 会话；为空时展示引导占位。
        selectedThread: CodexThreadSummaryModel | null;
    }>();

    /** 会话流连接状态。 */
    type ThreadViewerConnectionState = 'idle' | 'loading' | 'connected' | 'reconnecting' | 'failed';

    /** Markdown 行内片段，用于安全插值模拟 Codex 的 inline code、粗体、链接与图片效果。 */
    interface ThreadInlineSegment {
        /** 片段类型；text 普通文字，code 等宽代码，strong 加粗强调，link 链接，image 图片。 */
        type: 'text' | 'code' | 'strong' | 'link' | 'image';
        /** 当前片段正文；link 为展示标签，image 为 alt 文案。 */
        content: string;
        /** 链接或图片原始地址，普通文本、代码和粗体片段为空。 */
        href: string;
        /** Markdown 可选 title 或完整地址提示，普通文本、代码和粗体片段为空。 */
        title: string;
        /** 图片可直接用于 img src 的预览地址；非图片片段为空。 */
        previewSrc: string;
    }

    /** 右侧会话正文渲染块。 */
    interface ThreadProcessItem {
        /** 步骤标题；展示为 Codex 工作过程里的单行摘要。 */
        title: string;
        /** 步骤详情；为空时只展示摘要，不显示展开箭头。 */
        content: string;
        /** 步骤状态文案；普通步骤为空。 */
        statusText: string;
        /** 步骤状态样式。 */
        statusClass: string;
        /** 步骤图标类型，用于区分读取文件、运行命令、编辑、浏览器和普通工具。 */
        icon: 'tool' | 'file' | 'command' | 'edit' | 'browser';
    }

    /** 右侧会话正文渲染块。 */
    type ThreadMessageRenderBlock =
        | {
              /** 段落块，内部继续拆分行内片段。 */
              type: 'paragraph';
              /** 段落行内片段。 */
              segments: ThreadInlineSegment[];
          }
        | {
              /** 标题块，对应 Markdown heading。 */
              type: 'heading';
              /** 标题正文。 */
              content: string;
          }
        | {
              /** 列表块，对应 Markdown 无序和有序列表。 */
              type: 'list';
              /** 列表项，每一项继续按行内片段渲染。 */
              items: ThreadInlineSegment[][];
          }
        | {
              /** 引用块，对应 Markdown blockquote。 */
              type: 'quote';
              /** 引用正文。 */
              content: string;
          }
        | {
              /** 图片块，对应独立一行的 Markdown image。 */
              type: 'image';
              /** 图片替代文案。 */
              alt: string;
              /** 图片原始地址。 */
              href: string;
              /** 图片悬停标题。 */
              title: string;
              /** 可直接用于 img src 的预览地址。 */
              previewSrc: string;
          }
        | {
              /** 代码块，对应 Markdown fenced code。 */
              type: 'code';
              /** 代码语言；为空时展示 text。 */
              language: string;
              /** 代码正文。 */
              content: string;
          }
        | {
              /** 工具/命令块，用于把 Codex 工具调用文本低对比折叠展示。 */
              type: 'tool';
              /** 工具块标题。 */
              title: string;
              /** 工具块执行状态文案。 */
              statusText: string;
              /** 工具块执行状态样式。 */
              statusClass: string;
              /** 工具块完整正文。 */
              content: string;
          }
        | {
              /** 工作过程折叠组，对齐 Codex 原生的耗时/步骤列表展示。 */
              type: 'processGroup';
              /** 折叠组标题，通常为耗时文案。 */
              title: string;
              /** 组内按原始事件顺序展示的步骤。 */
              items: ThreadProcessItem[];
          };

    /** 可直接被模板渲染的会话回合视图模型。 */
    interface ThreadRenderedTurn {
        /** 回合第一条原始消息，用于判断角色和显示回合时间。 */
        source: CodexThreadMessageModel;
        /** 回合稳定渲染键，助手回合会覆盖多个连续消息。 */
        sourceKey: string;
        /** 当前回合拆分后的展示块。 */
        blocks: ThreadMessageRenderBlock[];
        /** 回合内是否存在命中长消息折叠阈值的消息。 */
        isLong: boolean;
        /** 回合内需要联动展开和收起的长消息顺序。 */
        longMessageOrders: number[];
        /** 本地化短时间文案。 */
        timeText: string;
    }

    /** 单个会话详情的运行态，用于切换会话后继续保留消息、连接和轮询状态。 */
    interface ThreadViewerRuntimeState {
        /** CodeX thread 稳定 ID。 */
        threadId: string;
        /** 当前会话首包或手动刷新是否正在读取。 */
        loading: boolean;
        /** 当前会话是否正在向前加载更早历史消息。 */
        isLoadingMoreBefore: boolean;
        /** 当前会话最近一次读取失败的安全错误文案。 */
        errorMessage: string;
        /** 当前会话流连接状态。 */
        connectionState: ThreadViewerConnectionState;
        /** 当前会话尾部消息窗口。 */
        messages: CodexThreadMessageModel[];
        /** 当前会话尾部消息窗口范围。 */
        messageRange: CodexThreadMessageRangeModel | null;
        /** 当前会话最新 SSE 事件序号。 */
        latestEventSeq: number;
        /** 当前会话最新快照签名，用于兜底轮询幂等。 */
        latestSnapshotSignature: string;
        /** 当前会话 SSE 取消控制器。 */
        streamAbortController: AbortController | null;
        /** 当前会话兜底轮询定时器。 */
        fallbackRefreshTimer: ReturnType<typeof window.setInterval> | null;
        /** 当前会话兜底轮询是否正在执行，防止慢请求堆积。 */
        isFallbackRefreshRunning: boolean;
        /** 当前会话最近被用户查看的时间戳，用于超过保活上限时回收最旧会话。 */
        lastActiveAt: number;
    }

    const threadRuntimeById = ref<Record<string, ThreadViewerRuntimeState>>({});
    const expandedMessageOrders = ref<Set<number>>(new Set<number>());
    const expandedBlockKeys = ref<Set<string>>(new Set<string>());
    const expandedProcessGroupKeys = ref<Set<string>>(new Set<string>());
    const expandedProcessItemKeys = ref<Set<string>>(new Set<string>());
    const copiedBlockKey = ref('');
    const scrollContainerRef = ref<HTMLElement | null>(null);
    let copiedTimer: ReturnType<typeof window.setTimeout> | null = null;

    const currentRuntime = computed<ThreadViewerRuntimeState | null>(() => {
        const threadId = props.selectedThread?.id ?? '';
        return threadId ? (threadRuntimeById.value[threadId] ?? null) : null;
    });

    const loading = computed<boolean>(() => currentRuntime.value?.loading ?? false);
    const errorMessage = computed<string>(() => currentRuntime.value?.errorMessage ?? '');
    const connectionState = computed<ThreadViewerConnectionState>(
        () => currentRuntime.value?.connectionState ?? 'idle'
    );
    const messages = computed<CodexThreadMessageModel[]>(() => currentRuntime.value?.messages ?? []);
    const messageRange = computed<CodexThreadMessageRangeModel | null>(
        () => currentRuntime.value?.messageRange ?? null
    );

    const connectionText = computed<string>(() => {
        if (!props.selectedThread) return '未选择';
        if (connectionState.value === 'loading') return '加载中';
        if (connectionState.value === 'connected') return '已连接';
        if (connectionState.value === 'reconnecting') return '重连中';
        if (connectionState.value === 'failed') return '连接失败';
        return '待连接';
    });

    const connectionBadgeClass = computed<string>(() => {
        if (connectionState.value === 'connected') return 'border-emerald-400/25 bg-emerald-400/10 text-emerald-300';
        if (connectionState.value === 'failed') return 'border-red-400/25 bg-red-400/10 text-red-300';
        return 'border-white/10 bg-white/[0.04] text-white/45';
    });

    const messageRangeText = computed<string>(() => {
        const range = messageRange.value;
        if (!range || !messages.value.length) return '';
        return `${range.startMessageOrder}-${range.endMessageOrder} / ${messages.value.length} 条`;
    });

    const renderedTurns = computed<ThreadRenderedTurn[]>(() => buildRenderedTurns(messages.value));

    // 需要响应左侧选中 thread 切换：当前右侧只换展示态，已打开且未回收的会话继续后台监听。
    watch(
        () => props.selectedThread?.id ?? '',
        (threadId) => {
            void activateThread(threadId);
        },
        { immediate: true }
    );

    /**
     * 构建会话回合列表。
     * 流程：用户消息作为独立右侧气泡；连续助手侧事件合并为一个 Codex 回合，避免工具、思考和状态事件重复出现头像标题。
     * 参数：sourceMessages 为服务端按 messageOrder 返回的窗口消息。
     * 返回：可直接渲染的回合列表。
     * 边界：空列表返回空数组；助手回合在遇到下一条用户消息时结束。
     */
    function buildRenderedTurns(sourceMessages: CodexThreadMessageModel[]): ThreadRenderedTurn[] {
        const turns: ThreadRenderedTurn[] = [];
        let assistantBucket: CodexThreadMessageModel[] = [];
        sourceMessages.forEach((message) => {
            if (message.role === 'user') {
                flushAssistantBucket(turns, assistantBucket);
                assistantBucket = [];
                turns.push(buildRenderedTurn([message]));
                return;
            }
            assistantBucket.push(message);
        });
        flushAssistantBucket(turns, assistantBucket);
        return turns;
    }

    /**
     * 刷新助手回合暂存区。
     * 流程：当暂存区存在连续助手事件时，将其压成一个回合后追加到目标数组。
     * 参数：turns 为最终回合数组，assistantBucket 为连续助手事件暂存区。
     * 返回：无返回值。
     * 边界：暂存区为空时不产生回合。
     */
    function flushAssistantBucket(turns: ThreadRenderedTurn[], assistantBucket: CodexThreadMessageModel[]): void {
        if (!assistantBucket.length) return;
        turns.push(buildRenderedTurn(assistantBucket));
    }

    /**
     * 构建单个会话回合。
     * 流程：合并回合内所有消息的 Markdown 块，并生成稳定 key、时间和长消息列表。
     * 参数：turnMessages 为同一展示回合内的消息，第一条消息决定角色与头像。
     * 返回：可直接渲染的回合模型。
     */
    function buildRenderedTurn(turnMessages: CodexThreadMessageModel[]): ThreadRenderedTurn {
        const firstMessage = turnMessages[0] as CodexThreadMessageModel;
        const lastMessage = turnMessages[turnMessages.length - 1] as CodexThreadMessageModel;
        const longMessageOrders = turnMessages
            .filter((message) => isLongMessage(message) && shouldUseTurnExpand(message))
            .map((message) => message.messageOrder);
        return {
            source: firstMessage,
            sourceKey: `${firstMessage.role}-${firstMessage.messageOrder}-${lastMessage.messageOrder}`,
            blocks: buildTurnBlocks(turnMessages),
            isLong: longMessageOrders.length > 0,
            longMessageOrders,
            timeText: formatMessageTime(firstMessage.createdAt || lastMessage.createdAt)
        };
    }

    /**
     * 构建回合内展示块。
     * 流程：将连续工作事件聚合成 Codex 风格的工作过程折叠组，普通正文仍按 Markdown 分块渲染。
     * 参数：turnMessages 为同一展示回合中的消息列表。
     * 返回：可直接渲染的块列表。
     * 边界：用户消息不参与工作过程聚合，避免用户正文被误折叠。
     */
    function buildTurnBlocks(turnMessages: CodexThreadMessageModel[]): ThreadMessageRenderBlock[] {
        const blocks: ThreadMessageRenderBlock[] = [];
        let processMessages: CodexThreadMessageModel[] = [];
        const flushProcessMessages = (): void => {
            if (!processMessages.length) return;
            blocks.push(buildProcessGroupBlock(processMessages));
            processMessages = [];
        };

        turnMessages.forEach((message) => {
            if (message.role !== 'user' && isProcessMessage(message)) {
                processMessages.push(message);
                return;
            }
            flushProcessMessages();
            blocks.push(...buildMessageBlocks(message));
        });
        flushProcessMessages();

        return blocks.length ? blocks : [{ type: 'paragraph', segments: [buildTextSegment('')] }];
    }

    /**
     * 滚动到底部。
     * 流程：等待 DOM 根据消息更新完成后，把右侧滚动容器移动到底部。
     * 参数：无。
     * 返回：无返回值。
     * 边界：容器尚未挂载时直接返回。
     */
    function scrollToBottom(): void {
        void nextTick(() => {
            const container = scrollContainerRef.value;
            if (!container) return;
            container.scrollTop = container.scrollHeight;
        });
    }

    /**
     * 判断当前消息区是否停留在底部附近。
     * 流程：根据 scrollHeight、scrollTop 和 clientHeight 计算剩余距离，允许少量阈值避免像素误差导致实时消息不跟随。
     * 参数：无。
     * 返回：接近底部时返回 true。
     * 边界：容器尚未挂载时按可跟随处理，保证首次快照仍能滚到底部。
     */
    function isMessageScrollNearBottom(): boolean {
        const container = scrollContainerRef.value;
        if (!container) return true;
        return container.scrollHeight - container.scrollTop - container.clientHeight < 120;
    }

    /**
     * 处理消息区滚动。
     * 流程：用户接近顶部且当前窗口存在更早历史时，触发向前分页加载；非当前会话或正在加载时直接忽略。
     * 参数：无。
     * 返回：无返回值。
     * 异常边界：滚动事件高频触发时由运行态 loading 标记去重。
     */
    function handleMessageScroll(): void {
        const container = scrollContainerRef.value;
        const runtime = currentRuntime.value;
        if (!container || !runtime || !runtime.messageRange?.hasMoreBefore) return;
        if (runtime.loading || runtime.isLoadingMoreBefore) return;
        if (container.scrollTop > 80) return;
        void loadMoreBefore(runtime);
    }

    /**
     * 向前加载更早会话消息。
     * 流程：以当前窗口第一条 messageOrder 为锚点请求上一页，成功后 prepend 到现有消息前，并用高度差保持用户阅读位置不跳动。
     * 参数：runtime 为当前会话运行态。
     * 返回：加载完成 Promise。
     * 异常边界：加载失败只降级连接态，不清空当前已读消息。
     */
    async function loadMoreBefore(runtime: ThreadViewerRuntimeState): Promise<void> {
        const beforeMessageOrder = runtime.messageRange?.startMessageOrder ?? 0;
        if (beforeMessageOrder <= 1) return;
        const container = scrollContainerRef.value;
        const previousHeight = container?.scrollHeight ?? 0;
        const previousTop = container?.scrollTop ?? 0;
        runtime.isLoadingMoreBefore = true;
        try {
            const response = await readCodexThreadMessages(runtime.threadId, beforeMessageOrder);
            mergeBeforeThreadSnapshot(runtime.threadId, response.messages, response.range);
            runtime.connectionState = 'connected';
            await nextTick();
            if (container) {
                container.scrollTop = container.scrollHeight - previousHeight + previousTop;
            }
        } catch {
            runtime.connectionState = runtime.connectionState === 'connected' ? 'connected' : 'reconnecting';
        } finally {
            runtime.isLoadingMoreBefore = false;
        }
    }

    /**
     * 创建会话详情运行态。
     * 流程：为首次打开的 thread 初始化消息、连接和轮询容器，后续切回时复用同一份状态。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：可被当前组件展示或后台保活的运行态。
     */
    function createThreadRuntime(threadId: string): ThreadViewerRuntimeState {
        return {
            threadId,
            loading: false,
            isLoadingMoreBefore: false,
            errorMessage: '',
            connectionState: 'idle',
            messages: [],
            messageRange: null,
            latestEventSeq: 0,
            latestSnapshotSignature: '',
            streamAbortController: null,
            fallbackRefreshTimer: null,
            isFallbackRefreshRunning: false,
            lastActiveAt: Date.now()
        };
    }

    /**
     * 读取或创建会话详情运行态。
     * 流程：优先复用已打开会话的运行态；不存在时创建并写入缓存。
     * 参数：threadId 为 CodeX 会话 ID。
     * 返回：该会话运行态。
     */
    function ensureThreadRuntime(threadId: string): ThreadViewerRuntimeState {
        const current = threadRuntimeById.value[threadId];
        if (current) return current;
        const created = createThreadRuntime(threadId);
        threadRuntimeById.value[threadId] = created;
        return created;
    }

    /**
     * 激活当前选中的会话详情。
     * 流程：切换后立即展示当前会话 loading 或缓存内容，并确保该会话 SSE 与兜底刷新处于保活状态。
     * 参数：threadId 为当前选中的 CodeX thread ID。
     * 返回：激活完成 Promise。
     * 异常边界：单个会话读取失败只写入该会话状态，不影响其它后台监听中的会话。
     */
    async function activateThread(threadId: string): Promise<void> {
        expandedMessageOrders.value = new Set<number>();
        expandedBlockKeys.value = new Set<string>();
        expandedProcessGroupKeys.value = new Set<string>();
        expandedProcessItemKeys.value = new Set<string>();
        copiedBlockKey.value = '';
        if (!threadId) {
            return;
        }
        const runtime = ensureThreadRuntime(threadId);
        runtime.lastActiveAt = Date.now();
        pruneInactiveThreadRuntimes(threadId);
        startThreadListeners(runtime);
        if (runtime.messages.length) {
            runtime.loading = true;
            scrollToBottom();
            await refreshThreadSnapshotIfChanged(threadId, true);
            return;
        }
        await loadInitialThreadSnapshot(runtime);
    }

    /**
     * 读取会话首包快照。
     * 流程：首次打开会话时展示内容区 loading，读取成功后应用快照并滚动到底部。
     * 参数：runtime 为当前会话运行态。
     * 返回：加载完成 Promise。
     * 异常边界：重复触发加载时直接复用正在进行的状态；失败时保留错误文案。
     */
    async function loadInitialThreadSnapshot(runtime: ThreadViewerRuntimeState): Promise<void> {
        if (runtime.loading) return;
        runtime.loading = true;
        runtime.errorMessage = '';
        runtime.connectionState = 'loading';
        try {
            const response = await readCodexThreadMessages(runtime.threadId);
            applyThreadSnapshot(runtime.threadId, response.messages, response.range);
            runtime.connectionState = 'connected';
            if (props.selectedThread?.id === runtime.threadId) scrollToBottom();
        } catch (error) {
            if (error instanceof Error && error.name === 'AbortError') return;
            runtime.errorMessage = error instanceof Error ? error.message : '读取会话内容失败。';
            runtime.connectionState = 'failed';
        } finally {
            runtime.loading = false;
        }
    }

    /**
     * 合并会话流事件。
     * 流程：按 seq 幂等丢弃旧事件；snapshot 整体替换窗口，messageDelta 按 messageOrder 更新或追加。
     * 参数：threadId 为事件所属会话 ID。
     * 参数：event 为 service 已解析的类型化 SSE 事件。
     * 返回：无返回值。
     * 边界：heartbeat 只更新连接态，不触发消息重渲染。
     */
    function handleStreamEvent(threadId: string, event: CodexThreadStreamEventModel): void {
        const runtime = threadRuntimeById.value[threadId];
        if (!runtime || event.seq <= runtime.latestEventSeq) return;
        runtime.latestEventSeq = event.seq;
        if (event.type === 'heartbeat') {
            runtime.connectionState = 'connected';
            return;
        }
        if (event.type === 'snapshot') {
            const shouldStickToBottom = props.selectedThread?.id === threadId && isMessageScrollNearBottom();
            applyThreadSnapshot(threadId, event.messages, event.range);
            runtime.connectionState = 'connected';
            runtime.loading = false;
            if (shouldStickToBottom) scrollToBottom();
            return;
        }
        const index = runtime.messages.findIndex((message) => message.messageOrder === event.message.messageOrder);
        if (index >= 0) {
            runtime.messages.splice(index, 1, event.message);
        } else {
            runtime.messages.push(event.message);
            runtime.messageRange = {
                startMessageOrder: runtime.messageRange?.startMessageOrder ?? event.message.messageOrder,
                endMessageOrder: event.message.messageOrder,
                hasMoreBefore: runtime.messageRange?.hasMoreBefore ?? false,
                hasMoreAfter: false
            };
            runtime.latestSnapshotSignature = buildThreadSnapshotSignature(runtime.messages, runtime.messageRange);
            if (props.selectedThread?.id === threadId) scrollToBottom();
        }
        runtime.connectionState = 'connected';
        runtime.loading = false;
    }

    /**
     * 启动指定会话的后台监听。
     * 流程：同一会话只启动一次 SSE 和兜底刷新，切换到其它会话时继续保持活跃。
     * 参数：runtime 为需要监听的会话运行态。
     * 返回：无返回值。
     * 边界：SSE 失败时兜底刷新仍继续工作；已启动时重复调用无副作用。
     */
    function startThreadListeners(runtime: ThreadViewerRuntimeState): void {
        if (!runtime.fallbackRefreshTimer) startFallbackRefresh(runtime.threadId);
        if (runtime.streamAbortController) return;
        runtime.streamAbortController = new AbortController();
        const { threadId } = runtime;
        void streamCodexThreadEvents(threadId, runtime.streamAbortController.signal, (event) => {
            handleStreamEvent(threadId, event);
        }).catch((error) => {
            const latestRuntime = threadRuntimeById.value[threadId];
            if (!latestRuntime || (error instanceof Error && error.name === 'AbortError')) return;
            latestRuntime.streamAbortController = null;
            if (latestRuntime.connectionState !== 'connected') {
                latestRuntime.connectionState = 'reconnecting';
            }
        });
    }

    /**
     * 启动会话详情兜底刷新。
     * 流程：在 SSE 之外定时读取同一 thread 的尾部窗口，只有内容签名变化时才应用快照，避免无意义重渲染。
     * 参数：threadId 为当前右侧正在展示的 CodeX 会话 ID。
     * 返回：无返回值。
     * 边界：同一会话只启动一个定时器，组件卸载或超过保活上限时统一清理。
     */
    function startFallbackRefresh(threadId: string): void {
        const runtime = threadRuntimeById.value[threadId];
        if (!runtime || runtime.fallbackRefreshTimer) return;
        runtime.fallbackRefreshTimer = window.setInterval(() => {
            void refreshThreadSnapshotIfChanged(threadId);
        }, THREAD_DETAIL_FALLBACK_REFRESH_INTERVAL_MS);
    }

    /**
     * 兜底刷新会话快照。
     * 流程：读取最新消息窗口，先确认仍是当前会话，再按签名判断是否需要替换右侧内容。
     * 参数：threadId 为定时器绑定的会话 ID。
     * 参数：showLoading 表示本次刷新是否需要在当前内容区展示加载状态。
     * 返回：刷新完成 Promise。
     * 异常边界：单次失败不打断页面阅读；连接态降级为重连中，下一轮继续尝试。
     */
    async function refreshThreadSnapshotIfChanged(threadId: string, showLoading = false): Promise<void> {
        const runtime = threadRuntimeById.value[threadId];
        if (!runtime || runtime.isFallbackRefreshRunning) return;
        runtime.isFallbackRefreshRunning = true;
        if (showLoading) runtime.loading = true;
        try {
            const response = await readCodexThreadMessages(threadId);
            const latestRuntime = threadRuntimeById.value[threadId];
            if (!latestRuntime) return;
            const nextSignature = buildThreadSnapshotSignature(response.messages, response.range);
            if (nextSignature === latestRuntime.latestSnapshotSignature) {
                if (latestRuntime.connectionState !== 'connected') latestRuntime.connectionState = 'connected';
                return;
            }
            const shouldStickToBottom = props.selectedThread?.id === threadId && isMessageScrollNearBottom();
            applyThreadSnapshot(threadId, response.messages, response.range);
            latestRuntime.connectionState = 'connected';
            if (shouldStickToBottom) scrollToBottom();
        } catch {
            const latestRuntime = threadRuntimeById.value[threadId];
            if (latestRuntime && latestRuntime.connectionState !== 'connected') {
                latestRuntime.connectionState = 'reconnecting';
            }
        } finally {
            const latestRuntime = threadRuntimeById.value[threadId];
            if (latestRuntime) {
                latestRuntime.isFallbackRefreshRunning = false;
                latestRuntime.loading = false;
            }
        }
    }

    /**
     * 应用会话消息快照。
     * 流程：整体替换当前尾部窗口，并同步记录签名，供 SSE 与兜底刷新共同幂等。
     * 参数：threadId 为当前快照所属会话 ID。
     * 参数：nextMessages 为最新消息窗口，nextRange 为窗口顺序范围。
     * 返回：无返回值。
     */
    function applyThreadSnapshot(
        threadId: string,
        nextMessages: CodexThreadMessageModel[],
        nextRange: CodexThreadMessageRangeModel
    ): void {
        const runtime = threadRuntimeById.value[threadId];
        if (!runtime) return;
        const currentRange = runtime.messageRange;
        if (currentRange && currentRange.startMessageOrder < nextRange.startMessageOrder) {
            const nextOrders = new Set(nextMessages.map((message) => message.messageOrder));
            const retainedMessages = runtime.messages.filter(
                (message) => message.messageOrder < nextRange.startMessageOrder && !nextOrders.has(message.messageOrder)
            );
            runtime.messages = [...retainedMessages, ...nextMessages].sort(
                (current, next) => current.messageOrder - next.messageOrder
            );
            runtime.messageRange = {
                startMessageOrder: currentRange.startMessageOrder,
                endMessageOrder: nextRange.endMessageOrder,
                hasMoreBefore: currentRange.hasMoreBefore,
                hasMoreAfter: nextRange.hasMoreAfter
            };
            runtime.latestSnapshotSignature = buildThreadSnapshotSignature(runtime.messages, runtime.messageRange);
            return;
        }
        runtime.messages = nextMessages;
        runtime.messageRange = nextRange;
        runtime.latestSnapshotSignature = buildThreadSnapshotSignature(nextMessages, nextRange);
    }

    /**
     * 合并更早的会话消息窗口。
     * 流程：按 messageOrder 去重后把上一页消息放到当前窗口前方，并同步扩展 range 起点。
     * 参数：threadId 为当前快照所属会话 ID。
     * 参数：beforeMessages 为向前分页返回的消息窗口。
     * 参数：beforeRange 为向前分页返回的窗口范围。
     * 返回：无返回值。
     * 异常边界：空窗口不改变当前消息，避免把已有详情清空。
     */
    function mergeBeforeThreadSnapshot(
        threadId: string,
        beforeMessages: CodexThreadMessageModel[],
        beforeRange: CodexThreadMessageRangeModel
    ): void {
        const runtime = threadRuntimeById.value[threadId];
        if (!runtime || !beforeMessages.length) return;
        const existingOrders = new Set(runtime.messages.map((message) => message.messageOrder));
        const prependMessages = beforeMessages.filter((message) => !existingOrders.has(message.messageOrder));
        if (!prependMessages.length) {
            runtime.messageRange = {
                startMessageOrder: beforeRange.startMessageOrder || runtime.messageRange?.startMessageOrder || 0,
                endMessageOrder: runtime.messageRange?.endMessageOrder ?? beforeRange.endMessageOrder,
                hasMoreBefore: beforeRange.hasMoreBefore,
                hasMoreAfter: runtime.messageRange?.hasMoreAfter ?? beforeRange.hasMoreAfter
            };
            return;
        }
        runtime.messages = [...prependMessages, ...runtime.messages].sort(
            (current, next) => current.messageOrder - next.messageOrder
        );
        runtime.messageRange = {
            startMessageOrder: beforeRange.startMessageOrder,
            endMessageOrder: runtime.messageRange?.endMessageOrder ?? beforeRange.endMessageOrder,
            hasMoreBefore: beforeRange.hasMoreBefore,
            hasMoreAfter: runtime.messageRange?.hasMoreAfter ?? beforeRange.hasMoreAfter
        };
        runtime.latestSnapshotSignature = buildThreadSnapshotSignature(runtime.messages, runtime.messageRange);
    }

    /**
     * 停止并释放单个会话运行态。
     * 流程：取消 SSE、清理兜底刷新定时器，并复位运行中的兜底标记。
     * 参数：runtime 为待释放的会话运行态。
     * 返回：无返回值。
     */
    function stopThreadRuntime(runtime: ThreadViewerRuntimeState): void {
        runtime.streamAbortController?.abort();
        runtime.streamAbortController = null;
        if (runtime.fallbackRefreshTimer) {
            window.clearInterval(runtime.fallbackRefreshTimer);
            runtime.fallbackRefreshTimer = null;
        }
        runtime.isFallbackRefreshRunning = false;
    }

    /**
     * 回收超过保活上限的旧会话运行态。
     * 流程：按最近查看时间升序停止最旧会话，保留当前会话和最近打开的少量后台监听。
     * 参数：activeThreadId 为当前用户正在查看的会话 ID。
     * 返回：无返回值。
     */
    function pruneInactiveThreadRuntimes(activeThreadId: string): void {
        const runtimes = Object.values(threadRuntimeById.value);
        if (runtimes.length <= MAX_KEEP_ALIVE_THREAD_COUNT) return;
        const removable = runtimes
            .filter((runtime) => runtime.threadId !== activeThreadId)
            .sort((first, second) => first.lastActiveAt - second.lastActiveAt);
        const removeCount = Math.max(0, runtimes.length - MAX_KEEP_ALIVE_THREAD_COUNT);
        removable.slice(0, removeCount).forEach((runtime) => {
            stopThreadRuntime(runtime);
            delete threadRuntimeById.value[runtime.threadId];
        });
    }

    /**
     * 构建前端会话快照签名。
     * 流程：只取影响右侧渲染的公开字段序列化，判断轮询结果是否真的变化。
     * 参数：sourceMessages 为消息窗口，range 为窗口范围。
     * 返回：稳定签名字符串。
     */
    function buildThreadSnapshotSignature(
        sourceMessages: CodexThreadMessageModel[],
        range: CodexThreadMessageRangeModel | null
    ): string {
        return JSON.stringify({
            range,
            messages: sourceMessages.map((message) => ({
                messageOrder: message.messageOrder,
                role: message.role,
                kind: message.kind,
                title: message.title,
                status: message.status,
                content: message.content,
                createdAt: message.createdAt
            }))
        });
    }

    /**
     * 重新读取当前会话。
     * 流程：复用当前选中 thread ID 重新执行加载链路。
     * 参数：无。
     * 返回：无返回值。
     * 边界：未选中会话时不触发请求。
     */
    function handleRetry(): void {
        const threadId = props.selectedThread?.id ?? '';
        if (!threadId) return;
        const runtime = threadRuntimeById.value[threadId];
        if (runtime) {
            stopThreadRuntime(runtime);
            delete threadRuntimeById.value[threadId];
        }
        void activateThread(threadId);
    }

    /**
     * 判断消息是否需要折叠。
     * 流程：按字符数量和行数双阈值判断，避免超长 Markdown 或日志直接撑爆页面。
     * 参数：message 为当前消息。
     * 返回：超过任一阈值时返回 true。
     */
    function isLongMessage(message: CodexThreadMessageModel): boolean {
        return message.content.length > 20_000 || message.content.split('\n').length > 300;
    }

    /**
     * 判断消息是否适合显示回合级展开入口。
     * 流程：仅普通助手回复和最终回复使用回合级展开；工具、思考和状态事件已有独立折叠块，避免在整组消息底部出现误导性的“展开完整内容”。
     * 参数：message 为当前消息。
     * 返回：适合由回合底部按钮展开时返回 true。
     */
    function shouldUseTurnExpand(message: CodexThreadMessageModel): boolean {
        return message.kind === 'assistant' || message.kind === 'commentary' || message.kind === 'finalAnswer';
    }

    /**
     * 判断回合内长消息是否已经全部展开。
     * 流程：回合可能聚合多条助手事件，需要所有长消息都展开时才展示收起态。
     * 参数：messageOrders 为回合内命中折叠阈值的消息顺序列表。
     * 返回：全部展开时返回 true。
     */
    function isTurnExpanded(messageOrders: number[]): boolean {
        return (
            messageOrders.length > 0 &&
            messageOrders.every((messageOrder) => expandedMessageOrders.value.has(messageOrder))
        );
    }

    /**
     * 切换回合长消息展开状态。
     * 流程：复制 Set 后批量增删回合内长消息顺序，保证 Vue 能感知变化。
     * 参数：messageOrders 为目标回合内命中折叠阈值的消息顺序列表。
     * 返回：无返回值。
     */
    function handleToggleTurn(messageOrders: number[]): void {
        const shouldCollapse = isTurnExpanded(messageOrders);
        const next = new Set(expandedMessageOrders.value);
        messageOrders.forEach((messageOrder) => {
            if (shouldCollapse) next.delete(messageOrder);
            else next.add(messageOrder);
        });
        expandedMessageOrders.value = next;
    }

    /**
     * 获取消息用于渲染的正文。
     * 流程：普通消息原样展示；长消息未展开时按行数和字符数截断并追加提示。
     * 参数：message 为当前消息。
     * 返回：用于分块渲染的正文。
     */
    function visibleMessageContent(message: CodexThreadMessageModel): string {
        if (
            !shouldUseTurnExpand(message) ||
            !isLongMessage(message) ||
            expandedMessageOrders.value.has(message.messageOrder)
        ) {
            return message.content;
        }
        const lines = message.content.split('\n').slice(0, 120).join('\n');
        return `${lines.slice(0, 12_000)}\n\n内容较长，已折叠。`;
    }

    /**
     * 判断消息是否属于 Codex 工作过程。
     * 流程：工具调用、工具结果、思考和状态事件进入工作过程折叠组；普通助手正文继续直接渲染。
     * 参数：message 为当前消息。
     * 返回：应展示在工作过程里时返回 true。
     */
    function isProcessMessage(message: CodexThreadMessageModel): boolean {
        return (
            message.kind === 'reasoning' ||
            message.kind === 'toolCall' ||
            message.kind === 'toolResult' ||
            message.kind === 'status'
        );
    }

    /**
     * 构建工作过程折叠组。
     * 流程：按消息创建时间计算耗时标题，再把每条结构化事件转成可展开步骤。
     * 参数：processMessages 为连续工作过程消息。
     * 返回：Codex 风格工作过程块。
     */
    function buildProcessGroupBlock(processMessages: CodexThreadMessageModel[]): ThreadMessageRenderBlock {
        return {
            type: 'processGroup',
            title: buildProcessGroupTitle(processMessages),
            items: processMessages.map((message) => buildProcessItem(message))
        };
    }

    /**
     * 构建工作过程标题。
     * 流程：优先使用首尾时间差生成“耗时”文案；时间不可用时降级为“工作过程”。
     * 参数：processMessages 为同一工作过程中的消息列表。
     * 返回：折叠按钮标题。
     */
    function buildProcessGroupTitle(processMessages: CodexThreadMessageModel[]): string {
        const timestamps = processMessages
            .map((message) => parseMessageTimestamp(message.createdAt))
            .filter((timestamp) => timestamp > 0);
        if (timestamps.length < 2) return '工作过程';
        const elapsedSeconds = Math.max(1, Math.round((Math.max(...timestamps) - Math.min(...timestamps)) / 1000));
        return `耗时 ${formatElapsedTime(elapsedSeconds)}`;
    }

    /**
     * 构建工作过程步骤。
     * 流程：根据 kind/title/content 识别步骤类型，摘要展示在列表，完整内容折叠到详情。
     * 参数：message 为结构化工作事件。
     * 返回：可渲染的工作过程步骤。
     */
    function buildProcessItem(message: CodexThreadMessageModel): ThreadProcessItem {
        const content = visibleMessageContent(message);
        const title = buildProcessItemTitle(message, content);
        return {
            title,
            content: buildProcessItemContent(message, content),
            statusText: formatMessageStatus(message.status),
            statusClass: buildStatusClass(message.status),
            icon: buildProcessItemIcon(title, message)
        };
    }

    /**
     * 构建工作过程步骤标题。
     * 流程：结构化工具优先使用工具标题，思考和状态使用专属文案，最终统一压缩长度。
     * 参数：message 为结构化工作事件。
     * 参数：content 为当前事件详情正文。
     * 返回：步骤摘要标题。
     */
    function buildProcessItemTitle(message: CodexThreadMessageModel, content: string): string {
        if (message.kind === 'reasoning') return compactProcessTitle(message.title || content || '思考');
        if (message.kind === 'status') return compactProcessTitle(message.title || content || '状态更新');
        if (message.kind === 'toolCall') return compactProcessTitle(buildStructuredToolTitle(message));
        if (message.kind === 'toolResult') return compactProcessTitle(buildStructuredToolTitle(message));
        return compactProcessTitle(message.title || '工作过程');
    }

    /**
     * 构建工作过程步骤详情。
     * 流程：工具和思考保留正文，纯状态且正文等于标题时不展示详情。
     * 参数：message 为结构化工作事件。
     * 参数：content 为当前事件详情正文。
     * 返回：展开后显示的详情文本。
     */
    function buildProcessItemContent(message: CodexThreadMessageModel, content: string): string {
        const trimmedContent = content.trim();
        if (!trimmedContent) return '';
        if (message.kind === 'status' && trimmedContent === message.title.trim()) return '';
        return trimmedContent;
    }

    /**
     * 构建工作过程步骤图标类型。
     * 流程：按标题和消息类型归类到文件、命令、编辑、浏览器或普通工具。
     * 参数：title 为步骤标题，message 为结构化工作事件。
     * 返回：模板可识别的图标类型。
     */
    function buildProcessItemIcon(title: string, message: CodexThreadMessageModel): ThreadProcessItem['icon'] {
        if (/读取|文件|上下文/.test(title)) return 'file';
        if (/命令|检查|lint|cargo|npm|python/i.test(title)) return 'command';
        if (/编辑|已编辑|patch/i.test(title)) return 'edit';
        if (/浏览器|页面|截图/i.test(title)) return 'browser';
        if (message.kind === 'toolResult' && /结果/.test(title)) return 'command';
        return 'tool';
    }

    /**
     * 压缩工作过程标题。
     * 流程：去除换行和多余空白，限制长度避免步骤行撑破右侧详情。
     * 参数：title 为原始标题。
     * 返回：短标题。
     */
    function compactProcessTitle(title: string): string {
        const normalizedTitle = title.trim().replace(/\s+/g, ' ');
        if (!normalizedTitle) return '工作过程';
        return normalizedTitle.length > 72 ? `${normalizedTitle.slice(0, 72)}...` : normalizedTitle;
    }

    /**
     * 解析消息时间戳。
     * 流程：兼容毫秒时间戳和 ISO 字符串，非法时间返回 0 供调用方过滤。
     * 参数：value 为消息创建时间。
     * 返回：毫秒时间戳。
     */
    function parseMessageTimestamp(value: string): number {
        if (!value) return 0;
        const timestamp = /^\d+$/.test(value) ? Number(value) : Date.parse(value);
        return Number.isFinite(timestamp) ? timestamp : 0;
    }

    /**
     * 格式化耗时。
     * 流程：按 Codex 风格展示秒、分钟秒或小时分钟秒。
     * 参数：elapsedSeconds 为总秒数。
     * 返回：中文耗时文案。
     */
    function formatElapsedTime(elapsedSeconds: number): string {
        const hours = Math.floor(elapsedSeconds / 3600);
        const minutes = Math.floor((elapsedSeconds % 3600) / 60);
        const seconds = elapsedSeconds % 60;
        if (hours > 0) return `${hours}小时 ${minutes}分钟 ${seconds}秒`;
        if (minutes > 0) return `${minutes}分钟 ${seconds}秒`;
        return `${seconds}秒`;
    }

    /**
     * 构建消息渲染块。
     * 流程：先按 Markdown 代码围栏切块，再把普通文本解析成标题、列表、引用、工具块和段落。
     * 参数：message 为当前消息。
     * 返回：可直接在模板中安全插值渲染的块列表。
     * 边界：不使用 v-html，避免外部会话正文注入页面。
     */
    function buildMessageBlocks(message: CodexThreadMessageModel): ThreadMessageRenderBlock[] {
        const source = visibleMessageContent(message);
        if (isStructuredToolMessage(message)) {
            return [
                {
                    type: 'tool',
                    title: buildStructuredToolTitle(message),
                    statusText: formatMessageStatus(message.status),
                    statusClass: buildStatusClass(message.status),
                    content: source || '暂无可展示内容。'
                }
            ];
        }
        if (message.kind === 'reasoning') {
            return [
                {
                    type: 'quote',
                    content: source || message.title || '思考中'
                }
            ];
        }
        if (message.kind === 'status' && message.title) {
            return [
                {
                    type: 'tool',
                    title: message.title,
                    statusText: formatMessageStatus(message.status),
                    statusClass: buildStatusClass(message.status),
                    content: source || message.title
                }
            ];
        }
        const blocks: ThreadMessageRenderBlock[] = [];
        const fencePattern = /```([^\n`]*)\n?([\s\S]*?)```/g;
        let cursor = 0;
        for (const match of source.matchAll(fencePattern)) {
            const matchIndex = match.index ?? 0;
            if (matchIndex > cursor) blocks.push(...buildTextBlocks(source.slice(cursor, matchIndex)));
            blocks.push({
                type: 'code',
                language: (match[1] ?? '').trim(),
                content: match[2] ?? ''
            });
            cursor = matchIndex + match[0].length;
        }
        if (cursor < source.length) blocks.push(...buildTextBlocks(source.slice(cursor)));
        return blocks.length ? blocks : [{ type: 'paragraph', segments: [buildTextSegment(source)] }];
    }

    /**
     * 构建普通文本块。
     * 流程：逐行识别 Markdown 轻量语义，连续列表合并，连续普通行合成段落。
     * 参数：source 为不包含 fenced code 的文本片段。
     * 返回：正文渲染块列表。
     */
    function buildTextBlocks(source: string): ThreadMessageRenderBlock[] {
        const lines = source.replace(/\r\n/g, '\n').split('\n');
        const blocks: ThreadMessageRenderBlock[] = [];
        let index = 0;
        while (index < lines.length) {
            const line = lines[index] ?? '';
            const trimmed = line.trim();
            if (!trimmed) {
                index += 1;
                continue;
            }
            const headingMatch = trimmed.match(/^(#{1,4})\s+(.+)$/);
            if (headingMatch) {
                blocks.push({ type: 'heading', content: headingMatch[2] ?? '' });
                index += 1;
                continue;
            }
            const strongHeadingMatch = trimmed.match(/^\*\*([^*\n]+)\*\*$/);
            if (strongHeadingMatch) {
                blocks.push({ type: 'heading', content: strongHeadingMatch[1] ?? '' });
                index += 1;
                continue;
            }
            if (/^>\s?/.test(trimmed)) {
                const quoteLines: string[] = [];
                while (index < lines.length && /^>\s?/.test((lines[index] ?? '').trim())) {
                    quoteLines.push((lines[index] ?? '').trim().replace(/^>\s?/, ''));
                    index += 1;
                }
                blocks.push({ type: 'quote', content: quoteLines.join('\n') });
                continue;
            }
            if (isToolLikeLine(trimmed)) {
                const toolLines: string[] = [];
                while (index < lines.length && (lines[index] ?? '').trim()) {
                    toolLines.push(lines[index] ?? '');
                    index += 1;
                }
                const content = toolLines.join('\n');
                blocks.push({
                    type: 'tool',
                    title: buildToolTitle(content),
                    statusText: '',
                    statusClass: '',
                    content
                });
                continue;
            }
            if (/^([-*+]\s+|\d+\.\s+)/.test(trimmed)) {
                const items: ThreadInlineSegment[][] = [];
                while (index < lines.length && /^([-*+]\s+|\d+\.\s+)/.test((lines[index] ?? '').trim())) {
                    items.push(parseInlineSegments((lines[index] ?? '').trim().replace(/^([-*+]\s+|\d+\.\s+)/, '')));
                    index += 1;
                }
                blocks.push({ type: 'list', items });
                continue;
            }
            const imageBlock = buildStandaloneImageBlock(trimmed);
            if (imageBlock) {
                blocks.push(imageBlock);
                index += 1;
                continue;
            }
            const paragraphLines: string[] = [];
            while (index < lines.length) {
                const current = lines[index] ?? '';
                const currentTrimmed = current.trim();
                if (!currentTrimmed) break;
                if (/^(#{1,4})\s+/.test(currentTrimmed)) break;
                if (/^>\s?/.test(currentTrimmed)) break;
                if (/^([-*+]\s+|\d+\.\s+)/.test(currentTrimmed)) break;
                if (buildStandaloneImageBlock(currentTrimmed)) break;
                if (paragraphLines.length && isToolLikeLine(currentTrimmed)) break;
                paragraphLines.push(current);
                index += 1;
            }
            blocks.push({ type: 'paragraph', segments: parseInlineSegments(paragraphLines.join('\n')) });
        }
        return blocks;
    }

    /**
     * 解析行内 Markdown 片段。
     * 流程：识别反引号 inline code 和双星号粗体，剩余内容作为普通文本安全插值。
     * 参数：source 为一段普通文本。
     * 返回：行内片段列表。
     */
    function parseInlineSegments(source: string): ThreadInlineSegment[] {
        const segments: ThreadInlineSegment[] = [];
        const inlinePattern =
            /(!\[[^\]\n]*\]\([^) \n]+(?:\s+"[^"]*")?\)|\[[^\]\n]+\]\([^) \n]+(?:\s+"[^"]*")?\)|!\[[^\]\n]*\]\([^) \n]+|\[[^\]\n]+\]\([^) \n]+|`[^`\n]+`|\*\*[^*\n]+\*\*)/g;
        let cursor = 0;
        for (const match of source.matchAll(inlinePattern)) {
            const matchIndex = match.index ?? 0;
            if (matchIndex > cursor) segments.push(buildTextSegment(source.slice(cursor, matchIndex)));
            const token = match[0];
            if (token.startsWith('![')) {
                segments.push(buildMarkdownResourceSegment(token, true));
            } else if (token.startsWith('[')) {
                segments.push(buildMarkdownResourceSegment(token, false));
            } else if (token.startsWith('`')) {
                segments.push(buildSimpleSegment('code', token.slice(1, -1)));
            } else {
                segments.push(buildSimpleSegment('strong', token.slice(2, -2)));
            }
            cursor = matchIndex + token.length;
        }
        if (cursor < source.length) segments.push(buildTextSegment(source.slice(cursor)));
        return segments.length ? segments : [buildTextSegment(source)];
    }

    /**
     * 构建普通文本行内片段。
     * 流程：复用完整字段结构，保证模板访问链接和图片字段时不需要可选链兜底。
     * 参数：content 为普通正文。
     * 返回：文本片段。
     */
    function buildTextSegment(content: string): ThreadInlineSegment {
        return buildSimpleSegment('text', content);
    }

    /**
     * 构建基础行内片段。
     * 流程：为 text、code、strong 统一补齐非业务字段，避免类型分支散落在调用点。
     * 参数：type 为基础片段类型，content 为展示正文。
     * 返回：基础片段。
     */
    function buildSimpleSegment(type: 'text' | 'code' | 'strong', content: string): ThreadInlineSegment {
        return {
            type,
            content,
            href: '',
            title: '',
            previewSrc: ''
        };
    }

    /**
     * 构建 Markdown 链接或图片片段。
     * 流程：解析 `[label](href "title")` 和 `![alt](src "title")`，过滤危险协议，本地图片在 Tauri 中转成 asset 地址。
     * 参数：token 为完整 Markdown token，isImage 表示是否为图片语法。
     * 返回：可安全插值渲染的链接或图片片段。
     * 边界：解析失败时按普通文本返回，避免误吞正文。
     */
    function buildMarkdownResourceSegment(token: string, isImage: boolean): ThreadInlineSegment {
        const match = token.match(
            isImage
                ? /^!\[([^\]\n]*)\]\(([^) \n]+)(?:\s+"([^"]*)")?\)?$/
                : /^\[([^\]\n]+)\]\(([^) \n]+)(?:\s+"([^"]*)")?\)?$/
        );
        if (!match) return buildTextSegment(token);
        const href = sanitizeMarkdownHref(match[2]?.trim() ?? '');
        if (!href) return buildTextSegment(match[1]?.trim() || token);
        return {
            type: isImage ? 'image' : 'link',
            content: match[1]?.trim() ?? '',
            href,
            title: match[3]?.trim() ?? '',
            previewSrc: isImage ? buildImagePreviewSrc(href) : ''
        };
    }

    /**
     * 构建独立 Markdown 图片块。
     * 流程：只处理整行图片语法，复用行内资源解析和本机图片预览地址转换。
     * 参数：line 为已 trim 的单行文本。
     * 返回：可渲染图片块；非独立图片或不可预览时返回 null。
     * 边界：不处理混排段落，混排仍由行内片段解析兜底。
     */
    function buildStandaloneImageBlock(line: string): ThreadMessageRenderBlock | null {
        if (!/^!\[[^\]\n]*\]\([^) \n]+(?:\s+"[^"]*")?\)?$/.test(line)) return null;
        const segment = buildMarkdownResourceSegment(line, true);
        if (segment.type !== 'image' || !segment.previewSrc) return null;
        return {
            type: 'image',
            alt: segment.content,
            href: segment.href,
            title: segment.title,
            previewSrc: segment.previewSrc
        };
    }

    /**
     * 清理 Markdown 资源地址。
     * 流程：只允许 http、https、data:image、blob、asset、本地绝对路径和站内相对路径；其它协议降级为纯文本。
     * 参数：href 为 Markdown 中的原始地址。
     * 返回：允许渲染的地址；不允许时返回空字符串。
     */
    function sanitizeMarkdownHref(href: string): string {
        if (!href) return '';
        if (/^(https?:\/\/|data:image\/|blob:|asset:|\/)/i.test(href)) return href;
        if (/^[A-Za-z]:[\\/]/.test(href)) return href;
        return '';
    }

    /**
     * 判断链接是否允许作为浏览器跳转地址。
     * 流程：仅 http/https 链接设置 href，避免 file、asset 或未知协议被点击跳转。
     * 参数：href 为已清理的链接地址。
     * 返回：可跳转时返回 true。
     */
    function isNavigableHref(href: string): boolean {
        return /^https?:\/\//i.test(href);
    }

    /**
     * 判断链接是否是可交给桌面端打开的本机文件路径。
     * 流程：只识别 Unix 绝对路径和 Windows 盘符路径；http、asset、data、站内接口路径等继续按不可本机打开处理。
     * 参数：href 为已清理的链接地址。
     * 返回：像本机绝对路径时返回 true。
     */
    function isOpenableLocalFileHref(href: string): boolean {
        return /^\/(?!v1\/|assets?\/)/.test(href) || /^[A-Za-z]:[\\/]/.test(href);
    }

    /**
     * 打开会话正文中的本机文件链接。
     * 流程：点击本机文件标签后交给 Tauri 原生命令，由 macOS 使用当前文件类型的默认应用打开。
     * 参数：href 为 Markdown 链接中的本机文件路径。
     * 返回：无返回值。
     * 异常/边界：普通浏览器或系统打开失败时展示错误提示，不影响会话流继续渲染。
     */
    async function handleOpenLocalFile(href: string): Promise<void> {
        try {
            await openLocalFileWithDefaultApp(href);
        } catch (error) {
            toast.error('打开文件失败', {
                description: error instanceof Error ? error.message : '请确认当前在 CodexMan 桌面端中使用。'
            });
        }
    }

    /**
     * 构建图片预览地址。
     * 流程：http、data、blob、asset 直接使用；本地绝对路径在 Tauri 环境转为 asset，普通浏览器走本机 sidecar 图片代理。
     * 参数：href 为已清理的图片地址。
     * 返回：可用于 img src 的地址；不可预览时返回空字符串。
     */
    function buildImagePreviewSrc(href: string): string {
        if (/^(https?:\/\/|data:image\/|blob:|asset:)/i.test(href)) return href;
        if (!isImageFilePath(href)) return '';
        return isTauri() ? convertFileSrc(href) : buildLocalMarkdownImagePreviewUrl(href);
    }

    /**
     * 判断地址是否像图片文件。
     * 流程：去掉查询和 hash 后匹配常见图片扩展名，避免把任意本地路径塞进图片标签。
     * 参数：href 为资源地址。
     * 返回：命中图片后缀时返回 true。
     */
    function isImageFilePath(href: string): boolean {
        const cleanHref = href.split(/[?#]/)[0] ?? '';
        return /\.(png|jpe?g|webp|gif|bmp|svg)$/i.test(cleanHref);
    }

    /**
     * 压缩链接展示文本。
     * 流程：URL 显示域名加末尾路径，本地路径显示文件名，过长时截断中间噪音。
     * 参数：href 为链接或资源地址。
     * 返回：适合气泡和正文内展示的短文案。
     */
    function compactLinkText(href: string): string {
        if (!href) return '';
        if (/^https?:\/\//i.test(href)) {
            try {
                const url = new URL(href);
                const pathName = url.pathname.split('/').filter(Boolean).pop();
                return pathName ? `${url.host}/${pathName}` : url.host;
            } catch {
                return href.length > 48 ? `${href.slice(0, 24)}...${href.slice(-18)}` : href;
            }
        }
        const normalizedHref = href.replace(/\\/g, '/');
        const fileName = normalizedHref.split('/').filter(Boolean).pop();
        return fileName || (href.length > 48 ? `${href.slice(0, 24)}...${href.slice(-18)}` : href);
    }

    /**
     * 判断一行文本是否更像 Codex 工具状态。
     * 流程：匹配历史转写中的 tool call/result、命令输出和补丁标记。
     * 参数：line 为已 trim 的单行文本。
     * 返回：命中工具块特征时返回 true。
     */
    function isToolLikeLine(line: string): boolean {
        return (
            /^\[\d+\]\s+tool\b/i.test(line) ||
            /^tool\s+[\w.-]+\s+(call|result)\b/i.test(line) ||
            /^Output:\s*$/i.test(line) ||
            /^Wall time:/i.test(line) ||
            /^\*\*\* Begin Patch/.test(line) ||
            /^Enumerating all window names/i.test(line) ||
            /^已读取文件运行了命令/.test(line)
        );
    }

    /**
     * 构建工具块标题。
     * 流程：优先取第一行并压缩空白，过长时截断，避免折叠标题撑破页面。
     * 参数：content 为工具块完整正文。
     * 返回：工具块标题文案。
     */
    function buildToolTitle(content: string): string {
        const firstLine = content.split('\n')[0]?.trim().replace(/\s+/g, ' ') || '工具调用';
        return firstLine.length > 96 ? `${firstLine.slice(0, 96)}...` : firstLine;
    }

    /**
     * 判断消息是否为服务端结构化工具消息。
     * 流程：基于后端新增 kind 字段识别工具调用和工具结果，避免前端只靠文本猜测。
     * 参数：message 为当前消息。
     * 返回：需要按工具块渲染时返回 true。
     */
    function isStructuredToolMessage(message: CodexThreadMessageModel): boolean {
        return message.kind === 'toolCall' || message.kind === 'toolResult';
    }

    /**
     * 构建结构化工具标题。
     * 流程：优先使用服务端标题；缺失时按 kind 给出稳定兜底文案。
     * 参数：message 为当前消息。
     * 返回：工具块标题。
     */
    function buildStructuredToolTitle(message: CodexThreadMessageModel): string {
        if (message.title) return normalizeToolTitle(message.title, message.content);
        return message.kind === 'toolCall' ? '工具调用' : '工具结果';
    }

    /**
     * 归一化历史工具标题。
     * 流程：兼容旧会话中已经落成 exec_command、apply_patch、tool_search 等内部名的数据，结合正文内容转成人可读摘要。
     * 参数：title 为服务端返回标题，content 为工具调用参数或工具输出。
     * 返回：适合折叠块展示的短标题。
     */
    function normalizeToolTitle(title: string, content: string): string {
        if (title === 'exec_command') return buildExecCommandDisplayTitle(content);
        if (title === 'apply_patch') return content.includes('Success.') ? '已编辑' : '编辑文件';
        if (title === 'tool_search' || title === 'tool_search_tool' || title === 'tool_search 结果') {
            return '工具查找结果';
        }
        if (title === 'mcp__node_repl__js' || title === 'node_repl.js' || title === 'js') {
            return buildNodeReplDisplayTitle(content);
        }
        if (title === '工具结果') return buildToolResultDisplayTitle(content);
        return title;
    }

    /**
     * 构建命令工具展示标题。
     * 流程：解析旧工具调用 JSON 中的 cmd，识别读取、检查、构建和浏览器连接场景。
     * 参数：content 为工具调用参数或原始命令文本。
     * 返回：命令类折叠标题。
     */
    function buildExecCommandDisplayTitle(content: string): string {
        const command = parseToolJsonStringField(content, 'cmd') || content;
        const trimmedCommand = command.trim();
        if (trimmedCommand.includes('browser-client.mjs')) return '连接本地页面浏览器';
        if (/^(sed|rg|tail|head|nl)\s/.test(trimmedCommand)) return '读取项目上下文';
        if (/^git\s+(diff|status|show)\b/.test(trimmedCommand)) return '读取项目上下文';
        if (trimmedCommand.startsWith('npm run lint')) return '运行前端检查';
        if (trimmedCommand.startsWith('cargo check')) return '运行 Rust 检查';
        if (trimmedCommand.startsWith('cargo fmt')) return '检查 Rust 格式';
        if (trimmedCommand.startsWith('python3 -m py_compile')) return '检查 Python 语法';
        return trimmedCommand ? `运行命令：${trimmedCommand.slice(0, 48)}` : '运行命令';
    }

    /**
     * 构建 Node REPL 工具展示标题。
     * 流程：优先复用调用参数中的 title，缺失时按代码内容识别浏览器、截图和页面状态读取。
     * 参数：content 为工具调用参数 JSON。
     * 返回：Node 工具折叠标题。
     */
    function buildNodeReplDisplayTitle(content: string): string {
        const title = parseToolJsonStringField(content, 'title');
        if (title) return title.slice(0, 48);
        const code = parseToolJsonStringField(content, 'code') || '';
        if (code.includes('setupBrowserRuntime') || code.includes('getForUrl')) return '连接本地页面浏览器';
        if (code.includes('screenshot')) return '保存页面截图';
        if (code.includes('domSnapshot') || code.includes('evaluate')) return '读取页面状态';
        return '运行本地脚本';
    }

    /**
     * 构建工具结果展示标题。
     * 流程：根据输出特征识别编辑完成、上下文压缩、工具查找、浏览器连接和命令结果。
     * 参数：content 为工具输出正文。
     * 返回：结果类折叠标题。
     */
    function buildToolResultDisplayTitle(content: string): string {
        if (content.includes('Success. Updated the following files:')) return '已编辑';
        if (content.includes('上下文已压缩') || content.includes('context compact')) return '上下文已压缩';
        if (content.includes('Found ') && content.includes(' tools')) return '工具查找结果';
        if (content.includes('Selected Browser')) return '浏览器已连接';
        if (content.includes('Exit code:') || content.includes('Wall time:')) return '命令结果';
        return '工具结果';
    }

    /**
     * 解析工具 JSON 参数中的字符串字段。
     * 流程：只做浅层 JSON.parse，解析失败返回空字符串让调用方兜底。
     * 参数：source 为工具参数 JSON，field 为字段名。
     * 返回：字段字符串；不存在或解析失败时为空字符串。
     */
    function parseToolJsonStringField(source: string, field: string): string {
        try {
            const value = JSON.parse(source) as Record<string, unknown>;
            const fieldValue = value[field];
            return typeof fieldValue === 'string' ? fieldValue : '';
        } catch {
            return '';
        }
    }

    /**
     * 格式化结构化消息状态。
     * 流程：将服务端状态枚举转成短文案；未知状态保持原值截断展示。
     * 参数：status 为工具或状态块执行状态。
     * 返回：状态标签文案。
     */
    function formatMessageStatus(status: string): string {
        if (!status) return '';
        if (status === 'running') return '运行中';
        if (status === 'completed') return '完成';
        if (status === 'failed') return '失败';
        return status.length > 12 ? `${status.slice(0, 12)}...` : status;
    }

    /**
     * 构建状态标签样式。
     * 流程：失败用红色、完成用绿色，其他状态保持低对比灰色。
     * 参数：status 为工具或状态块执行状态。
     * 返回：Tailwind class 字符串。
     */
    function buildStatusClass(status: string): string {
        if (status === 'completed') return 'border-emerald-400/20 bg-emerald-400/10 text-emerald-300';
        if (status === 'failed') return 'border-red-400/20 bg-red-400/10 text-red-300';
        return 'border-white/10 bg-white/[0.04] text-white/45';
    }

    /**
     * 生成块展开状态键。
     * 流程：用消息顺序和块下标组合，避免不同消息的块状态互相影响。
     * 参数：sourceKey 为回合稳定键，blockIndex 为块下标。
     * 返回：稳定状态键。
     */
    function buildBlockKey(sourceKey: string, blockIndex: number): string {
        return `${sourceKey}:${blockIndex}`;
    }

    /**
     * 判断工具块是否展开。
     * 流程：查询响应式 Set 中是否存在当前块键。
     * 参数：sourceKey 为回合稳定键，blockIndex 为块下标。
     * 返回：已展开时返回 true。
     */
    function isBlockExpanded(sourceKey: string, blockIndex: number): boolean {
        return expandedBlockKeys.value.has(buildBlockKey(sourceKey, blockIndex));
    }

    /**
     * 切换工具块展开状态。
     * 流程：复制 Set 后增删当前块键，保持 Vue 响应式更新。
     * 参数：sourceKey 为回合稳定键，blockIndex 为块下标。
     * 返回：无返回值。
     */
    function handleToggleBlock(sourceKey: string, blockIndex: number): void {
        const blockKey = buildBlockKey(sourceKey, blockIndex);
        const next = new Set(expandedBlockKeys.value);
        if (next.has(blockKey)) next.delete(blockKey);
        else next.add(blockKey);
        expandedBlockKeys.value = next;
    }

    /**
     * 判断工作过程组是否展开。
     * 流程：工作过程默认收起；用户点击耗时标题后记录到 expanded set，避免思考和工具过程默认铺满正文。
     * 参数：sourceKey 为回合稳定键，blockIndex 为块下标。
     * 返回：当前工作过程应展开时返回 true。
     */
    function isProcessGroupExpanded(sourceKey: string, blockIndex: number): boolean {
        return expandedProcessGroupKeys.value.has(buildBlockKey(sourceKey, blockIndex));
    }

    /**
     * 切换工作过程组展开状态。
     * 流程：复制 Set 后增删当前组 key，保证 Vue 响应式更新。
     * 参数：sourceKey 为回合稳定键，blockIndex 为块下标。
     * 返回：无返回值。
     */
    function handleToggleProcessGroup(sourceKey: string, blockIndex: number): void {
        const groupKey = buildBlockKey(sourceKey, blockIndex);
        const next = new Set(expandedProcessGroupKeys.value);
        if (next.has(groupKey)) next.delete(groupKey);
        else next.add(groupKey);
        expandedProcessGroupKeys.value = next;
    }

    /**
     * 生成工作过程步骤展开键。
     * 流程：用回合 key、组下标和步骤下标组合，避免不同工作过程的步骤状态互相影响。
     * 参数：sourceKey 为回合稳定键，blockIndex 为工作过程块下标，itemIndex 为步骤下标。
     * 返回：稳定步骤状态键。
     */
    function buildProcessItemKey(sourceKey: string, blockIndex: number, itemIndex: number): string {
        return `${buildBlockKey(sourceKey, blockIndex)}:${itemIndex}`;
    }

    /**
     * 判断工作过程步骤是否展开。
     * 流程：查询步骤展开 Set，步骤默认收起，减少命令输出对阅读的干扰。
     * 参数：sourceKey 为回合稳定键，blockIndex 为工作过程块下标，itemIndex 为步骤下标。
     * 返回：步骤已展开时返回 true。
     */
    function isProcessItemExpanded(sourceKey: string, blockIndex: number, itemIndex: number): boolean {
        return expandedProcessItemKeys.value.has(buildProcessItemKey(sourceKey, blockIndex, itemIndex));
    }

    /**
     * 切换工作过程步骤展开状态。
     * 流程：复制 Set 后增删当前步骤 key，让用户可以逐项查看工具详情。
     * 参数：sourceKey 为回合稳定键，blockIndex 为工作过程块下标，itemIndex 为步骤下标。
     * 返回：无返回值。
     */
    function handleToggleProcessItem(sourceKey: string, blockIndex: number, itemIndex: number): void {
        const itemKey = buildProcessItemKey(sourceKey, blockIndex, itemIndex);
        const next = new Set(expandedProcessItemKeys.value);
        if (next.has(itemKey)) next.delete(itemKey);
        else next.add(itemKey);
        expandedProcessItemKeys.value = next;
    }

    /**
     * 复制代码块内容。
     * 流程：调用浏览器剪贴板，成功后短暂展示已复制状态；失败时静默恢复，不影响阅读。
     * 参数：sourceKey 为回合稳定键，blockIndex 为块下标，content 为代码块正文。
     * 返回：无返回值。
     */
    function handleCopyBlock(sourceKey: string, blockIndex: number, content: string): void {
        const blockKey = buildBlockKey(sourceKey, blockIndex);
        void navigator.clipboard
            .writeText(content)
            .then(() => {
                copiedBlockKey.value = blockKey;
                if (copiedTimer) window.clearTimeout(copiedTimer);
                copiedTimer = window.setTimeout(() => {
                    if (copiedBlockKey.value === blockKey) copiedBlockKey.value = '';
                }, 1600);
            })
            .catch(() => {
                copiedBlockKey.value = '';
            });
    }

    /**
     * 格式化消息时间。
     * 流程：支持毫秒时间戳和 ISO 字符串；非法或空时间返回空展示。
     * 参数：value 为服务端时间字符串。
     * 返回：本地化后的短日期时间。
     */
    function formatMessageTime(value: string): string {
        if (!value) return '';
        const timestamp = /^\d+$/.test(value) ? Number(value) : Date.parse(value);
        if (!Number.isFinite(timestamp)) return '';
        return new Intl.DateTimeFormat('zh-CN', {
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit'
        }).format(new Date(timestamp));
    }

    onUnmounted(() => {
        Object.values(threadRuntimeById.value).forEach((runtime) => {
            stopThreadRuntime(runtime);
        });
        if (copiedTimer) window.clearTimeout(copiedTimer);
    });
</script>
