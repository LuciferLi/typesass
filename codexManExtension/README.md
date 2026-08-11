# codexManExtension

一个可直接加载到 Chrome 的 Manifest V3 多点元素标注插件。点击插件按钮后，可以像 Codex 右侧浏览器一样 hover 高亮页面元素，点击页面元素添加编号 marker，编辑每个点的描述，最后统一导出 Codex Browser comments 风格报告。

## 使用方式

1. 打开 Chrome 扩展管理页：`chrome://extensions/`。
2. 开启“开发者模式”。
3. 点击“加载已解压的扩展程序”。
4. 选择本目录：`devTool/codexManExtension`。
5. 打开任意普通网页或本地页面，点击扩展按钮，或在页面内右键选择“启用 CodexMan 选择器”。
6. 在页面中 hover 元素，点击目标元素添加一个编号点。
7. 在弹出的描述框里填写说明，点击“保存”。
8. 可以继续点击其它元素添加多个点，也可以点击已有编号 marker 修改描述或删除。
9. 点击右下角“发送全部”，报告会下载到浏览器默认下载目录，文件名类似 `codexManExtension-browser-comments-2026-08-11T...html`。

## 报告内容

- `Browser Comments`：可复制的 `# Browser comments` Markdown，字段对齐 Codex 选中元素发送格式。
- `Saved Marker Screenshot`：当前可视区截图，保留页面 marker 标注。
- `Data`：每个点的页面 URL、目标文本、selector、path、位置和元素属性快照。

## 快捷键

- `Esc`：退出当前元素标注模式。
- `Cmd/Ctrl + Enter`：统一发送全部标注并导出报告。

## 限制

- Chrome 内置页、Chrome Web Store 等受限页面无法注入插件。
- 跨域 iframe 内部 DOM 无法直接选取，只能选中 iframe 元素本身。
- 当前版本截取的是可视区内的元素区域；元素超出可视区时，报告会保存当前可见部分。
