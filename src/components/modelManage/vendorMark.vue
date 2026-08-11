<template>
    <span
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border text-[10px] font-semibold"
        :class="markConfig.className">
        {{ markConfig.text }}
    </span>
</template>

<script setup lang="ts">
    import type { ModelVendorKey } from '@/model/modelManage';

    defineOptions({
        name: 'ModelManageVendorMark'
    });

    const props = defineProps<{
        // 原生端返回的供应商标识；未知值按自定义模型展示。
        vendorKey: string;
        // 自定义模型名称，用于没有厂商预设时生成首字徽标。
        label: string;
    }>();

    // 厂商徽标视觉配置，避免直接内置第三方商标文件。
    type VendorMarkConfig = {
        // 徽标显示文本。
        text: string;
        // 徽标对应的 Tailwind 色彩类。
        className: string;
    };

    const vendorMarkConfigMap: Record<ModelVendorKey, VendorMarkConfig> = {
        'xiaomi-asr': { text: 'MI', className: 'border-orange-500/30 bg-orange-500/10 text-orange-300' },
        'xiaomi-text': { text: 'MI', className: 'border-orange-500/30 bg-orange-500/10 text-orange-300' },
        openai: { text: 'AI', className: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-300' },
        deepseek: { text: 'DS', className: 'border-blue-500/30 bg-blue-500/10 text-blue-300' },
        qwen: { text: 'QW', className: 'border-sky-500/30 bg-sky-500/10 text-sky-300' },
        'qwen-asr': { text: 'QA', className: 'border-cyan-500/30 bg-cyan-500/10 text-cyan-300' },
        gemini: { text: 'G', className: 'border-red-500/30 bg-red-500/10 text-red-300' },
        kimi: { text: 'KM', className: 'border-violet-500/30 bg-violet-500/10 text-violet-300' },
        zhipu: { text: 'GL', className: 'border-lime-500/30 bg-lime-500/10 text-lime-300' },
        volcengine: { text: 'ARK', className: 'border-rose-500/30 bg-rose-500/10 text-rose-300' }
    };

    /**
     * 判断供应商标识是否存在于前端预设徽标映射。
     * 流程：检查对象自有属性并收窄字符串类型。
     * 参数：vendorKey 为原生端返回的供应商标识。
     * 返回：存在预设徽标时返回 true。
     * 边界：自定义或未来新增供应商返回 false，页面使用名称首字兜底。
     */
    function isKnownVendorKey(vendorKey: string): vendorKey is ModelVendorKey {
        return Object.prototype.hasOwnProperty.call(vendorMarkConfigMap, vendorKey);
    }

    const markConfig = computed<VendorMarkConfig>(() => {
        if (isKnownVendorKey(props.vendorKey)) return vendorMarkConfigMap[props.vendorKey];
        return {
            text: props.label.trim().slice(0, 1).toUpperCase() || '自',
            className: 'border-border bg-muted text-muted-foreground'
        };
    });
</script>
