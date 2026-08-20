/**
 * 在 Chrome DevTools 中注册 codexMan 请求辅助面板。
 * 流程：DevTools 打开时创建独立面板，面板内部负责读取 Network 请求并创建 fix 任务。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：Chrome 限制无法注入原生 Network 右键菜单，因此使用自定义面板承载请求列表。
 */
chrome.devtools.panels.create('codexMan', 'icons/icon32.png', 'networkPanel.html');
