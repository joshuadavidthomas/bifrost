# Resolve Rust bare names by physical module scope

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Rust bare-name lookup currently compares a reference and same-file declarations using the analyzer's logical ownership graph when that graph contains a module ancestor. Logical ownership is useful for symbol identity, but it is not Rust's syntax visibility boundary. An `impl` can be physically written in `child.rs` for a type declared in `owner.rs`, and top-level statics can be logically owned by the synthetic `_module_` unit. In both cases a local helper, type, constructor, or static must be found from the physical file and inline-module scope where the reference is written.

After this change, `get_definitions_by_location` resolves bare names inside such an `impl` to declarations physically written in `child.rs`, while rejecting same-name declarations in `owner.rs`, sibling files, and nested inline modules. The focused integration test and exact FIRD replays of all issue #2031 production rows demonstrate that forward resolution and inverse usage recovery agree at the original byte ranges.

## Progress

- [x] (2026-08-13 23:10Z) Read the repository instructions, FIRD runbook, campaign plans, handoff, current resolver implementation, shared inline test harness, and neighboring Rust regressions.
- [x] (2026-08-13 23:12Z) Added the `InlineTestProject` regression under `tests/suite_issues/`; the unchanged resolver failed its first impl-local `helper` reference with `no_indexed_definition`.
- [x] (2026-08-13 23:13Z) Replaced logical module comparison in `rust_current_module_candidates` with physical syntax-module comparison over every declaration range, preserving role filtering and the later ownership guard.
- [x] (2026-08-13 23:15Z) Ran the focused issue test and the four named regression controls; all five tests pass.
- [x] (2026-08-13 23:18Z) Ran the Rust crate tests, formatting, focused isolated Clippy, dependency-boundary check, and diff check. All 53 Rust crate tests pass and every other gate is clean.
- [x] (2026-08-13 23:26Z) Built the release FIRD runner and exactly replayed all 12 issue-owned production rows with ephemeral caches. All 12 are resolved, consistent, byte-exact inverse hits with zero actionable findings and zero file errors.
- [ ] Commit only issue #2031 files with a detailed multiline message, push the current branch to `master`, and close #2031 with evidence.

## Surprises & Discoveries

- Observation: No implementation discovery has changed the handed-off diagnosis yet.
  Evidence: At `c4268c4f`, `rust_current_module_candidates` first walks `parent_of` to select a logical module and consults `rust_declaration_syntax_module_range` only when no logical reference module exists.
- Observation: A first production probe accidentally ran while the release link step was still active and therefore executed the preceding binary.
  Evidence: The stale probe left `Error` unresolved; after waiting for the final rebuild, the binary checksum changed to `6c4899f95d018e5b51654945bd85c1cdd5d2927c2774267f9183f29f51a0977e` and all 12 fresh outputs passed. The stale output is isolated outside `exact-final` and is not part of accepted evidence.
- Observation: Non-root module statics retain their synthetic identity beneath the physical module package.
  Evidence: The fixture and production replays resolve `TOKEN` as `issue_2031.child._module_.TOKEN`, `serde_json.number._module_.TOKEN`, and `serde_json.raw._module_.TOKEN`; the fix changes visibility selection, not identity construction.

## Decision Log

- Decision: Keep symbol identities and the later ownership guard unchanged; alter only the current-module visibility comparison.
  Rationale: `_module_` and cross-file ownership identities serve other analyzer invariants. The defect is that those identities were reused as a syntax visibility boundary, not that the identities themselves are wrong.
  Date/Author: 2026-08-13, Codex.
- Decision: Compare the reference's enclosing inline `mod` range with every recorded range for each same-file declaration.
  Rationale: A declaration can have multiple source ranges, and any same-scope range is sufficient to make it a physical current-module candidate. Looking only at the first range can reject a legitimate declaration because of range ordering.
  Date/Author: 2026-08-13, Codex.
- Decision: Do not bump an analysis epoch.
  Rationale: This change modifies resolver selection over already indexed declarations and ranges. It does not change persisted declaration rows, identities, or schema.
  Date/Author: 2026-08-13, Codex.

## Outcomes & Retrospective

The resolver now uses physical file/inline-module syntax scope for current-module candidates and checks all recorded declaration ranges. The new public-tool fixture proves helpers, unit-struct constructors, and a synthetic module static inside an impl for another module's type, with owner-file and nested-inline-module decoys plus free-function controls. The red baseline failed on the first impl-local helper; the corrected implementation passes.

