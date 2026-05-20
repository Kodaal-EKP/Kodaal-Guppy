# Run Kodaal Guppy

This guide is for users who want to run Kodaal Guppy locally on Windows, macOS, or Linux.

It does not include internal project planning, private status docs, local developer machine paths, or personal information.

## Requirements

- Git.
- Rust toolchain.
- Node.js if you want browser or IDE extensions.
- A local browser for the Web UI.

## Windows

Build:

```powershell
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
```

Start:

```powershell
.\target\debug\kodaal.exe start --detach
```

Open:

```text
http://127.0.0.1:7878/ui
```

Check status:

```powershell
.\target\debug\kodaal.exe status
```

Stop:

```powershell
.\target\debug\kodaal.exe stop
```

## macOS

Build:

```bash
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
```

Start:

```bash
./target/debug/kodaal start --detach
```

Open:

```text
http://127.0.0.1:7878/ui
```

Check status:

```bash
./target/debug/kodaal status
```

Stop:

```bash
./target/debug/kodaal stop
```

## Linux

Build:

```bash
git clone https://github.com/Kodaal-EKP/Kodaal-Guppy.git
cd Kodaal-Guppy/kodaal-guppy
cargo build --workspace
```

Start:

```bash
./target/debug/kodaal start --detach
```

Open:

```text
http://127.0.0.1:7878/ui
```

Check status:

```bash
./target/debug/kodaal status
```

Stop:

```bash
./target/debug/kodaal stop
```

## Browser Extension

Build extension bundles:

```bash
cd browser-ext
npm run build
```

Chromium browsers:

1. Open `chrome://extensions` or `edge://extensions`.
2. Enable developer mode.
3. Load the generated Chromium extension folder from `browser-ext/dist`.
4. Keep the Guppy daemon running.

Firefox:

1. Open `about:debugging#/runtime/this-firefox`.
2. Choose "Load Temporary Add-on".
3. Select the generated Firefox manifest from `browser-ext/dist`.
4. Keep the Guppy daemon running.

Supported browser match rules include:

- `claude.ai`
- `chatgpt.com`
- `chat.openai.com`
- `gemini.google.com`
- `perplexity.ai`

## IDE Extension

Build the VS Code-family extension:

```bash
cd ide-ext/vscode
npm run build
```

Install the generated VSIX in VS Code, Cursor, or a compatible VS Code-family editor.

Useful commands from the editor command palette:

- `Kodaal: Save Current Prompt`
- `Kodaal: Pause Capture`
- `Kodaal: Resume Capture`
- `Kodaal: Open Workspace`
- `Kodaal: Suggest Similar Prompt`

## CLI Capture And Suggestions

Install shell hooks for Bash or Zsh:

```bash
kodaal install-shell-hook --shell bash
kodaal install-shell-hook --shell zsh
```

Search and reuse prompts:

```bash
kodaal recent 20
kodaal search "refactor rust sqlite"
kodaal suggest --source cli --source-app codex-cli "refactor rust sqlite migration"
```

If you are running from `target/debug`, prefix commands with the binary path, for example `./target/debug/kodaal recent 20`.

## MCP

Use this command in an MCP client that supports stdio servers:

```bash
kodaal mcp-server
```

Example MCP server entry:

```json
{
  "mcpServers": {
    "kodaal": {
      "command": "kodaal",
      "args": ["mcp-server"]
    }
  }
}
```

## Troubleshooting

### UI does not open

Run:

```bash
kodaal status
```

If the service is stopped, run:

```bash
kodaal start --detach
```

If `kodaal` is not on your `PATH`, use the built binary path from the platform sections above.

### Browser or IDE is not capturing

Check:

- The Guppy daemon is running.
- Capture is not paused.
- The browser/IDE extension is installed and enabled.
- The provider domain or source app is not blocklisted.
- The prompt is user-authored text, not a notification, tool result, or internal system message.

### Smart suggestions do not appear

Check:

- Suggestions are enabled in settings.
- The draft text is long enough to match against prior prompts.
- Similar prompts already exist in the local database.
- The daemon is running.

### Export prompts

```bash
kodaal export --format json --output guppy-export.json
kodaal export --format md --output guppy-export.md
```

Exports are local files. Treat them as sensitive if your prompts contain sensitive text.
