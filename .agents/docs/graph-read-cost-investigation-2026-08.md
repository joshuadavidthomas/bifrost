# Why one indexed read "costs 10 s", and why one symbol reaches 100k names (m8)

Date: 2026-08-08. Subject: `bifrost-nlp-ft` HEAD `2259d633`. Read-only
measurement: no source file in the main worktree was touched and no commit was
made. Successor to `usage-graph-d4-remeasure-v1.md` (run 6) and
`memory-attribution-v1.md` (run 5).

**Owner directive in force**: verdicts rest on load-independent quantities.
Thread CPU time (`CLOCK_THREAD_CPUTIME_ID`) is the primary meter here; wall time
is reported beside it because the entire finding turns on the gap between the
two.

---

## Headline

**The premise of both questions does not survive measurement, and that is the
result.**

1. **A "10-second read" is not ten seconds of reading.** Measured at the same
   100,890 store reads run 6 measured, on a quiet host: the whole per-name store
   read costs **25.64 CPU-seconds in total, 0.254 ms per read**, against
   **190.32 s of wall**. The single hottest name, `main` (21,942 rows), costs
   **650 ms CPU / 2,952 ms wall** for its one read; the same SQL run
   single-threaded against the same database costs **43 ms**. Run 6's 10.97 s
   for `main` was a wall-clock span on a loaded host, and 93 % of that span is
   not CPU at all.
2. **The store reads are 0.44 % of the query's CPU.** The instrumented run burns
   **5,873 CPU-seconds**; row reads are 25.6 s of it and the whole export index
   (including `declarations(file)`) is another 26.0 s. A `perf` profile of the
   uninstrumented run names where the CPU actually is: **path comparison,
   allocation and cache traffic in Rust module resolution** -- `ProjectFile::cmp`
   14.9 % (children), glibc allocator ~28 % self, moka ~12 %. SQLite and
   tree-sitter do not appear in the top 30 symbols.
3. **The name volume is real, and it is dominated by module-specifier
   resolution, not by the export index.** 456,452 `definitions(fq)` calls
   collapse to 29,190 distinct fq names, which expand to 128,873 short-name
   spellings (**4.41 per name**), 100,847 distinct, 100,890 reads. Only 14.5 %
   of those reads happen inside an export-index build.
4. **The work is overwhelmingly discarded.** 87.2 % of the distinct names read
   return **zero rows**. Of the 1,296,646 candidate rows fed to assembly,
   **12,726 units survive -- 99.02 % discarded**. Of 165,516 export-candidacy
   decisions, **92.2 % are rejections**.
5. **The data wants a per-file set shape, and it already half-admits it.** The
   request memo already answers 93.6 % of `definitions` calls and 92.4 % of
   `parent_of` calls without touching the store, which is why the residual read
   cost is small. What is *not* deduplicated is the **spelling expansion** (4.41
   global index probes per fq name, of which every spelling containing `::` is a
   structurally guaranteed miss -- **0 of 324,891 stored `short_name` values
   contain `::`**) and the **per-row discard** (99.02 %).

---

## Method and identity

| item | value |
| --- | --- |
| host | 120 CPUs, 98 GB RAM, kernel 6.18.33.2-microsoft-standard-WSL2 |
| host load | **quiet by this box's standards**: per-cell 1-min loadavg **4.0-8.2** (run 6 was 22-58, run 4 was 84-486) |
| concurrent tenants | one sibling agent's `m7` A/B campaign (120 s scans) overlapped part of the window |
| workspace | rustc tree `rust--01f6ddf7`, copied read-only from `/mnt/T9/repo-clones/.codescale-sources/` to `/mnt/containers/bifrost-latency-probe/m8/rust-tree` |
| cache | `bifrost_cache.v18.db`, **844.1 MB**, one per binary, each built from scratch by that binary (v17 in run 6; the schema version moved, the size did not: 847 -> 844 MB) |
| cell | `scan_usages_by_reference`, `compiler/rustc_target/src/spec/mod.rs#SanitizerSet`, `max_duration_secs=600` (**clamped to 300** by `SCAN_USAGES_MAX_DURATION_CEILING`) |
| env | `BIFROST_CACHE_GC=off BIFROST_SEMANTIC_INDEX=off`, featureless release build, no `nlp`, **no `BIFROST_TIMING`** |
| binaries | `base` = `2259d633` clean, sha256 `9dc42fc2ed953908`; `probe` = same commit + the m8 probe, sha256 `c5c925fc7bb84a6c`; `knobs` = `9df5558f` clean, sha256 `9c69461e52534bda` (add-on cell only) |

