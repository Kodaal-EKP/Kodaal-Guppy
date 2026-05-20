"use strict";

const fs = require("node:fs");
const path = require("node:path");

function contentToText(content) {
  if (typeof content === "string") {
    return content;
  }
  if (Array.isArray(content)) {
    return content
      .map((part) => {
        if (typeof part === "string") {
          return part;
        }
        if (part && ["tool_result", "tool_use", "image"].includes(String(part.type || ""))) {
          return "";
        }
        if (part && typeof part.text === "string") {
          return part.text;
        }
        if (part && typeof part.content === "string") {
          return part.content;
        }
        return "";
      })
      .filter(Boolean)
      .join("\n");
  }
  if (content && ["tool_result", "tool_use", "image"].includes(String(content.type || ""))) {
    return "";
  }
  if (content && typeof content.text === "string") {
    return content.text;
  }
  return "";
}

function normalizePromptText(text) {
  const trimmed = String(text || "").trim();
  if (!trimmed || isInternalContextText(trimmed)) {
    return "";
  }
  if (trimmed.startsWith("<command-message>")) {
    return normalizeCommandMessage(trimmed);
  }
  if (trimmed.startsWith("# Files mentioned by the user:")) {
    return extractEmbeddedUserRequest(trimmed);
  }
  if (trimmed.startsWith("# /") && trimmed.includes("\n## Input")) {
    return normalizeSlashCommandWrapper(trimmed);
  }
  return String(text).trimEnd();
}

function isInternalContextText(text) {
  return text === "Tool loaded." ||
    text.startsWith("# AGENTS.md instructions") ||
    text.startsWith("<environment_context>") ||
    text.startsWith("<system-reminder>") ||
    text.startsWith("<INSTRUCTIONS>") ||
    text.startsWith("# Model Set Context") ||
    text.startsWith("This session is being continued from a previous conversation") ||
    text.startsWith("The following is the Codex agent history") ||
    text === "<<autonomous-loop-dynamic>>" ||
    (text.startsWith("Your task is to create a detailed summary of the conversation so far") &&
      text.includes("Do NOT use any tools"));
}

function normalizeCommandMessage(text) {
  const command = tagText(text, "command-name");
  const args = tagText(text, "command-args");
  return `${command} ${args}`.trim();
}

function extractEmbeddedUserRequest(text) {
  for (const marker of ["## My request for Codex:", "## My request for Claude:", "## My request:"]) {
    const index = text.lastIndexOf(marker);
    if (index >= 0) {
      return text.slice(index + marker.length).trim();
    }
  }
  return "";
}

function tagText(text, tag) {
  const open = `<${tag}>`;
  const close = `</${tag}>`;
  const start = text.indexOf(open);
  if (start < 0) {
    return "";
  }
  const bodyStart = start + open.length;
  const end = text.indexOf(close, bodyStart);
  return end < 0 ? "" : text.slice(bodyStart, end).trim();
}

function normalizeSlashCommandWrapper(text) {
  const firstLine = text.split(/\r?\n/, 1)[0] || "";
  const command = firstLine.replace(/^#\s*/, "").trim().split(/\s+/, 1)[0] || "";
  if (!command.startsWith("/")) {
    return "";
  }
  const index = text.lastIndexOf("## Input");
  const input = index >= 0 ? text.slice(index + "## Input".length).trim() : "";
  if (!input) {
    return "";
  }
  return input.startsWith(command) ? input : `${command} ${input}`;
}

function isInternalRecord(value) {
  return value.isSidechain === true ||
    value.isCompactSummary === true ||
    value.isVisibleInTranscriptOnly === true ||
    value.message?.isSidechain === true ||
    value.payload?.isSidechain === true;
}

function extractMessagesFromJson(value, messages = []) {
  if (!value || typeof value !== "object") {
    return messages;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      extractMessagesFromJson(item, messages);
    }
    return messages;
  }
  if (isInternalRecord(value)) {
    return messages;
  }
  const role = String(value.role || value.author || value.type || "").toLowerCase();
  const text = normalizePromptText(contentToText(value.content || value.text || value.message));
  if ((role === "user" || role === "human") && text.length > 0) {
    messages.push({
      text,
      conversation_id: typeof value.conversation_id === "string" ? value.conversation_id : undefined,
      created_at: typeof value.created_at === "string" ? value.created_at : undefined
    });
  }
  for (const child of Object.values(value)) {
    if (child && typeof child === "object") {
      extractMessagesFromJson(child, messages);
    }
  }
  return messages;
}

function walkJsonFiles(rootDir, acceptedPath) {
  const files = [];
  if (!rootDir || !fs.existsSync(rootDir)) {
    return files;
  }
  const stack = [rootDir];
  while (stack.length > 0) {
    const current = stack.pop();
    let entries = [];
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch (error) {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile() && entry.name.endsWith(".json") && (!acceptedPath || acceptedPath(full))) {
        files.push(full);
      }
    }
  }
  return files.sort();
}

class JsonChatWatcher {
  constructor(options) {
    this.name = options.name;
    this.roots = options.roots;
    this.acceptedPath = options.acceptedPath;
    this.client = options.client;
    this.projectHint = options.projectHint;
    this.sourceApp = options.sourceApp;
    this.intervalMs = options.intervalMs || 5000;
    this.output = options.output;
    this.seen = new Set();
    this.timer = null;
  }

  start() {
    if (this.timer) {
      return;
    }
    this.scan().catch((error) => this.log(`scan failed: ${error.message}`));
    this.timer = setInterval(() => {
      this.scan().catch((error) => this.log(`scan failed: ${error.message}`));
    }, this.intervalMs);
  }

  stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  async scan() {
    for (const root of this.roots) {
      for (const file of walkJsonFiles(root, this.acceptedPath)) {
        await this.scanFile(file);
      }
    }
  }

  async scanFile(file) {
    let parsed;
    try {
      parsed = JSON.parse(await fs.promises.readFile(file, "utf8"));
    } catch (error) {
      return;
    }
    const messages = extractMessagesFromJson(parsed);
    for (let index = 0; index < messages.length; index += 1) {
      const message = messages[index];
      const key = `${file}:${index}:${message.text}`;
      if (this.seen.has(key)) {
        continue;
      }
      await this.client.logPrompt({
        text: message.text,
        source: "ide",
        source_app: this.sourceApp,
        project_hint: this.projectHint(),
        conversation_id: message.conversation_id,
        metadata: {
          source_file: file,
          watcher: this.name
        }
      });
      this.seen.add(key);
    }
  }

  log(message) {
    if (this.output && this.output.appendLine) {
      this.output.appendLine(`Kodaal ${this.name}: ${message}`);
    }
  }
}

module.exports = {
  JsonChatWatcher,
  extractMessagesFromJson,
  normalizePromptText,
  walkJsonFiles
};
