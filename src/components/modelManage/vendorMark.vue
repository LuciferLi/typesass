<template>
    <span
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-background/80 p-1"
        :class="markConfig.className">
        <img
            v-if="markConfig.icon"
            :src="markConfig.icon"
            :alt="markConfig.alt"
            class="h-full w-full object-contain"
            loading="lazy"
            draggable="false" />
        <span
            v-else
            class="text-[10px] font-semibold">
            {{ markConfig.text }}
        </span>
    </span>
</template>

<script setup lang="ts">
    import alibabaCloudIcon from '@lobehub/icons-static-svg/icons/alibabacloud-color.svg?url';
    import deepseekIcon from '@lobehub/icons-static-svg/icons/deepseek-color.svg?url';
    import geminiIcon from '@lobehub/icons-static-svg/icons/gemini-color.svg?url';
    import iflytekCloudIcon from '@lobehub/icons-static-svg/icons/iflytekcloud-color.svg?url';
    import kimiIcon from '@lobehub/icons-static-svg/icons/kimi-color.svg?url';
    import openaiIcon from '@lobehub/icons-static-svg/icons/openai.svg?url';
    import qwenIcon from '@lobehub/icons-static-svg/icons/qwen-color.svg?url';
    import tencentCloudIcon from '@lobehub/icons-static-svg/icons/tencentcloud-color.svg?url';
    import volcengineIcon from '@lobehub/icons-static-svg/icons/volcengine-color.svg?url';
    import xiaomiMiMoIcon from '@lobehub/icons-static-svg/icons/xiaomimimo.svg?url';
    import zhipuIcon from '@lobehub/icons-static-svg/icons/zhipu-color.svg?url';

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

    // 厂商徽标视觉配置，优先使用 LobeHub 模型图标库，未知厂商保留字母兜底。
    type VendorMarkConfig = {
        // 徽标显示文本；没有图标资源时作为兜底展示。
        text?: string;
        // LobeHub 静态 SVG 图标资源地址。
        icon?: string;
        // 图标替代文本，供图片不可用和无障碍场景使用。
        alt: string;
        // 徽标对应的 Tailwind 色彩类，用于容器微调或未知厂商兜底。
        className: string;
    };

    const vendorMarkConfigMap: Record<ModelVendorKey, VendorMarkConfig> = {
        xiaomi: { icon: xiaomiMiMoIcon, alt: '小米 MiMo', className: '' },
        openai: { icon: openaiIcon, alt: 'OpenAI', className: '' },
        deepseek: { icon: deepseekIcon, alt: 'DeepSeek', className: '' },
        qwen: { icon: qwenIcon, alt: '阿里通义', className: '' },
        gemini: { icon: geminiIcon, alt: 'Google Gemini', className: '' },
        kimi: { icon: kimiIcon, alt: 'Moonshot Kimi', className: '' },
        zhipu: { icon: zhipuIcon, alt: '智谱 GLM', className: '' },
        volcengine: { icon: volcengineIcon, alt: '火山方舟', className: '' },
        'aliyun-realtime-asr': { icon: alibabaCloudIcon, alt: '阿里实时 ASR', className: '' },
        'tencent-realtime-asr': { icon: tencentCloudIcon, alt: '腾讯云实时 ASR', className: '' },
        'iflytek-realtime-asr': { icon: iflytekCloudIcon, alt: '讯飞实时转写', className: '' }
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
            alt: props.label.trim() || '自定义模型',
            className: 'border-border bg-muted text-muted-foreground'
        };
    });
</script>
