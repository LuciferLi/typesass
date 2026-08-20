# 本地 JSON 配置持久化

## 需求背景

- 用户发现模型、设置、词典和历史等配置大量保存在 WebView 浏览器本地存储中，卸载应用后容易丢失。
- 目标是由客户端读写用户电脑上的 JSON 配置文件，Web 端只通过客户端命令操作配置。
- JSON 文件被外部修改或其他窗口写入后，前端需要实时感知并刷新状态。

## 技术方案

- 客户端文件：通过 Tauri `app_data_dir()` 下的 `codexman-config.json` 保存配置。
- 文件结构：`version`、`updatedAt`、`items` 三段；`items` 按前端 `StorageKey` 分区保存模型、设置、语音、文字润色和字幕配置。
- Web 调客户端：前端统一使用 `src/service/storage/clientJsonStorage.ts`，通过 Tauri `invoke` 调用 `read_local_config_value`、`write_local_config_value`、`remove_local_config_value`。
- 实时变化：客户端启动 `start_local_config_watch` 后，每 500ms 检查配置文件修改时间；文件变化后广播 `local-config-changed` 快照给所有 WebView。
- 历史迁移：首次进入真实客户端时，将旧 `localStorage` 中已知配置迁移到 JSON 文件，迁移成功后删除旧浏览器数据；后续不再向浏览器本地存储写入配置。

## 覆盖范围

- 模型管理：`codexman.modelManage.v1`
- 系统设置：`codexman.settings.v1`
- 语音润色：`codexman.voicePolish.v1`
- 文字实时润色：`codexman.textPolish.v1`
- 实时字幕：`codexman.subtitle.v1`

## 测试用例

| 用例 | 步骤 | 预期 | 结果 |
| --- | --- | --- | --- |
| 客户端 JSON 读写命令注册 | 执行 Rust 检查和 Tauri 打包 | 新命令可编译并进入桌面包 | 通过 |
| 前端不再写浏览器存储 | 搜索 `aiTool/src`、`aiTool/src-tauri` 中 storage 关键字 | 不存在业务写入 `localStorage`/`sessionStorage`，仅保留迁移读取和清理 | 通过 |
| Store 异步水合 | 执行 `npm run lint` 和 `npm run build` | 类型通过，生产构建正常 | 通过 |
| 文件变化实时刷新 | 检查 `start_local_config_watch` 和 `local-config-changed` 订阅链路 | 客户端轮询配置文件并向前端分发快照 | 通过 |

## 验证记录

- `npm run lint`：通过。
- `npm run build`：通过。
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过，存在 3 个既有未使用函数 warning。
- `npm run tauri:build`：通过，已生成：
  - `src-tauri/target/release/bundle/macos/CodexMan.app`
  - `src-tauri/target/release/bundle/dmg/codexman_0.0.2_aarch64.dmg`

## 剩余风险

- 当前实时变化采用 500ms 轮询修改时间，满足配置文件变化后的准实时刷新；如果后续配置文件体积明显增大，可以替换为系统文件事件监听。
- 旧浏览器存储迁移只覆盖 `StorageKey` 中声明的已知配置；未登记的临时浏览器数据不会迁移，避免把无关数据继续带入新持久化体系。
