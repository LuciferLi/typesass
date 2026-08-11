# CodexMan

![CodexMan logo](src/assets/codexManLogo.png)

Official website: <https://codexman.tolern.com/>

CodexMan 是一款基于 Tauri、Vue 和 FastAPI 的语音输入工具。当前首发版本提供真实闭环的语音转文字、语音转文字后 AI 润色、现有文本 AI 润色、词典、历史记录、桌面快捷键和 CodeX 会话浏览。

## 当前可用功能

- 通过全局快捷键开始语音转文字或语音润色。
- 浏览器和桌面端统一通过独立 FastAPI HTTP 服务调用 ASR 与文本处理。
- 桌面端可读取选中文本、润色并尝试自动粘贴；无法确认插入时显示可复制结果，不伪报成功。
- 维护语音词典、模块内历史、主题、开机启动与系统权限诊断。
- 只读浏览并打开本机 CodeX 工作空间和已有会话。
- 第三方可依据运行时 OpenAPI、稳定错误码、requestId 和 Retry-After 接入。

模型管理用于维护本机 ASR 与文本模型、默认模型及启停状态；API Key 仅保存到 macOS Keychain。任务管理用于维护项目和 CodeX 任务，首发版使用单 worker 按排队顺序执行，只有当前任务进入可靠终态后才领取下一项，并以真实的 CodeX 终态事件推进状态。翻译、语音问答和实时字幕尚未达到端到端生产标准，本版本不提供入口、快捷键或对外承诺。

## 默认快捷键

| 功能 | 快捷键 |
| --- | --- |
| 语音转文字 | `Control + Shift + D` |
| 语音转文字并润色 | `Control + P` |
| 选中文本润色 | `Control + Shift + P` |

## 开发

要求 Node.js 20+、npm、Rust/Cargo；Sidecar 构建固定使用 CPython 3.9，构建脚本会在 macOS 自动探测系统/Xcode 与 PATH 中的合规解释器，不依赖当前 Shell 的 `python3` 指向。只有需要覆盖自动选择时才设置 `AITOOL_PYTHON=/absolute/path/to/python3.9`；最终用户不需要安装 Python。

App 启动时自动托管固定地址为 `http://127.0.0.1:18080` 的本机 HTTP 服务，用户无需配置 IP 或域名。HTTP 服务契约见 [server/README.md](server/README.md)：

```bash
npm install
npm run dev
```

浏览器通过设备码流程取得 8 小时工作会话 Token：浏览器生成并展示 userCode，用户在桌面 App 的 HTTP 文档页手工批准，再由原浏览器轮询领取 Token。批准使用的临时 Basic 凭据只在 Rust 内存中，普通浏览器和 WebView JS 都无法读取；机密服务端客户端仍可调用 `POST /v1/auth/token`。Mimo Key、上游地址、模型和长期调用 secret 禁止进入前端构建变量或客户端配置。

浏览器来源不参与本机 HTTP 服务访问判断；`/health` 和设备码创建可直接访问。模型、任务、Codex 状态等敏感接口只依赖 Bearer/Basic/设备码流程鉴权；错误响应中的 `retryable` 决定是否允许重试，存在 `Retry-After` 时优先按该值等待。

## 验证

```bash
npm run lint
npm run typecheck
npm run build
cd src-tauri && cargo check && cargo test
cd ../server && .venv-test/bin/python -m pytest --cov=app --cov-branch
```

## License

MIT
