import type { LocalConfigChangedPayloadModel, LocalConfigJsonValueModel } from '@/model/localConfig';
import {
    listenEvent,
    readLocalConfigValue,
    removeLocalConfigValue,
    startLocalConfigWatch,
    writeLocalConfigValue
} from '@/service/tauri/command';

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
 * 流程：通过受 Tauri capability 保护的 IPC 读取用户电脑上的配置文件。
 * 参数：key 为配置分区名，fallback 只用于分区尚未创建的首次启动场景。
 * 返回：对应分区的配置值。
 * 边界：分区不存在时返回 fallback；配置损坏或 IPC 失败会抛错，避免静默用默认值覆盖问题现场。
 */
export async function readClientJson<T>(key: string, fallback: T): Promise<T> {
    const value = await readLocalConfigValue(key);
    return value === null ? fallback : (value as T);
}

/**
 * 写入客户端 JSON 配置中的单个分区。
 * 流程：校验值可 JSON 序列化后，通过桌面 IPC 写入用户电脑上的配置文件。
 * 参数：key 为 StorageKey 分区名，value 为需要保存的配置值。
 * 返回：写入完成 Promise。
 * 边界：客户端未启动或不可序列化值会直接抛错，不再写浏览器本地存储。
 */
export async function writeClientJson<T>(key: string, value: T): Promise<void> {
    if (!isLocalConfigJsonValue(value)) {
        throw new Error('配置内容不是有效 JSON，无法写入客户端文件。');
    }
    await writeLocalConfigValue(key, value);
}

/**
 * 删除客户端 JSON 配置中的单个分区。
 * 流程：通过桌面 IPC 移除配置文件内指定 key，不访问浏览器 storage。
 * 参数：key 为 StorageKey 分区名。
 * 返回：删除完成 Promise。
 * 边界：分区不存在时客户端保持幂等成功。
 */
export async function removeClientJson(key: string): Promise<void> {
    await removeLocalConfigValue(key);
}

/**
 * 启动客户端 JSON 配置文件监听。
 * 流程：先让 Rust 侧开启文件监听，再通过 Tauri event 接收配置快照。
 * 参数：handler 为配置文件变化后的前端刷新函数。
 * 返回：取消前端事件监听的函数。
 * 边界：仅桌面运行时可调用；IPC 或监听注册失败会抛错，交由应用初始化错误处理记录。
 */
export async function watchClientJson(handler: (payload: LocalConfigChangedPayloadModel) => void): Promise<() => void> {
    const unlisten = await listenEvent<LocalConfigChangedPayloadModel>('local-config-changed', handler);
    await startLocalConfigWatch();
    return unlisten;
}
