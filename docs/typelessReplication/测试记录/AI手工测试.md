# AI 手工测试

## 2026-07-24 Hub 视觉检查

检查对象：

- `/tmp/aitool-final-hub.png`
- `.playwright-cli/page-2026-07-24T02-58-15-358Z.png`

结论：

- Hub 仪表盘已渲染，非空白。
- 首页信息密度符合轻量工具：左侧导航、真实本地统计卡、快捷键卡、最近结果面板均可见。
- 设置页已渲染，Mimo 配置、麦克风、快捷键、个人偏好、保存/恢复按钮均可见。
- 页面未出现明显文字重叠、按钮溢出或主区域空白；麦克风刷新按钮已调整为稳定横向按钮。
- 悬浮录音条仍然不是常驻主界面，主窗口打开后显示 Hub。

待用户确认：

- 快捷键真实按下后的悬浮条位置和录音状态。
- 错误气泡在真实失败路径中的展示位置。
- 真实录音、AI 整理、翻译、随便问输出是否符合预期。

## 2026-07-24 参考风格视觉检查

检查对象：

- `/tmp/aitool-reference-style-hub-20260724.png`
- `/tmp/aitool-reference-style-settings-20260724.png`

结论：

- 首页已按参考图方向收敛为深色侧边栏、顶部标题栏、状态胶囊、深色卡片和紫色主按钮。
- 设置页已改成分组设置行和真实 toggle 开关，视觉上接近参考图的系统设置页结构。
- 可见按钮均对应真实功能或真实禁用态，没有仅用于展示的 demo/mock 入口。
- 首页首屏未发现白屏、明显遮挡、按钮文字溢出或内容互相覆盖。
- 浏览器预览下系统诊断明确显示“仅桌面端可检测/注册”，避免把桌面权限伪装成已验证。

待用户确认：

- `.app` 内诊断区的 Mimo Key、辅助功能和快捷键注册状态是否符合本机授权结果。
- 真实快捷键唤起悬浮条后，录音、转写和自动粘贴的最终体验。

## 2026-07-24 桌面交互动效视觉检查

检查对象：

- `/tmp/aitool-desktop-polish-hub-20260724.png`
- `/tmp/aitool-floating-top-20260724.png`
- `/tmp/aitool-missing-key-settings-20260724.png`

结论：

- 真实 `.app` Hub 首页为深色侧边栏和深色卡片结构，内容非空白，未发现明显遮挡、错位或按钮文字溢出。
- 悬浮录音条截图尺寸为 `132x46`，无阴影，按钮 hover/状态动效已通过 CSS 接入；窗口位置由原生层顶居中控制。
- 缺 Key 后自动跳到设置页，用户下一步操作明确，不需要在错误后手动找入口。
- 错误 toast 不再留下透明窗口；等待后窗口列表只保留 Hub 主窗口。
- Hub 关闭后隐藏到后台，重新按快捷键能唤回，符合菜单栏常驻工具的交互预期。

待用户确认：

- 真实录音时波形节奏、停止时处理动画和自动粘贴结果。
- 授权辅助功能后，转写结果是否稳定进入当前聚焦输入框。

## 2026-07-24 结果兜底与深色风格复核

检查对象：

- `/tmp/aitool-style-hub-20260724-result-fallback.png`
- 新增 `result` 窗口源码与样式

结论：

- Hub 保持深色侧栏、深色卡片、紫色主按钮和绿色状态点风格，首页数据来自本地历史统计，不是 mock 数据。
- 新增结果窗口只包含真实结果文本、失败原因、复制、关闭和按需辅助功能设置入口，没有 demo 按钮。
- 自动粘贴未授权或无输入焦点时不再展示“已成功粘贴”类误导文案，改为可复制结果窗口。
- 当前截图未发现主窗口白屏、明显遮挡、按钮文字溢出或内容重叠。

待用户确认：

- 真实无焦点或辅助功能未授权场景下，结果窗口是否按预期出现在屏幕顶部偏下。
- 用户授权辅助功能后，目标输入框聚焦时是否直接粘贴，不弹结果窗口。

## 2026-07-24 后台启动与参考风格复核

检查对象：

- `output/playwright/aitool-hub-20260724-latest.png`
- `output/playwright/aitool-result-20260724-latest.png`
- macOS 窗口枚举结果

结论：

