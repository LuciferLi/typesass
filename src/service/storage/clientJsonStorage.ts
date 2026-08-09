import { StorageKey } from '@/config/storageKey';
import type { LocalConfigChangedPayloadModel, LocalConfigJsonValueModel } from '@/model/localConfig';
import { requestClientHttpBridge } from '@/service/tauri/command';

const LOCAL_CONFIG_WATCH_INTERVAL_MS = 500;

/**
 * 判断未知值是否为可序列化 JSON。
 * 流程：按 JSON 基础类型、数组、纯对象递归检查，避免把函数、DOM 或循环引用交给客户端写盘。
 * 参数：value 为需要写入客户端 JSON 文件的未知值。
 * 返回：值是否可以安全作为 JSON 写入。
 * 边界：循环引用会在调用方 JSON.stringify 兜底时失败；这里不额外维护引用集合以保持轻量。
 */
function isLocalConfigJsonValue(value: unknown): value is LocalConfigJsonValueModel {
    if (value === null) return true;
    const valueType = typeof value;
    if (valueType === 'string' || valueType === 'number' || valueType === 'boolean') return true;
    if (Array.isArray(value)) {
        return value.every((item) => isLocalConfigJsonValue(item));
    }
    if (valueType === 'object') {
        return Object.values(value as Record<string, unknown>).every((item) => isLocalConfigJsonValue(item));
    }
    return false;
}

/**
 * 读取客户端 JSON 配置中的单个分区。
 * 流程：通过客户端本地 HTTP 桥接读取用户电脑上的配置文件。
 * 参数：key 为 StorageKey 分区名，fallback 为配置缺失或读取失败时的兜底值。
 * 返回：对应分区的配置值。
 * 边界：客户端未启动、配置文件损坏或分区不存在时返回 fallback，不再读取 localStorage。
 */
export async function readClientJson<T>(key: string, fallback: T): Promise<T> {
    try {
        const value = await requestClientHttpBridge<LocalConfigJsonValueModel | null>('/read-local-config-value', {
            key
        });
        return value === null ? fallback : (value as T);
    } catch {
        return fallback;
    }
}

/**
 * 写入客户端 JSON 配置中的单个分区。
 * 流程：校验值可 JSON 序列化后，通过客户端本地 HTTP 桥接写入用户电脑上的配置文件。
 * 参数：key 为 StorageKey 分区名，value 为需要保存的配置值。
 * 返回：写入完成 Promise。
 * 边界：客户端未启动或不可序列化值会直接抛错，不再写浏览器本地存储。
 */
export async function writeClientJson<T>(key: string, value: T): Promise<void> {
    if (!isLocalConfigJsonValue(value)) {
        throw new Error('配置内容不是有效 JSON，无法写入客户端文件。');
    }
    await requestClientHttpBridge<void>('/write-local-config-value', { key, value });
}

/**
 * 删除客户端 JSON 配置中的单个分区。
 * 流程：通过客户端本地 HTTP 桥接移除配置文件内指定 key，不再访问浏览器 storage。
 * 参数：key 为 StorageKey 分区名。
 * 返回：删除完成 Promise。
 * 边界：分区不存在时客户端保持幂等成功。
 */
export async function removeClientJson(key: string): Promise<void> {
    await requestClientHttpBridge<void>('/remove-local-config-value', { key });
}

/**
 * 启动客户端 JSON 配置文件监听。
 * 流程：先让 Rust 侧开启文件轮询监听，Web 侧再通过 HTTP 定时读取全量配置快照。
 * 参数：handler 为配置文件变化后的前端刷新函数。
 * 返回：取消前端事件监听的函数。
 * 边界：客户端未启动时不会启动监听，直接返回空取消函数。
 */
export async function watchClientJson(handler: (payload: LocalConfigChangedPayloadModel) => void): Promise<() => void> {
    try {
        await requestClientHttpBridge<void>('/start-local-config-watch', {});
    } catch {
        return () => {};
    }
    let previousSnapshot = '';
    const refreshSnapshot = async () => {
        try {
            const snapshot =
                await requestClientHttpBridge<LocalConfigChangedPayloadModel>('/read-local-config-snapshot');
            const nextSnapshot = JSON.stringify(snapshot);
            if (nextSnapshot === previousSnapshot) return;
            previousSnapshot = nextSnapshot;
            handler(snapshot);
        } catch {
            // 客户端关闭后停止刷新交给取消函数处理，单次失败不打断页面。
        }
    };
    await refreshSnapshot();
    const timer = window.setInterval(() => {
        void refreshSnapshot();
    }, LOCAL_CONFIG_WATCH_INTERVAL_MS);
    return () => window.clearInterval(timer);
}

/**
 * 把历史浏览器 localStorage 配置迁移到客户端 JSON 文件。
 * 流程：遍历已知 StorageKey，读取浏览器旧值并写入客户端文件，写入成功后删除旧 localStorage。
 * 参数：无。
 * 返回：迁移完成 Promise。
 * 边界：只在真实 Tauri 环境执行；单个旧值损坏时跳过该 key，避免阻塞其他配置迁移。
 */
export async function migrateBrowserStorageToClientJson(): Promise<void> {
    const keys = Object.values(StorageKey);
    for (const key of keys) {
        const rawValue = window.localStorage.getItem(key);
        if (!rawValue) continue;
        try {
            const value = JSON.parse(rawValue) as unknown;
            if (!isLocalConfigJsonValue(value)) continue;
            await writeClientJson(key, value);
            window.localStorage.removeItem(key);
        } catch {
            window.localStorage.removeItem(key);
        }
    }
}
