# Bound C conditional declaration attributes at their real terminator

This ExecPlan is a living document maintained under `.agents/PLANS.md`. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

## Purpose / Big Picture

When a C function has an attribute enabled only outside MSVC, Bifrost must still see the function itself in every configuration. tree-sitter-cpp can attach the function and the rest of the file to the attribute's `#ifndef`; after this fix the exact recovered `#endif` before the function declarator ends that guard, so later same-file calls resolve without making genuinely guarded functions visible.

## Progress

- [x] (2026-08-14) Reduced Git `die` bytes 7514..7517 and 8434..8437 and filed #2135.
- [x] (2026-08-14) Recovered the declaration-prefix terminator, required full containment at displaced boundaries, and added the integration regression.
- [x] (2026-08-14) Passed the focused C++ crate, issue integration, callable-activation controls, formatting, Clippy, dependency-boundary, and diff checks; both dirty-runner production candidates are exact and actionable zero.
- [ ] Rebuild at the clean pushed head and replay both production rows into checksummed final evidence.
- [ ] Commit, push, record checksummed evidence, and close #2135.

## Surprises & Discoveries

- Observation: the real `#endif` survives as a structured token, but the old helper selects a later one.
  Evidence: the oversized conditional's first function child contains `attribute_specifier`, then a direct `ERROR` containing `#endif`, then the ordinary storage/type/declarator. The generic displaced-token walk deliberately selects its last candidate, which is wrong for this declaration-prefix shape.
- Observation: testing only the descendant's start against the recovered boundary still marks the function as guarded.
  Evidence: the function node starts at the conditional attribute before the real `#endif`, but its declarator and body cross that terminator. Requiring the entire descendant to end at or before the displaced boundary distinguishes the decorated function from declarations wholly inside a real guard.

## Decision Log

- Decision: prefer an exact error-owned terminator that occurs in the first declaration before its declarator, then fall back to the split-typedef and generic displaced-token recoveries.
  Rationale: declaration ordering proves that the conditional decorates only the prefix. It avoids source scanning and leaves ordinary function-level guards unchanged.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

The structured fix and regression test are implemented. Both Git rows pass the candidate replay as resolved, inverse-exact, and actionable zero; clean pushed-head evidence remains before closure.

## Context and Orientation

`crates/bifrost-cpp/src/declarations.rs::cpp_displaced_preprocessor_boundary` supplies an effective conditional end to declaration materialization and `crates/bifrost-cpp/src/graph/resolver.rs::preprocessor_conditional_contains_descendant`. #2134 added a split-typedef boundary. #2135 adds the sibling declaration-prefix form where a real `#endif` token exists inside an `ERROR` before the callable's declarator.

## Plan of Work

Add a structured helper in `declarations.rs` that examines only the oversized conditional's first declaration or function definition. Find an `ERROR` before the declaration's `declarator` field and accept a non-missing `#endif` token owned by that error. Return its exact end as the highest-priority boundary. Do not traverse function bodies for this specialized decision.

Add an `InlineTestProject` test under `tests/suite_issues/issue_2135_c_conditional_attribute_guard.rs` and register it. Assert both one-argument and two-argument calls to a variadic local function resolve despite the conditional attribute. Retain the existing callable-activation suite as the control for declarations wholly contained in genuine guards.

## Concrete Steps

From `/mnt/optane/bifrost-fird`, run:

    cargo test -p brokk-bifrost-cpp --lib displaced_preprocessor
    cargo test --test suite_issues -- issue_2135
    cargo test --test suite_analyzers -- cpp_callable_activation_visibility
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Then rebuild `bifrost_reference_differential` in release mode and replay both exact Git rows with `--cache-mode ephemeral`.

## Validation and Acceptance

The attribute fixture must resolve the variadic function and the ordinary guarded control must remain `no_definition`. Each production replay must be `completed`, `resolved`, `consistent`, inverse-exact, free of truncation and file errors, and exit with actionable zero.

## Idempotence and Recovery

Tests are read-only and replays use ephemeral caches. Preserve raw outputs until a compact checksummed clean-head manifest exists. Retry interrupted runs to a new revision-specific output path.

## Artifacts and Notes

The source rows originate in `/mnt/optane/tmp/bifrost-fird/final-3643963/c-target-complete-supplement-3643963-raw-ledger.jsonl`.

Plan revision note (2026-08-14): Created after the post-#2134 C audit reduced the conditional-attribute mechanism and filed #2135.

Plan revision note (2026-08-14): Recorded the implemented full-containment rule, focused gate results, and successful candidate replays for Git bytes 7514..7517 and 8434..8437.

## Interfaces and Dependencies

Extend the existing `CppDisplacedPreprocessorBoundary` decision in `brokk-bifrost-cpp`. Add no dependency, crate, schema, epoch, or identity change.
