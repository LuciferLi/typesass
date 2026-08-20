# E2E 测试

## 2026-07-24 桌面包启动验证

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 进程：`/CodexMan.app/Contents/MacOS/ai-tool`

执行：

1. `pkill -f '/CodexMan.app/Contents/MacOS/ai-tool' || true`
2. `open -n /Users/lucifer/Documents/source/t/monorepo/aiTool/src-tauri/target/release/bundle/macos/CodexMan.app`
3. `pgrep -fl '/CodexMan.app/Contents/MacOS/ai-tool|ai-tool'`
4. `screencapture -x -o -l <CodexManWindowId> /tmp/aitool-final-hub.png`
5. Playwright 打开 `http://127.0.0.1:1421/?mode=hub`，点击设置页并验证快捷键录制交互。

结果：

- `.app` 启动成功，进程号已出现。
- Hub 仪表盘可见，显示真实本地统计、三种模式按钮、动态快捷键标签和最近结果面板。
- 设置页可见，快捷键录制按钮可用；浏览器预览中录制 `Control + Shift + Y` 后，口述快捷键输入框更新为 `ctrl+shift+y`，顶部提示“保存后生效”。
- 截图证据：`/tmp/aitool-final-hub.png`、`.playwright-cli/page-2026-07-24T02-58-15-358Z.png`

未覆盖：

- macOS 全局快捷键无法通过当前命令可靠模拟，需要用户实际按 `Control + P`、`Control + T`、`Control + Space` 验证。
- 自动粘贴需要目标输入框和辅助功能权限，需要用户本机确认。
- 麦克风设备名和真实录音需要用户授权麦克风后确认。

## 2026-07-24 功能按钮与快捷键配置复测

环境：

- 页面：`http://127.0.0.1:1420/?mode=hub`
- App 包：`src-tauri/target/release/bundle/macos/CodexMan.app`

执行与结果：

1. 打开 Hub 首页，确认左侧导航、首页统计、语音模式、开始按钮、最近结果均来自本地状态，没有 demo/mock 数据。
2. 点击设置页，确认模型、录音输出、快捷键、个人偏好、系统诊断均为可编辑或可检测的真实入口。
3. 点击口述“录制”后按 `Control + Shift + Y`，口述输入框更新为 `ctrl+shift+y`，顶部提示“保存后生效”。
4. 回到首页点击“翻译”，仅切换模式高亮和主按钮文案为“开始翻译”，不会直接触发录音。
5. 刷新浏览器预览后检查控制台，error 级日志为 0。
6. 重新执行 `npx tauri build --bundles app`，生成最新 `.app`。

截图证据：

- `/tmp/aitool-reference-style-hub-20260724.png`
- `/tmp/aitool-reference-style-settings-20260724.png`

未覆盖：

- 真实全局快捷键注册结果、自动粘贴和麦克风录音仍需在 `.app` 内由用户授权后确认。

## 2026-07-24 桌面后台与缺 Key 引导复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 进程：`/CodexMan.app/Contents/MacOS/ai-tool`
- 环境变量：`MIMO_API_KEY` 为空

执行与结果：

1. 重新打包并启动最新 `.app`，进程启动成功。
2. 使用 macOS 窗口枚举拿到 Hub 主窗口，单窗口截图成功：`/tmp/aitool-desktop-polish-hub-20260724.png`。
3. 通过系统按键事件触发 `Control + P`，窗口列表出现悬浮条窗口 `132x46` 和错误提示窗口 `460x86`，二者均位于屏幕顶部偏下。
4. 缺 Key 时，Hub 自动切换到设置页，截图：`/tmp/aitool-missing-key-settings-20260724.png`。
5. 等待 7 秒后再次枚举窗口，只剩 Hub 主窗口，悬浮条和错误提示窗口自动隐藏成功。
6. 对 Hub 执行 `Command + W` 后，Hub 窗口隐藏但 CodexMan 进程仍在。
7. Hub 隐藏后再次触发 `Control + P`，Hub 可重新显示并切到设置页，后台快捷键链路有效。

未覆盖：

- 填入 Mimo Key 后的真实麦克风录音、Mimo 转写、AI 整理和自动粘贴仍需用户授权后确认。

## 2026-07-24 结果兜底窗口构建验证

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 分支：`dev-20260723`

执行与结果：

