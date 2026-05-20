(function kodaalPopup(root) {
  "use strict";

  function extensionApi() {
    return root.browser || root.chrome || null;
  }

  async function sendMessage(message) {
    const api = extensionApi();
    const result = api.runtime.sendMessage(message);
    return result && typeof result.then === "function" ? result : null;
  }

  async function activeTab() {
    const api = extensionApi();
    const result = api.tabs.query({ active: true, currentWindow: true });
    const tabs = result && typeof result.then === "function" ? await result : [];
    return tabs[0] || null;
  }

  function setText(id, text) {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = text;
    }
  }

  function show(id, visible) {
    const element = document.getElementById(id);
    if (element) {
      element.classList.toggle("hidden", !visible);
    }
  }

  function renderRecent(items) {
    const list = document.getElementById("recent");
    if (!list) {
      return;
    }
    list.textContent = "";
    for (const item of items || []) {
      const row = document.createElement("li");
      const text = document.createElement("span");
      text.textContent = item.text || "";
      row.appendChild(text);
      list.appendChild(row);
    }
  }

  async function refresh() {
    const tab = await activeTab();
    const response = await sendMessage({ type: "getPopupState", tabId: tab ? tab.id : null });
    if (!response || !response.ok) {
      setText("connection", "Background unavailable");
      show("setup", true);
      show("controls", false);
      show("recent-panel", false);
      return;
    }
    const state = response.state;
    setText("connection", state.connected ? "Connected to local core" : "Local core offline");
    show("setup", !state.tokenConfigured);
    show("controls", state.tokenConfigured);
    show("recent-panel", state.tokenConfigured);
    setText("global-state", state.status && state.status.paused ? "Paused" : "Capturing");
    setText("pause-toggle", state.status && state.status.paused ? "Resume" : "Pause");
    setText("tab-label", tab && tab.url ? new URL(tab.url).hostname : "This tab");
    setText("tab-toggle", state.perTabDisabled ? "Enable" : "Disable");
    renderRecent(state.recent || []);
  }

  async function saveToken() {
    const input = document.getElementById("token-input");
    const token = input ? input.value.trim() : "";
    const response = await sendMessage({ type: "setToken", token });
    if (!response || !response.ok) {
      setText("setup-error", response && response.error ? response.error : "Token could not be saved");
      return;
    }
    setText("setup-error", "");
    await refresh();
  }

  async function togglePause() {
    const button = document.getElementById("pause-toggle");
    const messageType = button && button.textContent === "Resume" ? "resume" : "pause";
    await sendMessage({ type: messageType });
    await refresh();
  }

  async function toggleTab() {
    const tab = await activeTab();
    if (!tab) {
      return;
    }
    const button = document.getElementById("tab-toggle");
    await sendMessage({ type: "toggleTab", tabId: tab.id, disabled: button && button.textContent === "Disable" });
    await refresh();
  }

  async function openUi() {
    const api = extensionApi();
    await api.tabs.create({ url: "http://127.0.0.1:7878/ui" });
  }

  function bind() {
    document.getElementById("save-token").addEventListener("click", () => saveToken());
    document.getElementById("pause-toggle").addEventListener("click", () => togglePause());
    document.getElementById("tab-toggle").addEventListener("click", () => toggleTab());
    document.getElementById("open-ui").addEventListener("click", () => openUi());
    document.getElementById("sync-now").addEventListener("click", async () => {
      await sendMessage({ type: "drainQueue" });
      await refresh();
    });
  }

  if (typeof module !== "undefined" && module.exports) {
    module.exports = { renderRecent, setText, show };
  } else {
    document.addEventListener("DOMContentLoaded", () => {
      bind();
      refresh().catch(() => setText("connection", "Unable to read extension state"));
    });
  }
})(globalThis);
