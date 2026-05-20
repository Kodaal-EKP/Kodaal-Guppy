#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BANNED_LICENSE_RE = re.compile(r"\b(?:A?GPL|LGPL)\b", re.IGNORECASE)
TOKEN_RE = re.compile(r"\b[A-Za-z0-9.+-]+(?:\s+WITH\s+[A-Za-z0-9.+-]+)?\b")
ALLOWED_LICENSES = {
    "0BSD",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CC0-1.0",
    "ISC",
    "MIT",
    "MIT-0",
    "MPL-2.0",
    "Unlicense",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Zlib",
}


def rust_host() -> str:
    result = subprocess.run(
        ["rustc", "-vV"],
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.split(":", 1)[1].strip()
    raise RuntimeError("rustc -vV did not report a host target")


def cargo_metadata() -> dict:
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--features",
            "desktop",
            "--filter-platform",
            rust_host(),
        ],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        print(result.stdout, end="")
        print(result.stderr, end="", file=sys.stderr)
        raise SystemExit(result.returncode)
    return json.loads(result.stdout)


def license_tokens(expression: str) -> set[str]:
    return {
        token.strip()
        for token in TOKEN_RE.findall(expression.replace("(", " ").replace(")", " "))
        if token.upper() not in {"AND", "OR", "WITH"}
    }


def main() -> int:
    metadata = cargo_metadata()
    failures: list[str] = []
    for package in metadata.get("packages", []):
        name = package.get("name", "<unknown>")
        license_expr = package.get("license")
        license_file = package.get("license_file")
        source = package.get("source")
        if not source and license_expr == "BUSL-1.1":
            continue
        if not source and license_file:
            continue
        if not license_expr:
            failures.append(f"{name}: missing license expression")
            continue
        if BANNED_LICENSE_RE.search(license_expr):
            failures.append(f"{name}: banned copyleft license expression {license_expr!r}")
            continue
        unknown = license_tokens(license_expr) - ALLOWED_LICENSES
        if unknown:
            failures.append(f"{name}: unapproved license token(s) {', '.join(sorted(unknown))}")

    if failures:
        print("Cargo license gate failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("Cargo license gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
