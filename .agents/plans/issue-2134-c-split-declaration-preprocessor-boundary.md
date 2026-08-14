# Recover C preprocessor boundaries displaced by split declarations

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept current as work proceeds. This document follows `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

C permits preprocessor branches to select part of a declaration. The C analyzer parses with tree-sitter-cpp, whose recovery can attach every later declaration to that branch when the branch selects a typedef target. Valid calls to functions visibly declared earlier in the same file then answer `no_definition`. After this change, Bifrost will recover the end of that malformed split declaration from tree-sitter structure, stop the invented guard there, and resolve later same-file calls normally. Ordinary guarded declarations and genuinely nested conditionals must remain guarded.

## Progress

- [x] (2026-08-14) Replayed libarchive `Rescale` at bytes 27178..27185 on clean head `cce7ec1e3` and confirmed `actionable=1`.
- [x] (2026-08-14) Proved that `Rescale` is indexed with the correct global identity and activation byte, but callable visibility rejects an invented `Defined("PPMD_32BIT")` guard.
- [x] (2026-08-14) Reduced tree-sitter's exact split-typedef shape and filed #2134.
- [x] (2026-08-14) Added a structured split-declaration boundary recovery and positive/near-miss tests.
- [x] (2026-08-14) Passed the C++ crate suite, focused integration and guard controls, formatting, dependency checks, diff checks, and focused isolated Clippy.
- [ ] Rebuild the release differential runner and replay all issue-owned production rows with ephemeral caches.
- [ ] Record clean checksummed evidence, commit, push to master, and close #2134.

## Surprises & Discoveries

- Observation: the earlier displaced-terminator recovery does not cover this parse.
  Evidence: `cpp_displaced_preprocessor_terminator` finds a concrete non-missing `#endif` below an `ERROR`. In `archive_ppmd8.c`, tree-sitter exposes the real branch terminator only as the end of an `ERROR` inside a malformed declaration; no `#endif` node exists.

- Observation: the indexed declaration and lexical lookup are otherwise exact.
  Evidence: the `Rescale` candidate is a global `Function`, has signature `(CPpmd8 *)`, no type owner, lexical scope `[]`, and activation byte 23676. Only `callable_preprocessor_context_is_visible_for_reference` rejects it.

- Observation: the full parse retains the declaration keyword as an anonymous tree-sitter token even though the surrounding node is `ERROR`.
  Evidence: the immediately preceding top-level `ERROR` has exactly one child of kind `typedef`. Requiring that token distinguishes this split-declaration recovery from an arbitrary malformed declaration inside an ordinary guard.

## Decision Log

- Decision: recover a boundary only from a structurally proven split-declaration shape and reuse it everywhere that consumes displaced conditional ends.
  Rationale: weakening guard subset checks would make genuinely conditional declarations visible. Source-text searching would violate the analyzer's structured-resolution contract. A shared boundary keeps declaration materialization and callable visibility consistent.
  Date/Author: 2026-08-14 / Codex

- Decision: retain the existing concrete-token recovery and add a second boundary form rather than changing `_module_` identities, callable naming, or lexical lookup.
  Rationale: all identity and lexical facts are already correct; the defect is exclusively conditional range provenance.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

The structured fix and tests are implemented. A candidate-runner exact replay of libarchive `Rescale` now resolves one target, returns the exact inverse hit, and exits with `actionable=0`. The C++ crate's 87 tests, five complete/incomplete guard-family controls, seven callable-activation controls, the new integration test, formatting, workspace dependency check, diff check, and focused isolated Clippy all pass. Completion still requires committing, pushing, rebuilding from the clean commit, and preserving a checksummed clean replay.

## Context and Orientation

`crates/bifrost-cpp/src/declarations.rs` extracts C and C++ declarations. Its `cpp_displaced_preprocessor_terminator` helper recovers the real end of a conditional when tree-sitter consumes a concrete `#endif` token inside an `ERROR`. `crates/bifrost-cpp/src/graph/resolver.rs` uses that helper through `preprocessor_conditional_contains_descendant` to decide which guards apply to declarations and references. The declaration collector also uses it to bound `MaterializationRecord::ConfigurationConditional` ranges.