- 最新 `.app` 启动后后台驻留，窗口列表中没有 typesass 可见窗口，符合“快捷键唤起悬浮条，不常驻屏幕”的要求。
- Hub 预览保持深色侧栏、紧凑卡片、紫色主操作和状态胶囊风格，视觉方向与用户参考图一致。
- 首页没有 demo/mock 数据：统计为本地历史计算值，最近结果为空时复制/重新整理处于禁用态。
- 设置页中的快捷键、Mimo、麦克风、开机启动、Dock、辅助功能诊断均有真实代码入口。
- 结果窗口在 `520x320` 真实尺寸下没有按钮挤压、文本遮挡或白屏；辅助功能按钮按权限原因条件显示。

待用户确认：

- 菜单栏图标打开 Hub 的实际手感。
- 物理快捷键、真实录音、Mimo 转写、AI 整理和自动粘贴的完整链路。

## 2026-07-24 权限与耗时体验视觉复核

检查对象：

- `output/playwright/aitool-hub-timing-20260724.png`
- `output/playwright/aitool-settings-microphone-20260724.png`
- `output/playwright/aitool-history-source-20260724.png`

结论：

- 最近结果的耗时摘要清晰展示录音、转写和 AI 处理时长，`866ms` 这种短耗时保留下来，速度感比 `00:00` 更准确。
- 设置页麦克风“授权/刷新”两个按钮均为真实入口，排列紧凑但未挤压。
- 历史记录的“查看原文”折叠区只在原文和最终输出不一致时出现，展开后可对比原始转写和整理结果。
- 三张截图均未发现白屏、遮挡、错位、按钮文字溢出或内容重叠。

待用户确认：

- 在真实 `.app` 中点击“授权”后的麦克风系统弹窗和授权成功状态。
- 授权后完整录音、Mimo 转写、AI 整理与自动粘贴链路。

## 2026-07-24 参考风格与真实按钮复核

检查对象：

- `output/playwright/aitool-hub-redesign-home-20260724.png`
- `output/playwright/aitool-hub-redesign-modes-20260724.png`
- `output/playwright/aitool-hub-redesign-shortcuts-final-20260724.png`
- `output/playwright/aitool-hub-redesign-settings-20260724.png`

结论：

- Hub 已调整为参考图同类的深色工具台结构：左侧窄导航、右侧深色内容容器、低圆角卡片、细分割线、紫色主按钮、绿色状态点。
- 首页没有无效功能入口；顶部只保留“刷新状态”和“开始当前模式”，两者均有真实事件绑定。
- 语音模式页三张卡均为真实流程入口；快捷键页录制、默认、保存均可操作并有状态反馈。
- 系统设置页保留的 Key、模型、语言、翻译目标、麦克风、历史、AI 整理、声音、静音、开机启动、Dock、偏好、诊断均对应现有代码入口。
- 当前截图未发现白屏、遮挡、错位、按钮文字溢出或假数据。

待用户确认：

- 真实 `.app` 中保存快捷键后，macOS 全局快捷键是否立即按新配置生效。
- 真实录音、Mimo 转写、AI 整理和自动粘贴完整链路。

## 2026-07-24 准备状态与 Keychain 视觉复核

检查对象：

- `output/playwright/aitool-hub-keychain-health-20260724.png`
- `output/playwright/aitool-settings-keychain-20260724.png`

结论：

- 首页准备状态面板没有假数据：浏览器预览下明确展示“仅桌面端可检测/注册”，麦克风数量来自 `enumerateDevices`。
- 准备状态面板和快捷键动作面板保持参考图深色工具台风格，没有按钮文字溢出、遮挡或错位。
- 设置页 Key 区域展示“保存到 macOS 钥匙串，不写入配置文件”，清除按钮为红色弱危险操作，视觉上和普通保存动作区分清楚。
- 新增面板没有引入无功能按钮；“打开设置”“编辑”“清除 Key”均有真实事件绑定。

待用户确认：

- 在真实 `.app` 中保存真实 Mimo Key 后，首页状态是否从“未配置”变成“钥匙串 Key 已就绪”。
- 麦克风授权后，录音悬浮条实时波形是否符合说话强弱变化。

## 2026-07-24 快捷键诊断与 App 图标视觉复核

检查对象：

- `output/playwright/typesass-shortcut-diagnostic-home-20260724.png`
- `output/playwright/typesass-shortcut-diagnostic-settings-bottom-20260724.png`
- `src-tauri/target/release/bundle/macos/typesass.app/Contents/Resources/icon.icns`

结论：

