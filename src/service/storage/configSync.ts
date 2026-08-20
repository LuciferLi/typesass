import { StorageKey } from '@/config/storageKey';
import type { LocalConfigChangedPayloadModel } from '@/model/localConfig';
import type { ShortcutProfileModel } from '@/model/permission';
import {
    DefaultShortcutProfile,
    hasShortcutProfileValue,
    normalizeShortcutProfileValue
} from '@/service/shortcut/shortcutProfile';
import { readClientJson, watchClientJson } from '@/service/storage/clientJsonStorage';
import { isTauriRuntime, registerShortcuts } from '@/service/tauri/command';
import { useSettingsStore } from '@/stores/settings';
import { useTextPolishStore } from '@/stores/textPolish';
import { useVoicePolishStore } from '@/stores/voicePolish';

/**
 * 把客户端 JSON 配置快照应用到所有本地持久化 Store。
 * 流程：按 StorageKey 分区分发给各 Store 的 apply 方法，实现文件变化后的实时刷新。
 * 参数：snapshot 为 Rust 侧广播的全量配置快照。
 * 返回：无返回值。
 * 边界：缺失分区不会覆盖当前状态，避免外部手动删除单个 key 时影响当前会话的运行态。
 */
function applyClientJsonSnapshot(snapshot: LocalConfigChangedPayloadModel): void {
    const settingsStore = useSettingsStore();
    const voicePolishStore = useVoicePolishStore();
    const textPolishStore = useTextPolishStore();

    if (StorageKey.settings in snapshot.items) {
        settingsStore.applyPersistedSettings(snapshot.items[StorageKey.settings]);
    }
    if (StorageKey.voicePolish in snapshot.items) {
        voicePolishStore.applyPersistedVoicePolish(snapshot.items[StorageKey.voicePolish]);
    }
    if (StorageKey.textPolish in snapshot.items) {
        textPolishStore.applyPersistedTextPolish(snapshot.items[StorageKey.textPolish]);
    }
    if (StorageKey.shortcuts in snapshot.items && hasShortcutProfileValue(snapshot.items[StorageKey.shortcuts])) {
        void registerShortcuts(
            normalizeShortcutProfileValue(snapshot.items[StorageKey.shortcuts], DefaultShortcutProfile)
        );
    }
}

/**
 * 注册已保存的全局快捷键配置。
 * 流程：从客户端 JSON 快捷键分区读取保存值，有有效配置时提交给原生快捷键注册命令。
 * 参数：无。
 * 返回：注册完成 Promise。
 * 边界：配置缺失时不主动写入默认值，避免覆盖原生侧默认注册结果。
 */
async function registerPersistedShortcuts(): Promise<void> {
    const savedShortcuts = await readClientJson<Partial<ShortcutProfileModel>>(StorageKey.shortcuts, {});
    if (!hasShortcutProfileValue(savedShortcuts)) return;
    await registerShortcuts(normalizeShortcutProfileValue(savedShortcuts, DefaultShortcutProfile));
}

/**
 * 初始化客户端 JSON 配置同步。
 * 流程：分别水合各业务 Store，最后启动客户端文件监听；首次上线不执行任何历史数据迁移。
 * 参数：无。
 * 返回：初始化完成 Promise。
 * 边界：任一读取失败不会阻塞应用启动；文件监听失败时仅失去外部变更实时刷新能力。
 */
export async function initClientJsonConfigSync(): Promise<void> {
    if (!isTauriRuntime()) return;
    const settingsStore = useSettingsStore();
    const voicePolishStore = useVoicePolishStore();
    const textPolishStore = useTextPolishStore();

    await Promise.all([
        settingsStore.hydrateSettings(),
        voicePolishStore.hydrateVoicePolish(),
        textPolishStore.hydrateTextPolish(),
        registerPersistedShortcuts()
    ]);
    await watchClientJson(applyClientJsonSnapshot);
}
