"use strict";

const http = require("node:http");
const fs = require("node:fs/promises");
const path = require("node:path");
const { defaultKodaalHome, readToken } = require("./token");

const DEFAULT_ENDPOINT = "http://127.0.0.1:7878";

function normalizeEndpoint(value) {
  const raw = String(value || DEFAULT_ENDPOINT).trim() || DEFAULT_ENDPOINT;
  const url = new URL(raw);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || url.port !== "7878") {
    throw new Error("Kodaal endpoint must be http://127.0.0.1:7878");
  }
  return DEFAULT_ENDPOINT;
}

function isValidToken(token) {
  return /^[a-f0-9]{64}$/.test(String(token || ""));
}

function requestJson(endpoint, token, method, apiPath, body) {
  return new Promise((resolve, reject) => {
    const base = new URL(normalizeEndpoint(endpoint));
    const payload = body ? JSON.stringify(body) : "";
    const request = http.request(
      {
        hostname: base.hostname,
        port: base.port,
        path: apiPath,
        method,
        headers: {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(payload),
          "X-Kodaal-Token": token
        },
        timeout: 3000
      },
      (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("end", () => {
          const text = Buffer.concat(chunks).toString("utf8");
          if (response.statusCode < 200 || response.statusCode >= 300) {
            reject(new Error(`HTTP_${response.statusCode}`));
            return;
          }
          resolve(text ? JSON.parse(text) : null);
        });
      }
    );
    request.on("timeout", () => {
      request.destroy(new Error("REQUEST_TIMEOUT"));
    });
    request.on("error", reject);
    if (payload) {
      request.write(payload);
    }
    request.end();
  });
}

class KodaalClient {
  constructor(options = {}) {
    this.endpoint = normalizeEndpoint(options.endpoint || DEFAULT_ENDPOINT);
    this.tokenReader = options.tokenReader || readToken;
    this.requestJson = options.requestJson || requestJson;
    this.queueFile =
      options.queueFile || path.join(defaultKodaalHome(process.env, process.platform), "ide-queue.json");
  }

  async token() {
    const token = await this.tokenReader();
    if (!isValidToken(token)) {
      throw new Error("AUTH_TOKEN_INVALID");
    }
    return token;
  }

  async call(method, apiPath, body) {
    return this.requestJson(this.endpoint, await this.token(), method, apiPath, body);
  }

  async logPrompt(payload) {
    try {
      const response = await this.call("POST", "/api/prompts", payload);
      await this.drainQueue();
      return response;
    } catch (error) {
      await this.enqueuePrompt(payload, error);
      return { queued: true };
    }
  }

  async suggestPrompts(query) {
    const params = new URLSearchParams();
    params.set("q", String(query.q || ""));
    params.set("surface", query.surface || "ide");
    if (query.source_app) params.set("source_app", query.source_app);
    if (query.project_id) params.set("project_id", query.project_id);
    if (query.limit) params.set("limit", String(query.limit));
    return this.call("GET", `/api/prompts/suggestions?${params.toString()}`);
  }

  async reusePrompt(id) {
    return this.call("POST", `/api/prompts/${encodeURIComponent(id)}/reuse`, {});
  }

  async pause() {
    return this.call("POST", "/api/capture/pause", {});
  }

  async resume() {
    return this.call("POST", "/api/capture/resume", {});
  }

  async status() {
    return this.call("GET", "/api/capture/status");
  }

  async enqueuePrompt(payload, error) {
    const queue = await this.readQueue();
    queue.push({
      payload,
      attempts: 0,
      enqueued_at: new Date().toISOString(),
      last_error: String(error && error.message ? error.message : error)
    });
    await this.writeQueue(queue);
  }

  async drainQueue() {
    const queue = await this.readQueue();
    if (!queue.length) return { replayed: 0, remaining: 0 };
    const remaining = [];
    let replayed = 0;
    for (const item of queue) {
      try {
        await this.call("POST", "/api/prompts", item.payload);
        replayed += 1;
      } catch (error) {
        item.attempts = Number(item.attempts || 0) + 1;
        item.last_error = String(error && error.message ? error.message : error);
        remaining.push(item);
      }
    }
    await this.writeQueue(remaining);
    return { replayed, remaining: remaining.length };
  }

  async readQueue() {
    try {
      const text = await fs.readFile(this.queueFile, "utf8");
      const parsed = JSON.parse(text);
      return Array.isArray(parsed) ? parsed : [];
    } catch (error) {
      if (error && error.code === "ENOENT") return [];
      throw error;
    }
  }

  async writeQueue(queue) {
    await fs.mkdir(path.dirname(this.queueFile), { recursive: true });
    await fs.writeFile(this.queueFile, JSON.stringify(queue, null, 2));
  }
}

module.exports = {
  DEFAULT_ENDPOINT,
  KodaalClient,
  isValidToken,
  normalizeEndpoint,
  requestJson
};
