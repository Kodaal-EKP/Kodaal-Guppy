"use strict";

function resolveProjectHint(vscode) {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return undefined;
  }
  const folder = folders[0];
  return {
    type: "path",
    value: folder.uri.fsPath
  };
}

module.exports = {
  resolveProjectHint
};
