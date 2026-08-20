const requestTableBody = document.getElementById('requestTableBody');
const requestCountText = document.getElementById('requestCountText');
const filterInput = document.getElementById('filterInput');
const typeFilterSelect = document.getElementById('typeFilterSelect');
const clearButton = document.getElementById('clearButton');
const emptyState = document.getElementById('emptyState');
const messageText = document.getElementById('messageText');
const detailEmptyState = document.getElementById('detailEmptyState');
const detailContent = document.getElementById('detailContent');
const detailFooter = document.getElementById('detailFooter');
const detailTitle = document.getElementById('detailTitle');
const detailSubtitle = document.getElementById('detailSubtitle');
const projectSelect = document.getElementById('projectSelect');
const copyCurlButton = document.getElementById('copyCurlButton');
const detailCreateFixButton = document.getElementById('detailCreateFixButton');
const problemDescriptionInput = document.getElementById('problemDescriptionInput');
const fixTaskDialog = document.getElementById('fixTaskDialog');
const fixTaskDialogSubtitle = document.getElementById('fixTaskDialogSubtitle');
const closeFixTaskDialogButton = document.getElementById('closeFixTaskDialogButton');
const cancelFixTaskButton = document.getElementById('cancelFixTaskButton');
const confirmFixTaskButton = document.getElementById('confirmFixTaskButton');
const detailMethod = document.getElementById('detailMethod');
const detailStatus = document.getElementById('detailStatus');
const detailType = document.getElementById('detailType');
const detailTime = document.getElementById('detailTime');
const detailUrl = document.getElementById('detailUrl');
const detailRequestHeaders = document.getElementById('detailRequestHeaders');
const detailRequestBody = document.getElementById('detailRequestBody');
const detailResponseHeaders = document.getElementById('detailResponseHeaders');
const detailResponseBody = document.getElementById('detailResponseBody');
const REQUEST_BODY_MAX_LENGTH = 12000;
const RESPONSE_BODY_MAX_LENGTH = 18000;
const TASK_PROMPT_MAX_LENGTH = 48000;
const capturedRequests = [];
let selectedRequestKey = '';
let projectsLoaded = false;
let pendingFixRequestKey = '';

/**
 * 转义 Shell 单引号参数，避免请求内容破坏 cURL 命令结构。
 * 流程：用 POSIX 常见的 '\'' 片段拼接带单引号字符串。
 * 参数：value 为待写入命令的原始文本。
 * 返回：可放入单引号中的安全片段。
 * 异常/边界：空字符串保留为空参数，二进制正文由调用方提前截断或省略。
 */
function quoteShellValue(value) {
  return `'${String(value).replace(/'/g, "'\\''")}'`;
}

/**
 * 截断过长文本，避免任务 prompt 超出本地任务接口限制。
 * 流程：长度在限制内原样返回，超出时保留前段内容并追加截断说明。
 * 参数：value 为原始文本，maxLength 为最大字符数，label 为截断说明中的字段名。
 * 返回：截断后的文本。
 * 异常/边界：非字符串会先转成字符串，null 与 undefined 转为空字符串。
 */
function truncateText(value, maxLength, label) {
  const text = value === null || value === undefined ? '' : String(value);
  if (text.length <= maxLength) {
    return text;
  }
  return `${text.slice(0, maxLength)}\n\n[${label} 已截断，原始长度 ${text.length} 字符]`;
}

/**
 * 读取请求头字段值。
 * 流程：按 header name 忽略大小写匹配 DevTools HAR 头数组。
 * 参数：headers 为 HAR 头数组，name 为字段名。
 * 返回：命中的字段值；不存在时返回空字符串。
 * 异常/边界：headers 不是数组时按空数组处理。
 */
function getHeaderValue(headers, name) {
  const headerList = Array.isArray(headers) ? headers : [];
  const matched = headerList.find((header) => String(header.name || '').toLowerCase() === name.toLowerCase());
  return matched ? String(matched.value || '') : '';
}

