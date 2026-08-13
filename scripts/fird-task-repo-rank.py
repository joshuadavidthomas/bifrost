#!/usr/bin/env python3
"""Select FIRD corpus repositories through brokkbench tasks.py.

This helper is intentionally small. It calls tasks.task_repos with
tasks.SFT_PREDICATES, then uses a stable descending task-count sort. Stable
sorting keeps tasks.py's own order as the tie-break. SFT_PREDICATES has
not_overlarge enabled, so large-repos.csv members cannot enter the result.
"""

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--brokkbench-root", type=Path, required=True)
    parser.add_argument("--commits-root", type=Path, required=True)
    parser.add_argument("--languages", required=True)
    parser.add_argument("--top", type=int, default=None)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    root = args.brokkbench_root.resolve()
    sys.path.insert(0, str(root))
    import tasks  # noqa: E402

    if not tasks.SFT_PREDICATES.not_overlarge:
        raise RuntimeError("tasks.SFT_PREDICATES must exclude large-repos.csv")

    ranking = {}
    for language in args.languages.split(","):
        rows = tasks.task_repos(
            tasks.SFT_PREDICATES,
            commits_dir=args.commits_root.resolve(),
            langs=[language],
        )
        rows = sorted(rows, key=lambda row: -row.task_count)
        selected = rows if args.top is None else rows[: args.top]
        ranking[language] = [
            {"slug": row.repo_slug, "task_count": row.task_count} for row in selected
        ]

    payload = json.dumps(ranking, sort_keys=True) + "\n"
    if args.output is None:
        sys.stdout.write(payload)
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
