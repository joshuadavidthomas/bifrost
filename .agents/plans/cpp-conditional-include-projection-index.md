# Build conditional include projections once per C++ reference file

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document follows `.agents/PLANS.md` and must continue to satisfy it.

## Purpose / Big Picture

Large C++ files should not spend minutes rewalking the same guarded include graph once for every possible declaration source. After this change, forward definition lookup will build one conditional-include projection index for a reference file and answer every donor-header visibility question from that index. The behavior remains the same, but the AMReX rank-31+ replay that stalled after 612 of 622 files should complete with steady progress instead of spending many minutes in `conditional_include_projections_for_source`.

## Progress

- [x] (2026-08-13 08:34Z) Captured a live AMReX stack and reduced issue #2097 to repeated donor-specific conditional include graph traversal.
- [x] (2026-08-13 08:36Z) Read `.agents/PLANS.md`, the visibility cache layout, and the current conditional projection implementation.
- [x] (2026-08-13 08:44Z) Replaced donor-pair memoization with a single-flight per-reference-file projection index.
- [x] (2026-08-13 08:49Z) Added concurrent behavior and deterministic work-count coverage: four donors, two guard paths, a cycle, and an absent donor share one six-state build.
- [x] (2026-08-13 08:55Z) Passed focused featureless validation, formatting, targeted clippy, dependency checks, and diff checks.
- [x] (2026-08-13 09:19Z) Committed as `545778bc`, merged current `origin/master` without rebasing as `03fbff9d`, pushed to `master`, and closed #2097 with focused validation evidence.
- [ ] Rebuild the release runner and rerun pinned AMReX and Tink into a provenance-preserving supplement to finish the C++ ledger.

## Surprises & Discoveries

- Observation: The existing cache does memoize results, but its key is too narrow: `(reference_file, donor_source)`.
  Evidence: `VisibilityIndex::conditional_include_projections_for_source` calls `find_conditional_include_projections` on every cold donor pair. That helper scans the reference tree and calls `conditional_include_requirement_paths`, which walks the reachable include graph solely to find that one donor.

- Observation: The AMReX slowdown is not the project using-index problem fixed by #2088 and not the external system-header parsing problem tracked by #2095.
  Evidence: A live stack showed `conditional_include_projections_for_source -> foreign_declaration_reachable_at_reference -> external_type_candidate_visible_in_context -> resolve_type_node_lexically` while seven workers used full CPU.

- Observation: One traversal state can safely be shared across donors without losing path-sensitive guards.
  Evidence: Include-edge guard sets are merged monotonically. A state is fully described by the reached `ProjectFile` and its merged `HashSet<PreprocessorGuard>`; recording that state for the reached donor before following outgoing includes preserves every distinct compatible guard path and terminates cycles by deduplicating identical states.

- Observation: The existing end-to-end parity fixture already exercises a class/function collision behind a conditional include on both targeted and whole-workspace inverse surfaces.
  Evidence: `usages_cpp_graph_test::cpp_class_inverse_matches_forward_direct_temporary_resolution` remained green after the index replacement, including its `Conditional()` assertions.

## Decision Log

- Decision: Build a map from every reachable donor file to its conditional projections, keyed only by the reference file.
  Rationale: A forward batch asks about many donor declarations from one source file. This product shares the expensive include graph walk and directly matches the query shape.
  Date/Author: 2026-08-13 / Codex

- Decision: Preserve distinct guard states per reached file rather than collapsing to one path or one union.
  Rationale: Mutually exclusive include paths can make the same donor visible under different reference guards. Collapsing them would change correctness; deduplicating equal guard sets removes only redundant work.
  Date/Author: 2026-08-13 / Codex

- Decision: Use a keyed `PoolSafeMemo` cell per reference file, with pool-independent single publication for the iterative build.
  Rationale: `VisibilityIndex` is shared by parallel forward workers. A blocking `OnceLock` can park rayon workers behind a build that needs the same pool, while the projection builder is serial and only reads prepared syntax/include facts, so it can truthfully use the pool-independent memo path. Distinct reference files remain parallel.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation is pushed and #2097 is closed. One reference-file index now serves every donor query, and cold concurrent donor queries publish one build. The exact scale fixture proves one six-state traversal where the former design performed a separate graph build per queried donor. End-to-end conditional include behavior remains unchanged. Completion still requires the AMReX/Tink replay. The key lesson is that caching a whole-graph computation by its final donor query can still be effectively uncached when one reference file asks about hundreds of donors.

