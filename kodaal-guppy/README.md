# Kodaal Guppy

Kodaal Guppy is a local-first prompt workspace. It captures prompts from supported AI tools, stores them on your machine, and gives you a workspace for search, tags, projects, favorites, copy/reuse, deletion, export, and analytics.

This public README is intentionally limited to what users need to run Guppy. Internal project planning, audits, private status notes, local paths, and personal information are not part of the public run documentation.

## What It Runs

| Surface | What Guppy captures |
|---|---|
| Web UI | Local prompt workspace at `http://127.0.0.1:7878/ui`. |
| Browser extension | Browser prompts from supported providers including Claude, ChatGPT, Gemini, and Perplexity. |
| CLI watcher | Local prompt history and shell-hook captures for tools such as Claude Code, Codex, and Aider. |
| IDE extension | VS Code-family prompt capture, project hints, and local similar-prompt suggestions. |
| MCP server | Local MCP stdio tools through `kodaal mcp-server`. |

## Run From Source

Requirements:

- Rust toolchain.
- Git.
- Node.js if you want to build or test the browser and IDE extensions.

Windows PowerShell:

```powershell
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
.\target\debug\kodaal.exe start --detach
```

macOS/Linux:

```bash
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
./target/debug/kodaal start --detach
```

Open:

```text
http://127.0.0.1:7878/ui
```

Stop:

```powershell
kodaal stop
```

If you are running directly from `target/debug`, use `.\target\debug\kodaal.exe stop` on Windows or `./target/debug/kodaal stop` on macOS/Linux.

## User Guide

Use [RUN_GUPPY.md](RUN_GUPPY.md) for platform-specific run steps, extension setup, smart suggestions, MCP setup, and troubleshooting.

## Common Commands

```powershell
kodaal status
kodaal recent 20
kodaal search "rust sqlx"
kodaal tag <prompt-id> rust
kodaal favorite <prompt-id>
kodaal export --format json --output guppy-export.json
```

## Build Checks

```powershell
cargo test --workspace
node browser-ext/scripts/validate-extension.mjs
node --test browser-ext/tests/*.test.mjs
node ide-ext/vscode/scripts/validate-extension.mjs
node --test ide-ext/vscode/tests/*.test.mjs
```

## License

Kodaal Guppy is licensed under the Business Source License 1.1.

- Licensor: Kodaal-EKP.
- Additional Use Grant: free use for personal use and for organizations below the employee and revenue thresholds stated in [LICENSE](LICENSE).
- Change License: Apache License 2.0.
- Change Date: four years after each version's publication date.

See [LICENSE](LICENSE) before commercial deployment.
