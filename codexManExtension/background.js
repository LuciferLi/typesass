const CODEXMAN_CONTEXT_MENU_ID = "typesass-start-picker";
const TYPESASS_API_BASE_URL = "http://127.0.0.1:18080";
const TYPESASS_STORAGE_KEYS = {
  accessToken: "typesassAccessToken",
  projectId: "typesassProjectId"
};

/**
 * 注册浏览器右键菜单入口，便于在页面内直接启用选择器。
 * @returns {void}
 */
function registerContextMenu() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: CODEXMAN_CONTEXT_MENU_ID,
      title: "启用 Typesass 选择器",
      contexts: ["page", "selection", "link", "image", "video", "audio", "editable"]
    });
  });
}

/**
 * 读取插件本地保存的 App 授权码和任务项目 ID。
 * @returns {Promise<{accessToken:string, projectId:string}>} 当前插件设置。
 */
async function getTypesassSettings() {
  const settings = await chrome.storage.local.get([
    TYPESASS_STORAGE_KEYS.accessToken,
    TYPESASS_STORAGE_KEYS.projectId
  ]);
  return {
    accessToken: String(settings[TYPESASS_STORAGE_KEYS.accessToken] || ""),
    projectId: String(settings[TYPESASS_STORAGE_KEYS.projectId] || "")
  };
}

/**
 * 保存插件与 Typesass App 通信用的授权码和任务项目 ID。
 * @param {{accessToken?:string, projectId?:string}} settings 待保存设置。
 * @returns {Promise<void>} 无返回值。
 */
async function saveTypesassSettings(settings) {
  await chrome.storage.local.set({
    [TYPESASS_STORAGE_KEYS.accessToken]: String(settings.accessToken || "").trim(),
    [TYPESASS_STORAGE_KEYS.projectId]: String(settings.projectId || "").trim()
  });
}

/**
 * 调用 Typesass App 本机 HTTP 服务。
 * @param {string} path 接口路径。
 * @param {{method?:string, payload?:unknown, token?:string, timeoutMs?:number}=} options 请求配置。
 * @returns {Promise<unknown>} 解析后的 JSON 响应。
 */
