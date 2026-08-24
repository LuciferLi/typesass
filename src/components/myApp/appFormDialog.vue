<template>
    <ui-dialog
        :open="open"
        @update:open="emit('update:open', $event)">
        <ui-dialog-content
            class="grid max-h-[min(720px,calc(100vh-2rem))] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 overflow-hidden p-0 sm:max-w-[560px]">
            <ui-dialog-header class="px-6 pb-4 pt-6">
                <ui-dialog-title>{{ title }}</ui-dialog-title>
                <ui-dialog-description class="sr-only">创建或修改我的应用。</ui-dialog-description>
            </ui-dialog-header>
            <form
                class="contents"
                @submit.prevent="handleSubmit">
                <div class="min-h-0 overflow-y-auto px-6 py-1">
                    <ui-field-group>
                        <ui-field>
                            <ui-field-label>应用 logo</ui-field-label>
                            <div class="flex items-center gap-3">
                                <button
                                    class="group relative grid h-12 w-12 shrink-0 place-items-center overflow-hidden rounded-lg border border-border bg-muted transition-colors hover:border-primary/55 hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                    type="button"
                                    title="上传 logo"
                                    @click="handleSelectLogo">
                                    <img
                                        v-if="form.logoDataUrl"
                                        class="h-full w-full object-cover"
                                        :src="form.logoDataUrl"
                                        alt="" />
                                    <application-menu
                                        v-else
                                        theme="outline"
                                        size="22"
                                        class="text-muted-foreground" />
                                    <span
                                        class="absolute inset-0 grid place-items-center bg-background/65 text-[11px] font-medium text-foreground opacity-0 backdrop-blur-sm transition-opacity group-hover:opacity-100">
                                        {{ form.logoDataUrl ? '更换' : '上传' }}
                                    </span>
                                </button>
                                <div class="grid min-w-0 flex-1 gap-1.5">
                                    <input
                                        ref="logoInputRef"
                                        class="hidden"
                                        accept="image/png,image/jpeg,image/webp,image/svg+xml"
                                        type="file"
                                        @change="handleLogoChanged" />
                                    <ui-field-description>支持 png、jpeg、webp、svg，可不上传。</ui-field-description>
                                </div>
                            </div>
                        </ui-field>

                        <ui-field>
                            <ui-field-label>应用名称</ui-field-label>
                            <ui-input
                                v-model="form.name"
                                maxlength="80"
                                placeholder="请输入应用名称" />
                        </ui-field>

                        <ui-field>
                            <ui-field-label>访问方式</ui-field-label>
                            <ui-select-root v-model="form.accessType">
                                <ui-select-trigger>
                                    <ui-select-value placeholder="选择访问方式" />
                                </ui-select-trigger>
                                <ui-select-content>
                                    <ui-select-item value="local">本地服务器托管</ui-select-item>
                                    <ui-select-item value="remote">远程 URL 访问</ui-select-item>
                                </ui-select-content>
                            </ui-select-root>
                        </ui-field>

                        <template v-if="form.accessType === 'local'">
                            <ui-field>
                                <ui-field-label>服务端口</ui-field-label>
                                <div class="flex gap-2">
                                    <ui-input
                                        v-model="form.port"
                                        inputmode="numeric"
                                        placeholder="例如 18123" />
                                    <ui-button
                                        class="shrink-0"
                                        variant="outline"
                                        type="button"
                                        :disabled="allocatingPort || saving"
                                        @click="handleAllocatePort">
                                        {{ allocatingPort ? '分配中' : '自动分配' }}
                                    </ui-button>
                                </div>
                                <ui-field-description
                                    >创建后会固定使用该端口；服务监听 0.0.0.0，允许局域网访问。</ui-field-description
                                >
                            </ui-field>

                            <ui-field>
                                <ui-field-label>公网二级域名</ui-field-label>
                                <div class="flex items-center gap-2">
                                    <ui-input
                                        v-model="form.publicSubdomain"
                                        maxlength="63"
                                        placeholder="例如 demo" />
                                    <span class="shrink-0 text-[13px] text-muted-foreground">.tolern.com</span>
                                </div>
                                <ui-field-description>
                                    填写后启动服务会自动开放 https://二级域名.tolern.com；留空则仅本机和局域网访问。
                                </ui-field-description>
                            </ui-field>

                            <ui-field>
                                <ui-field-label>静态页面 zip</ui-field-label>
                                <button
                                    class="flex min-h-14 w-full items-center gap-3 rounded-lg border border-dashed border-border bg-muted/25 px-3 py-3 text-left transition-colors hover:border-primary/55 hover:bg-accent/45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                    type="button"
                                    @click="handleSelectZip">
                                    <span
                                        class="grid h-9 w-9 shrink-0 place-items-center rounded-md border border-border bg-background text-muted-foreground">
                                        <file-zip
                                            v-if="form.zipFileName"
                                            theme="outline"
                                            size="19" />
                                        <upload
                                            v-else
                                            theme="outline"
                                            size="18" />
                                    </span>
                                    <span class="grid min-w-0 flex-1 gap-0.5">
                                        <span class="truncate text-[13px] font-medium text-foreground">
                                            {{ form.zipFileName || '请上传 zip 压缩包' }}
                                        </span>
                                        <span class="truncate text-[12px] text-muted-foreground">
                                            {{
                                                form.zipFileName
                                                    ? '点击可重新上传'
                                                    : '根目录或第一层目录需要包含 index.html'
                                            }}
                                        </span>
                                    </span>
                                </button>
                                <input
                                    ref="zipInputRef"
                                    class="hidden"
                                    accept=".zip,application/zip,application/x-zip-compressed"
                                    type="file"
                                    @change="handleZipChanged" />
                                <ui-field-description>
                                    {{ zipDescription }}
                                </ui-field-description>
                            </ui-field>
                        </template>

                        <ui-field v-else>
                            <ui-field-label>远程 URL</ui-field-label>
                            <ui-input
                                v-model="form.remoteUrl"
                                placeholder="https://example.com/dashboard" />
                        </ui-field>
                    </ui-field-group>

                    <p
                        v-if="errorMessage"
                        class="mt-4 text-[13px] leading-5 text-destructive"
                        role="alert">
                        {{ errorMessage }}
                    </p>
                </div>

                <ui-dialog-footer class="border-t border-border px-6 pb-6 pt-4">
                    <ui-button
                        variant="outline"
                        type="button"
                        :disabled="saving"
                        @click="emit('update:open', false)">
                        取消
                    </ui-button>
                    <ui-button
                        type="submit"
                        :disabled="saving">
                        {{ saving ? '保存中' : '保存' }}
                    </ui-button>
                </ui-dialog-footer>
            </form>
        </ui-dialog-content>
    </ui-dialog>
