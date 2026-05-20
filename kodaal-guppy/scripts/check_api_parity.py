#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
API_SPEC = ROOT / "Docs" / "Specs" / "API_SPEC.md"
OPENAPI_SPEC = ROOT / "core" / "openapi.yaml"
HTTP_METHODS = {"get", "post", "put", "patch", "delete", "options", "head", "trace"}


def normalize_path(path: str) -> str:
    path = path.strip().strip("`")
    path = re.sub(r"\?.*$", "", path)
    path = re.sub(r":([A-Za-z_][A-Za-z0-9_]*)", r"{\1}", path)
    path = re.sub(r"/\*$", "/{path}", path)
    path = re.sub(r"/+", "/", path)
    if len(path) > 1:
        path = path.rstrip("/")
    return path


def route_key(method: str, path: str) -> tuple[str, str]:
    return method.upper(), normalize_path(path)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise SystemExit(f"missing required file: {path.relative_to(ROOT)}")


def parse_markdown_routes(path: Path) -> set[tuple[str, str]]:
    text = read_text(path)
    routes: set[tuple[str, str]] = set()
    heading = re.compile(
        r"^#{2,6}\s+`?\s*(GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD|TRACE)\s+([^`\s]+)",
        re.IGNORECASE | re.MULTILINE,
    )
    for match in heading.finditer(text):
        routes.add(route_key(match.group(1), match.group(2)))
    return routes


def parse_openapi_with_yaml(path: Path) -> set[tuple[str, str]] | None:
    try:
        import yaml  # type: ignore[import-untyped]
    except ImportError:
        return None

    document = yaml.safe_load(read_text(path))
    if not isinstance(document, dict):
        raise SystemExit(f"invalid OpenAPI document: {path.relative_to(ROOT)}")
    paths = document.get("paths")
    if not isinstance(paths, dict):
        raise SystemExit(f"OpenAPI document has no paths map: {path.relative_to(ROOT)}")

    routes: set[tuple[str, str]] = set()
    for raw_path, operations in paths.items():
        if not isinstance(raw_path, str) or not isinstance(operations, dict):
            continue
        for method in operations:
            if isinstance(method, str) and method.lower() in HTTP_METHODS:
                routes.add(route_key(method, raw_path))
    return routes


def parse_openapi_without_yaml(path: Path) -> set[tuple[str, str]]:
    lines = read_text(path).splitlines()
    in_paths = False
    paths_indent: int | None = None
    current_path: str | None = None
    current_path_indent: int | None = None
    routes: set[tuple[str, str]] = set()

    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        indent = len(line) - len(line.lstrip(" "))
        if not in_paths:
            if stripped == "paths:":
                in_paths = True
                paths_indent = indent
            continue

        if paths_indent is not None and indent <= paths_indent:
            break

        path_match = re.match(r"^(/[^:]*):\s*$", stripped)
        if path_match:
            current_path = path_match.group(1)
            current_path_indent = indent
            continue

        if current_path is None or current_path_indent is None:
            continue
        if indent <= current_path_indent:
            current_path = None
            current_path_indent = None
            continue

        method_match = re.match(r"^([A-Za-z]+):\s*$", stripped)
        if method_match and method_match.group(1).lower() in HTTP_METHODS:
            routes.add(route_key(method_match.group(1), current_path))

    return routes


def parse_openapi_routes(path: Path) -> set[tuple[str, str]]:
    return parse_openapi_with_yaml(path) or parse_openapi_without_yaml(path)


def format_routes(routes: set[tuple[str, str]]) -> str:
    return "\n".join(f"  {method} {path}" for method, path in sorted(routes))


def main() -> int:
    docs_routes = parse_markdown_routes(API_SPEC)
    openapi_routes = parse_openapi_routes(OPENAPI_SPEC)

    missing_from_openapi = docs_routes - openapi_routes
    missing_from_docs = openapi_routes - docs_routes

    if missing_from_openapi or missing_from_docs:
        print("API route parity check failed.")
        if missing_from_openapi:
            print("\nRoutes in Docs/Specs/API_SPEC.md but missing from core/openapi.yaml:")
            print(format_routes(missing_from_openapi))
        if missing_from_docs:
            print("\nRoutes in core/openapi.yaml but missing from Docs/Specs/API_SPEC.md:")
            print(format_routes(missing_from_docs))
        return 1

    print(f"API route parity check passed: {len(docs_routes)} routes matched.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
