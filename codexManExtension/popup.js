const startPickerButton = document.getElementById("startPickerButton");
const messageText = document.getElementById("messageText");
const statusBadge = document.getElementById("statusBadge");

/**
 * 获取当前可操作标签页，用于向页面发送元素选择模式启动消息。
 * @returns {Promise<chrome.tabs.Tab>} 当前激活标签页。
 */
async function getActiveTab() {
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tabs[0] || !tabs[0].id) {
    throw new Error("未找到当前标签页");
  }
  return tabs[0];
}

/**
 * 确保当前标签页已经注入元素选择脚本；页面早于插件加载时需要主动补注入。
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
 * 启动当前页面的元素选择模式；受限页面无法注入时给出明确提示。
 * @returns {Promise<void>} 无返回值。
 */
async function startElementPicker() {
  startPickerButton.disabled = true;
  statusBadge.textContent = "选择中";
  messageText.textContent = "请在页面里选择一个元素。";

  try {
    const tab = await getActiveTab();
    await ensurePickerInjected(tab.id);
    await chrome.tabs.sendMessage(tab.id, { type: "CODEXMAN_START_PICKER" });
    window.close();
  } catch (error) {
    statusBadge.textContent = "未启动";
    messageText.textContent = error instanceof Error ? error.message : "当前页面无法启动选择模式";
    startPickerButton.disabled = false;
  }
}

startPickerButton.addEventListener("click", () => {
  void startElementPicker();
});
