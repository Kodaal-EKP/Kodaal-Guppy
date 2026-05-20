#!/usr/bin/env python3
from __future__ import annotations

import filecmp
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PAIRS = (
    (
        ROOT / "Docs" / "presentation" / "KodaalReport.jsx",
        ROOT / "Docs" / "Concept" / "KodaalReport.jsx",
    ),
    (
        ROOT / "Docs" / "presentation" / "KodaalReveal.jsx",
        ROOT / "Docs" / "Concept" / "KodaalReveal.jsx",
    ),
)


def main() -> int:
    findings: list[str] = []
    for source, duplicate in PAIRS:
        if not source.exists():
            findings.append(f"missing brand source: {source.relative_to(ROOT)}")
            continue
        if not duplicate.exists():
            continue
        if not filecmp.cmp(source, duplicate, shallow=False):
            findings.append(
                f"brand JSX duplicate drift: {duplicate.relative_to(ROOT)} must match {source.relative_to(ROOT)}"
            )

    if findings:
        print("Brand JSX copy check failed.")
        print("\n".join(findings))
        return 1

    print("Brand JSX copy check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
