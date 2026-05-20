import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const { extractMessagesFromJson, normalizePromptText, walkJsonFiles } = require("../src/watchers/jsonChat.js");
const { appDataRoot } = require("../src/watchers/paths.js");

test("extractMessagesFromJson returns only user-authored chat messages", () => {
  const messages = extractMessagesFromJson({
    conversation_id: "root",
    messages: [
      { role: "system", content: "Rules" },
      { role: "user", content: [{ type: "text", text: "Build the parser" }] },
      { role: "user", content: [{ type: "tool_result", content: "command output" }] },
      { role: "user", content: "# AGENTS.md instructions for C:/work\n\n<INSTRUCTIONS>ignore</INSTRUCTIONS>" },
      { role: "user", content: "sidechain output", isSidechain: true },
      { role: "assistant", content: "Done" },
      { author: "human", message: "Explain this error" }
    ]
  });
  assert.deepEqual(messages.map((message) => message.text), ["Build the parser", "Explain this error"]);
});

test("normalizePromptText extracts wrapper requests and command messages", () => {
  assert.equal(normalizePromptText("Tool loaded."), "");
  assert.equal(normalizePromptText("<<autonomous-loop-dynamic>>"), "");
  assert.equal(
    normalizePromptText("This session is being continued from a previous conversation that ran out of context.\n\nSummary:\nignore"),
    ""
  );
  assert.equal(
    normalizePromptText("The following is the Codex agent history added since your last approval assessment. Continue assessing the request action."),
    ""
  );
  assert.equal(
    normalizePromptText("<command-message>loop</command-message>\n<command-name>/loop</command-name>\n<command-args>ship it</command-args>"),
    "/loop ship it"
  );
  assert.equal(
    normalizePromptText("# Files mentioned by the user:\n\n## a.md: C:/a.md\n\n## My request for Codex:\nfix attribution"),
    "fix attribution"
  );
  assert.equal(
    normalizePromptText("# /loop — schedule a recurring prompt\n\ninternal command docs\n\n## Input\n\nbuild attribution"),
    "/loop build attribution"
  );
});

test("walkJsonFiles filters accepted paths", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "kodaal-ide-"));
  fs.mkdirSync(path.join(dir, "GitHub.copilot-chat"), { recursive: true });
  fs.writeFileSync(path.join(dir, "GitHub.copilot-chat", "chat.json"), "{}");
  fs.writeFileSync(path.join(dir, "other.json"), "{}");
  const files = walkJsonFiles(dir, (file) => file.includes("copilot-chat"));
  assert.equal(files.length, 1);
  assert.equal(path.basename(files[0]), "chat.json");
  fs.rmSync(dir, { recursive: true, force: true });
});

test("appDataRoot maps Windows VS Code storage path", () => {
  const root = appDataRoot("Code", { APPDATA: "C:\\Users\\testuser\\AppData\\Roaming" }, "win32");
  assert.equal(root, "C:\\Users\\testuser\\AppData\\Roaming\\Code\\User\\workspaceStorage");
});
