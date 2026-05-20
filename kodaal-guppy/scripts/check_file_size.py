#!/usr/bin/env python3
from __future__ import annotations

import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MAX_RUST_LINES = 500
EXCLUDED_PARTS = {"target", "dist"}
RUST_SOURCE_ROOTS = [ROOT / "core" / "src", ROOT / "tray-app" / "src"]


def main() -> int:
    findings: list[str] = []
    for source_root in RUST_SOURCE_ROOTS:
        if not source_root.exists():
            continue
        for path in sorted(source_root.rglob("*.rs")):
            if any(part in EXCLUDED_PARTS for part in path.parts):
                continue
            line_count = len(path.read_text(encoding="utf-8").splitlines())
            if line_count > MAX_RUST_LINES:
                rel = path.relative_to(ROOT).as_posix()
                findings.append(f"{rel}: {line_count} lines exceeds {MAX_RUST_LINES}")

    if findings:
        print("File-size check failed.")
        print("\n".join(findings))
        return 1

    print(f"File-size check passed: Rust source files are <= {MAX_RUST_LINES} lines.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
