# Design: ProjectFile identity cost - staged repair

Status: Stage 1 LANDED. **Stage 2 CLOSED as not warranted** by the re-profile gate; see "The
Stage-2 gate: VERDICT" below. Author: Fable, 2026-08-09.
Substrate: `.agents/docs/` companions (graph-read-cost-investigation, gate-cell-overhead) and the
fresh profile `graph-churn-profile-v1.md` (session scratchpad; check in with the first
implementation commit). Governing rule: AGENTS.md Implementation details - "Do not use reference
counting by default. In graph domains, prefer explicit IDs and arena allocation."

## The measured target

Post-volume-cuts profile of the rustc answering cell (two windows, m8-comparable method):
path handling holds 21.9-22.2% self, `ProjectFile::cmp` 16.3-18.0% with children,
`ProjectFile` own impls ~5%, allocator churn provably fed by path construction ~12.3%
(`PathBuf::_push`, `Path::_join`, `rust_crate_root_package`). Combined lower bound: **~38% of
graph-phase CPU**. Moka+crossbeam (~20%) are cache machinery serving the same lookups and are
expected to shrink with them. SQLite and tree-sitter are non-factors (<1.5%).

Call-site facts (read at HEAD `c2592c3f`):
1. `ProjectFile` is `Arc<{root: PathBuf, rel_path: PathBuf}>`; `cmp`/`eq`/`hash` walk both
   paths. Every file in a workspace shares the identical root, so half of every comparison is
   known-equal by construction, and `hash` re-walks the path on every moka/HashMap touch.
2. `RustPackageFileIndex::contains` = `files.binary_search` over 35,370 entries = ~15.1 `cmp`
   calls per membership test, called from `is_analyzed` and `resolve_module_files`.
3. `rust_module_files_from_segments` builds 4 `ProjectFile::new` + 4 `.exists()` per specifier;
   `rust_package_components`/`rust_crate_root_package`/`ModuleKey::new` allocate one heap
   `String` per path component per call.

Caveat carried from the profile: `cmp`'s caller split is source-reading (release stacks
truncate); the aggregate share is measured.

## Stage 1 - mechanical, semantics-identical (recommended to ship first)

No identity model change; every item preserves `Ord`/`Eq`/`Hash` semantics byte-for-byte.

1. **Precomputed hash.** `ProjectFileInner` gains a `path_hash: u64` computed once at
   construction (hash of root+rel_path exactly as `Hash` produces today); `Hash::hash` writes
   the cached value. Kills the per-touch path walk under moka/HashMap keys.
2. **Comparison fast paths.** `eq`: `Arc::ptr_eq` first (clones abound). `cmp`/`eq` slow path:
   compare `root` by `Arc`/pointer identity of the shared root where roots are shared (verify
   how roots are stored - if each inner holds its own `PathBuf` root, intern the root as
   `Arc<Path>` at listing construction so pointer equality applies), then compare `rel_path`
   only. Public ordering (path order) unchanged.
3. **Membership by hash.** `RustPackageFileIndex` gains a `HashSet<ProjectFile>` (cheap now
   that hash is precomputed) for `contains`; the sorted `Vec` stays for ordered iteration.
4. **Per-specifier construction memo.** `rust_module_files_from_segments`' 4-candidate probe
   memoized per (dir, name) in the existing walk-cache mechanism; component-String allocation
   in `rust_package_components`-family replaced with borrow/`SmallVec` iteration where the
   consumer does not retain.

Expected yield: the hash and comparison shares (~21-23%) compress toward the rel_path-only
cost; membership tests drop ~15x in comparisons. Gate: re-profile the same cell; Stage 2
proceeds only if path handling still holds a double-digit share.

### Stage 1 as landed (2026-08-09, branch `bifrost-nlp-ft`)

All four items plus the ride-along, one commit each; read the commit messages for the
reasoning and the fail-before evidence.

| Item | Commit | Notable deviation from the design above |
| --- | --- | --- |
| 1 precomputed hash | `30656b52` | none; hash VALUES change, which is safe because nothing persists or transmits them (checked) |
| 2 comparison fast paths | `30656b52`, `de38e6bc` | the design's open question resolved AGAINST shared roots: each inner owned its own `PathBuf`, so a process-global root interner had to be added first. `eq` also gained a `path_hash` inequality reject |
| 3 membership by hash | `0f746ce1` | no cheap seam exists for a comparison counter (it would need an atomic inside `Ord for ProjectFile`); the mechanism is pinned structurally instead |
| 4 probe memo + components | `65ae9be4` | `ModuleKey::new` skipped: its `components` and `crate_root` are the key's own retained storage and cannot borrow |
| ride-along canonicalize | `e7831df9` | root canonicalized once per root, per-file only on the below-root-symlink fallback; measured 24 files -> 1 canonicalization |