1. 新增 `result` Tauri 窗口配置，加入 capabilities，窗口默认隐藏、置顶、无装饰、无阴影。
2. 新增 `show_result_window` / `hide_result_window` 命令，前端 `pasteTranscription` 在 `pasted=false` 或异常时调用结果窗口。
3. Rust `paste_text` 改为先写剪贴板，再检查辅助功能和文本输入焦点；未满足时返回失败状态和原因。
4. 执行 `npm run build`：通过。
5. 执行 `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
6. 执行 `npx tauri build --bundles app`：通过，最新 `.app` 已生成。
7. 启动最新 `.app` 后枚举到 Hub 主窗口 `1200x800`，截图成功：`/tmp/aitool-style-hub-20260724-result-fallback.png`。

未覆盖：

- 真实结果窗口需要完整录音、Mimo 转写和粘贴失败条件触发；本轮未使用用户密钥执行真实录音链路。

## 2026-07-24 后台启动与真实按钮复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 页面预览：`http://127.0.0.1:1421/?mode=hub`、`http://127.0.0.1:1421/?mode=result`
- 分支：`dev-20260723`

执行与结果：

1. 执行 `npm run lint`，通过；确认当前项目已有真实 lint/typecheck 脚本。
2. 执行 `npm run build`，通过。
3. 执行 `cargo fmt --check --manifest-path src-tauri/Cargo.toml`，通过。
4. 执行 `cargo check --manifest-path src-tauri/Cargo.toml`，通过。
5. 执行 `git diff --check`，通过。
6. 执行 `npx tauri build --bundles app`，通过并生成最新 `.app`。
7. 启动最新 `.app`，进程存在；窗口枚举未发现 CodexMan 可见窗口，说明启动后不再弹常驻 Hub。
8. 打开 Hub 预览，确认首页统计、模式按钮、快捷键展示、最近结果按钮均来自本地真实状态；没有结果时复制/重新整理按钮为禁用态。
9. 打开结果窗口预览并注入真实转写兜底 payload，确认复制按钮显示；需要辅助功能权限时才显示“打开辅助功能设置”。

截图证据：

- `output/playwright/aitool-hub-20260724-latest.png`
- `output/playwright/aitool-result-20260724-latest.png`

未覆盖：

- 真实 Mimo Key、麦克风录音、物理快捷键和目标输入框自动粘贴仍需用户授权后确认。

## 2026-07-24 权限准备与耗时展示复测

环境：

- 页面预览：`http://127.0.0.1:1421/?mode=hub`
- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 分支：`dev-20260723`

执行与结果：

1. 首页注入一条浏览器预览用本地历史样本，仅用于检查 UI 展示，不进入产品源码。
2. 首页最近结果显示 `口述 · 录音 4.2s · 转写 866ms · AI 1.3s`，短耗时不再显示为 `00:00`。
3. 设置页麦克风行显示“授权”和“刷新”两个真实按钮，布局无挤压。
4. 历史记录显示链路耗时，ASR 原文和最终输出不一致时出现“查看原文”折叠区。
5. 点击“查看原文”后展开原始转写文本，页面无遮挡、无溢出。
6. 重新执行 `npm run lint`、`npm run build`、Rust 格式/编译检查和 Tauri 打包，均通过。
7. 启动最新 `.app`，进程存在且无可见常驻窗口。

截图证据：

- `output/playwright/aitool-hub-timing-20260724.png`
- `output/playwright/aitool-settings-microphone-20260724.png`
- `output/playwright/aitool-history-source-20260724.png`

未覆盖：

- 真实麦克风授权弹窗、真实录音和自动粘贴仍需用户在 `.app` 中点击系统权限后验证。

## 2026-07-24 真实入口与快捷键配置复测

环境：

- 页面预览：`http://127.0.0.1:1421/index.html?mode=hub`
- 分支：`dev-20260723`

执行与结果：

1. 打开 Hub 首页，确认首页只保留真实统计、最近结果、刷新状态和开始当前模式；最近结果为空时复制/重新整理按钮为禁用态。
2. 打开语音模式页，确认口述、翻译、随便问三张卡片均可切换模式并触发开始按钮；未新增无动作卡片。
3. 打开快捷键页，点击口述“录制”，按下 `Control+Shift+D`，页面即时展示 `Control + Shift + D` 并提示保存后生效。
4. 点击口述“默认”，再点击“保存快捷键”，页面恢复 `Control + P` 并提示“快捷键已保存并重新生效”。
5. 执行 `npm run lint`、`npm run build`、`cargo fmt --check --manifest-path src-tauri/Cargo.toml`、`cargo check --manifest-path src-tauri/Cargo.toml`，均通过。

截图证据：

