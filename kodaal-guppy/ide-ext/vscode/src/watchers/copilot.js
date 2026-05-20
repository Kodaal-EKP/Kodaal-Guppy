"use strict";

const { JsonChatWatcher } = require("./jsonChat");
const { appDataRoot } = require("./paths");

function createCopilotWatchers(options) {
  const appNames = ["Code", "Cursor", "Windsurf"];
  return appNames.map((appName) => new JsonChatWatcher({
    name: `${appName} Copilot Chat`,
    roots: [appDataRoot(appName)],
    acceptedPath: (file) => file.includes("GitHub.copilot-chat"),
    sourceApp: `${appName.toLowerCase()}-copilot-chat`,
    client: options.client,
    projectHint: options.projectHint,
    output: options.output
  }));
}

module.exports = {
  createCopilotWatchers
};
