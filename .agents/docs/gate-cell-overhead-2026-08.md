# D4 gate cells: the full wall, decomposed from exec to exit (run 9)

Date: 2026-08-08. Subject: HEAD `f05c0e48` on `bifrost-nlp-ft`, in a pinned
detached scratch worktree. Measurement only. No file in the main worktree
changed. Predecessor: `usage-graph-d4-remeasure-v1.md` (run 6, at `37540fb3`).

Purpose: run 6 measured gate cells (a) and (b) at 5.31 s against a 5 s bar, with
every cell returning `resolved=0` / `time_budget`. It read that as "3.0 s of
scan budget plus about 2.3 s of overhead", and the overhead had never been
decomposed. This run decomposes it, and answers the cell-(b) puzzle.

**Owner directive in force: the box is noisy. Verdicts rest on CPU time,
proportions and counts. Wall clock is context. Per-cell 1-minute loadavg was
12-62 during the measured cells.**

## Headline

1. **The premise "3.0 s budget plus 2.3 s overhead" is wrong in one term and
   right in the other.** The non-budget wall is real and is **2.1-2.3 s**, as
   estimated. But the scan budget window is **not 3.0 s**: it ran **3.20-4.61 s**
   (median 4.23 s for (a), 3.43 s for (b)). The 3 s deadline overshoots by
   0.2-1.6 s, because the phase that consumes it does not poll the deadline.
2. **One previously unpriced phase dominates the whole cell.**
   `excluded_test_files` costs **2.26-2.85 s**, which is **66-87 % of the scan
   budget window** and about **40-45 % of the entire process wall**. It runs
   before any symbol work, in both cells, on every run. It has never carried a
   span. It is the biggest single item in the cell by a factor of two.
3. **Cargo-route composition is not part of the non-budget overhead.** It is
   **inside** `excluded_test_files`, inside the scan budget:
   `RustAnalyzer::build_cargo_routes` is 0.73-1.12 s and is triggered by
   `is_test_like_file` -> `file_is_test_only` -> `cargo_routes()`.
4. **The rust-fact catch-up probe never runs in a gate cell.** Zero occurrences
   of `RustAnalyzer::rust_fact_catch_up` in all six timed reps. It sits at the
   head of the cross-file usage walk, and no gate cell reaches the walk.
5. **The workspace file listing is built twice per process**, 0.34-0.46 s each.
   The second build is inside the budget, inside `sibling_extensions`, and it is
   caused by `start_watcher` calling `invalidate_cached_file_listing()`.
6. **PUZZLE ANSWERED. Cell (b) never reaches the #1839 gate.** The gate is
   correct and it fires perfectly when it is reached. It is unreachable at the
   product default budget because the scan prologue has already spent the budget
   before resolution starts, so `Cancelled` beats `TooManyCandidates`. **Proved
   two ways below.**
7. **Removing the prologue moves cell (b) from 5.61 s to 2.90 s and turns its
   answer from `time_budget` into the correct structured
   `too_many_candidates[total=4186 limit=200]`.** Measured, not modelled.

## Method

| item | value |
| --- | --- |
| host | 120 CPUs, 98 GB RAM, kernel 6.18.33.2-microsoft-standard-WSL2 |
| load | not quiet. Build started at loadavg 265. Measured cells ran at 1-min loadavg **12-62** |
| workspace | rustc tree `rust--01f6ddf7`, copied read-only from `/mnt/T9/repo-clones/.codescale-sources/` to `/mnt/containers/bifrost-latency-probe/m9/rust-tree` (425 MB) |
| binary | one featureless release `bifrost`, `sha256[0:16]=076e5fc8f70ab01d`, built by `scripts/with-isolated-cargo-target.sh` from `wt-v9` @ `f05c0e48` plus the additive spans listed below |
| cache | `bifrost_cache.v18.db`, **845.2 MB**, built once by this binary (prewarm wall 1:55.98, user 87.54 s, sys 61.86 s). Run 6 used v17 at 847.4 MB |
| env | `BIFROST_CACHE_GC=off BIFROST_SEMANTIC_INDEX=off`, no `nlp` |
| cells | `scan_usages_by_reference`, product default budget. (a) `compiler/rustc_target/src/spec/mod.rs#SanitizerSet`, (b) `compiler/rustc/src/main.rs#main`. Identical to runs 1-6 |
| reps | one warm-up per cell, then 3 timed and 3 untimed reps of each, interleaved a/b/a/b, one process at a time |

