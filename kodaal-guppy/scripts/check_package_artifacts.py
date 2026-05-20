#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = {
    "dist/browser-ext-chromium-0.1.0.zip": {"manifest.json", "background.js", "content.js", "popup.html", "popup.js", "sites.json"},
    "dist/browser-ext-firefox-0.1.0.xpi": {"manifest.json", "background.js", "content.js", "popup.html", "popup.js", "sites.json"},
    "dist/kodaal-guppy-vscode-0.1.0.vsix": {"package.json", "src/client.js", "src/extension.js", "src/token.js"},
}
FORBIDDEN_NAME_PARTS = ("node_modules/", ".map", "package-lock.json", "pnpm-lock.yaml", "yarn.lock")
FORBIDDEN_TEXT = (
    re.compile(r"<REQ" + r"UIRED:"),
    re.compile(r"C:\\Users\\", re.I),
    re.compile(r"/Users/[^/\s]+/"),
    re.compile(r"/home/[^/\s]+/"),
    re.compile(r"sentry|posthog|amplitude|mixpanel|google-analytics", re.I),
)


def check_manifest(name: str, manifest: dict) -> list[str]:
    failures: list[str] = []
    if manifest.get("externally_connectable") is not None:
        failures.append(f"{name}: externally_connectable is not allowed")
    if manifest.get("update_url") is not None:
        failures.append(f"{name}: update_url is not allowed")
    for permission in manifest.get("host_permissions", []):
        if permission in {"<all_urls>", "*://*/*"}:
            failures.append(f"{name}: overbroad host permission {permission}")
    return failures


def main() -> int:
    failures: list[str] = []
    for relative, required in ARTIFACTS.items():
        archive = ROOT / relative
        if not archive.exists():
            failures.append(f"{relative}: missing archive")
            continue
        with zipfile.ZipFile(archive) as zipped:
            names = set(zipped.namelist())
            missing = required - names
            if missing:
                failures.append(f"{relative}: missing {', '.join(sorted(missing))}")
            for name in names:
                lowered = name.lower()
                if any(part in lowered for part in FORBIDDEN_NAME_PARTS):
                    failures.append(f"{relative}:{name}: forbidden packaged file")
                if not lowered.endswith((".js", ".json", ".html", ".css")):
                    continue
                text = zipped.read(name).decode("utf-8", errors="ignore")
                for pattern in FORBIDDEN_TEXT:
                    if pattern.search(text):
                        failures.append(f"{relative}:{name}: forbidden packaged text pattern")
                if name == "manifest.json":
                    failures.extend(check_manifest(f"{relative}:{name}", json.loads(text)))

    if failures:
        print("Package artifact check failed.")
        print("\n".join(failures))
        return 1

    print("Package artifact check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
