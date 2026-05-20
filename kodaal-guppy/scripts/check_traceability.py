#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROJECT_STATUS = ROOT / "Docs" / "Status" / "PROJECT_STATUS.md"
TRACEABILITY = ROOT / "Docs" / "Status" / "TRACEABILITY.md"
STATUS_SYMBOLS = ("🟢", "🟡", "🔴", "⚪")


def status_symbol(value: str) -> str:
    for symbol in STATUS_SYMBOLS:
        if symbol in value:
            return symbol
    raise ValueError(f"missing status symbol in {value!r}")


def cells(line: str) -> list[str]:
    return [part.strip() for part in line.strip().strip("|").split("|")]


def requirement_rows(path: Path) -> dict[str, str]:
    rows: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not re.match(r"^\|\s*(FR|NFR)-\d+\s*\|", line):
            continue
        parts = cells(line)
        req_id = parts[0]
        if req_id.startswith("FR-"):
            rows[req_id] = status_symbol(parts[2] if path == PROJECT_STATUS else parts[3])
        else:
            rows[req_id] = status_symbol(parts[2])
    return rows


def project_status_counts() -> tuple[Counter[str], Counter[str], dict[str, Counter[str]]]:
    fr_counts: Counter[str] = Counter()
    nfr_counts: Counter[str] = Counter()
    declared = {"fr": Counter(), "nfr": Counter()}
    summary_mode: str | None = None

    for line in PROJECT_STATUS.read_text(encoding="utf-8").splitlines():
        if line.startswith("| Status | Count |"):
            summary_mode = "fr"
            continue
        if line.startswith("| NFR Status | Count |"):
            summary_mode = "nfr"
            continue
        if summary_mode and line.startswith("| **Total"):
            summary_mode = None
            continue
        if summary_mode and re.match(r"^\|\s*[🟢🟡🔴⚪]", line):
            parts = cells(line)
            declared[summary_mode][status_symbol(parts[0])] = int(parts[1])
            continue
        if re.match(r"^\|\s*FR-\d+\s*\|", line):
            fr_counts[status_symbol(cells(line)[2])] += 1
        if re.match(r"^\|\s*NFR-\d+\s*\|", line):
            nfr_counts[status_symbol(cells(line)[2])] += 1

    return fr_counts, nfr_counts, declared


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail on drift")
    parser.parse_args()

    findings: list[str] = []
    fr_counts, nfr_counts, declared = project_status_counts()
    for symbol in STATUS_SYMBOLS:
        if declared["fr"][symbol] != fr_counts[symbol]:
            findings.append(
                f"PROJECT_STATUS FR summary {symbol} declares {declared['fr'][symbol]}, actual {fr_counts[symbol]}"
            )
        if declared["nfr"][symbol] != nfr_counts[symbol]:
            findings.append(
                f"PROJECT_STATUS NFR summary {symbol} declares {declared['nfr'][symbol]}, actual {nfr_counts[symbol]}"
            )

    status_rows = requirement_rows(PROJECT_STATUS)
    trace_rows = requirement_rows(TRACEABILITY)
    missing_in_trace = sorted(set(status_rows) - set(trace_rows))
    missing_in_status = sorted(set(trace_rows) - set(status_rows))
    if missing_in_trace:
        findings.append(f"TRACEABILITY missing IDs: {', '.join(missing_in_trace)}")
    if missing_in_status:
        findings.append(f"PROJECT_STATUS missing IDs: {', '.join(missing_in_status)}")
    for req_id in sorted(set(status_rows) & set(trace_rows)):
        if status_rows[req_id] != trace_rows[req_id]:
            findings.append(
                f"{req_id} status drift: PROJECT_STATUS={status_rows[req_id]} TRACEABILITY={trace_rows[req_id]}"
            )

    if findings:
        print("Traceability check failed.")
        print("\n".join(findings))
        return 1

    print("Traceability check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
