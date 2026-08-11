const CODEXMAN_STATE = {
  active: false,
  highlightedElement: null,
  root: null,
  shadowRoot: null,
  interactionLayer: null,
  markerLayer: null,
  highlightBox: null,
  tooltip: null,
  actionBar: null,
  editor: null,
  toast: null,
  annotations: [],
  selectedAnnotationId: null,
  draftAnnotation: null
};

const CODEXMAN_OVERLAY_STYLE = `
  :host {
    position: fixed;
    inset: 0;
    pointer-events: none;
    z-index: 2147483647;
    font-size: 13px;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    --codexman-blue: #0069fb;
  }

  .interaction-layer {
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    pointer-events: none;
  }

  .hover-box,
  .saved-box {
    position: fixed;
    box-sizing: border-box;
    border: 2px solid var(--codexman-blue);
    background: color-mix(in srgb, var(--codexman-blue) 3%, transparent);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.28);
    pointer-events: none;
  }

  .markers-layer {
    position: fixed;
    inset: 0;
    pointer-events: none;
  }

  .marker {
    position: fixed;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 25px;
    padding: 0;
    border: 0;
    color: #ffffff;
    background: transparent;
    transform: translate(-50%, -50%);
    pointer-events: auto;
    cursor: pointer;
  }

  .marker[data-selected="true"] {
    transform: translate(-50%, -50%) scale(1.08);
  }

  .marker-icon {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }

  .marker-label {
    position: relative;
    z-index: 1;
    color: white;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    pointer-events: none;
  }

  .element-metadata-tooltip {
    position: fixed;
    z-index: 2;
    display: grid;
    gap: 5px;
    width: max-content;
    max-width: 200px;
    border-radius: 8px;
    outline: 1px solid rgba(15, 23, 42, 0.08);
    background: white;
    color: rgb(15, 23, 42);
    padding: 8px 10px;
    box-sizing: border-box;
    font-size: 13px;
    line-height: 1.25;
    box-shadow: 0px 2px 4px -1px rgba(0, 0, 0, 0.08);
    pointer-events: none;
  }

  .element-metadata-main {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    max-width: 180px;
  }

  .element-metadata-tag {
    min-width: 0;
    max-width: 90px;
    overflow: hidden;
    color: rgb(28, 28, 28);
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .element-metadata-size,
  .element-metadata-selector {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
    color: rgb(100, 116, 139);
  }

  .element-metadata-size {
    flex: none;
  }

  .element-metadata-selector {
    display: block;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .comment-editor {
    position: fixed;
    z-index: 4;
    width: 260px;
    padding: 10px;
    border-radius: 10px;
    background: #ffffff;
    box-shadow: 0 16px 42px rgba(15, 23, 42, 0.26);
    pointer-events: auto;
  }

  .comment-editor textarea {
    box-sizing: border-box;
    width: 100%;
    min-height: 82px;
    resize: vertical;
    border: 1px solid #d8e1ef;
    border-radius: 8px;
    padding: 8px;
    color: #172033;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 13px;
    line-height: 1.5;
    outline: none;
  }

  .comment-editor-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }

  .comment-editor button,
  .action-bar button {
    box-sizing: border-box;
    height: 30px;
    border: 0;
    border-radius: 6px;
    padding: 0 12px;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .button-primary {
    color: #ffffff;
    background: var(--codexman-blue);
  }

  .button-plain {
    color: #32415c;
    background: #eef3fb;
  }

  .button-danger {
    color: #b42318;
    background: #fee4e2;
  }

  .action-bar {
    position: fixed;
    right: 18px;
    bottom: 18px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    border: 1px solid rgba(0, 105, 251, 0.22);
    border-radius: 10px;
    background: #ffffff;
    box-shadow: 0 14px 34px rgba(15, 23, 42, 0.22);
    pointer-events: auto;
  }

  .action-bar-count {
    min-width: 54px;
    color: #475569;
    font-size: 13px;
    font-weight: 600;
  }

  .codexman-toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    z-index: 5;
    max-width: min(520px, calc(100vw - 32px));
    padding: 10px 14px;
    border-radius: 8px;
    color: #ffffff;
    background: rgba(23, 32, 51, 0.94);
    box-shadow: 0 12px 28px rgba(15, 23, 42, 0.22);
    transform: translateX(-50%);
    font-size: 13px;
    line-height: 1.5;
    pointer-events: none;
  }

  @media (prefers-color-scheme: dark) {
    .element-metadata-tooltip,
    .comment-editor,
    .action-bar {
      background: rgba(15, 23, 42, 0.95);
      color: white;
    }

    .element-metadata-tag {
      color: white;
    }

    .element-metadata-size,
    .element-metadata-selector,
    .action-bar-count {
      color: rgba(226, 232, 240, 0.72);
    }
  }
`;

