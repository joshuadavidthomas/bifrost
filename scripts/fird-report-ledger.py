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


def selected_tiers(config: dict) -> str:
    tiers = config["tiers"]
    selected = [str(tier) for tier in (1, 2, 3) if tiers[f"tier{tier}"]]
    if not selected:
        raise ValueError("report configuration selects no census tiers")
    return ",".join(selected)


def rerun(
    binary: Path,
    clones_root: Path,
    output: Path,
    record: dict,
    site: dict,
) -> str:
    config = record["report"]["config"]
    args = [
        str(binary), "run-repo",
        "--root", str(clones_root / record["repo_slug"]),
        "--language", record["corpus_language"],
        "--output", str(output),
        "--cache-mode", "ephemeral",
        "--strict",
        "--probe-seed", config["probe_seed"],
        "--tiers", selected_tiers(config),
        "--max-files", str(config["max_files"]),
        "--max-sites", str(config["max_sites"]),
        "--max-candidates-per-file", str(config["max_candidates_per_file"]),
        "--max-source-bytes", str(config["max_source_bytes"]),
        "--max-targets", str(config["max_targets"]),
        "--jobs", str(config["parallelism"]),
        "--max-usage-files", str(config["max_usage_files"]),
        "--max-usages", str(config["max_usages"]),
        "--seed", str(config["seed"]),
        "--path", site["path"],
        "--start-byte", str(site["start_byte"]),
    ]
    if site.get("end_byte") is not None:
        args.extend(["--end-byte", str(site["end_byte"])])
    if config["include_tests"]:
        args.append("--include-tests")
    if config.get("shard") is not None:
        shard = config["shard"]
        args.extend(["--shard", f"{shard['index']}/{shard['count']}"])
    return shlex.join(args)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--clones-root", type=Path, required=True)
    parser.add_argument("--exact-output-root", type=Path, required=True)
    args = parser.parse_args()

    seen = set()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.input.open() as input_stream, args.output.open("x") as output_stream:
        for line_number, line in enumerate(input_stream, 1):
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"invalid JSON on input line {line_number}: {error}") from error
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
                row = {
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
                    "config": report["config"],
                    "site": site,
                    "rerun_command": rerun(
                        args.binary, args.clones_root, exact_output, record, site
                    ),
                    "disposition": None,
                }
                output_stream.write(
                    json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n"
                )
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
                trigger_site = finding.get("trigger_site")
                if trigger_site is None:
                    raise ValueError(
                        "inverse-precision finding lacks trigger_site; regenerate the report "
                        "with the current FIRD runner"
                    )
                exact_output = args.exact_output_root / f"{occurrence_key[:16]}.jsonl"
                row = {
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
                    "config": report["config"],
                    "finding": finding,
                    "rerun_command": rerun(
                        args.binary,
                        args.clones_root,
                        exact_output,
                        record,
                        trigger_site,
                    ),
                    "disposition": None,
                }
                output_stream.write(
                    json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n"
                )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
