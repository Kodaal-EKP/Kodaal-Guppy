import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const background = require("../background.js");
const backgroundPath = require.resolve("../background.js");

function installChrome(stored = {}) {
  const badges = [];
  globalThis.chrome = {
    action: {
      setBadgeText(value) {
        badges.push(value);
      }
    },
    storage: {
      local: {
        get(keys, callback) {
          const output = {};
          const names = Array.isArray(keys) ? keys : [keys];
          for (const key of names) output[key] = stored[key];
          callback(output);
        },
        set(values, callback) {
          Object.assign(stored, values);
          callback();
        },
        remove(keys, callback) {
          const names = Array.isArray(keys) ? keys : [keys];
          for (const key of names) delete stored[key];
          callback();
        }
      }
    }
  };
  return { badges, stored };
}

function response(status, body) {
  return {
    ok: status >= 200 && status < 300,
    status,
    async json() {
      return body;
    }
  };
}

function installIndexedDb() {
  const records = new Map();
  function requestResult(result) {
    const request = { result, error: null };
    queueMicrotask(() => request.onsuccess && request.onsuccess());
    return request;
  }
  globalThis.indexedDB = {
    open() {
      const request = { result: null, error: null };
      const db = {
        objectStoreNames: { contains: () => true },
        createObjectStore() {},
        transaction() {
          return {
            objectStore() {
              return {
                put(item) {
                  records.set(item.id, item);
                  return requestResult(item.id);
                },
                getAll() {
                  return requestResult(Array.from(records.values()));
                },
                delete(id) {
                  records.delete(id);
                  return requestResult(undefined);
                }
              };
            }
          };
        },
        close() {}
      };
      queueMicrotask(() => {
        request.result = db;
        request.onsuccess && request.onsuccess();
      });
      return request;
    }
  };
  return records;
}

test("normalizeEndpoint only allows the local core endpoint", () => {
  assert.equal(background.normalizeEndpoint("http://127.0.0.1:7878"), background.DEFAULT_ENDPOINT);
  assert.throws(() => background.normalizeEndpoint("https://api.example.com"));
  assert.throws(() => background.normalizeEndpoint("http://127.0.0.1:9999"));
});

test("isValidToken follows the core token shape", () => {
  assert.equal(background.isValidToken("a".repeat(64)), true);
  assert.equal(background.isValidToken("A".repeat(64)), false);
  assert.equal(background.isValidToken("a".repeat(63)), false);
  assert.equal(background.isValidToken("g".repeat(64)), false);
});

test("isCapturePayload accepts only browser prompt captures", () => {
  assert.equal(
    background.isCapturePayload({
      text: "Prompt",
      source: "browser",
      source_app: "claude.ai"
    }),
    true
  );
  assert.equal(background.isCapturePayload({ text: "Prompt", source: "ide", source_app: "vscode" }), false);
});

test("structured blocklist matches domains, paths, and source apps without requiring project_hint", () => {
  const blocklist = {
    domains: ["claude.ai"],
    paths: ["C:\\work\\private"],
    source_apps: ["blocked-app"]
  };
  assert.equal(
    background.matchesBlocklist(blocklist, {
      text: "Prompt",
      source: "browser",
      source_app: "claude.ai",
      project_hint: { type: "domain", value: "claude.ai" }
    }),
    true
  );
  assert.equal(
    background.matchesBlocklist(blocklist, {
      text: "Prompt",
      source: "browser",
      source_app: "browser",
      project_hint: { type: "cwd", value: "C:\\work\\private\\repo" }
    }),
    true
  );
  assert.equal(
    background.matchesBlocklist(blocklist, {
      text: "Prompt",
      source: "browser",
      source_app: "blocked-app"
    }),
    true
  );
  assert.equal(
    background.matchesBlocklist(blocklist, {
      text: "Prompt",
      source: "browser",
      source_app: "browser"
    }),
    false
  );
});

test("pause and blocklist errors are non-retryable", () => {
  assert.equal(background.isNonRetryableCaptureError({ status: 403, code: "FORBIDDEN" }), true);
  assert.equal(background.isNonRetryableCaptureError({ status: 409, code: "CAPTURE_PAUSED" }), true);
  assert.equal(background.isNonRetryableCaptureError({ status: 500, code: "INTERNAL" }), false);
});

test("native messaging can discover and store the local token", async () => {
  const stored = {};
  globalThis.chrome = {
    runtime: {
      sendNativeMessage(host, message, callback) {
        assert.equal(host, "com.kodaal.guppy");
        assert.deepEqual(message, { type: "token" });
        callback({ token: "a".repeat(64) });
      }
    },
    storage: {
      local: {
        get(keys, callback) {
          const output = {};
          for (const key of Array.isArray(keys) ? keys : [keys]) output[key] = stored[key];
          callback(output);
        },
        set(values, callback) {
          Object.assign(stored, values);
          callback();
        },
        remove(keys, callback) {
          for (const key of Array.isArray(keys) ? keys : [keys]) delete stored[key];
          callback();
        }
      }
    }
  };
  delete require.cache[backgroundPath];
  const isolated = require("../background.js");
  assert.equal(await isolated.discoverNativeToken(), "a".repeat(64));
  assert.equal(stored.kodaal_token, "a".repeat(64));
  delete globalThis.chrome;
  delete require.cache[backgroundPath];
});

