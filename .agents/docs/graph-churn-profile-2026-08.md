# The graph phase's churn, profiled at HEAD (m13)

Date: 2026-08-09. Subject: `bifrost-nlp-ft` HEAD **`946710c4`** ("Let a scan's
deadline reach the reads it spends its budget in"). Read-only measurement: no
source file in the main worktree was touched, no commit was made.

Predecessor and comparison basis: `.agents/docs/graph-read-cost-investigation-2026-08.md`
(m8, at `2259d633`) and `usage-graph-d4-final-v1.md` (run 10, at `50666910`).

**The concurrent MultiAnalyzer prefetch fix is NOT in the subject. It landed
mid-run.** At build time (2026-08-08 23:09) HEAD was `946710c4`, which records
the prefetch gap as an unfixed finding in its own message ("MultiAnalyzer ...
overrides `import_infos_for_files` but not `prefetch_import_targets`, so
#1748's batch never fires on a multi-language workspace"). The fix committed as
**`c2592c3f` ("Let the import-target batch reach a multi-language workspace")
at 23:35:34**, between this run's profiled answering cell (finished ~23:34) and
its A/B cells (started 23:48). **Both binaries were built from clean detached
worktrees pinned at `946710c4` before that commit existed**, so every cell in
this report -- profile, A/B and ladder alike -- is the same pre-fix subject.
Its effect lands squarely on one term measured here: it removes the 9,648
per-`use` point reads from the discovery window, which is the input to the sys
ladder in section 3. It does not touch any path or allocator term in sections
1-2. **Section 3's sys figures should be re-taken at `c2592c3f` before anyone
quotes them as current.**

**Owner directive in force**: CPU-time basis. Load is labelled per cell and
never waited on.

---

## Headline

1. **The churn survived. It did not shrink, it grew slightly as a share.** The
   m8 profile and the HEAD profile are the same profile. `compare_components`
   **10.43 -> 11.87 %** self; `ProjectFile as Ord::cmp` **14.90 -> 18.04 %**
   with children. Not one m8 term dropped out of the table. The 26 % row-read
   cut and the 40 % export-build cut removed work that was never in the
   profile: run 10's inference was right.
2. **Rust module resolution's path handling now holds ~31 % of query CPU in the
   m8-comparable window and ~38 % in the graph-phase tail** (lower bounds, same
   attribution method on both runs; m8 measures **28.5 %**).
3. **The sized target for an ID/interning change is 25-27 % of graph-phase CPU
   in self time alone** (path 22.2 % + `ProjectFile` own 5.0 % in the tail),
   **plus ~12 % of the window that is allocator churn provably fed by path
   construction** (`PathBuf::_push` 6.13 %, `Path::_join` 4.64 %,
   `rust_crate_root_package` 1.49 %). Total attackable surface: **~38 % of the
   graph phase**, lower bound.
4. **The sys-CPU flag: mechanism 1 dominates, mechanism 2 is real but small.**
   Restoring `mmap_size = 256 MiB` removes **3.12 s of 23.42 s** of cell (a)
   sys (**-13.3 %**, 3 reps a side, interleaved). Scaling the budget instead
   moves sys from **7.36 s at a 1 s budget to 23.79 s at 3 s to 65.61 s at
   6 s**, against a construction-only floor of **7.9-8.6 s**. The scan window's
   syscall traffic is **99.1 % `pread64`**; every path syscall in the process
   (`readlink` 352 k, `newfstatat` 124 k, `statx` 41 k) is construction, not
   scan, and is unchanged since run 6.
5. **The 0.3 s overshoot residue is not a CPU term and is not visible as one.**
   SQLite is **1.40 % / 0.61 %** of the two windows; `sqlite3VdbeExec` is
   **0.55 % / 0.00 %**. What the syscall census does show is that the scan
   window is ~100 % `pread64` at **~21-36 us each on this host**, which is the
   shape the `946710c4` message predicted when it named `sqlite3_progress_handler`
   (VM steps) over the 512-row interval. Observation only.

---

## Method and identity

Matched to m8 so the two profiles are directly comparable.

| item | value |
| --- | --- |
| host | 120 CPUs, 98 GB RAM, kernel 6.18.33.2-microsoft-standard-WSL2 |
| host load | 1-min loadavg **5.8 at cell start, 7.9 at window A, 10.5 at window B**; A/B cells at **1.1-5.9**; budget ladder at **1.5-2.7**. A sibling agent's `clippy-driver` (2 cores of 120) appeared after window B and ran through the A/B; it is present on both sides of every paired comparison. |
| workspace | rustc tree `rust--01f6ddf7`, `cp -a` from `/mnt/T9/repo-clones/.codescale-sources/` (read only, never written) to `/mnt/containers/bifrost-latency-probe/m13/rust-tree`, 438 MB, 35,370 `.rs` files |
| cache | `bifrost_cache.v18.db`, **845.0 MB** (base) / **845.0 MB** (mmap), one per binary, each built from scratch by that binary via a warm `get_symbol_sources` miss |
| answering cell | `scan_usages_by_reference`, `compiler/rustc_target/src/spec/mod.rs#SanitizerSet`, `max_duration_secs=600` (**clamped to 300** by `SCAN_USAGES_MAX_DURATION_CEILING`) |
| gate cell (a) | same symbol, **no `max_duration_secs`** (product default 3 s), used for the sys-CPU work |
| env | `BIFROST_CACHE_GC=off BIFROST_SEMANTIC_INDEX=off`, featureless release build, no `nlp`, **no `BIFROST_TIMING`** |
| binaries | `base` = `946710c4` clean, sha256 `4f8bef75a3ae4254`; `mmap` = same commit + a one-line change (`mmap_size` 0 -> 268435456 in `configure_readonly_page_cache`), sha256 `d7c003b556c19101`. Both built from **clean detached worktrees pinned at `946710c4`** via `scripts/with-isolated-cargo-target.sh`. |
| profiler | `perf record -F 99 -g -p <pid>`, i.e. `cycles:Pu`. **`perf evlist -v` confirms byte-identical event config to m8's `perf.data`**: `PERF_COUNT_HW_CPU_CYCLES`, `sample_freq 99`, `IP|TID|TIME|CALLCHAIN|PERIOD`, `exclude_kernel: 1`, `inherit: 1`. |

### Two windows, not one

m8 took one 240 s window starting ~90 s into a 1,073 s run, covering the end of
candidate discovery and the start of the graph phase. This run takes that same
window **plus** a second one in the tail, on the same process:

| window | span | samples | user-cores busy (samples/s / 99) |
| --- | --- | ---: | ---: |
| **A** (m8-comparable) | 90 s -> 330 s | **293,301** | **12.34** |
| **B** (graph-phase tail) | 600 s -> 840 s | **77,372** | **3.26** |
| m8 | ~90 s -> 330 s | 308,553 | 12.99 |

Window A is the like-for-like comparison. Window B is new information: the tail
of the graph phase runs at **a quarter of window A's CPU occupancy**, which is
the same "blocked, not busy" fact m8 established from the CPU-to-wall ratio,
seen from the sampling side.

### The cell reproduces run 10

| | run 10 untimed | **m13 (this run, perf attached)** |
| --- | ---: | ---: |
| wall (s) | 1,070.5 | **1,076.6** |
| user CPU (s) | 4,454.3 | **4,350.7** |
| sys CPU (s) | 1,499.6 | **1,411.2** |
| peak RSS (GB) | 4.69 | **4.93** |
| result | resolved=1, **11 hits**, 6 files | resolved=1, **11 hits**, 6 files |
| `complete` / reason | false / `time_budget` | false / `time_budget` |

Within 2.3 % on user CPU and identical in answer. Whatever the profile says
about this process is what run 10 measured.

---

## (1) The delta table

Self time, `--no-children -g none --sort symbol`, duplicate symbol rows merged
by name. Both columns produced by the same script from the two `perf.data`
files. Window A against m8.

| # | HEAD (A) | m8 | delta | symbol |
| ---: | ---: | ---: | ---: | --- |
| 1 | **11.87** | 10.43 | **+1.44** | `std::path::compare_components` |
| 2 | 7.95 | 8.52 | -0.57 | `_int_free_chunk` |
| 3 | 7.33 | 8.17 | -0.84 | `realloc` |
| 4 | 5.12 | 5.85 | -0.73 | `moka::BucketArrayRef::get_key_value_and_then` |
| 5 | 4.40 | 4.85 | -0.45 | `_int_malloc` |
| 6 | 4.36 | 4.27 | +0.09 | `__libc_malloc2` |
| 7 | **4.26** | 2.85 | **+1.41** | `<path::Components as Iterator>::next` |
| 8 | 3.71 | 4.22 | -0.51 | `crossbeam::Channel::try_send` |
| 9 | 3.60 | 3.86 | -0.26 | `Vec::from_iter` (`SpecFromIterNested`) |
| 10 | 2.77 | 2.95 | -0.18 | `__memcmp_evex_movbe` |
| 11 | 2.43 | 2.31 | +0.12 | `crossbeam_epoch::Global::try_advance` |
| 12 | 2.34 | 2.96 | -0.62 | `moka::Deques::move_to_back_ao` |
| 13 | **2.07** | 1.64 | **+0.43** | `<ProjectFile as cmp::Ord>::cmp` |
| 14 | 1.82 | 2.39 | -0.57 | `moka::base_cache::Inner::do_run_pending_tasks` |
| 15 | 1.77 | 1.50 | +0.27 | `<path::Path>::components` |
| 16 | 1.74 | 1.70 | +0.04 | `<path::Components as DoubleEndedIterator>::next_back` |
| 17 | 1.44 | 0.94 | +0.50 | `pthread_mutex_lock` |
| 18 | 1.28 | 1.32 | -0.04 | `crossbeam_epoch::default::with_handle` |
| 19 | 1.13 | 0.85 | +0.28 | `pthread_mutex_unlock` |
| 20 | 1.06 | 1.21 | -0.15 | `<path::Path as hash::Hash>::hash` |
| 21 | 1.03 | 1.27 | -0.24 | `crossbeam::Channel::try_recv` |
| 22 | 1.02 | 0.99 | +0.03 | `malloc` |
| 23 | 0.97 | 1.10 | -0.13 | `Iterator::find_map::check::{{closure}}` |
| 24 | 0.95 | 0.87 | +0.08 | `<ProjectFile as cmp::PartialEq>::eq` |
| 25 | 0.92 | 0.93 | -0.01 | `__memmove_avx512_unaligned_erms` |
| 26 | 0.88 | 0.94 | -0.06 | `<sip::Hasher>::write` |
| 27 | 0.87 | 0.88 | -0.01 | `cfree` |
| 28 | 0.85 | 0.78 | +0.07 | `RustUsageWalks::resolve_segments` |
| 29 | 0.63 | 0.66 | -0.03 | `str::join_generic_copy` |
| 30 | 0.63 | 0.71 | -0.08 | `slice::memchr::memchr_aligned` |
| 31 | 0.60 | 0.66 | -0.06 | `moka::BaseCache::get_with_hash::{{closure}}` |
| 32 | 0.55 | 0.41 | +0.14 | `sqlite3VdbeExec` |
| 33 | 0.49 | 0.48 | +0.01 | `<path::Components>::parse_next_component_back` |
| 34 | 0.46 | 0.45 | +0.01 | `<str::pattern::CharSearcher>::next_match` |

**Nothing left the table.** The largest single decline is
`crossbeam::Channel::try_send` at -0.51, and that is moka's write queue, i.e.
second-order cache traffic. The largest single rise is
`compare_components` at +1.44.

### The same table for the graph-phase tail (window B)

The tail is where the phase actually spends its 1,087 s, and there the shift is
sharper still.

| HEAD (B) | m8 | delta | symbol |
| ---: | ---: | ---: | --- |
| 10.42 | 8.17 | **+2.25** | `realloc` |
| 9.94 | 10.43 | -0.49 | `compare_components` |
| 8.90 | 8.52 | +0.38 | `_int_free_chunk` |
| **8.09** | 4.85 | **+3.24** | `_int_malloc` |
| 4.76 | 2.85 | **+1.91** | `<path::Components as Iterator>::next` |
| 3.72 | 2.95 | +0.77 | `__memcmp_evex_movbe` |
| **3.20** | 0.87 | **+2.33** | `<ProjectFile as cmp::PartialEq>::eq` |
| 2.76 | 1.70 | +1.06 | `Components::next_back` |
| 0.91 | 4.22 | **-3.31** | `crossbeam::Channel::try_send` |

### Bucket rollups, one script, all three profiles

Self time, bucketed by regex on the symbol name. `other` is dominated by
generic `core`/`alloc` iterator and drop glue that the bucket rules do not
claim.

| bucket | m8 | **HEAD A** | **HEAD B** |
| --- | ---: | ---: | ---: |
| allocator (glibc) | 19.29 | 18.33 | **23.31** |
| path | 18.95 | **21.91** | **22.16** |
| moka | 13.47 | 11.30 | 9.67 |
| crossbeam | 9.29 | 8.64 | 5.86 |
| `ProjectFile` (own impls) | 2.68 | 3.19 | **4.99** |
| libc mem/str | 3.92 | 3.74 | 4.63 |
| locking | 1.80 | 2.57 | 1.51 |
| tree-sitter | 1.73 | 2.05 | 0.13 |
| hashing/hashmap | 1.43 | 1.31 | 1.37 |
| **sqlite** | **1.04** | **1.40** | **0.61** |
| other | 25.48 | 24.88 | 25.32 |

(These buckets are stricter than m8's hand rollup, which read ~28 % allocator
against this script's 19.29 % on the same data. The script is the comparable
instrument; m8's prose figures and these are not the same measure.)

### Children view, the named call paths

`--children`, percent-limit 0.4, duplicate rows merged.

| symbol (children %) | m8 | **HEAD A** | **HEAD B** |
| --- | ---: | ---: | ---: |
| **`<ProjectFile as Ord>::cmp`** | **14.90** | **18.04** | **16.29** |
| `<PathBuf as NormalizePath>::normalize` | 5.30 | 4.63 | **6.87** |
| `rust_module_files_from_segments` | 2.92 | 2.47 | **3.65** |
| `ProjectFile::exists` | 2.30 | 2.26 | **3.11** |
| `<ProjectFile as PartialEq>::eq` | 0.87 | 0.95 | **3.20** |
| `drop_in_place<RustImportEdge>` | 2.44 | 2.08 | 2.31 |
| `RustUsageWalks::resolve_segments` | 1.91 | 1.80 | 1.56 |
| `rust_crate_root_package` | 1.25 | 1.13 | 1.52 |
| `rust_current_crate_name` | 1.07 | 0.99 | 1.26 |
| `rust_package_components` | 0.53 | 0.48 | 0.42 |
| `sqlite3VdbeExec` | 0.41 | 0.55 | **0.00** |
| `sqlite3_step` | -- | 0.43 | **0.00** |

**Verdict on the design input: the churn SURVIVED, unchanged in kind and
slightly larger in share.** Rust module resolution's path handling is
**30.71 %** of the m8-comparable window and **38.19 %** of the graph-phase tail
(attribution method below), against m8's **28.49 %**. Neither SQLite nor
tree-sitter is a factor: SQLite is 1.4 % of window A and **0.61 %** of the tail,
and in the tail `sqlite3VdbeExec` and `sqlite3_step` do not appear at all above
the 0.4 % children limit.

---

## (2) What an ID/interning change would attack, sized

The shares moved only slightly, so m8's named terms stand. What this run adds
is a **size** for them, and a caveat about what cannot be sized.

### The attribution method

`perf script` folded stacks, one bucket per sample. A sample counts as
path-attributed if **any** frame in its stack matches
`std::path|PathBuf|Path>|Components|compare_components|NormalizePath|ProjectFile|path_normalization`.
Because release stacks are truncated (no frame pointers; 25-33 % of stacks are
depth 1), **every figure here is a lower bound.**

| | m8 | **HEAD A** | **HEAD B** |
| --- | ---: | ---: | ---: |
| stacks of depth 1 (unattributable) | 32.5 % | 29.8 % | 25.1 % |
| **stacks naming any path/`ProjectFile` frame** | **28.49 %** | **30.71 %** | **38.19 %** |
| allocator-leaf samples | 28.36 % | 27.10 % | 36.10 % |
| ... of which path-attributed | 8.45 % | 7.56 % | **12.03 %** |
| ... path share among allocator samples deep enough to attribute | 43.7 % | 42.0 % | **47.0 %** |

### The allocator's callers, window B

Frame immediately above the allocator leaf, as a share of the whole window:

| share of window B | caller |
| ---: | --- |
| **6.13 %** | `<std::path::PathBuf>::_push` |
| **4.64 %** | `<std::path::Path>::_join` |
| 4.18 % | (truncated) |
| 3.40 % | `<Map<I,F> as Iterator>::next` |
| 2.38 % | `drop_in_place<RustImportEdge>` |
| 1.82 % | `<String as Clone>::clone` |
| **1.49 %** | `rust_crate_root_package` |
| 1.34 % | `Vec::from_iter` |
| 1.12 % | `<String as fmt::Write>::write_str` |
| 0.77 % | `str::join_generic_copy` |

`PathBuf::_push` + `Path::_join` + `rust_crate_root_package` alone are
**12.26 %** of the graph-phase tail spent in `malloc`/`free`/`realloc` on behalf
of path construction.

### The sized target

**~38 % of graph-phase CPU (lower bound), decomposing as:**

| term | window B share | what an ID/arena change does to it |
| --- | ---: | --- |
| `path` self (compare/iterate/parse) | 22.16 % | a `u32` compare replaces a component walk |
| `ProjectFile` own impls (`cmp`/`eq`/`hash`) | 4.99 % | becomes an integer compare/hash |
| allocator churn under `PathBuf::_push`/`Path::_join`/`rust_crate_root_package` | 12.26 % | the allocations stop being made |
| **total** | **~39 %** (overlapping terms; ~38 % measured as distinct samples) | |

### The three concrete call sites the number is made of

Each read from source at `946710c4`, each consistent with the profile:

1. **`ProjectFile` is a path pair, compared as paths.**
   `crates/bifrost-core/src/analyzer/model.rs:1745-1855`:
   `ProjectFile(Arc<ProjectFileInner{ root: PathBuf, rel_path: PathBuf }>)`, and
   `Ord::cmp` is `self.0.root.cmp(&other.0.root)` then `rel_path`. **Every
   `ProjectFile` in a workspace carries the identical absolute `root`**, so the
   first half of every comparison is a scan of a byte prefix that is known equal
   by construction. (std's `compare_components` has a shared-prefix fast path, so
   the waste is a memcmp over the root rather than a component walk -- which is
   exactly why `__memcmp_evex_movbe` sits at 2.77-3.72 % self.) `Hash` and `eq`
   have the same shape.

2. **The membership test is a binary search over 35,370 of them.**
   `graph_support.rs:522` -- `RustPackageFileIndex::contains` is
   `self.files.binary_search(file)`, reached from
   `RustUsageWalks::is_analyzed` (`usage_walks.rs:373`) and
   `RustAnalyzer::resolve_module_files` (`graph_support.rs:1240`). That is
   **~15.1 `ProjectFile::cmp` calls per membership question**, each one scanning
   the shared root prefix. This is the single largest identified consumer of the
   18.04 % / 16.29 % `cmp` figure. A dense `u32` file id makes it one integer
   compare, or a `HashSet<FileId>` lookup.

3. **The path is rebuilt, and the package name re-derived, per resolution.**
   `rust_module_files_from_segments` (`graph_support.rs:1802`) constructs **four**
   `ProjectFile::new` candidates per module specifier -- each `new` normalizes
   both components -- and calls `.exists()` on each, which is
   `abs_path().exists()`, i.e. another `join` allocation plus a `stat`.
   `rust_package_components` (`declarations.rs:231`) and `rust_crate_root_package`
   (`imports.rs:808`) each allocate a `Vec<String>` with **one heap `String` per
   path component** plus a joined `String`, per call; `ModuleKey::new`
   (`usage.rs:174`) does it again. These are the `PathBuf::_push` (6.13 %),
   `Path::_join` (4.64 %) and `rust_crate_root_package` (1.49 %) allocator rows
   above, and they are what interning a package name per file would remove.

### What could not be sized, and why

**The caller tree for `ProjectFile::cmp` is unavailable in this profile.** With
truncated release stacks, 14.58 % of window B's samples reach `cmp` with
`[unknown]` as the frame above it. Attribution of the 16-18 % **between**
`is_analyzed`, `resolve_module_files` and any other `binary_search` caller is
therefore made from source reading, not from the profile. m8 recorded the same
limitation (its limitation 4). A frame-pointer build would settle it in one
rebuild.

---

## (3) SYS-CPU attribution

### The profile cannot answer this, in either run

`perf_event_paranoid` is **2** and `kptr_restrict` is **1** on this host, and
there is no sudo. `perf evlist -v` shows `exclude_kernel: 1` on **both** m8's
`perf.data` and this run's: neither profile contains kernel samples (the 0.78 %
of `[k]` rows in window A are unresolved callchain boundary addresses, not
kernel-mode samples). The question is settled below by the A/B, a budget ladder
and a syscall census instead.

### A/B: restore `mmap_size`, gate cell (a), 3 reps a side, interleaved

`base` = HEAD (`mmap_size = 0`). `mmap` = HEAD + one line,
`mmap_size = 268435456` in `configure_readonly_page_cache`. Same tree, same
cell, own cache per binary, loadavg 5.0-5.9 throughout.

| | base (HEAD) | mmap restored | delta |
| --- | ---: | ---: | ---: |
| **sys CPU (s)**, median of 3 | **23.42** (23.42/24.35/22.88) | **20.30** (20.30/20.97/19.77) | **-3.12 s (-13.3 %)** |
| user CPU (s), median of 3 | 4.00 (4.07/3.84/4.00) | 4.44 (4.44/4.47/4.19) | +0.44 s |
| wall (s) | 4.50-4.60 | 4.60-4.70 | +0.1 |
| peak RSS (GB) | **0.44** | **1.48** | +1.04 GB |
| `pread64` calls (strace `-c -f`) | **432,165** | **285,811** | **-146,354 (-33.9 %)** |
| answer | 0 hits, `time_budget` | 0 hits, `time_budget` | identical |

**Mechanism 2 is real and it is small.** Restoring a 256 MiB mapping on an
845 MB database removes a third of the preads and **13.3 % of the sys**. It
cannot remove more: the mapping covers 256 of 845 MB, so the rest is `pread`
either way. It also costs 1.04 GB of RSS on a gate cell whose budget is 4 GB
and whose D4-2 win was just recorded on RSS.

### Budget ladder: does sys track the scan window?

Same binary (`base`), same cell, only `max_duration_secs` varies. Loadavg
1.5-2.7.

| cell | wall (s) | user (s) | **sys (s)** | peak RSS (GB) |
| --- | ---: | ---: | ---: | ---: |
| **construction only** (`get_symbol_sources`, warm miss) | 1.30 | 1.28 | **8.55** | 0.12 |
| **construction only**, rep 2 | 1.30 | 1.22 | **7.89** | 0.13 |
| `scan_usages`, budget 1 s | 3.20 | 2.58 | **7.36** | 0.22 |
| `scan_usages`, budget 3 s (product default = cell (a)) | 4.60 | 3.97 | **23.79** | 0.45 |
| `scan_usages`, budget 6 s | 7.41 | 36.49 | **65.61** | 0.86 |
| `scan_usages`, budget 12 s | 13.62 | 41.59 | **57.10** | 1.09 |

**Mechanism 1 dominates.** There is a **7.9-8.6 s construction floor** that a
scan does not add to (budget 1 s is *below* it), and above that sys is a
function of how long the scan spends at high fan-out: **+0 s at a 1 s budget,
+16 s at 3 s, +58 s at 6 s.** Run 10's cell (a) went 7.76 -> 22.28 s while its
discovery window went 1.62 -> 3.585 s; this ladder is that relationship measured
directly.

### Syscall census: where the syscalls actually come from

`strace -f -c`. Counts are exact; **strace's own time column is not used** -- it
is distorted by tracing overhead and by counting futex wait as time.

| syscall | construction only | full cell (a) | **scan-window delta** | origin |
| --- | ---: | ---: | ---: | --- |
| **`pread64`** | 3,908 | **432,165** | **+428,257** | **SQLite reads -- 99.1 % of the scan window's syscalls** |
| `readlink` | 352,706 (100 % `EINVAL`) | 352,701 | **~0** | construction |
| `newfstatat` | 123,795 | 123,782 | ~0 | construction |
| `statx` | 41,009 | 41,004 | ~0 | construction |
| `getdents64` | 17,266 | 17,266 | 0 | construction |
| `openat` | 11,514 | 11,514 | 0 | construction |
| `futex` | 19,027 | 17,497 | ~0 | both (wait, not CPU) |
| `sched_yield` | 25,714 | 22,380 | ~0 | construction |

**The split is clean: the scan window is `pread64` and nothing else; every path
syscall in the process belongs to construction.**

At ~15.5 s of scan-window sys over 428,257 preads that is **~36 us per pread**,
and the A/B's 3.12 s over 146,354 removed preads is **~21 us per pread** --
expensive for a page-cache read, and consistent with this being WSL2. The two
estimates agree to within a factor of 1.7, which is as much as this method
supports.

### Incidental: the construction-side `readlink` storm

Traced to its source with `strace -f -k` plus `addr2line`. **352,494 `readlink`
calls, 100 % of them `EINVAL`**, all from `__GI___realpath`, reached through
`WorkspaceAnalyzer::clone_with_project` -> `TreeSitterAnalyzer::new_internal` ->
`build_state` -> `reconcile_file_states` -> `LiveSnapshot::validate` ->
`Liveness::refresh_overlay`. The line is
`crates/bifrost-analysis/src/analyzer/store/liveness.rs:273`:

```rust
let canonical_abs = abs_path.canonicalize().unwrap_or_else(|_| abs_path.clone());
```

in `rel_path_from_workdir`, called once per workspace file from
`refresh_overlay`. `realpath` walks **every ancestor** of a ~10-component
absolute path, so ~35,746 canonicalize calls become ~352 k `readlink`s, of which
the first five per call are the identical, already-canonical workspace root
prefix (`/mnt`, `/mnt/containers`, ... `/rust-tree`). It is a **constant,
present in run 6 as well, and it is not the sys change run 10 flagged** -- it
sits in the 5.2-7.2 s construction term run 10 called unchanged. Recorded
because it is the largest single syscall count in the process and nobody has
named it before.

### Verdict on the sys question

Both mechanisms are real; they are not equal.

- **Mechanism 1 (discovery runs 2.2x longer) is the dominant one.** Sys above
  the construction floor is a function of the budget window: 0 s at 1 s, 16 s at
  3 s, 58 s at 6 s.
- **Mechanism 2 (`mmap_size = 0` turning page-cache hits into `pread`) accounts
  for 13.3 % of cell (a)'s total sys** and at most ~20 % of the part that is new
  since run 6. It is directionally exactly what `9df5558f`'s own comment
  predicted ("about 7 % CPU, all of it `sys`"), and at this cell it is larger
  than 7 % -- but it is not the tripling.
- **Run 10's sys column is not a regression in the sense of new waste**; it is
  the same per-unit read cost applied to a scan that now spends its whole budget
  reading. No verdict in this report rests on sys CPU, per the standing
  directive.

---

## (4) The 0.3 s overshoot residue

**It is not visible in the profile window as CPU, because it is not CPU.**
SQLite's whole self-time share is **1.40 %** of window A and **0.61 %** of the
graph-phase tail; `sqlite3VdbeExec` is 0.55 % and 0.00 %, and in the tail
neither it nor `sqlite3_step` clears the 0.4 % children limit. A statement that
ran long enough between 512-row polls to leak 0.3 s past a deadline would have
to be doing something, and in this profile it is not doing it on a CPU. What the
syscall census shows instead is that the scan window's syscall traffic is
**99.1 % `pread64`** at **~21-36 us apiece on this host** -- so the uninterruptible
step inside a statement is a run of page reads, not a run of returned rows. That
is consistent with, and independent evidence for, the mechanism `946710c4`'s
message named when it ruled out the 512-row interval ("26 ms at the measured
0.052 ms/row") and pointed at `sqlite3_progress_handler`, which counts VM steps:
an index scan that touches thousands of entries and returns few rows issues many
preads and few rows, so a row-counted poll never fires while a step-counted one
would. **Stated as an observation. This run did not measure a gate-cell
overshoot, did not instrument statement duration, and does not claim the residue
is fully explained.**

---

## Limitations

1. **One profiled answering cell.** Two windows on one process. It reproduces
   run 10's answering cell to within 2.3 % on user CPU and returns the same
   eleven hits, but it is n=1. m8 was also n=1 per configuration.
2. **`perf` stack unwinding is truncated** (release build, no frame pointers):
   25-33 % of stacks are depth 1. Every children percentage and every
   path-attribution figure is a **lower bound**, and the caller tree for
   `ProjectFile::cmp` is `[unknown]`. The self-time table does not depend on
   unwinding and is the load-bearing part. Same limitation as m8, which is why
   the two are comparable.
3. **No kernel-side samples exist in either profile** (`exclude_kernel: 1`,
   `perf_event_paranoid = 2`, no sudo). The sys attribution is by A/B, budget
   ladder and syscall census, not by profile.
4. **`strace -c` times are not used.** Only counts. The per-pread microsecond
   figures are derived from `/usr/bin/time -v` sys deltas divided by strace
   counts, from two different runs of the same cell, and agree only to a factor
   of 1.7.
5. **The bucket rollups are a different instrument from m8's prose rollups**
   (19.29 % vs "~28 %" allocator on the same data). Only same-script comparisons
   are used for the verdict.
6. **The `mmap` binary changes one line and nothing else** -- `cache_size` stays
   at `READER_PAGE_CACHE_KIB` (8 MiB) and the reader-pool idle cap stays at
   `min(parallelism, 16)`. It isolates the mapping, not all of `9df5558f`.
7. **A sibling `clippy-driver` (2 of 120 cores) ran during window B and the A/B
   cells.** It is present on both sides of every paired comparison and cannot
   flip a paired sign; it may inflate window B's absolute occupancy figure
   slightly.
8. **The subject predates the MultiAnalyzer prefetch fix, which landed during
   the run** (`c2592c3f`, 23:35:34, between the profiled cell and the A/B
   cells). Both binaries were pinned at `946710c4` before it existed, so the
   run is internally consistent, but section 3's sys figures are pre-fix and
   the fix removes reads from exactly the window sys is a function of. Sections
   1-2 are unaffected: the fix touches read batching, not path handling.
9. **Budget ladder cells are one rep each.** The 3 s point is corroborated by
   the three A/B base reps (22.88-24.35 s against the ladder's 23.79 s); the
   1 s, 6 s and 12 s points are not.

---

## Cleanup (confirmed)

| item | state |
| --- | --- |
| `/mnt/T9/repo-clones/.codescale-sources/rust--01f6ddf7` | **never written**; read only, by `cp -a` |
| `/mnt/containers/bifrost-latency-probe/m13` (tree copy, 2 caches, 2 worktrees) | **removed**, absence verified |
| git worktrees `wt-base`, `wt-mmap` | **removed** with `git worktree remove --force`, then `git worktree prune`; 0 worktrees match `latency-probe/m13` |
| isolated Cargo targets | **none remain**; both builds logged their removal |
| benchmark processes | **none remain**, verified by `pgrep` |
| scratch binaries | the two 113 MB binaries deleted after the last cell |
| main worktree `/mnt/optane/bifrost-nlp` | **no source file touched by this task**; `git status` clean on arrival and on exit. HEAD moved `946710c4` -> `c2592c3f` during the run, by the owner/another agent, not by this task. |
| `/mnt/containers/bifrost-latency-probe/m7`, `m10` | other agents' campaigns; **not touched** |
| artefacts kept | `scratchpad/m13/` (2 `perf.data`, folded stacks, delta tables, rollups, 4 strace censuses, `results.txt`, `.time` files) |