### Spans added for this run (additive only)

Every phase named in the brief lacked a span except `mcp_cold.analyzer_construction`,
`WorkspaceAnalyzer::build` and `RustAnalyzer::build_cargo_routes`. These were added:

- CLI skeleton, in `src/bin/bifrost.rs`: `cli.pre_main` (from `/proc/self/stat`
  start time, 10 ms granularity), `cli.semantic_model_pack_install`,
  `cli.argument_normalization`, `cli.service_construction`,
  `cli.call_tool_output`, `cli.render_and_print`, `cli.service_teardown`,
  `cli.main_entry_to_run_return`.
- Service construction: `service_new.canonical_root`,
  `analyzer_construction.build_project`,
  `analyzer_construction.workspace_analyzer`,
  `analyzer_construction.prewarm_semantic_models`,
  `service_new.assemble_session`, `assemble_session.workspace_root_open`,
  `assemble_session.start_watcher`.
- Store open: `workspace.persistent_store_context`,
  `store_context.open_persistent` (this is the cache open plus the
  `current_schema_fast_path` check), `store_context.liveness`.
- Scan prologue: `scan_usages.query_scope_new`,
  `scan_usages.excluded_test_files` (with an excluded-file count),
  `scan_usages.sibling_extensions`.
- Resolution trace: `fuzzy.bare_query_leaf`, `bare_name_resolution[...]`,
  `bare_name_resolution.definitions`,
  `bare_name_resolution.lookup_candidates_by_identifier` (with a candidate
  count), `bare_name_resolution.filter_candidates`, a deduplicated-match count,
  and notes at the two `FuzzyResolveBudget` exits
  (`m9.fuzzy_budget.keep_going_false`, `m9.fuzzy_budget.admits_reached`) plus a
  per-symbol outcome note.
- Rust fact catch-up: `rust_fact_catch_up.live_path_snapshot`,
  `rust_fact_catch_up.get_analyzed_files`,
  `rust_fact_catch_up.blobs_with_rust_facts` with probed and present oid counts.
  These never fired. See headline 4.
- `crates/bifrost-core/src/profiling.rs`: an opt-in process-CPU sample on each
  span, behind `BIFROST_TIMING_CPU`. It reads `/proc/self/stat` at BEGIN and END
  and reports the process-wide user and system CPU consumed during the span.
  Process-wide is what is wanted: a phase that fans out over five language build
  threads must be charged for all of them.

One change is a reorder, not an addition: `run_tool` now drops the service
explicitly after the stdout write, so teardown can be priced. The order of
observable work is unchanged.

The probe cost is small. A timed rep emits 216-394 span lines. The timed and
untimed walls overlap inside their own spread (see the next table), so the
ledger's absolute values are usable, not only its proportions.

## The gate cells at HEAD, against run 6

Untimed cells, which are the comparable figures:

| cell | run 9 wall (s), 3 reps | run 9 median | run 9 user CPU (s) | run 6 wall | run 6 user CPU |
| --- | --- | ---: | --- | ---: | ---: |
| a warm | 5.71 / 5.71 / 5.81 | **5.71** | 4.35 / 4.56 / 3.77 | 5.31 | 4.21 |
| b warm | 5.41 / 5.81 / 5.61 | **5.61** | 3.87 / 4.46 / 3.97 | 5.31 | 4.35 |

**User CPU agrees with run 6 to within the spread (median 4.35 and 3.97 against
4.21 and 4.35). The work has not changed, so the run-9 ledger describes the
run-6 cells.** Every cell returned `resolved=0 found=0 total_hits=0 failure=1
partial=true`, status `failure`, `reason_kind=time_budget` -- the same result as
runs 1-6. Peak RSS 0.16-0.25 GB.

## The phase ledger

