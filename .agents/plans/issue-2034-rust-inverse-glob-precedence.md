# Make Rust inverse lookup honor glob-import precedence

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document follows `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

After this change, finding usages of a Rust function declared in a file will continue to find direct and closure-contained calls when the file also has a glob import that exposes another function with the same name. The lower-precedence glob will not make the call ambiguous or steal it. Explicit imports and glob imports in files without a competing local declaration will continue to resolve normally. The behavior is visible in a focused `suite_usages` integration test and in an exact replay of the serde-json production witness from issue #2034.

## Progress

- [x] (2026-08-13 22:33Z) Read issue #2034 and trace targeted Rust inverse lookup from `UsageFinder` through `usage_reference_at`.
- [x] (2026-08-13 22:37Z) Added a pre-fix failing `InlineTestProject` regression covering direct calls, closure calls, an explicit alias, an explicit-import-vs-glob collision, and a glob-only caller.
- [x] (2026-08-13 22:40Z) Preserved import provenance in `RustOriginRoute` and applied glob precedence in the shared structured resolver without weakening mutually exclusive configuration alternatives.
- [x] (2026-08-13 22:49Z) Ran focused Rust usage tests, all 53 Rust language-crate tests, formatting, clippy, the dependency check, and the exact serde-json replay.
- [x] (2026-08-13 22:50Z) Prepared the implementation and this completed living plan for the user-requested pause-point commit.
- [ ] After the user resumes: push the checkpoint and close issue #2034 with the recorded evidence.

## Surprises & Discoveries

- Observation: closure syntax is not part of the failure. `usage_reference_at` initially unions every visible import-origin route with same-file declarations, and a glob edge is lowered to a named route before precedence is considered.
  Evidence: `crates/bifrost-rust/src/usage.rs` builds `matches` from `origin_routes` before adding identities returned by `identities_in_file_named`; `RustOriginRoute` currently does not retain whether its source edge was a glob.

- Observation: the whole-workspace per-file scanner already tries an exact same-file nonmember before its general reference context, while the target-oriented scanner uses `usage_reference_at`. The correction belongs in the shared seed resolver so target-oriented inverse lookup and any other seed consumer agree.
  Evidence: `crates/bifrost-rust/src/graph/inverted.rs::RustScan::bare_callee_in_namespace` calls `exact_local_bare_callee` first, while `crates/bifrost-analysis/src/analyzer/usages/rust_graph/extractor.rs::ScanCtx::matches_resolved_identifier` calls `usage_reference_at`.

- Observation: the existing #1377 mutually exclusive configuration test remains exact for both declarations after glob precedence is applied.
  Evidence: `cargo test --test suite_usages -- usages_rust_graph_test::issue_1377_cfg_alternatives_keep_both_inverse_targets --exact` passed.

## Decision Log

- Decision: retain an `is_glob` fact on each `RustOriginRoute` and rank routes using structured import provenance.
  Rationale: the import graph already knows whether an edge came from `use path::*`. Preserving that fact avoids name or source-text heuristics and lets the resolver distinguish a lower-precedence glob from an explicit named import.
  Date/Author: 2026-08-13 / Codex

- Decision: test one fixture with a same-file declaration plus glob, an explicit aliased import, and a separate glob-only module.
  Rationale: this matrix proves both sides of the precedence rule. A patch that rejects every colliding glob would pass the production positive but incorrectly lose legitimate imported calls.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation now distinguishes glob-origin routes from explicit imports, applies local and explicit precedence only when candidates can be active together, and retains the existing configuration-alternative behavior. The new behavior test passes across the complete acceptance matrix. The historical serde-json site at bytes 3775..3797 changed from `missing` to `consistent`, with an exact inverse hit and zero actionable findings. Focused validation is green. Publication and issue closure remain only because the user requested a pause immediately after the next commit.

## Context and Orientation

Rust usage lookup starts from a target `CodeUnit`, the repository's indexed declaration record. `crates/bifrost-analysis/src/analyzer/usages/rust_graph/extractor.rs` scans candidate files for references to that target. It builds `RustBindingSeeds`, which describe the target and all import paths that can expose it, and asks `crates/bifrost-rust/src/usage.rs::usage_reference_at` whether a written path at a byte offset resolves exactly to a seed.

`RustUsageWalks::origin_routes_of` in `crates/bifrost-rust/src/usage_walks.rs` converts import edges into `RustOriginRoute` values. A glob import is an import such as `use crate::rounding::*`; it supplies many names but has lower Rust name-resolution precedence than an explicit declaration in the importing module. Today the route stores its path, scope, namespace, origin, visibility, and configuration guard, but not whether the edge was a glob. `usage_reference_at` therefore treats the imported function and a same-file function as peers and returns `Ambiguous`, causing inverse lookup to omit a forward-resolved call.

The existing configuration-guard logic must be preserved. Mutually exclusive `#[cfg(...)]` declarations are alternatives, not simultaneous competitors. The existing local explicit-import behavior must also remain: a named `use` inside a function body can shadow an enclosing item. The implementation should therefore remove only glob-origin candidates that overlap a higher-precedence candidate; it must not flatten all imports into a single rule.

