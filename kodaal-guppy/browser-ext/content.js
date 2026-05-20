(function kodaalContent(root) {
  "use strict";

  const MAX_PROMPT_BYTES = 1024 * 1024;
  const DUPLICATE_WINDOW_MS = 2000;
  const MAX_TITLE_CHARS = 200;
  const state = {
    site: null,
    boundInputs: new WeakSet(),
    globalListenersBound: false,
    lastCapture: new Map(),
    initialized: false
  };

  function extensionApi() {
    return root.browser || root.chrome || null;
  }

  function globToRegex(pattern) {
    const escaped = pattern
      .replace(/[.+?^${}()|[\]\\]/g, "\\$&")
      .replace(/\*/g, ".*");
    return new RegExp(`^${escaped}$`);
  }

  function matchesUrl(pattern, href) {
    return globToRegex(pattern).test(href);
  }

  function selectSiteConfig(sites, href) {
    return sites.find((site) => site.matches.some((pattern) => matchesUrl(pattern, href))) || null;
  }

  function readInputText(element) {
    if (!element) {
      return "";
    }
    if (typeof element.value === "string") {
      return normalizePromptText(element.value);
    }
    return normalizePromptText(element.textContent || "");
  }

  function normalizePromptText(value) {
    const normalized = String(value || "").replace(/\r\n/g, "\n").replace(/\n+$/g, "");
    if (normalized.trim().length === 0) {
      return "";
    }
    const byteLength = new TextEncoder().encode(normalized).length;
    if (byteLength > MAX_PROMPT_BYTES) {
      return "";
    }
    return normalized;
  }

  function findPromptInput(doc, site) {
    for (const selector of site.input_selectors) {
      const element = doc.querySelector(selector);
      if (element) {
        return element;
      }
    }
    return null;
  }

  function extractPathSegmentAfter(prefix, href) {
    const url = new URL(href);
    const index = url.pathname.indexOf(prefix);
    if (index < 0) {
      return null;
    }
    const start = index + prefix.length;
    const segment = url.pathname.slice(start).split("/")[0];
    return segment ? decodeURIComponent(segment) : null;
  }

  function capTitle(value) {
    const text = String(value || "").trim();
    return text.length > MAX_TITLE_CHARS ? text.slice(0, MAX_TITLE_CHARS) : text || null;
  }

  function extractByRule(rule, doc, href) {
    if (!rule) {
      return null;
    }
    if (rule.startsWith("url_path_segment_after:")) {
      return extractPathSegmentAfter(rule.slice("url_path_segment_after:".length), href);
    }
    if (rule === "selector:title") {
      return capTitle(doc.title);
    }
    if (rule.startsWith("selector:")) {
      const element = doc.querySelector(rule.slice("selector:".length));
      return element ? capTitle(element.textContent) : null;
    }
    if (rule.startsWith("attribute:")) {
      const [, selector, attr] = rule.split(":");
      if (!selector || !attr) {
        return null;
      }
      const element = doc.querySelector(selector);
      return element ? capTitle(element.getAttribute(attr)) : null;
    }
    return null;
  }

  function buildPayload(text, site, doc, locationRef, method) {
    return {
      text,
      source: "browser",
      source_app: site.name,
      project_hint: {
        type: "domain",
        value: locationRef.hostname
      },
      conversation_id: extractByRule(site.conversation_id_from, doc, locationRef.href),
      conversation_title: extractByRule(site.conversation_title_from, doc, locationRef.href),
      metadata: {
        url: locationRef.href,
        host: locationRef.hostname,
        path: locationRef.pathname,
        capture_method: method
      }
    };
  }

  function triggerMatchesKey(event, trigger) {
    const parts = trigger.split(":");
    if (parts[0] !== "keydown") {
      return false;
    }
    if (parts[1] !== event.key) {
      return false;
    }
    if (parts[2] === "no-shift" && event.shiftKey) {
      return false;
    }
    if (parts[2] === "no-modifier") {
      return !event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
    }
    return true;
  }

  function isBlockedByPolicy(policy, hostname) {
    if (!policy) {
      return false;
    }
    if (policy.paused || policy.perTabDisabled) {
      return true;
    }
    const blocklist = policy.blocklist || {};
    return Array.isArray(blocklist.domains) && blocklist.domains.includes(hostname);
  }

  function isDuplicate(text, href, now) {
    const key = `${href}\u0000${text}`;
    const last = state.lastCapture.get(key) || 0;
    state.lastCapture.set(key, now);
    return now - last < DUPLICATE_WINDOW_MS;
  }

  async function sendRuntimeMessage(message) {
    const api = extensionApi();
    if (!api || !api.runtime || !api.runtime.sendMessage) {
      return null;
    }
    const result = api.runtime.sendMessage(message);
    return result && typeof result.then === "function" ? result : null;
  }

  async function submitPrompt(doc, locationRef, method) {
    if (!state.site) {
      return;
    }
    const input = findPromptInput(doc, state.site);
    const text = readInputText(input);
    if (!text || isDuplicate(text, locationRef.href, Date.now())) {
      return;
    }
    const policy = await sendRuntimeMessage({ type: "getCapturePolicy", hostname: locationRef.hostname });
    if (isBlockedByPolicy(policy, locationRef.hostname)) {
      return;
    }
    const payload = buildPayload(text, state.site, doc, locationRef, method);
    sendRuntimeMessage({ type: "capture", payload });
  }

  function bindInput(input, doc, locationRef) {
    if (!input || state.boundInputs.has(input)) {
      return;
    }
    state.boundInputs.add(input);
    const keyTriggers = state.site.submit_triggers.filter((trigger) => trigger.startsWith("keydown:"));
    input.addEventListener(
      "keydown",
      (event) => {
        if (keyTriggers.some((trigger) => triggerMatchesKey(event, trigger))) {
          submitPrompt(doc, locationRef, "keydown");
        }
      },
      true
    );
  }

  function bindGlobalListeners(doc, locationRef) {
    if (state.globalListenersBound) {
      return;
    }
    state.globalListenersBound = true;
    doc.addEventListener(
      "click",
      (event) => {
        if (!state.site) {
          return;
        }
        const clickTriggers = state.site.submit_triggers.filter((trigger) => trigger.startsWith("click:"));
        const matched = clickTriggers.some((trigger) => {
          const selector = trigger.slice("click:".length);
          return event.target && event.target.closest && event.target.closest(selector);
        });
        if (matched) {
          submitPrompt(doc, locationRef, "click");
        }
      },
      true
    );
    doc.addEventListener(
      "submit",
      () => {
        submitPrompt(doc, locationRef, "submit");
      },
      true
    );
  }

  function bindHistoryEvents(callback) {
    if (root.__kodaalHistoryBound) {
      return;
    }
    root.__kodaalHistoryBound = true;
    for (const method of ["pushState", "replaceState"]) {
      const original = root.history[method];
      root.history[method] = function patchedHistory() {
        const result = original.apply(this, arguments);
        root.setTimeout(callback, 50);
        return result;
      };
    }
    root.addEventListener("popstate", () => root.setTimeout(callback, 50));
  }

  async function loadSiteConfig(doc, locationRef) {
    const api = extensionApi();
    if (!api || !api.runtime || !api.runtime.getURL || typeof root.fetch !== "function") {
      return null;
    }
    const response = await root.fetch(api.runtime.getURL("sites.json"));
    const sites = await response.json();
    return selectSiteConfig(sites, locationRef.href);
  }

  async function initialize(doc, locationRef) {
    state.site = await loadSiteConfig(doc, locationRef);
    if (!state.site) {
      return;
    }
    bindInput(findPromptInput(doc, state.site), doc, locationRef);
    bindGlobalListeners(doc, locationRef);
    bindHistoryEvents(() => initialize(doc, locationRef));
    const observer = new MutationObserver(() => {
      bindInput(findPromptInput(doc, state.site), doc, locationRef);
    });
    observer.observe(doc.documentElement, { childList: true, subtree: true });
    state.initialized = true;
  }

  const exported = {
    buildPayload,
    capTitle,
    extractByRule,
    extractPathSegmentAfter,
    findPromptInput,
    globToRegex,
    isBlockedByPolicy,
    matchesUrl,
    normalizePromptText,
    readInputText,
    selectSiteConfig,
    triggerMatchesKey
  };

  if (typeof module !== "undefined" && module.exports) {
    module.exports = exported;
  } else {
    root.KodaalContent = exported;
    if (root.document && root.location && extensionApi()) {
      initialize(root.document, root.location).catch(() => {});
    }
  }
})(globalThis);