- 首页准备状态里的“快捷键”状态仍是可行动的真实诊断入口，没有新增测试按钮或无效功能。
- 网页预览下明确展示“仅桌面端可注册”，没有把桌面全局快捷键伪装成已验证。
- 设置页系统诊断四张卡片在底部视图中布局稳定，快捷键状态文案没有挤压、遮挡或溢出。
- App 包已经包含 `icon.icns`，Finder/Dock 侧不再依赖默认图标。

待用户确认：

- 真实 `.app` 中如果快捷键被系统占用，设置页是否正确显示失败原因并引导改键。
- 真实物理快捷键、录音、Mimo 转写、AI 整理和自动粘贴完整链路。

## 2026-07-24 最终圆润视觉复核

检查对象：

- `output/playwright/typesass-polished-home-20260724.png`
- `output/playwright/typesass-polished-shortcuts-20260724.png`
- `output/playwright/typesass-polished-history-timing-20260724.png`
- `output/playwright/typesass-polished-floating-pill-20260724.png`

结论：

- Hub 整体保持参考图方向的深色工具台风格，卡片圆角更大，左侧导航、顶部操作、状态胶囊和内容卡片没有文字溢出或遮挡。
- IconPark 图标体系仍然统一，未出现 emoji、自绘无功能图标或临时占位按钮。
- 历史记录明确展示录音、转写、AI 总结三段耗时；“查看原文”折叠区圆角和历史卡片风格一致。
- 悬浮条仍保持无阴影胶囊形态，确认/取消按钮圆润，图标尺寸稳定。
- 本批没有新增 demo/mock 数据；历史视觉样本只存在于截图流程中的临时 localStorage，已清除。

待用户确认：

- 最新 `.app` 中用真实麦克风说 2-4 秒后，转写、AI 整理和自动粘贴的完整手感。

## 2026-07-24 权限与误触反馈视觉复核

检查对象：

- `output/playwright/typesass-accessibility-watch-state-20260724.png`
- `output/playwright/typesass-short-recording-toast-20260724.png`

结论：

- 辅助功能等待态清晰可见，用户能知道系统设置打开后应用正在检测授权，不会误以为没有反应。
- 下一步按钮在等待态变为“重新检查”，是可执行动作，不是展示按钮。
- 短录音错误气泡直接展示原因，文本没有溢出，圆角和深色风格与悬浮条一致。
- 本批没有新增无功能界面或 mock 数据。

## 2026-07-24 自动粘贴诊断日志复核

检查对象：

- `output/playwright/typesass-diagnostic-log-20260724.png`
- `src/main.ts`
- `src-tauri/src/lib.rs`

结论：

- Hub 侧边栏已新增“诊断日志”真实栏目，空态显示“还没有诊断日志”，无日志时“复制日志”和“清空日志”处于禁用态。
- 日志记录快捷键触发、开始/停止录音、ASR、AI 处理、自动粘贴和异常路径，不保存用户转写正文，只保存字数、耗时、目标 App、前台 App、权限状态、剪贴板状态和粘贴方式。
- Rust `paste_text` 返回剪贴板写入状态、辅助功能状态、System Events/CoreGraphics 粘贴方式，以及发送前、激活后、发送后的前台 App，便于定位授权后偶发不粘贴的问题。
- 页面未发现白屏、遮挡、错位或按钮文字溢出；新增按钮均有真实复制/清空动作，不是 demo/mock。

验证命令：

- `npm run lint`：通过。
- `npm run build`：通过。
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `git diff --check`：通过。
- `npx tauri build --bundles app`：通过。

待用户确认：

- 在真实 `.app` 中复现“授权后偶发不粘贴”时，诊断日志里目标 App、发送前后前台 App 和粘贴方式是否能暴露焦点丢失或系统粘贴失效原因。

待用户确认：

- 在真实 macOS 辅助功能设置中勾选当前 `typesass.app` 后，Hub 是否自动刷新为已授权。

## 2026-07-24 翻译目标下拉选择视觉复核

检查对象：

- `output/playwright/typesass-translation-target-select-20260724.png`

结论：

- 设置页“翻译目标”已从手输输入框改为下拉选择，交互形态和“识别语言”一致。
- 下拉项覆盖常用目标语言：简体中文、繁体中文、英语、日语、韩语、法语、德语、西班牙语、葡萄牙语、俄语、意大利语、阿拉伯语、泰语、越南语、印尼语。
- 旧版本手输保存的自定义语言会作为“已保存”选项回填，不会因为改成下拉导致现有配置直接丢失。
- 本批没有新增 demo/mock 数据或无功能按钮。

