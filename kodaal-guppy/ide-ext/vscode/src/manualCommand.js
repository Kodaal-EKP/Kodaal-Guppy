"use strict";

function selectedOrDocumentText(vscode) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return "";
  }
  const selectionText = editor.document.getText(editor.selection);
  if (selectionText && selectionText.trim().length > 0) {
    return selectionText;
  }
  return editor.document.getText();
}

async function saveCurrentPrompt(vscode, client, resolveProjectHint) {
  const text = selectedOrDocumentText(vscode);
  if (!text || text.trim().length === 0) {
    vscode.window.showWarningMessage("Kodaal: there is no prompt text to save.");
    return null;
  }
  const confirmation = await vscode.window.showInformationMessage("Save current text to Kodaal?", "Save prompt", "Cancel");
  if (confirmation !== "Save prompt") {
    return null;
  }
  const payload = {
    text,
    source: "ide",
    source_app: "vscode-manual",
    project_hint: resolveProjectHint(),
    metadata: {
      editor: vscode.env.appName
    }
  };
  const result = await client.logPrompt(payload);
  vscode.window.showInformationMessage("Kodaal: prompt saved.");
  return result;
}

module.exports = {
  saveCurrentPrompt,
  selectedOrDocumentText
};