The original C++ report already contains 18 durable clean repository rows at Bifrost `3691bb01`. The missing AMReX and Tink rows must be written to a separate supplement at the fixed pushed commit. Appending them to the old JSONL would make one artifact silently mix analyzer revisions. Final acceptance therefore combines the base 18-row report and the two-row supplement while retaining both commit identities and hashes.

## Context and Orientation

The relevant implementation is `crates/bifrost-cpp/src/graph/resolver.rs`. `VisibilityIndex` is a per-query object that owns prepared visibility facts and memoized derived products. A "conditional include projection" is an activation byte plus a set of preprocessor guards under which a source file included another file, directly or transitively. `external_type_candidate_visible_in_context` asks whether a declaration from another file reaches the current reference. It first checks unconditional include activation; if that fails, `foreign_declaration_reachable_at_reference` asks for conditional projections from the reference file to the declaration's source file.

Today `conditional_include_projection_cells` is a mutex-protected map keyed by both files. A miss invokes `find_conditional_include_projections`, which walks every conditional include in the reference file and then invokes `conditional_include_requirement_paths(first, donor_source, initial_guards)`. That second helper explores the include graph until it reaches only the requested donor. Repeating it for many declaration sources produces work proportional to donor count times graph size.

`brokk_bifrost_core::analyzer::PoolSafeMemo` is the repository's single-publication primitive for products reached from rayon workers. The builder in this plan is iterative and does not use rayon. `get_or_build_pool_independent` is therefore the correct call: workers may wait for the one serial builder without starving a computation that needs their pool.

## Plan of Work

In `crates/bifrost-cpp/src/graph/resolver.rs`, replace the pair-keyed `ConditionalIncludeProjectionCache` alias with a per-reference-file map of `PoolSafeMemo<ConditionalIncludeProjectionIndex>` cells. Define `ConditionalIncludeProjectionIndex` next to `ConditionalIncludeProjection`; it maps each reached `ProjectFile` to an immutable `Arc<[ConditionalIncludeProjection]>`.

Change `VisibilityIndex::conditional_include_projections_for_source` so it obtains or creates the reference file's cell, performs one pool-independent build through a new `find_conditional_include_projection_index`, and returns the donor slice from the completed index. Keep a shared empty immutable slice for donors absent from the index.

Implement `find_conditional_include_projection_index` as an iterative traversal. Scan the reference syntax once for conditional `preproc_include` nodes. For every uniquely resolved direct include, seed a state containing the target file, that include's activation byte, and its structured guard environment. Pop states from an explicit stack. Record the state's projection under its current file, deduplicating equal activation/guard pairs. Deduplicate expansion states by current file, activation byte, and exact guard set. Then read the current file's prepared syntax, find its include nodes, merge their guard environments with the accumulated guard set, resolve unique targets, and push unseen compatible states. A cycle terminates because an identical file/activation/guard state is expanded once. Sort each donor's projections by activation byte before converting to `Arc<[...]>`.

Add test-support counters for index builds and expanded include states, initialized in every `VisibilityIndex` constructor. Expose a named test accessor rather than extending the already opaque six-tuple used by using-index tests.

Add low-level behavior/cost tests in the existing resolver test module or the closest current C++ graph test module. Build a small inline include graph with one conditional root, several donors, a cycle, and two distinct guard paths to one donor. Query several donor sources through one visibility index. Assert exact projections, one index build, and a state count bounded by the unique structured states rather than multiplied by donor queries. Add a concurrent query test if the current module can safely construct one shared visibility instance without a new standalone binary.

