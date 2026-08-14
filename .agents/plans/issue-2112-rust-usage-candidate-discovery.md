# Optimize Rust usage candidate discovery for real-project scans

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document follows `.agents/PLANS.md`.

## Purpose / Big Picture

Rust usage queries currently spend their full time budget on large repositories such as Qdrant and Starship. After this work, the query will use persisted import and usage facts to avoid a whole-workspace source-text fallback when those facts provide a sound candidate universe, reuse immutable per-file scan preparation where practical, and emit phase timings plus candidate counts under `BIFROST_TIMING=1`. The issue's three UsageBench cases must finish within their 60-second per-scan budget without losing required references or partial-result cancellation behavior.

## Progress

- [x] (2026-08-13 19:15Z) Read issue #2112, fetched current `origin/master`, and verified the attached issue branch starts at `331b676a3` with a clean worktree.
- [x] (2026-08-13 19:25Z) Located the duplicate candidate-discovery boundary in `effective_scan_files` and the per-file preparation stages in both Rust scan paths.
- [x] (2026-08-13 20:05Z) Added stable phase instrumentation and candidate-count notes for seed inference, importer/reference/include discovery, prepared syntax, reference context, lexical scope construction, and AST walking.
- [x] (2026-08-13 20:20Z) Confirmed the persisted binding-seed importer walk, module-reference route, include expansion, and target source form the structured fallback candidate universe used by the original graph algorithm and current persisted fact design.
- [x] (2026-08-13 21:15Z) Profiled the pinned Qdrant case and found that duplicate Rust candidate augmentation before graph dispatch consumed 53.2 of the 59.9 seconds spent in candidate discovery.
- [x] (2026-08-13 22:05Z) Removed the duplicate outer augmentation and moved the structured importer/reference union into the Rust graph, where inferred seeds are already available.
- [x] (2026-08-13 22:20Z) Restored the textual fallback for genuinely empty inferred scopes after the pinned Qdrant fixture demonstrated that deleting it outright was not sound.
- [x] (2026-08-13 22:35Z) Added behavior regressions for default discovery and graph-local augmentation of a nonempty supplied scope.
- [x] (2026-08-13 23:30Z) Added a bounded generation-lifetime cache for verified import edges by symbol identity; the pinned Qdrant query completed in 52.8 seconds with all four required locations and zero runner errors.
- [x] (2026-08-14 00:10Z) Narrowed nonempty class/function scopes to verified seed importers instead of all Cargo-reachable files, while preserving the broad module-target route required by module-qualifier semantics.
- [x] (2026-08-14 00:25Z) Validated all three named cases without `time_budget`, with all required locations and zero runner errors; `read_disk_usage` passed exactly.
- [x] (2026-08-14 00:40Z) Ran the complete 12-case Rust slice with a writable shared cache and observed zero runner errors.
- [x] (2026-08-14 00:55Z) Ran `cargo fmt --check`, all 246 focused Rust graph tests, the two issue-specific tests, and strict all-features workspace Clippy successfully.
- [x] (2026-08-14 00:25Z) Reproduced the named Qdrant and Starship cases and ran the complete non-release UsageBench Rust slice at the fix revision.
- [x] (2026-08-14 00:55Z) Made checkpoint commits for instrumentation, graph-local discovery, binding-edge caching, and verified-importer narrowing; kept this plan current with measured evidence.

## Surprises & Discoveries

- Observation: `UsageFinder` performs language candidate discovery before the Rust graph scan, but an empty non-authoritative result causes `effective_scan_files` to read every analyzed Rust file and search it with `str::contains`.
  Evidence: `crates/bifrost-analysis/src/analyzer/usages/finder.rs` builds `UsageScanScope` from language candidates; `crates/bifrost-analysis/src/analyzer/usages/rust_graph/extractor.rs::effective_scan_files` enters `textual_candidates` only when that scope is empty.

- Observation: the fallback is not only a discovery cost. Each textual match enters a parallel semantic scan that obtains prepared syntax and reference context, constructs `RustLexicalScopeIndex`, derives binding names, and walks the AST.
  Evidence: `scan_files_for_target` and `scan_files_for_member_target` in `extractor.rs` repeat those stages per candidate file.

- Observation: the original Rust project-graph implementation selected only graph importers plus the target source when no explicit candidates were supplied. The later textual fallback expanded that structured set rather than preserving an older correctness route.
  Evidence: history at commit `86f438680e` shows `effective_scan_files` returning `graph.usage_graph.importers_of_seeds(seeds)` chained with the target source; current `usage_importers` is the persisted-fact equivalent.

- Observation: the outer language augmentation duplicated Rust seed and importer discovery before the Rust graph repeated those operations. On the pinned Qdrant case this was the timeout, not the semantic AST scan.
  Evidence: the debug profile reported `usages::candidate_discovery=59928.2ms`, `RustQueryResolver::usage_candidates=53166.7ms`, and never entered the graph before the 60-second budget. Removing only the outer augmentation reduced `usages::candidate_discovery` to `3790.9ms` and allowed graph dispatch.