Local acceptance is clean: the issue test and four named controls pass, `brokk-bifrost-rust` passes 53/53 plus doctests, formatting is clean, focused isolated Clippy for `brokk-bifrost-rust` and `brokk-bifrost-analysis` is clean, the workspace dependency graph is valid, and `git diff --check` is clean. The 12 pinned serde-json production rows all changed from `no_definition` to resolved/consistent with exact inverse hits and zero actionable findings. Publication and issue closure remain.

## Context and Orientation

The implementation is in `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs`. `rust_current_module_candidates` gathers same-file declarations matching a bare reference, filters them by Rust namespace role, filters them to the current module, and finally applies an ownership guard for nested declarations. `lexical_scope::enclosing_mod_item_range_at` returns the byte range of the inline `mod` item physically containing a byte position, or `None` for the file's top-level module. `rust_declaration_syntax_module_range` performs the corresponding calculation for a declaration; a module declaration itself belongs to its parent syntax module.

The existing defect is the current-module filter. It builds `enclosing` from `IAnalyzer::enclosing_code_unit` and `parent_of`, chooses the first logical module, and then compares candidate logical module ancestry whenever such a module exists. An implementation block physically in `child.rs` can be logically owned by a type declared in `owner.rs`, so this comparison admits the owner's declarations or rejects the child's declarations even though Rust bare-name visibility follows `child.rs` syntax.

The new behavior test belongs in `tests/suite_issues/issue_2031_rust_physical_module_scope.rs` and must be registered by one `mod` line in `tests/suite_issues/main.rs`. It uses `tests/common/inline_project.rs::InlineTestProject`, builds a small Cargo package with `src/lib.rs`, `src/owner.rs`, and `src/child.rs`, and queries the public `get_definitions_by_location` tool. `owner.rs` declares `External` and same-name decoys. `child.rs` declares a local trait implemented for `owner::External`, a module helper, `Here`, and `TOKEN`; references inside the foreign-type impl and free functions must select those physical declarations. A nested inline module supplies same-name near misses that must not leak outward. A sibling file is optional because the required three-file fixture already exercises cross-file decoys through `owner.rs`.

The production evidence comes from the 2,787-row immutable Rust ledger at `/mnt/optane/tmp/bifrost-fird/final-63a1912a/rust-ranks31-44-63a1912a-raw-ledger.jsonl`, whose SHA-256 is `aeaabdd187d57a791d9ae9278b1da665c94670f17f79dc40bfc8703857239bd1`. All 12 rows owned by issue #2031 must be recovered without mutating that ledger and replayed with `--cache-mode ephemeral` using a freshly rebuilt release runner. Representative sites include `Error` in `src/error.rs` byte 217, `visit_array` in `src/value/de.rs` byte 298, `eq_str` in `src/value/partial_eq.rs` byte 33, `TOKEN` in `src/number.rs` byte 484, and `HEX0` in `src/read.rs` byte 1077.

## Plan of Work

First add the three-file integration fixture. Query each local bare name inside the foreign-type implementation and the free-function controls. Assert a single resolved definition with the expected `child` identity and path. Query nested-module and cross-file decoys as negative controls where needed. Run only this test before editing product code and retain its failing transcript in this plan.

Then edit `rust_current_module_candidates`. Preserve construction of `enclosing`, because the later ownership guard needs it. Remove the `reference_module` logical-ancestry selection and the candidate `parent_of` walk. Always calculate `reference_syntax_module` with `lexical_scope::enclosing_mod_item_range_at(root, reference_start)`. For each role-compatible same-file candidate, accept it when any range from `analyzer.ranges(candidate)` has `rust_declaration_syntax_module_range(root, range, candidate.is_module()) == reference_syntax_module`. Keep `rust_role_accepts_current_module`, sorting, deduplication, and the subsequent ownership guard unchanged.

After focused validation, extract the 12 issue-owned ledger rows into a durable issue-specific manifest below `/mnt/optane/tmp/bifrost-fird/`, preserving occurrence keys, repository pins, paths, ranges, and source evidence. Build `target/release/bifrost_reference_differential` from the clean implementation head and run each exact-site command with a unique output and `--cache-mode ephemeral`. Every row must finish as resolved and consistent, contain the original exact inverse range, and contribute zero actionable findings. Record checksums for the extracted manifest and replay output before committing.

## Concrete Steps

Run all commands from `/mnt/optane/bifrost-fird`.

Create and register the regression, then establish the red result:

    cargo test --test suite_issues -- issue_2031_rust_physical_module_scope

After the resolver edit, rerun that command and the existing controls:

    cargo test --test suite_symbols -- rust_bare_values_inside_impl_prefer_module_constants_over_associated_constants
    cargo test --test suite_symbols -- rust_current_module_item_beats_glob_import_for_scoped_owner
    cargo test --test suite_symbols -- rust_unimported_inline_module_type_does_not_guess_same_file_identifier
    cargo test --test suite_symbols -- rust_bare_name_does_not_cross_independent_cargo_example_targets

