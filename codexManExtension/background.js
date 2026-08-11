const CODEXMAN_CONTEXT_MENU_ID = "codexman-start-picker";

/**
 * 注册浏览器右键菜单入口，便于在页面内直接启用选择器。
 * @returns {void}
 */
function registerContextMenu() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: CODEXMAN_CONTEXT_MENU_ID,
      title: "启用 CodexMan 选择器",
      contexts: ["page", "selection", "link", "image", "video", "audio", "editable"]
    });
  });
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
    <title>CodexMan element report</title>
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
      <h1>CodexMan element report</h1>
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
    <title>CodexMan browser comments report</title>
    <style>
      body { margin: 0; padding: 24px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #172033; background: #f7f9fc; }
      main { max-width: 1080px; margin: 0 auto; }
      h1 { margin: 0 0 8px; font-size: 24px; }
      p { margin: 0 0 18px; color: #5f6f89; }
      section { margin-top: 18px; padding: 18px; border: 1px solid #d8e1ef; border-radius: 8px; background: #fff; }
      h2 { margin: 0 0 12px; font-size: 16px; }
      img { display: block; max-width: 100%; border: 1px solid #d8e1ef; border-radius: 6px; background: #fff; }
      pre { overflow: auto; max-height: 520px; margin: 0; padding: 12px; border-radius: 6px; color: #dbeafe; background: #111827; white-space: pre-wrap; }
    </style>
  </head>
  <body>
    <main>
      <h1>CodexMan browser comments report</h1>
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
    filename: `codexManExtension-element-report-${timestamp}.html`
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
  const html = buildCommentsReportHtml(payload, screenshots);
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  await chrome.tabs.sendMessage(tabId, {
    type: "CODEXMAN_DOWNLOAD_REPORT",
    html,
    filename: `codexManExtension-browser-comments-${timestamp}.html`
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

chrome.runtime.onMessage.addListener((message, sender) => {
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

  return false;
});