- `output/playwright/aitool-hub-redesign-home-20260724.png`
- `output/playwright/aitool-hub-redesign-modes-20260724.png`
- `output/playwright/aitool-hub-redesign-shortcuts-final-20260724.png`
- `output/playwright/aitool-hub-redesign-settings-20260724.png`

未覆盖：

- 浏览器预览只能验证快捷键录制和本地保存；真实全局快捷键重新注册、物理按键触发录音仍需在 `.app` 中验证。

## 2026-07-24 Keychain、准备状态与真实快捷键引导复测

环境：

- 页面预览：`http://127.0.0.1:1421/index.html?mode=hub`
- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 分支：`dev-20260723`

执行与结果：

1. 设置页 Mimo API Key 输入框文案改为“保存到 macOS 钥匙串，不写入配置文件”，并新增“清除 Key”按钮。
2. 使用 macOS `security` 命令完成旧钥匙串 service 标识 `asia.aijob.aitool` 的测试 Key 写入、读取、删除闭环；复核测试值未遗留，已有用户钥匙串值不展示也不删除。
3. 首页新增准备状态面板，展示 Mimo Key、麦克风、辅助功能、快捷键状态；“打开设置”和“编辑”按钮均为真实跳转。
4. 悬浮条录音态改为按 WebAudio 采样估算音量并驱动九段波形；错误态增加轻震动画，结果窗口增加弹入动画。
5. 启动最新 `.app` 后不弹常驻窗口；模拟 `Control+P` 后出现 CodexMan 提示和 Hub，符合缺 Key 时引导设置的预期。
6. 执行 `npm run lint`、`npm run build`、`cargo fmt --check --manifest-path src-tauri/Cargo.toml`、`cargo check --manifest-path src-tauri/Cargo.toml`、`npx tauri build --bundles app`，均通过。

截图证据：

- `output/playwright/aitool-hub-keychain-health-20260724.png`
- `output/playwright/aitool-settings-keychain-20260724.png`

未覆盖：

- 未用用户真实 Mimo Key 测试钥匙串保存后的真实 ASR 请求。
- 实时波形已完成代码级和构建验证，真实麦克风音量手感需用户授权麦克风后确认。

## 2026-07-24 快捷键真实诊断与 App 图标资源复测

环境：

- 页面预览：`http://127.0.0.1:1422/?mode=hub`
- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 分支：`dev-20260723`

执行与结果：

1. 执行 `npm run lint`，通过。
2. 执行 `npm run build`，通过。
3. 执行 `cargo fmt --check --manifest-path src-tauri/Cargo.toml`，通过。
4. 执行 `cargo check --manifest-path src-tauri/Cargo.toml`，通过。
5. 执行 `git diff --check`，通过。
6. 执行 Mimo Key 扫描，未发现用户真实密钥落盘。
7. 打开 Hub 首页，确认快捷键诊断在网页预览下显示“仅桌面端可注册”，下一步引导仍指向真实桌面 App 使用路径。
8. 打开设置页并滚动到系统诊断区域，确认四个诊断卡片无文字挤压、无遮挡，快捷键诊断为真实状态入口。
9. 执行 `npx tauri build --bundles app`，通过并生成最新 `.app`。
10. 检查打包后 `Info.plist`：`CFBundleDisplayName=CodexMan`，`NSMicrophoneUsageDescription=CodexMan 需要使用麦克风录制语音并转换成文字。`，`CFBundleIconFile=icon.icns`。
11. 检查打包后资源：`Contents/Resources/icon.icns` 存在，文件类型为 Mac OS X icon。

截图证据：

- `output/playwright/codexman-shortcut-diagnostic-home-20260724.png`
- `output/playwright/codexman-shortcut-diagnostic-settings-bottom-20260724.png`

未覆盖：

- 真实快捷键注册失败路径需要用户在系统快捷键冲突环境中确认；当前已从代码和诊断状态保证失败原因可见。
- 真实 Mimo 转写、AI 整理、物理快捷键和目标输入框自动粘贴仍需用户授权后确认。

## 2026-07-24 Mimo Keychain 真实接口冒烟

环境：

- Key 来源：macOS Keychain，service `asia.aijob.aitool`，account `mimo-api-key`
- ASR 模型：`mimo-v2.5-asr`
- AI 模型：`mimo-v2.5`
- 分支：`dev-20260723`

执行与结果：