</template>

<script setup lang="ts">
    import { ApplicationMenu, FileZip, Upload } from '@icon-park/vue-next';

    import { Button as UiButton } from '@/components/ui/button';
    import {
        Dialog as UiDialog,
        DialogContent as UiDialogContent,
        DialogDescription as UiDialogDescription,
        DialogFooter as UiDialogFooter,
        DialogHeader as UiDialogHeader,
        DialogTitle as UiDialogTitle
    } from '@/components/ui/dialog';
    import {
        Field as UiField,
        FieldDescription as UiFieldDescription,
        FieldGroup as UiFieldGroup,
        FieldLabel as UiFieldLabel
    } from '@/components/ui/field';
    import { Input as UiInput } from '@/components/ui/input';
    import {
        Select as UiSelectRoot,
        SelectContent as UiSelectContent,
        SelectItem as UiSelectItem,
        SelectTrigger as UiSelectTrigger,
        SelectValue as UiSelectValue
    } from '@/components/ui/select';
    import type { MyAppFormModel, MyAppModel } from '@/model/myApp';
    import {
        MY_APP_LOGO_DATA_URL_MAX_LENGTH,
        MY_APP_NAME_MAX_LENGTH,
        MY_APP_PORT_MAX,
        MY_APP_PORT_MIN,
        MY_APP_PUBLIC_SUBDOMAIN_MAX_LENGTH,
        MY_APP_PUBLIC_SUBDOMAIN_PATTERN,
        MY_APP_ZIP_DATA_URL_MAX_LENGTH
    } from '@/model/myApp';

    const props = defineProps<{
        /** 弹窗是否打开。 */
        open: boolean;
        /** 编辑中的应用；为空表示新增。 */
        app: MyAppModel | null;
        /** 是否正在保存。 */
        saving: boolean;
        /** 是否正在自动分配端口。 */
        allocatingPort: boolean;
        /** 自动分配端口动作。 */
        allocatePort: () => Promise<number>;
    }>();

    const emit = defineEmits<{
        /** 通知父组件更新弹窗打开状态。 */
        'update:open': [open: boolean];
        /** 提交已校验表单。 */
        submit: [form: MyAppFormModel];
    }>();

    const form = reactive<MyAppFormModel>(createEmptyForm());
    const errorMessage = ref('');
    const logoInputRef = ref<HTMLInputElement | null>(null);
    const zipInputRef = ref<HTMLInputElement | null>(null);
    const title = computed(() => (props.app ? '编辑应用' : '创建应用'));
    const zipDescription = computed(() => {
        if (form.zipFileName) return '已上传，可点击上方区域重新选择 zip。';
        if (props.app?.accessType === 'local') return '不重新上传时会复用当前静态页面。';
        return '请上传 Vue/Vite 等打包产物 zip。';
    });

    /**
     * 创建空表单。
     * 流程：给新增场景提供稳定默认值。
     * 参数：无。
     * 返回：空表单模型。
     * 边界：端口留空，避免前端自行猜测可用端口。
     */
    function createEmptyForm(): MyAppFormModel {
        return {
            id: '',
            name: '',
            logoDataUrl: '',
            accessType: 'local',
            port: '',
            remoteUrl: '',
            publicSubdomain: '',
            zipDataUrl: '',
            zipFileName: ''
        };
    }

    /**
     * 从应用记录同步表单。
     * 流程：打开新增时重置为空；打开编辑时复制服务端返回的可编辑字段。
     * 参数：app 为当前编辑应用。
     * 返回：无。
     * 边界：不会把旧 zipDataUrl 留到下一次打开。
     */
    function resetForm(app: MyAppModel | null): void {
        const next = app
            ? {
                  id: app.id,
                  name: app.name,
                  logoDataUrl: app.logoDataUrl,
                  accessType: app.accessType,
                  port: app.port ? String(app.port) : '',
                  remoteUrl: app.remoteUrl || '',
                  publicSubdomain: app.publicSubdomain || '',
                  zipDataUrl: '',
                  zipFileName: ''
              }
            : createEmptyForm();
        Object.assign(form, next);
        errorMessage.value = '';
    }

    /**
     * 读取文件为 data URL。
     * 流程：使用浏览器 FileReader，按长度上限提前拒绝过大文件。
     * 参数：file 为用户选择文件，maxLength 为 data URL 上限。
     * 返回：data URL 字符串。
     * 异常：读取失败或超过限制时抛出明确错误。
     */
    function readFileAsDataUrl(file: File, maxLength: number): Promise<string> {
        return new Promise((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => {
                if (typeof reader.result !== 'string') {
                    reject(new Error('文件读取失败。'));
                    return;
                }
                if (reader.result.length > maxLength) {
                    reject(new Error('文件过大，请选择更小的文件。'));
                    return;
                }
                resolve(reader.result);
            };
            reader.onerror = () => reject(new Error('文件读取失败。'));
            reader.readAsDataURL(file);
        });
    }

    /**
     * 触发 logo 文件选择。
     * 流程：点击定制 logo 方块后代理点击隐藏的文件输入框。
     * 参数：无。
     * 返回：无。
     * 边界：隐藏 input 不存在时静默跳过，避免页面异常。
     */
    function handleSelectLogo(): void {
        logoInputRef.value?.click();
    }

    /**
     * 触发 zip 文件选择。
     * 流程：点击定制 zip 上传框后代理点击隐藏的文件输入框。
     * 参数：无。
     * 返回：无。
     * 边界：隐藏 input 不存在时静默跳过，避免页面异常。
     */
    function handleSelectZip(): void {
        zipInputRef.value?.click();
    }

    /**
     * 处理 logo 文件选择。
     * 流程：读取第一项文件并写入表单预览。
     * 参数：event 为 input change 事件。
     * 返回：无。
     * 异常：读取失败时展示错误并保留旧 logo。
     */
    async function handleLogoChanged(event: Event): Promise<void> {
        const input = event.target as HTMLInputElement;
        const file = input.files?.[0];
        if (!file) return;
        try {
            form.logoDataUrl = await readFileAsDataUrl(file, MY_APP_LOGO_DATA_URL_MAX_LENGTH);
            errorMessage.value = '';
        } catch (error) {
            errorMessage.value = error instanceof Error ? error.message : 'logo 读取失败。';
        } finally {
            input.value = '';
        }
    }

    /**
     * 处理 zip 文件选择。
     * 流程：读取第一项 zip 为 data URL，并记录文件名。
     * 参数：event 为 input change 事件。
     * 返回：无。
     * 异常：读取失败时清空本次 zip 字段。
     */
    async function handleZipChanged(event: Event): Promise<void> {
        const input = event.target as HTMLInputElement;
        const file = input.files?.[0];
        if (!file) return;
        try {
            form.zipDataUrl = await readFileAsDataUrl(file, MY_APP_ZIP_DATA_URL_MAX_LENGTH);
            form.zipFileName = file.name;
            errorMessage.value = '';
        } catch (error) {
            form.zipDataUrl = '';
            form.zipFileName = '';
            errorMessage.value = error instanceof Error ? error.message : 'zip 读取失败。';
        } finally {
            input.value = '';
        }
    }

    /**
     * 自动分配端口。
     * 流程：调用父组件传入的 HTTP 动作并回填端口输入框。
     * 参数：无。
     * 返回：无。
     * 异常：失败时展示错误。
     */
    async function handleAllocatePort(): Promise<void> {
        try {
            form.port = String(await props.allocatePort());
            errorMessage.value = '';
        } catch (error) {
            errorMessage.value = error instanceof Error ? error.message : '自动分配端口失败。';
        }
    }

    /**
     * 校验表单。
     * 流程：按访问方式校验名称、端口、zip 或远程 URL。
     * 参数：无。
     * 返回：错误文案；为空表示通过。
     * 边界：编辑本地应用允许不重新上传 zip。
     */
    function validateForm(): string {
        if (!form.name.trim()) return '请填写应用名称。';
        if (form.name.trim().length > MY_APP_NAME_MAX_LENGTH) return '应用名称最多 80 个字符。';
        if (form.accessType === 'local') {
            const port = Number(form.port);
            if (!Number.isInteger(port) || port < MY_APP_PORT_MIN || port > MY_APP_PORT_MAX) return '请填写合法端口。';
            const subdomain = form.publicSubdomain.trim();
            if (subdomain.length > MY_APP_PUBLIC_SUBDOMAIN_MAX_LENGTH) return '公网二级域名前缀最多 63 个字符。';
            if (subdomain && !MY_APP_PUBLIC_SUBDOMAIN_PATTERN.test(subdomain.toLowerCase()))
                return '公网二级域名仅支持小写字母、数字和短横线，且不能以短横线开头或结尾。';
            if (!props.app && !form.zipDataUrl) return '请上传静态页面 zip 包。';
            if (props.app?.accessType === 'remote' && !form.zipDataUrl)
                return '从远程 URL 改为本地托管时需要上传 zip 包。';
        } else if (!/^https?:\/\/\S+$/i.test(form.remoteUrl.trim())) return '请输入 http 或 https 网址。';
        return '';
    }

    /**
     * 提交表单。
     * 流程：前端完成基础校验后把完整表单交给父组件保存。
     * 参数：无。
     * 返回：无。
     * 边界：最终 zip、URL 和端口校验仍由 HTTP/Rust 执行。
     */
    function handleSubmit(): void {
        const validationError = validateForm();
        if (validationError) {
            errorMessage.value = validationError;
            return;
        }
        emit('submit', { ...form });
    }

    watch(
        () => [props.open, props.app] as const,
        ([open]) => {
            if (open) resetForm(props.app);
        },
        { immediate: true }
    );
</script>