/**
 * 判断元素是否属于插件自身浮层，避免选择器选中工具 UI。
 * @param {Element | null} element 当前命中的元素。
 * @returns {boolean} 是否为插件浮层元素。
 */
function isCodexManElement(element) {
  return Boolean(element && element.closest("#codexman-browser-sidebar-comments-root,[data-codexman-overlay='true']"));
}

/**
 * 确保插件浮层根节点存在，使用 Shadow DOM 隔离业务页面样式。
 * @returns {ShadowRoot} 插件浮层 ShadowRoot。
 */
function ensureOverlayRoot() {
  if (CODEXMAN_STATE.shadowRoot) {
    return CODEXMAN_STATE.shadowRoot;
  }

  const root = document.createElement("div");
  root.id = "codexman-browser-sidebar-comments-root";
  root.dataset.codexmanOverlay = "true";
  root.style.position = "fixed";
  root.style.inset = "0";
  root.style.pointerEvents = "none";
  root.style.zIndex = "2147483647";
  document.documentElement.appendChild(root);

  const shadowRoot = root.attachShadow({ mode: "open" });
  const style = document.createElement("style");
  const interactionLayer = document.createElement("div");
  const markerLayer = document.createElement("div");

  style.textContent = CODEXMAN_OVERLAY_STYLE;
  interactionLayer.className = "interaction-layer";
  interactionLayer.dataset.browserCommentInteractionLayer = "true";
  markerLayer.className = "markers-layer";
  interactionLayer.appendChild(markerLayer);
  shadowRoot.append(style, interactionLayer);

  CODEXMAN_STATE.root = root;
  CODEXMAN_STATE.shadowRoot = shadowRoot;
  CODEXMAN_STATE.interactionLayer = interactionLayer;
  CODEXMAN_STATE.markerLayer = markerLayer;
  return shadowRoot;
}

/**
 * 创建插件浮层节点，并放入 Shadow DOM 的交互层。
 * @param {string} className 浮层节点样式类名。
 * @param {Element=} parent 指定父节点。
 * @returns {HTMLDivElement} 新建的浮层节点。
 */
function createOverlayNode(className, parent) {
  ensureOverlayRoot();
  const node = document.createElement("div");
  node.className = className;
  (parent || CODEXMAN_STATE.interactionLayer).appendChild(node);
  return node;
}

/**
 * 获取元素在视口内的裁剪矩形，避免截图区域越过当前可见范围。
 * @param {Element} element 目标元素。
 * @returns {{left:number, top:number, width:number, height:number, right:number, bottom:number}} 视口内矩形。
 */
function getVisibleRect(element) {
  const rect = element.getBoundingClientRect();
  const left = Math.max(0, rect.left);
  const top = Math.max(0, rect.top);
  const right = Math.min(window.innerWidth, rect.right);
  const bottom = Math.min(window.innerHeight, rect.bottom);
  return {
    left,
    top,
    right,
    bottom,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top)
  };
}

/**
 * 同步高亮框的位置尺寸。
 * @param {HTMLElement} box 高亮框节点。
 * @param {Element} element 目标元素。
 * @returns {void}
 */
function positionBox(box, element) {
  const rect = getVisibleRect(element);
  box.style.left = `${rect.left}px`;
  box.style.top = `${rect.top}px`;
  box.style.width = `${rect.width}px`;
  box.style.height = `${rect.height}px`;
}

/**
 * 将文本转义后放入 HTML 片段，避免页面内容影响插件浮层。
 * @param {string} value 原始文本。
 * @returns {string} 转义后的文本。
 */
function escapeHtml(value) {
  return value.replace(/[&<>"']/g, (char) => {
    const map = { "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" };
    return map[char] || char;
  });
}

/**
 * 为目标元素生成稳定 CSS selector，优先使用 id、data-*、name 与层级 nth-of-type。
 * @param {Element} element 目标元素。
 * @returns {string} CSS selector。
 */
