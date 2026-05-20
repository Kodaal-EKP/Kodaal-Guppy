# Kodaal Guppy

**Your prompts are everywhere. Guppy brings them home.**

You write prompts in the browser, in the terminal, in the IDE, and in desktop apps. They end up scattered across tabs, chat histories, log files, and copy-paste buffers. When you need that perfect phrasing again — for a refactor, a bugfix, or the next feature — you search from memory or start over.

**Kodaal Guppy** is a local-first prompt workspace that captures what you actually send to AI tools, stores it on your machine, and gives you one place to search, tag, organize, reuse, and export it.

---

## The problem

- Prompts live in **many surfaces** (Claude, ChatGPT, Gemini, Perplexity, Claude Code, Codex, Copilot, Cursor, and more).
- There is **no shared history** across those tools.
- Reuse means **remembering**, **digging**, or **rewriting** — not retrieving.
- Cloud-only “prompt libraries” mean **your work leaves your machine** by default.

---

## The solution

Guppy runs as a small daemon on your computer. It:

1. **Captures** user-authored prompts from supported browser, CLI, IDE, and MCP surfaces.
2. **Stores** them in a local SQLite database with full-text search, projects, tags, and favorites.
3. **Serves** a workspace UI at `http://127.0.0.1:7878/ui` so you can find, copy, and reuse prompts without leaving your machine.

One workspace. Every surface you already use.

---

## Why Guppy is different

| | Guppy |
|---|---|
| **Local-first** | Data stays under your user profile (`~/.kodaal` or `%APPDATA%\Kodaal`). No cloud account required to work. Default config makes **no outbound network** calls. |
| **Multi-surface** | Browser extension, CLI watchers, VS Code–family IDE extension, and MCP tools — not a single-app plugin. |
| **Rust core** | Fast capture path, low idle overhead, and a single static binary model built for daily use alongside your editor and browser. |
| **Security by design** | Loopback-only API, token auth, no prompt text in logs, domain blocklist, pause/resume, and audit events that use hashes — not raw prompt content. |

---

## What it runs

| Surface | What Guppy captures |
|---|---|
| Web UI | Local prompt workspace at `http://127.0.0.1:7878/ui`. |
| Browser extension | Prompts from supported providers including Claude, ChatGPT, Gemini, and Perplexity. |
| CLI watcher | Local prompt history and shell-hook captures for tools such as Claude Code, Codex, and Aider. |
| IDE extension | VS Code-family capture, project hints, and local similar-prompt suggestions. |
| MCP server | Local MCP stdio tools through `kodaal mcp-server`. |

---

## Run from source

Requirements:

- Rust toolchain
- Git
- Node.js (if you build or test the browser and IDE extensions)

**Windows (PowerShell):**

```powershell
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
.\target\debug\kodaal.exe start --detach
```

**macOS / Linux:**

```bash
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
./target/debug/kodaal start --detach
```

Open `http://127.0.0.1:7878/ui` in your browser.

Stop the daemon:

```powershell
kodaal stop
```

If you run directly from `target/debug`, use `.\target\debug\kodaal.exe stop` on Windows or `./target/debug/kodaal stop` on macOS/Linux.

---

## User guide

See [RUN_GUPPY.md](RUN_GUPPY.md) for platform-specific setup, extension install, MCP configuration, and troubleshooting.

---

## Common commands

```powershell
kodaal status
kodaal recent 20
kodaal search "rust sqlx"
kodaal tag <prompt-id> rust
kodaal favorite <prompt-id>
kodaal export --format json --output guppy-export.json
```

---

## Build checks

```powershell
cargo test --workspace
node browser-ext/scripts/validate-extension.mjs
node --test browser-ext/tests/*.test.mjs
node ide-ext/vscode/scripts/validate-extension.mjs
node --test ide-ext/vscode/tests/*.test.mjs
```

---

## License

Kodaal Guppy is licensed under the **Business Source License 1.1**.

- **Licensor:** Kodaal-EKP
- **Additional Use Grant:** free for personal use and for organizations below the employee and revenue thresholds in [LICENSE](LICENSE)
- **Change License:** Apache License 2.0
- **Change Date:** four years after each version’s publication date

See [LICENSE](LICENSE) before commercial deployment.
