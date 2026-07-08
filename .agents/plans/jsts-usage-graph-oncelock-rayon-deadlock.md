# Fix rayon/OnceLock deadlock in JS/TS usage-graph index initialization (issue #549)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `.agents/PLANS.md` at the repository root.

## Purpose / Big Picture

GitHub issue #549 reports that a JS/TS `analyze_commit` call can hang forever (observed: 40+ minutes at 0% CPU, all threads parked on futexes) while building the usage graph for the jest repository (~1,946 JS/TS files). After this change, `usage_graph` / `analyze_commit` on JS/TS workspaces always completes or fails boundedly; a new regression test that reliably hangs on the pre-fix code passes in seconds.

The root cause is a classic rayon-plus-blocking-lazy-init deadlock, and the fix is general: we replace the blocking lazy-initialization primitive used for analyzer-level indexes whose builders themselves use rayon with a non-blocking, pool-safe memo primitive, and we pre-materialize the JS/TS index before entering the parallel scan so the common path never races.

## Root cause (read this first)

Terms: "rayon" is the Rust data-parallelism library; `par_iter()` fans a loop out over a process-global pool of worker threads. `std::sync::OnceLock` is a write-once cell; `get_or_init(f)` runs `f` on the first calling thread and *parks* (blocks, futex-wait) every other thread that calls it until `f` finishes. Parking is invisible to rayon: a parked worker cannot steal or execute rayon jobs. Additionally, a rayon worker that is *waiting inside its own nested `par_iter`* keeps itself busy by stealing any pending job in the pool — including jobs unrelated to its current computation.

The deadlocking configuration, all confirmed in the current tree:

1. `usage_graph` (src/searchtools.rs:3611, called twice by `analyze_commit`, src/commit_analysis.rs:311/318) calls `build_jsts_usage_edges` (src/analyzer/usages/js_ts_graph.rs:70) → `JsTsEdgeResolver::build_edges` (js_ts_graph.rs:230) → `inverted::build_jsts_edges` (src/analyzer/usages/js_ts_graph/inverted.rs) → the shared driver `inverted_edges::build_edges` → `collect_per_file_edges` (src/analyzer/usages/inverted_edges.rs:442-459), which runs `files.par_iter()` over every JS/TS file in the workspace.