async function requestTypesassApi(path, options = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), options.timeoutMs || 12000);
  const headers = { Accept: "application/json", "X-Request-ID": crypto.randomUUID() };
  if (options.payload !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  if (options.token) {
    headers.Authorization = `Bearer ${options.token}`;
  }
  try {
    const response = await fetch(`${TYPESASS_API_BASE_URL}${path}`, {
      method: options.method || "GET",
      headers,
      body: options.payload === undefined ? undefined : JSON.stringify(options.payload),
      cache: "no-store",
      signal: controller.signal
    });
    const text = await response.text();
    const json = text ? JSON.parse(text) : null;
    if (response.ok) {
      return json;
    }
    const error = json?.error || {};
    throw new Error(error.message || `Typesass HTTP 请求失败（${response.status}）`);
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError") {
      throw new Error("Typesass App HTTP 服务请求超时，请确认 App 已打开。");
    }
    if (error instanceof TypeError) {
      throw new Error("无法连接 Typesass App，请先打开 App 后重试。");
    }
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * 检查 Typesass App 本机 HTTP 服务是否可用。
 * @returns {Promise<{ok:boolean, name:string}>} 健康检查结果。
 */
async function checkTypesassHealth() {
  return await requestTypesassApi("/health", { timeoutMs: 1600 });
}

/**
 * 申请 App 授权码并保存到插件本地。
 * @returns {Promise<{accessToken:string, expiresAt:string|null}>} 已批准的明文授权码。
 */
async function requestTypesassAccessToken() {
  const response = await requestTypesassApi("/v1/access-tokens/request", {
    method: "POST",
    payload: {
      name: "typesass-extension",
      expiresAt: null
    }
  });
  if (response?.status !== "approved" || !response.accessToken) {
    throw new Error("授权申请未通过，请在 Typesass App 中确认授权。");
  }
  const settings = await getTypesassSettings();
  await saveTypesassSettings({ ...settings, accessToken: response.accessToken });
  return response;
}

/**
 * 读取 App 任务项目列表，供插件选择创建任务的目标项目。
 * @param {string} token App 授权码。
 * @returns {Promise<Array<{id:string,name:string,workspacePath:string}>>} 项目列表。
 */
async function listTypesassProjects(token) {
  if (!token) {
    throw new Error("请先在插件中获取并保存 App 授权码。");
  }
  const data = await requestTypesassApi("/v1/task-workspace/query", {
    method: "POST",
    payload: {},
    token
  });
  return Array.isArray(data?.projects) ? data.projects : [];
}

/**
 * 解析本次创建任务使用的项目 ID。
 * @param {string} token App 授权码。
 * @returns {Promise<string>} 任务项目 ID。
 */
async function resolveTypesassProjectId(token) {
  const settings = await getTypesassSettings();
  if (settings.projectId) {
    return settings.projectId;
  }
  const projects = await listTypesassProjects(token);
  if (projects.length === 1 && projects[0]?.id) {
    await saveTypesassSettings({ ...settings, projectId: projects[0].id });
    return projects[0].id;
  }
  if (projects.length === 0) {
    throw new Error("Typesass App 中还没有任务项目，请先在任务管理页面创建项目。");
  }
  throw new Error("检测到多个任务项目，请先在插件弹窗中选择目标项目。");
}

/**
 * 根据多个点位描述生成任务标题。
 * @param {Array<{comment?:string}>} comments 浏览器评论列表。
 * @returns {string} 任务标题。
 */
function buildTaskTitle(comments) {
  const descriptions = comments
    .map((comment) => String(comment.comment || "").trim())
    .filter(Boolean);
  const title = descriptions.length > 0 ? descriptions.join(" / ") : "浏览器元素标注任务";
  return title.slice(0, 200);
}

/**
 * 调用 App HTTP 接口创建任务，任务内容保持 Codex Browser comments 格式。
 * @param {{markdown:string, comments:Array<{comment?:string}>}} payload 多点标注数据。
 * @returns {Promise<{createdTaskId:string, title:string}>} 创建结果。
 */
async function createTypesassTask(payload) {
  const settings = await getTypesassSettings();
  if (!settings.accessToken) {
    throw new Error("请先在插件弹窗中获取 App 授权码。");
  }
  const projectId = await resolveTypesassProjectId(settings.accessToken);
  const title = buildTaskTitle(payload.comments || []);
  const response = await requestTypesassApi("/v1/tasks", {
    method: "POST",
    token: settings.accessToken,
    payload: {
      projectId,
      title,
      prompt: payload.markdown || title
    }
  });
  if (!response?.createdTaskId) {
    throw new Error("Typesass App 没有返回创建的任务 ID。");
  }
  return { createdTaskId: response.createdTaskId, title };
}

/**
 * 将 data URL 转换为 Blob，便于浏览器下载报告文件。
 * @param {string} dataUrl 原始 data URL。
 * @returns {Blob} Blob 对象。
 */
function dataUrlToBlob(dataUrl) {
  const [metadata, payload] = dataUrl.split(",");
  const mimeMatch = metadata.match(/data:([^;]+)/);
  const mimeType = mimeMatch?.[1] || "application/octet-stream";
  const binary = atob(payload);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return new Blob([bytes], { type: mimeType });
}

/**
 * 按元素在视口内的位置裁剪当前标签页截图。
 * @param {string} screenshotDataUrl 当前标签页完整可视区截图。
 * @param {{left:number, top:number, width:number, height:number}} rect 目标元素视口矩形。
 * @param {number} devicePixelRatio 当前页面像素比。
 * @returns {Promise<string>} 裁剪后的 PNG data URL。
 */
async function cropElementScreenshot(screenshotDataUrl, rect, devicePixelRatio) {
  const blob = dataUrlToBlob(screenshotDataUrl);
  const bitmap = await createImageBitmap(blob);
  const scale = devicePixelRatio || 1;
  const sourceX = Math.max(0, Math.round(rect.left * scale));
  const sourceY = Math.max(0, Math.round(rect.top * scale));
  const sourceWidth = Math.max(1, Math.min(bitmap.width - sourceX, Math.round(rect.width * scale)));
  const sourceHeight = Math.max(1, Math.min(bitmap.height - sourceY, Math.round(rect.height * scale)));
  const canvas = new OffscreenCanvas(sourceWidth, sourceHeight);
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("无法创建截图画布");
  }
  context.drawImage(bitmap, sourceX, sourceY, sourceWidth, sourceHeight, 0, 0, sourceWidth, sourceHeight);
  const croppedBlob = await canvas.convertToBlob({ type: "image/png" });
  return await blobToDataUrl(croppedBlob);
}

