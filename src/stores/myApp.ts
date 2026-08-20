import { defineStore } from 'pinia';

import type { CreateMyAppRequestModel, MyAppModel, MyAppOpenTargetType, UpdateMyAppRequestModel } from '@/model/myApp';
import {
    allocateMyAppPort,
    createMyApp,
    deleteMyApp,
    listMyApps,
    openMyApp,
    restartMyApp,
    updateMyApp
} from '@/service/tauri/command';

/** 我的应用 Store 状态。 */
interface MyAppState {
    /** 应用列表。 */
    apps: MyAppModel[];
    /** 是否正在加载列表。 */
    loading: boolean;
    /** 是否正在保存表单。 */
    saving: boolean;
    /** 正在执行操作的应用 ID 集合。 */
    operatingIds: string[];
    /** 是否正在自动分配端口。 */
    allocatingPort: boolean;
    /** 最近一次状态提示。 */
    message: string;
}

/**
 * 替换或插入列表项。
 * 流程：存在相同 ID 时就地替换，否则追加到列表前方。
 * 参数：apps 为当前列表，next 为服务端返回的最新应用。
 * 返回：新列表。
 * 边界：不根据前端旧状态推断服务状态，始终使用 HTTP 返回值。
 */
function replaceApp(apps: MyAppModel[], next: MyAppModel): MyAppModel[] {
    const index = apps.findIndex((app) => app.id === next.id);
    if (index < 0) return [next, ...apps];
    return apps.map((app) => (app.id === next.id ? next : app));
}

export const useMyAppStore = defineStore('myApp', {
    state: (): MyAppState => ({
        apps: [],
        loading: false,
        saving: false,
        operatingIds: [],
        allocatingPort: false,
        message: ''
    }),
    actions: {
        /**
         * 读取我的应用列表。
         * 流程：调用公共 HTTP API 并用服务端状态替换本地列表。
         * 参数：无。
         * 返回：加载完成 Promise。
         * 异常：失败时保留旧列表并写入 message，调用方负责提示。
         */
        async loadApps(): Promise<void> {
            this.loading = true;
            try {
                this.apps = await listMyApps();
                this.message = '';
            } catch (error) {
                this.message = error instanceof Error ? error.message : '读取我的应用失败。';
                throw error;
            } finally {
                this.loading = false;
            }
        },

        /**
         * 自动分配本地端口。
         * 流程：请求 HTTP 服务查找可绑定端口并返回给表单。
         * 参数：无。
         * 返回：当前可用端口。
         * 异常：无可用端口或服务不可用时透传。
         */
        async allocatePort(): Promise<number> {
            this.allocatingPort = true;
            try {
                const response = await allocateMyAppPort();
                return response.port;
            } finally {
                this.allocatingPort = false;
            }
        },

        /**
         * 保存我的应用。
         * 流程：根据是否存在 ID 选择创建或更新接口，并用返回项更新列表。
         * 参数：request 为创建或修改请求。
         * 返回：最新应用。
         * 异常：保存失败时保留原列表并向上抛出。
         */
        async saveApp(request: CreateMyAppRequestModel | UpdateMyAppRequestModel): Promise<MyAppModel> {
            this.saving = true;
            try {
                const saved = 'id' in request && request.id ? await updateMyApp(request) : await createMyApp(request);
                this.apps = replaceApp(this.apps, saved);
                this.message = saved.serviceMessage;
                return saved;
            } catch (error) {
                this.message = error instanceof Error ? error.message : '保存我的应用失败。';
                throw error;
            } finally {
                this.saving = false;
            }
        },

        /**
         * 删除我的应用。
         * 流程：调用 HTTP 删除接口，成功后从列表移除。
         * 参数：appId 为应用 ID。
         * 返回：删除完成 Promise。
         * 异常：删除失败时保留列表并向上抛出。
         */
        async removeApp(appId: string): Promise<void> {
            this.operatingIds.push(appId);
            try {
                await deleteMyApp(appId);
                this.apps = this.apps.filter((app) => app.id !== appId);
            } finally {
                this.operatingIds = this.operatingIds.filter((id) => id !== appId);
            }
        },

        /**
         * 启动或重启本地应用。
         * 流程：先把目标项标记为启动中，再调用 HTTP 重启接口并替换为服务端返回状态。
         * 参数：appId 为本地应用 ID。
         * 返回：最新应用。
         * 异常：失败时刷新列表，避免卡片停留在启动中。
         */
        async restartApp(appId: string): Promise<MyAppModel> {
            this.operatingIds.push(appId);
            this.apps = this.apps.map((app) =>
                app.id === appId ? { ...app, serviceStatus: 'starting', serviceMessage: '服务启动中。' } : app
            );
            try {
                const restarted = await restartMyApp(appId);
                this.apps = replaceApp(this.apps, restarted);
                return restarted;
            } catch (error) {
                await this.loadApps();
                throw error;
            } finally {
                this.operatingIds = this.operatingIds.filter((id) => id !== appId);
            }
        },

        /**
         * 打开我的应用。
         * 流程：通过 HTTP 请求 Rust 按目标打开；本地服务启动失败时由后端拒绝。
         * 参数：appId 为应用 ID，target 为打开目标。
         * 返回：打开完成 Promise。
         * 异常：打开失败时刷新列表以展示最新服务状态。
         */
        async openApp(appId: string, target: MyAppOpenTargetType): Promise<void> {
            this.operatingIds.push(appId);
            try {
                await openMyApp(appId, target);
                const current = this.apps.find((app) => app.id === appId);
                if (current?.accessType === 'local' && current.serviceStatus !== 'running') await this.loadApps();
            } catch (error) {
                await this.loadApps();
                throw error;
            } finally {
                this.operatingIds = this.operatingIds.filter((id) => id !== appId);
            }
        }
    }
});