Wall seconds. Median of 3 timed reps, then the individual reps. `cpu_u` is the
median process-wide **user** CPU consumed inside the phase, which is the
load-independent term. Indentation is containment.

### Cell (a), `#SanitizerSet`

Timed walls 6.71 / 5.21 / 6.41 s; user CPU 5.01 / 3.91 / 4.85 s.

| phase | median | cpu_u | reps |
| --- | ---: | ---: | --- |
| `cli.pre_main` (exec, loader, libc init) | 0.00 | -- | 0.00 / 0.00 / 0.00 |
| `cli.semantic_model_pack_install` | 0.00 | 0.00 | 0.00 / 0.00 / 0.00 |
| `cli.argument_normalization` | 0.00 | 0.00 | 0.00 / 0.00 / 0.00 |
| **`cli.service_construction`** | **1.91** | 1.44 | 1.80 / 1.91 / 1.93 |
| &nbsp;&nbsp;`analyzer_construction.build_project` | 0.42 | 0.31 | 0.35 / 0.42 / 0.48 |
| &nbsp;&nbsp;`analyzer_construction.workspace_analyzer` | 1.05 | 0.99 | 1.09 / 1.05 / 1.05 |
| &nbsp;&nbsp;&nbsp;&nbsp;`workspace.persistent_store_context` | 0.002 | 0.00 | cache open + schema fast path |
| &nbsp;&nbsp;`analyzer_construction.prewarm_semantic_models` | 0.03 | 0.00 | 0.04 / 0.03 / 0.03 |
| &nbsp;&nbsp;`service_new.assemble_session` | 0.37 | 0.19 | 0.33 / 0.41 / 0.37 |
| &nbsp;&nbsp;&nbsp;&nbsp;`assemble_session.start_watcher` | 0.37 | 0.19 | 0.33 / 0.41 / 0.37 |
| **`cli.call_tool_output`** (the budget window) | **4.23** | 3.28 | 4.61 / 3.08 / 4.23 |
| &nbsp;&nbsp;**`scan_usages.excluded_test_files`** | **2.30** | 1.51 | 2.40 / 2.30 / 2.26 |
| &nbsp;&nbsp;&nbsp;&nbsp;`RustAnalyzer::build_cargo_routes` | 0.83 | 0.57 | 0.83 / 0.85 / 0.81 |
| &nbsp;&nbsp;`scan_usages.sibling_extensions` | 0.47 | 0.37 | 0.47 / 0.77 / 0.43 |
| &nbsp;&nbsp;`searchtools::scan_usages_symbol_resolution` | 0.01 | 0.01 | 0.01 / **absent** / 0.01 |
| &nbsp;&nbsp;`usages::candidate_discovery` | 1.62 | 1.45 | 1.72 / **absent** / 1.52 |
| `cli.render_and_print` | 0.00 | 0.00 | 0.00 / 0.00 / 0.00 |
| `cli.service_teardown` | 0.10 | 0.10 | 0.12 / 0.10 / 0.10 |
| exec + exit residual (wall minus in-main) | 0.15 | -- | 0.17 / 0.13 / 0.15 |

### Cell (b), `#main`

Timed walls 5.41 / 7.12 / 6.06 s; user CPU 4.09 / 5.16 / 4.36 s.

