"use strict";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

function pathApi(platform) {
  return platform === "win32" ? path.win32 : path.posix;
}

function defaultKodaalHome(env = process.env, platform = process.platform) {
  const platformPath = pathApi(platform);
  if (env.KODAAL_HOME) {
    return env.KODAAL_HOME;
  }
  if (env.KODAAL_CONFIG) {
    return platformPath.dirname(env.KODAAL_CONFIG);
  }
  if (platform === "win32" && env.APPDATA) {
    return platformPath.join(env.APPDATA, "Kodaal");
  }
  return platformPath.join(env.HOME || os.homedir(), ".kodaal");
}

function tokenPath(env = process.env, platform = process.platform) {
  return pathApi(platform).join(defaultKodaalHome(env, platform), "token");
}

async function readToken(env = process.env, platform = process.platform) {
  const file = tokenPath(env, platform);
  return (await fs.promises.readFile(file, "utf8")).trim();
}

module.exports = {
  defaultKodaalHome,
  pathApi,
  readToken,
  tokenPath
};
