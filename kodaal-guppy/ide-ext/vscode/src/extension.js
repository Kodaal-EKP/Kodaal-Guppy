"use strict";

const { KodaalClient } = require("./client");
const { saveCurrentPrompt } = require("./manualCommand");
const { resolveProjectHint } = require("./projectResolver");
const { createSuggestionController } = require("./suggestions");
const { createCopilotWatchers } = require("./watchers/copilot");
const { createCursorChatWatcher } = require("./watchers/cursorChat");

function endpointFromConfig(vscode) {
  return vscode.workspace.getConfiguration("kodaal").get("endpoint", "http://127.0.0.1:7878");
}

function activate(vscodeContext) {
  const vscode = require("vscode");
  const output = vscode.window.createOutputChannel("Kodaal Guppy");
  const client = new KodaalClient({ endpoint: endpointFromConfig(vscode) });
  const projectHint = () => resolveProjectHint(vscode);
  const suggestions = createSuggestionController({ vscode, context: vscodeContext, client, output });
  const watchers = [];

  vscodeContext.subscriptions.push(output);
  vscodeContext.subscriptions.push(vscode.commands.registerCommand("kodaal.saveCurrentPrompt", () => (
    saveCurrentPrompt(vscode, client, projectHint).catch((error) => {
      output.appendLine(`Kodaal save failed: ${error.message}`);
      vscode.window.showErrorMessage("Kodaal could not save the prompt. Check that the local app is running.");
    })
  )));
  vscodeContext.subscriptions.push(vscode.commands.registerCommand("kodaal.pauseCapture", () => (
    client.pause().then(() => vscode.window.showInformationMessage("Kodaal capture paused.")).catch((error) => {
      output.appendLine(`Kodaal pause failed: ${error.message}`);
      vscode.window.showErrorMessage("Kodaal could not pause capture.");
    })
  )));
  vscodeContext.subscriptions.push(vscode.commands.registerCommand("kodaal.resumeCapture", () => (
    client.resume().then(() => vscode.window.showInformationMessage("Kodaal capture resumed.")).catch((error) => {
      output.appendLine(`Kodaal resume failed: ${error.message}`);
      vscode.window.showErrorMessage("Kodaal could not resume capture.");
    })
  )));
  vscodeContext.subscriptions.push(vscode.commands.registerCommand("kodaal.openWorkspace", () => (
    vscode.env.openExternal(vscode.Uri.parse("http://127.0.0.1:7878/ui"))
  )));
  vscodeContext.subscriptions.push(vscode.commands.registerCommand("kodaal.suggestSimilarPrompt", () => (
    suggestions.showForActiveEditor().catch((error) => {
      output.appendLine(`Kodaal suggestions failed: ${error.message}`);
      vscode.window.showErrorMessage("Kodaal could not load prompt suggestions.");
    })
  )));
  suggestions.start();
  vscodeContext.subscriptions.push({ dispose: () => suggestions.dispose() });

  const watcherEnabled = vscode.workspace.getConfiguration("kodaal").get("capture.ideWatchers", true);
  if (watcherEnabled) {
    watchers.push(...createCopilotWatchers({ client, projectHint, output }));
    watchers.push(createCursorChatWatcher({ client, projectHint, output }));
    for (const watcher of watchers) {
      watcher.start();
      vscodeContext.subscriptions.push({ dispose: () => watcher.stop() });
    }
    output.appendLine(`Kodaal started ${watchers.length} local IDE capture watchers.`);
  }
  const queueTimer = setInterval(() => {
    client
      .drainQueue()
      .then((result) => {
        if (result.replayed) output.appendLine(`Kodaal replayed ${result.replayed} queued prompts.`);
      })
      .catch(() => {});
  }, 30000);
  vscodeContext.subscriptions.push({ dispose: () => clearInterval(queueTimer) });
}

function deactivate() {}

module.exports = {
  activate,
  deactivate
};