## Plan of Work

First add `tests/suite_usages/issue_2034_rust_inverse_glob_precedence.rs` and register it in `tests/suite_usages/main.rs`. Build a small Cargo project with an imported one-argument function, a same-file three-argument function, direct and closure calls to the local function, an explicit alias call to the imported function, and another module whose only source of the one-argument name is the glob. Query both declarations and compare exact source ranges, excluding import syntax from the external usage surface.

Next extend `RustOriginRoute` in `crates/bifrost-rust/src/usage.rs` with the structured fact that its source edge is a glob, and populate it in `RustUsageWalks::origin_routes_of` in `crates/bifrost-rust/src/usage_walks.rs`. In `usage_reference_at`, retain the current candidates and guard conditions, but treat non-glob import routes and visible same-file declarations as higher precedence than glob-only identities. Remove a glob-only identity only when a higher-precedence candidate can be active at the same time. Preserve candidates whose guard sets are proven mutually exclusive so the existing #1377 configuration-alternative behavior remains exact for both targets. A function-local explicit named import remains authoritative over the enclosing declaration; a function-local glob does not acquire that stronger status.

Finally run the new focused test, the existing #1377 configuration and local-import controls, all `brokk-bifrost-rust` crate tests, formatting, focused featureless clippy, and the exact serde-json witness with ephemeral cache. Record concise output in this plan, commit only the plan, implementation, and tests, push the current branch, then close #2034 with the commit and validation evidence.

## Concrete Steps

Run all commands from `/mnt/optane/bifrost-fird`.

Add and exercise the regression:

    cargo test --test suite_usages -- issue_2034_rust_inverse_glob_precedence:: --nocapture

Before the production correction, expect the same-file target to omit the two colliding call ranges. Afterward, expect the test to pass and the imported target to contain only its explicit-alias and glob-only call ranges.

Run focused non-regressions and crate tests:

    cargo test --test suite_usages -- usages_rust_graph_test::issue_1377_cfg_alternatives_keep_both_inverse_targets --exact
    cargo test --test suite_usages -- usages_rust_graph_test::issue_1377_function_scoped_import_beats_enclosing_function_name --exact
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs

Use the reference-differential command embedded in the #2034 serde-json ledger record with `--cache-mode ephemeral`. The exact `round_nearest_tie_even` call must change from `forward_inverse` to `consistent` with an exact inverse range.

Observed validation transcript:

    issue_2034_rust_inverse_glob_precedence ... ok
    issue_1377_cfg_alternatives_keep_both_inverse_targets ... ok
    issue_1377_function_scoped_import_beats_enclosing_function_name ... ok
    rust_graph_strategy_prefers_local_declaration_over_glob_reexport ... ok
    brokk-bifrost-rust: 53 passed; 0 failed
    workspace dependency graph is valid
    done rust serde-rs__json: actionable=0 elapsed=0.3s

The exact replay report recorded `classification: consistent`, `inverse_hit.exact_range: true`, and `inverse_precision_unbacked_hits: 0`.

## Validation and Acceptance

The integration test must prove that the local three-argument declaration owns both the direct and closure-contained call tokens despite the same-spelled glob import. The imported one-argument declaration must not claim those tokens, but must still own a call through an explicit alias and a call in a module with no local declaration. Existing mutually exclusive `#[cfg]` alternatives must remain usages of their respective declarations, and a function-scoped explicit named import must continue to shadow an enclosing declaration.

The exact production replay must report no actionable finding for the serde-json witness. Formatting, focused clippy, the dependency-boundary checker, and all language-crate tests must pass before pushing.

## Idempotence and Recovery

The test and validation commands are safe to repeat. The production change alters only derived in-memory route facts and resolution; it does not migrate persisted data or write corpus repositories. If a test exposes a configuration-guard regression, keep the failing evidence, restore the candidate set, and apply precedence only after separating glob and higher-precedence guard sets. Do not replace the structured route distinction with text scanning.

## Artifacts and Notes

Issue #2034 production shape:

    use super::rounding::*;
    fn round_nearest_tie_even(/* local three-argument form */) { ... }
    let closure = || round_nearest_tie_even(/* three arguments */);

Forward definition selects the local function. Before this change, target-oriented inverse lookup returns no call because it sees the local and glob-imported functions as equal candidates.

## Interfaces and Dependencies

`RustOriginRoute` remains an internal Rust language-crate value. It gains a structured glob-provenance field populated from `RustImportEdgeKind::Glob`. No new crate or dependency is needed. `usage_reference_at` continues returning `RustReferenceResolution::{Exact, Ambiguous, Unresolved}`; only its candidate precedence changes. The implementation must keep iterative graph walks and current cache boundaries intact.

Plan revision note (2026-08-13 22:33Z): created the living plan after tracing the failure to import-origin candidate precedence and before adding the regression.

Plan revision note (2026-08-13 22:50Z): recorded the completed structured fix, full focused validation, exact corpus evidence, and the user-requested pause boundary. Push and issue closure are deliberately left for resumption.