| phase | median | cpu_u | reps |
| --- | ---: | ---: | --- |
| `cli.pre_main` | 0.00 | -- | 0.00 / 0.00 / 0.00 |
| `cli.semantic_model_pack_install` | 0.00 | 0.00 | 0.00 / 0.00 / 0.00 |
| `cli.argument_normalization` | 0.00 | 0.00 | 0.00 / 0.00 / 0.00 |
| **`cli.service_construction`** | **2.05** | 1.72 | 2.05 / 3.52 / 1.93 |
| &nbsp;&nbsp;`analyzer_construction.build_project` | 0.46 | 0.35 | 0.46 / 1.04 / 0.41 |
| &nbsp;&nbsp;`analyzer_construction.workspace_analyzer` | 1.17 | 1.16 | 1.17 / 1.90 / 1.10 |
| &nbsp;&nbsp;`analyzer_construction.prewarm_semantic_models` | 0.03 | 0.00 | 0.03 / 0.05 / 0.03 |
| &nbsp;&nbsp;`service_new.assemble_session` | 0.40 | 0.21 | 0.39 / 0.53 / 0.40 |
| &nbsp;&nbsp;&nbsp;&nbsp;`assemble_session.start_watcher` | 0.40 | 0.21 | 0.39 / 0.53 / 0.40 |
| **`cli.call_tool_output`** (the budget window) | **3.43** | 2.20 | 3.20 / 3.43 / 3.80 |
| &nbsp;&nbsp;**`scan_usages.excluded_test_files`** | **2.78** | 1.69 | 2.26 / 2.85 / 2.78 |
| &nbsp;&nbsp;&nbsp;&nbsp;`RustAnalyzer::build_cargo_routes` | 1.03 | 0.58 | 0.73 / 1.03 / 1.12 |
| &nbsp;&nbsp;`scan_usages.sibling_extensions` | 0.58 | 0.45 | 0.43 / 0.58 / 1.02 |
| &nbsp;&nbsp;`searchtools::scan_usages_symbol_resolution` | 0.49 | 0.45 | 0.49 / **absent** / **absent** |
| `cli.render_and_print` | 0.00 | 0.00 | 0.00 / 0.00 / 0.00 |
| `cli.service_teardown` | 0.08 | 0.08 | 0.06 / 0.08 / 0.13 |
| exec + exit residual | 0.11 | -- | 0.11 / 0.09 / 0.20 |

**The ledger closes.** `wall - cli.pre_main - cli.main_entry_to_run_return` is
0.09-0.20 s in every rep, and that residual is exec, dynamic linking, libc init
and process exit for a 113 MB binary. Nothing is unaccounted for.

**"Absent" is a measurement, not a gap.** In three of the six timed reps
(a rep 2, b reps 2 and 3), the prologue alone consumed the whole 3 s budget. The
backend then hit its cancellation check before the symbols loop and returned. No
symbol resolution happened at all. This reproduces run 4's observation that
`searchtools::scan_usages_symbol_resolution` "does not appear in the cell (b)
span set", and explains it.

### Where the non-budget wall goes

| bucket | cell (a) | cell (b) | note |
| --- | ---: | ---: | --- |
| exec, loader, libc init, exit | 0.15 | 0.11 | residual; `cli.pre_main` alone is under the 10 ms `/proc` granularity |
| semantic model pack install | 0.00 | 0.00 | |
| argument normalization + root canonicalization | 0.00 | 0.00 | |
| cache open + schema fast path + liveness | **0.002** | **0.003** | `store_context.open_persistent` 1.8-2.2 ms, `store_context.liveness` 0.2-0.4 ms |
| `build_project` (first whole-workspace listing) | 0.42 | 0.46 | |
| `workspace_analyzer` (5 language delegates) | 1.05 | 1.17 | |
| `prewarm_semantic_models` | 0.03 | 0.03 | |
| `assemble_session` (almost all `start_watcher`) | 0.37 | 0.40 | |
| render + print | 0.00 | 0.00 | |
| service teardown | 0.10 | 0.08 | |
| **total non-budget** | **2.12** | **2.25** | matches the brief's ~2.3 s estimate |

Two of the brief's named residents are not in this list, and that is the point:

- **Cargo-route composition is inside the budget**, not outside it. 0.83 s (a)
  and 1.03 s (b), nested under `excluded_test_files`.
- **The rust-fact catch-up probe does not run.** `blobs_with_rust_facts` was
  never called in any cell.

### Inside the two largest phases

`analyzer_construction.workspace_analyzer` (1.05-1.17 s) builds five language
delegates concurrently. Rust is the critical path and equals the whole phase:

| delegate | wall (ms) | cpu_u (s) | cpu_sys (s) |
| --- | ---: | ---: | ---: |
| Rust | 1083.3 | 1.05 | 7.09 |
| JavaScript | 499.1 | 0.77 | 6.68 |
| TypeScript | 437.4 | 0.72 | 6.47 |
| Cpp | 425.2 | 0.69 | 6.46 |
| Python | 418.8 | 0.67 | 6.46 |