- Observation: removing the textual fallback entirely was unsound. The pinned `UnsizedHandler` case then omitted the required cross-crate `positions.rs` reference when generic discovery supplied a nonempty but incomplete scope.
  Evidence: the release-binary UsageBench run completed within budget but returned 3 of 4 required locations. This prompted the final design: union structured Rust importers inside the graph for every non-authoritative scope, and retain text discovery only when no generic candidates exist.

- Observation: after removing duplicate outer augmentation, cold binding-seed discovery remained the dominant graph-local cost because the same verified identity-to-import-edge result was recomputed through re-export propagation.
  Evidence: before the cache the release profile reported `binding_seed_discovery=67320.9ms`. With the bounded identity cache, the pinned query reported `binding_seed_discovery=28864.8ms`, total backend time `52786.2ms`, all 4 required locations, and no `time_budget` diagnostic.

## Decision Log

- Decision: Instrument before changing candidate semantics, with stable phase labels and count notes rather than per-file dynamic labels.
  Rationale: The acceptance criteria require diagnosable profiles, and stable aggregate labels keep timing output useful without adding high-cardinality noise or meaningful disabled-path overhead.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not solve the budget by truncating candidate files.
  Rationale: Issue #2112 explicitly requires semantic correctness and forbids silently omitting candidates. Any narrower route must be justified by persisted import/usage facts or a structural completeness argument.
  Date/Author: 2026-08-13 / Codex

- Decision: Remove Rust's outer `candidate_augmentation` and perform the structured importer/reference/include union inside `effective_scan_files` after graph seeds have been inferred once.
  Rationale: The outer hook duplicated the dominant Rust traversal before graph dispatch. Graph-local augmentation preserves candidates that generic discovery cannot see while sharing the query's already-computed binding seeds.
  Date/Author: 2026-08-13 / Codex

- Decision: Retain whole-workspace text discovery only for an empty non-authoritative supplied scope.
  Rationale: A nonempty scope can be safely enlarged with structured Rust facts. An empty scope still represents unresolved forms whose completeness is not yet proved by those facts; the Qdrant fixture showed that deleting this fallback could lose required references.
  Date/Author: 2026-08-13 / Codex

- Decision: Cache verified import edges by `RustSymbolIdentity` in `RustWalkCaches`.
  Rationale: The value depends on analyzer-generation state and is reused by transitive seed propagation. The existing generation-owned weighted cache boundary supplies correct invalidation and bounded memory without introducing reference counting outside the established cache design.
  Date/Author: 2026-08-13 / Codex

- Decision: For a nonempty inferred scope, add only `RustBindingSeeds::verified_importer_files` for non-module targets; retain the broader Cargo-reachable expansion for module targets and empty scopes.
  Rationale: Verified binding edges are the structured completeness supplement for named declarations. Module qualifiers can be referenced through namespace routes that do not bind the terminal declaration name, as pinned by existing module-qualifier tests, so they retain the established broader route.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

The implementation eliminates the duplicate pre-graph traversal, memoizes repeated verified binding-edge discovery, and avoids scanning every Cargo-reachable file for named targets when generic discovery already supplied candidates. The three named issue cases complete without time-budget partials and retain every required location. The complete 12-case Rust slice reports zero runner errors; remaining non-passing cases are pre-existing precision expectations rather than runner failures.

## Context and Orientation

`UsageFinder` in `crates/bifrost-analysis/src/analyzer/usages/finder.rs` first asks a language-specific provider for candidate files and wraps them in `UsageScanScope`. Rust dispatch then enters `RustQueryResolver::find_usages` in `crates/bifrost-analysis/src/analyzer/usages/rust_graph.rs`. It infers graph seeds, derives binding seeds, calls `effective_scan_files`, and scans the result through functions in `crates/bifrost-analysis/src/analyzer/usages/rust_graph/extractor.rs`.

A graph seed is a declaration identity that the resolver considers equivalent to the requested target, including re-export identities. A binding seed is the transitive set of import and alias identities derived from those roots. Persisted Rust usage facts are content-addressed rows produced during analysis and queried by `brokk-bifrost-rust`; they record modules, imports, bindings, and related usage information without reparsing source. A candidate universe is sound when every file that could semantically reference the target is included; it may contain false positives, but it must not contain false negatives.

The profiler in `crates/bifrost-core/src/profiling.rs` prints nested spans and notes when `BIFROST_TIMING` is enabled. Disabled `scope_with` and `note_with` calls avoid allocating labels.

## Plan of Work

First, add spans around the aggregate discovery stages in `effective_scan_files` and around each aggregate per-file scan stage. Emit candidate counts after the graph/import/include/textual sets are materialized. Keep cancellation checks at the same or finer granularity.