待用户确认：

- 在真实 `.app` 中选择目标语言并保存后，用 `Control + T` 录一句话，确认翻译目标符合选择结果。

## 2026-07-24 模式设置拆分视觉复核

检查对象：

- `output/playwright/typesass-mode-settings-split-20260724.png`
- `output/playwright/typesass-dictate-settings-20260724.png`
- `output/playwright/typesass-translate-settings-20260724.png`
- `output/playwright/typesass-ask-settings-20260724.png`
- `output/playwright/typesass-system-settings-split-20260724.png`

结论：

- 语音模式页已移除“当前模式”面板，三张模式卡不再出现选中态或模式锁定概念。
- 口述、翻译、随便问卡片的开始按钮旁边均有 IconPark 设置按钮，点击后进入对应模式设置页。
- 口述设置只保留口述后 AI 润色和口述输出偏好；保存按钮有真实本地配置写入反馈。
- 翻译设置只保留目标语言下拉和翻译输出偏好；目标语言已从系统设置迁出。
- 随便问设置只保留回答偏好；系统设置只保留 Key、模型、识别语言、麦克风、历史、系统能力、通用输出偏好和诊断。
- 浏览器预览中未发现白屏、明显遮挡、按钮文字溢出或无功能入口。

待用户确认：

- 在真实 `.app` 中分别保存三个模式设置后，用对应快捷键录音确认实际 AI 输出风格符合各自配置。

## 2026-07-24 口述 AI 输出语言视觉复核

检查对象：

- `output/playwright/typesass-dictation-output-language-enabled-20260724.png`
- `output/playwright/typesass-dictation-output-language-disabled-20260724.png`

结论：

- 口述设置页在“口述后 AI 润色”下方新增“输出语言”下拉框。
- AI 润色开启时，下拉框可选择跟随原文、简体中文、繁体中文、英语、日语、韩语、法语、德语、西班牙语、葡萄牙语、俄语、意大利语、阿拉伯语、泰语、越南语、印尼语。
- AI 润色关闭时，下拉框置灰禁用，并提示关闭后会直接使用 ASR 原文。
- 浏览器预览中选择“英语”后点击“保存口述设置”，顶部反馈“设置已保存。”，选中状态保持正常。
- 当前页面未发现白屏、遮挡、错位、文字溢出或无功能入口。

待用户确认：

- 在真实 `.app` 中开启 AI 润色并选择目标输出语言后，用 `Control + P` 录一句话，确认 Mimo 返回结果符合语言设置。

## 2026-07-24 辅助功能移除后状态刷新复核

检查对象：

- `src/main.ts` Hub 诊断刷新逻辑

结论：

- 原逻辑只在 Hub 初始化、手动刷新、保存设置或从 App 内打开辅助功能设置后刷新诊断。
- 如果用户直接在 macOS 系统设置中移除 `typesass.app` 辅助功能权限，Hub 可能短时间保留旧的“已授权”展示。
- 本批新增 Hub 诊断自动刷新：窗口重新聚焦、页面重新可见、以及 Hub 打开时每 4 秒自动刷新一次。
- 自动刷新仍读取 Rust `get_runtime_diagnostics` 的真实 `accessibilityTrusted`，不是前端缓存或 mock 状态。

待用户确认：

- 在最新 `.app` 中移除辅助功能权限后，回到 Hub 或等待几秒，确认辅助功能状态变为“未授权”。

## 2026-07-24 多语言输出后自动粘贴回归复核

检查对象：

- `src/main.ts` 转写后调用 `paste_text` 的目标 App 参数。
- `src-tauri/src/lib.rs` 桌面端剪贴板与 Cmd+V 自动粘贴策略。

结论：

- 口述输出语言本身只会追加到 AI 文本处理提示词，不直接影响剪贴板写入。
- 多语言输出开启后 AI 处理耗时可能变长，期间悬浮窗/Hub 更容易让 macOS 焦点恢复不稳定。
- 旧粘贴策略会在没有记录到标准输入焦点时提前进入结果兜底，容易表现为“转写成功但没有粘贴”。
- 本批调整为：写入剪贴板后只检查辅助功能权限；权限通过后隐藏悬浮条和结果窗口，若记录到目标 App 则先切回目标 App，识别不到目标时也会等待 macOS 恢复上一个焦点后发送 Cmd+V。
- 当前没有新增 demo/mock 数据或无功能入口。

