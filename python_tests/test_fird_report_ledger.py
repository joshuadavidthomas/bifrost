from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "fird-report-ledger.py"


class FirdReportLedgerTest(unittest.TestCase):
    def test_preserves_config_and_reruns_inverse_precision_from_trigger(self) -> None:
        config = {
            "corpus_language": "rust",
            "max_files": 17,
            "max_sites": 23,
            "max_candidates_per_file": 29,
            "max_source_bytes": 31,
            "max_targets": 37,
            "parallelism": 2,
            "max_usage_files": 41,
            "max_usages": 43,
            "seed": 47,
            "include_tests": True,
            "exact_site": None,
            "probe_seed": "census",
            "tiers": {"tier1": True, "tier2": False, "tier3": True},
            "shard": {"index": 2, "count": 3},
        }
        missing = {
            "path": "src/lib.rs",
            "start_byte": 10,
            "end_byte": 16,
            "classification": "missing",
            "tier": None,
            "syntactic_shape": "call_expression>identifier",
        }
        precision = {
            "signature": "inverse_precision:direct:target",
            "path": "src/lib.rs",
            "start_byte": 30,
            "end_byte": 36,
            "line": 3,
            "kind": "direct",
            "snippet": "target()",
            "expected_names": ["target"],
            "targets": [],
            "trigger_site": {
                "path": "src/lib.rs",
                "start_byte": 10,
                "end_byte": 16,
            },
        }
        record = {
            "status": "completed",
            "corpus_language": "rust",
            "repo_slug": "fixture__repo",
            "repo_head": "repo-head",
            "bifrost_head": "bifrost-head",
            "run_fingerprint": "fingerprint",
            "report": {
                "config": config,
                "sites": [missing],
                "inverse_precision_findings": [precision],
            },
        }

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            input_path = root / "report.jsonl"
            output_path = root / "ledger.jsonl"
            input_path.write_text(json.dumps(record) + "\n")
            subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--input",
                    str(input_path),
                    "--output",
                    str(output_path),
                    "--binary",
                    "/opt/fird",
                    "--clones-root",
                    "/opt/clones",
                    "--exact-output-root",
                    str(root / "exact"),
                ],
                check=True,
            )
            rows = [json.loads(line) for line in output_path.read_text().splitlines()]

        self.assertEqual(
            [row["finding_class"] for row in rows],
            ["forward_inverse", "inverse_precision"],
        )
        for row in rows:
            self.assertEqual(row["config"], config)
            command = row["rerun_command"]
            self.assertIn("--max-files 17", command)
            self.assertIn("--max-sites 23", command)
            self.assertIn("--tiers 1,3", command)
            self.assertIn("--include-tests", command)
            self.assertIn("--shard 2/3", command)
            self.assertIn("--path src/lib.rs --start-byte 10 --end-byte 16", command)

    def test_refuses_to_overwrite_an_existing_raw_ledger(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            input_path = root / "empty.jsonl"
            output_path = root / "ledger.jsonl"
            input_path.write_text("")
            output_path.write_text("preserve me\n")
            completed = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--input",
                    str(input_path),
                    "--output",
                    str(output_path),
                    "--binary",
                    "/opt/fird",
                    "--clones-root",
                    "/opt/clones",
                    "--exact-output-root",
                    str(root / "exact"),
                ],
                text=True,
                capture_output=True,
            )
            preserved = output_path.read_text()

        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(preserved, "preserve me\n")


if __name__ == "__main__":
    unittest.main()