The main worktree was dirty on arrival (a sibling agent's in-progress
reader-pool and page-cache pragma work). **Both main binaries were built from clean
detached worktrees pinned at `2259d633`**, so none of that uncommitted work is in
the subject (the add-on cell's third binary is pinned at `9df5558f`, which is
that work as committed). That matters for reading the numbers: at `2259d633` the reader pool
capacity is still `available_parallelism()` = 120 and the reader pragmas are
still `cache_size = -65536` / `mmap_size = 268435456`.

### The probe does not distort the measurement

| | wall (s) | user (s) | sys (s) | **CPU (s)** | peak RSS (GB) | hits |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `probe` (instrumented, `BIFROST_P8=1`) | 1,005.7 | 4,491.6 | 1,381.3 | **5,872.9** | 15.38 | 8 |
| `base` (clean, `perf` attached for 240 s) | 1,073.3 | 4,552.0 | 1,471.8 | **6,023.9** | 15.66 | 8 |

**2.5 % apart in CPU**, same result, same `complete=false`
(`incomplete_reason=time_budget`). Every probe figure below can be read as the
product's own behaviour.

The probe is ~350 additive lines in a throwaway detached worktree
(`crates/bifrost-analysis/src/p8probe.rs` plus call-site instrumentation in
`store/mod.rs`, `tree_sitter_analyzer.rs`, `rust/graph_support.rs`, and one
`dump()` call in `src/bin/bifrost.rs`). Everything is behind
`BIFROST_P8=1`, checked once into a `OnceLock<bool>`.

---

## Q1 -- anatomy of one indexed read

### Q1a. The bare SQL is fast. It was never the cost.

`EXPLAIN QUERY PLAN` on the exact production statement
(`definition_order_candidate_sql`, `in_declarations = 1`), against a copy of the
same 844 MB cache with the production reader pragmas:

```
SEARCH units USING INDEX idx_code_units_lang_short_name (lang=? AND short_name=?)
SEARCH meta USING PRIMARY KEY (blob_oid=? AND lang=?)
CORRELATED SCALAR SUBQUERY 2
  SEARCH active_blob USING PRIMARY KEY (blob_oid=? AND lang=?)
  SEARCH active_epoch USING PRIMARY KEY (lang=?) LEFT-JOIN
CORRELATED SCALAR SUBQUERY 1
  SEARCH ranges USING PRIMARY KEY (blob_oid=? AND lang=? AND unit_key=?)
```

All seeks, no temp B-tree: `idx_code_units_lang_short_name` is
`(lang, short_name)` on a `WITHOUT ROWID` table keyed
`(blob_oid, lang, unit_key)`, so the index's implicit trailing PK columns
already satisfy `ORDER BY units.blob_oid, units.unit_key`. There are **five
b-tree seeks per returned row** (index entry, table row, `blob_meta`, `blobs`,
`unit_ranges`) plus the `analysis_epochs` probe.

Warm, single-threaded, `sqlite3` CLI 3.46.1, cumulative build-up of the same
statement (`Run Time: real`, second execution):

| name | rows | index-only count | + `blob_meta` join | + `EXISTS(blobs/epochs)` | **+ `MIN(unit_ranges)` = full** |
| --- | ---: | ---: | ---: | ---: | ---: |
| `main` | 21,942 | 15 ms | 21 ms | 31 ms | **43 ms** |
| `foo` | 3,355 | 3.7 ms | 5.5 ms | 7.1 ms | **12 ms** |
| `bar` | 1,113 | 0.6 ms | 2.1 ms | 3.1 ms | **4 ms** |
| `SanitizerSet` | 1 | -- | -- | -- | **0.07-0.12 ms** |
| absent name | 0 | -- | -- | -- | **0.02-0.07 ms** |

The two correlated subqueries roughly **triple** the per-row cost (15 -> 43 ms
for `main`), which is a real and avoidable multiplier -- but the absolute total
is 43 ms, i.e. **2.0 us per row**. The first (cold) execution of the `main`
query was 1.06 s; every later one is 27-49 ms.

Adjacent costs, measured separately: **opening a fresh read-only connection with
the production pragmas costs 0.26-0.68 ms**; **`BEGIN` + the `analysis_epochs`
generation check + `COMMIT` costs 4.2 us**.

**Concurrency ladder** -- N independent processes, each its own connection, each
running the full `main` query three times, quiet host:

| concurrent readers | avg real per query | avg user CPU per query |
| ---: | ---: | ---: |
| 1 | 35 ms | 27 ms |
| 8 | 34 ms | 24 ms |
| 32 | 42 ms | 33 ms |
| 120 | **110 ms** | **63 ms** |
| 240 | 184 ms | 66 ms |

120-way concurrency inflates the same query **3.1x in wall and 2.3x in CPU**.
That is a real term and it is *not* two orders of magnitude.

### Q1b. In-process layer table (100,890 reads, whole run)

Thread CPU and wall, accumulated inside
`declaration_order_candidate_rows_by_short_name_for_langs`:

| layer | **CPU (s)** | CPU % | wall (s) | wall % |
| --- | ---: | ---: | ---: | ---: |
| **whole store read** | **25.64** | 100 | **190.32** | 100 |
| `read_conn()` pooled checkout | **7.78** | **30.3 %** | 25.11 | 13.2 % |
| `conn.transaction()` (`BEGIN`) | 0.16 | 0.6 % | 0.17 | 0.1 % |
| `require_generation_map` | 1.39 | 5.4 % | 2.92 | 1.5 % |
| build the SQL text (`format!`) | 0.19 | 0.7 % | -- | -- |
| `prepare_cached` | 1.12 | 4.4 % | 6.25 | 3.3 % |
| `sqlite3_step` over all rows | **13.48** | **52.5 %** | **153.62** | **80.7 %** |
| row decode in Rust (`CandidateRow` + `Oid::from_str`) | 0.24 | 0.9 % | (inside step) | -- |
| `tx.commit()` | 0.55 | 2.1 % | 1.09 | 0.6 % |
| **per read, average** | **0.254 ms** | | **1.886 ms** | |
| rows returned, all reads | 74,335 (**0.74 per read**) | | | |
| language passes per read | **1.00** (Rust only; the per-lang loop does not multiply) | | | |

And the span the campaign has been quoting -- the whole memoized operation,
which is what `sql_definition_candidates.rows[name]` wraps:

| | n | CPU (s) | wall (s) |
| --- | ---: | ---: | ---: |
| memo op (`definition_candidate_rows`) | 128,873 | **26.51** | **372.65** |
| of which the store read | 100,890 | 25.64 | 190.32 |
| **memo + single-flight park + cache-handle lock** | | **0.87** | **182.32** |

Three things follow.

- **Rust-side assembly is not the cost.** Row decode is **0.9 %** of read CPU
  (0.24 s). Run 6's own span data agrees from the other side:
  `sql_definition_candidates.resolve_rows` was 6.15 s over 32,502 calls against
  1,255 s of `rows`.
- **Nor is the SQL, in CPU.** `sqlite3_step` is 13.48 CPU-seconds for the entire
  query -- for all 100,890 reads and all 74,335 rows.
- **The cost is time not spent computing.** `sqlite3_step` is 52.5 % of read CPU
  but **80.7 % of read wall**, an **11.4x CPU-to-wall gap**; the memo operation
  as a whole is **14.1x**. The threads are blocked, not busy. Corroborated
  independently by process-level sampling during the graph phase: **~11 of 120
  cores busy with 122 live threads**, i.e. roughly 110 threads not running at
  any instant.
- **One layer is a genuine, avoidable CPU term: `read_conn()` at 30.3 % of read
  CPU (7.78 s).** For a zero-row name it is usually the *whole* cost -- e.g. the
  read of `compiler.rustc_abi.src.layou...` (0 rows) cost 127 ms CPU of which
  252 ms wall was checkout. This is the 120-capacity pool creating and
  configuring connections (each `mmap_size = 256 MiB`, `cache_size = 64 MiB`),
  the same per-connection state run 5 charged 5.55 GB + 1.32 GB of RSS to.

### Q1c. Hot names versus rare names

Per-name, one read each (the memo is at its 1.0004 reads/key floor):

| name | rows | **CPU (ms)** | wall (ms) | bare SQL, 1 thread | run 6 span (loaded host) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `main` | 21,942 | **650.2** | 2,952 | 43 ms | 10,970 ms |
| `Foo` | 3,202 | 337.6 | 2,564 | -- | -- |
| `foo` | 3,355 | 324.1 | 2,555 | 12 ms | 11,158 ms |
| `bar` | 1,113 | 183.4 | 2,240 | 4 ms | 10,146 ms |
| `A` | 875 | 165.1 | 1,897 | -- | 9,051 ms |
| `Trait` | 964 | 164.7 | 1,921 | -- | 8,985 ms |
| `test` | 754 | 118.8 | 1,119 | -- | 9,693 ms |
| **rare: 0-row names** | 0 | **0.094** each (87,935 names, 8.27 s total) | -- | 0.02-0.07 ms | -- |
| **rare: 1-row names** | 1 | **0.456** each (9,441 names, 4.30 s total) | -- | ~0.1 ms | -- |