/**
 * 判断请求头是否适合写入 cURL。
 * 流程：过滤 Chrome 自动计算或容易导致误导的 hop-by-hop 头，保留鉴权与业务头。
 * 参数：name 为请求头名称。
 * 返回：适合写入 cURL 时返回 true。
 * 异常/边界：空名称直接过滤。
 */
function shouldIncludeCurlHeader(name) {
  const lowerName = String(name || '').toLowerCase();
  return Boolean(lowerName) && ![
    'content-length',
    'host',
    'connection',
    'cookie2'
  ].includes(lowerName);
}

/**
 * 将 DevTools 请求对象转换成可复现的 cURL 命令。
 * 流程：写入 method、URL、请求头和请求体；GET/HEAD 不额外写 body。
 * 参数：entry 为面板缓存的请求记录。
 * 返回：多行 cURL 命令。
 * 异常/边界：请求体过长会截断，multipart 或二进制仍按 DevTools 暴露文本写入。
 */
function buildCurlCommand(entry) {
  const request = entry.raw.request;
  const parts = [`curl ${quoteShellValue(request.url)}`, `  -X ${quoteShellValue(request.method || 'GET')}`];
  (request.headers || []).filter((header) => shouldIncludeCurlHeader(header.name)).forEach((header) => {
    parts.push(`  -H ${quoteShellValue(`${header.name}: ${header.value}`)}`);
  });
  const postText = request.postData?.text;
  if (postText && !['GET', 'HEAD'].includes(String(request.method || '').toUpperCase())) {
    parts.push(`  --data-raw ${quoteShellValue(truncateText(postText, REQUEST_BODY_MAX_LENGTH, '请求体'))}`);
  }
  return parts.join(' \\\n');
}

/**
 * 格式化响应头，便于任务中直接查看服务端返回上下文。
 * 流程：按 HAR 头数组拼成 name: value 多行文本。
 * 参数：headers 为响应头数组。
 * 返回：响应头文本。
 * 异常/边界：没有响应头时返回空字符串。
 */
function formatHeaders(headers) {
  return (Array.isArray(headers) ? headers : [])
    .map((header) => `${header.name}: ${header.value}`)
    .join('\n');
}

/**
 * 获取请求记录的规范化资源类型。
 * 流程：优先使用 DevTools 提供的 `_resourceType`，再退回响应 MIME 类型。
 * 参数：rawEntry 为 DevTools HAR entry。
 * 返回：用于展示和筛选的类型文本。
 * 异常/边界：缺少类型时返回 `other`，避免筛选控件出现空值。
 */
function getResourceType(rawEntry) {
  return String(rawEntry?._resourceType || rawEntry?.response?.content?.mimeType || 'other') || 'other';
}

/**
 * 读取请求路径作为任务标题片段。
 * 流程：优先解析 URL pathname，失败时退回原始 URL。
 * 参数：url 为请求地址。
 * 返回：适合标题展示的路径。
 * 异常/边界：非标准 URL 不抛出，直接返回截断后的原值。
 */
function getRequestPath(url) {
  try {
    const parsedUrl = new URL(url);
    return `${parsedUrl.pathname}${parsedUrl.search}`.slice(0, 120);
  } catch {
    return String(url || '').slice(0, 120);
  }
}

/**
 * 读取当前被调试页面地址。
 * 流程：通过 DevTools inspectedWindow 在页面上下文读取 location.href。
 * 参数：无。
 * 返回：当前页面地址，读取失败时返回“未知”。
 * 异常/边界：页面处于特殊地址或 DevTools eval 失败时不阻断创建任务。
 */
function getInspectedPageUrl() {
  return new Promise((resolve) => {
    chrome.devtools.inspectedWindow.eval('location.href', (result, exceptionInfo) => {
      if (exceptionInfo || !result) {
        resolve('未知');
        return;
      }
      resolve(String(result));
    });
  });
}

/**
 * 读取 URL 的域名，用于生成贴近用户描述的默认修复提示词。
 * 流程：优先解析页面 URL 的 host，失败时解析请求 URL。
 * 参数：pageUrl 为当前页面地址，requestUrl 为请求地址。
 * 返回：域名或原始地址片段。
 * 异常/边界：特殊页面或非标准 URL 不抛出，返回“当前”兜底。
 */
