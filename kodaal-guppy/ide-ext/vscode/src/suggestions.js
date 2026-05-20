"use strict";

const SUGGESTION_DEBOUNCE_MS = 450;

function activeDraftText(vscode) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return "";
  const selection = editor.document.getText(editor.selection);
  if (selection && selection.trim()) return selection;
  const line = editor.document.lineAt(editor.selection.active.line).text;
  return line || "";
}

function suggestionQuickPickItems(response) {
  const items = Array.isArray(response && response.items) ? response.items : [];
  return items.map((prompt) => ({
    label: truncate(prompt.text || "", 80),
    description: `${prompt.source || "ide"}/${prompt.source_app || "unknown"} score ${Number(prompt.score || 0).toFixed(3)}`,
    detail: `Matched: ${Array.isArray(prompt.matched_terms) ? prompt.matched_terms.join(", ") : "none"}`,
    prompt
  }));
}

function truncate(value, max) {
  const text = String(value || "").replace(/\s+/g, " ").trim();
  return text.length > max ? `${text.slice(0, Math.max(0, max - 3))}...` : text;
}

function suggestionsEnabled(vscode) {
  return vscode.workspace.getConfiguration("kodaal").get("suggestions.ideTypingHints", true);
}

function createSuggestionController({ vscode, context, client, output }) {
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 90);
  status.command = "kodaal.suggestSimilarPrompt";
  status.name = "Kodaal prompt suggestions";
  let timer = null;
  let lastDraft = "";

  async function fetchForDraft(draft, limit = 3) {
    return client.suggestPrompts({
      q: draft,
      surface: "ide",
      limit
    });
  }

  async function refreshStatus() {
    if (!suggestionsEnabled(vscode)) {
      status.hide();
      return;
    }
    const draft = activeDraftText(vscode).trim();
    if (!draft || draft === lastDraft) return;
    lastDraft = draft;
    try {
      const response = await fetchForDraft(draft, 3);
      const count = Number(response && response.similar_count ? response.similar_count : 0);
      if (response && response.enabled && count > 0) {
        status.text = `$(lightbulb) ${count} similar`;
        status.tooltip = "Kodaal found similar local prompts";
        status.show();
      } else {
        status.hide();
      }
    } catch (error) {
      status.hide();
      if (output) output.appendLine(`Kodaal suggestion refresh failed: ${error.message}`);
    }
  }

  function scheduleRefresh() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      refreshStatus().catch(() => {});
    }, SUGGESTION_DEBOUNCE_MS);
  }

  async function showForActiveEditor() {
    const draft = activeDraftText(vscode).trim();
    if (!draft) {
      vscode.window.showWarningMessage("Kodaal: there is no draft text to compare.");
      return null;
    }
    let response;
    try {
      response = await fetchForDraft(draft, 5);
    } catch (error) {
      if (output) output.appendLine(`Kodaal suggestions failed: ${error.message}`);
      vscode.window.showErrorMessage("Kodaal could not load prompt suggestions.");
      return null;
    }
    if (!response.enabled) {
      vscode.window.showInformationMessage("Kodaal smart suggestions are disabled in settings.");
      return null;
    }
    const items = suggestionQuickPickItems(response);
    if (!items.length) {
      vscode.window.showInformationMessage("Kodaal found no similar local prompts.");
      return null;
    }
    const picked = await vscode.window.showQuickPick(items);
    if (!picked) return null;
    await vscode.env.clipboard.writeText(picked.prompt.text);
    await client.reusePrompt(picked.prompt.id).catch(() => {});
    vscode.window.showInformationMessage("Kodaal copied the prompt to the clipboard.");
    return picked.prompt;
  }

  function start() {
    context.subscriptions.push(status);
    context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(scheduleRefresh));
    context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(scheduleRefresh));
    scheduleRefresh();
  }

  function dispose() {
    if (timer) clearTimeout(timer);
    status.dispose();
  }

  return {
    start,
    dispose,
    showForActiveEditor,
    refreshStatus
  };
}

module.exports = {
  activeDraftText,
  createSuggestionController,
  suggestionQuickPickItems,
  suggestionsEnabled,
  truncate
};