Full distribution by rows returned (all 100,847 distinct names):

| rows returned | distinct names | rows | read CPU (s) | share of read CPU |
| --- | ---: | ---: | ---: | ---: |
| **0** | **87,935 (87.2 %)** | 0 | **8.27** | **32.3 %** |
| 1 | 9,441 | 9,441 | 4.30 | 16.8 % |
| 2-3 | 2,017 | 4,572 | 2.37 | 9.2 % |
| 4-7 | 765 | 3,889 | 2.20 | 8.6 % |
| 8-15 | 364 | 3,887 | 1.58 | 6.2 % |
| 16-31 | 173 | 3,598 | 1.41 | 5.5 % |
| 32-255 | 129 | 8,734 | 1.94 | 7.6 % |
| 256-4095 | 22 | 18,270 | 2.98 | 11.6 % |
| 16384+ (`main` alone) | 1 | 21,942 | 0.65 | 2.5 % |

**The 5-11 s figure is a wall-clock artefact of contention, in three separable
parts, none of which is "the query is slow":** (1) the bare statement, 43 ms at
worst; (2) 120-way reader concurrency, a measured 2.3-3.1x; (3) everything else
-- the thread being descheduled while 120 rayon workers and 120 SQLite
connections contend for a machine that is only running ~11 of them at a time.
On a quiet host part (3) shrinks the same read from 10,970 ms to 2,952 ms while
its CPU stays at 650 ms.

---

## Q2 -- provenance of the 100,847 names

### The chain, with counts

```
scan_usages_by_reference (one symbol)
  -> candidate discovery -> 2,759 files whose export index gets built
  -> graph scan over candidate files
       IAnalyzer::definitions(fq_name)                     456,452 calls
         request memo hit                                  427,262   (93.6 %)
         miss -> sql_definition_candidates_vec              29,190   (6.4 %)
              -> definition_candidate_short_names(fq)      128,873 spellings (4.41 per fq)
                   -> definition_candidate_rows(spelling)  128,873 memo ops
                        -> store read                      100,890  (100,847 distinct keys,
                                                                     1.0004 reads/key)
                        -> rows out of the store            74,335
              -> rows fed to assemble_definition_candidates 1,296,646
              -> CodeUnits surviving assembly                  12,726  (99.02 % discarded)
```

**Where the 456,452 `definitions` calls come from.** Counted: `parent_of`
(`definition_parent_unit`) makes **87,119** of them (19.1 %), of which 80,480
(92.4 %) are served by the request-scoped `parent_units` memo. The remaining
~369k are not counted individually, but the code path and the CPU profile agree
on the dominant caller: **`resolve_module_files`**
(`graph_support.rs:1175`) asks `definitions(&resolved_module)` once per resolved
module specifier, and the independent `perf` profile puts
`rust_module_files_from_segments`, `RustUsageWalks::resolve_segments`,
`ProjectFile::exists` and `PathBuf::normalize` among the top named frames.
**Stated as code-path attribution corroborated by a profile, not as a counter.**

### The spelling expansion is the multiplier, and half of it cannot match

`definition_candidate_short_names` runs `lookup_suffix_candidates(fq, [".", "::"])`
(the default adapter; Rust does not override it). For
`a::b::T.m` that yields `{a::b::T.m, b::T.m, T.m, m}` -- **4.41 spellings per fq
name, measured**.

**Zero of the 324,891 persisted `short_name` values contain `::`** (verified by
query). Every spelling carrying a `::` is therefore a **structurally guaranteed
miss** that still pays a pooled connection checkout, a `BEGIN`, a generation
check, a `prepare_cached` and an index probe. This is consistent with the
measured 87.2 % zero-row rate, though the exact `::` share of the 100,847 was
not counted -- see Limitations.

By contrast the `.`-bearing spellings *can* match: Rust `short_name` is a dotted
owner-member suffix, 168,733 of 319,339 Rust units carry a `.`, to a maximum
depth of 6.

### Names per file

Per export-index build, distinct short names asked while the build runs:

| distinct names asked | builds |
| --- | ---: |
| 0 | **1,754 (63.6 %)** |
| 1 | 40 |
| 2-3 | 230 |
| 4-7 | 422 |
| 8-15 | 186 |
| 16-31 | 69 |
| 32-127 | 43 |
| 128-2047 | 15 |

