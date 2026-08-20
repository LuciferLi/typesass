<template>
    <section class="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-4 overflow-hidden">
        <header class="flex min-w-0 items-start justify-between gap-4">
            <div class="flex min-w-0 items-start gap-3">
                <span
                    class="grid h-9 w-9 shrink-0 place-items-center rounded-md border border-border bg-card text-foreground">
                    <terminal class="h-4 w-4" />
                </span>
                <div class="grid min-w-0 gap-1">
                    <h1 class="text-[18px] font-semibold leading-7 text-foreground">HTTP API 文档</h1>
                    <p class="text-[13px] leading-6 text-muted-foreground">
                        {{ documentDescription }}
                    </p>
                </div>
            </div>
            <button
                type="button"
                class="inline-flex h-8 shrink-0 items-center gap-2 rounded-md border border-border bg-background px-3 text-[13px] text-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="loading"
                @click="loadDocument">
                <refresh :class="['h-3.5 w-3.5', loading ? 'animate-spin' : '']" />
                刷新
            </button>
        </header>

        <div
            v-if="loading"
            class="grid place-items-center rounded-md border border-border text-[13px] text-muted-foreground">
            正在读取公共 HTTP API 文档。
        </div>

        <div
            v-else-if="errorMessage"
            class="grid place-items-center rounded-md border border-border p-8 text-center text-[13px] text-muted-foreground">
            <div class="grid max-w-[520px] gap-2">
                <p class="font-medium text-foreground">文档读取失败</p>
                <p>{{ errorMessage }}</p>
            </div>
        </div>

        <div
            v-else
            class="min-h-0 overflow-y-auto pr-1">
            <div class="grid gap-4">
                <div class="flex flex-wrap items-center gap-2 text-[12px] text-muted-foreground">
                    <span class="rounded-md bg-muted px-2 py-1 text-muted-foreground">
                        OpenAPI {{ apiDocument?.openapi || '-' }}
                    </span>
                    <span class="rounded-md bg-muted px-2 py-1 text-muted-foreground">
                        版本 {{ apiDocument?.info.version || '-' }}
                    </span>
                    <span
                        v-for="server in apiDocument?.servers ?? []"
                        :key="server.url"
                        class="rounded-md border border-border px-2 py-1">
                        {{ server.url }}
                    </span>
                </div>

                <section
                    class="grid gap-3 border-y border-border py-4 text-[12px] leading-5 text-muted-foreground lg:grid-cols-2">
                    <div class="grid content-start gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">接入流程与鉴权</h2>
                        <p>
                            1. 客户端先调用 <code class="text-foreground">GET /health</code>
                            判断本机 App HTTP 服务是否可用；服务未启动时提示用户打开 typesass App。
                        </p>
                        <p>
                            2. 没有授权码的客户端调用
                            <code class="text-foreground">POST /v1/access-tokens/request</code>
                            申请 App 授权码；用户确认后立即返回明文授权码。
                        </p>
                        <p>
                            3. App 授权码统一在系统设置页维护，可手动创建、长期查看、复制和撤销；HTTP API
                            文档页只负责说明契约，不维护授权码列表。
                        </p>
                        <p>
                            4. 内网来源业务接口可免授权码；公网 IP 或公网域名来源必须携带
                            <code class="text-foreground">Authorization: Bearer &lt;App 授权码&gt;</code>。
                        </p>
                        <p>
                            5. 鉴权后先调用 <code class="text-foreground">GET /v1/models</code>，按 capability 选择
                            enabled=true 的不透明 modelId；优先使用 isDefault=true 的对应能力模型。
                        </p>
                        <p>
                            6. 业务请求携带
                            <code class="text-foreground">Authorization: Bearer &lt;App 授权码&gt;</code
                            >；每次尝试生成新的
                            <code class="text-foreground">X-Request-ID</code>；响应也会返回该值，排障时请提供它。
                        </p>
                        <p>
                            7. ASR 与文本请求必须发送目录返回的
                            modelId。上游地址和密钥由可信服务管理，第三方请求不能覆盖。
                        </p>
                    </div>
                    <div class="grid content-start gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">限制、重试与排障</h2>
                        <p>v1 固定正文上限 12 MiB、解码后音频上限 8 MiB、文本上限 20000 字符。</p>
                        <p>授权码由 App 明文维护，支持永久有效或指定到期时间；撤销或过期后公网业务接口统一返回 401。</p>
                        <p>
                            401 检查授权码是否缺失、错误、过期或已撤销；413 缩小请求；422 修正字段；429 严格遵循
                            Retry-After。
                        </p>
                        <p>
                            模型 ID 失效、被禁用或 capability 不匹配时，先刷新 GET
                            /v1/models，再选择同能力的已启用模型；没有可用项时停止请求并提示管理员配置。
                        </p>
                        <p>
                            AI 转换请求只在 retryable=true 时额外重试最多 2 次，每次使用新请求 ID。响应存在 Retry-After
                            时必须优先按该值等待；仅在缺失时使用 1 秒、2 秒加随机抖动的退避。
                        </p>
                        <p>
                            并发/分钟限流按 Retry-After 再请求；DAILY_QUOTA_EXCEEDED 等到 Retry-After
                            指定时间后发起新的业务请求，不受 60 秒转换重试窗口约束。
                        </p>
                        <p>服务不会在响应或日志中记录授权码、模型密钥、音频正文和完整请求正文。</p>
                        <p>
                            浏览器来源参与访问判断；健康检查和授权码申请可以直接访问，模型、任务和 Codex
                            状态等敏感接口按内网/公网来源执行授权码门禁。
                        </p>
                        <p>
                            Chrome/Edge 等浏览器还可能要求用户允许当前站点访问本地网络。该权限由浏览器控制，与 Bearer
                            鉴权相互独立；若 Network 中请求在到达服务前被拦截，请允许本地网络访问后重试。
                        </p>
                    </div>
                </section>

                <section class="grid gap-2 border-b border-border pb-4">
                    <h2 class="text-[14px] font-semibold text-foreground">授权当前 Web 会话</h2>
                    <div class="flex max-w-[680px] flex-wrap gap-2">
                        <button
                            type="button"
                            class="h-9 rounded-md bg-primary px-4 text-[13px] text-primary-foreground"
                            :disabled="exchangingToken"
                            @click="handleRequestAccessToken">
                            {{ exchangingToken ? '申请中' : '申请 App 授权码' }}
                        </button>
                    </div>
                    <p class="text-[12px] text-muted-foreground">{{ integrationTokenMessage }}</p>
                </section>

                <section class="grid gap-3 lg:grid-cols-2">
                    <div class="grid min-w-0 gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">请求 App 授权码</h2>
                        <pre class="overflow-x-auto rounded-md bg-muted p-3 text-[11px] leading-5 text-foreground">{{
                            requestAccessTokenCurlExample
                        }}</pre>
                    </div>
                    <div class="grid min-w-0 gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">系统设置手动创建</h2>
                        <pre class="overflow-x-auto rounded-md bg-muted p-3 text-[11px] leading-5 text-foreground">{{
                            createAccessTokenCurlExample
                        }}</pre>
                    </div>
                    <div class="grid min-w-0 gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">文本处理示例</h2>
                        <pre class="overflow-x-auto rounded-md bg-muted p-3 text-[11px] leading-5 text-foreground">{{
                            textCurlExample
                        }}</pre>
                    </div>
                    <div class="grid min-w-0 gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">语音转写示例</h2>
                        <pre class="overflow-x-auto rounded-md bg-muted p-3 text-[11px] leading-5 text-foreground">{{
                            audioCurlExample
                        }}</pre>
                    </div>
                </section>

                <section class="grid gap-3 border-y border-border py-4">
                    <div class="grid gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">Codex 连接门禁与重启流程</h2>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            调用任务 queue 前先请求
                            <code class="text-foreground">GET /v1/codex/connection</code>
                            获取服务端权威状态。connected=true 才表示可以由 Codex Desktop
                            原生创建会话并发送首条消息；不能只根据 desktopRunning
                            或本机进程存在自行推断连接成功。需要持续展示状态时可轮询：前台页面建议 2 至 5
                            秒一次，页面隐藏后应降低频率，且禁止并发叠加慢请求。
                        </p>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            只有用户理解影响并明确确认后，才调用
                            <code class="text-foreground">POST /v1/codex/connection/restart</code>。202
                            只表示服务端接受了异步重启，不表示已经恢复；随后继续轮询连接接口，直到 connected=true
                            或出现稳定失败状态。canRestart=false
                            时不要展示可执行的重启按钮；重启中、存在运行任务或平台不支持时，按接口返回的 code、message
                            和 requestId 提示用户，禁止自动反复提交重启。
                        </p>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            明确重启时，即使 connected=true 也会真正退出旧
                            Codex。服务端先请求正常退出，再结束仍与重启前进程快照一致的官方 Codex 进程；正常退出和 TERM
                            均失败时，最后会强制结束这些已验证进程。未发送草稿和手工任务可能丢失。只有原监听消失且固定端口不可连接后才会启动新实例；若出现未知进程、新监听者或进程身份变化，服务端会拒绝重启，不会换端口或结束第三方进程。
                        </p>
                    </div>

                    <div class="grid gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">会话管理完整流程</h2>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            先读取工作空间，再把返回的 cwd 原样用于会话搜索；threadId 必须来自搜索结果。打开接口返回
                            <code class="text-foreground">ok=true</code>
                            只表示 Rust 已确认会话存在并向系统提交 CodeX 打开请求，不代表 CodeX 界面已经完成切换。
                        </p>
                        <pre class="overflow-x-auto rounded-md bg-muted p-3 text-[11px] leading-5 text-foreground">{{
                            codexSessionCurlExample
                        }}</pre>
                    </div>

                    <div class="grid gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">任务管理完整流程</h2>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            项目 ID 必须从项目响应取得，任务 ID 必须使用创建响应的 createdTaskId。创建任务只产生
                            created；queue 后通过聚合查询读取 queued、running、waiting_acceptance 或
                            failed，禁止客户端自行推进状态。只有 waiting_acceptance 可以 complete；failed
                            仅在确认未发送且修正原因后才可以重新 queue。
                        </p>
                        <pre class="overflow-x-auto rounded-md bg-muted p-3 text-[11px] leading-5 text-foreground">{{
                            taskManagementCurlExample
                        }}</pre>
                    </div>

                    <div class="grid gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">queue 失败与防重复发送</h2>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            queue 返回 503
                            <code class="text-foreground">CODEX_DESKTOP_NOT_CONNECTED</code>
                            时，服务端尚未修改任务、会话和事件数据。客户端应停止本次操作，展示连接说明和经用户确认的重启入口；连接恢复后由用户重新点击，禁止在后台自动重放。
                        </p>
                        <p class="text-[12px] leading-5 text-muted-foreground">
                            queue 返回 409 <code class="text-foreground">CODEX_SEND_UNCERTAIN</code>
                            表示首条消息可能已经进入
                            Codex，但服务端无法安全确认结果。此状态优先于断连状态，必须立即停止自动重试和人工直接重排，保留
                            taskId、code、requestId 交给维护人员核对真实会话；任何客户端都不得再次发送原 prompt。
                        </p>
                    </div>
                </section>

                <section
                    class="grid gap-3 border-b border-border pb-4 text-[12px] leading-5 text-muted-foreground lg:grid-cols-2">
                    <div class="grid content-start gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">任务状态与写请求重试</h2>
                        <p>
                            正常状态为 created -> queued -> running -> waiting_acceptance -> completed；执行失败进入
                            failed。
                        </p>
                        <p>
                            创建、更新、删除、queue 和 complete 不承诺 HTTP 幂等。遇到 503、504 或连接中断时，先调用
                            <code class="text-foreground">POST /v1/task-workspace/query</code>
                            核对真实项目和任务状态，再由用户决定下一步，禁止自动原样重放写请求。
                        </p>
                        <p>
                            只读工作空间、会话搜索和任务聚合仅在
                            <code class="text-foreground">retryable=true</code>
                            时退避重试，建议 500 ms 起步、加入随机抖动、最多额外重试 2 次；每次尝试生成新的
                            X-Request-ID。
                        </p>
                    </div>
                    <div class="grid content-start gap-2">
                        <h2 class="text-[14px] font-semibold text-foreground">容量与当前边界</h2>
                        <p>
                            最多 200 个项目；任务和会话历史不按条数设上限；任务聚合业务 JSON 预算 7 MiB，私有 RPC
                            响应硬上限 8 MiB。达到上限会返回稳定错误，不会截断后伪装成功。
                        </p>
                        <p>
                            CodeX 会话搜索每页 1 到 60 条；项目名最多 100 个 Unicode 字符，任务标题最多 200 个 Unicode
                            字符，prompt 最多 50000 个 Unicode 字符。
                        </p>
                        <p>
                            v1 不提供取消任务、SSE、会话正文、删除历史任务、批量操作或级联删除。第三方不得直连 Tauri
                            业务 command、私有 Socket、任务 SQLite 或 CodeX 来补造这些能力。
                        </p>
                    </div>
                </section>

                <ui-accordion
                    v-if="endpointGroups.length"
                    type="multiple"
                    class="grid gap-3">
                    <ui-accordion-item
                        v-for="group in endpointGroups"
                        :key="group.name"
                        :value="group.name"
                        class="overflow-hidden rounded-md border border-border bg-card px-4">
                        <ui-accordion-trigger class="py-4 hover:no-underline">
                            <div class="flex min-w-0 items-center gap-3">
                                <span
                                    class="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-muted text-foreground">
                                    <list class="h-4 w-4" />
                                </span>
                                <span class="grid min-w-0 gap-1">
                                    <span class="flex min-w-0 flex-wrap items-center gap-2">
                                        <span class="truncate text-[15px] font-semibold text-foreground">
                                            {{ group.name }}
                                        </span>
                                        <span
                                            class="rounded-md border border-border bg-background px-2 py-0.5 text-[11px] font-normal text-muted-foreground">
                                            {{ group.endpoints.length }} 个接口
                                        </span>
                                    </span>
                                    <span
                                        v-if="group.description"
                                        class="line-clamp-2 text-[12px] font-normal leading-5 text-muted-foreground">
                                        {{ group.description }}
                                    </span>
                                </span>
                            </div>
                        </ui-accordion-trigger>
                        <ui-accordion-content>
                            <ui-accordion
                                type="single"
                                collapsible
                                class="grid gap-2">
                                <ui-accordion-item
                                    v-for="endpoint in group.endpoints"
                                    :key="`${endpoint.method}:${endpoint.path}`"
                                    :value="endpointItemValue(group.name, endpoint)"
                                    class="overflow-hidden rounded-md border border-border/70 bg-background px-3">
                                    <ui-accordion-trigger class="py-3 hover:no-underline">
                                        <div class="flex min-w-0 items-start gap-3">
                                            <span
                                                class="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-md bg-muted text-foreground">
                                                <terminal class="h-3.5 w-3.5" />
                                            </span>
                                            <span class="grid min-w-0 gap-1">
                                                <span class="flex min-w-0 flex-wrap items-center gap-2">
                                                    <span :class="methodBadgeClass(endpoint.method)">
                                                        {{ endpoint.method }}
                                                    </span>
                                                    <code
                                                        class="break-all rounded bg-muted px-2 py-1 text-[12px] font-normal text-foreground">
                                                        {{ endpoint.path }}
                                                    </code>
                                                </span>
                                                <span class="text-left text-[13px] font-medium text-foreground">
                                                    {{ endpoint.summary }}
                                                </span>
                                                <span
                                                    v-if="endpoint.description"
                                                    class="line-clamp-2 text-left text-[12px] font-normal leading-5 text-muted-foreground">
                                                    {{ endpoint.description }}
                                                </span>
                                            </span>
                                        </div>
                                    </ui-accordion-trigger>
                                    <ui-accordion-content>
                                        <div class="grid items-start gap-3 pb-1 lg:grid-cols-2">
                                            <section
                                                class="grid content-start gap-2 rounded-md border border-border/70 p-3">
                                                <h3
                                                    class="flex items-center gap-2 text-[12px] font-medium text-foreground">
                                                    <terminal class="h-3.5 w-3.5" />
                                                    入参
                                                </h3>
                                                <p class="text-[12px] leading-5 text-muted-foreground">
                                                    鉴权：{{ endpointSecuritySummary(endpoint) }}
                                                </p>
                                                <p
                                                    v-for="parameter in endpoint.operation.parameters ?? []"
                                                    :key="`${parameter.in}:${parameter.name}`"
                                                    class="text-[12px] leading-5 text-muted-foreground">
                                                    {{ parameter.in }} 参数 {{ parameter.name }}：{{
                                                        parameter.description || '无说明'
                                                    }}
                                                </p>
                                                <template v-if="requestSchema(endpoint)">
                                                    <p class="text-[12px] leading-5 text-muted-foreground">
                                                        请求体{{
                                                            requestSchema(endpoint)?.required ? '必填' : '可选'
                                                        }}，格式
                                                        {{ requestSchema(endpoint)?.contentType }}
                                                    </p>
                                                    <http-api-doc-schema-field-panel
                                                        :schema="requestSchema(endpoint)?.schema ?? null"
                                                        :schemas="componentSchemas" />
                                                </template>
                                                <p
                                                    v-else
                                                    class="text-[12px] leading-5 text-muted-foreground">
                                                    无请求体。
                                                </p>
                                            </section>

                                            <section
                                                class="grid content-start gap-2 rounded-md border border-border/70 p-3">
                                                <h3
                                                    class="flex items-center gap-2 text-[12px] font-medium text-foreground">
                                                    <list class="h-3.5 w-3.5" />
                                                    出参
                                                </h3>
                                                <div class="grid gap-3">
                                                    <p class="text-[12px] leading-5 text-muted-foreground">
                                                        稳定错误码：{{ endpointErrorCodes(endpoint) }}
                                                    </p>
                                                    <article
                                                        v-for="response in endpointResponses(endpoint)"
                                                        :key="response.status"
                                                        class="grid gap-2 rounded-md bg-muted/30 p-2">
                                                        <div class="flex flex-wrap items-center gap-2">
                                                            <span
                                                                class="rounded-md bg-muted px-2 py-0.5 text-[11px] font-medium text-foreground">
                                                                {{ response.status }}
                                                            </span>
                                                            <span class="text-[12px] text-muted-foreground">
                                                                {{ response.description }}
                                                            </span>
                                                        </div>
                                                        <http-api-doc-schema-field-panel
                                                            v-if="response.schema"
                                                            :schema="response.schema"
                                                            :schemas="componentSchemas" />
                                                        <p
                                                            v-for="header in response.headers"
                                                            :key="header.name"
                                                            class="text-[11px] leading-5 text-muted-foreground">
                                                            Header {{ header.name }}：{{ header.description }}
                                                        </p>
                                                        <pre
                                                            v-if="response.example"
                                                            class="overflow-x-auto rounded bg-muted p-2 text-[10px] leading-4 text-foreground"
                                                            >{{ response.example }}</pre
                                                        >
                                                    </article>
                                                    <p
                                                        v-if="!endpointResponses(endpoint).length"
                                                        class="text-[12px] leading-5 text-muted-foreground">
                                                        未声明响应。
                                                    </p>
                                                </div>
                                            </section>
                                        </div>
                                    </ui-accordion-content>
                                </ui-accordion-item>
                            </ui-accordion>
                        </ui-accordion-content>
                    </ui-accordion-item>
                </ui-accordion>

                <div
                    v-else
                    class="grid place-items-center rounded-md border border-border p-8 text-[13px] text-muted-foreground">
                    当前文档没有可展示的 HTTP 接口。
                </div>
            </div>
        </div>
    </section>
