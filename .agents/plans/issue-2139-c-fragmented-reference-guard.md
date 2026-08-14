# Recover C reference guards across statement fragments

This ExecPlan is a living document maintained under `.agents/PLANS.md`. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

## Purpose / Big Picture

When a preprocessor conditional selects an `else if` fragment inside a C function, calls in that fragment must inherit the physical guard even if tree-sitter-cpp cannot make the conditional their normal ancestor. After this fix, an identically guarded helper is visible without making the helper visible from unguarded or contradictory configurations.

## Progress

- [x] (2026-08-14) Reduced four libarchive `string_to_size` failures to a statement-fragment conditional and filed self-assigned #2139.
- [x] (2026-08-14) Recovered the physical reference guard through structured syntax and added positive, unguarded, and contradictory tests.
- [x] (2026-08-14) Passed the low-level guard test, issue integration, callable-activation controls, C++ crate, formatting, focused Clippy, dependency-boundary, and diff checks; all four dirty-runner production candidates are exact and actionable zero.
- [x] (2026-08-14) Replayed the two issue-owned rows from clean integrated head `d0efa72bb` into checksummed final evidence.
- [x] (2026-08-14) Committed, pushed, preserved checksummed evidence, and closed #2139 at `https://github.com/BrokkAi/bifrost/issues/2139#issuecomment-5291594906`.

## Surprises & Discoveries

- Observation: guard text, macro state, declaration indexing, and arity are not the differentiators.
  Evidence: the same helper declaration and `HAVE_ONE && HAVE_TWO` condition resolve when the call's conditional is wholly nested in a block. Moving the opener before `} else if (...) {` alone changes the result to `no_definition`.
- Observation: the malformed structural pair remains exact enough to recover without line scanning.
  Evidence: tree-sitter emits one `preproc_if` with a missing `#endif` as the consequence's last named child, and emits the real terminator after the reference as a `preproc_call` whose structured `directive` field is `#endif`.

## Decision Log

- Decision: recover the reference environment from tree-sitter nodes and conditional-family structure, while retaining the existing declaration-guard subset check.
  Rationale: the semantic fact is physical conditional containment. Weakening subset matching would expose genuinely configuration-specific declarations, and source-line scanning would duplicate the parser.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

The structured pair recovery and regressions landed in `6ba7704de` and were pushed at integrated head `d0efa72bb`. Both issue-owned libarchive rows pass from that clean head as resolved, consistent, inverse-exact, and actionable zero. The compact manifest is `/mnt/optane/tmp/bifrost-fird/final-d0efa72b/issue-2139-clean-replay-manifest-d0efa72b.jsonl` (SHA-256 `5f38c16718293c900ae66354d68717cf1e0c35328b0f68f32b018b4c5ab93b03`), and its checksum inventory has SHA-256 `179798a8c0bcbea9f26e2cb9f8cabc6d357e69c5651bda26fb9088dc3bb1037c`.

## Context and Orientation

`crates/bifrost-cpp/src/graph/resolver.rs::preprocessor_guard_environment` walks ordinary ancestors, and `CallableReferenceContext::guards` uses it for callable visibility. `callable_preprocessor_context_is_visible_for_reference` then requires every declaration guard in that reference environment. The lost fact occurs before this subset check.

## Plan of Work

Inspect the exact tree-sitter shape for the reduced fragment. Add the smallest shared structured recovery that finds a conditional physically governing the reference even when parser recovery moves the reference out of ordinary ancestry. Reuse `preprocessor_guard_for_descendant`, displaced-boundary rules, and conditional-family structure where applicable.

Add and register an `InlineTestProject` regression for the fragmented positive. Include an unguarded call and a call under the negated condition as near misses that must not resolve to the guarded helper. Add a low-level guard-environment control if it clarifies the parser shape without duplicating the integration assertion.

## Concrete Steps

From `/mnt/optane/bifrost-fird`, run:

    cargo test -p brokk-bifrost-cpp fragmented_reference_guard
    cargo test --test suite_issues -- issue_2139
    cargo test --test suite_analyzers -- cpp_callable_activation_visibility
    cargo test -p brokk-bifrost-cpp
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Then rebuild `bifrost_reference_differential` in release mode and replay libarchive bytes 9313..9327 and 9605..9619 with `--cache-mode ephemeral`.

## Validation and Acceptance

The fragmented guarded call must resolve. Unguarded and contradictory references must remain unresolved. Each production replay must be `completed`, `resolved`, `consistent`, inverse-exact, free of truncation and file errors, and exit with actionable zero.

## Idempotence and Recovery

Tests are read-only and replays use ephemeral caches. Preserve raw outputs until a compact checksummed clean-head manifest exists. Retry interrupted runs to a new revision-specific output path.

## Artifacts and Notes

The two issue-owned raw rows are in `/mnt/optane/tmp/bifrost-fird/final-3643963/c-target-complete-supplement-3643963-raw-ledger.jsonl`. Current exact failures are `/tmp/fird-libarchive-string-size-9313-current.jsonl` and `/tmp/fird-libarchive-string-size-9605-current.jsonl`.

Plan revision note (2026-08-14): Created after exact replay separated the libarchive target-bound residue into independent guard and overload families.

Plan revision note (2026-08-14): Recorded the exact missing-terminator/structured-`preproc_call` AST pair, implemented recovery, focused validation, and successful candidate replays.

Plan revision note (2026-08-14): Finalized clean-head evidence and closure. The release runner SHA-256 is `692b96a28e54933f80ba41929b36f1096da39af23734c2ad0f632bcb38f149f2`; raw report SHA-256 values are `948a0c7597a8ed57d329ad106f414d05691f6d12928d33f9f7af28c22d31111d` and `1c1e283029b61608f4bbba9033f0c7fb66d735880d92b2b8b8274b6c3468a81d`.

## Interfaces and Dependencies

Change only shared C/C++ preprocessor reference visibility. Add no dependency, crate, schema, epoch, or identity change.