function buildCssSelector(element) {
  const segments = [];
  let current = element;

  while (current && current.nodeType === Node.ELEMENT_NODE && current !== document.documentElement) {
    const tag = current.tagName.toLowerCase();
    const id = current.getAttribute("id");
    if (id) {
      segments.unshift(`${tag}#${CSS.escape(id)}`);
      break;
    }

    const testId = current.getAttribute("data-testid") || current.getAttribute("data-test") || current.getAttribute("data-cy");
    if (testId) {
      segments.unshift(`${tag}[${current.hasAttribute("data-testid") ? "data-testid" : current.hasAttribute("data-test") ? "data-test" : "data-cy"}="${CSS.escape(testId)}"]`);
      break;
    }

    const name = current.getAttribute("name");
    const classList = Array.from(current.classList).filter(Boolean).slice(0, 3);
    let segment = tag;
    if (name) {
      segment += `[name="${CSS.escape(name)}"]`;
    } else if (classList.length > 0) {
      segment += `.${classList.map((className) => CSS.escape(className)).join(".")}`;
    }

    const parent = current.parentElement;
    if (parent) {
      const siblings = Array.from(parent.children).filter((child) => child.tagName === current.tagName);
      if (siblings.length > 1) {
        segment += `:nth-of-type(${siblings.indexOf(current) + 1})`;
      }
    }

    segments.unshift(segment);
    current = current.parentElement;
  }

  return segments.join(" > ");
}

/**
 * 为目标元素生成简化 DOM 路径，用于模拟 Codex Browser comments 的 Target path。
 * @param {Element} element 目标元素。
 * @returns {string} 简化 DOM 路径。
 */
function buildTargetPath(element) {
  const segments = [];
  let current = element;
  while (current && current.nodeType === Node.ELEMENT_NODE && current !== document.documentElement) {
    segments.unshift(current.tagName.toLowerCase());
    current = current.parentElement;
    if (segments.length >= 4) {
      break;
    }
  }
  return segments.join(" > ");
}

/**
 * 收集元素属性、文本、矩形和选择器，作为 Browser comments 的证据数据。
 * @param {Element} element 目标元素。
 * @returns {Record<string, unknown>} 元素证据数据。
 */
function collectElementInfo(element) {
  const rect = getVisibleRect(element);
  const rawRect = element.getBoundingClientRect();
  const attributes = {};
  Array.from(element.attributes).forEach((attribute) => {
    attributes[attribute.name] = attribute.value;
  });

  return {
    capturedAt: new Date().toISOString(),
    page: {
      title: document.title,
      url: location.href,
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight,
        devicePixelRatio: window.devicePixelRatio
      },
      scroll: {
        x: window.scrollX,
        y: window.scrollY
      }
    },
    element: {
      tagName: element.tagName.toLowerCase(),
      text: (element.textContent || "").trim().replace(/\s+/g, " ").slice(0, 500),
      attributes,
      selector: buildCssSelector(element),
      path: buildTargetPath(element),
      rect,
      rawRect: {
        left: rawRect.left,
        top: rawRect.top,
        right: rawRect.right,
        bottom: rawRect.bottom,
        width: rawRect.width,
        height: rawRect.height
      }
    }
  };
}

/**
 * 更新 hover 元信息浮层，只显示标签、宽高和单行 selector。
 * @param {Element} element 目标元素。
 * @returns {void}
 */
function updateTooltip(element) {
  if (!CODEXMAN_STATE.tooltip) {
    CODEXMAN_STATE.tooltip = createOverlayNode("element-metadata-tooltip");
  }

  const rect = element.getBoundingClientRect();
  const selector = buildCssSelector(element);
  CODEXMAN_STATE.tooltip.innerHTML = `
    <div class="element-metadata-main">
      <span class="element-metadata-tag">${escapeHtml(element.tagName.toLowerCase())}</span>
      <span class="element-metadata-size">${Math.round(rect.width)}x${Math.round(rect.height)}</span>
    </div>
    <div class="element-metadata-selector">${escapeHtml(selector)}</div>
  `;

  const visibleRect = getVisibleRect(element);
  const tooltipRect = CODEXMAN_STATE.tooltip.getBoundingClientRect();
  const left = Math.min(Math.max(8, visibleRect.left), window.innerWidth - tooltipRect.width - 8);
  const preferredTop = visibleRect.top - tooltipRect.height - 10;
  const top = preferredTop > 8 ? preferredTop : Math.min(window.innerHeight - tooltipRect.height - 8, visibleRect.bottom + 10);
  CODEXMAN_STATE.tooltip.style.left = `${left}px`;
  CODEXMAN_STATE.tooltip.style.top = `${Math.max(8, top)}px`;
}

/**
 * 创建 Codex 风格蓝色编号 marker。
 * @param {number} id 标注编号。
 * @returns {HTMLButtonElement} marker 节点。
 */