1. 仅检查 Keychain 条目是否存在，不打印密钥：存在。
2. 使用系统 `say` 生成短音频 `output/e2e/codexman-asr-smoke.wav`。
3. 使用与 App 一致的 OpenAI 兼容 `chat/completions` 请求体调用 Mimo ASR。
4. ASR 请求成功：HTTP `200`，耗时 `927ms`，模型 `mimo-v2.5-asr`，识别文本 `TypeSess测试语音输入。`。
5. 使用同一 Key 调用 Mimo 文本模型做 AI 整理冒烟。
6. AI 请求成功：HTTP `200`，耗时 `2237ms`，模型 `mimo-v2.5`，输出文本 `TypeSess测试语音输入。`。

证据文件：

- `output/e2e/codexman-asr-smoke-result.json`
- `output/e2e/codexman-ai-smoke-result.json`

未覆盖：

- 该冒烟验证真实 Mimo API 可用，但没有覆盖真实麦克风、物理快捷键、系统辅助功能授权和自动粘贴。

## 2026-07-24 用户确认后的最终收口复测

环境：

- 页面预览：历史截图来自本地 Vite 预览。
- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 分支：`dev-20260723`

执行与结果：

1. 复核 Hub 首页：只展示真实准备状态、最近结果、本地统计和开始当前模式入口；未出现无功能按钮或 demo/mock 数据。
2. 复核语音模式页：口述、翻译、随便问三种模式可切换，开始按钮会根据准备状态显示真实动作或继续配置。
3. 复核快捷键页：快捷键可以录制、恢复默认、保存；保存失败后前端会同步回 Rust 当前真实运行快捷键，避免旧快捷键失效。
4. 复核历史页：每条历史记录展示录音、转写、AI 总结耗时；原文和整理结果不一致时可展开查看。
5. 复核悬浮条：只在快捷键或 Hub 开始后出现；位于屏幕顶部下方；无阴影；确认/取消按钮 hover 有缩放动效；错误原因以气泡形式展示在工具条上。
6. 复核动画：Hub 进入、页面切换、卡片出现、按钮 hover、危险操作二次确认、处理态扫光和结果弹窗弹入均有过渡。
7. 复核 Logo/切图：桌面图标、Hub Logo 和输出切图文件均存在。

截图证据：

- `output/playwright/codexman-final-polish-home-20260724.png`
- `output/playwright/codexman-final-polish-modes-20260724.png`
- `output/playwright/codexman-final-polish-history-timing-20260724.png`
- `output/playwright/codexman-floating-nudge-busy-20260724.png`
- `output/playwright/codexman-final-delivery-shortcuts-20260724.png`
- `output/playwright/codexman-brand-clean-settings-20260724.png`

未覆盖：

- 本批没有主动向用户当前输入框模拟粘贴，避免误触真实工作内容；最终粘贴手感以用户本机实际授权和焦点输入框为准。

## 2026-07-24 真实桌面链路验证与空语音修复

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 目标输入框：TextEdit 临时文档
- 快捷键：`Control + P`
- 分支：`dev-20260723`

执行与结果：

1. 启动最新 `CodexMan.app`，App 后台驻留，不弹常驻 Hub。
2. TextEdit 临时文档保持焦点，内容为 `CodexMan 自动粘贴验证：`。
3. 发送第一次 `Control + P`，屏幕顶部下方出现录音胶囊，位置和无阴影符合预期。
4. macOS 弹出麦克风权限请求，允许后录音继续。
5. 播放系统测试语音后发送第二次 `Control + P`，录音停止并进入转写流程。
6. 因当前 `CodexMan.app` 未通过辅助功能授权，自动粘贴被系统阻止，结果兜底窗口出现并提示需要开启辅助功能权限。
7. 本次系统播放语音未被麦克风形成有效输入，上游返回“无实际内容输出”占位文案；已补修为空语音拦截，不再把占位文案当作成功结果。

截图证据：

- `output/e2e/codexman-real-shortcut-start-20260724.png`
- `output/e2e/codexman-permission-after-quartz-click-20260724.png`
- `output/e2e/codexman-real-chain-after-stop-20260724.png`
- `output/e2e/codexman-accessibility-settings-20260724.png`

未覆盖：

- 辅助功能权限仍需用户在系统设置中重新添加或切换当前 `CodexMan.app` 后复测。
- 最终语音输入效果需用户真实说话验证，系统扬声器播放未能稳定进入麦克风。

## 2026-07-24 最终视觉抛光与切图复测

环境：

- 页面预览：`http://127.0.0.1:1421/?mode=hub`
- 预览来源：`npm run build` 后的 `dist`
- 分支：`dev-20260723`