Run the requested local gates:

    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

Use the exact commands generated from the immutable ledger, with issue-specific output paths and `--cache-mode ephemeral`. Inspect each completed envelope with `jq`; require `forward_status == "resolved"`, `classification == "consistent"`, an inverse hit whose nested exact range matches the input path and bytes, no file or explicit-limit invalidation, and `summary.actionable == 0`. Compute SHA-256 checksums for the issue manifest and combined exact output.

Finally update this plan, stage only the plan, resolver, new test, and suite registration, and commit on the current branch. Push the current branch directly to `origin/master`. Comment on #2031 with the commit, test evidence, exact replay count and checksums, then close it. Add the publication state to this plan in a follow-up checkpoint if needed before continuing to #2035.

## Validation and Acceptance

The new integration test must fail before the resolver edit by selecting a logical-owner decoy or failing to resolve a child declaration. After the edit, every bare `helper`, `Here`, constructor `Here`, and `TOKEN` reference inside the impl must resolve to the physical `child.rs` declaration. Free-function controls must remain correct. Same-name declarations in `owner.rs` and a nested inline module must not win.

The four named existing regressions must pass unchanged. All tests in `brokk-bifrost-rust`, formatting, focused Clippy for both touched crates, and the workspace dependency check must pass. All 12 production rows must be exact, resolved, consistent inverse hits with zero actionable findings under an ephemeral cache and a release runner built from the implementation head.

## Idempotence and Recovery

The test and validation commands are safe to repeat. Do not overwrite the immutable full Rust ledger. Generate a separate issue-owned manifest and new head-scoped exact output. Exact replays use in-memory analyzer stores and therefore do not alter corpus caches. If a replay is interrupted, keep completed evidence and rerun into a new output or use the runner's append-safe semantics only after verifying the existing completion keys. Do not remove campaign temporary artifacts until the final compact manifests preserve their required evidence.

Git staging must name exact files; never use `git add -A`. Do not change branches. If publication fails, retain the local commit and retry the non-destructive push after inspecting local and remote heads.

## Artifacts and Notes

Expected changed repository files are:

    .agents/plans/issue-2031-rust-physical-module-scope.md
    crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs
    tests/suite_issues/issue_2031_rust_physical_module_scope.rs
    tests/suite_issues/main.rs

Durable campaign evidence remains outside the repository below `/mnt/optane/tmp/bifrost-fird/` until final campaign closure. Issue-specific artifacts are:

    /mnt/optane/tmp/bifrost-fird/issue-2031-c4268c4f/issue-2031-raw-ledger.jsonl
      12 rows; SHA-256 a99d7d66b7a9e67e36e3d7694d1f3766d0c4e1d383af60e0477cdeee59c367f9
    /mnt/optane/tmp/bifrost-fird/issue-2031-c4268c4f/issue-2031-exact-replay.jsonl
      12 completed/resolved/consistent/exact rows; actionable 0; file errors 0
      SHA-256 677f1476191439b7b666301cfdc308774d997ff901342c525f098a0f6011cab6
    target/release/bifrost_reference_differential
      pre-commit validation binary SHA-256 6c4899f95d018e5b51654945bd85c1cdd5d2927c2774267f9183f29f51a0977e

The accepted replay uses serde-json `827a315bf2198558f0325b07bcc1e2cd973aba2f`, ephemeral caches, the original paths and byte ranges, and Bifrost source at `c4268c4f` with the uncommitted issue patch (`bifrost_dirty=true`). A clean published-head replay can be added before issue closure if publication metadata is required.

## Interfaces and Dependencies

No public interface or dependency changes are required. The existing internal functions keep their signatures. The relevant interfaces are `IAnalyzer::ranges`, `IAnalyzer::parent_of`, `lexical_scope::enclosing_mod_item_range_at`, `rust_declaration_syntax_module_range`, and `rust_role_accepts_current_module`. The test uses only `InlineTestProject` and the public `get_definitions_by_location` search tool.

Plan revision, 2026-08-13: Created the living issue #2031 implementation plan from the confirmed production diagnosis and current clean checkout. It records the required structured fix, behavior fixture, regression controls, local gates, exact replay contract, publication workflow, and recovery rules so work can resume from this file alone.

Plan revision, 2026-08-13 (implementation complete): Recorded the red/green behavior proof, the physical-range implementation, all focused and crate-level validation, the recovered 12-row manifest, the exact production replay, and the stale-binary operational discovery. Publication and issue closure are the only remaining steps.