function getPageHost(pageUrl, requestUrl) {
  for (const url of [pageUrl, requestUrl]) {
    try {
      const parsedUrl = new URL(url);
      if (parsedUrl.host) {
        return parsedUrl.host;
      }
    } catch {
      if (url && url !== '未知') {
        return String(url).slice(0, 80);
      }
    }
  }
  return '当前';
}

/**
 * 生成创建 fix 任务默认发送内容。
 * 流程：使用用户要求的自然语言模板，附加可复现 cURL、响应头和响应结果。
 * 参数：entry 为请求记录，responseBody 为 DevTools 读取到的响应正文，responseEncoding 为响应编码，pageUrl 为页面地址。
 * 返回：可提交给 codexMan 的任务正文。
 * 异常/边界：响应体缺失时写明无法读取，不阻断任务创建。
 */
function buildFixTaskPrompt(entry, responseBody, responseEncoding, pageUrl) {
  const request = entry.raw.request;
  const response = entry.raw.response;
  const contentType = getHeaderValue(response.headers, 'content-type') || response.content?.mimeType || '';
  const pageHost = getPageHost(pageUrl, request.url);
  const responseText = responseBody
    ? truncateText(responseBody, RESPONSE_BODY_MAX_LENGTH, '响应体')
    : '[DevTools 未返回响应体，可能是缓存、跨域、重定向、预检或二进制内容]';
  const responseTitle = responseEncoding ? `响应如下（encoding: ${responseEncoding}）` : '响应如下';
  const prompt = `这是 ${pageHost} 域名网页中请求的 HTTP 请求。

页面地址：${pageUrl || '未知'}
请求地址：${request.url}
请求方法：${request.method}
响应状态：${response.status} ${response.statusText || ''}
请求类型：${entry.resourceType || 'unknown'}
响应类型：${contentType || 'unknown'}
耗时：${Math.round(entry.raw.time || 0)} ms

curl 如下

\`\`\`bash
${buildCurlCommand(entry)}
\`\`\`

响应头如下

\`\`\`http
${formatHeaders(response.headers) || '[无响应头]'}
\`\`\`

${responseTitle}

\`\`\`text
${responseText}
\`\`\`

帮我解决这个问题。
`;
  return truncateText(prompt, TASK_PROMPT_MAX_LENGTH, '任务内容');
}

/**
 * 向后台脚本发送消息并转换错误响应。
 * 流程：统一处理 runtime message 的 error 字段。
 * 参数：type 为消息类型，payload 为消息载荷。
 * 返回：后台响应。
 * 异常/边界：后台返回 error 时抛出 Error，便于按钮状态恢复。
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
 * 读取插件已保存的授权和任务项目，并填充 DevTools 项目下拉。
 * 流程：先读取保存项，再通过后台复用项目列表接口，最后恢复保存的项目选择。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：缺少授权码或服务不可用时仅展示提示，不阻断查看请求详情。
 */
async function loadProjectOptions() {
  if (projectsLoaded) {
    return;
  }
  projectSelect.disabled = true;
  projectSelect.innerHTML = '<option value="">正在读取项目...</option>';
  try {
    const settings = await sendBackgroundMessage('TYPESASS_GET_SETTINGS');
    const projects = await sendBackgroundMessage('TYPESASS_LIST_PROJECTS');
    projectSelect.innerHTML = '<option value="">选择项目后发送</option>';
    projects.forEach((project) => {
      const option = document.createElement('option');
      option.value = project.id;
      option.textContent = `${project.name} · ${project.workspacePath}`;
      projectSelect.appendChild(option);
    });
    if (settings?.projectId && projects.some((project) => project.id === settings.projectId)) {
      projectSelect.value = settings.projectId;
    }
    projectsLoaded = true;
    messageText.textContent = projects.length
      ? '项目已读取，选择请求后可以复制 cURL 或创建 fix 任务。'
      : 'codexMan 中还没有任务项目，请先在任务管理页面创建项目。';
  } catch (error) {
    projectSelect.innerHTML = '<option value="">项目读取失败</option>';
    messageText.textContent = error instanceof Error ? error.message : '项目读取失败，请先确认插件授权码可用。';
  } finally {
    projectSelect.disabled = false;
  }
}