执行与结果：

1. 打开 Hub 首页，确认卡片圆角已加大，首页只展示真实准备状态、快捷键动作、最近结果和本地统计，没有新增无功能入口。
2. 切换到语音模式页，确认三张模式卡都有真实开始动作或继续配置动作，卡片入场和 hover 动画稳定。
3. 切换到快捷键页，确认三个快捷键编辑卡为真实录制/默认/保存入口，圆角和按钮排列没有挤压。
4. 临时写入一条浏览器本地历史，仅用于视觉截图；历史页展示 `录音 3.2s`、`转写 866ms`、`AI总结 1.3s`，并可展开查看原文。
5. 截图后清除临时 `aiToolVoiceHistoryV1`，不保留测试样本。
6. 打开悬浮条预览，确认仍是顶部胶囊样式、无阴影，取消和确认按钮保持 IconPark 图标与圆形 hover 效果。

截图证据：

- `output/playwright/codexman-polished-home-20260724.png`
- `output/playwright/codexman-polished-modes-20260724.png`
- `output/playwright/codexman-polished-shortcuts-20260724.png`
- `output/playwright/codexman-polished-history-timing-20260724.png`
- `output/playwright/codexman-polished-floating-pill-20260724.png`
- `output/e2e/codexman-polished-hub-app-20260724.png`

未覆盖：

- 本批不重复做长录音；后续真实语音链路按用户要求只录 2-4 秒做确认。

## 2026-07-24 权限等待态与短录音保护复测

环境：

- 页面预览：`http://127.0.0.1:1421`
- 预览来源：`npm run build` 后的 `dist`
- 分支：`dev-20260723`

执行与结果：

1. 复核辅助功能等待态：打开系统辅助功能设置后，Hub 会显示“等待辅助功能授权”，首页下一步切换为“重新检查”，顶部提示进入检测中状态。
2. 复核结果窗口等待态：打开辅助功能设置后按钮进入“检测中”；超时或检测中断时会恢复“重新打开辅助功能设置”。
3. 复核短录音保护：录音低于 `800ms` 时前端直接提示“录音太短了，请说完一句话后再停止”，不继续请求 Mimo。
4. 复核错误气泡：顶部气泡能展示短录音错误原因，位置和样式稳定。

截图证据：

- `output/playwright/codexman-accessibility-watch-state-20260724.png`
- `output/playwright/codexman-short-recording-toast-20260724.png`

未覆盖：

- 真实 macOS 权限勾选动作需要用户操作；授权成功后的自动刷新需要在真实系统设置里确认。

## 2026-07-24 权限后短录音自动粘贴复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 目标输入框：TextEdit 临时文档
- 快捷键：`Control + P`
- 分支：`dev-20260723`

执行与结果：

1. 用户确认当前 `CodexMan.app` 已获得辅助功能权限。
2. TextEdit 临时文档保持焦点，内容为 `CodexMan 权限后自动粘贴验证：`。
3. 发送第一次 `Control + P`，开始短录音。
4. 播放一句短测试语音后发送第二次 `Control + P`，录音停止并进入转写流程。
5. Hub 历史记录新增 `TextEdit` 记录，展示 `录音 3.3s`、`转写 1.0s`、`AI总结 11s`。
6. TextEdit 文档内容变为 `收你的成本。CodexMan 权限后自动粘贴验证：`，确认结果已自动粘贴到聚焦输入框。

截图证据：

- `output/e2e/codexman-real-short-chain-paste-success-20260724.png`
- `output/e2e/codexman-real-short-chain-history-timing-2-20260724.png`

备注：

- 本次按用户要求只做数秒短录音验证；识别文本不完全准确，主要受系统扬声器到麦克风的收音质量影响。

## 2026-07-24 口述 AI 润色开关复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 页面预览：`http://127.0.0.1:1421/?mode=hub`
- 分支：`dev-20260723`

执行与结果：

1. Hub 顶部新增 `口述润色` 开关，设置页保留 `口述后 AI 润色` 开关，两处状态来自同一个 `postProcessDictation` 配置。
2. 关闭开关后，口述模式只等待 Mimo ASR；`transcribe_audio` 返回后直接使用原始转写作为输出，`processElapsedMs` 记录为 `0`。
3. 开启开关后，口述模式继续调用 `process_text`，历史记录展示实际 AI 润色耗时。
4. 翻译和随便问不受该开关影响，仍然需要 AI 文本处理。

截图证据：

