import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const {
  activeDraftText,
  createSuggestionController,
  suggestionQuickPickItems,
  suggestionsEnabled,
  truncate
} = require("../src/suggestions.js");

test("activeDraftText prefers selection and otherwise uses current line", () => {
  const vscode = {
    window: {
      activeTextEditor: {
        selection: { active: { line: 2 } },
        document: {
          getText: (selection) => (selection ? "selected prompt" : "whole document"),
          lineAt: (line) => ({ text: `line ${line} prompt` })
        }
      }
    }
  };
  assert.equal(activeDraftText(vscode), "selected prompt");

  vscode.window.activeTextEditor.document.getText = () => "";
  assert.equal(activeDraftText(vscode), "line 2 prompt");
});

test("suggestionQuickPickItems preserves prompt payloads", () => {
  const items = suggestionQuickPickItems({
    items: [
      {
        id: "p1",
        text: "refactor rust sqlx transaction handling in service",
        source: "ide",
        source_app: "codex-vscode",
        score: 0.75,
        matched_terms: ["rust", "sqlx"]
      }
    ]
  });

  assert.equal(items.length, 1);
  assert.equal(items[0].description, "ide/codex-vscode score 0.750");
  assert.equal(items[0].detail, "Matched: rust, sqlx");
  assert.equal(items[0].prompt.id, "p1");
});

test("suggestionsEnabled defaults on and respects IDE override", () => {
  assert.equal(
    suggestionsEnabled({
      workspace: { getConfiguration: () => ({ get: (_key, fallback) => fallback }) }
    }),
    true
  );
  assert.equal(
    suggestionsEnabled({
      workspace: { getConfiguration: () => ({ get: () => false }) }
    }),
    false
  );
});

test("truncate keeps quick pick labels bounded", () => {
  assert.equal(truncate("a ".repeat(80), 20), "a a a a a a a a a...");
});

test("suggestion controller shows status hint for matching active editor draft", async () => {
  const status = {
    showCalled: false,
    hideCalled: false,
    show() {
      this.showCalled = true;
    },
    hide() {
      this.hideCalled = true;
    },
    dispose() {}
  };
  const vscode = {
    StatusBarAlignment: { Right: 1 },
    workspace: {
      getConfiguration: () => ({ get: () => true }),
      onDidChangeTextDocument: () => ({ dispose() {} })
    },
    window: {
      createStatusBarItem: () => status,
      onDidChangeActiveTextEditor: () => ({ dispose() {} }),
      activeTextEditor: {
        selection: { active: { line: 0 } },
        document: {
          getText: () => "",
          lineAt: () => ({ text: "refactor rust sqlx transaction handling" })
        }
      }
    }
  };
  const calls = [];
  const controller = createSuggestionController({
    vscode,
    context: { subscriptions: [] },
    client: {
      suggestPrompts: async (query) => {
        calls.push(query);
        return { enabled: true, similar_count: 2, items: [] };
      }
    }
  });

  await controller.refreshStatus();

  assert.equal(calls.length, 1);
  assert.equal(calls[0].surface, "ide");
  assert.equal(status.text, "$(lightbulb) 2 similar");
  assert.equal(status.tooltip, "Kodaal found similar local prompts");
  assert.equal(status.showCalled, true);
  assert.equal(status.hideCalled, false);
});

test("suggestion controller quick pick copies and marks reuse", async () => {
  const prompt = {
    id: "p1",
    text: "refactor rust sqlx transaction handling in service",
    source: "ide",
    source_app: "codex-vscode",
    score: 0.8,
    matched_terms: ["rust", "sqlx"]
  };
  const status = { show() {}, hide() {}, dispose() {} };
  let copied = "";
  let reused = "";
  let pickedItems = null;
  const messages = [];
  const vscode = {
    StatusBarAlignment: { Right: 1 },
    env: {
      clipboard: {
        writeText: async (value) => {
          copied = value;
        }
      }
    },
    workspace: {
      getConfiguration: () => ({ get: () => true }),
      onDidChangeTextDocument: () => ({ dispose() {} })
    },
    window: {
      createStatusBarItem: () => status,
      onDidChangeActiveTextEditor: () => ({ dispose() {} }),
      showQuickPick: async (items) => {
        pickedItems = items;
        return items[0];
      },
      showInformationMessage: (message) => messages.push(message),
      showWarningMessage: (message) => messages.push(message),
      showErrorMessage: (message) => messages.push(message),
      activeTextEditor: {
        selection: { active: { line: 0 } },
        document: {
          getText: () => "refactor rust sqlx transaction handling",
          lineAt: () => ({ text: "" })
        }
      }
    }
  };
  const controller = createSuggestionController({
    vscode,
    context: { subscriptions: [] },
    client: {
      suggestPrompts: async () => ({ enabled: true, similar_count: 1, items: [prompt] }),
      reusePrompt: async (id) => {
        reused = id;
      }
    }
  });

  const result = await controller.showForActiveEditor();

  assert.equal(result.id, "p1");
  assert.equal(pickedItems.length, 1);
  assert.equal(copied, prompt.text);
  assert.equal(reused, "p1");
  assert.deepEqual(messages, ["Kodaal copied the prompt to the clipboard."]);
});