/**
 * 确保创建任务弹窗所需项目列表已经可用。
 * 流程：复用项目读取逻辑，并检查下拉中是否存在可发送的项目。
 * 参数：无。
 * 返回：存在可选项目时返回 true。
 * 异常/边界：读取失败或项目为空时返回 false，不阻断查看请求详情。
 */
async function ensureProjectOptionsLoaded() {
  await loadProjectOptions();
  return Array.from(projectSelect.options).some((option) => option.value);
}

/**
 * 保存 DevTools 面板当前选择的任务项目。
 * 流程：复用插件设置存储，仅更新 projectId，保留已有授权码。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：保存失败只提示，不清空当前选择。
 */
async function saveSelectedProject() {
  try {
    const settings = await sendBackgroundMessage('TYPESASS_GET_SETTINGS');
    await sendBackgroundMessage('TYPESASS_SAVE_SETTINGS', {
      accessToken: settings?.accessToken || '',
      projectId: projectSelect.value
    });
  } catch (error) {
    messageText.textContent = error instanceof Error ? error.message : '保存项目选择失败。';
  }
}

/**
 * 将 DevTools HAR 条目加入请求列表缓存。
 * 流程：提取展示字段并按 URL、method、startedDateTime 去重。
 * 参数：rawEntry 为 DevTools HAR entry。
 * 返回：无返回值。
 * 异常/边界：缺少 request 或 response 的异常条目直接忽略。
 */
function addRequest(rawEntry) {
  if (!rawEntry?.request || !rawEntry?.response) {
    return;
  }
  const key = [
    rawEntry.startedDateTime || '',
    rawEntry.request.method || '',
    rawEntry.request.url || ''
  ].join('|');
  if (capturedRequests.some((item) => item.key === key)) {
    return;
  }
  capturedRequests.unshift({
    key,
    raw: rawEntry,
    pageUrl: rawEntry.pageref || '',
    resourceType: getResourceType(rawEntry),
    responseBody: null,
    responseEncoding: ''
  });
  if (capturedRequests.length > 300) {
    capturedRequests.pop();
  }
  renderTypeFilterOptions();
  renderRequestTable();
}

/**
 * 刷新类型筛选下拉选项。
 * 流程：从当前请求缓存中提取类型，保持用户已有选择。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：当前选择的类型已不存在时自动回到全部类型。
 */
function renderTypeFilterOptions() {
  const selectedType = typeFilterSelect.value;
  const types = [...new Set(capturedRequests.map((entry) => entry.resourceType || 'other'))].sort();
  typeFilterSelect.innerHTML = '<option value="">全部类型</option>';
  types.forEach((type) => {
    const option = document.createElement('option');
    option.value = type;
    option.textContent = type;
    typeFilterSelect.appendChild(option);
  });
  typeFilterSelect.value = types.includes(selectedType) ? selectedType : '';
}

/**
 * 根据筛选词返回当前可见请求。
 * 流程：把 method、状态码、URL 和资源类型合并成搜索文本。
 * 参数：无。
 * 返回：过滤后的请求记录。
 * 异常/边界：筛选词为空时返回全部缓存记录。
 */
function getVisibleRequests() {
  const keyword = filterInput.value.trim().toLowerCase();
  const selectedType = typeFilterSelect.value;
  return capturedRequests.filter((entry) => {
    if (selectedType && entry.resourceType !== selectedType) {
      return false;
    }
    if (!keyword) {
      return true;
    }
    const text = [
      entry.raw.request.method,
      entry.raw.response.status,
      entry.raw.request.url,
      entry.resourceType
    ].join(' ').toLowerCase();
    return text.includes(keyword);
  });
}