Inside Rust: `enumerate_files` 134.1 ms, `reconcile_file_states` 948.1 ms, of
which `resolve_live_oids` is 308.2 ms. **This phase is where the cell's whole
system-CPU bill is incurred**: about 7 s of `sys`, shared across the five
threads that resolve live blob oids over the tree. Run 6 reported 7.5-9.1 s of
system CPU for these cells and called it host noise. It is not noise. It is
`resolve_live_oids` at workspace startup. (User CPU there is only about 1.0 s,
so the owner's rule still holds: read user, not sys.)

`scan_usages.excluded_test_files` (2.30-2.78 s) contains one child span,
`RustAnalyzer::build_cargo_routes` (0.83-1.03 s). The remaining 1.5-1.8 s is the
per-file `is_test_like_file` loop over the analyzed file set. It excluded
**29,748 files** in every rep, out of a tree of about 35k analyzed files. The
chain is `excluded_test_files` -> `is_test_like_file` ->
`analyzer.file_is_test_only(file)` -> `RustAnalyzer::file_is_test_only` ->
`cargo_routes()`, which is what drags the cargo-route index into the budget.

`scan_usages.sibling_extensions` (0.43-1.02 s) is almost entirely a **second
whole-workspace listing**: `project::collect_workspace_files` fires again inside
it, at 445.9 ms in cell (a) rep 1, against 341.8 ms for the first listing in
`build_project`. The cause is visible in the code:
`start_session_watcher` calls `project.invalidate_cached_file_listing()` after
registering the watcher, deliberately, so that no listing predates event
coverage. The next `all_files_shared` therefore re-walks the tree -- inside the
scan budget. The phase's whole product is a set of file extensions.

## The cell-(b) puzzle: does resolution reach the #1839 gate?

**Verdict: no, it does not -- and the gate itself is correct.**

### The trace, cell (b) at the default 3 s budget

From `r-b-timed-1`, the one default-budget rep in which resolution ran at all:

```
scan_usages.excluded_test_files                      2259.1 ms   (excluded_test_files=29748)
scan_usages.sibling_extensions                        433.8 ms
searchtools::scan_usages_symbol_resolution            493.8 ms
  m9.fuzzy_bare_leaf query=main leaf=Some("main")
  bare_name_resolution.definitions                    177.2 ms   matches=1
  bare_name_resolution.lookup_candidates_by_identifier 306.2 ms  identifier_candidates=22177
  m9.fuzzy_budget.keep_going_false                              <-- STOP
  outcome=budget_cancelled
```

`m9.fuzzy_budget.admits_reached` -- the note at the #1839 gate -- **is absent**.
The gate is never evaluated.

The sequence is exact. `excluded_test_files` (2.26 s) plus `sibling_extensions`
(0.43 s) is **2.69 s of the 3.00 s budget, spent before resolution starts**.
Resolution then has about 0.31 s left. It spends 0.18 s reading
`definitions("main")` and 0.31 s in `lookup_candidates_by_identifier("main")`,
which returns **22,177 candidates**. Neither of those two reads polls the
budget. The first poll that does occur is `budget.keep_going()?` at the top of
the per-candidate filter loop in `bare_name_resolution`, and by then the
deadline is 0.18 s past. It returns `FuzzyResolveStop::Cancelled`. The scan
reports `incomplete_reason=time_budget`.

`resolution_from_matches`, which holds `budget.admits(matches.len())?`, is one
call further down. It is never reached.

### Proof that the gate works when it is reached

Same cell, same binary, same cache, `max_duration_secs=60`:

```
tag=b-b60-t1  wall=5.71 s
  bare_name_resolution.definitions                    189.3 ms   matches=1
  bare_name_resolution.lookup_candidates_by_identifier 312.6 ms  identifier_candidates=22177
  bare_name_resolution.filter_candidates               11.7 ms
  deduplicated_matches=4186
  m9.fuzzy_budget.admits_reached total=4186 limit=200
  outcome=too_many_candidates[total=4186 limit=200]

result: status="ambiguous", incomplete_reason="resolution_candidates",
        too_many_candidates={"cap": 200, "total_candidates": 4186}
```

Total resolution cost: **515 ms**. The gate fires correctly, reports the true
deduplicated count (4,186 declarations from 22,177 identifier candidates), and
produces the structured `resolution_candidates` outcome #1839 specifies.

### Proof that the prologue is the sole cause

Same cell, **default 3 s budget**, with only `include_tests: true` -- which
makes `excluded_test_files` return `None` immediately and skips the prologue's
big term:

| rep | wall (s) | user CPU (s) | outcome |
| --- | ---: | ---: | --- |
| b-incltests-1 | 3.61 | 3.10 | `too_many_candidates[4186/200]` |
| b-incltests-2 | **2.80** | 2.37 | `too_many_candidates[4186/200]` |
| b-incltests-3 | 2.90 | 2.43 | `too_many_candidates[4186/200]` |

Ledger for rep 2: construction 1.81 s, `cli.call_tool_output` **0.87 s**
(`excluded_test_files` 0.0 ms, `sibling_extensions` 0.37 s, resolution 0.49 s),
teardown 0.05 s, exec+exit 0.08 s.

**Cell (b) goes from 5.61 s and a wrong `time_budget` answer to 2.90 s and the
right structured answer, by removing one prologue phase.** Nothing else changed.

The same counterfactual on cell (a) is honest to report and does not help:
5.81 / 5.21 / 5.01 s, against 5.71 s untimed at the default. Cell (a)'s
resolution is trivial (1 candidate), so the budget freed by the prologue is
immediately re-spent by `usages::candidate_discovery`, which grows from 1.62 s
to 3.38 s once test files are in scope. **Cell (a) is deadline-bound; cell (b)
is prologue-bound.**

### The exact seam

Two separate defects sit on this path. Only the second is a one-line fix.

**Seam 1 (the reported behaviour, and the real one).**
`excluded_test_files` and `sibling_extensions` are inside the scan's wall-clock
budget but are not part of answering the query. They spend 2.7-3.4 s of a 3.0 s
budget on whole-workspace bookkeeping. Any structured early outcome that lives
downstream of them -- the #1839 gate is one, but so is every other resolution
verdict -- is unreachable on a workspace this size. This is the finding with the
latency in it.

**Seam 2 (the ordering bug in the gate itself).**
In `bare_name_resolution` (`crates/bifrost-analysis/src/analyzer/symbol_lookup.rs`),
the candidate count that the gate exists to test is already known the moment
`lookup_candidates_by_identifier` returns, but the gate is not consulted until
`resolution_from_matches`, on the far side of a `budget.keep_going()?` loop over
all 22,177 candidates. `keep_going` is checked first, so **`Cancelled` always
wins over `TooManyCandidates` whenever the caller's budget is already spent.**
The one-line change is to admit the count where it is first known:

```rust
let identifier_candidates = analyzer.lookup_candidates_by_identifier(leaf);
budget.admits(identifier_candidates.len())?;   // <-- before the keep_going loop
for candidate in identifier_candidates { budget.keep_going()?; ... }
```

Measured effect on latency: about **10-12 ms** (the `filter_candidates` span).
Measured effect on the answer: cell (b) would report
`too_many_candidates[total>=22177 limit=200]` instead of `time_budget`, in the
reps where resolution runs at all. Note the reported `total` changes from the
deduplicated 4,186 to the pre-dedup 22,177, which is a contract question worth
deciding, not a free change.

**This one-line fix does not make the gate pass.** In three of six default-budget
reps the budget is gone before resolution is entered, so no change inside
resolution can be observed. Seam 1 has to be fixed for seam 2 to matter.

A larger, better fix exists and is already in the codebase for other callers:
`TreeSitterAnalyzer::lookup_declarations_by_identifier_limited(identifier, limit,
continue_query)` (used by the Ruby and Scala usage paths). Routing
`bare_name_resolution` through a bounded lookup would let the store stop at
`limit + 1` rows instead of materializing 22,177 `CodeUnit`s, which would cut the
306 ms lookup as well as reaching the gate. That is not a one-line change.

## Ranked list: where 0.31 s could come from

The gate needs 5.31 s to become under 5.00 s. Sizes below are measured medians
from this run. Nothing here is an estimate of a saving; each row states what the
phase costs and, where a counterfactual was run, what removing it did.

| rank | phase | in budget? | median cost (a / b) | evidence for what is recoverable |
| ---: | --- | --- | ---: | --- |
| 1 | `scan_usages.excluded_test_files` | **yes** | **2.30 / 2.78 s** | Measured removal: cell (b) 5.61 -> **2.90 s** median, and the answer becomes correct. Cell (a) 5.71 -> 5.21 s only, because the freed budget is re-spent by `candidate_discovery`. |
| 2 | `analyzer_construction.workspace_analyzer` | no | 1.05 / 1.17 s | Not counterfactually tested. Rust is the critical path at 1083 ms; `reconcile_file_states` is 948 ms of it and `resolve_live_oids` 308 ms. Carries ~7 s of the cell's system CPU. |
| 3 | `RustAnalyzer::build_cargo_routes` | **yes** (inside rank 1) | 0.83 / 1.03 s | Wholly contained in rank 1, so it is not additive with it. It is dragged in by `file_is_test_only`, which is a test-classification question, not a routing question. |
| 4 | `scan_usages.sibling_extensions` | **yes** | 0.47 / 0.58 s | 445.9 of 474.0 ms in cell (a) rep 1 is a **second** `project::collect_workspace_files`, caused by `start_watcher`'s deliberate `invalidate_cached_file_listing()`. Its entire product is a set of file extensions. |
| 5 | `analyzer_construction.build_project` | no | 0.42 / 0.46 s | The first whole-workspace listing (341.8 ms) plus `gitblob::dirty_worktree_paths` (67.2 ms). |
| 6 | `assemble_session.start_watcher` | no | 0.37 / 0.40 s | Pure setup, 0.19-0.21 s user CPU. On its own it is **larger than the 0.31 s the gate needs**, and it is outside the budget, so any saving here lands directly on the wall. A one-shot CLI call that exits immediately arguably does not need a file watcher at all. |
| 7 | exec + loader + libc init + exit | no | 0.15 / 0.11 s | Residual for a 113 MB binary. `cli.pre_main` is under the 10 ms `/proc` granularity, so most of this is exit. |
| 8 | `cli.service_teardown` | no | 0.10 / 0.08 s | Explicit drop of the service, watcher and store. |
| 9 | `analyzer_construction.prewarm_semantic_models` | no | 0.03 / 0.03 s | |
| 10 | cache open + schema fast path + liveness | no | **0.002 / 0.003 s** | `store_context.open_persistent` 1.8-2.2 ms on an 845 MB store. `current_schema_fast_path` works. **There is nothing here.** |
| 11 | semantic pack install, argument normalization, root canonicalization, `workspace_root_open`, `snapshot_for_query`, render + print | no/mixed | 0.000-0.001 s each | Nothing here either. |
| -- | rust-fact catch-up existence probe | n/a | **0.00 s** | **Never runs in a gate cell.** Zero `RustAnalyzer::rust_fact_catch_up` spans in 6 of 6 timed reps. It sits at the head of the cross-file walk (`usage_walks.rs:329`), which no gate cell reaches. |

**Reading.** Ranks 6, 5 and 2 are outside the budget and together are
1.84-2.03 s, so the 0.31 s the gate needs is available there several times over,
and any of the three alone would supply it. Rank 1 is much larger than all of
them, but it is inside the budget, so on cell (a) removing it converts prologue
time into scan time and buys almost nothing on the wall. On cell (b) it buys
2.7 s and a correct answer.

## Corrections to the run-6 record

1. **"3 s scan deadline fully consumed, leaving ~2.3 s of overhead" is half
   right.** The overhead is 2.12-2.25 s, as estimated. The budget window is
   3.20-4.61 s, not 3.00 s. The 3 s deadline is exceeded by 0.2-1.6 s because
   `excluded_test_files` and `candidate_discovery` do not poll it finely enough.
2. **Cargo-route composition is not overhead.** Run 6 listed it among the
   non-budget residents. It is inside the scan budget, inside
   `excluded_test_files`.
3. **The rust-fact catch-up probe is not a resident of these cells at all.**
4. **Run 6's high system CPU is not host noise.** 7 s of the 7.8-9.9 s of `sys`
   is `TreeSitterAnalyzer::resolve_live_oids` inside analyzer construction. The
   directive to read user CPU rather than system CPU is still correct, but the
   system CPU has a named owner.
5. **Run 4's "`scan_usages_symbol_resolution` does not appear in the cell (b)
   span set" is confirmed and explained.** It reproduces in 3 of 6 reps here.
   The prologue consumes the budget, and the backend returns at the cancellation
   check before the symbols loop.

## Limitations

1. **Three repetitions per configuration on a host with other tenants.** Cell
   (b) rep 2 is visibly a load outlier (construction 3.52 s against 1.93-2.05 s).
   Medians are quoted for that reason.
2. **`BIFROST_TIMING` spans are wall-based and thread-summed.** The per-language
   delegate rows overlap in time; they do not add up to the parent.
3. **The per-span CPU sample is process-wide, not per-thread.** For a sequential
   phase this is the right figure. For two phases that overlap it would double
   count, and none of the skeleton phases overlap.
4. **`cli.pre_main` has 10 ms resolution** (`USER_HZ`), so pre-main time is
   reported as zero and lands in the exec+exit residual instead.
5. **The `include_tests: true` counterfactual changes the query, not only the
   phase.** It removes the prologue but also widens the scan scope. That is why
   it helps (b), which stops at resolution, and not (a), which reaches the scan.
   It prices the prologue; it is not a proposed fix.
6. **The cache is `v18` at 845.2 MB, against run 6's `v17` at 847.4 MB.** The
   schema epoch differs. User CPU for the cells agrees with run 6 within spread,
   which is the evidence that the comparison holds.
7. **The `total` a fixed gate would report is not the same number.** 22,177
   pre-dedup against 4,186 deduplicated. See seam 2.

## Recommendations

1. **Take `excluded_test_files` out of the scan's wall-clock budget, or make it
   cheap.** It is 66-87 % of the budget window and it is bookkeeping, not
   answering. It is also what pulls the cargo-route index in. This is the single
   largest item anywhere in the cell. Search the issues first; this is adjacent
   to the #1839 work but the subject is the prologue, not resolution.
2. **Fix the gate ordering in `bare_name_resolution`** (seam 2, one line), but
   record that it changes the reported `total` from deduplicated to pre-dedup,
   and that it does not on its own make the gate pass. Prefer routing the lookup
   through the existing `lookup_declarations_by_identifier_limited` if the
   contract question can be settled.
3. **Look at `assemble_session.start_watcher` for a one-shot CLI call.** 0.37-0.40 s
   outside the budget, larger than the gap the gate needs, for a watcher the
   process never uses before exiting. Its `invalidate_cached_file_listing()` also
   costs a second whole-workspace listing of 0.45 s inside the budget (rank 4).
4. **Do not spend effort on cache open, the schema fast path, process start,
   argument handling, render, or the rust-fact catch-up probe in this cell.**
   Measured: 2-3 ms, 2-3 ms, under 10 ms, under 1 ms, under 1 ms, and zero.
5. **Amend the D4 record so the gate cells are described correctly.** They are
   not "3 s of budget plus overhead". Cell (a) is deadline-bound and overshoots
   its deadline. Cell (b) is prologue-bound and never reaches its own early-exit
   gate.

## Artefacts

All under `/tmp/claude-1000/-mnt-optane-bifrost-nlp/b5398767-af2f-42d8-9210-eea66ede9085/scratchpad/m9/`:
`results.txt` (3+3 reps per cell), `results2.txt` (60 s budget and
`include_tests` cells), `r-*.stderr.spans`, `r-*.json`, `r-*.time`,
`ledger.py`, `cell.sh`, `cell2.sh`, `drive.sh`, `drive2.sh`, `prewarm.sh`.
The scratch worktree, tree copy and cache were removed after the run.