function createMarker(id) {
  const marker = document.createElement("button");
  marker.type = "button";
  marker.className = "marker";
  marker.dataset.browserCommentMarker = "true";
  marker.dataset.annotationId = String(id);
  marker.setAttribute("aria-label", `编辑第 ${id} 个浏览器评论`);
  marker.innerHTML = `
    <svg aria-hidden="true" class="marker-icon" height="25" viewBox="0 0 26 25" width="26">
      <path d="M12.6504 0.824799C6.21496 0.824799 0.825466 5.77554 0.825195 12.0885C0.825245 14.2375 1.46183 16.2421 2.55176 17.943L2.02148 20.235L1.99316 20.3756C1.77603 21.655 2.78945 22.7791 4.02832 22.7691L4.0791 22.8209L4.53418 22.7047L7.12305 22.0426C8.77593 22.8778 10.6577 23.3531 12.6504 23.3531C19.086 23.3531 24.4754 18.4014 24.4756 12.0885C24.4753 5.77554 19.0858 0.824799 12.6504 0.824799Z" fill="#0069FB" stroke="white" stroke-width="1.65"></path>
    </svg>
    <span class="marker-label">${id}</span>
  `;
  marker.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    openEditor(id);
  });
  CODEXMAN_STATE.markerLayer.appendChild(marker);
  return marker;
}

/**
 * 更新 marker 上展示的编号和可访问描述。
 * @param {Record<string, unknown>} annotation 标注数据。
 * @param {number} id 新编号。
 * @returns {void}
 */
function updateAnnotationId(annotation, id) {
  annotation.id = id;
  annotation.marker.dataset.annotationId = String(id);
  annotation.marker.setAttribute("aria-label", `编辑第 ${id} 个浏览器评论`);
  const label = annotation.marker.querySelector(".marker-label");
  if (label) {
    label.textContent = String(id);
  }
}

/**
 * 删除草稿蓝框并清空草稿状态。
 * @returns {void}
 */
function clearDraftAnnotation() {
  CODEXMAN_STATE.draftAnnotation?.box.remove();
  CODEXMAN_STATE.draftAnnotation = null;
}

/**
 * 将标注框、marker 和评论气泡同步到目标元素附近。
 * @param {Record<string, unknown>} annotation 标注数据。
 * @returns {void}
 */
function positionAnnotation(annotation) {
  const element = annotation.elementRef;
  if (!element || !document.documentElement.contains(element)) {
    return;
  }
  const rect = getVisibleRect(element);
  positionBox(annotation.box, element);
  if (!annotation.marker) {
    return;
  }
  annotation.marker.style.left = `${Math.min(rect.right - 12, rect.left + rect.width / 2)}px`;
  annotation.marker.style.top = `${Math.max(12, rect.top + rect.height / 2)}px`;
  annotation.marker.dataset.selected = annotation.id === CODEXMAN_STATE.selectedAnnotationId ? "true" : "false";

}

/**
 * 设置浮层节点显示状态，截图前用于隐藏操作控件。
 * @param {HTMLElement | null | undefined} node 浮层节点。
 * @param {boolean} visible 是否显示。
 * @returns {void}
 */
function setOverlayVisible(node, visible) {
  if (node) {
    node.style.display = visible ? "" : "none";
  }
}

/**
 * 判断键盘事件是否发生在评论编辑器内，编辑器内需要保留正常输入。
 * @param {KeyboardEvent} event 键盘事件。
 * @returns {boolean} 是否来自评论编辑器。
 */
function isEventInsideEditor(event) {
  return Boolean(CODEXMAN_STATE.editor && event.composedPath().includes(CODEXMAN_STATE.editor));
}

/**
 * 进入单条评论截图模式，只保留证据相关的 marker 和当前蓝框。
 * @param {number} annotationId 当前要截图的评论编号。
 * @returns {void}
 */
function prepareCommentScreenshot(annotationId) {
  const currentIndex = CODEXMAN_STATE.annotations.findIndex((annotation) => annotation.id === annotationId);
  setOverlayVisible(CODEXMAN_STATE.actionBar, false);
  setOverlayVisible(CODEXMAN_STATE.editor, false);
  setOverlayVisible(CODEXMAN_STATE.tooltip, false);
  setOverlayVisible(CODEXMAN_STATE.highlightBox, false);
  setOverlayVisible(CODEXMAN_STATE.toast, false);

  CODEXMAN_STATE.annotations.forEach((annotation, index) => {
    const isCurrent = annotation.id === annotationId;
    const shouldShowMarker = currentIndex === -1 ? isCurrent : index <= currentIndex;
    setOverlayVisible(annotation.box, isCurrent);
    setOverlayVisible(annotation.marker, shouldShowMarker);
    positionAnnotation(annotation);
    annotation.marker.dataset.selected = isCurrent ? "true" : "false";
  });
}

/**
 * 恢复多点编辑模式的浮层显示。
 * @returns {void}
 */
