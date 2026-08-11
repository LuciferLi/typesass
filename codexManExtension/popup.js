const startPickerButton = document.getElementById("startPickerButton");
const requestTokenButton = document.getElementById("requestTokenButton");
const refreshProjectsButton = document.getElementById("refreshProjectsButton");
const healthButton = document.getElementById("healthButton");
const accessTokenInput = document.getElementById("accessTokenInput");
const projectSelect = document.getElementById("projectSelect");
const messageText = document.getElementById("messageText");
const healthText = document.getElementById("healthText");
const statusBadge = document.getElementById("statusBadge");

/**
 * 向后台脚本发送消息并等待响应。
 * @param {string} type 消息类型。
 * @param {unknown=} payload 消息载荷。
 * @returns {Promise<unknown>} 后台响应。
 */
function sendBackgroundMessage(type, payload) {
  return chrome.runtime.sendMessage({ type, payload }).then((response) => {
    if (response?.error) {
      throw new Error(response.error);
    }
    return response;
  });
}

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
 * 按当前状态更新插件顶部角标。
 * @param {string} label 角标文案。
 * @returns {void}
 */
function setStatus(label) {
  statusBadge.textContent = label;
}

/**
 * 加载已保存的插件设置。
 * @returns {Promise<void>} 无返回值。
 */
async function loadSettings() {
  const settings = await sendBackgroundMessage("TYPESASS_GET_SETTINGS");
  accessTokenInput.value = settings?.accessToken || "";
  if (settings?.projectId) {
    projectSelect.innerHTML = `<option value="${settings.projectId}">已保存项目：${settings.projectId}</option>`;
    projectSelect.value = settings.projectId;
  }
}

/**
 * 检查 App 健康状态，提示用户是否需要先打开 App。
 * @returns {Promise<void>} 无返回值。
 */
async function refreshHealth() {
  healthButton.disabled = true;
  try {
    await sendBackgroundMessage("TYPESASS_CHECK_HEALTH");
    healthText.textContent = "Typesass App 已打开，可以请求授权码并创建任务。";
    setStatus("已连接");
  } catch (error) {
    healthText.textContent = error instanceof Error ? error.message : "请先打开 Typesass App。";
    setStatus("未连接");
  } finally {
    healthButton.disabled = false;
  }
}

/**
 * 请求 App 授权码并写入输入框。
 * @returns {Promise<void>} 无返回值。
 */
async function requestAccessToken() {
  requestTokenButton.disabled = true;
  try {
    const response = await sendBackgroundMessage("TYPESASS_REQUEST_ACCESS_TOKEN");
    accessTokenInput.value = response?.accessToken || "";
    messageText.textContent = "授权码已保存，可以读取项目并开始选择元素。";
    setStatus("已授权");
    await refreshProjects();
  } catch (error) {
    messageText.textContent = error instanceof Error ? error.message : "授权码申请失败。";
  } finally {
    requestTokenButton.disabled = false;
  }
}

/**
 * 读取任务项目列表并保存当前选择。
 * @returns {Promise<void>} 无返回值。
 */
async function refreshProjects() {
  refreshProjectsButton.disabled = true;
  try {
    await sendBackgroundMessage("TYPESASS_SAVE_SETTINGS", {
      accessToken: accessTokenInput.value,
      projectId: projectSelect.value
    });
    const projects = await sendBackgroundMessage("TYPESASS_LIST_PROJECTS");
    projectSelect.innerHTML = "";
    if (!projects.length) {
      projectSelect.innerHTML = "<option value=\"\">请先在 App 创建项目</option>";
      return;
    }
    projects.forEach((project) => {
      const option = document.createElement("option");
      option.value = project.id;
      option.textContent = `${project.name} · ${project.workspacePath}`;
      projectSelect.appendChild(option);
    });
    await saveSettings();
    messageText.textContent = "项目已读取，发送标注时会在该项目中创建任务。";
  } catch (error) {
    messageText.textContent = error instanceof Error ? error.message : "读取项目失败。";
  } finally {
    refreshProjectsButton.disabled = false;
  }
}

/**
 * 保存当前输入的授权码和项目 ID。
 * @returns {Promise<void>} 无返回值。
 */
async function saveSettings() {
  await sendBackgroundMessage("TYPESASS_SAVE_SETTINGS", {
    accessToken: accessTokenInput.value,
    projectId: projectSelect.value
  });
}

/**
 * 启动当前页面的元素选择模式；受限页面无法注入时给出明确提示。
 * @returns {Promise<void>} 无返回值。
 */
async function startElementPicker() {
  startPickerButton.disabled = true;
  setStatus("选择中");
  messageText.textContent = "请在页面里选择元素，填写描述后点击发送全部。";

  try {
    await saveSettings();
    const tab = await getActiveTab();
    await ensurePickerInjected(tab.id);
    await chrome.tabs.sendMessage(tab.id, { type: "CODEXMAN_START_PICKER" });
    window.close();
  } catch (error) {
    setStatus("未启动");
    messageText.textContent = error instanceof Error ? error.message : "当前页面无法启动选择模式";
    startPickerButton.disabled = false;
  }
}

startPickerButton.addEventListener("click", () => {
  void startElementPicker();
});
requestTokenButton.addEventListener("click", () => {
  void requestAccessToken();
});
refreshProjectsButton.addEventListener("click", () => {
  void refreshProjects();
});
healthButton.addEventListener("click", () => {
  void refreshHealth();
});
accessTokenInput.addEventListener("change", () => {
  void saveSettings();
});
projectSelect.addEventListener("change", () => {
  void saveSettings();
});

void loadSettings().then(() => refreshHealth());
