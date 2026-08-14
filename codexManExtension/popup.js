const startPickerButton = document.getElementById("startPickerButton");
const requestTokenButton = document.getElementById("requestTokenButton");
const refreshProjectsButton = document.getElementById("refreshProjectsButton");
const healthButton = document.getElementById("healthButton");
const toggleTokenButton = document.getElementById("toggleTokenButton");
const accessTokenInput = document.getElementById("accessTokenInput");
const projectSelect = document.getElementById("projectSelect");
const messageText = document.getElementById("messageText");
const healthText = document.getElementById("healthText");
const statusBadge = document.getElementById("statusBadge");
let accessTokenVerified = false;

/**
 * 判断当前是否已经选择了可用于创建任务的项目。
 * @returns {boolean} 已选择项目时返回 true。
 */
function hasSelectedProject() {
  return Boolean(projectSelect.value);
}

/**
 * 根据项目选择状态刷新开始按钮。
 * @returns {void}
 */
function updateStartPickerState() {
  startPickerButton.disabled = !hasSelectedProject();
}

/**
 * 按授权码可用状态刷新授权按钮。
 * @returns {void} 无返回值。
 */
function updateAccessTokenButtonState() {
  requestTokenButton.disabled = accessTokenVerified;
  requestTokenButton.textContent = accessTokenVerified ? "授权码可用" : "获取授权码";
}

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
 * 设置当前授权码校验状态，并同步顶部状态与按钮可点击性。
 * @param {boolean} verified 授权码是否可用。
 * @returns {void} 无返回值。
 */
function setAccessTokenVerified(verified) {
  accessTokenVerified = verified;
  updateAccessTokenButtonState();
  if (verified) {
    setStatus("已授权");
  }
}

/**
 * 切换授权码输入框的明文和密码展示。
 * @returns {void} 无返回值。
 */
function toggleTokenVisibility() {
  const nextVisible = accessTokenInput.type === "password";
  accessTokenInput.type = nextVisible ? "text" : "password";
  toggleTokenButton.setAttribute("aria-label", nextVisible ? "隐藏授权码" : "查看授权码");
  toggleTokenButton.setAttribute("title", nextVisible ? "隐藏授权码" : "查看授权码");
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
  updateStartPickerState();
  setAccessTokenVerified(false);
}

/**
 * 检查应用健康状态，提示用户是否需要先打开 codexMan。
 * @returns {Promise<void>} 无返回值。
 */
async function refreshHealth() {
  healthButton.disabled = true;
  try {
    await sendBackgroundMessage("TYPESASS_CHECK_HEALTH");
    healthText.textContent = "codexMan 已打开，可以请求授权码并创建任务。";
    setStatus(accessTokenVerified ? "已授权" : "已连接");
  } catch (error) {
    healthText.textContent = error instanceof Error ? error.message : "请先打开 codexMan。";
    setStatus("未连接");
  } finally {
    healthButton.disabled = false;
  }
}

/**
 * 请求应用授权码并写入输入框。
 * @returns {Promise<void>} 无返回值。
 */
async function requestAccessToken() {
  if (accessTokenVerified) {
    messageText.textContent = "当前授权码可用，无需重新获取。";
    return;
  }
  requestTokenButton.disabled = true;
  try {
    const response = await sendBackgroundMessage("TYPESASS_REQUEST_ACCESS_TOKEN");
    accessTokenInput.value = response?.accessToken || "";
    messageText.textContent = "授权码已保存，可以读取项目并开始选择元素。";
    setAccessTokenVerified(true);
    await refreshProjects();
  } catch (error) {
    setAccessTokenVerified(false);
    messageText.textContent = error instanceof Error ? error.message : "授权码申请失败。";
  } finally {
    updateAccessTokenButtonState();
  }
}

/**
 * 校验输入框内的授权码是否有效。
 * @returns {Promise<boolean>} 授权码可用时返回 true。
 */
async function verifyAccessToken() {
  await sendBackgroundMessage("TYPESASS_SAVE_SETTINGS", {
    accessToken: accessTokenInput.value,
    projectId: projectSelect.value
  });
  await sendBackgroundMessage("TYPESASS_VERIFY_ACCESS_TOKEN");
  setAccessTokenVerified(true);
  return true;
}

/**
 * 读取任务项目列表并保存当前选择。
 * @returns {Promise<void>} 无返回值。
 */
async function refreshProjects() {
  refreshProjectsButton.disabled = true;
  try {
    const savedProjectId = projectSelect.value;
    if (!accessTokenVerified) {
      await verifyAccessToken();
    }
    const projects = await sendBackgroundMessage("TYPESASS_LIST_PROJECTS");
    projectSelect.innerHTML = "";
    if (!projects.length) {
      projectSelect.innerHTML = "<option value=\"\">请先在 codexMan 创建项目</option>";
      updateStartPickerState();
      return;
    }
    projects.forEach((project) => {
      const option = document.createElement("option");
      option.value = project.id;
      option.textContent = `${project.name} · ${project.workspacePath}`;
      projectSelect.appendChild(option);
    });
    if (savedProjectId && projects.some((project) => project.id === savedProjectId)) {
      projectSelect.value = savedProjectId;
    }
    await saveSettings();
    updateStartPickerState();
    messageText.textContent = "项目已读取，发送标注时会在该项目中创建任务。";
  } catch (error) {
    projectSelect.innerHTML = "<option value=\"\">项目读取失败，请重试</option>";
    updateStartPickerState();
    const errorMessage = error instanceof Error ? error.message : "读取项目失败。";
    messageText.textContent = accessTokenVerified
      ? `授权码可用，但项目读取失败：${errorMessage}`
      : errorMessage;
  } finally {
    refreshProjectsButton.disabled = false;
    updateAccessTokenButtonState();
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
  if (!hasSelectedProject()) {
    messageText.textContent = "请先读取并选择任务项目。";
    updateStartPickerState();
    return;
  }
  startPickerButton.disabled = true;
  setStatus("检测中");
  messageText.textContent = "正在检查 codexMan 连接...";

  try {
    await sendBackgroundMessage("TYPESASS_CHECK_HEALTH");
    setStatus("选择中");
    messageText.textContent = "请在页面里选择元素，填写描述后点击发送全部。";
    await saveSettings();
    const tab = await getActiveTab();
    await ensurePickerInjected(tab.id);
    await chrome.tabs.sendMessage(tab.id, { type: "CODEXMAN_START_PICKER" });
    window.close();
  } catch (error) {
    setStatus("未连接");
    messageText.textContent = error instanceof Error ? error.message : "请先打开 codexMan 后再开始选择元素。";
    updateStartPickerState();
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
toggleTokenButton.addEventListener("click", () => {
  toggleTokenVisibility();
});
accessTokenInput.addEventListener("change", () => {
  setAccessTokenVerified(false);
  void saveSettings();
});
projectSelect.addEventListener("change", () => {
  updateStartPickerState();
  void saveSettings();
});

void loadSettings().then(async () => {
  updateStartPickerState();
  await refreshHealth();
  if (accessTokenInput.value) {
    try {
      await verifyAccessToken();
      messageText.textContent = "授权码可用，可以读取项目。";
      await refreshProjects();
    } catch (error) {
      setAccessTokenVerified(false);
      messageText.textContent = error instanceof Error ? error.message : "授权码校验失败。";
    }
  }
});