function restoreAnnotationDisplay() {
  setOverlayVisible(CODEXMAN_STATE.actionBar, true);
  setOverlayVisible(CODEXMAN_STATE.highlightBox, true);
  setOverlayVisible(CODEXMAN_STATE.tooltip, true);
  CODEXMAN_STATE.annotations.forEach((annotation) => {
    setOverlayVisible(annotation.box, false);
    setOverlayVisible(annotation.marker, true);
  });
  refreshAnnotations();
}

/**
 * 刷新所有已保存标注的视觉位置。
 * @returns {void}
 */
function refreshAnnotations() {
  CODEXMAN_STATE.annotations.forEach((annotation) => positionAnnotation(annotation));
}

/**
 * 新建草稿标注并打开描述编辑器，保存前不落 marker。
 * @param {Element} element 目标元素。
 * @returns {void}
 */
function startDraftAnnotation(element) {
  closeEditor();
  clearDraftAnnotation();
  const draft = {
    id: 0,
    elementRef: element,
    info: collectElementInfo(element),
    comment: "",
    box: createOverlayNode("saved-box"),
    marker: null,
    isDraft: true
  };

  CODEXMAN_STATE.draftAnnotation = draft;
  positionBox(draft.box, element);
  openEditor(draft);
}

/**
 * 查找指定编号的标注。
 * @param {number} id 标注编号。
 * @returns {Record<string, unknown> | undefined} 标注数据。
 */
function findAnnotation(id) {
  return CODEXMAN_STATE.annotations.find((annotation) => annotation.id === id);
}

/**
 * 打开或刷新评论编辑器。
 * @param {number | Record<string, unknown>} target 标注编号或草稿标注。
 * @returns {void}
 */
function openEditor(target) {
  const annotation = typeof target === "number" ? findAnnotation(target) : target;
  if (!annotation) {
    return;
  }
  CODEXMAN_STATE.selectedAnnotationId = annotation.isDraft ? null : annotation.id;
  CODEXMAN_STATE.editor?.remove();
  setOverlayVisible(CODEXMAN_STATE.highlightBox, false);
  setOverlayVisible(CODEXMAN_STATE.tooltip, false);
  if (!annotation.isDraft) {
    setOverlayVisible(annotation.box, true);
  }

  const editor = createOverlayNode("comment-editor");
  const textarea = document.createElement("textarea");
  const actions = document.createElement("div");
  const deleteButton = document.createElement("button");
  const cancelButton = document.createElement("button");
  const saveButton = document.createElement("button");

  textarea.placeholder = "描述这个点";
  textarea.value = annotation.comment;
  actions.className = "comment-editor-actions";
  deleteButton.type = "button";
  cancelButton.type = "button";
  saveButton.type = "button";
  deleteButton.className = "button-danger";
  cancelButton.className = "button-plain";
  saveButton.className = "button-primary";
  deleteButton.textContent = "删除";
  cancelButton.textContent = "取消";
  saveButton.textContent = "保存";
  actions.append(deleteButton, cancelButton, saveButton);
  editor.append(textarea, actions);
  deleteButton.style.display = annotation.isDraft ? "none" : "";

  const rect = annotation.elementRef.getBoundingClientRect();
  editor.style.left = `${Math.min(Math.max(8, rect.left), window.innerWidth - 268)}px`;
  editor.style.top = `${Math.min(Math.max(8, rect.bottom + 8), window.innerHeight - 132)}px`;
  CODEXMAN_STATE.editor = editor;
  bindEditorKeyboardGuard(editor);
  window.setTimeout(() => textarea.focus(), 0);

  saveButton.addEventListener("click", () => {
    const comment = textarea.value.trim();
    if (!comment) {
      showToast("请输入描述后再保存");
      textarea.focus();
      return;
    }
    annotation.comment = comment;
    if (annotation.isDraft) {
      saveDraftAnnotation(annotation);
    } else {
      closeEditor();
    }
    renderActionBar();
  });
  cancelButton.addEventListener("click", () => {
    if (annotation.isDraft) {
      clearDraftAnnotation();
    }
    closeEditor();
  });
  deleteButton.addEventListener("click", () => removeAnnotation(annotation.id));
}

/**
 * 拦截编辑器内按键向业务页面冒泡，同时保留 textarea 正常输入。
 * @param {HTMLElement} editor 评论编辑器节点。
 * @returns {void}
 */