- `output/playwright/codexman-dictation-polish-switch-20260724.png`

未覆盖：

- 本批不做长录音；真实速度手感按用户要求只需要后续录 2-4 秒确认。

## 2026-07-24 Typeless 风格托盘菜单复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 分支：`dev-20260723`

执行与结果：

1. 托盘图标取消 macOS template 模式，改为显示 CodexMan 彩色 Logo。
2. 托盘左键不再直接打开 Hub，改为弹出菜单。
3. 菜单项按 Typeless 参考结构补齐：`打开 CodexMan 主页`、`显示历史记录`、`将词汇添加到词典`、`设置...`、`选择麦克风`、版本、`检查更新...`、`退出 CodexMan`。
4. `打开 CodexMan 主页`、`显示历史记录`、`设置...` 会打开 Hub 并切换到对应视图。
5. `将词汇添加到词典` 会读取系统剪贴板文本，写入本地词典并切到词典页；剪贴板为空时展示真实错误提示。
6. `选择麦克风` 子菜单提供系统默认、打开麦克风设置和刷新麦克风列表入口。
7. `检查更新...` 在未接入在线更新通道前展示当前版本状态，不伪装成已有在线更新能力。

截图证据：

- `output/e2e/codexman-tray-menu-20260724.png`

未覆盖：

- 本批不录音；托盘菜单视觉位置和最终图标显示需要以用户当前 macOS 菜单栏实机观感为准。

## 2026-07-24 Logo 透明圆角切图复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 图标源：`src-tauri/icons/icon.png`
- 分支：`dev-20260723`

执行与结果：

1. 从现有 CodexMan 图标生成透明圆角源图，只保留原 ts 字母、波形、颜色和内部圆角图形，不改变功能逻辑。
2. 使用 Tauri 图标生成流程重新切出标准 PNG、`.icns`、`.ico`、iOS、Android 和 Windows 图标资源。
3. 同步更新 Hub 使用的 `src/assets/codexManLogo.png`，以及输出预览图 `output/assets/codexman-logo-128.png`、`output/assets/codexman-logo-512.png`。
4. 复核主 Logo、Tauri 512 图标、32/64/128/256 PNG 和输出预览图均为 RGBA，四角 alpha 为 0，不再是黑色方底。
5. 执行静态检查和桌面打包，确认最新 `.app` 可生成。

验证证据：

- `src/assets/codexManLogo.png`：`256x256` RGBA，四角透明。
- `src-tauri/icons/icon.png`：`512x512` RGBA，四角透明。
- `src-tauri/icons/icon.icns`、`src-tauri/icons/icon.ico`：已重新生成。
- `output/assets/codexman-logo-128.png`、`output/assets/codexman-logo-512.png`：已同步为 RGBA 圆角图。
- 圆角透明预览图：`output/e2e/codexman-logo-rounded-preview-20260724.png`。

未覆盖：

- macOS 可能缓存旧 App 图标；本轮已重新注册最新 `.app`，如 Finder/Dock 仍短暂显示旧图标，以系统缓存刷新为准。

## 2026-07-24 Logo 去除外部边距复测

环境：

- App：`src-tauri/target/release/bundle/macos/CodexMan.app`
- 图标源：`src-tauri/icons/icon.png`
- 分支：`dev-20260723`

执行与结果：

1. 检测上一版 512 图标的 alpha 有效边界为 `47,47-464,464`，存在约 `47px` 外部透明边距。
2. 按 alpha 有效边界裁切并等比放大到整张 `512x512` 画布，保留四角透明和内部 ts 字母、波形、颜色。
3. 重新生成 Tauri 标准 PNG、`.icns`、`.ico`、iOS、Android 和 Windows 图标资源。
4. 同步更新 Hub 使用的 `src/assets/codexManLogo.png`，以及输出预览图 `output/assets/codexman-logo-128.png`、`output/assets/codexman-logo-512.png`。
5. 复核新版 `src-tauri/icons/icon.png` alpha 有效边界为 `0,0-511,511`，确认外部透明边距已去掉。

验证证据：

- `src-tauri/icons/icon.png`：`512x512` RGBA，alpha 有效边界 `0,0-511,511`。
- `src/assets/codexManLogo.png`：`256x256` RGBA，alpha 有效边界 `0,0-255,255`。
- 无外部边距预览图：`output/e2e/codexman-logo-no-padding-preview-20260724.png`。

未覆盖：

- macOS Finder/Dock/菜单栏可能缓存旧图标；需要以重新打包并重新注册后的 App 为准。
