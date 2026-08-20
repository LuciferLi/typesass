import type { SessionTaskAttachmentModel } from '@/model/sessionManage';

/** 任务提示词中的图片资源，用于把 Browser comments 截图从 Markdown 文本转成可预览缩略图。 */
export interface TaskPromptImageModel {
    /** 图片地址，支持 data URL、http(s) 与本地可访问路径。 */
    src: string;
    /** Markdown 图片 alt 文案，作为预览标题和无障碍说明。 */
    alt: string;
}

/**
 * 把任务附件转换成预览图片资源。
 * 流程：读取附件 dataUrl/name 字段生成缩略图模型，供任务卡片和详情弹窗统一展示。
 * 参数：attachments 为任务响应中的结构化附件。
 * 返回：可直接渲染的图片列表。
 * 边界：空列表返回空数组；不从 prompt 文本解析附件。
 */
export function attachmentListToPromptImages(attachments: SessionTaskAttachmentModel[]): TaskPromptImageModel[] {
    return attachments
        .filter((attachment) => attachment.dataUrl)
        .map((attachment) => ({
            alt: attachment.name,
            src: attachment.dataUrl
        }));
}

/**
 * 提取任务提示词中的 Markdown 图片。
 * 流程：扫描 `![alt](src)` 语法，过滤空地址后返回图片列表，供卡片缩略图、详情缩略图和预览弹窗使用。
 * 参数：prompt 为任务提示词原文。
 * 返回：任务提示词中出现的图片资源列表。
 * 边界：不解析普通链接，避免把 Page URL、Target selector 等证据误当成图片。
 */
export function extractPromptImages(prompt: string): TaskPromptImageModel[] {
    const imageList: TaskPromptImageModel[] = [];
    const imagePattern = /!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
    let match: RegExpExecArray | null = imagePattern.exec(prompt);
    while (match) {
        const src = match[2]?.trim() ?? '';
        if (src) {
            imageList.push({
                alt: match[1]?.trim() ?? '',
                src
            });
        }
        match = imagePattern.exec(prompt);
    }
    return imageList;
}

/**
 * 格式化任务内容正文。
 * 流程：移除 Markdown 图片语法，再压缩多余空行，把图片交给缩略图区域展示。
 * 参数：prompt 为任务提示词原文。
 * 返回：适合正文区域展示的纯文本内容。
 * 边界：如果任务内容只有图片，展示短横线保留页面结构。
 */
export function formatPromptText(prompt: string): string {
    const summary = prompt
        .replace(/!\[[^\]]*\]\([^)]+\)/g, '')
        .replace(/\n{3,}/g, '\n\n')
        .trim();
    return summary || '-';
}
