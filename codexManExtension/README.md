# codexMan

可直接加载到 Chrome 的 Manifest V3 多点元素标注插件。插件会在网页中高亮元素，点击页面元素添加编号标记，编辑每个点的描述，最后统一发送到 codexMan 创建任务。

## 使用方式

1. 打开 codexMan，并在任务管理中创建至少一个项目。
2. 打开 Chrome 扩展管理页：`chrome://extensions/`。
3. 开启“开发者模式”。
4. 点击“加载已解压的扩展程序”，选择解压后的 `typesass-extension` 目录。
5. 点击插件图标，确认 codexMan 已连接。
6. 首次使用点击“获取授权码”，codexMan App 会弹出“是否确认授权”窗口；点击“确认授权”后插件会保存授权码。
7. 点击“读取项目”，选择任务要创建到哪个 codexMan 项目。
8. 打开任意普通网页或本地页面，点击扩展按钮，或在页面内右键选择“启用 codexMan 选择器”。
9. 在页面中 hover 元素，点击目标元素添加一个编号点。
10. 在弹出的描述框里填写说明，点击“保存”。
11. 可以继续点击其它元素添加多个点，也可以点击已有编号 marker 修改描述或删除。
12. 点击右下角“发送全部”，插件会在 codexMan 中创建任务。

## DevTools 请求 fix 任务

1. 打开目标页面的 Chrome DevTools。
2. 切换到 `codexMan` 面板。
3. 刷新页面或执行业务操作，面板会自动收集 Network 请求。
4. 在请求列表中按 URL、Method 或状态码筛选目标接口。
5. 点击某条请求查看右侧详情，可复制 cURL，也可以填写当前问题描述。
6. 在右侧选择任务项目后，点击“创建 fix 任务”，插件会读取该请求的 cURL、响应状态、响应头和响应内容，并在选中的 codexMan 项目中创建一个 fix 任务。

DevTools 面板复用插件弹窗中的应用授权码和项目列表接口；首次使用前仍需在插件弹窗中完成授权。创建 fix 任务必须在 DevTools 详情区选择任务项目。

## 创建任务格式

- 任务标题：使用点位描述；多个点位时按描述顺序拼接。
- 任务内容：使用 Codex Browser comments 风格文本，保留 `# Browser comments`、`User Comment`、`Page URL`、`Target selector`、`Target path`、`Saved marker screenshot` 和 `Comment` 等字段。
- 发送结果：任务创建成功后只展示页面提示，不自动下载文件；每条评论会把当前视口内的元素截图以内联 Markdown 图片写入任务内容。

## 快捷键

- `Esc`：非编辑态退出标注模式；编辑态只关闭当前编辑框。
- `Cmd/Ctrl + Enter`：统一发送全部标注并创建任务。

## 限制

- Chrome 内置页、Chrome Web Store 等受限页面无法注入插件。
- 跨域 iframe 内部 DOM 无法直接选取，只能选中 iframe 元素本身。
- 任务接口只接收文本内容；截图会以内联 Markdown 图片写入任务内容，不会作为独立附件上传到 codexMan。
- Chrome 不开放向原生 Network 右键菜单注入自定义项，因此请求 fix 功能在独立的 `codexMan` DevTools 面板中提供。
- DevTools 对部分缓存、重定向、预检、二进制或跨域响应可能无法返回响应体；这种情况下任务会保留 cURL、状态和响应头，并标明响应体未读取。