待用户确认：

- 在最新 `.app` 中从真实输入框内按 `Control + P` 开始和停止录音，开启口述 AI 输出语言后确认结果能自动进入输入框。

## 2026-07-24 自动粘贴目标和系统粘贴事件复测

检查对象：

- `src-tauri/src/lib.rs` 的外部目标 App 记忆、Hub 隐藏和系统粘贴触发。
- `src/main.ts` 的 Hub 启动录音事件 payload。

结论：

- 已确认 macOS 基准粘贴可用：TextEdit 正文获得焦点后，剪贴板内容可以通过 `System Events` 的 `Cmd+V` 写入文档。
- 已确认自动化脚本模拟 `Control + P` 不稳定：AppleScript / CoreGraphics 发送的 `Control + P` 不总是被 Tauri 全局快捷键插件当成真实快捷键，因此不能替代用户物理按键验收。
- 修复方向改为产品逻辑层：打开 Hub 前记住最近一次非 typesass 前台 App；Hub 前台启动录音时隐藏 Hub 并把目标 App 传给悬浮录音条；粘贴前隐藏 main/result/hub 三类临时窗口。
- 自动粘贴触发从单一 CoreGraphics 改为 `System Events` 优先，失败时回退 CoreGraphics，贴近本机基准粘贴成功路径。
- 本地自动化能确认写剪贴板和系统粘贴动作，但物理 `Control + P -> 录音 -> 停止 -> 粘贴` 仍需要用户手按确认。

待用户确认：

- 打开最新 `.app` 后，点进真实输入框，用物理 `Control + P` 开始和停止录音，确认文本是否直接进入当前输入框。

## 2026-07-25 重启后自动粘贴目标兜底复核

检查对象：

- `src-tauri/src/lib.rs` 的 `get_recording_target_app` 和 `paste_text` 目标 App 兜底。
- `src/main.ts` 的录音开始目标 App 确认逻辑。
- 最新打包产物 `src-tauri/target/release/bundle/macos/typesass.app`，打包时间 `2026-07-25 01:07`。

结论：

- 用户反馈退出并重新打开 App 后，转写能完成，但结果没有追加到当前输入框。
- 该现象更符合“重启后目标 App 为空或丢失，粘贴指令没有回到输入框所在应用”，不是 ASR 失败。
- 本批新增 Rust 侧录音目标读取命令：当前前台是 typesass 时回退到最近一次外部 App。
- 自动粘贴时若前端传入目标为空，会依次使用发送前的外部前台 App、Rust 运行期最近外部 App 作为兜底，再激活目标后发送 Cmd+V。
- 前端开始录音时也会主动读取 Rust 侧保存的目标 App，避免重启后第一次使用时 targetApp 在事件 payload 中丢失。
- 构建和静态检查已通过；未使用脚本模拟真实录音，避免污染用户体验结论。

验证命令：

- `npm run lint`：通过。
- `npm run build`：通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `git diff --check`：通过。
- `npx tauri build --bundles app`：通过。

待用户确认：

- 退出 typesass 后重新打开，聚焦真实输入框，按物理 `Control + P` 开始录音，说 2-4 秒，再按 `Control + P` 停止，确认文本是否直接追加到输入框。
- 如果仍失败，打开“诊断日志”查看最新粘贴记录中的目标、发送前、激活后、发送后和粘贴方式。

## 2026-07-25 ChatGPT WebView 粘贴事件复核

检查对象：

- 用户提供的 `2026-07-25 08:14` 诊断日志。
- `src-tauri/src/lib.rs` 的 `trigger_system_paste` 和焦点诊断。
- `src/main.ts` 的自动粘贴日志级别和焦点字段展示。

结论：

- 用户日志显示 ASR、AI 润色、剪贴板写入、辅助功能授权、目标 App 激活均正常。
- 日志同时显示 `System Events` 粘贴指令已发给 `ChatGPT`，但文字没有进入 ChatGPT 输入框。
- 这说明“App 已前台”和“输入框真实接收粘贴”不是同一件事；旧日志把指令发出记为“成功”，容易误导。
- 本批将系统粘贴改为优先使用更接近物理键盘事件的 `CoreGraphics`，仅在键盘事件创建失败时回退 `System Events`。
- 自动粘贴日志中“粘贴指令已发送”改为信息级别，不再把命令发出等同于实际插入成功。
- 诊断日志新增目标 App 内的焦点元素摘要：发送前、激活后、发送后，用于判断 ChatGPT 这类 WebView 是否还聚焦在输入框。
- 最新打包产物 `typesass.app/Contents/MacOS/ai-tool` 时间为 `2026-07-25 08:20:41`，已启动。

