import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const require = createRequire(import.meta.url);
const { DEFAULT_ENDPOINT, KodaalClient, isValidToken, normalizeEndpoint } = require("../src/client.js");
const { defaultKodaalHome, tokenPath } = require("../src/token.js");

test("normalizeEndpoint only allows the local Kodaal core endpoint", () => {
  assert.equal(normalizeEndpoint("http://127.0.0.1:7878"), DEFAULT_ENDPOINT);
  assert.throws(() => normalizeEndpoint("http://localhost:7878"));
  assert.throws(() => normalizeEndpoint("https://example.com"));
});

test("token validation mirrors core token policy", () => {
  assert.equal(isValidToken("a".repeat(64)), true);
  assert.equal(isValidToken("A".repeat(64)), false);
  assert.equal(isValidToken("a".repeat(63)), false);
  assert.equal(isValidToken("g".repeat(64)), false);
});

test("token path follows Kodaal core path precedence", () => {
  assert.equal(defaultKodaalHome({ KODAAL_HOME: "C:\\KodaalHome" }, "win32"), "C:\\KodaalHome");
  assert.equal(defaultKodaalHome({ APPDATA: "C:\\Users\\testuser\\AppData\\Roaming" }, "win32"), "C:\\Users\\testuser\\AppData\\Roaming\\Kodaal");
  assert.equal(tokenPath({ HOME: "/home/testuser" }, "linux"), "/home/testuser/.kodaal/token");
});

test("IDE client queues prompts while core is offline and replays them later", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kodaal-ide-queue-"));
  const queueFile = join(dir, "queue.json");
  let fail = true;
  const client = new KodaalClient({
    queueFile,
    tokenReader: async () => "a".repeat(64),
    requestJson: async () => {
      if (fail) throw new Error("ECONNREFUSED");
      return { id: "p1" };
    }
  });

  assert.deepEqual(await client.logPrompt({ text: "queued prompt", source: "ide", source_app: "codex-vscode" }), { queued: true });
  assert.equal(JSON.parse(await readFile(queueFile, "utf8")).length, 1);
  fail = false;
  assert.deepEqual(await client.drainQueue(), { replayed: 1, remaining: 0 });
  assert.equal(JSON.parse(await readFile(queueFile, "utf8")).length, 0);
  await rm(dir, { recursive: true, force: true });
});

test("IDE client calls local suggestion and reuse endpoints without queueing drafts", async () => {
  const calls = [];
  const client = new KodaalClient({
    tokenReader: async () => "a".repeat(64),
    requestJson: async (_endpoint, _token, method, apiPath) => {
      calls.push({ method, apiPath });
      return method === "GET" ? { enabled: true, items: [] } : { use_count: 2 };
    }
  });

  assert.deepEqual(
    await client.suggestPrompts({ q: "refactor rust sqlx", surface: "ide", limit: 3 }),
    { enabled: true, items: [] }
  );
  await client.reusePrompt("prompt/1");

  assert.equal(calls[0].method, "GET");
  assert.equal(
    calls[0].apiPath,
    "/api/prompts/suggestions?q=refactor+rust+sqlx&surface=ide&limit=3"
  );
  assert.equal(calls[1].method, "POST");
  assert.equal(calls[1].apiPath, "/api/prompts/prompt%2F1/reuse");
});