test("handleMessage captures prompts, stores recents, and reports popup state", async () => {
  const stored = { kodaal_token: "b".repeat(64) };
  installChrome(stored);
  const calls = [];
  globalThis.fetch = async (url, options) => {
    calls.push({ url, method: options.method });
    if (url.endsWith("/healthz")) return response(200, { status: "ok" });
    if (url.endsWith("/api/capture/status")) {
      return response(200, { paused: false, blocklist: { domains: [], paths: [], source_apps: [] } });
    }
    if (url.endsWith("/api/prompts")) return response(201, { id: "prompt_1" });
    throw new Error(`unexpected fetch ${url}`);
  };

  const payload = {
    text: "browser capture",
    source: "browser",
    source_app: "claude.ai",
    project_hint: { type: "domain", value: "claude.ai" }
  };
  const captured = await background.handleMessage({ type: "capture", payload }, { tab: { id: 7 } });
  assert.deepEqual(captured, { ok: true, captured: true, queued: false });
  assert.equal(stored.kodaal_recent_captures[0].text, "browser capture");

  const popup = await background.handleMessage({ type: "getPopupState", tabId: 7 }, { tab: { id: 7 } });
  assert.equal(popup.ok, true);
  assert.equal(popup.state.connected, true);
  assert.equal(popup.state.tokenConfigured, true);
  assert.equal(popup.state.recent.length, 1);
  assert.equal(calls.some((call) => call.url.endsWith("/api/prompts") && call.method === "POST"), true);

  delete globalThis.fetch;
  delete globalThis.chrome;
});

test("handleMessage queues retryable failures and drainQueue replays them", async () => {
  const stored = { kodaal_token: "c".repeat(64) };
  installChrome(stored);
  const records = installIndexedDb();
  let promptsAvailable = false;
  globalThis.fetch = async (url) => {
    if (url.endsWith("/api/capture/status")) {
      return response(200, { paused: false, blocklist: { domains: [], paths: [], source_apps: [] } });
    }
    if (url.endsWith("/api/prompts")) {
      return promptsAvailable
        ? response(201, { id: "prompt_2" })
        : response(503, { error: { code: "SERVICE_UNAVAILABLE", message: "offline" } });
    }
    throw new Error(`unexpected fetch ${url}`);
  };

  const queued = await background.handleMessage(
    {
      type: "capture",
      payload: { text: "queued browser capture", source: "browser", source_app: "chatgpt.com" }
    },
    { tab: { id: 8 } }
  );
  assert.equal(queued.ok, true);
  assert.equal(queued.queued, true);
  assert.equal(records.size, 1);

  promptsAvailable = true;
  await background.handleMessage({ type: "drainQueue" }, {});
  assert.equal(records.size, 0);
  assert.equal(stored.kodaal_recent_captures[0].text, "queued browser capture");

  delete globalThis.fetch;
  delete globalThis.indexedDB;
  delete globalThis.chrome;
});

test("handleMessage applies pause, resume, endpoint, and per-tab controls", async () => {
  const stored = { kodaal_token: "d".repeat(64) };
  const { badges } = installChrome(stored);
  let paused = false;
  globalThis.fetch = async (url, options) => {
    if (url.endsWith("/api/capture/pause")) {
      paused = true;
      return response(200, { paused });
    }
    if (url.endsWith("/api/capture/resume")) {
      paused = false;
      return response(200, { paused });
    }
    if (url.endsWith("/api/capture/status")) {
      return response(200, { paused, blocklist: { domains: [], paths: [], source_apps: [] } });
    }
    throw new Error(`unexpected fetch ${url} ${options.method}`);
  };

  assert.deepEqual(await background.handleMessage({ type: "setEndpoint", endpoint: "http://127.0.0.1:7878" }, {}), {
    ok: true,
    endpoint: "http://127.0.0.1:7878"
  });
  assert.equal((await background.handleMessage({ type: "pause" }, {})).status.paused, true);
  assert.equal(badges.at(-1).text, "II");
  assert.equal((await background.handleMessage({ type: "resume" }, {})).status.paused, false);
  assert.equal(badges.at(-1).text, "");
  assert.deepEqual(await background.handleMessage({ type: "toggleTab", tabId: 9, disabled: true }, {}), {
    ok: true,
    perTabDisabled: true
  });
  assert.deepEqual(await background.handleMessage({ type: "toggleTab", tabId: 9, disabled: false }, {}), {
    ok: true,
    perTabDisabled: false
  });

  delete globalThis.fetch;
  delete globalThis.chrome;
});