验证命令：

- `npm run lint`：通过。
- `npm run build`：通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `git diff --check`：通过。
- `npx tauri build --bundles app`：通过。

待用户确认：

- 在最新 `.app` 中重新聚焦 ChatGPT 输入框，用物理 `Control + P` 录 2-4 秒再停止，确认粘贴方式是否变为 `CoreGraphics`，以及文字是否进入输入框。
- 如果仍没有进入输入框，复制最新“粘贴”日志，重点看焦点发送前、焦点激活后、焦点发送后。

## 2026-07-25 前台抢占后粘贴重试复核

检查对象：

- 用户提供的 `2026-07-25 08:22` 诊断日志。
- `src-tauri/src/lib.rs` 的粘贴后前台校验和重试逻辑。

结论：

- 用户日志显示粘贴前目标为 `ChatGPT`，激活后也是 `ChatGPT`，但发送后系统前台变为 `System Settings`。
- 这说明第一次 `Cmd+V` 发送后被系统设置抢回前台，粘贴事件很可能没有落到 ChatGPT 输入框。
- 本批新增一次性补救：发送粘贴后如果前台不是目标 App，会重新激活目标 App，等待前台稳定后再补发一次 `Cmd+V`。
- 为避免重复粘贴，只有“发送后前台不是目标 App”时才会补发；发送后仍是目标 App 时不会重试。
- 日志中的“方式”会记录补救链路，例如 `CoreGraphics -> 前台被System Settings抢占，恢复目标后重试 -> 重试：CoreGraphics`。
- 最新打包产物 `typesass.app/Contents/MacOS/ai-tool` 时间为 `2026-07-25 08:26:10`，已启动。

验证命令：

- `npm run lint`：通过。
- `npm run build`：通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `git diff --check`：通过。
- `npx tauri build --bundles app`：通过。

待用户确认：

- 在最新 `.app` 中重新聚焦 ChatGPT 输入框，用物理 `Control + P` 录 2-4 秒再停止。
- 如果仍没有进入输入框，复制最新“粘贴”日志，重点看“方式”和“发送后”。

## 2026-07-25 录音开始不影响 Hub 显示复核

检查对象：

- `src-tauri/src/lib.rs` 的 `trigger_voice_mode`、`show_main_window` 和 `paste_text`。
- 最新打包产物 `src-tauri/target/release/bundle/macos/typesass.app`。

结论：

- 问题原因是 Rust 原生层在开始录音时主动执行了 `hub.hide()`，这是之前为了减少自动粘贴时焦点落回 typesass 的保护逻辑。
- 本批删除“开始录音/Hub 发起录音时隐藏 Hub”的逻辑，Hub 打开时按快捷键不会被主动收起。
- 当当前前台就是 typesass Hub 时，录音会发给后台录音窗口处理，不再弹出顶部悬浮胶囊，也不恢复外部 App 焦点，避免录制动作影响 Hub 主页面显示。
- 从 Hub 主界面发起的录音完成后会跳过自动粘贴，结果只更新到最近结果和历史记录，避免结束录音后 `paste_text` 隐藏 Hub。
- 当当前前台是外部输入 App 时，仍显示顶部悬浮胶囊并保留目标 App 焦点，保持快捷口述体验。
- 自动粘贴阶段仍保留隐藏 `main`、`result` 和 `hub` 的逻辑，避免最终文本粘贴回 typesass 自己。
- 最新 `.app` 已重新构建并启动，进程为 `typesass.app/Contents/MacOS/ai-tool`。

验证命令：

- `npm run lint`：通过。
- `npm run build`：通过。
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `git diff --check`：通过。
- `npx tauri build --bundles app`：通过。

待用户确认：

- 打开 Hub 后按物理快捷键开始录音，确认 Hub 不再立即消失，也不被顶部悬浮胶囊或目标 App 抢走显示。
- 再次按物理快捷键结束录音，确认 Hub 仍保持显示，最近结果或历史记录可看到本次输出。
- 聚焦真实输入框后再走一次完整录音和停止转写，确认粘贴阶段仍能把文本送回目标输入框。
