#!/usr/bin/env python3
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXACT_VERSION = re.compile(r"^=\d+\.\d+\.\d+(?:[-+][A-Za-z0-9_.-]+)?$")
DEPENDENCY_SECTIONS = {
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "workspace.dependencies",
}
NODE_VERSION = re.compile(r"^\d+\.\d+\.\d+$")
RUST_TOOLCHAIN = re.compile(r'^channel\s*=\s*"(\d+\.\d+\.\d+)"$')


def is_dependency_section(section: str) -> bool:
    if section in DEPENDENCY_SECTIONS:
        return True
    return any(
        section.startswith("target.") and section.endswith(f".{suffix}")
        for suffix in ("dependencies", "dev-dependencies", "build-dependencies")
    )


def strip_comment(line: str) -> str:
    in_string = False
    escaped = False
    result: list[str] = []
    for char in line:
        if escaped:
            result.append(char)
            escaped = False
            continue
        if char == "\\" and in_string:
            result.append(char)
            escaped = True
            continue
        if char == '"':
            in_string = not in_string
            result.append(char)
            continue
        if char == "#" and not in_string:
            break
        result.append(char)
    return "".join(result).strip()


def validate_dependency_line(manifest: Path, section: str, line_number: int, line: str) -> list[str]:
    if "=" not in line:
        return []
    name, raw_spec = line.split("=", 1)
    dependency_name = name.strip()
    spec = raw_spec.strip()
    if not dependency_name:
        return []

    location = f"{manifest.relative_to(ROOT).as_posix()}:{line_number}: {section}.{dependency_name}"

    string_spec = re.fullmatch(r'"([^"]+)"', spec)
    if string_spec:
        version = string_spec.group(1)
        if EXACT_VERSION.match(version):
            return []
        return [f"{location} must use an exact =x.y.z version, found {version!r}"]

    if spec.startswith("{") and spec.endswith("}"):
        if re.search(r"\bworkspace\s*=\s*true\b", spec):
            return []
        version_match = re.search(r'\bversion\s*=\s*"([^"]+)"', spec)
        if version_match:
            version = version_match.group(1)
            if EXACT_VERSION.match(version):
                return []
            return [f"{location} must use an exact =x.y.z version, found {version!r}"]
        if re.search(r'\bpath\s*=\s*"[^"]+"', spec) and not re.search(
            r'\b(git|registry)\s*=', spec
        ):
            return []
        return [f"{location} must declare an exact version"]

    return [f"{location} has unsupported dependency spec {spec!r}"]


def scan_manifest(manifest: Path) -> list[str]:
    findings: list[str] = []
    section = ""
    for line_number, raw_line in enumerate(
        manifest.read_text(encoding="utf-8").splitlines(), start=1
    ):
        line = strip_comment(raw_line)
        if not line:
            continue
        section_match = re.fullmatch(r"\[([^\]]+)\]", line)
        if section_match:
            section = section_match.group(1)
            continue
        if is_dependency_section(section):
            findings.extend(validate_dependency_line(manifest, section, line_number, line))
    return findings


def validate_toolchain_pins() -> list[str]:
    findings: list[str] = []
    nvmrc = ROOT / ".nvmrc"
    if not nvmrc.exists():
        findings.append(".nvmrc is missing")
    else:
        node_pin = nvmrc.read_text(encoding="utf-8").strip()
        if not NODE_VERSION.fullmatch(node_pin):
            findings.append(f".nvmrc must pin an exact x.y.z version, found {node_pin!r}")
        else:
            try:
                node_version = subprocess.run(
                    ["node", "--version"],
                    check=True,
                    capture_output=True,
                    text=True,
                ).stdout.strip()
            except (OSError, subprocess.CalledProcessError) as error:
                findings.append(f"node --version failed while checking .nvmrc: {error}")
            else:
                if node_version.lstrip("v") != node_pin:
                    findings.append(
                        f"installed Node {node_version!r} does not match .nvmrc {node_pin!r}"
                    )

    rust_toolchain = ROOT / "rust-toolchain.toml"
    if not rust_toolchain.exists():
        findings.append("rust-toolchain.toml is missing")
    else:
        channel = None
        for line in rust_toolchain.read_text(encoding="utf-8").splitlines():
            match = RUST_TOOLCHAIN.fullmatch(line.strip())
            if match:
                channel = match.group(1)
                break
        if channel is None:
            findings.append("rust-toolchain.toml must pin channel to an exact x.y.z version")
    return findings


def main() -> int:
    findings: list[str] = []
    for manifest in sorted(ROOT.rglob("Cargo.toml")):
        if "target" in manifest.parts:
            continue
        findings.extend(scan_manifest(manifest))
    findings.extend(validate_toolchain_pins())

    if findings:
        print("Dependency pin check failed.")
        print("\n".join(findings))
        return 1

    print("Dependency and toolchain pin check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