/**
 * 按缓存 key 查找请求记录。
 * 流程：在当前请求数组中查找相同 key 的条目。
 * 参数：key 为请求缓存 key。
 * 返回：命中的请求记录；不存在时返回 undefined。
 * 异常/边界：空 key 直接返回 undefined。
 */
function findRequestByKey(key) {
  return capturedRequests.find((entry) => entry.key === key);
}

/**
 * 渲染请求列表表格。
 * 流程：按当前筛选结果重建 tbody，并绑定每行创建任务按钮。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：空列表时展示空态，不创建多余行。
 */
function renderRequestTable() {
  const visibleRequests = getVisibleRequests();
  requestCountText.textContent = `${capturedRequests.length} 个请求`;
  emptyState.classList.toggle('network-panel__empty--visible', visibleRequests.length === 0);
  requestTableBody.innerHTML = '';
  visibleRequests.forEach((entry) => {
    const row = document.createElement('tr');
    const status = Number(entry.raw.response.status || 0);
    const duration = Math.round(entry.raw.time || 0);
    row.className = entry.key === selectedRequestKey ? 'network-panel__row--selected' : '';
    row.innerHTML = `
      <td class="network-panel__method"></td>
      <td class="${status >= 400 ? 'network-panel__status--error' : 'network-panel__status--success'}"></td>
      <td class="network-panel__url"></td>
      <td class="network-panel__muted"></td>
      <td class="network-panel__muted"></td>
      <td></td>
    `;
    row.children[0].textContent = entry.raw.request.method || 'GET';
    row.children[1].textContent = status ? String(status) : '-';
    row.children[2].textContent = entry.raw.request.url || '';
    row.children[2].title = entry.raw.request.url || '';
    row.children[3].textContent = entry.resourceType || '-';
    row.children[4].textContent = duration ? `${duration} ms` : '-';
    row.addEventListener('click', () => {
      void selectRequest(entry.key);
    });
    const button = document.createElement('button');
    button.className = 'network-panel__action';
    button.type = 'button';
    button.textContent = 'Fix';
    button.title = '创建 fix 任务';
    button.addEventListener('click', (event) => {
      event.stopPropagation();
      void openFixTaskDialog(entry);
    });
    row.children[5].appendChild(button);
    requestTableBody.appendChild(row);
  });
}

/**
 * 读取并缓存响应正文。
 * 流程：优先使用请求记录上的缓存，未读取时调用 DevTools getContent。
 * 参数：entry 为请求记录。
 * 返回：响应正文和编码信息。
 * 异常/边界：读取不到内容时返回空字符串，不抛错中断详情展示。
 */
function getResponseContent(entry) {
  if (entry.responseBody !== null) {
    return Promise.resolve({
      body: entry.responseBody,
      encoding: entry.responseEncoding
    });
  }
  return new Promise((resolve) => {
    entry.raw.getContent((body, encoding) => {
      entry.responseBody = body || '';
      entry.responseEncoding = encoding || '';
      resolve({
        body: entry.responseBody,
        encoding: entry.responseEncoding
      });
    });
  });
}

/**
 * 选中一条请求并刷新右侧详情。
 * 流程：记录选中 key、重绘表格选中态、填充请求信息并异步读取响应体。
 * 参数：key 为请求缓存 key。
 * 返回：无返回值。
 * 异常/边界：请求已被清空时展示空态。
 */
async function selectRequest(key) {
  const entry = findRequestByKey(key);
  selectedRequestKey = entry ? key : '';
  renderRequestTable();
  if (!entry) {
    renderEmptyDetail();
    return;
  }
  renderRequestDetail(entry);
  const content = await getResponseContent(entry);
  if (selectedRequestKey === entry.key) {
    detailResponseBody.textContent = content.body
      ? truncateText(content.body, RESPONSE_BODY_MAX_LENGTH, '响应体')
      : '[DevTools 未返回响应体，可能是缓存、跨域、重定向、预检或二进制内容]';
  }
}

/**
 * 展示右侧详情空态。
 * 流程：隐藏详情内容并恢复空态提示。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：无。
 */
