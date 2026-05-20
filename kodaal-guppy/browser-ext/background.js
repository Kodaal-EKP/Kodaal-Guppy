(function kodaalBackground(root) {
  "use strict";

  const STORAGE_KEYS = {
    token: "kodaal_token",
    endpoint: "kodaal_endpoint",
    status: "kodaal_status_cache",
    perTabDisabled: "kodaal_per_tab_disabled",
    recent: "kodaal_recent_captures"
  };
  const DEFAULT_ENDPOINT = "http://127.0.0.1:7878";
  const DB_NAME = "kodaal_queue";
  const STORE_NAME = "pending_captures";
  const NATIVE_HOST = "com.kodaal.guppy";
  const MAX_RETRIES = 5;
  const RECENT_LIMIT = 5;

  class CoreHttpError extends Error {
    constructor(status, code, message) {
      super(message || code || `HTTP_${status}`);
      this.status = status;
      this.code = code || `HTTP_${status}`;
    }
  }

  function extensionApi() {
    return root.browser || root.chrome || null;
  }

  function normalizeEndpoint(value) {
    const raw = String(value || DEFAULT_ENDPOINT).trim() || DEFAULT_ENDPOINT;
    const url = new URL(raw);
    if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || url.port !== "7878") {
      throw new Error("Endpoint must be http://127.0.0.1:7878");
    }
    return "http://127.0.0.1:7878";
  }

  function isValidToken(token) {
    return /^[a-f0-9]{64}$/.test(String(token || ""));
  }

  function isCapturePayload(payload) {
    return (
      payload &&
      typeof payload.text === "string" &&
      payload.text.trim().length > 0 &&
      payload.source === "browser" &&
      typeof payload.source_app === "string" &&
      payload.source_app.length > 0
    );
  }

  async function storageGet(keys) {
    const api = extensionApi();
    if (!api || !api.storage || !api.storage.local) {
      return {};
    }
    if (root.browser && api === root.browser) {
      return api.storage.local.get(keys);
    }
    return new Promise((resolve) => api.storage.local.get(keys, resolve));
  }

  async function storageSet(values) {
    const api = extensionApi();
    if (!api || !api.storage || !api.storage.local) {
      return;
    }
    if (root.browser && api === root.browser) {
      await api.storage.local.set(values);
      return;
    }
    await new Promise((resolve) => api.storage.local.set(values, resolve));
  }

  async function storageRemove(keys) {
    const api = extensionApi();
    if (!api || !api.storage || !api.storage.local) {
      return;
    }
    if (root.browser && api === root.browser) {
      await api.storage.local.remove(keys);
      return;
    }
    await new Promise((resolve) => api.storage.local.remove(keys, resolve));
  }

  async function getSettings() {
    const values = await storageGet([STORAGE_KEYS.token, STORAGE_KEYS.endpoint]);
    let token = values[STORAGE_KEYS.token] || "";
    if (!isValidToken(token)) {
      token = await discoverNativeToken().catch(() => "");
    }
    return {
      token,
      endpoint: normalizeEndpoint(values[STORAGE_KEYS.endpoint] || DEFAULT_ENDPOINT)
    };
  }

  async function discoverNativeToken() {
    const api = extensionApi();
    if (!api || !api.runtime || !api.runtime.sendNativeMessage) {
      throw new Error("NATIVE_HOST_UNAVAILABLE");
    }
    const response = await new Promise((resolve, reject) => {
      const callback = (message) => {
        const lastError = api.runtime.lastError;
        if (lastError) {
          reject(new Error(lastError.message || "NATIVE_HOST_UNAVAILABLE"));
          return;
        }
        resolve(message);
      };
      try {
        api.runtime.sendNativeMessage(NATIVE_HOST, { type: "token" }, callback);
      } catch (error) {
        reject(error);
      }
    });
    const token = response && typeof response.token === "string" ? response.token : "";
    if (!isValidToken(token)) {
      throw new Error("AUTH_TOKEN_INVALID");
    }
    await storageSet({ [STORAGE_KEYS.token]: token });
    return token;
  }

  async function setBadge(text) {
    const api = extensionApi();
    if (api && api.action && api.action.setBadgeText) {
      await api.action.setBadgeText({ text });
    }
  }

  async function apiFetch(path, options) {
    const settings = await getSettings();
    if (!isValidToken(settings.token)) {
      throw new Error("AUTH_TOKEN_MISSING");
    }
    const response = await root.fetch(`${settings.endpoint}${path}`, {
      method: options.method || "GET",
      headers: {
        "Content-Type": "application/json",
        "X-Kodaal-Token": settings.token
      },
      body: options.body || undefined
    });
    if (!response.ok) {
      const errorBody = await response.json().catch(() => null);
      const code = errorBody && errorBody.error && errorBody.error.code ? errorBody.error.code : `HTTP_${response.status}`;
      const message = errorBody && errorBody.error && errorBody.error.message ? errorBody.error.message : code;
      if (response.status === 401) {
        await storageRemove(STORAGE_KEYS.token);
        await setBadge("!");
      }
      throw new CoreHttpError(response.status, code, message);
    }
    if (response.status === 204) {
      return null;
    }
    return response.json();
  }

  async function healthz() {
    const settings = await getSettings();
    const response = await root.fetch(`${settings.endpoint}/healthz`, { method: "GET" });
    return response.ok;
  }

  async function refreshStatus() {
    const status = await apiFetch("/api/capture/status", { method: "GET" });
    const cached = {
      paused: Boolean(status.paused),
      blocklist: normalizeBlocklist(status.blocklist),
      fetched_at: new Date().toISOString()
    };
    await storageSet({ [STORAGE_KEYS.status]: cached });
    await setBadge(cached.paused ? "II" : "");
    return cached;
  }

  async function getCachedStatus() {
    const values = await storageGet(STORAGE_KEYS.status);
    const cached = values[STORAGE_KEYS.status] || { paused: false, blocklist: normalizeBlocklist(null), fetched_at: null };
    return { ...cached, blocklist: normalizeBlocklist(cached.blocklist) };
  }

  function openQueueDb() {
    return new Promise((resolve, reject) => {
      const request = root.indexedDB.open(DB_NAME, 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains(STORE_NAME)) {
          db.createObjectStore(STORE_NAME, { keyPath: "id" });
        }
      };
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);
    });
  }

  async function withStore(mode, callback) {
    const db = await openQueueDb();
    try {
      return await new Promise((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, mode);
        const store = tx.objectStore(STORE_NAME);
        const request = callback(store);
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
      });
    } finally {
      db.close();
    }
  }

  function queueId() {
    if (root.crypto && root.crypto.randomUUID) {
      return root.crypto.randomUUID();
    }
    return `capture-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  async function queueCapture(payload, errorMessage) {
    const item = {
      id: queueId(),
      payload,
      attempts: 0,
      enqueued_at: new Date().toISOString(),
      last_error: String(errorMessage || "")
    };
    await withStore("readwrite", (store) => store.put(item));
    return item.id;
  }

  async function queueAll() {
    return withStore("readonly", (store) => store.getAll());
  }

  async function queuePut(item) {
    return withStore("readwrite", (store) => store.put(item));
  }

  async function queueDelete(id) {
    return withStore("readwrite", (store) => store.delete(id));
  }

  async function postCapture(payload) {
    const result = await apiFetch("/api/prompts", {
      method: "POST",
      body: JSON.stringify(payload)
    });
    await rememberCapture(payload);
    return result;
  }

  async function rememberCapture(payload) {
    const values = await storageGet(STORAGE_KEYS.recent);
    const recent = Array.isArray(values[STORAGE_KEYS.recent]) ? values[STORAGE_KEYS.recent] : [];
    recent.unshift({
      text: payload.text.slice(0, 160),
      source_app: payload.source_app,
      captured_at: new Date().toISOString()
    });
    await storageSet({ [STORAGE_KEYS.recent]: recent.slice(0, RECENT_LIMIT) });
  }

  async function drainQueue() {
    const items = (await queueAll()).sort((left, right) => left.enqueued_at.localeCompare(right.enqueued_at));
    for (const item of items) {
      if (item.attempts >= MAX_RETRIES) {
        await queueDelete(item.id);
        console.warn(`Kodaal dropped queued capture ${item.id} after ${MAX_RETRIES} attempts`);
        continue;
      }
      try {
        await postCapture(item.payload);
        await queueDelete(item.id);
      } catch (error) {
        item.attempts += 1;
        item.last_error = String(error && error.message ? error.message : error);
        await queuePut(item);
        return;
      }
    }
  }

  async function tabDisabled(tabId) {
    if (!Number.isInteger(tabId)) {
      return false;
    }
    const values = await storageGet(STORAGE_KEYS.perTabDisabled);
    const disabled = Array.isArray(values[STORAGE_KEYS.perTabDisabled]) ? values[STORAGE_KEYS.perTabDisabled] : [];
    return disabled.includes(tabId);
  }

  async function setTabDisabled(tabId, disabled) {
    const values = await storageGet(STORAGE_KEYS.perTabDisabled);
    const current = Array.isArray(values[STORAGE_KEYS.perTabDisabled]) ? values[STORAGE_KEYS.perTabDisabled] : [];
    const next = disabled
      ? Array.from(new Set([...current, tabId]))
      : current.filter((id) => id !== tabId);
    await storageSet({ [STORAGE_KEYS.perTabDisabled]: next });
    return next.includes(tabId);
  }

  async function capturePolicy(sender, hostname) {
    let status = await getCachedStatus();
    try {
      status = await refreshStatus();
    } catch (error) {
      status = await getCachedStatus();
    }
    const perTabDisabled = await tabDisabled(sender && sender.tab ? sender.tab.id : null);
    return {
      paused: Boolean(status.paused),
      blocklist: normalizeBlocklist(status.blocklist),
      perTabDisabled,
      hostname
    };
  }

  async function listRecent() {
    const values = await storageGet(STORAGE_KEYS.recent);
    return Array.isArray(values[STORAGE_KEYS.recent]) ? values[STORAGE_KEYS.recent] : [];
  }

  async function popupState(tabId) {
    const settings = await getSettings();
    const [status, recent, connected, disabled] = await Promise.all([
      getCachedStatus(),
      listRecent(),
      healthz().catch(() => false),
      tabDisabled(tabId)
    ]);
    return {
      connected,
      tokenConfigured: isValidToken(settings.token),
      endpoint: settings.endpoint,
      status,
      recent,
      perTabDisabled: disabled
    };
  }

  async function handleMessage(message, sender) {
    if (!message || typeof message.type !== "string") {
      return { ok: false, error: "INVALID_MESSAGE" };
    }
    if (message.type === "capture") {
      if (!isCapturePayload(message.payload)) {
        return { ok: false, error: "INVALID_CAPTURE" };
      }
      const policy = await capturePolicy(sender, message.payload.project_hint && message.payload.project_hint.value);
      if (policy.paused || policy.perTabDisabled || matchesBlocklist(policy.blocklist, message.payload)) {
        return { ok: true, captured: false, queued: false };
      }
      try {
        await postCapture(message.payload);
        return { ok: true, captured: true, queued: false };
      } catch (error) {
        if (isNonRetryableCaptureError(error)) {
          return { ok: true, captured: false, queued: false, blocked: true, error: error.code || String(error) };
        }
        const id = await queueCapture(message.payload, error && error.message ? error.message : error);
        return { ok: true, captured: false, queued: true, queue_id: id };
      }
    }
    if (message.type === "getCapturePolicy") {
      return capturePolicy(sender, message.hostname || "");
    }
    if (message.type === "setToken") {
      if (!isValidToken(message.token)) {
        return { ok: false, error: "AUTH_TOKEN_INVALID" };
      }
      await storageSet({ [STORAGE_KEYS.token]: message.token });
      await refreshStatus();
      return { ok: true };
    }
    if (message.type === "setEndpoint") {
      const endpoint = normalizeEndpoint(message.endpoint);
      await storageSet({ [STORAGE_KEYS.endpoint]: endpoint });
      return { ok: true, endpoint };
    }
    if (message.type === "pause") {
      await apiFetch("/api/capture/pause", { method: "POST", body: "{}" });
      return { ok: true, status: await refreshStatus() };
    }
    if (message.type === "resume") {
      await apiFetch("/api/capture/resume", { method: "POST", body: "{}" });
      return { ok: true, status: await refreshStatus() };
    }
    if (message.type === "toggleTab") {
      return { ok: true, perTabDisabled: await setTabDisabled(message.tabId, Boolean(message.disabled)) };
    }
    if (message.type === "getPopupState") {
      return { ok: true, state: await popupState(message.tabId) };
    }
    if (message.type === "drainQueue") {
      await drainQueue();
      return { ok: true };
    }
    return { ok: false, error: "UNKNOWN_MESSAGE" };
  }

  function isNonRetryableCaptureError(error) {
    return Boolean(
      error &&
        (error.status === 403 ||
          error.status === 409 ||
          error.code === "FORBIDDEN" ||
          error.code === "CAPTURE_PAUSED")
    );
  }

  function normalizeBlocklist(blocklist) {
    return {
      domains: Array.isArray(blocklist && blocklist.domains) ? blocklist.domains : [],
      paths: Array.isArray(blocklist && blocklist.paths) ? blocklist.paths : [],
      source_apps: Array.isArray(blocklist && blocklist.source_apps) ? blocklist.source_apps : []
    };
  }

  function matchesBlocklist(blocklist, payload) {
    const normalized = normalizeBlocklist(blocklist);
    if (normalized.source_apps.includes(payload.source_app)) {
      return true;
    }
    const hint = payload.project_hint || {};
    const value = typeof hint.value === "string" ? hint.value : "";
    if (!value) {
      return false;
    }
    if (hint.type === "path" || hint.type === "cwd") {
      return normalized.paths.some((entry) => value === entry || value.includes(entry));
    }
    return normalized.domains.some((entry) => value === entry || value.endsWith(`.${entry}`));
  }

  function registerRuntime() {
    const api = extensionApi();
    if (!api || !api.runtime || !api.runtime.onMessage) {
      return;
    }
    api.runtime.onMessage.addListener((message, sender, sendResponse) => {
      handleMessage(message, sender)
        .then((response) => sendResponse(response))
        .catch((error) => sendResponse({ ok: false, error: String(error && error.message ? error.message : error) }));
      return true;
    });
    root.setInterval(() => refreshStatus().catch(() => {}), 30000);
    root.setInterval(() => drainQueue().catch(() => {}), 30000);
    if (root.addEventListener) {
      root.addEventListener("online", () => drainQueue().catch(() => {}));
    }
  }

  const exported = {
    DEFAULT_ENDPOINT,
    NATIVE_HOST,
    STORAGE_KEYS,
    capturePolicy,
    discoverNativeToken,
    drainQueue,
    handleMessage,
    isCapturePayload,
    isValidToken,
    matchesBlocklist,
    isNonRetryableCaptureError,
    normalizeBlocklist,
    normalizeEndpoint,
    queueId
  };

  if (typeof module !== "undefined" && module.exports) {
    module.exports = exported;
  } else {
    root.KodaalBackground = exported;
    registerRuntime();
  }
})(globalThis);
