#!/usr/bin/env python3
from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
UI = ROOT / "core" / "src" / "ui" / "index.html"


class StaticA11yParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.failures: list[str] = []
        self.stack: list[dict[str, object]] = []
        self.html_lang = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attr = {key: value or "" for key, value in attrs}
        if tag == "html" and attr.get("lang"):
            self.html_lang = True
        if tag == "img" and not attr.get("alt"):
            self.failures.append("static <img> is missing alt text")
        if tag == "button":
            self.stack.append(
                {
                    "tag": tag,
                    "named": bool(attr.get("aria-label") or attr.get("title")),
                    "text": "",
                }
            )
        else:
            self.stack.append({"tag": tag})

    def handle_data(self, data: str) -> None:
        for item in reversed(self.stack):
            if item.get("tag") == "button":
                item["text"] = str(item.get("text", "")) + data
                break

    def handle_endtag(self, tag: str) -> None:
        while self.stack:
            item = self.stack.pop()
            if item.get("tag") != tag:
                continue
            if tag == "button":
                text = str(item.get("text", "")).strip()
                if not item.get("named") and not text:
                    self.failures.append("static <button> is missing text, aria-label, or title")
            break


def main() -> int:
    html = UI.read_text(encoding="utf-8")
    parser = StaticA11yParser()
    parser.feed(html)
    failures = list(parser.failures)
    if not parser.html_lang:
        failures.append("<html> is missing lang")
    required_markers = {
        "keyboard listener": "addEventListener(\"keydown\"",
        "ARIA labels": "aria-label",
        "semantic roles": "role=\"",
        "focus styling": ":focus",
        "bulk select control": "id=\"select-all\"",
    }
    for label, marker in required_markers.items():
        if marker not in html:
            failures.append(f"missing {label} marker")
    if failures:
        print("Static UI accessibility gate failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("Static UI accessibility gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