function renderEmptyDetail() {
  detailEmptyState.hidden = false;
  detailContent.hidden = true;
  detailFooter.hidden = true;
  detailCreateFixButton.onclick = null;
  copyCurlButton.onclick = null;
}

/**
 * 渲染右侧请求详情。
 * 流程：同步展示请求基础信息、请求头、请求体和响应头，响应体先展示读取中。
 * 参数：entry 为请求记录。
 * 返回：无返回值。
 * 异常/边界：请求体为空时展示无请求体。
 */
function renderRequestDetail(entry) {
  const request = entry.raw.request;
  const response = entry.raw.response;
  const status = Number(response.status || 0);
  detailEmptyState.hidden = true;
  detailContent.hidden = false;
  detailFooter.hidden = false;
  detailTitle.textContent = getRequestPath(request.url);
  detailSubtitle.textContent = request.url || '';
  detailMethod.textContent = request.method || 'GET';
  detailStatus.textContent = status ? `${status} ${response.statusText || ''}` : '-';
  detailType.textContent = entry.resourceType || 'other';
  detailTime.textContent = entry.raw.time ? `${Math.round(entry.raw.time)} ms` : '-';
  detailUrl.textContent = request.url || '';
  detailRequestHeaders.textContent = formatHeaders(request.headers) || '[无请求头]';
  detailRequestBody.textContent = request.postData?.text
    ? truncateText(request.postData.text, REQUEST_BODY_MAX_LENGTH, '请求体')
    : '[无请求体]';
  detailResponseHeaders.textContent = formatHeaders(response.headers) || '[无响应头]';
  detailResponseBody.textContent = entry.responseBody === null
    ? '正在读取响应结果...'
    : (entry.responseBody || '[DevTools 未返回响应体，可能是缓存、跨域、重定向、预检或二进制内容]');
  detailCreateFixButton.disabled = false;
  detailCreateFixButton.textContent = '创建 fix 任务';
  detailCreateFixButton.onclick = () => {
    void openFixTaskDialog(entry);
  };
  copyCurlButton.disabled = false;
  copyCurlButton.textContent = '复制 cURL';
  copyCurlButton.onclick = () => {
    void copyCurl(entry);
  };
}

/**
 * 复制当前请求的 cURL 到剪贴板。
 * 流程：生成 cURL 后调用 Clipboard API，成功和失败均给出底部提示。
 * 参数：entry 为当前请求记录。
 * 返回：无返回值。
 * 异常/边界：剪贴板权限不可用时提示用户。
 */
async function copyCurl(entry) {
  try {
    await navigator.clipboard.writeText(buildCurlCommand(entry));
    messageText.textContent = 'cURL 已复制。';
    copyCurlButton.textContent = '已复制';
    window.setTimeout(() => {
      copyCurlButton.textContent = '复制 cURL';
    }, 1200);
  } catch (error) {
    messageText.textContent = error instanceof Error ? error.message : '复制 cURL 失败，请确认浏览器允许剪贴板权限。';
  }
}

/**
 * 打开创建 fix 任务弹窗。
 * 流程：选中当前请求、读取项目列表和响应内容，生成默认发送内容后展示弹窗。
 * 参数：entry 为当前请求记录。
 * 返回：无返回值。
 * 异常/边界：项目列表读取失败时仍打开弹窗，用户可看到下拉中的错误提示。
 */
async function openFixTaskDialog(entry) {
  pendingFixRequestKey = entry.key;
  selectedRequestKey = entry.key;
  renderRequestTable();
  renderRequestDetail(entry);
  fixTaskDialogSubtitle.textContent = `${entry.raw.request.method || 'GET'} ${getRequestPath(entry.raw.request.url)}`;
  fixTaskDialog.hidden = false;
  confirmFixTaskButton.disabled = false;
  confirmFixTaskButton.textContent = '创建 fix 任务';
  problemDescriptionInput.value = '正在生成发送内容...';
  const hasProjectOptions = await ensureProjectOptionsLoaded();
  if (!hasProjectOptions) {
    messageText.textContent = '请先在 codexMan 中创建任务项目，并确认插件授权码可用。';
  }
  const content = await getResponseContent(entry);
  const pageUrl = await getInspectedPageUrl();
  problemDescriptionInput.value = buildFixTaskPrompt(entry, content.body, content.encoding, pageUrl);
  window.setTimeout(() => {
    problemDescriptionInput.focus();
    problemDescriptionInput.setSelectionRange(0, 0);
  }, 0);
}

