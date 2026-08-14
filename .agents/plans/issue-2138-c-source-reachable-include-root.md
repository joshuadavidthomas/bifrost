# Resolve C includes through a unique source-reachable inferred root

This ExecPlan is a living document maintained under `.agents/PLANS.md`. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

## Purpose / Big Picture

When a C source includes a project-root-relative header such as `"config/parse.h"`, Bifrost must resolve it even if a generated or tooling subtree contains another file with the same suffix. After this fix, a uniquely source-reachable inferred include root selects the real header, so declarations visible before their later definitions resolve without guessing among unrelated trees.

## Progress

- [x] (2026-08-14) Reduced pgBackRest `cfgParseOptionKeyIdxName` bytes 24353..24377 to a three-file fixture and filed self-assigned #2138.
- [x] (2026-08-14) Added the structured include-root selection and focused unit/integration regressions.
- [x] (2026-08-14) Passed the imports unit tests, issue integration, ambiguous-include controls, C++ crate, formatting, focused Clippy, dependency-boundary, and diff checks; the dirty-runner production candidate is exact and actionable zero.
- [ ] Rebuild at the clean pushed head and replay the production row into checksummed final evidence.
- [ ] Commit, push, preserve checksummed evidence, and close #2138.

## Surprises & Discoveries

- Observation: both the intended header declaration and later source definition are indexed.
  Evidence: `search_symbols` returns `src/config/parse.h:114` and `src/config/parse.c:1364`; a call after the definition resolves, while the call at line 396 before the definition does not.
- Observation: suffix fallback is ambiguous only because the workspace also contains the build tool's `src/build/config/parse.h`.
  Evidence: stripping the literal include `config/parse.h` gives inferred roots `src` and `src/build`; only `src` is an ancestor of the referencing `src/config/parse.c`.

## Decision Log

- Decision: retain direct source-relative and project-relative resolution first, retain the globally unique suffix fallback second, and only for multiple suffix matches accept exactly one candidate whose inferred include root is an ancestor of the source path.
  Rationale: this uses cross-platform `Path` component relations, proves a conventional include-root layout from workspace structure, and remains fail-closed when the source relation does not disambiguate.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

The implementation and regressions are complete. The pgBackRest candidate replay resolves the declaration/definition group, returns a consistent exact inverse hit, and exits with actionable zero; clean pushed-head evidence remains before closure.

## Context and Orientation

`crates/bifrost-cpp/src/imports.rs::resolve_include_targets_with_index` resolves direct paths and then calls `IncludeTargetIndex::resolve_unique_fallback`. The fallback currently knows only the include literal, so it cannot distinguish two suffix candidates using the referencing source. Forward callable visibility consumes this result through the shared include graph.

## Plan of Work

Pass the source file into the fallback. Gather exact path-suffix matches as today. Return the sole match unchanged. For multiple matches, strip the literal include suffix from each candidate path, keep candidates whose inferred root prefixes the source path, and return a candidate only when that filtered set is unique.

Extend the existing imports unit test with a nested decoy and explicit ambiguity controls. Add and register an `InlineTestProject` issue regression containing `src/config/parse.c`, `src/config/parse.h`, and `src/build/config/parse.h`; assert that the call before the same-file definition resolves to the real definition.

## Concrete Steps

From `/mnt/optane/bifrost-fird`, run:

    cargo test -p brokk-bifrost-cpp imports
    cargo test --test suite_issues -- issue_2138
    cargo test --test suite_analyzers -- cpp_imported_code_units
    cargo test -p brokk-bifrost-cpp
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Then rebuild `bifrost_reference_differential` in release mode and replay pgBackRest bytes 24353..24377 with `--cache-mode ephemeral`.

## Validation and Acceptance

The reduced early call must be `resolved` and name the real source definition. Existing ambiguous-basename behavior must remain fail-closed. The production replay must be `completed`, `resolved`, `consistent`, inverse-exact, free of truncation and file errors, and exit with actionable zero.

## Idempotence and Recovery

Tests are read-only and replays use ephemeral caches. Preserve raw outputs until a compact checksummed clean-head manifest exists. Retry interrupted runs to a new revision-specific output path.

## Artifacts and Notes

The raw source row is in `/mnt/optane/tmp/bifrost-fird/final-3643963/c-target-complete-supplement-3643963-raw-ledger.jsonl`. The current exact failure is `/tmp/fird-pgbackrest-cfg-current.jsonl`.

Plan revision note (2026-08-14): Created after the C target-bound audit separated the preceding external-libc `atexit` disposition from the live pgBackRest include-visibility defect.

Plan revision note (2026-08-14): Recorded the implemented source-reachable-root filter, its fail-closed controls, all focused gate results, and the successful candidate replay.

## Interfaces and Dependencies

Change only the shared C/C++ include-target selection. Add no dependency, crate, schema, epoch, or identity change.