### The Stage-2 gate: VERDICT, Stage 2 is NOT warranted (2026-08-09, m14)

Re-profiled at `d91dbabd` on the same cell, same rustc tree, same perf event config
(`perf evlist -v` byte-identical), same bucket script; recorded in
`projectfile-stage1-reprofile-v1.md` (session scratchpad).

**The gate asked whether path handling still holds a double-digit share. It does not,
on any measure:**

| measure | m8 | m13 A / B | now A / B |
| --- | ---: | ---: | ---: |
| `path` bucket, self | 18.95 | 21.91 / 22.16 | **4.97 / 6.17** |
| `path` + `ProjectFile` own, self | 21.63 | 25.10 / 27.15 | **6.27 / 7.06** |
| stacks naming any path/`ProjectFile` frame | 28.49 | 30.71 / 38.19 | **6.94 / 8.04** |
| `ProjectFile::cmp`, with children | 14.90 | 18.04 / 16.29 | **4.25 / 4.41** |

The design's sized target of "~38% of graph-phase CPU" is now 8.04% by the identical
attribution script. `NormalizePath::normalize`, `rust_module_files_from_segments` and
`ProjectFile::exists` no longer clear the 0.4% children limit at all.

**The churn question is CLOSED. Stage 2 is not implemented, in either the fragile or
the boring-safe shape.** End to end the same cell went 1,076.6 s -> 585.6 s wall,
4,350.7 s -> 2,977.3 s user CPU, 1,411.2 s -> 918.9 s sys CPU, and returned 17 hits
instead of 11 - the same 11 plus 6 more, none lost, still `time_budget`. The
canonicalize ride-along removed the storm: `readlink` 352,706 -> 211 per process. Gate
cell (a) sys CPU fell 23.42 -> 6.90 s median.

**What leads now is `moka`, at 32.5% of window A and 20.5% of window B** (up from
11.30% / 9.67%), of which 20.1% is lookup and 11.9% is LRU/eviction bookkeeping. The
growth is absolute, not only a share: 1.39 -> 3.43 cores in window A. Two causes, one
by construction - item 4 routed the module probe through a new
`RustWalkCaches::module_probes` moka cache, trading four `ProjectFile::new` plus four
`exists()` syscalls per specifier for one lookup; and the surviving lookups got cheap
enough that the walk issues them faster. **This falsifies the design's prediction that
"Moka+crossbeam ... are expected to shrink with them"**: they went 19.94% -> 38.05%.
moka's caller tree is not attributable from this profile (30.1% of window A is an
all-moka truncated stack), so no intervention is sized here; a frame-pointer build
must come first.

## Stage 2 - interned file IDs (NOT WARRANTED; the gate above closed it)

Per-generation interner: the workspace listing is already a sorted `BTreeSet`; assign
sequential `u32` IDs in sorted order at listing construction, so **ID order equals path order
within a generation** and ordered containers can key by ID without changing iteration
semantics. `ProjectFile` carries `{id, generation, inner}`; `cmp`/`eq` fast-path on matching
generation via IDs, fall back to paths across generations (content-stability and
cross-snapshot comparisons keep today's meaning). Hot per-file maps in the walk layer migrate
to dense `Vec`-indexed-by-ID arenas per the AGENTS.md rule. This is the full IDs+arena answer;
it is deliberately gated because Stage 1 may capture most of the win at a tenth of the blast
radius - `ProjectFile` is the codebase's central identity type and Stage 2 touches its
representation.

OWNER REVIEW NOTE (2026-08-09): the sorted-order ID assignment ("ID order equals path order
within a generation") is judged FRAGILE by the owner and must not be implemented as written.
If Stage 2 is ever warranted by the re-profile, use the boring-safe shape instead: IDs are
opaque and serve equality/hash/membership/arena indexing ONLY; every structure whose iteration
or output order is contractual stays keyed/sorted by path. Ordering never rides on ID
assignment.

## Rides along with Stage 1 (measured, designless)

**The canonicalize storm**: `liveness.rs:273` `rel_path_from_workdir` calls
`abs_path.canonicalize()` once per workspace file - 352,494 always-`EINVAL` `readlink`
syscalls per process, the largest syscall count observed, inside the construction sys floor.
Canonicalize the workspace root once and join rel paths; per-file canonicalize only on the
rare non-prefix fallback path (read the function; preserve symlink correctness - the watcher
plan's probe repo and the #1793 symlink test are the guards). Pin with a syscall-count or
call-count counter.

## Pins and validation (both stages)

Equivalence: `Ord`/`Eq`/`Hash` property tests (same total order, same equality classes, same
hashes pre/post for Stage 1; cross-generation semantics tests for Stage 2). The usual ladder;
EQP untouched (no store changes). Measured acceptance: re-profile delta table against
`graph-churn-profile-v1.md`, same windows, same method. Fail-before via reverting the specific
mechanism per the house idiom.
