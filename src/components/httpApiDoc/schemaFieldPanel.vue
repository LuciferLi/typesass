<template>
    <div
        v-if="schema"
        class="grid gap-3">
        <div class="overflow-x-auto rounded border border-border/70">
            <div
                class="grid grid-cols-[minmax(88px,1fr)_72px_48px_minmax(120px,1.4fr)] bg-muted/70 text-[11px] font-medium text-muted-foreground md:grid-cols-[minmax(120px,1fr)_96px_56px_minmax(160px,1.4fr)]">
                <span class="px-2 py-1.5">字段</span>
                <span class="px-2 py-1.5">类型</span>
                <span class="px-2 py-1.5">必填</span>
                <span class="px-2 py-1.5">规则 / 说明</span>
            </div>
            <div
                v-for="field in schemaPropertyRows"
                :key="field.name"
                class="grid grid-cols-[minmax(88px,1fr)_72px_48px_minmax(120px,1.4fr)] border-t border-border/70 text-[11px] md:grid-cols-[minmax(120px,1fr)_96px_56px_minmax(160px,1.4fr)]">
                <code class="break-all px-2 py-1.5 text-foreground">{{ field.name }}</code>
                <span class="px-2 py-1.5 text-muted-foreground">{{ field.type }}</span>
                <span class="px-2 py-1.5 text-muted-foreground">{{ field.required ? '是' : '否' }}</span>
                <span class="px-2 py-1.5 leading-5 text-muted-foreground">{{ field.description }}</span>
            </div>
            <p
                v-if="!schemaPropertyRows.length"
                class="border-t border-border/70 px-2 py-2 text-[12px] text-muted-foreground">
                {{ schemaTypeSummary(schema) }}
            </p>
        </div>
        <pre class="max-h-[240px] overflow-auto rounded bg-muted p-3 text-[11px] leading-5 text-muted-foreground">{{
            formatSchema(schema)
        }}</pre>
    </div>
</template>

<script setup lang="ts">
    import type { HttpApiReferenceModel, HttpApiSchemaModel } from '@/model/httpApiDoc';

    /**
     * Schema 字段面板属性。
     * 业务含义：渲染 OpenAPI schema 的字段表和原始 JSON。
     */
    interface SchemaFieldPanelProps {
        /** 当前请求或响应体 schema。 */
        schema: HttpApiSchemaModel | HttpApiReferenceModel | null;
        /** OpenAPI components.schemas 映射，用于展开 `$ref`。 */
        schemas: Record<string, HttpApiSchemaModel>;
    }

    /**
     * Schema 字段行模型。
     * 业务含义：把 OpenAPI schema 顶层 properties 转成页面表格行。
     */
    interface HttpApiSchemaPropertyRowModel {
        /** 字段名。 */
        name: string;
        /** 字段类型摘要。 */
        type: string;
        /** 字段是否必填。 */
        required: boolean;
        /** 字段规则和说明。 */
        description: string;
    }

    defineOptions({
        name: 'HttpApiDocSchemaFieldPanel'
    });

    const props = defineProps<SchemaFieldPanelProps>();

    const schemaPropertyRows = computed<HttpApiSchemaPropertyRowModel[]>(() => {
        if (!props.schema) return [];
        const resolvedSchema = resolveSchema(props.schema);
        const properties = resolvedSchema?.properties;
        if (!properties) return [];
        const requiredFields = new Set(resolvedSchema.required ?? []);
        return Object.entries(properties).map(([name, fieldSchema]) => ({
            name,
            type: schemaTypeSummary(fieldSchema),
            required: requiredFields.has(name),
            description: schemaRuleSummary(fieldSchema)
        }));
    });

    /**
     * 格式化 schema JSON。
     * 流程：使用 JSON.stringify 保留 OpenAPI 原始结构，方便开发者查看完整规则。
     * 参数：schema 为 OpenAPI schema 或引用。
     * 返回：格式化后的 JSON 字符串。
     * 边界：schema 为空时返回空字符串。
     */
    function formatSchema(schema: HttpApiSchemaModel | HttpApiReferenceModel | null): string {
        if (!schema) return '';
        return JSON.stringify(schema, null, 2);
    }

    /**
     * 判断值是否为 OpenAPI 引用。
     * 流程：检查对象上是否存在字符串 `$ref`。
     * 参数：value 为待判断值。
     * 返回：是否为引用模型。
     * 边界：null 或非对象返回 false。
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
     * 解析 schema 引用。
     * 流程：若 schema 为 `#/components/schemas/Xxx`，从 schemas 映射中读取实际 schema。
     * 参数：schema 为 schema 或引用。
     * 返回：解析后的 schema。
     * 边界：非引用或找不到引用时返回原 schema 或 null。
     */
    function resolveSchema(schema: HttpApiSchemaModel | HttpApiReferenceModel | null): HttpApiSchemaModel | null {
        if (!schema) return null;
        if (!isReference(schema)) return schema;
        const schemaName = schema.$ref.replace('#/components/schemas/', '');
        return props.schemas[schemaName] ?? null;
    }

    /**
     * 生成 schema 类型摘要。
     * 流程：优先展示引用名，再展示 oneOf、array、object 或基础 type。
     * 参数：schema 为 schema 或引用。
     * 返回：短类型说明。
     * 边界：未知 schema 返回 unknown。
     */
    function schemaTypeSummary(schema: HttpApiSchemaModel | HttpApiReferenceModel | null): string {
        if (!schema) return '无';
        if (isReference(schema)) return schema.$ref.replace('#/components/schemas/', '');
        if (schema.oneOf) return 'oneOf';
        if (schema.type === 'array') return `array<${schema.items ? schemaTypeSummary(schema.items) : 'unknown'}>`;
        if (schema.type) return schema.type;
        const resolvedSchema = resolveSchema(schema);
        return resolvedSchema?.type ?? 'unknown';
    }

    /**
     * 生成 schema 字段规则摘要。
     * 流程：拼接 description、enum、const、minLength、minimum、pattern 和 additionalProperties。
     * 参数：schema 为字段 schema 或引用。
     * 返回：用户可读规则说明。
     * 边界：引用 schema 会先解析实际定义；没有规则时返回短横线。
     */
    function schemaRuleSummary(schema: HttpApiSchemaModel | HttpApiReferenceModel | null): string {
        if (!schema) return '-';
        const resolvedSchema = resolveSchema(schema);
        if (!resolvedSchema) return isReference(schema) ? schema.$ref : '-';
        const rules = [
            resolvedSchema.description,
            resolvedSchema.enum ? `枚举：${resolvedSchema.enum.join('、')}` : '',
            resolvedSchema.const !== undefined ? `固定值：${String(resolvedSchema.const)}` : '',
            resolvedSchema.minLength !== undefined ? `最小长度：${resolvedSchema.minLength}` : '',
            resolvedSchema.minimum !== undefined ? `最小值：${resolvedSchema.minimum}` : '',
            resolvedSchema.pattern ? `正则：${resolvedSchema.pattern}` : '',
            resolvedSchema.additionalProperties === false ? '不允许额外字段' : ''
        ].filter((item) => item && item.trim().length > 0);
        return rules.length ? rules.join('；') : '-';
    }
</script>
