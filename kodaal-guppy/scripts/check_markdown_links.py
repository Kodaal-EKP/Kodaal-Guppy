#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1]
LINK_PATTERN = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
EXTERNAL_PREFIXES = (
    "http://",
    "https://",
    "mailto:",
    "tel:",
)


def markdown_files() -> list[Path]:
    return sorted(
        path
        for path in ROOT.rglob("*.md")
        if ".git" not in path.parts and "target" not in path.parts
    )


def link_target(raw: str) -> str | None:
    target = raw.strip()
    if not target or target.startswith("#"):
        return None
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1].strip()
    lowered = target.lower()
    if lowered.startswith(EXTERNAL_PREFIXES):
        return None
    return unquote(target.split("#", 1)[0])


def main() -> int:
    findings: list[str] = []
    for path in markdown_files():
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            for match in LINK_PATTERN.finditer(line):
                target = link_target(match.group(1))
                if target is None:
                    continue
                resolved = (path.parent / target).resolve()
                try:
                    resolved.relative_to(ROOT)
                except ValueError:
                    findings.append(
                        f"{path.relative_to(ROOT)}:{line_number}: link leaves repo: {target}"
                    )
                    continue
                if not resolved.exists():
                    findings.append(
                        f"{path.relative_to(ROOT)}:{line_number}: missing link target: {target}"
                    )

    if findings:
        print("Markdown link check failed.")
        print("\n".join(findings))
        return 1

    print("Markdown link check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