/**
 * 将 Blob 转成 data URL，便于嵌入单文件 HTML 报告。
 * @param {Blob} blob 图片 Blob。
 * @returns {Promise<string>} data URL。
 */
function blobToDataUrl(blob) {
  return blob.arrayBuffer().then((buffer) => {
    const bytes = new Uint8Array(buffer);
    let binary = "";
    bytes.forEach((byte) => {
      binary += String.fromCharCode(byte);
    });
    return `data:${blob.type || "application/octet-stream"};base64,${btoa(binary)}`;
  });
}

/**
 * 生成可直接打开查看的单文件 HTML 报告。
 * @param {Record<string, unknown>} payload 元素信息。
 * @param {string} screenshotDataUrl 元素截图 data URL。
 * @returns {string} HTML 文本。
 */
function buildReportHtml(payload, screenshotDataUrl) {
  const json = JSON.stringify({ ...payload, screenshot: screenshotDataUrl }, null, 2);
  const selector = payload.element?.selector || "";
  const pageTitle = payload.page?.title || "Untitled";
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8">
    <title>Typesass element report</title>
    <style>
      body { margin: 0; padding: 24px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #172033; background: #f7f9fc; }
      main { max-width: 1080px; margin: 0 auto; }
      h1 { margin: 0 0 8px; font-size: 24px; }
      p { margin: 0 0 18px; color: #5f6f89; }
      section { margin-top: 18px; padding: 18px; border: 1px solid #d8e1ef; border-radius: 8px; background: #fff; }
      h2 { margin: 0 0 12px; font-size: 16px; }
      img { display: block; max-width: 100%; border: 1px solid #d8e1ef; border-radius: 6px; background: #fff; }
      code { display: block; overflow: auto; padding: 12px; border-radius: 6px; color: #0f172a; background: #eef3fb; white-space: pre-wrap; word-break: break-all; }
      pre { overflow: auto; max-height: 520px; margin: 0; padding: 12px; border-radius: 6px; color: #dbeafe; background: #111827; }
    </style>
  </head>
  <body>
    <main>
      <h1>Typesass element report</h1>
      <p>${escapeReportHtml(String(pageTitle))}</p>
      <section>
        <h2>Selector</h2>
        <code>${escapeReportHtml(String(selector))}</code>
      </section>
      <section>
        <h2>Element Screenshot</h2>
        <img src="${screenshotDataUrl}" alt="Selected element screenshot">
      </section>
      <section>
        <h2>Data</h2>
        <pre>${escapeReportHtml(json)}</pre>
      </section>
    </main>
  </body>
</html>`;
}

/**
 * 生成多点 Browser comments 报告，保留可复制的 Codex 风格消息文本和每条评论截图。
 * @param {{markdown:string, comments:Array<Record<string, unknown>>}} payload 多点标注数据。
 * @param {Array<{id:number, screenshot:string}>} screenshots 每条评论对应的截图。
 * @returns {string} HTML 文本。
 */
function buildCommentsReportHtml(payload, screenshots) {
  const json = JSON.stringify(payload.comments || [], null, 2);
  const screenshotSections = screenshots.map((item, index) => `
      <section>
        <h2>Saved Marker Screenshot - Comment ${index + 1}</h2>
        <img src="${item.screenshot}" alt="Saved marker screenshot for Comment ${index + 1}">
      </section>`).join("");
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8">
    <title>Typesass browser comments report</title>
    <style>
      body { margin: 0; padding: 24px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #f4f4f5; background: #09090b; }
      main { max-width: 1080px; margin: 0 auto; }
      h1 { margin: 0 0 8px; font-size: 24px; }
      p { margin: 0 0 18px; color: #a1a1aa; }
      section { margin-top: 18px; padding: 18px; border: 1px solid #27272a; border-radius: 8px; background: #18181b; }
      h2 { margin: 0 0 12px; font-size: 16px; }
      img { display: block; max-width: 100%; border: 1px solid #3f3f46; border-radius: 6px; background: #09090b; }
      pre { overflow: auto; max-height: 520px; margin: 0; padding: 12px; border-radius: 6px; color: #e4e4e7; background: #09090b; white-space: pre-wrap; }
    </style>
  </head>
  <body>
    <main>
      <h1>Typesass browser comments report</h1>
      <p>统一发送导出的多点元素选择消息。</p>
      <section>
        <h2>Browser Comments</h2>
        <pre>${escapeReportHtml(payload.markdown || "")}</pre>
      </section>
${screenshotSections}
      <section>
        <h2>Data</h2>
        <pre>${escapeReportHtml(json)}</pre>
      </section>
    </main>
  </body>
</html>`;
}

/**
 * 转义报告里的文本内容，避免页面数据破坏报告结构。
 * @param {string} value 原始文本。
 * @returns {string} 转义后的文本。
 */
function escapeReportHtml(value) {
  return value.replace(/[&<>"']/g, (char) => {
    const map = { "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" };
    return map[char] || char;
  });
}

/**
 * 捕获当前标签页并生成元素报告。
 * @param {chrome.runtime.MessageSender} sender 消息发送来源。
 * @param {Record<string, unknown>} payload 元素信息。
 * @returns {Promise<void>} 无返回值。
 */
async function createElementReport(sender, payload) {
  const tabId = sender.tab?.id;
  const windowId = sender.tab?.windowId;
  if (!tabId || windowId === undefined) {
    throw new Error("无法定位当前标签页");
  }

  const screenshot = await chrome.tabs.captureVisibleTab(windowId, { format: "png" });
  const rect = payload.element?.rect;
  const devicePixelRatio = payload.page?.viewport?.devicePixelRatio || 1;
  const elementScreenshot = rect
    ? await cropElementScreenshot(screenshot, rect, devicePixelRatio)
    : screenshot;
  const html = buildReportHtml(payload, elementScreenshot);
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  await chrome.tabs.sendMessage(tabId, {
    type: "CODEXMAN_DOWNLOAD_REPORT",
    html,
    filename: `typesass-extension-element-report-${timestamp}.html`
  });
}

/**
 * 捕获当前标签页并生成多点 Browser comments 报告。
 * @param {chrome.runtime.MessageSender} sender 消息发送来源。
 * @param {{markdown:string, comments:Array<Record<string, unknown>>}} payload 多点标注数据。
 * @returns {Promise<void>} 无返回值。
 */
async function createCommentsReport(sender, payload) {
  const tabId = sender.tab?.id;
  const windowId = sender.tab?.windowId;
  if (!tabId || windowId === undefined) {
    throw new Error("无法定位当前标签页");
  }

  const screenshots = [];
  try {
    for (const comment of payload.comments || []) {
      await chrome.tabs.sendMessage(tabId, {
        type: "CODEXMAN_PREPARE_COMMENT_SCREENSHOT",
        annotationId: comment.id
      });
      await waitForOverlayPaint();
      screenshots.push({
        id: comment.id,
        screenshot: await chrome.tabs.captureVisibleTab(windowId, { format: "png" })
      });
      await waitForCaptureQuota();
    }
  } finally {
    await chrome.tabs.sendMessage(tabId, { type: "CODEXMAN_RESTORE_ANNOTATIONS" }).catch(() => undefined);
  }
  const task = await createTypesassTask(payload);
  const html = buildCommentsReportHtml(payload, screenshots);
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  await chrome.tabs.sendMessage(tabId, {
    type: "TYPESASS_TASK_CREATED",
    payload: task
  });
  await chrome.tabs.sendMessage(tabId, {
    type: "CODEXMAN_DOWNLOAD_REPORT",
    html,
    filename: `typesass-extension-browser-comments-${timestamp}.html`
  });
}

/**
 * 等待页面完成一次绘制，确保截图前浮层已经切换到对应评论状态。
 * @returns {Promise<void>} 无返回值。
 */
function waitForOverlayPaint() {
  return new Promise((resolve) => {
    setTimeout(resolve, 80);
  });
}

/**
 * 避免 Chrome 对 captureVisibleTab 的短时间调用频率限制。
 * @returns {Promise<void>} 无返回值。
 */
function waitForCaptureQuota() {
  return new Promise((resolve) => {
    setTimeout(resolve, 650);
  });
}

/**
 * 确保目标标签页已经注入元素选择脚本。
 * @param {number} tabId 当前标签页 id。
 * @returns {Promise<void>} 无返回值。
 */
async function ensurePickerInjected(tabId) {
  try {
    await chrome.tabs.sendMessage(tabId, { type: "CODEXMAN_PING" });
  } catch (_error) {
    await chrome.scripting.executeScript({
      target: { tabId },
      files: ["contentScript.js"]
    });
  }
}

/**
 * 从右键菜单启动当前页面的元素选择模式。
 * @param {number} tabId 当前标签页 id。
 * @returns {Promise<void>} 无返回值。
 */
async function startPickerFromContextMenu(tabId) {
  await ensurePickerInjected(tabId);
  await chrome.tabs.sendMessage(tabId, { type: "CODEXMAN_START_PICKER" });
}

chrome.runtime.onInstalled.addListener(() => {
  registerContextMenu();
});

chrome.runtime.onStartup.addListener(() => {
  registerContextMenu();
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId !== CODEXMAN_CONTEXT_MENU_ID || !tab?.id) {
    return;
  }
  void startPickerFromContextMenu(tab.id).catch(async (error) => {
    await chrome.tabs.sendMessage(tab.id, {
      type: "CODEXMAN_REPORT_FAILED",
      reason: error instanceof Error ? error.message : "当前页面无法启动选择模式"
    }).catch(() => undefined);
  });
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message?.type === "CODEXMAN_ELEMENT_CONFIRMED") {
    void createElementReport(sender, message.payload).catch(async (error) => {
      if (sender.tab?.id) {
        await chrome.tabs.sendMessage(sender.tab.id, {
          type: "CODEXMAN_REPORT_FAILED",
          reason: error instanceof Error ? error.message : "报告生成失败"
        });
      }
    });
    return true;
  }

  if (message?.type === "CODEXMAN_COMMENTS_CONFIRMED") {
    void createCommentsReport(sender, message.payload).catch(async (error) => {
      if (sender.tab?.id) {
        await chrome.tabs.sendMessage(sender.tab.id, {
          type: "CODEXMAN_REPORT_FAILED",
          reason: error instanceof Error ? error.message : "报告生成失败"
        });
      }
    });
    return true;
  }

  if (message?.type === "TYPESASS_GET_SETTINGS") {
    void getTypesassSettings().then(sendResponse).catch((error) => {
      sendResponse({ error: error instanceof Error ? error.message : "读取插件设置失败" });
    });
    return true;
  }

  if (message?.type === "TYPESASS_SAVE_SETTINGS") {
    void saveTypesassSettings(message.payload || {}).then(() => sendResponse({ ok: true })).catch((error) => {
      sendResponse({ error: error instanceof Error ? error.message : "保存插件设置失败" });
    });
    return true;
  }

  if (message?.type === "TYPESASS_CHECK_HEALTH") {
    void checkTypesassHealth().then(sendResponse).catch((error) => {
      sendResponse({ error: error instanceof Error ? error.message : "检测 App 服务失败" });
    });
    return true;
  }

  if (message?.type === "TYPESASS_REQUEST_ACCESS_TOKEN") {
    void requestTypesassAccessToken().then(sendResponse).catch((error) => {
      sendResponse({ error: error instanceof Error ? error.message : "请求授权码失败" });
    });
    return true;
  }

  if (message?.type === "TYPESASS_LIST_PROJECTS") {
    void getTypesassSettings().then((settings) => listTypesassProjects(settings.accessToken)).then(sendResponse).catch((error) => {
      sendResponse({ error: error instanceof Error ? error.message : "读取项目失败" });
    });
    return true;
  }

  return false;
});
