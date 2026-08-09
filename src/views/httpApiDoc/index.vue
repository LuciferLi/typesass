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
            正在读取 App HTTP API 文档。
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

                <ui-accordion
                    v-if="endpointGroups.length"
                    type="multiple"
                    :default-value="defaultModuleAccordionValues"
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
                                :default-value="defaultEndpointAccordionValue(group)"
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
    import { readClientHttpBridgeOpenApi } from '@/service/tauri/command';

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
    }

    const apiDocument = ref<HttpApiOpenApiDocumentModel | null>(null);
    const loading = ref(false);
    const errorMessage = ref('');

    const documentDescription = computed(() => {
        return apiDocument.value?.info.description || '读取客户端本地 HTTP 桥接当前真实开放的接口。';
    });

    const componentSchemas = computed<Record<string, HttpApiSchemaModel>>(() => {
        return apiDocument.value?.components?.schemas ?? {};
    });

    const endpointGroups = computed<HttpApiEndpointGroupModel[]>(() => {
        if (!apiDocument.value) return [];
        const endpoints = collectEndpoints(apiDocument.value);
        return groupEndpoints(apiDocument.value, endpoints);
    });

    const defaultModuleAccordionValues = computed<string[]>(() => endpointGroups.value.map((group) => group.name));

    onMounted(() => {
        loadDocument().catch(() => undefined);
    });

    /**
     * 读取客户端 HTTP API 文档。
     * 流程：清空旧错误，调用客户端桥接读取 OpenAPI 文档，成功后刷新页面数据。
     * 参数：无。
     * 返回：读取完成 Promise。
     * 边界：客户端未启动或接口异常时仅展示错误，不影响其它页面。
     */
    async function loadDocument(): Promise<void> {
        loading.value = true;
        errorMessage.value = '';
        try {
            apiDocument.value = await readClientHttpBridgeOpenApi();
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
     * 获取模块内默认展开的第一个接口。
     * 流程：读取模块第一条接口并生成 Accordion value。
     * 参数：group 为接口模块分组。
     * 返回：默认展开 value，模块为空时返回空字符串。
     * 边界：空模块不会生成可展开接口。
     */
    function defaultEndpointAccordionValue(group: HttpApiEndpointGroupModel): string {
        const [firstEndpoint] = group.endpoints;
        return firstEndpoint ? endpointItemValue(group.name, firstEndpoint) : '';
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
                    schema: null
                };
            }
            return {
                status,
                description: resolvedResponse.description || '响应',
                schema: responseSchema(resolvedResponse)
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
