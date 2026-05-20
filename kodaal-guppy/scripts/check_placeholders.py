#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCAN_DIRS = ("core/src", "browser-ext", "ide-ext/vscode/src", "scripts")
TEXT_SUFFIXES = {".rs", ".js", ".mjs", ".json", ".html", ".css", ".py", ".toml", ".yaml", ".yml"}
EXCLUDED_PARTS = {"target", "node_modules", "dist", "tests", "__pycache__"}
SELF = Path(__file__).resolve()
CHECKER_FILES = {"scripts/check_no_banned_patterns.py", "scripts/check_placeholders.py"}


def patterns() -> tuple[tuple[str, re.Pattern[str]], ...]:
    return (
        ("required credential marker", re.compile(r"<REQ" + r"UIRED:[^>]+>")),
        ("todo implementation marker", re.compile(r"\bTO" + r"DO\s+(implement|wire|finish|later)\b", re.I)),
        ("mock return marker", re.compile(r"\bmock\s+return\b", re.I)),
        ("placeholder marker", re.compile(r"\bplace" + r"holder\b", re.I)),
        ("dummy marker", re.compile(r"\bdum" + r"my\b", re.I)),
        ("example domain marker", re.compile(r"\bexample\.com\b", re.I)),
    )


def iter_files() -> list[Path]:
    files: list[Path] = []
    for directory in SCAN_DIRS:
        root = ROOT / directory
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file() or path.resolve() == SELF:
                continue
            if path.relative_to(ROOT).as_posix() in CHECKER_FILES:
                continue
            if path.suffix.lower() not in TEXT_SUFFIXES:
                continue
            if any(part in EXCLUDED_PARTS for part in path.relative_to(ROOT).parts):
                continue
            files.append(path)
    return sorted(files)


def main() -> int:
    findings: list[str] = []
    checks = patterns()
    for path in iter_files():
        text = path.read_text(encoding="utf-8", errors="ignore")
        relative = path.relative_to(ROOT).as_posix()
        for line_number, line in enumerate(text.splitlines(), start=1):
            for label, pattern in checks:
                if pattern.search(line):
                    if label == "placeholder marker" and any(
                        marker in line for marker in ("::placeholder", "placeholder=", "[placeholder")
                    ):
                        continue
                    findings.append(f"{relative}:{line_number}: {label}")

    if findings:
        print("Placeholder check failed.")
        print("\n".join(findings))
        return 1

    print("Placeholder check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
