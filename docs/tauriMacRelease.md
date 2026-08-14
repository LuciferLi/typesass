# CodexMan Tauri macOS 官网发布流程

本文只适用于 CodexMan 的 Tauri macOS App 官网分发，不适用于 Electron。

## 固定产物

- App 名称：`CodexMan`
- Tauri 配置：`src-tauri/tauri.conf.json`
- Developer ID：`Developer ID Application: Tamba Trading Co., Ltd. (9VKQ2P8P6N)`
- Notary profile：`codexman-notary`
- 官网下载文件：`website/downloads/codexman_版本_架构.dmg`
- 官网下载链接：`/downloads/codexman_版本_架构.dmg`
- 版本递增：每次打包发布必须先递增 `package.json` 与 `src-tauri/tauri.conf.json` 版本；小修复递增 patch 位（如 `0.0.2` -> `0.0.3`），较大功能版本递增 minor 位（如 `0.0.x` -> `0.1.0`）。

## 发布命令

```bash
npm run release:mac
```

该命令会执行以下步骤：

1. 清理 `src-tauri/target/release/bundle`，避免复用历史 `typesass` 产物。
2. 执行 `npm run build`，同步刷新浏览器插件 ZIP、sidecar 和前端 dist。
3. 执行 `npx tauri build --bundles dmg`。
4. 使用 `xcrun notarytool submit --keychain-profile codexman-notary --wait` 上传 Apple 公证。
5. 使用 `xcrun stapler staple` 贴票。
6. 使用 `xcrun stapler validate` 和 `spctl -a -vvv -t install` 验证 Gatekeeper 状态。
7. 把已公证 dmg 复制到 `website/downloads/`。

## OSS 同步

发布脚本只生成本地官网目录。上传时同步整个 `website/` 目录到官网 OSS Bucket，确保 `index.html`、`assets/` 和当前版本 `downloads/codexman_版本_架构.dmg` 同时更新。

```bash
ossutil sync website oss://<codexman-website-bucket>/ --delete
```

同步后必须验证：

```bash
curl -I https://typesass.tolern.com/
curl -I https://typesass.tolern.com/downloads/codexman_当前版本_aarch64.dmg
```

## 禁止事项

- 禁止使用 Electron Builder、Electron Forge、`electron-notarize` 或 Electron updater 流程发布 CodexMan Tauri App。
- 禁止上传 `typesass_*.dmg`。
- 禁止只签名不公证。
- 禁止跳过 `stapler validate` 和 `spctl` 验证。
