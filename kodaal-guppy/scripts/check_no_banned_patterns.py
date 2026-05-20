#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXCLUDED_DIRS = {".git", ".kodaal-dev", ".tmp", "__pycache__", "target", "node_modules", "zip"}
EXCLUDED_SUFFIXES = {
    ".zip",
    ".7z",
    ".gz",
    ".rar",
    ".tar",
    ".tgz",
    ".xz",
}
TEXT_SUFFIXES = {
    "",
    ".c",
    ".cfg",
    ".css",
    ".csv",
    ".h",
    ".html",
    ".ini",
    ".js",
    ".json",
    ".jsx",
    ".lock",
    ".md",
    ".py",
    ".rs",
    ".sh",
    ".sql",
    ".toml",
    ".ts",
    ".tsx",
    ".txt",
    ".yaml",
    ".yml",
}


def banned_patterns() -> tuple[tuple[str, re.Pattern[str]], ...]:
    pieces = (
        ("ev" + "al", r"\bev" + r"al\s*\("),
        ("new " + "Function", r"\bnew\s+" + r"Function\b"),
        ("inner" + "HTML", r"\binner" + r"HTML\b"),
        (
            "dangerously" + "Set" + "Inner" + "HTML",
            r"\bdangerously" + r"Set" + r"Inner" + r"HTML\b",
        ),
        ("TO" + "DO", r"\bTO" + r"DO\b"),
        ("FIX" + "ME", r"\bFIX" + r"ME\b"),
        ("X" + "XX", r"(?<!-)X" + r"XX\b"),
        ("HA" + "CK", r"\bHA" + r"CK\b"),
        ("<REQ" + "UIRED:", r"<REQ" + r"UIRED:"),
    )
    return tuple((name, re.compile(pattern)) for name, pattern in pieces)


def is_excluded(path: Path) -> bool:
    relative_parts = path.relative_to(ROOT).parts
    if any(part in EXCLUDED_DIRS for part in relative_parts):
        return True
    return path.suffix.lower() in EXCLUDED_SUFFIXES


def is_text_candidate(path: Path) -> bool:
    return path.suffix.lower() in TEXT_SUFFIXES


def describes_checker(path: Path, text: str) -> bool:
    if path.suffix.lower() != ".md":
        return False
    normalized = path.relative_to(ROOT).as_posix()
    if normalized == "Docs/Status/Issues.md":
        return True
    markers = (
        "check_no_banned_patterns.py",
        "check_no_banned_patterns",
        "banned pattern",
        "banned-pattern",
    )
    return any(marker in text for marker in markers)


def is_allowed_doc_policy_line(path: Path, line: str) -> bool:
    if path.suffix.lower() != ".md":
        return False
    lowered = line.lower()
    policy_markers = (
        "banned",
        "checker",
        "check",
        "ci fails",
        "comment",
        "comments",
        "placeholder",
        "placeholders",
    )
    return any(marker in lowered for marker in policy_markers)


def read_text(path: Path) -> tuple[str | None, str | None]:
    try:
        return path.read_text(encoding="utf-8"), None
    except UnicodeDecodeError:
        return None, None
    except OSError as error:
        relative = path.relative_to(ROOT).as_posix()
        return None, f"{relative}: cannot read file for banned-pattern scan: {error}"


def scan_file(path: Path, patterns: tuple[tuple[str, re.Pattern[str]], ...]) -> list[str]:
    text, read_error = read_text(path)
    if read_error is not None:
        return [read_error]
    if text is None:
        return []
    if describes_checker(path, text):
        return []

    relative = path.relative_to(ROOT).as_posix()
    findings: list[str] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        for name, pattern in patterns:
            if pattern.search(line):
                if is_allowed_doc_policy_line(path, line):
                    continue
                findings.append(f"{relative}:{line_number}: banned pattern {name}")
    return findings


def iter_project_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or is_excluded(path) or not is_text_candidate(path):
            continue
        files.append(path)
    return sorted(files)


def main() -> int:
    findings: list[str] = []
    patterns = banned_patterns()
    for path in iter_project_files():
        findings.extend(scan_file(path, patterns))

    if findings:
        print("Banned pattern check failed.")
        print("\n".join(findings))
        return 1

    print("Banned pattern check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