</template>

<script setup lang="ts">
    import { List, Refresh, Terminal } from '@icon-park/vue-next';

    import HttpApiDocSchemaFieldPanel from '@/components/httpApiDoc/schemaFieldPanel.vue';
    import {
        Accordion as UiAccordion,
        AccordionContent as UiAccordionContent,
        AccordionItem as UiAccordionItem,
        AccordionTrigger as UiAccordionTrigger
    } from '@/components/ui/accordion';
    import type {
        HttpApiEndpointModel,
        HttpApiOpenApiDocumentModel,
        HttpApiReferenceModel,
        HttpApiResponseModel,
        HttpApiSchemaModel
    } from '@/model/httpApiDoc';
    import { getPublicApiToken, readPublicApiOpenApi, requestPublicApiAccessToken } from '@/service/tauri/command';

    defineOptions({
        name: 'HttpApiDocView'
    });

    /**
     * API 文档模块分组模型。
     * 业务含义：把 OpenAPI 的路径操作按 tag 聚合，供页面分模块展示。
     */
    interface HttpApiEndpointGroupModel {
        /** 模块名称。 */
        name: string;
        /** 模块说明。 */
        description: string;
        /** 模块下的接口列表。 */
        endpoints: HttpApiEndpointModel[];
    }

    /**
     * 请求 schema 展示模型。
     * 业务含义：把 OpenAPI requestBody 转为页面可渲染的内容类型、必填状态和 schema。
     */
    interface HttpApiRequestSchemaModel {
        /** 请求体是否必填。 */
        required: boolean;
        /** 请求体 MIME 类型。 */
        contentType: string;
        /** 请求体 schema。 */
        schema: HttpApiSchemaModel | HttpApiReferenceModel | null;
    }

    /**
     * 响应 schema 展示模型。
     * 业务含义：把 OpenAPI responses 转为页面可渲染的状态码、说明和 schema。
     */
    interface HttpApiResponseSchemaModel {
        /** HTTP 状态码或 default。 */
        status: string;
        /** 响应说明。 */
        description: string;
        /** 响应体 schema。 */
        schema: HttpApiSchemaModel | HttpApiReferenceModel | null;
        /** 响应 Header 说明。 */
        headers: Array<{ name: string; description: string }>;
        /** 格式化 JSON 示例。 */
        example: string;
    }

    const apiDocument = ref<HttpApiOpenApiDocumentModel | null>(null);
    const loading = ref(false);
    const errorMessage = ref('');
    const exchangingToken = ref(false);
    const integrationTokenMessage = ref('正在检查当前运行会话授权码。');

    const documentDescription = computed(() => {
        return apiDocument.value?.info.description || '读取独立 HTTP 服务当前真实开放的接口。';
    });

    const componentSchemas = computed<Record<string, HttpApiSchemaModel>>(() => {
        return apiDocument.value?.components?.schemas ?? {};
    });

    const endpointGroups = computed<HttpApiEndpointGroupModel[]>(() => {
        if (!apiDocument.value) return [];
        const endpoints = collectEndpoints(apiDocument.value);
        return groupEndpoints(apiDocument.value, endpoints);
    });
    const documentServerUrl = computed(() => apiDocument.value?.servers?.[0]?.url || 'http://127.0.0.1:18080');
    const requestAccessTokenCurlExample = computed(
        () => `curl -X POST '${documentServerUrl.value}/v1/access-tokens/request' \\
  -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-access-token-request-001' \\
  -d '{"name":"Chrome 插件","expiresAt":null}'`
    );
    const createAccessTokenCurlExample = computed(
        () => `curl -X POST '${documentServerUrl.value}/v1/access-tokens' \\
  -H 'Authorization: Bearer <APP_ACCESS_TOKEN>' \\
  -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: app-access-token-create-001' \\
  -d '{"name":"官网后台","expiresAt":null}'`
    );
    const textCurlExample = computed(
        () => `curl -X POST '${documentServerUrl.value}/v1/text/process' \\
  -H 'Authorization: Bearer <APP_ACCESS_TOKEN>' \\
  -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-order-20260810-001' \\
  -d '{"modelId":"<ENABLED_TEXT_MODEL_ID>","mode":"polish","text":"需要润色的文本","audioDurationMs":0,"dictionary":[],"contextApp":"partner-web","styleInstruction":"表达简洁"}'`
    );
    const audioCurlExample = computed(
        () => `curl -X POST '${documentServerUrl.value}/v1/audio/transcriptions' \\
  -H 'Authorization: Bearer <APP_ACCESS_TOKEN>' \\
  -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-audio-20260810-001' \\
  -d '{"modelId":"<ENABLED_ASR_MODEL_ID>","audioBase64":"<BASE64_AUDIO>","contentType":"audio/wav","language":"auto"}'`
    );
    const codexSessionCurlExample = computed(
        () => `BASE_URL='${documentServerUrl.value}'
TOKEN='<APP_ACCESS_TOKEN>'

# 1. 从响应选择真实 cwd
curl "$BASE_URL/v1/codex/workspaces" \\
  -H "Authorization: Bearer $TOKEN" \\
  -H 'X-Request-ID: partner-workspaces-001'

# 2. cwd 原样用于搜索；limit=1..60、offset>=0
curl -X POST "$BASE_URL/v1/codex/threads/search" \\
  -H "Authorization: Bearer $TOKEN" \\
  -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-thread-search-001' \\
  -d '{"workspaceCwd":"/Users/demo/Documents/project-a","limit":20,"offset":0,"keyword":"接口文档"}'

# 3. threadId 必须来自搜索响应；不存在时返回 CODEX_THREAD_NOT_FOUND
curl -X POST "$BASE_URL/v1/codex/threads/<THREAD_ID>/open" \\
  -H "Authorization: Bearer $TOKEN" \\
  -H 'X-Request-ID: partner-thread-open-001'`
    );
    const taskManagementCurlExample = computed(
        () => `BASE_URL='${documentServerUrl.value}'
TOKEN='<APP_ACCESS_TOKEN>'

# 1. 创建项目，并从响应 projects 中取得 PROJECT_ID
PROJECT_RESPONSE="$(curl -fsS -X POST "$BASE_URL/v1/projects" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-project-create-001' \\
  -d '{"name":"AI 工具接口接入","workspacePath":"/Users/demo/Documents/project-a"}')"
PROJECT_ID="$(printf '%s' "$PROJECT_RESPONSE" | jq -r '.projects[] | select(.name == "AI 工具接口接入") | .id')"

# 2. 创建 created 任务，并直接读取本次事务返回的 createdTaskId
TASK_RESPONSE="$(curl -fsS -X POST "$BASE_URL/v1/tasks" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-task-create-001' \\
  -d "$(jq -nc --arg projectId "$PROJECT_ID" '{projectId:$projectId,title:"完善 HTTP 接口文档",prompt:"补齐接口契约和错误码。"}')")"
TASK_ID="$(printf '%s' "$TASK_RESPONSE" | jq -er '.createdTaskId')"

# 3. 进入队列；failed 任务修正原因后也使用同一接口重新排队
curl -X POST "$BASE_URL/v1/tasks/$TASK_ID/queue" \\
  -H "Authorization: Bearer $TOKEN" \\
  -H 'X-Request-ID: partner-task-queue-001'

# 4. 轮询权威状态和 resultJson；不要在客户端自行推进状态
curl -X POST "$BASE_URL/v1/task-workspace/query" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -H 'X-Request-ID: partner-task-poll-001' \\
  -d "$(jq -nc --arg projectId "$PROJECT_ID" '{projectId:$projectId}')" \\
  | jq --arg taskId "$TASK_ID" '.tasks[] | select(.id == $taskId) | {status,lastError,resultJson}'

# 5. 仅 waiting_acceptance 可以验收；resultJson 是需要再次 JSON.parse 的字符串
curl -X POST "$BASE_URL/v1/tasks/$TASK_ID/complete" \\
  -H "Authorization: Bearer $TOKEN" \\
  -H 'X-Request-ID: partner-task-complete-001'`
    );

    onMounted(async () => {
        try {
            integrationTokenMessage.value = (await getPublicApiToken())
                ? '当前运行会话已配置 App 授权码；公网来源业务请求会自动携带。'
                : '尚未配置 App 授权码；内网来源可访问业务接口，公网来源会返回 401。';
        } catch (error) {
            integrationTokenMessage.value = error instanceof Error ? error.message : '读取当前授权状态失败。';
        }
        await loadDocument();
    });

    /**
     * 为当前 Web 会话申请 App 授权码。
     * 流程：调用授权码申请接口，服务端在 App 用户确认后直接返回明文授权码，并保存到当前运行会话。
     * 参数：无。
     * 返回：申请完成 Promise。
     * 边界：不生成设备码、不保存 pending、不轮询授权结果。
     */
    async function handleRequestAccessToken(): Promise<void> {
        if (exchangingToken.value) return;
        exchangingToken.value = true;
        try {
            const response = await requestPublicApiAccessToken('HTTP API 文档调试会话', null);
            integrationTokenMessage.value = response.expiresAt
                ? `App 授权码已应用到当前运行会话，到期时间：${response.expiresAt}。`
                : 'App 授权码已应用到当前运行会话，当前授权码为永久有效。';
        } catch (error) {
            integrationTokenMessage.value = error instanceof Error ? error.message : '授权码申请失败。';
        } finally {
            exchangingToken.value = false;
        }
    }

    /**
     * 读取接口鉴权方案摘要。
     * 流程：展开 OpenAPI security 对象中的方案名称并合并为页面文案。
     * 参数：endpoint 为当前接口操作。
     * 返回：鉴权方案列表；空 security 返回“无需鉴权”。
     * 边界：未知方案只展示服务端原始名称，不在前端猜测权限。
     */
    function endpointSecuritySummary(endpoint: HttpApiEndpointModel): string {
        if (!endpoint.operation.security?.length) return '无需鉴权';
        return endpoint.operation.security.flatMap((item) => Object.keys(item)).join('、') || '无需鉴权';
    }

    /**
     * 读取服务端声明的稳定业务错误码。
     * 流程：按状态码展开 x-error-codes，并组合 code、可重试性和处理建议。
     * 参数：endpoint 为当前接口操作。
     * 返回：适合第三方阅读的错误码摘要。
     * 边界：服务端未声明扩展字段时明确显示无额外业务错误码。
     */
    function endpointErrorCodes(endpoint: HttpApiEndpointModel): string {
        const errorCodes = endpoint.operation['x-error-codes'];
        if (!errorCodes) return '无额外业务错误码';
        return Object.entries(errorCodes)
            .flatMap(([status, items]) =>
                items.map(
                    (item) => `${status}/${item.code}（${item.retryable ? '可重试' : '不可重试'}：${item.action}）`
                )
            )
            .join('；');
    }

    /**
     * 读取公共 HTTP API 文档。
     * 流程：清空旧错误，调用独立服务读取自动生成的 OpenAPI 文档，成功后刷新页面数据。
     * 参数：无。
     * 返回：读取完成 Promise。
     * 边界：服务未启动或接口异常时仅展示错误，不影响其它页面。
     */
    async function loadDocument(): Promise<void> {
        loading.value = true;
        errorMessage.value = '';
        try {
            apiDocument.value = await readPublicApiOpenApi();
        } catch (error) {
            errorMessage.value = error instanceof Error ? error.message : String(error);
        } finally {
            loading.value = false;
        }
    }

    /**
     * 收集 OpenAPI 文档里的接口操作。
     * 流程：遍历 paths 下支持的 HTTP 方法，把 operation 拍平成接口列表。
     * 参数：document 为客户端返回的 OpenAPI 文档。
     * 返回：页面可渲染的接口列表。
     * 边界：缺少 tag 或 summary 时使用兜底展示文案。
     */
    function collectEndpoints(document: HttpApiOpenApiDocumentModel): HttpApiEndpointModel[] {
        const methods = ['get', 'post', 'options'] as const;
        return Object.entries(document.paths).flatMap(([path, pathItem]) => {
            return methods.flatMap((method) => {
                const operation = pathItem[method];
                if (!operation) return [];
                const tag = operation.tags?.[0] || '未分组';
                return [
                    {
                        path,
                        method: method.toUpperCase(),
                        tag,
                        summary: operation.summary || `${method.toUpperCase()} ${path}`,
                        description: operation.description || '',
                        operation
                    }
                ];
            });
        });
    }

    /**
     * 按 tag 分组接口列表。
     * 流程：优先使用 OpenAPI tags 的顺序和说明，再补充文档中未声明的 tag。
     * 参数：document 为 OpenAPI 文档；endpoints 为已拍平的接口列表。
     * 返回：按模块组织后的接口分组。
     * 边界：没有接口时返回空数组。
     */
    function groupEndpoints(
        document: HttpApiOpenApiDocumentModel,
        endpoints: HttpApiEndpointModel[]
    ): HttpApiEndpointGroupModel[] {
        const endpointMap = new Map<string, HttpApiEndpointModel[]>();
        endpoints.forEach((endpoint) => {
            endpointMap.set(endpoint.tag, [...(endpointMap.get(endpoint.tag) || []), endpoint]);
        });

        const declaredGroups = (document.tags || [])
            .map((tag) => ({
                name: tag.name,
                description: tag.description || '',
                endpoints: endpointMap.get(tag.name) || []
            }))
            .filter((group) => group.endpoints.length > 0);

        const declaredNames = new Set(declaredGroups.map((group) => group.name));
        const extraGroups = [...endpointMap.entries()]
            .filter(([name]) => !declaredNames.has(name))
            .map(([name, groupEndpointsValue]) => ({
                name,
                description: '',
                endpoints: groupEndpointsValue
            }));

        return [...declaredGroups, ...extraGroups];
    }

    /**
     * 生成接口 Accordion 的唯一值。
     * 流程：组合模块名、方法和路径，避免不同模块下同名接口互相影响展开状态。
     * 参数：groupName 为模块名；endpoint 为接口行。
     * 返回：Accordion item value。
     * 边界：路径包含特殊字符时仍作为普通字符串使用。
     */
    function endpointItemValue(groupName: string, endpoint: HttpApiEndpointModel): string {
        return `${groupName}:${endpoint.method}:${endpoint.path}`;
    }

    /**
     * 解析请求体 schema。
     * 流程：优先读取 application/json，其次读取 OpenAPI content 中第一种媒体类型。
     * 参数：endpoint 为接口行。
     * 返回：请求体展示模型；无请求体时返回 null。
     * 边界：声明了 requestBody 但未声明 schema 时仍展示媒体类型和必填状态。
     */
    function requestSchema(endpoint: HttpApiEndpointModel): HttpApiRequestSchemaModel | null {
        const { requestBody } = endpoint.operation;
        if (!requestBody?.content) return null;
        const jsonSchema = requestBody.content['application/json']?.schema;
        if (jsonSchema) {
            return {
                required: Boolean(requestBody.required),
                contentType: 'application/json',
                schema: jsonSchema
            };
        }
        const firstContent = Object.entries(requestBody.content)[0];
        if (!firstContent) return null;
        return {
            required: Boolean(requestBody.required),
            contentType: firstContent[0],
            schema: firstContent[1].schema ?? null
        };
    }

    /**
     * 解析接口响应 schema 列表。
     * 流程：遍历 responses，解析公共响应引用并读取 JSON 或首个 content schema。
     * 参数：endpoint 为接口行。
     * 返回：响应展示模型列表。
     * 边界：未声明 responses 时返回空数组；无法解析引用时展示引用路径。
     */
    function endpointResponses(endpoint: HttpApiEndpointModel): HttpApiResponseSchemaModel[] {
        return Object.entries(endpoint.operation.responses || {}).map(([status, response]) => {
            const resolvedResponse = resolveResponse(response);
            if (!resolvedResponse) {
                return {
                    status,
                    description: isReference(response) ? response.$ref : '响应',
                    schema: null,
                    headers: [],
                    example: ''
                };
            }
            return {
                status,
                description: resolvedResponse.description || '响应',
                schema: responseSchema(resolvedResponse),
                headers: Object.entries(resolvedResponse.headers || {}).map(([name, header]) => ({
                    name,
                    description: isReference(header) ? header.$ref : header.description || '无说明'
                })),
                example: responseExample(resolvedResponse)
            };
        });
    }

    /**
     * 解析公共响应引用。
     * 流程：普通响应直接返回；`#/components/responses/Xxx` 从文档 components 中查找。
     * 参数：response 为响应或引用。
     * 返回：解析后的响应模型。
     * 边界：引用目标不存在时返回 null。
     */
    function resolveResponse(response: HttpApiResponseModel | HttpApiReferenceModel): HttpApiResponseModel | null {
        if (!isReference(response)) return response;
        const responseName = response.$ref.replace('#/components/responses/', '');
        return apiDocument.value?.components?.responses?.[responseName] ?? null;
    }

    /**
     * 读取响应体 schema。
     * 流程：优先读取 application/json，其次读取响应 content 的第一种媒体类型。
     * 参数：response 为已解析响应。
     * 返回：响应 schema；无响应体时返回 null。
     * 边界：204 等无 body 响应会返回 null。
     */
    function responseSchema(response: HttpApiResponseModel): HttpApiSchemaModel | HttpApiReferenceModel | null {
        if (!response.content) return null;
        return response.content['application/json']?.schema || Object.values(response.content)[0]?.schema || null;
    }

    /**
     * 读取响应 JSON 示例。
     * 流程：优先读取 application/json example，其次读取首个命名 example，并格式化为可复制 JSON。
     * 参数：response 为已解析响应。
     * 返回：格式化示例；未声明时返回空字符串。
     * 边界：示例不是 JSON 原生值时仍由 JSON.stringify 安全转换。
     */
    function responseExample(response: HttpApiResponseModel): string {
        const media = response.content?.['application/json'] || Object.values(response.content || {})[0];
        const example = media?.example ?? Object.values(media?.examples || {})[0]?.value;
        return example === undefined ? '' : JSON.stringify(example, null, 2);
    }

    /**
     * 判断 OpenAPI 节点是否为引用对象。
     * 流程：检查节点是否包含字符串类型 `$ref` 字段。
     * 参数：value 为待判断的 schema 或 response。
     * 返回：为引用对象时返回 true。
     * 边界：空值或普通对象返回 false。
     */
    function isReference(value: unknown): value is HttpApiReferenceModel {
        return (
            typeof value === 'object' &&
            value !== null &&
            '$ref' in value &&
            typeof (value as { $ref?: unknown }).$ref === 'string'
        );
    }

    /**
     * 获取请求方法标签样式。
     * 流程：按 HTTP 方法返回不同色彩的 Tailwind class，提升接口列表扫描效率。
     * 参数：method 为大写 HTTP 方法。
     * 返回：方法标签 class 字符串。
     * 边界：未知方法使用中性标签样式。
     */
    function methodBadgeClass(method: string): string {
        const baseClass = 'rounded px-2 py-1 text-[11px] font-medium';
        if (method === 'GET') return `${baseClass} bg-emerald-500/15 text-emerald-700 dark:text-emerald-300`;
        if (method === 'POST') return `${baseClass} bg-primary/15 text-primary`;
        if (method === 'OPTIONS') return `${baseClass} bg-amber-500/15 text-amber-700 dark:text-amber-300`;
        return `${baseClass} bg-muted text-muted-foreground`;
    }
</script>
