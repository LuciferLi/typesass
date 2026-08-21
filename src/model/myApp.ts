/** 我的应用访问方式。 */
export type MyAppAccessType = 'local' | 'remote';

/** 我的应用本地服务状态。 */
export type MyAppServiceStatusType = 'starting' | 'running' | 'paused' | 'failed' | 'unavailable';

/** 我的应用打开目标。 */
export type MyAppOpenTargetType = 'codexman' | 'browser';

/** 我的应用本地托管 zip data URL 长度上限，受 HTTP 12 MiB body 限制约束。 */
export const MY_APP_ZIP_DATA_URL_MAX_LENGTH = 12_000_000;

/** 我的应用 logo data URL 长度上限。 */
export const MY_APP_LOGO_DATA_URL_MAX_LENGTH = 400_000;

/** 我的应用名称长度上限。 */
export const MY_APP_NAME_MAX_LENGTH = 80;

/** 我的应用端口最小值。 */
export const MY_APP_PORT_MIN = 1024;

/** 我的应用端口最大值。 */
export const MY_APP_PORT_MAX = 65_535;

/** 我的应用公网二级域名前缀长度上限。 */
export const MY_APP_PUBLIC_SUBDOMAIN_MAX_LENGTH = 63;

/** 我的应用公网二级域名前缀校验规则。 */
export const MY_APP_PUBLIC_SUBDOMAIN_PATTERN = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;

/** 我的应用列表项。 */
export interface MyAppModel {
    /** 应用稳定 ID。 */
    id: string;
    /** 应用名称。 */
    name: string;
    /** logo data URL；为空时前端展示默认图标。 */
    logoDataUrl: string;
    /** 访问方式。 */
    accessType: MyAppAccessType;
    /** 本地托管端口；远程 URL 应用为空。 */
    port: number | null;
    /** 远程 URL；本地托管应用为空。 */
    remoteUrl: string | null;
    /** 本机访问地址；远程 URL 应用为空。 */
    localUrl: string;
    /** 局域网访问地址；远程 URL 应用为空。 */
    lanUrl: string;
    /** 公网访问地址；未配置二级域名或远程 URL 应用为空。 */
    publicUrl: string;
    /** 公网访问二级域名前缀；未配置时为空。 */
    publicSubdomain: string | null;
    /** 默认打开地址。 */
    openUrl: string;
    /** 当前服务状态。 */
    serviceStatus: MyAppServiceStatusType;
    /** 服务状态说明或最近错误。 */
    serviceMessage: string;
    /** 创建时间。 */
    createdAt: string;
    /** 更新时间。 */
    updatedAt: string;
}

/** 创建我的应用请求。 */
export interface CreateMyAppRequestModel {
    /** 应用名称。 */
    name: string;
    /** logo data URL；为空时使用默认图标。 */
    logoDataUrl: string;
    /** 访问方式。 */
    accessType: MyAppAccessType;
    /** 本地托管端口。 */
    port?: number;
    /** 远程 URL。 */
    remoteUrl?: string;
    /** 公网访问二级域名前缀。 */
    publicSubdomain?: string;
    /** 本地托管 zip data URL。 */
    zipDataUrl?: string;
}

/** 修改我的应用请求。 */
export interface UpdateMyAppRequestModel extends CreateMyAppRequestModel {
    /** 待修改应用稳定 ID。 */
    id: string;
}

/** 自动分配端口响应。 */
export interface MyAppPortResponseModel {
    /** 当前检测可用端口。 */
    port: number;
}

/** 我的应用表单模型。 */
export interface MyAppFormModel {
    /** 编辑时的应用 ID；新增时为空。 */
    id: string;
    /** 应用名称。 */
    name: string;
    /** logo data URL。 */
    logoDataUrl: string;
    /** 访问方式。 */
    accessType: MyAppAccessType;
    /** 端口输入值。 */
    port: string;
    /** 远程 URL。 */
    remoteUrl: string;
    /** 公网访问二级域名前缀。 */
    publicSubdomain: string;
    /** 本次上传的 zip data URL。 */
    zipDataUrl: string;
    /** 本次上传的 zip 文件名。 */
    zipFileName: string;
}
