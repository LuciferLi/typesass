# typesass

轻量语音转文字桌面工具。当前版本已经具备 Typeless 风格核心闭环：

- 填写小米 Mimo API Key
- 全局快捷键唤起录音
- 停止后自动转写、整理、翻译或问答
- 口述/翻译结果自动粘贴到当前输入框
- 口述可一键关闭 AI 润色，直接粘贴原始转写
- 无法粘贴或随便问时在 Hub 展示结果
- 本地历史、词典、设置和 Typeless 风格托盘菜单

默认快捷键：

- 口述：`Control + P`
- 翻译：`Control + T`
- 随便问：`Control + Space`

## 本地预览

当前机器如果还没有 Rust/Cargo，可以先用网页预览模式验证转写效果：

```bash
npm install
npm run build
npm run preview:web
```

打开终端里显示的地址后，在页面输入 Mimo API Key 再录音。

也可以用环境变量提供密钥：

```bash
MIMO_API_KEY=你的密钥 npm run preview:web
```

## Tauri 桌面端

安装 Rust 后运行：

```bash
npm install
npm run dev
```

也可以用环境变量提供密钥，避免把 Key 填进界面：

```bash
MIMO_API_KEY=你的密钥 npm run dev
```

打包：

```bash
npm run tauri:build
```

## 默认配置

- Base URL: `https://token-plan-cn.xiaomimimo.com/v1`
- ASR 模型: `mimo-v2.5-asr`
- AI 模型: `mimo-v2.5`
- 语言: 自动识别

## 安全说明

当前版本不会把 API Key 写死在代码里，也不会保存到 localStorage。桌面端设置页输入的 Key 会写入 macOS 钥匙串；也可以通过 `MIMO_API_KEY` 环境变量提供。