Run existing guarded include and external type visibility behavior tests to prove no semantic drift. Then rerun pinned AMReX and Tink with the exact corpus command into a separate two-record supplement. Treat the original 18 records and the supplement as one audited selection, but do not merge unlike `bifrost_head` values into one JSONL.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`.

First edit the plan and implementation with `apply_patch`. Format and run the focused unit tests:

    cargo fmt --all
    cargo test -p brokk-bifrost-cpp conditional_include_projection --lib

The exact focused tests used were:

    cargo test -p brokk-bifrost-analysis analyzer::usages::cpp_graph::resolver_tests::tests::conditional_include_projection_index_walks_each_guard_state_once_for_all_donors --lib -- --exact --nocapture
    cargo test -p brokk-bifrost-analysis analyzer::usages::cpp_graph::resolver_tests::tests::unconditional_include_reachability --lib -- --nocapture
    cargo test --test suite_usages -- usages_cpp_graph_test::cpp_class_inverse_matches_forward_direct_temporary_resolution --exact --nocapture

Then run:

    cargo check -p brokk-bifrost-cpp --all-targets
    cargo check -p brokk-bifrost-analysis --all-targets
    cargo clippy -p brokk-bifrost-core -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

After commit and push, rebuild the release runner and rerun only `AMReX-Codes__amrex` and `google__tink` at their pinned clean commits, using the same clone root and limits as the interrupted campaign. Write them under `/mnt/optane/tmp/bifrost-fird/final-03fbff9d/` as a two-row supplement. Expect 18 completed clean base rows at `3691bb01` plus two completed clean supplement rows at `03fbff9d`.

## Validation and Acceptance

The low-level cost test must fail on the old implementation because querying multiple donors causes more than one graph build, and pass after the change with exactly one per-reference index build. Its behavior assertions must prove direct and transitive donors, two guard paths, and cycle termination.

Existing guarded-include resolution tests must retain their exact targets and ambiguity/fail-closed behavior. Compilation and clippy must pass for the language crate and analysis consumers.

The corpus acceptance is observable steady completion. Pinned AMReX must pass file 607 and finish all 622 forward files without reproducing the donor-times-graph tail. Tink must complete afterward. The combined base-plus-supplement evidence must contain 20 clean completed rows and zero repository-level errors, with commit provenance explicit for each artifact.

## Idempotence and Recovery

The code edits and tests are safe to repeat. The projection index belongs to a `VisibilityIndex`, so analyzer update naturally discards it with the query object; no persistent schema or cache migration is involved. If a test or build is interrupted, rerun it normally. If the corpus replay is interrupted, already durable JSONL rows remain valid; select only missing repositories on retry. Never append rows from a different Bifrost revision to an existing report. Do not reset or clean unrelated user changes.

## Artifacts and Notes

The decisive pre-fix trace was:

    #0 ts_node_type
    #1 VisibilityIndex::conditional_include_projections_for_source
    #2 VisibilityIndex::foreign_declaration_reachable_at_reference
    #3 VisibilityIndex::external_type_candidate_visible_in_context
    #4 resolve_type_components_lexically_at_scoped
    #5 resolve_type_node_lexically
    #6 resolve_bare_call_target
    #7 resolve_cpp

Pre-fix AMReX progress:

    forward 606/622 at 146.5s
    forward 607/622 at 375.4s
    forward 610/622 at 558.3s
    forward 612/622 at 1292.7s

Issue: https://github.com/BrokkAi/bifrost/issues/2097

## Interfaces and Dependencies

In `crates/bifrost-cpp/src/graph/resolver.rs`, the completed design should have these internal shapes, with exact field names allowed to follow local style:

    type ConditionalIncludeProjectionIndex =
        HashMap<ProjectFile, Arc<[ConditionalIncludeProjection]>>;

    type ConditionalIncludeProjectionCell =
        Arc<PoolSafeMemo<ConditionalIncludeProjectionIndex>>;

    fn find_conditional_include_projection_index(
        cpp: &dyn CppSource,
        file: &ProjectFile,
        prepared: &PreparedSyntaxTree,
    ) -> ConditionalIncludeProjectionIndex;

`VisibilityIndex::conditional_include_projections_for_source` remains the query-facing method so callers need no semantic changes. It must return an immutable shared slice for the requested donor and must never expose a partially built index.

Plan revision note (2026-08-13): Created after the interrupted clean C++ campaign proved #2097 and a live stack localized the donor-specific guarded include traversal. The plan records the single-index design, pool-safety rule, behavior/cost evidence, and recovery path needed to finish the missing AMReX/Tink records.

Plan revision note (2026-08-13 08:56Z): Updated after implementation and focused validation. It records the exact test names, the concurrent six-state cost result, preserved end-to-end behavior, and the remaining commit/replay work.