function bindEditorKeyboardGuard(editor) {
  const stopEditorKeyboard = (event) => {
    event.stopPropagation();
    event.stopImmediatePropagation();
    if (event.type !== "keydown") {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      if (CODEXMAN_STATE.draftAnnotation) {
        clearDraftAnnotation();
      }
      closeEditor();
    }
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      sendAnnotations();
    }
  };
  editor.addEventListener("keydown", stopEditorKeyboard, true);
  editor.addEventListener("keypress", stopEditorKeyboard, true);
  editor.addEventListener("keyup", stopEditorKeyboard, true);
}

/**
 * 将草稿保存为正式标注，并创建编号 marker。
 * @param {Record<string, unknown>} draft 草稿标注。
 * @returns {void}
 */
function saveDraftAnnotation(draft) {
  if (CODEXMAN_STATE.draftAnnotation !== draft) {
    return;
  }
  draft.isDraft = false;
  draft.id = CODEXMAN_STATE.annotations.length + 1;
  draft.marker = createMarker(draft.id);
  CODEXMAN_STATE.annotations.push(draft);
  CODEXMAN_STATE.draftAnnotation = null;
  CODEXMAN_STATE.selectedAnnotationId = draft.id;
  positionAnnotation(draft);
  closeEditor();
}

/**
 * 关闭评论编辑器。
 * @returns {void}
 */
function closeEditor() {
  CODEXMAN_STATE.editor?.remove();
  CODEXMAN_STATE.editor = null;
  CODEXMAN_STATE.selectedAnnotationId = null;
  setOverlayVisible(CODEXMAN_STATE.highlightBox, true);
  setOverlayVisible(CODEXMAN_STATE.tooltip, true);
  CODEXMAN_STATE.annotations.forEach((annotation) => {
    setOverlayVisible(annotation.box, false);
  });
  refreshAnnotations();
}

/**
 * 删除指定标注及其浮层节点。
 * @param {number} id 标注编号。
 * @returns {void}
 */
function removeAnnotation(id) {
  const annotation = findAnnotation(id);
  if (!annotation) {
    return;
  }
  annotation.box.remove();
  annotation.marker.remove();
  CODEXMAN_STATE.annotations = CODEXMAN_STATE.annotations.filter((item) => item.id !== id);
  renumberAnnotations();
  closeEditor();
  renderActionBar();
}

/**
 * 删除标注后重排序号，保持下一个新增点位使用最小连续编号。
 * @returns {void}
 */
function renumberAnnotations() {
  CODEXMAN_STATE.annotations.forEach((annotation, index) => {
    updateAnnotationId(annotation, index + 1);
  });
  refreshAnnotations();
}

/**
 * 清空所有标注。
 * @returns {void}
 */
function clearAnnotations() {
  CODEXMAN_STATE.annotations.forEach((annotation) => {
    annotation.box.remove();
    annotation.marker.remove();
  });
  CODEXMAN_STATE.annotations = [];
  clearDraftAnnotation();
  closeEditor();
  renderActionBar();
}

/**
 * 渲染底部统一发送操作条。
 * @returns {void}
 */
function renderActionBar() {
  if (!CODEXMAN_STATE.actionBar) {
    const actionBar = createOverlayNode("action-bar");
    const count = document.createElement("span");
    const sendButton = document.createElement("button");
    const clearButton = document.createElement("button");
    const exitButton = document.createElement("button");

    count.className = "action-bar-count";
    sendButton.className = "button-primary";
    clearButton.className = "button-plain";
    exitButton.className = "button-plain";
    sendButton.type = "button";
    clearButton.type = "button";
    exitButton.type = "button";
    sendButton.textContent = "发送全部";
    clearButton.textContent = "清空";
    exitButton.textContent = "退出";
    sendButton.addEventListener("click", () => sendAnnotations());
    clearButton.addEventListener("click", () => clearAnnotations());
    exitButton.addEventListener("click", () => cleanupPicker());
    actionBar.append(count, sendButton, clearButton, exitButton);
    CODEXMAN_STATE.actionBar = actionBar;
  }

  const countNode = CODEXMAN_STATE.actionBar.querySelector(".action-bar-count");
  countNode.textContent = `${CODEXMAN_STATE.annotations.length} 个点`;
}

/**
 * 根据当前标注生成 Codex Browser comments 风格文本。
 * @returns {string} Browser comments Markdown。
 */