/**
 * 关闭创建 fix 任务弹窗。
 * 流程：隐藏弹窗并清理待发送请求 key。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：不清空发送内容，避免用户误关后丢失编辑。
 */
function closeFixTaskDialog() {
  fixTaskDialog.hidden = true;
  pendingFixRequestKey = '';
}

/**
 * 从弹窗确认创建 codexMan fix 任务。
 * 流程：校验项目选择，并把弹窗中可编辑的发送内容交给后台复用任务接口。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：响应读取失败时仍创建任务，并在任务正文中说明响应体缺失。
 */
async function createFixTaskFromDialog() {
  const entry = findRequestByKey(pendingFixRequestKey);
  if (!entry) {
    messageText.textContent = '请先选择要创建 fix 任务的请求。';
    return;
  }
  const projectId = projectSelect.value;
  if (!projectId) {
    messageText.textContent = '请先在弹窗中选择任务项目。';
    projectSelect.focus();
    return;
  }
  confirmFixTaskButton.disabled = true;
  confirmFixTaskButton.textContent = '创建中';
  messageText.textContent = '正在创建 fix 任务...';
  try {
    const prompt = truncateText(problemDescriptionInput.value, TASK_PROMPT_MAX_LENGTH, '任务内容');
    const title = `fix：排查 ${entry.raw.request.method || 'GET'} ${getRequestPath(entry.raw.request.url)}`.slice(0, 200);
    const response = await sendBackgroundMessage('CODEXMAN_CREATE_FIX_TASK', {
      projectId,
      title,
      markdown: prompt
    });
    messageText.textContent = `任务已创建：${response?.title || response?.createdTaskId || title}`;
    confirmFixTaskButton.textContent = '已创建';
    closeFixTaskDialog();
  } catch (error) {
    messageText.textContent = error instanceof Error ? error.message : '创建 fix 任务失败。';
    confirmFixTaskButton.disabled = false;
    confirmFixTaskButton.textContent = '创建 fix 任务';
  }
}

/**
 * 初始化 DevTools 面板请求采集。
 * 流程：先读取当前 HAR，再监听后续 Network 请求完成事件。
 * 参数：无。
 * 返回：无返回值。
 * 异常/边界：DevTools 未保留历史请求时只采集后续请求。
 */
function initNetworkPanel() {
  chrome.devtools.network.getHAR((harLog) => {
    (harLog?.entries || []).forEach(addRequest);
    renderTypeFilterOptions();
    renderRequestTable();
  });
  chrome.devtools.network.onRequestFinished.addListener(addRequest);
}

filterInput.addEventListener('input', renderRequestTable);
typeFilterSelect.addEventListener('change', renderRequestTable);
projectSelect.addEventListener('change', () => {
  void saveSelectedProject();
});
closeFixTaskDialogButton.addEventListener('click', closeFixTaskDialog);
cancelFixTaskButton.addEventListener('click', closeFixTaskDialog);
confirmFixTaskButton.addEventListener('click', () => {
  void createFixTaskFromDialog();
});
fixTaskDialog.addEventListener('click', (event) => {
  if (event.target === fixTaskDialog) {
    closeFixTaskDialog();
  }
});
clearButton.addEventListener('click', () => {
  capturedRequests.length = 0;
  selectedRequestKey = '';
  renderTypeFilterOptions();
  renderRequestTable();
  renderEmptyDetail();
  closeFixTaskDialog();
  messageText.textContent = '请求列表已清空，刷新页面后会重新收集。';
});

void loadProjectOptions();
initNetworkPanel();
