#!/usr/bin/env python3
from __future__ import annotations

import shutil
import subprocess
import sys


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


def main() -> int:
    if shutil.which("cargo-deny") is None and shutil.which("cargo") is None:
        print("cargo-deny gate failed: cargo is not installed.")
        return 1
    host = rust_host()
    command = [
        "cargo",
        "deny",
        "--features",
        "desktop",
        "--target",
        host,
        "check",
        "advisories",
        "licenses",
        "bans",
        "sources",
    ]
    try:
        result = subprocess.run(command, text=True)
    except OSError as error:
        print(f"cargo-deny gate failed: {error}")
        return 1
    if result.returncode != 0:
        print("cargo-deny gate failed.")
        print("Install with: cargo install cargo-deny --locked")
        return result.returncode
    print("cargo-deny advisory and license gate passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