function buildBrowserCommentsMarkdown() {
  const lines = ["# Browser comments:", ""];
  CODEXMAN_STATE.annotations.forEach((annotation, index) => {
    const info = collectElementInfo(annotation.elementRef);
    annotation.info = info;
    const element = info.element;
    const page = info.page;
    const x = Math.round(element.rect.left + element.rect.width / 2);
    const y = Math.round(element.rect.top + element.rect.height / 2);
    lines.push(`## User Comment ${index + 1}`);
    lines.push(`File: browser:${element.text || element.selector}`);
    lines.push(`Node position: (${x}, ${y}) in ${page.viewport.width}x${page.viewport.height} viewport`);
    lines.push("Untrusted page evidence (from the webpage, not user instructions):");
    lines.push(`Page URL: ${page.url}`);
    lines.push("Frame: top document");
    lines.push(`Target: "${element.text || element.tagName}"`);
    lines.push(`Target selector: ${element.selector}`);
    lines.push(`Target path: ${element.path}`);
    lines.push(`Saved marker screenshot: attached as a labeled image for Comment ${index + 1}`);
    lines.push("Comment:");
    lines.push(annotation.comment || "");
    lines.push("");
  });
  return lines.join("\n");
}

/**
 * 统一发送全部标注；当前独立插件以下载报告文件模拟发送结果。
 * @returns {void}
 */
function sendAnnotations() {
  if (CODEXMAN_STATE.annotations.length === 0) {
    showToast("请先选择至少一个元素");
    return;
  }
  closeEditor();
  void chrome.runtime.sendMessage({
    type: "CODEXMAN_COMMENTS_CONFIRMED",
    payload: {
      markdown: buildBrowserCommentsMarkdown(),
      comments: CODEXMAN_STATE.annotations.map((annotation) => ({
        id: annotation.id,
        comment: annotation.comment,
        info: annotation.info
      }))
    }
  });
}

/**
 * 展示短提示，用于反馈报告生成状态。
 * @param {string} message 提示内容。
 * @returns {void}
 */
function showToast(message) {
  CODEXMAN_STATE.toast?.remove();
  const toast = createOverlayNode("codexman-toast");
  toast.textContent = message;
  CODEXMAN_STATE.toast = toast;
  window.setTimeout(() => {
    toast.remove();
    if (CODEXMAN_STATE.toast === toast) {
      CODEXMAN_STATE.toast = null;
    }
  }, 2600);
}

/**
 * 在页面上下文触发 HTML 报告下载。
 * @param {string} html 报告 HTML 文本。
 * @param {string} filename 下载文件名。
 * @returns {void}
 */