Second, trace `usage_candidate_files_while`, `usage_importers`, include-route facts, and `referencing_files_of` to establish their completeness boundaries. Add focused tests that construct a large inline Rust project with many files containing a common spelling but no structural import route, alongside direct imports, renamed imports, glob imports, re-exports, local declarations, macro uses, and included files. The tests must assert both returned usages and the analyzer's candidate-scan counter.

Third, remove duplicate outer Rust augmentation and change `effective_scan_files` to union structured importers and references with any generic candidates. Retain whole-workspace text discovery only for an empty inferred scope, where current facts do not establish completeness. Do not use a file cap, regex, or arbitrary heuristic.

Fourth, profile repeated per-file setup. Reuse an existing analyzer-owned immutable prepared tree or reference context rather than duplicating it. Add a new bounded generation-lifetime cache only if measurements show reconstruction remains material and the cached value is valid across every target query.

Finally, validate locally and against the pinned UsageBench corpus. Record exact Bifrost and UsageBench revisions, commands, candidate counts, phase timings, completion reason, found locations, and runner errors in this plan.

## Concrete Steps

From the repository root, run focused tests during development:

    cargo fmt
    cargo test --test suite_usages usages_rust_graph_test --no-default-features

Run broader Rust usage suites selected by the changed behavior, followed by:

    cargo clippy --workspace --all-targets --all-features -- -D warnings

For all-features validation, first check disk space and use `scripts/with-isolated-cargo-target.sh` if isolation is needed. Do not create a manually named Cargo target directory.

Run the real-project reproduction with `BIFROST_TIMING=1`, the UsageBench corpus at `benchmarks/cases/evaluation/real-project-v2`, and one declaration per process with a 60-second scan budget. The exact command will be added after inspecting the current UsageBench runner interface.

## Validation and Acceptance

Existing Rust usage correctness tests must pass. New focused tests must prove that default discovery retains imported usages and that the graph augments a nonempty supplied scope with structured Rust importers. Cancellation tests must continue returning proven partial hits and typed `time_budget` or cancellation metadata.

The final profile must report counts and elapsed time for seed inference, importer discovery, textual fallback if any, prepared syntax, reference context, lexical scope construction, and AST scanning. Qdrant cases `real-project-v2-rust-02-1` and `real-project-v2-rust-02-2`, plus Starship case `real-project-v2-rust-01-1`, must complete without `reason=time_budget` under the documented 60-second configuration. The complete non-release Rust slice must report zero runner errors.

## Idempotence and Recovery

Instrumentation and tests are safe to rerun. Analyzer caches introduced by this work must be bounded and generation-owned so updates retire stale values automatically. The UsageBench reproduction must use its pinned corpus checkout and avoid modifying corpus repositories. If a long all-features build is interrupted, the isolated-target helper will clean its target directory.

## Artifacts and Notes

Issue baseline: Bifrost v0.9.3 at `30dacd4778b9e042bf55ed5e519e8780293f07a1`; UsageBench at `f06f3b810770dbc6c41b7f2bee5f5d1e5c07f774`. Observed failures were Qdrant `UnsizedHandler` and `read_disk_usage` with 1 of 4 required locations, and Starship `StarshipConfig` with 7 of 9, all after exhausting 60 seconds with zero runner errors.

Final corpus evidence used the release binary and `--scan-usages-max-duration-secs 60`. `real-project-v2-rust-02-2` passed exactly with 4 TP, 0 FP, and 0 FN. `real-project-v2-rust-02-1` returned all 4 required locations with 5 existing extra hits; `real-project-v2-rust-01-1` returned all 9 required locations with 4 existing extra hits. None had an incomplete diagnostic. The complete 12-case Rust slice used `BIFROST_CACHE_DIR=/private/tmp/usagebench-2112.hovy1e/writable-cache` and reported 0 runner errors.

## Interfaces and Dependencies

Use the existing `brokk_bifrost_core::profiling::{scope, scope_with, note_with}` interface. Candidate discovery must remain based on `RustBindingSeeds`, `usage_candidate_files_while`, `usage_importers`, analyzer import/reference facts, and `RustIncludeRoutes`. Continue using `PreparedSyntaxTree`, `RustReferenceContext`, and `RustLexicalScopeIndex` for semantic scanning. No new crate or external dependency is expected.

Revision note (2026-08-13): Created the plan after live issue and source inspection so implementation can proceed from a self-contained performance and correctness contract.

Revision note (2026-08-13): Recorded aggregate phase instrumentation and the initial structured-fallback hypothesis after the first implementation milestone.

Revision note (2026-08-13): Corrected the first fallback-removal hypothesis after the pinned Qdrant case exposed a false negative; recorded the measured duplicate outer-augmentation bottleneck and final graph-local union design.

Revision note (2026-08-14): Recorded the verified-importer narrowing, final named-case results, and zero-error complete Rust slice.
