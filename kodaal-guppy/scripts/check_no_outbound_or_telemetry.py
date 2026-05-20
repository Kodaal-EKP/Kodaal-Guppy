#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCAN_DIRS = ("core/src", "browser-ext", "ide-ext/vscode/src", "tray-app/src")
TEXT_SUFFIXES = {".rs", ".js", ".json", ".html", ".css"}
ALLOWED_REMOTE_URL_PREFIXES = (
    "http://127.0.0.1:7878",
    "http://localhost:7878",
    "http://www.w3.org/2000/svg",
    "https://claude.ai/",
    "https://chatgpt.com/",
    "https://chat.openai.com/",
    "https://gemini.google.com/",
    "https://perplexity.ai/",
    "https://www.perplexity.ai/",
)
TELEMETRY_PATTERNS = (
    "sentry",
    "posthog",
    "amplitude",
    "mixpanel",
    "google-analytics",
    "gtag(",
    "analytics.js",
    "telemetry.",
)


def iter_files() -> list[Path]:
    files: list[Path] = []
    for directory in SCAN_DIRS:
        root = ROOT / directory
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in TEXT_SUFFIXES:
                continue
            if any(part in {"target", "node_modules", "dist"} for part in path.relative_to(ROOT).parts):
                continue
            files.append(path)
    return sorted(files)


def allowed_url(url: str) -> bool:
    return any(url.startswith(prefix) for prefix in ALLOWED_REMOTE_URL_PREFIXES)


def allowed_fetch(line: str) -> bool:
    stripped = line.strip()
    allowed_fragments = (
        "fetch(path",
        "root.fetch(`${settings.endpoint}",
        "root.fetch(api.runtime.getURL(",
    )
    return any(fragment in stripped for fragment in allowed_fragments)


def validate_manifest(path: Path) -> list[str]:
    failures: list[str] = []
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest.get("externally_connectable") is not None:
        failures.append(f"{path.relative_to(ROOT).as_posix()}: externally_connectable is not allowed")
    if manifest.get("update_url") is not None:
        failures.append(f"{path.relative_to(ROOT).as_posix()}: update_url is not allowed")
    for key in ("host_permissions", "permissions"):
        for permission in manifest.get(key, []):
            if permission in {"<all_urls>", "*://*/*"}:
                failures.append(
                    f"{path.relative_to(ROOT).as_posix()}: overbroad permission {permission}"
                )
    return failures


def main() -> int:
    findings: list[str] = []
    url_pattern = re.compile(r"https?://[^\s\"'`),]+")
    fetch_pattern = re.compile(r"\bfetch\s*\(")

    for path in iter_files():
        relative = path.relative_to(ROOT).as_posix()
        text = path.read_text(encoding="utf-8", errors="ignore")
        lowered = text.lower()
        for marker in TELEMETRY_PATTERNS:
            if marker in lowered:
                findings.append(f"{relative}: telemetry marker {marker}")
        for line_number, line in enumerate(text.splitlines(), start=1):
            for url in url_pattern.findall(line):
                if not allowed_url(url):
                    findings.append(f"{relative}:{line_number}: remote URL {url}")
            if "TcpStream::connect" in line and "127.0.0.1" not in line:
                findings.append(f"{relative}:{line_number}: non-loopback TcpStream::connect")
            if fetch_pattern.search(line) and not allowed_fetch(line):
                findings.append(f"{relative}:{line_number}: unapproved fetch call")

    findings.extend(validate_manifest(ROOT / "browser-ext" / "manifest.json"))

    if findings:
        print("Outbound/telemetry check failed.")
        print("\n".join(findings))
        return 1

    print("Outbound/telemetry check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