Nearly two thirds of files ask the store nothing at all -- the request memos
already hold the answers. The distribution is long-tailed, not uniform: the
worst single build (`compiler/rustc_passes/src/errors.rs`, 455 declarations)
asked **496 distinct names**.

### How much of the answer is thrown away

| question | measured |
| --- | ---: |
| distinct names read that return **zero rows** | **87,935 / 100,847 = 87.2 %** |
| candidate rows fed to assembly vs units kept | **1,296,646 -> 12,726 = 99.02 % discarded** |
| `definitions(fq)` calls answering **empty** | 16,887 / 456,452 (3.7 %) -- but 16,797 `parent_of` calls resolve to no owner |
| **export candidacy decisions rejected** | **152,599 / 165,516 = 92.2 %** |
| ... rejected because the declaration is not export-visible | 89,062 (53.8 %) |
| ... rejected walking the owner chain | 63,537 (38.4 %) |
| ... accepted | 12,917 (7.8 %) |
| declarations examined vs export entries emitted | **166,102 -> 18,184 = 89.1 % emit nothing** |
| `parent_of` resolutions that land **in the asking file** | 47,724 / 70,322 resolved = **67.9 %** |
| `parent_of` resolutions that cross a file boundary | 22,598 = 32.1 % |

The 99.02 % row-discard number is the sharpest per-name statement available: the
store is asked a **global** question ("every declaration in the workspace whose
short name is `main`") to answer a **specific** one ("the unit whose fq name is
exactly this"), and 21,941 of `main`'s 21,942 rows exist only to be filtered out
by `assemble_definition_candidates`'s exact/normalized predicate.

Two-thirds of owner lookups resolve to a unit **in the file that asked**, whose
declaration set the caller already holds -- `export_index_of_declarations`
receives `declarations` as an argument and computes `export_visible` over
exactly that set before it starts asking the store per declaration.

---

## Also asked: `export_index_of_declarations`

Run 6 recorded it as `n=3,076 / 533.85 s / 38.1 % of the backend`. That figure is
a **thread-summed wall span on a loaded host, and it is inclusive of the store
reads nested inside it.** Measured here:

| quantity | value |
| --- | ---: |
| `export_index_of` calls (incl. cache hits) | 310,861 |
| ... served from the per-file cache | 308,102 (**99.1 %**) |
| **builds** (`export_index_of_declarations`) | **2,759** |
| build, inclusive **CPU** | **14.48 s** (5.2 ms per build) |
| build, inclusive **wall** | 70.95 s (25.7 ms per build) |
| build CPU excluding nested store reads | 12.71 s |
| `declarations(file)` (measured just outside the span) | **11.47 s CPU** (4.16 ms per build) |
| **total per-file export-index work** | **~25.95 s CPU = 0.44 % of the query's 5,873 CPU-s** |

Per-call split of that work:

| component | CPU (s) | wall (s) | shape |
| --- | ---: | ---: | --- |
| `declarations(file)` | 11.47 | -- | **per-file bounded** (file's own unit set) |
| `export_visible_declarations` (read source, `parse_rust_tree`, per-declaration node lookup) | 5.39 | 21.18 | **per-file bounded** (one parse of one file) |
| `re_exports_of` (persisted per-file export facts) | 5.21 | 23.34 | **per-file bounded** (one fact row set) |
| the candidacy loop (`is_module_export_candidate` -> `parent_of` chain) | 3.87 | -- | **workspace-shaped**: a global `(lang, short_name)` index probe per distinct owner name |
| store reads nested under a build | 1.77 | -- | workspace-shaped |

| shape | declarations in | entries out | stars |
| --- | ---: | ---: | ---: |
| per build | 60.2 | 6.6 | 0.14 |
| total | 166,102 | 18,184 | 384 |

**Answer to the sub-question: the per-call work is per-file-bounded in the
large, workspace-shaped in the small.** 22.07 of 25.95 CPU-seconds (85 %) is
reading and parsing one file and its own fact rows; 5.64 s (22 %) is the owner
chain and its nested global index probes. The measured cost of a build is
**5.2 ms CPU / 25.7 ms wall**, not 173 ms; the run-6 figure is blocked time on a
loaded host, most of it charged twice because the span nests the row reads.

Two incidental observations. **The build is not single-flighted**: the same file
appears three times in the top-60 build list
(`compiler/rustc_middle/src/ty/generics.rs`), so concurrent misses on
`export_indexes` each build the index. And 63.6 % of builds issue **no store
read at all**.

---

## Where the CPU actually is

`perf record -F 99 -g` on the clean `base` binary, 308,553 samples over 240 s
starting ~90 s into the run (spanning the end of candidate discovery and the
start of the graph phase). Self time, top symbols:

| self % | symbol |
| ---: | --- |
| 10.43 | `std::path::compare_components` (10.13 via `ProjectFile as Ord::cmp`) |
| 8.52 | `_int_free_chunk` |
| 8.17 | `realloc` (5.31 `RawVec::finish_grow`, of which 3.09 `PathBuf::_push` under `NormalizePath::normalize`, 2.21 `Path::_join`) |
| 4.85 | `_int_malloc` |
| 4.27 | `__libc_malloc2` |
| 3.26 | `Vec::from_iter` |
| 2.96 | `moka::...::Deques::move_to_back_ao` |
| 2.95 | `__memcmp_evex_movbe` |
| 2.85 + 1.70 + 1.50 | `Path::Components::next` / `next_back` / `Path::components` |
| 2.51 + 2.02 + 1.07 | `moka::cht::...::get_key_value_and_then` |
| 2.31 + 1.28 | `crossbeam_epoch` |
| 1.89 + 1.17 + 1.08 | `crossbeam_channel::...::try_send` (moka's write queue) |
| 1.64 / 0.87 / 0.90 | `ProjectFile::cmp` / `ProjectFile::eq` / `Path::hash` |

Rolled up: **~28 % glibc allocator, ~22 % path comparison / iteration / hashing,
~12 % moka cache machinery, ~4 % crossbeam channels.** With children:
`ProjectFile as Ord::cmp` **14.90 %**, `PathBuf::normalize` 5.30 %,
`rust_module_files_from_segments` 2.92 %,
`drop_in_place<RustImportEdge>` 2.44 %, `ProjectFile::exists` 2.30 %,
`RustUsageWalks::resolve_segments` 1.91 %, `rust_crate_root_package` 1.25 %.

**Neither SQLite nor tree-sitter appears in the top 30 self entries.** The
graph phase's CPU is spent building, normalizing, joining, hashing and comparing
`ProjectFile` paths inside Rust module resolution, and freeing the allocations
that work makes.

---

## Add-on cell: the reader knobs at rustc scale (`2259d633` vs `9df5558f`)

Requested mid-task: one A/B confirming whether the `+7 %` sys-CPU price
`9df5558f` reported on a mid-size cell holds, shrinks or grows on the rustc
tree. `9df5558f` sets `mmap_size` 256 MiB -> 0, `cache_size` 64 -> 8 MiB, and
caps reader-pool **idle retention** at `min(available_parallelism(), 16)`.

Both binaries built from clean detached worktrees, each with its own cache built
by itself; same tree, same cell, same 600 s (clamped 300 s) budget; **run
back-to-back, never concurrently**; memory sampled every 2 s from
`/proc/<pid>/smaps_rollup` by an external witness.

| | **base `2259d633`** | **knobs `9df5558f`** | delta |
| --- | ---: | ---: | ---: |
| **user CPU (s)** | **4,225.97** | **4,304.73** | **+1.9 %** |
| **sys CPU (s)** | **1,355.42** | **1,449.07** | **+6.9 %** |
| total CPU (s) | 5,581.39 | 5,753.80 | +3.1 % |
| wall (s, context) | 1,068.4 | 1,003.1 | -6.1 % |
| **peak RSS, `time -v` (GB)** | **14.88** | **4.97** | **-66.6 %** |
| witness peak RSS (GB) | 14.29 | 4.80 | -66.4 % |
| **witness peak PSS (GB)** | **6.40** | **4.80** | **-25.0 %** |
| witness peak `Private_Dirty` (GB) | 6.12 | 4.77 | -22.1 % |
| cache-DB mappings at peak | **123** | **2** | -- |
| hits / files / `complete` | 11 / 6 / false | 11 / 6 / false | identical |
| loadavg 1-min before -> after | 31.0 -> 3.5 | 3.4 -> 6.6 | -- |

**The price holds at rustc scale and is smaller than the "+10 % CPU" worry, but
it is real and it is where the commit said it would be.** Sys CPU is **+6.9 %**,
matching the mid-size cell's +7 % almost exactly -- it neither shrank nor grew
with a 5x larger workspace. User CPU, the load-independent term on this host, is
**+1.9 %**; total CPU **+3.1 %**. Wall improved 6.1 %, which is context only.

**The memory result splits exactly the way run 5 predicted.** RSS falls 66.6 %
because 121 of 123 duplicate mappings of one 845 MB file disappear -- but those
were multiply-counted clean pages, so **PSS, the honest meter, falls 25.0 %**
(6.40 -> 4.80 GB) and `Private_Dirty` 22.1 %. The genuine saving is the
per-connection page cache (64 -> 8 MiB across retained readers), not the mmap.
Note also that at `9df5558f` **RSS and PSS converge** (4.80 vs 4.80): with the
mapping gone there is nothing left to double-count, so the campaign's peak-RSS
figures become meaningful again rather than needing run 5's 1.3-3.5x multiplier.

Both cells returned the **full eleven-hit set** -- a fourth independent instance
of the run-3 answer from a full-scope truncated sweep, further corroborating
that the 8-vs-11 split is deadline coverage, not resolution.

Caveats: **one repetition per side**; the two cells ran under different
1-minute loadavg (the base cell started as a 31.0 spike decayed), and per the
owner's standing directive **sys CPU is not a load-independent quantity on this
host** -- so the +6.9 % is the least trustworthy figure in the table, even
though it reproduces the commit's own number. The +1.9 % user CPU and all four
memory figures are load-independent.

---

## What these facts constrain (no proposals)

Stated as constraints on the design space, each tied to the measurement that
establishes it.

1. **Any design justified by "the per-read cost of a high-cardinality short
   name" is aimed at 25.6 CPU-seconds out of 5,873.** Even eliminating the store
   read entirely -- every read, every name -- removes 0.44 % of the query's CPU
   and at most 190 s of thread-summed wall out of a 1,272 s phase whose wall is
   overwhelmingly blocked time. *(Q1b layer table; probe/base CPU control.)*
2. **Wall-clock spans on this workload measure contention, not work.** The
   `rows[*]` span is 14.1x its own CPU; `sqlite3_step` is 11.4x; the process runs
   ~11 of 120 cores with 122 threads. Any gate, estimate or attribution read off
   `BIFROST_TIMING` span seconds is reading a queueing artefact. Run 6's 10.97 s
   for `main`, run 6's 533.85 s for `export_index_of_declarations`, and the
   "~940 s recoverable" estimate on #1748 are all of this kind. *(Q1b; core
   occupancy sampling.)*
3. **The three multipliers on the read are separable and measured**: bare
   statement 43 ms worst case, of which the two correlated subqueries are ~2.9x
   (15 -> 43 ms); 120-way reader concurrency 2.3x CPU / 3.1x wall; and the pooled
   connection checkout, which is **30.3 % of read CPU** and effectively the
   entire cost of a zero-row read. The last is a property of the 120-capacity
   pool and its per-connection 256 MiB mmap / 64 MiB page cache -- the same
   configuration run 5 charged 6.87 GB of RSS to. *(Q1a ladder; Q1b layer table.)*
4. **Deduplication is finished as a lever.** 1.0004 reads per key, 93.6 % memo
   hit rate on `definitions`, 92.4 % on `parent_of`, 99.1 % on `export_index_of`.
   There is no remaining duplication of this shape to remove. *(Q2 chain.)*
5. **The two undeduplicated multipliers are spelling expansion and row
   discard.** 4.41 index probes per fq name, with every `::`-bearing spelling a
   guaranteed miss against a column in which no stored value contains `::`; and
   99.02 % of candidate rows discarded because a global short-name question is
   being asked to answer a specific fq-name question. These are the only two
   places where the per-name work is provably answering something never used.
   *(Q2 discard table; `short_name` shape query.)*
6. **A per-file set question is already available for two thirds of the owner
   lookups.** 67.9 % of resolved `parent_of` owners live in the file that asked,
   and `export_index_of_declarations` is handed that file's whole declaration set
   before it starts asking. The remaining 32.1 % genuinely cross a file boundary
   (`mod x;` in a parent module file), so a purely file-local answer is
   incomplete, not wrong. *(Q2 parent table.)*
7. **The export index is not the problem it looked like.** 25.95 CPU-seconds
   total, 85 % of it per-file-bounded parse-and-fact work, 5.2 ms per build.
   What is wasteful there is proportion, not absolute cost: 92.2 % of candidacy
   decisions are rejections and 89.1 % of declarations emit no entry. It is also
   not single-flighted, so concurrent misses rebuild the same file's index.
   *(Export-index anatomy.)*
8. **The measured CPU sink is Rust module resolution's path handling**, not
   anything either question named: ~22 % path compare/iterate/hash, ~28 %
   allocator churn feeding it, ~12 % moka. `ProjectFile` is compared as a
   `std::path::Path` component sequence and rebuilt by `PathBuf::push` /
   `Path::join` / `NormalizePath::normalize` inside
   `rust_module_files_from_segments` and `resolve_segments`. This is the term
   that would have to move for the phase to move. *(perf profile, 308k samples,
   clean binary.)*
9. **The reader-knob change is a memory win with a small, real CPU price, and
   it does not touch any term in constraints 1-8.** At rustc scale: user CPU
   +1.9 %, sys CPU +6.9 %, PSS -25.0 %, RSS -66.6 %, cache-DB mappings 123 -> 2,
   identical eleven-hit answer. It removes run 5's RSS double-counting entirely
   (RSS and PSS converge), so peak-RSS gates become readable again -- but it
   moves none of the path-handling CPU that constraint 8 names.
   *(Add-on A/B cell.)*
10. **Deadline truncation still governs the cell.** `max_duration_secs=600` is
   clamped to 300 s; both runs are `complete=false` with `reason=time_budget` and
   returned 8 hits. Nothing here is a measurement of a completed query.

---

## Limitations

1. **One repetition per configuration.** Two answering-cell runs (probe and
   base) on the same tree and matched caches; they agree to 2.5 % in CPU. The
   SQL-side measurements are 2-3 repetitions each.
2. **The exact `::` share of the 100,847 distinct names was not counted.** The
   probe dumps only the top 200 names by CPU (none of which contain `::`) plus a
   rows-returned histogram. The claim that `::`-bearing spellings are guaranteed
   misses is established structurally (0 of 324,891 stored `short_name` values
   contain `::`) and is consistent with the 87.2 % zero-row rate, but the share
   is inferred, not measured. A counter would settle it in one rebuild.
3. **The non-`parent_of` share of `definitions` calls (~369k of 456k) is
   attributed by code path plus profile, not by a counter.** The named caller,
   `resolve_module_files`, is the one the profile independently ranks; no probe
   counted its calls.
4. **`perf` stack unwinding is truncated** (release build, no frame pointers),
   so the children percentages are lower bounds and the caller trees are partial.
   The self-time table does not depend on unwinding and is the load-bearing part.
5. **The `perf` window is 240 s of a 1,073 s run**, starting ~90 s in. It covers
   the transition from candidate discovery into the graph phase, not the tail.
6. **`sqlite3` CLI 3.46.1 versus rusqlite 0.32.1's bundled SQLite.** Same major
   version, but the bare-SQL numbers are a different build of the engine on a
   copy of the database, with the production pragmas applied by hand.
7. **Per-name `step_ms` and `decode_ms` columns in the raw probe dump are
   invalid** and are not used anywhere above: they were computed as deltas of
   process-global atomics across a read, so concurrent threads contaminate them.
   Per-name `cpu_ns`, `wall_ns`, `rows`, `reads` and `conn_ns` are measured
   locally and are sound.
8. **The subject is `2259d633`, not the working tree.** A sibling agent's
   uncommitted work (reader pool capacity 120 -> 16, `cache_size` 64 -> 8 MiB,
   `mmap_size` -> 0) would change constraint 3 materially and is not measured
   here.

---

## Cleanup (confirmed)

| item | state |
| --- | --- |
| `/mnt/T9/repo-clones/.codescale-sources/rust--01f6ddf7` | **never written**; read only, by `cp -a` |
| `/mnt/containers/bifrost-latency-probe/m8` (tree copy, 4 caches, 3 worktrees, 3.8 GB) | **removed**, absence verified |
| git worktrees `wt-base`, `wt-probe`, `wt-knobs` | **removed** with `git worktree remove --force`, then `git worktree prune`; 0 worktrees match `latency-probe/m8` |
| isolated Cargo targets | **none remain** (`/tmp/bifrost-cargo-target.*` absent); all three builds logged "Removed isolated Cargo target" |
| benchmark processes | **none remain**, verified by `pgrep -x` |
| scratch binaries | the three 113 MB binaries deleted after the last cell; scratch is 28 MB |
| main worktree `/mnt/optane/bifrost-nlp` | **no source file touched**; it was already dirty on arrival with another agent's work, which was left untouched |
| `/mnt/containers/bifrost-latency-probe/m7` | another agent's campaign; **not touched** |
| artefacts kept | `scratchpad/m8/` (probe dump, `results.txt`, `perf.data`, SQL scripts, `.time` files) |
