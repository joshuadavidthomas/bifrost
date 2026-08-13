#!/usr/bin/env python3
"""Convert FIRD repository envelopes into an exact-site finding ledger."""

import argparse
import hashlib
import json
import shlex
from pathlib import Path


def signature(parts: list[str]) -> str:
    payload = "\0".join(parts).encode()
    return hashlib.sha256(payload).hexdigest()


def rerun(binary: Path, clones_root: Path, output: Path, record: dict, site: dict) -> str:
    args = [
        str(binary), "run-repo",
        "--root", str(clones_root / record["repo_slug"]),
        "--language", record["corpus_language"],
        "--output", str(output),
        "--cache-mode", "ephemeral",
        "--strict",
        "--probe-seed", record["report"]["config"]["probe_seed"],
        "--tiers", "1,2,3",
        "--path", site["path"],
        "--start-byte", str(site["start_byte"]),
        "--end-byte", str(site["end_byte"]),
    ]
    return shlex.join(args)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--clones-root", type=Path, required=True)
    parser.add_argument("--exact-output-root", type=Path, required=True)
    args = parser.parse_args()

    rows = []
    seen = set()
    for line in args.input.read_text().splitlines():
        record = json.loads(line)
        if record.get("status") != "completed":
            continue
        report = record["report"]
        for site in report["sites"]:
            if site["classification"] != "missing":
                continue
            finding_class = "census_gap" if site.get("tier") else "forward_inverse"
            parts = [
                finding_class,
                str(site.get("tier") or 0),
                record["corpus_language"],
                site.get("syntactic_shape") or "unknown",
            ]
            occurrence = [
                record["corpus_language"], record["repo_slug"], record["repo_head"],
                site["path"], str(site["start_byte"]), str(site["end_byte"]),
            ]
            occurrence_key = signature(occurrence)
            if occurrence_key in seen:
                continue
            seen.add(occurrence_key)
            exact_output = args.exact_output_root / f"{occurrence_key[:16]}.jsonl"
            rows.append({
                "schema_version": 1,
                "finding_class": finding_class,
                "signature": signature(parts),
                "signature_parts": parts,
                "occurrence_key": occurrence_key,
                "language": record["corpus_language"],
                "repo_slug": record["repo_slug"],
                "repo_head": record["repo_head"],
                "bifrost_head": record["bifrost_head"],
                "run_fingerprint": record["run_fingerprint"],
                "site": site,
                "rerun_command": rerun(
                    args.binary, args.clones_root, exact_output, record, site
                ),
                "disposition": None,
            })
        for finding in report.get("inverse_precision_findings", []):
            parts = [
                "inverse_precision", record["corpus_language"],
                finding["kind"], "|".join(finding["expected_names"]),
            ]
            occurrence = [
                record["corpus_language"], record["repo_slug"], record["repo_head"],
                finding["path"], str(finding["start_byte"]), str(finding["end_byte"]),
                "inverse_precision",
            ]
            occurrence_key = signature(occurrence)
            if occurrence_key in seen:
                continue
            seen.add(occurrence_key)
            rows.append({
                "schema_version": 1,
                "finding_class": "inverse_precision",
                "signature": signature(parts),
                "signature_parts": parts,
                "occurrence_key": occurrence_key,
                "language": record["corpus_language"],
                "repo_slug": record["repo_slug"],
                "repo_head": record["repo_head"],
                "bifrost_head": record["bifrost_head"],
                "run_fingerprint": record["run_fingerprint"],
                "finding": finding,
                "rerun_command": None,
                "disposition": None,
            })

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w") as stream:
        for row in rows:
            stream.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