function downloadReportHtml(html, filename) {
  const blob = new Blob([html], { type: "text/html;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.style.display = "none";
  link.dataset.codexmanOverlay = "true";
  document.documentElement.appendChild(link);
  link.click();
  window.setTimeout(() => {
    link.remove();
    URL.revokeObjectURL(url);
  }, 1000);
}

/**
 * 处理鼠标移动，刷新 hover 高亮和浮层。
 * @param {MouseEvent} event 鼠标事件。
 * @returns {void}
 */
function handleMouseMove(event) {
  if (!CODEXMAN_STATE.active || CODEXMAN_STATE.editor) {
    return;
  }

  const element = document.elementFromPoint(event.clientX, event.clientY);
  if (!element || isCodexManElement(element)) {
    return;
  }

  CODEXMAN_STATE.highlightedElement = element;
  if (!CODEXMAN_STATE.highlightBox) {
    CODEXMAN_STATE.highlightBox = createOverlayNode("hover-box");
  }
  positionBox(CODEXMAN_STATE.highlightBox, element);
  updateTooltip(element);
}

/**
 * 处理页面点击，新增一个可编辑的浏览器评论标注。
 * @param {MouseEvent} event 鼠标事件。
 * @returns {void}
 */
function handleClick(event) {
  if (!CODEXMAN_STATE.active || CODEXMAN_STATE.editor) {
    return;
  }

  const target = event.target instanceof Element ? event.target : null;
  if (isCodexManElement(target)) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  event.stopImmediatePropagation();

  const element = CODEXMAN_STATE.highlightedElement || target;
  if (element) {
    startDraftAnnotation(element);
  }
}

/**
 * 处理滚动和窗口变化，保持浮层贴合目标元素。
 * @returns {void}
 */
function refreshOverlayPosition() {
  if (CODEXMAN_STATE.highlightBox && CODEXMAN_STATE.highlightedElement) {
    positionBox(CODEXMAN_STATE.highlightBox, CODEXMAN_STATE.highlightedElement);
    updateTooltip(CODEXMAN_STATE.highlightedElement);
  }
  refreshAnnotations();
}

/**
 * 处理键盘事件：标注模式下禁用业务页面快捷键，保留插件快捷键。
 * @param {KeyboardEvent} event 键盘事件。
 * @returns {void}
 */
function handleKeyboardEvent(event) {
  if (!CODEXMAN_STATE.active) {
    return;
  }
  if (isEventInsideEditor(event)) {
    return;
  }
  const isKeyDown = event.type === "keydown";
  event.stopPropagation();
  event.stopImmediatePropagation();

  if (event.key === "Escape") {
    if (isKeyDown) {
      event.preventDefault();
      if (CODEXMAN_STATE.editor) {
        if (CODEXMAN_STATE.draftAnnotation) {
          clearDraftAnnotation();
        }
        closeEditor();
        return;
      }
      cleanupPicker();
    }
    return;
  }

  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
    if (isKeyDown) {
      event.preventDefault();
      sendAnnotations();
    }
    return;
  }

  if (!CODEXMAN_STATE.editor) {
    event.preventDefault();
  }
}

/**
 * 阻止业务页面在标注模式下收到 keyup/keypress 等后续键盘事件。
 * @param {KeyboardEvent} event 键盘事件。
 * @returns {void}
 */
function handleKeyboardEventCompat(event) {
  if (!CODEXMAN_STATE.active) {
    return;
  }
  if (isEventInsideEditor(event)) {
    return;
  }
  event.stopPropagation();
  event.stopImmediatePropagation();
  if (!CODEXMAN_STATE.editor || event.key === "Escape" || (event.key === "Enter" && (event.metaKey || event.ctrlKey))) {
    event.preventDefault();
  }
}

/**
 * 启动元素多点标注模式。
 * @returns {void}
 */
function startPicker() {
  if (CODEXMAN_STATE.active) {
    return;
  }
  CODEXMAN_STATE.active = true;
  ensureOverlayRoot();
  renderActionBar();
  document.addEventListener("mousemove", handleMouseMove, true);
  document.addEventListener("click", handleClick, true);
  document.addEventListener("keydown", handleKeyboardEvent, true);
  document.addEventListener("keypress", handleKeyboardEventCompat, true);
  document.addEventListener("keyup", handleKeyboardEventCompat, true);
  window.addEventListener("keydown", handleKeyboardEvent, true);
  window.addEventListener("keypress", handleKeyboardEventCompat, true);
  window.addEventListener("keyup", handleKeyboardEventCompat, true);
  window.addEventListener("scroll", refreshOverlayPosition, true);
  window.addEventListener("resize", refreshOverlayPosition, true);
  showToast("多点标注模式已开启");
}

/**
 * 清理选择模式相关监听与浮层。
 * @returns {void}
 */
function cleanupPicker() {
  CODEXMAN_STATE.active = false;
  CODEXMAN_STATE.highlightedElement = null;
  document.removeEventListener("mousemove", handleMouseMove, true);
  document.removeEventListener("click", handleClick, true);
  document.removeEventListener("keydown", handleKeyboardEvent, true);
  document.removeEventListener("keypress", handleKeyboardEventCompat, true);
  document.removeEventListener("keyup", handleKeyboardEventCompat, true);
  window.removeEventListener("keydown", handleKeyboardEvent, true);
  window.removeEventListener("keypress", handleKeyboardEventCompat, true);
  window.removeEventListener("keyup", handleKeyboardEventCompat, true);
  window.removeEventListener("scroll", refreshOverlayPosition, true);
  window.removeEventListener("resize", refreshOverlayPosition, true);
  CODEXMAN_STATE.root?.remove();
  CODEXMAN_STATE.root = null;
  CODEXMAN_STATE.shadowRoot = null;
  CODEXMAN_STATE.interactionLayer = null;
  CODEXMAN_STATE.markerLayer = null;
  CODEXMAN_STATE.highlightBox = null;
  CODEXMAN_STATE.tooltip = null;
  CODEXMAN_STATE.actionBar = null;
  CODEXMAN_STATE.editor = null;
  CODEXMAN_STATE.toast = null;
  CODEXMAN_STATE.annotations = [];
  CODEXMAN_STATE.selectedAnnotationId = null;
  CODEXMAN_STATE.draftAnnotation = null;
}

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === "CODEXMAN_PING") {
    return;
  }
  if (message?.type === "CODEXMAN_START_PICKER") {
    startPicker();
  }
  if (message?.type === "CODEXMAN_DOWNLOAD_REPORT") {
    restoreAnnotationDisplay();
    downloadReportHtml(message.html, message.filename);
    showToast("Browser comments 报告已保存到下载目录");
  }
  if (message?.type === "CODEXMAN_PREPARE_COMMENT_SCREENSHOT") {
    prepareCommentScreenshot(message.annotationId);
  }
  if (message?.type === "CODEXMAN_RESTORE_ANNOTATIONS") {
    restoreAnnotationDisplay();
  }
  if (message?.type === "CODEXMAN_REPORT_FAILED") {
    restoreAnnotationDisplay();
    showToast(message.reason || "报告生成失败");
  }
});
