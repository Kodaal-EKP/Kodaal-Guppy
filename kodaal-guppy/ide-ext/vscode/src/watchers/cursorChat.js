"use strict";

const { JsonChatWatcher } = require("./jsonChat");
const { appDataRoot } = require("./paths");

function createCursorChatWatcher(options) {
  return new JsonChatWatcher({
    name: "Cursor Chat",
    roots: [appDataRoot("Cursor")],
    acceptedPath: (file) => {
      const lowered = file.toLowerCase();
      return lowered.includes("cursor") && lowered.includes("chat");
    },
    sourceApp: "cursor-chat",
    client: options.client,
    projectHint: options.projectHint,
    output: options.output
  });
}

module.exports = {
  createCursorChatWatcher
};
