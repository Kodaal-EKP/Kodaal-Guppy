import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";
import sites from "../sites.json" with { type: "json" };

const require = createRequire(import.meta.url);
const content = require("../content.js");

test("selectSiteConfig matches supported hosts only", () => {
  assert.equal(content.selectSiteConfig(sites, "https://claude.ai/chat/abc").name, "claude.ai");
  assert.equal(content.selectSiteConfig(sites, "https://chatgpt.com/c/abc").name, "chatgpt.com");
  assert.equal(content.selectSiteConfig(sites, "https://example.com/"), null);
});

test("extractByRule reads conversation id from configured path segment", () => {
  const doc = { title: "React refactor", querySelector: () => null };
  assert.equal(content.extractByRule("url_path_segment_after:/chat/", doc, "https://claude.ai/chat/abc-123"), "abc-123");
  assert.equal(content.extractByRule("url_path_segment_after:/chat/", doc, "https://claude.ai/new"), null);
});

test("normalizePromptText preserves leading whitespace and strips trailing newlines", () => {
  assert.equal(content.normalizePromptText("  keep indentation\n\n"), "  keep indentation");
  assert.equal(content.normalizePromptText(" \n\t"), "");
});

test("buildPayload creates the API capture contract", () => {
  const site = content.selectSiteConfig(sites, "https://chatgpt.com/c/thread-1");
  const doc = { title: "Thread title", querySelector: () => null };
  const locationRef = new URL("https://chatgpt.com/c/thread-1");
  const payload = content.buildPayload("Refactor this component", site, doc, locationRef, "keydown");
  assert.equal(payload.source, "browser");
  assert.equal(payload.source_app, "chatgpt.com");
  assert.deepEqual(payload.project_hint, { type: "domain", value: "chatgpt.com" });
  assert.equal(payload.conversation_id, "thread-1");
  assert.equal(payload.metadata.capture_method, "keydown");
});
