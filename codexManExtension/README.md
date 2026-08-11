# typesass-extension

可直接加载到 Chrome 的 Manifest V3 多点元素标注插件。插件会像 Codex 右侧浏览器一样 hover 高亮页面元素，点击页面元素添加编号 marker，编辑每个点的描述，最后统一发送到 Typesass App 创建任务，并保留 Browser comments 报告文件。

## 使用方式

1. 打开 Typesass App，并在任务管理中创建至少一个项目。
2. 打开 Chrome 扩展管理页：`chrome://extensions/`。
3. 开启“开发者模式”。
4. 点击“加载已解压的扩展程序”，选择解压后的 `typesass-extension` 目录。
5. 点击插件图标，确认 App 服务已连接。
6. 首次使用点击“获取授权码”，插件会调用 App 授权码接口并保存授权码。
7. 点击“读取项目”，选择任务要创建到哪个 Typesass 项目。
8. 打开任意普通网页或本地页面，点击扩展按钮，或在页面内右键选择“启用 Typesass 选择器”。
9. 在页面中 hover 元素，点击目标元素添加一个编号点。
10. 在弹出的描述框里填写说明，点击“保存”。
11. 可以继续点击其它元素添加多个点，也可以点击已有编号 marker 修改描述或删除。
12. 点击右下角“发送全部”，插件会在 App 中创建任务，并下载一份 Browser comments HTML 报告。

## 创建任务格式

- 任务标题：使用点位描述；多个点位时按描述顺序拼接。
- 任务内容：使用 Codex Browser comments 风格文本，保留 `# Browser comments`、`User Comment`、`Page URL`、`Target selector`、`Target path`、`Saved marker screenshot` 和 `Comment` 等字段。
- 截图报告：每个点位单独截图，操作条、编辑框和生成提示不会进入截图。

## 快捷键

- `Esc`：非编辑态退出标注模式；编辑态只关闭当前编辑框。
- `Cmd/Ctrl + Enter`：统一发送全部标注并创建任务。

## 限制

- Chrome 内置页、Chrome Web Store 等受限页面无法注入插件。
- 跨域 iframe 内部 DOM 无法直接选取，只能选中 iframe 元素本身。
- 任务接口只接收文本内容；截图会进入本地 HTML 报告，不会作为附件上传到 App。
