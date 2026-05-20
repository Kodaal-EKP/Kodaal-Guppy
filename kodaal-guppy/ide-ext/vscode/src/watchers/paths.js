"use strict";

const os = require("node:os");
const path = require("node:path");

function appDataRoot(appName, env = process.env, platform = process.platform) {
  if (platform === "win32") {
    const appdata = env.APPDATA || path.join(os.homedir(), "AppData", "Roaming");
    return path.join(appdata, appName, "User", "workspaceStorage");
  }
  if (platform === "darwin") {
    return path.join(os.homedir(), "Library", "Application Support", appName, "User", "workspaceStorage");
  }
  const config = env.XDG_CONFIG_HOME || path.join(os.homedir(), ".config");
  return path.join(config, appName, "User", "workspaceStorage");
}

module.exports = {
  appDataRoot
};