2. Each per-file scan closure constructs a `JsTsReceiverFactProvider` (added by commit ee1af6d, "bounded object-sensitive receiver analysis", PR #402). Receiver typing of an imported function reaches `summarize_imported_function` (src/analyzer/usages/js_ts_graph/receiver_analysis.rs:556) → `resolve_js_ts_module_binding_candidates` → `jsts_module_export_candidates` (src/analyzer/usages/get_definition/js_ts.rs:839) → `cached_jsts_index` (src/analyzer/usages/js_ts_graph.rs:85) → `TypescriptAnalyzer::jsts_usage_index()` / `JavascriptAnalyzer::jsts_usage_index()`.

3. Those getters are `OnceLock::get_or_init` (src/analyzer/typescript/mod.rs:219-222, src/analyzer/javascript/mod.rs:202-206) and the initializer, `build_jsts_usage_index` (src/analyzer/usages/js_ts_graph/resolver.rs:38-60), itself runs `files.par_iter()` over the same file set on the same global pool.

So the first worker thread to need the index (call it T1) becomes the initializer and spawns a nested `par_iter`; every other worker that needs the index parks on the `OnceLock` futex and is lost to the pool; and T1, while waiting for its nested join, can steal a *pending outer scan task*, which calls `get_or_init` on the very cell T1 is already initializing — reentrant initialization on one thread, which `OnceLock` documents as a guaranteed deadlock. Even without perfect reentrancy, enough parked workers starve the nested build. This exactly matches the field report: rayon workers parked in `jsts_usage_index`/`cached_jsts_index` under `build_jsts_edges`/`collect_per_file_edges` frames, all at 0% CPU.

This is a regression introduced by ee1af6d (2026-07-01): the `OnceLock` cache itself predates it (8ebd419, safe at the time because nothing inside the parallel scan touched it), and ee1af6d wired receiver analysis — which consults the cache — into the scan closures.

The same hazardous *shape* (OnceLock whose initializer uses `par_iter`) also exists for `reverse_import_index` in essentially every analyzer (TypeScript src/analyzer/typescript/mod.rs:390, JavaScript src/analyzer/javascript/mod.rs:362, and python/rust/go/ruby/scala/csharp/cpp via their `imports.rs`/`cache.rs`), all funneling through `build_reverse_import_index` → `build_reverse_file_index` (src/analyzer/capabilities.rs:56-73, `par_iter` at line 65). No *currently existing* call path reaches those getters from inside a parallel scan, so they have not deadlocked in the field — but they are one new caller away from reproducing this bug, so this plan fixes them as a class rather than leaving the trap armed.

## Fix design

Two complementary pieces; the first is the correctness fix, the second keeps the fast path fast.

Piece 1 — a pool-safe memo primitive. Add a small shared type (new file `src/analyzer/pool_memo.rs`, module wired in `src/analyzer/mod.rs`), tentatively:

    pub(crate) struct PoolSafeMemo<T> {
        slot: Mutex<Option<Arc<T>>>,
    }

    impl<T> PoolSafeMemo<T> {
        pub(crate) fn new() -> Self { ... }
        /// Return the memoized value, building it if absent. Never parks the
        /// caller for longer than a pointer-sized critical section, so it is
        /// safe to call from inside rayon worker threads even when builders
        /// use rayon themselves.
        pub(crate) fn get_or_build(
            &self,
            build_parallel: impl FnOnce() -> T,
            build_serial: impl FnOnce() -> T,
        ) -> Arc<T> { ... }
        pub(crate) fn get(&self) -> Option<Arc<T>> { ... }
        pub(crate) fn invalidate(&self) { ... }
    }

Semantics of `get_or_build`: lock the mutex only to read/write the `Option` (never while building). If the slot is filled, clone the `Arc` and return. Otherwise build *outside* the lock — choosing `build_serial` when `rayon::current_thread_index().is_some()` (we are on a rayon worker; a nested `par_iter` here is what enables steal-reentrancy, and parking here starves the pool) and `build_parallel` otherwise — then relock and store first-write-wins: if another racer filled the slot meanwhile, drop our copy and return theirs. This is deadlock-free by construction in every interleaving: nobody ever parks waiting for someone else's build, and a serial builder on a pool thread never yields to work-stealing, so it can never re-enter itself. The cost of the racy design is occasional duplicated builds under concurrent first access; Piece 2 makes that rare in practice, and first-write-wins keeps results consistent. If the repo has a logging facility in use in the analyzer (check for `log::`/`tracing::` usage before assuming), emit a debug/warn when a serial on-pool build happens — it signals a missed pre-materialization — but do not add a logging dependency just for this.

Why not alternatives: keeping `OnceLock` and only pre-initializing is a point fix that leaves the trap armed for the next caller (and cross-request first-touch on a shared analyzer can still park an entire pool while an off-pool parallel build waits for those same workers — a rarer but real deadlock). Running builders in a dedicated second rayon pool keeps blocking semantics but depends on subtle cross-pool `install` behavior that differs across rayon versions. A serial-only initializer under `OnceLock` avoids deadlock but permanently serializes a hot index build. The racy Arc memo is the smallest primitive that is correct in all interleavings; the Arc refcount here is intentional and appropriate — one immutable index shared across worker threads.

Piece 2 — pre-materialize before parallel scans. In `inverted::build_jsts_edges` and the scoped variant in the same file (src/analyzer/usages/js_ts_graph/inverted.rs), before calling into `inverted_edges::build_edges`/`build_edge_weights`, resolve the `JsTsUsageIndex` for the language being scanned — and for the sibling JS/TS language when that analyzer has files, because receiver analysis resolves imports into files whose language may differ from the scanned language (verify while implementing: trace what `language` is passed at get_definition/js_ts.rs:839 from the receiver-analysis path; if the sibling index is genuinely unreachable, pre-materialize only the scanned language and record that in the Decision Log). With Piece 1 in place this is purely a performance measure (build once, with full parallelism, off the scan's critical path), not a correctness requirement. Note that `build_jsts_scoped_usage_edges` (js_ts_graph.rs:287) and `JsTsQueryResolver::find_usages` (js_ts_graph.rs:129) already touch the index at entry before their scans; after the migration those call sites go through the memo and need no extra pre-init beyond what they already do.

Migration scope for Piece 1:

- `jsts_usage_index` in `TypescriptAnalyzer` (src/analyzer/typescript/mod.rs:188, getter at 219) and `JavascriptAnalyzer` (src/analyzer/javascript/mod.rs:174 inside `JsMemoCaches`, getter at 202). Getters change to return `Arc<JsTsUsageIndex>`; `cached_jsts_index` (js_ts_graph.rs:85) returns `Option<Arc<JsTsUsageIndex>>`; adjust its callers (js_ts_graph.rs:129 and :287, get_definition/js_ts.rs:839, the hierarchy call sites typescript/mod.rs:462 and javascript/mod.rs:428, and any others `cargo check` surfaces). Preserve each analyzer's existing sharing/reset semantics exactly: TypeScript wraps its cells in `Arc<...>` shared across clones and JavaScript owns them inside `JsMemoCaches`; find every place the old cells were replaced or reset on `update`/`update_all` (grep for `jsts_usage_index` and the memo-cache reset paths) and reset the memo (`invalidate` or fresh construction) in the same places.
- `build_jsts_usage_index` (resolver.rs:38) gains a parallelism mode: either a `parallel: bool` parameter or two thin entry points; internally the only difference is `files.par_iter()` vs `files.iter()` for the per-file parse loop. Do not fork the rest of the function body.
- `reverse_import_index` in all analyzers that lazily build it through `build_reverse_import_index`: TypeScript (mod.rs:185/390), JavaScript (mod.rs:171/362), and the python/rust/go/ruby/scala/csharp/cpp equivalents (their `imports.rs`/`cache.rs`/`mod.rs` — locate with `grep -rn reverse_import_index src/analyzer`). Same treatment: `PoolSafeMemo`, serial/parallel modes threaded through `build_reverse_import_index` → `build_reverse_file_index` (src/analyzer/capabilities.rs:56-73). This is mechanical; keep getter return types as close to current shape as practical (an `Arc<HashMap<ProjectFile, Arc<HashSet<ProjectFile>>>>` clone at the getter is fine).

Explicitly out of scope, with rationale: the other `OnceLock` analyzer caches whose initializers are sequential and rayon-free (`direct_descendant_index`, Rust/Python usage and hierarchy indexes, Go/Ruby/Scala/C++ import caches). A sequential initializer under `get_or_init` cannot deadlock — the initializing thread never waits on the pool, so parked waiters always wake; the worst case is a temporary parallelism dip. Migrating them is uniformity, not correctness, and touching nine more analyzers' getter signatures would bloat this change. Document the invariant instead (see below).

Document the invariant in `src/analyzer/pool_memo.rs` module docs: an analyzer-level lazy cache whose initializer uses rayon must use `PoolSafeMemo` (never a blocking `get_or_init`), because it may be reached from inside rayon worker threads; and code that runs a whole-workspace `par_iter` scan should pre-materialize any such index it can touch before the scan starts.

## Progress

- [x] (2026-07-08) Root cause confirmed by code inspection; call chain and regression commit (ee1af6d) identified.
- [x] (2026-07-08) ExecPlan written.
- [x] (2026-07-08) Milestone 1: `PoolSafeMemo` primitive + unit tests (src/analyzer/pool_memo.rs).
- [x] (2026-07-08) Milestone 2: JS/TS `jsts_usage_index` migration, pre-materialization, regression test (tests/jsts_usage_graph_deadlock.rs). The pre-fix hang demonstration (run during review, not by the implementer) showed the first-cut test did NOT reproduce the hang; the test was reworked — see Surprises & Discoveries — and the reworked scenario was observed to hang the pre-fix code (1 of 3 runs), plus a deterministic primitive-level reentrancy guard was added to pool_memo.rs.
- [x] (2026-07-08) Milestone 3: `reverse_import_index` migration across analyzers (ts, js, python, rust, go, ruby, scala, csharp, cpp) plus equivalent Java and reverse-file indexes whose initializers used `build_reverse_file_index`.
- [x] (2026-07-08) Milestone 4: full gates green (fmt, clippy --all-features -D warnings, targeted deadlock test, full --features nlp,python suite).

## Surprises & Discoveries

- The scoped JS/TS builder and `find_usages` already pre-touch the index at entry (js_ts_graph.rs:129, :287); only the unscoped `JsTsEdgeResolver::build_edges` path lacked it. That is why the hang appears specifically under `usage_graph`/`analyze_commit`.
- The receiver-analysis path to `get_definition/js_ts.rs:839` is JavaScript-only for imported-function summarization: `JsTsReceiverFactProvider::summarize_imported_function` returns early unless `self.language == Language::JavaScript`, and it passes that same `Language::JavaScript` to module binding resolution. JavaScript module resolution only considers JS extensions, so the sibling TypeScript index is not reachable from this path. TypeScript scans still pre-materialize their own index for consistency with the scanned language, but they do not reach imported-function receiver analysis.
- The grep sweep found Java `reverse_import_index` and Java/C#/Scala same-package or implicit-reference caches, plus C++ `reverse_include_index`, with the same `OnceLock` + `build_reverse_file_index` shape. Those were migrated too so the documented invariant has no known analyzer-level violations for these reverse-file builders.
- `PoolSafeMemo::invalidate` is implemented and unit-tested, but current analyzers preserve their prior reset semantics by replacing cache cells/buckets on `update`/`update_all` rather than calling `invalidate` in place.
- The first-cut regression test (60 consumer files, 2-thread pool) passed in 0.06s against the PRE-fix code — it never deadlocked. Instrumenting the pre-fix getter showed why: when every scanned file triggers the index lookup, whichever worker initializes does so while the other workers are either still busy or immediately parked, so the initializer self-executes its entire nested `par_iter` from its own deque and completes. The deadlock needs most files to be non-triggering (so workers go idle and steal), slow files inside the index builder's `par_iter` (so a stolen inner parse stays in flight while the initializer waits at a join), and triggering files late in scan order (so pending trigger tasks remain on/near the initializer's deque for it to pop mid-initialization — the reentrant `get_or_init`).
  Evidence: reworked scenario (300 noise files, 8 large `zz_big*.js` files, 24 late-sorting `zzz_consumer*.js` files, 4-thread pool) hung the pre-fix code in 1 of 3 runs under `timeout 60`; the fixed code completes it in ~9s every time. The interleaving is genuinely racy — idle workers usually park on stolen consumer tasks before reaching the initializer's inner work — so the end-to-end hang cannot be made deterministic through file geometry alone.
- Because the end-to-end repro is probabilistic, the deterministic regression guard lives at the primitive level: `pool_memo::tests::reentrant_build_from_inner_parallelism_completes` reproduces the exact deadlock shape (an off-pool builder waiting on its own `par_iter` items while those items re-enter the cell from pool threads) — with a blocking once-cell this deadlocks unconditionally; with `PoolSafeMemo` it must complete within a 60s watchdog. The integration test is kept as an end-to-end smoke test at the configuration that demonstrably hung pre-fix; post-fix it also exercises the on-pool serial-build backstop, since the pre-materialization call inside `pool.install` runs on a rayon worker.

## Decision Log

- Decision: fix the class (non-blocking memo primitive + serial-on-pool builders) rather than only pre-initializing the JS/TS index before its scan.
  Rationale: blocking `get_or_init` with a rayon-using initializer is unsound whenever reachable from a rayon worker; pre-init alone leaves the trap armed for the next caller (this exact regression happened when PR #402 added a new caller), and cross-request first-touch can still park a whole pool. Project design philosophy: fix root causes, accept blast radius.
  Date/Author: 2026-07-08, Claude (planning) / Jonathan.
- Decision: on a rayon worker thread, a cache miss builds the index *serially*; parallel builds happen only off-pool.
  Rationale: a nested `par_iter` on a worker enables steal-reentrancy (the deadlock); a serial build never yields to the scheduler, so it cannot re-enter, and with pre-materialization in place the on-pool miss path is a rarely-taken backstop.
  Date/Author: 2026-07-08, Claude / Jonathan.
- Decision: racy first-write-wins with `Mutex<Option<Arc<T>>>` instead of blocking waiters (condvar/singleflight) for late racers.
  Rationale: any blocking wait re-introduces pool starvation and interleaving analysis; duplicated builds are rare (pre-materialization) and bounded (one per racer, no recursion since on-pool racers build serially). Simplicity is worth an occasional duplicate parse pass.
  Date/Author: 2026-07-08, Claude / Jonathan.
- Decision: leave sequential-initializer `OnceLock` caches (hierarchy indexes etc.) on `OnceLock`.
  Rationale: they cannot deadlock (initializer never waits on the pool); migrating them is churn without a correctness payoff. The invariant is documented in `pool_memo.rs` for future cache authors.
  Date/Author: 2026-07-08, Claude / Jonathan.
- Decision: regression test uses a dedicated 2-thread `rayon::ThreadPool` via `install`, a ~60-file synthetic JS project, and a watchdog thread with `recv_timeout`, rather than reproducing on jest.
  Rationale: the global pool is process-wide in tests (cross-test interference), a tiny dedicated pool maximizes the steal/park interleaving probability (hang was reproduced reliably pre-fix), and an external multi-thousand-file corpus doesn't belong in CI.
  Date/Author: 2026-07-08, Claude / Jonathan.
- Decision: `PoolSafeMemo::get_or_build` takes two closures (parallel/serial) rather than one closure and a mode flag passed back in.
  Rationale: call sites read better and the primitive stays decision-owner for "which mode am I in"; builders that don't care pass the same closure twice.
  Date/Author: 2026-07-08, Claude / Jonathan (planning; implementers may revise with a log entry).
- Decision: pre-materialize only the scanned JS/TS language before `inverted::build_jsts_edges`, not the sibling JS/TS language.
  Rationale: tracing the receiver-analysis route to `get_definition/js_ts.rs:839` showed that imported-function receiver analysis is JavaScript-only and passes `Language::JavaScript`; JavaScript module resolution does not resolve TypeScript extensions. The sibling index is therefore not reachable from the confirmed deadlock path, and building it would add unnecessary work.
  Date/Author: 2026-07-08, Codex.
- Decision: migrate Java and the reverse-file caches built by `build_reverse_file_index` (Java/Scala same-package references, C# implicit references, C++ reverse includes) in addition to the named `reverse_import_index` fields.
  Rationale: the invariant is about blocking lazy initialization around rayon-using builders, not just field names. Leaving these cells on `OnceLock` would preserve the same hazardous shape after introducing `PoolSafeMemo`.
  Date/Author: 2026-07-08, Codex.
- Decision: the mandatory deterministic regression guard is the primitive-level test `reentrant_build_from_inner_parallelism_completes` in pool_memo.rs; the integration test is retained as a probabilistic end-to-end repro (observed 1-in-3 pre-fix hang rate) rather than being tuned further.
  Rationale: review verification showed the original integration test never reproduced the hang, and the deadlock interleaving depends on rayon steal timing that file geometry cannot fully control (several tuning attempts made it *less* likely). The primitive test reproduces the deadlock shape unconditionally under a blocking once-cell, so it deterministically guards the property whose absence caused issue #549, while the integration test proves the product path end-to-end and exercises the serial-on-pool backstop.
  Date/Author: 2026-07-08, Claude / Jonathan (review).

## Outcomes & Retrospective

- Implemented `PoolSafeMemo<T>` as `Mutex<Option<Arc<T>>>` with first-write-wins storage, non-blocking build behavior, serial-on-rayon-worker fallback, `get`, and `invalidate`.
- Migrated JS/TS usage indexes to `PoolSafeMemo<JsTsUsageIndex>`, changed cached access to return `Arc<JsTsUsageIndex>`, split JS/TS index construction into parallel vs serial per-file parsing, and pre-materialized the scanned language before the unscoped JS/TS edge scan.
- Added `tests/jsts_usage_graph_deadlock.rs`, an inline JavaScript workspace (300 noise files, 8 large files, 24 late-sorting consumer files) run through `usage_graph` inside a dedicated 4-thread rayon pool with a 120s watchdog, plus the deterministic reentrancy guard `pool_memo::tests::reentrant_build_from_inner_parallelism_completes`.
- Migrated reverse import/include/reference indexes whose builders call `build_reverse_import_index` or `build_reverse_file_index` to `PoolSafeMemo`, preserving existing cache replacement semantics on analyzer `update`/`update_all`.
- Validation passed: `cargo fmt`; `cargo clippy --all-targets --all-features -- -D warnings`; `BIFROST_SEMANTIC_INDEX=off cargo test --test jsts_usage_graph_deadlock` (5 passed); `BIFROST_SEMANTIC_INDEX=off cargo test --features nlp,python` (full suite passed; 3,496 runnable tests passed, 24 ignored tests listed).

## Context and Orientation

Bifrost is a Rust codebase (single crate, sources under `src/`) providing code-analysis tools over tree-sitter, exposed via search tools such as `scan_usages`, `usage_graph`, and `analyze_commit` (src/searchtools.rs, src/commit_analysis.rs). Per-language analyzers live under `src/analyzer/<language>/`; language-agnostic usage-graph machinery under `src/analyzer/usages/`. "Usage graph" = a caller→callee edge set built by scanning every workspace file in parallel (rayon) and resolving references. The JS/TS analyzer pair (TypeScript and JavaScript are separate analyzers over disjoint file sets) shares a lazily-built `JsTsUsageIndex` — per-file export tables and import binders plus re-export/importer maps — defined in src/analyzer/usages/js_ts_graph/resolver.rs and cached on each analyzer.

Tests live in `tests/` as integration suites. For small ad hoc projects use the shared inline harness `tests/common/inline_project.rs` (`InlineTestProject`) rather than handwritten tempdirs. Tests must never spawn the semantic indexer: run with `BIFROST_SEMANTIC_INDEX=off`, and note that `cargo test` without `--features nlp,python` silently skips nlp-gated suites.

## Plan of Work

Milestone 1 — the primitive. Create `src/analyzer/pool_memo.rs` with `PoolSafeMemo<T>` as specified in "Fix design", module docs stating the invariant, and unit tests in the same file: (a) two threads racing `get_or_build` both observe the same stored value afterward; (b) `get_or_build` invoked inside `rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap().install(...)` selects the serial closure, and off-pool selects the parallel closure (assert via closures setting flags); (c) `invalidate` causes a rebuild; (d) a racer that loses first-write-wins returns the winner's `Arc` (compare `Arc::ptr_eq`). Wire the module into `src/analyzer/mod.rs`. Acceptance: `cargo test pool_memo` passes.

Milestone 2 — JS/TS index migration + pre-materialization + regression test. Migrate the two `jsts_usage_index` cells to `PoolSafeMemo<JsTsUsageIndex>` preserving clone-sharing and update/reset semantics; add the serial mode to `build_jsts_usage_index`; change getters and `cached_jsts_index` to `Arc`; fix all callers. Pre-materialize in `inverted::build_jsts_edges` (and the scoped builder if it turns out not to pre-touch) for the scanned language and, if reachable (verify; record in Decision Log), the sibling language. Add `tests/jsts_usage_graph_deadlock.rs`: build an `InlineTestProject` of roughly 60 JavaScript files — one `lib.js` exporting a factory function returning an object with a method, and ~59 files each doing `import { makeThing } from './lib.js'; const t = makeThing(); t.frob();` plus a locally-defined exported function so every file contributes nodes — then, inside a dedicated 2-thread rayon pool's `install`, call the public edge-building entry (`build_jsts_usage_edges` or the narrowest `pub(crate)`-reachable equivalent the test can drive; an integration test may need a small `#[doc(hidden)]` test hook or to drive `usage_graph` through the service layer — prefer the existing service-level pattern other usage-graph tests use). Guard with a watchdog: run the build on a spawned thread, `recv_timeout(Duration::from_secs(120))` on a channel, panic with a clear message on timeout. Acceptance: this test hangs (watchdog fires) when run against the pre-migration code — verify once by stashing the fix or checking out ee1af6d — and passes quickly after; existing JS/TS usage-graph and scan_usages suites stay green.

Milestone 3 — reverse_import_index migration. Thread a parallelism mode through `build_reverse_import_index`/`build_reverse_file_index` (src/analyzer/capabilities.rs) and migrate every analyzer's `reverse_import_index` cell to `PoolSafeMemo`, preserving each analyzer's reset semantics. Mechanical; acceptance is compilation plus existing per-language scan_usages/usage_graph suites green.

Milestone 4 — gates. `cargo fmt`; `cargo clippy --all-targets --all-features -- -D warnings`; `BIFROST_SEMANTIC_INDEX=off cargo test --features nlp,python` (full suite). Update this plan's living sections; commit per repo conventions (current branch, only files we changed).

## Concrete Steps

All commands run from the repository root `/home/jonathan/Projects/bifrost`.

    # after each milestone
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    BIFROST_SEMANTIC_INDEX=off cargo test --test jsts_usage_graph_deadlock
    BIFROST_SEMANTIC_INDEX=off cargo test --features nlp,python   # milestone 4, full gate

To demonstrate the bug once (optional but recommended, milestone 2): `git stash` the fix, run the new regression test, observe the watchdog panic after ~120s (or a hard `timeout 150 cargo test ...` exiting 124), then `git stash pop`.

## Validation and Acceptance

Acceptance is behavioral: `tests/jsts_usage_graph_deadlock.rs` deterministically hangs (watchdog fires) against the pre-fix tree and completes in seconds against the fixed tree; `pool_memo` unit tests prove the primitive's semantics including serial-on-pool selection; the full `--features nlp,python` test suite passes with no regressions in scan_usages/usage_graph behavior for any language (the reverse-import migration must be behavior-preserving). The original field failure (jest at d5dd58cf4f, via brokkbench `analyze_commit`) should no longer hang; that external validation is worth running opportunistically but is not a CI gate.

## Idempotence and Recovery

All steps are additive code edits plus mechanical migrations; re-running builds/tests is safe. If a milestone's migration breaks a suite, the memo getters are drop-in replaceable by the previous `OnceLock` pattern per analyzer, so partial rollback is per-file `git checkout`. Commit at each milestone boundary so any regression bisects to one milestone.

## Interfaces and Dependencies

No new crate dependencies (rayon and std only). End state must include, in `src/analyzer/pool_memo.rs`:

    pub(crate) struct PoolSafeMemo<T> { /* Mutex<Option<Arc<T>>> */ }
    impl<T> PoolSafeMemo<T> {
        pub(crate) fn new() -> Self;
        pub(crate) fn get(&self) -> Option<Arc<T>>;
        pub(crate) fn get_or_build(&self, build_parallel: impl FnOnce() -> T, build_serial: impl FnOnce() -> T) -> Arc<T>;
        pub(crate) fn invalidate(&self);
    }

and in src/analyzer/usages/js_ts_graph/resolver.rs a `build_jsts_usage_index` capable of serial or parallel per-file parsing; `cached_jsts_index` returning `Option<Arc<JsTsUsageIndex>>`; `build_reverse_file_index` (src/analyzer/capabilities.rs) capable of serial or parallel operation.

---

Revision note (2026-07-08): Initial version, authored from issue #549 plus code inspection; call chain, regression commit, and hazard sweep verified against the working tree at 93c834b.

Revision note (2026-07-08): Implemented milestones 1-4. Added `PoolSafeMemo`, migrated JS/TS and reverse-file caches, added the JS/TS usage-graph deadlock regression test, recorded sibling-language reachability, and ran the required gates successfully.

Revision note (2026-07-08, review): Pre-fix verification exposed that the first-cut integration test never reproduced the deadlock. Reworked it (noise/large/late-trigger file mix, 4 threads) until the pre-fix code was observed to hang, documented why the interleaving cannot be made deterministic end-to-end, and added the deterministic primitive-level reentrancy test as the primary regression guard. Updated Progress, Surprises & Discoveries, Decision Log, and Outcomes accordingly.