The failing source begins a `typedef`, then places `#ifdef PPMD_32BIT`, `#else`, and `#endif` between the type and typedef name. Tree-sitter leaves the leading `typedef` as a preceding top-level `ERROR`, makes the conditional's first body child a malformed declaration containing its own `ERROR`, and incorrectly pairs the conditional with a much later file-level `#endif`. The malformed declaration ends at the real typedef name and supplies a safe upper bound even though no terminator token survives.

A “guard” here is the condition such as `Defined("PPMD_32BIT")` that must be true for a declaration to exist. An “activation byte” is the source position after which that declaration is nameable. The fix changes only which source interval a guard covers.

## Plan of Work

In `crates/bifrost-cpp/src/declarations.rs`, represent a displaced conditional boundary by its end byte and end line rather than requiring a returned tree-sitter token. Preserve the existing concrete error-owned `#endif` path. Add a conservative structured path for a conditional displaced by a split declaration: require the malformed leading/declaration relationship that tree-sitter produces, require an error-bearing first declaration ending well before the conditional's parsed terminator, and use that declaration end as the recovered boundary. Do not inspect source delimiters with regular expressions, splitting, or substring searches.

Update the declaration materialization range and `preprocessor_conditional_contains_descendant` in `crates/bifrost-cpp/src/graph/resolver.rs` to consume the shared boundary. Add low-level parser tests for the split typedef and near misses: an ordinary guarded malformed declaration stays inside its guard, a real complete `#if/#else` family keeps its direct terminator, and a damaged nested conditional must not truncate its outer family.

Add an `InlineTestProject` integration regression under `tests/suite_issues/` and register it in that suite's `main.rs`. The fixture must contain a split typedef followed by a local function, a later call, and an ordinary guarded decoy. Assert that the later call resolves exactly to the local function while the guarded decoy does not become an unconditional candidate.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`. Edit with `apply_patch`, then run:

    cargo test -p brokk-bifrost-cpp --lib displaced_preprocessor
    cargo test --test suite_issues -- issue_2134
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Build the durable runner with:

    cargo build --release --bin bifrost_reference_differential

Replay the exact libarchive command from occurrence key `59417a75f33dce8e33e60f9be962a82a7599cd1c6da9d6c3ea7feff75c90c8d3`, changing only its output path and retaining `--cache-mode ephemeral`.

## Validation and Acceptance

The new low-level test must fail before the implementation because the recovered boundary is absent, then pass with a boundary no later than the malformed split declaration's end. Near misses must prove that normal conditional visibility remains conservative. The integration test must return `resolved` for the later local call and must not select a declaration confined to an unrelated guard.

The exact production replay is accepted only when its one site is `consistent`, forward lookup is `resolved`, inverse lookup returns the exact call range, `actionable=0`, and every truncation, candidate-cap, membership, usage-limit, and file-error indicator is clean. Focused Clippy, formatting, dependency boundaries, and diff checks must pass before push.

## Idempotence and Recovery

All tests and exact replays are repeatable. Exact replays use ephemeral cache mode and revision-specific output files, so they do not modify corpus caches. Keep earlier raw ledgers and debug outputs until a compact checksummed clean-head manifest preserves the evidence. If a build or replay is interrupted, rerun it to a new output path rather than overwriting accepted evidence.

## Artifacts and Notes

The original target-bound C ledger is `/mnt/optane/tmp/bifrost-fird/final-3643963/c-target-complete-supplement-3643963-raw-ledger.jsonl`. The `Rescale` occurrence key is `59417a75f33dce8e33e60f9be962a82a7599cd1c6da9d6c3ea7feff75c90c8d3`. Current-head diagnostic replays are `/tmp/issue-c-rescale-debug.jsonl`, `/tmp/issue-c-rescale-debug2.jsonl`, and `/tmp/issue-c-rescale-debug3.jsonl`; they are diagnostic only because their runner contained temporary logging.

Plan revision note (2026-08-14): Created after the target-bound C audit proved a distinct split-declaration conditional-boundary mechanism and filed #2134.

Plan revision note (2026-08-14): Updated after implementation and candidate validation. The final predicate requires the structured `typedef` token, multiline declarator recovery, trailing declarator name, and later over-captured children; this was the smallest rule that matched production without weakening ordinary guards.

## Interfaces and Dependencies

Keep the recovery in `brokk-bifrost-cpp`; no new dependency, crate, epoch bump, cache schema change, or analysis-layer API is required. The shared helper should return enough boundary information for both `CppVisitor` materialization ranges and graph resolver containment checks. It must remain based on tree-sitter nodes and existing `Range`/position types.
