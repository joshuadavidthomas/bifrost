# Port the bifrost-nlp-ft optimization arc onto upstream's reorganized master

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `.agents/PLANS.md` in the repository root. Read that file before you revise this one.

## Purpose / Big Picture

Two lines of work grew apart for several weeks and must become one line again.

The branch `bifrost-nlp-ft` carries 128 commits of latency, memory and correctness work. A person who runs the Bifrost MCP server against a large repository sees the result directly: a code-intelligence call that used to take tens of seconds returns in under a second, a workspace that used to need tens of gigabytes of resident memory fits in a few, a `get_summaries` call with a `**/*` target answers with a clear "too broad" message instead of grinding, and the file watcher no longer wakes itself up in a loop every time Git writes inside `.git`.

The branch `origin/master` carries 668 commits over the same period. Its largest change is structural: the language implementations moved out of one very large crate (`crates/bifrost-analysis`) into nine per-language crates (`crates/bifrost-cpp`, `crates/bifrost-csharp`, `crates/bifrost-go`, `crates/bifrost-js-ts`, `crates/bifrost-jvm`, `crates/bifrost-php`, `crates/bifrost-python`, `crates/bifrost-ruby`, `crates/bifrost-rust`). Upstream also improved Rust import and include resolution, and it did so by building new work on the very data structure that our branch deleted.

After this work, both sets of gains exist in one tree, on top of upstream's crate layout, and the whole workspace test suite passes. Someone who checks out `bifrost-nlp-ft` after Phase 2 gets upstream's nine-crate layout, upstream's new Rust include-expansion resolution, and every latency and memory fix from the optimization arc, with the tests that pin each one still passing.

The work is split into two phases because one of the two sides cannot be ported mechanically. Phase 1, described here and executed first, integrates everything except the Rust usage subsystem. Phase 2, also described here but explicitly gated on owner review of Phase 1, re-lands the Rust usage rewrite on the new layout. Splitting this way is the only route to a tree that compiles and can be reviewed; the alternative is to invent a hybrid design in the middle of a merge, with 53 conflicted files open and no way to run a test.

## Definitions

Read these before the rest of the plan. Every one of them appears repeatedly.

An **ExecPlan** is this kind of document: a self-contained specification a newcomer can follow end to end. The rules are in `.agents/PLANS.md`.

The **arc** means the 128 commits on branch `bifrost-nlp-ft` that are not on `origin/master`. They run from `0c45c25d` to `0a53a550`. Their design records are the five ExecPlans `.agents/plans/rust-usage-index-v2.md`, `.agents/plans/usage-graph-streaming.md`, `.agents/plans/watcher-git-event-exemption.md`, `.agents/plans/store-schema-cleanup.md` and `.agents/plans/searchtools-too-broad-scope-guards.md`.

**Upstream** means the branch `origin/master`. At the time this plan was written its tip was `b48412bf`.

The **merge base** is commit `db9f60c3`. It is the last commit both branches share. Every three-way comparison in this plan uses it as the base.

A **usage index** in this repository is a data structure that answers "who refers to this symbol". The Rust one, `RustUsageIndex`, was a whole-workspace in-memory index: it parsed every Rust file in the workspace and built thirteen maps from the result, then answered every question from those maps. Building it for a large workspace took 88 seconds and 30 gigabytes.

**Usage v2** is what the arc replaced it with: the same facts, extracted once per file during the parse that already happens, written to per-file rows in the SQLite analyzer cache, and read back through lazy walks that touch only the files a question actually needs. The measured effect is that a workspace which has already been analyzed answers the same questions after a 0.07 to 0.18 second catch-up instead of an 88 second build.

An **include-expansion route** is upstream's newer concept. Rust code can pull one file's contents into another with the `include!` macro. Upstream's commit `649bebcb` added 731 lines that track where such an inclusion came from (a Cargo manifest, a module declaration, a host import, or a nested include) and carry that provenance into inverse usage scans, so that a usage found in an included file is attributed to the right owner. Upstream stored those routes as a field on `RustUsageIndex`, next to the twelve other maps.

The **A1 conflict cluster** is the thirteen files where the arc's deletion of `RustUsageIndex` collides with upstream's new work built on it. They are listed in full in "Conflict inventory" below.

A **pin** is a test that fails before a fix and passes after it. When this plan says "the pin for fix X", it means the specific test named there, and the way to check the port is to run that test.

**Featureless** means built with no Cargo features enabled. The root manifest sets `default = []`, so a plain `cargo test` is featureless and skips every integration suite gated behind `#![cfg(feature = "nlp")]`.

## Progress

- [x] (2026-08-09 10:05Z) Read the preserved merge-state inventory, `.agents/PLANS.md`, the arc's commit list, and the upstream crate topology.
- [x] (2026-08-09 10:20Z) Established the conflict set by re-running `git merge origin/master`: 53 files, matching the preserved inventory exactly.
- [x] (2026-08-09 10:30Z) Authored this ExecPlan.
- [x] (2026-08-09 11:10Z) Phase 1 Step 1: A1 cluster resolved to upstream; five usage-v2 sources parked under `.agents/phase2/rust-usage-v2/`.
- [x] (2026-08-09 12:05Z) Phase 1 Step 2: C cluster resolved, 31 files.
- [x] (2026-08-09 12:30Z) Phase 1 Step 3: A2 cluster resolved. Migrations renumbered to 0017/0018/0019, `CURRENT_MIGRATION_VERSION = 19`, blob-store salt at v9.
- [x] (2026-08-09 14:20Z) Phase 1 Step 4: arc fixes ported into the language crates.
- [x] (2026-08-09 14:40Z) Phase 1 Step 5: workspace compiles featureless and with all features, no warnings.
- [x] (2026-08-09 15:30Z) Phase 1 Step 6: featureless nextest matches the upstream baseline exactly; doctests pass; all-features clippy clean.
- [x] (2026-08-09 16:20Z) Phase 1 Step 7: committed on `bifrost-nlp-ft` as a merge commit. Not pushed. The comprehensive `nlp,python` gate also ran and matches the baseline.
- [x] (2026-08-09) Phase 2 authorized by the owner. Started.
- [x] (2026-08-09) Phase 2 Step 1 -- restore the write path. Fact value types in `crates/bifrost-core/src/analyzer/rust_facts.rs`; extraction in `crates/bifrost-rust/src/facts.rs`; `extract_rust_module_route_facts` back in `crates/bifrost-rust/src/cargo_routes.rs`; `ParsedFile`/`FileState` carry `rust_usage_facts`; the store writes and reads the eight `rust_*` tables; the two detector salt tokens added. Eight parked store tests restored and green.
- [ ] Phase 2 Step 2: restore the read path (`usage_queries`, `usage_walks`, `usage`, `fact_catch_up`) on the nine-crate topology.
- [ ] Phase 2 Step 3: rebuild upstream's include-expansion routes on the v2 substrate.
- [ ] Phase 2 Step 4: delete `RustUsageIndex`.

## Surprises & Discoveries

- Observation: `crates/bifrost-core/src/analyzer/model.rs` auto-merged cleanly and the merged `ImportInfo` already carries both sides' new fields.
  Evidence: the merged struct has our `pub is_global: bool` and upstream's `pub binder_span: Option<Span>`, each with `#[serde(default)]`. The semantic collision described in the preserved inventory is therefore only in the *persistence* layer, not in the parse product.

- Observation: `ParsedFile` moved to a new file upstream and kept a field the arc deleted.
  Evidence: upstream created `crates/bifrost-core/src/analyzer/parsed_file.rs`, whose `ParsedFile` still declares `pub import_statements: Vec<String>`. The arc removed that field from the old `crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs` copy, because the same information is derivable from `imports`. Nine writers of the field now live in the language crates.

- Observation: the epoch-salt "semantic collision" the preserved inventory predicted does not exist. The arc had already solved it.
  Evidence: `crates/bifrost-analysis/src/analyzer/store/mod.rs` on the arc side writes `binder_start` / `binder_end` as columns of `import_statements`, and `0019-import-bindings.sql` declares them with their `CHECK` constraints. Upstream's bincode `ImportInfo.binder_span` and the arc's relational form describe the same datum; only the salt needed a version above both.

- Observation: the two migrations compose by ordering alone; neither needed editing.
  Evidence: upstream's `0016-optional-fact-manifest.sql` rebuilds `blob_meta` without the per-analyzer count columns but keeps `import_count`; the arc's `0019-import-bindings.sql` then runs `ALTER TABLE blob_meta DROP COLUMN import_count` against the rebuilt table. Applying them in numeric order gives the intended merged schema with no hand-written combination step.

- Observation: taking a whole file with `git checkout --theirs` during a merge silently discards the auto-merged content of the *unconflicted* hunks. It is not "resolve the conflicts to theirs".
  Evidence: doing that to `crates/bifrost-analysis/src/analyzer/symbol_lookup.rs` removed `FuzzyResolveBudget`, `FuzzyResolveStop` and `resolve_codeunit_fuzzy_bounded`, which had merged cleanly and which `crates/bifrost-analysis/src/searchtools/mod.rs` imports; the loss surfaced only as an unresolved-import error. Eighteen files were re-merged with `git checkout -m --` and then resolved hunk by hunk. Any future merge of this shape must do the same.

- Observation: upstream independently adopted one of the arc's fixes verbatim, and independently fixed one of the same defects differently.
  Evidence: `crates/bifrost-core/src/analyzer/tree_walk.rs::expanded_comment_start` already carries the arc's backward-walk rewrite (arc commit `bc45f901`), differing only in that it asserts on a non-boundary offset where the arc returns; and `store/liveness.rs` reaches the same conclusion as the arc's stat-revalidation through `gitblob::WorkingTreeIdentity::clean_index_oid`, which also handles EOL and filter attributes. The arc's two pins for the second, `editing_file_changes_bulk_oid_after_the_first_projection` and `bulk_projection_resolves_a_file_created_after_the_first_projection`, pass unchanged against upstream's mechanism.

- Observation: the arc's #1758 include-target change and upstream's #1837 include-driven inference each break the other's test if adopted alone.
  Evidence: sourcing the C++ include-target index from the workspace listing alone loses adopted unclaimed-extension files, and `included_inc_fragment_is_indexed_and_visible_from_its_includer` fails with an empty visible set. Widening the listing filter to every unclaimed extension instead lets an unadopted extensionless `vendor/vector` answer `#include <vector>`, and `cpp_extensionless_angle_include_with_unrelated_basename_reports_boundary` fails. The union of the language-filtered listing and the analyzed set satisfies both, and all five C++ include pins pass.

- Observation: upstream's tip is redder than the recorded baseline, by upstream's own hand.
  Evidence: the baseline was measured at `ce8857c8` with four failures. At `b48412bf` a fifth is red, `diff_analysis_test::analyze_diff_gives_introduced_and_deleted_symbols_their_whole_callee_list`, broken by upstream's own `71f03d40` ("analyze_diff: detect moved/renamed symbols by body similarity"): the fixture renames `Retired` to `Fresh` with an identical body, which the new similarity detector classifies as a rename rather than as an introduction plus a deletion. Reproduced directly on `origin/master` in the scratch worktree.

- Observation: upstream's crate-aware Rust package naming changed a fully-qualified name an arc test spelled literally.
  Evidence: the fixture in `issue_1748_dotted_rust_spellings_are_all_still_sought` has a `Cargo.toml` naming the crate `probe`, and the stored declarations are now `probe.inner.Widget` rather than `inner.Widget`. The pin's claim is unchanged -- a dotted lookup drops no spelling and every spelling still seeks -- so the test was re-pointed at the name the store actually holds, and its seek count went from two spellings to three.

- Observation: the majority of arc changes inside A1 files are genuinely coupled to usage v2, so taking upstream's side there loses little that is independent.
  Evidence: the diffs are dominated by `RustReferenceContext<'r>` becoming a borrowed view, `resolve_bare` returning an owned `String`, and `parsed.rust_usage_facts`. Three independent items are the exception and are listed in the port table: Rust's `prefetch_import_targets`, the `SmallVec`/`Cow` allocation cuts in `rust_crate_root_package` and `rust_package_components`, and the `import_statements` removal.

## Decision Log

- Decision: upstream's nine-crate topology is the base; the arc ports into it. The merge is a real `git merge`, never a rebase, and the working tree is never stashed.
  Rationale: owner instruction. Upstream is 668 commits and its reorganization touches nearly every file; re-applying it onto our 128 would be the larger and riskier operation, and rebasing would rewrite published arc history.
  Date/Author: 2026-08-09, integration agent, on owner direction.

- Decision: in this merge only, every file in the A1 cluster takes upstream's side verbatim. `RustUsageIndex` therefore lives again in the Phase 1 tree.
  Rationale: it is the only resolution that yields a compiling, reviewable tree without inventing a hybrid design mid-merge. Upstream's include-expansion routes stay intact and their regression suites stay green, which is exactly what Phase 2 needs as its acceptance tests.
  Date/Author: 2026-08-09, integration agent, on owner direction.

- Decision: the arc's usage-v2 source files are kept in the tree but not declared as modules during Phase 1.
  Rationale: deleting them would force Phase 2 to recover them from history; declaring them would not compile against upstream's Rust analyzer. Keeping them undeclared preserves the work verbatim at a known path and costs nothing at build time. Every such file is listed in the "Dormant v2 sources" table.
  Date/Author: 2026-08-09, integration agent.

- Decision: all three arc migrations are renumbered above upstream's highest and retained, including the two that only usage v2 reads. `CURRENT_MIGRATION_VERSION` becomes 19.
  Rationale: owner instruction, and it is also the stable choice. A store that has already reached upstream's v16 must migrate forward, never sideways. Retaining `rust-usage-facts` and `rust-module-routes` as empty tables in Phase 1 means Phase 2 adds code, not schema, so no user's cache is rebuilt twice.
  Date/Author: 2026-08-09, integration agent, on owner direction.

- Decision: `binder_span` is persisted as a column on `import_statements`, not as a field inside a serialized blob, and the blob-store epoch salt becomes `analyzer-blob-store-v9-import-bindings-with-binder-span`.
  Rationale: the two sides changed the same thing in incompatible ways. Upstream added `binder_span` to a bincode-serialized `ImportInfo`; the arc deleted that serialized form and replaced it with one relational row per binding. The relational form is the one that survives, so upstream's new datum has to become a column. A fresh salt above both v7 and v8 is required because neither side's salt describes the merged layout.
  Date/Author: 2026-08-09, integration agent.

- Decision: the arc's Rust detector salt tokens (`per-file-usage-facts-2026-08`, `cargo-route-facts-2026-08`) are **not** added in Phase 1.
  Rationale: a detector salt token exists to invalidate caches when detector semantics change. In Phase 1 the Rust usage detector semantics are upstream's, unchanged, so adding the tokens would invalidate caches for no behavioural difference. Phase 2 changes those semantics and adds the tokens then.
  Date/Author: 2026-08-09, integration agent.

- Decision: arc tests that can only pass against usage v2 are marked `#[ignore]` with a reason string that names this plan, never deleted, and every one is listed in the "Cfg-ignored v2 tests" table with the Phase 2 step that re-enables it.
  Rationale: deletion loses the specification. `#[ignore]` keeps the test compiling and visible in `cargo nextest list`, so the re-enable step is a one-line change that cannot be forgotten.
  Date/Author: 2026-08-09, integration agent.

- Decision: `CLAUDE.md`'s rule about `analyzer/capabilities.rs` staying in `bifrost-analysis` is corrected rather than enforced.
  Rationale: upstream moved the file to `crates/bifrost-core/src/analyzer/capabilities.rs`, which contradicts the documented rule. Upstream's move is the base and reverting it is out of scope, so the documentation is what is wrong. The correction records why the exception holds.
  Date/Author: 2026-08-09, integration agent.

- Decision: the Rust usage fact VALUE types live in `crates/bifrost-core/src/analyzer/rust_facts.rs`, not in `brokk-bifrost-rust`, and `RustVisibility` and `RustRulesItemMacroDefinition` move there with them (re-exported from their old Rust-crate homes so no call site changes).
  Rationale: the plan requires `ParsedFile` to regain a `rust_usage_facts` field, and `ParsedFile` is in core, which may not depend on `brokk-bifrost-rust`. The precedent is exact: `ScalaExportInfo` and `CppTemplateMetadata` are language-specific plain data on `ParsedFile` and already live in core's model. The types name no `IAnalyzer`, store, grammar, or language module, so `CLAUDE.md`'s rule puts them in core. The tree-sitter extraction that fills them stays in `brokk-bifrost-rust`; the SQL that persists them stays in `brokk-bifrost-analysis`.
  Date/Author: 2026-08-09, integration agent.

- Decision: Phase 2 lands in four steps, each of which leaves the workspace compiling and green, rather than as one change. Step 1 is the write path with `RustUsageIndex` still in place and still the only reader.
  Rationale: the two sides cannot be swapped atomically without a tree that neither compiles nor runs a test for the length of the work. With the fact rows written but unread, the eight parked store tests become the first executable evidence that the v2 substrate is correct on upstream's topology, and every later step has a green base to bisect against.
  Date/Author: 2026-08-09, integration agent.

## Outcomes & Retrospective

### Phase 1, 2026-08-09

Phase 1 is complete and validated. The merge of `origin/master` (`b48412bf`, 668 commits) into `bifrost-nlp-ft` (128 commits) resolved 53 conflicts and produced a tree that compiles featureless and with all features, with no warnings, and whose featureless test suite fails exactly the five tests `origin/master` fails at its own tip and nothing else.

Measured, in the working tree:

    $ cargo nextest run --workspace --all-targets --no-fail-fast
    Summary [157.047s] 9849 tests run: 9844 passed (11 slow), 5 failed, 42 skipped

    upstream b48412bf, same command, scratch worktree:
    Summary [216.913s] 9744 tests run: 9739 passed (10 slow), 5 failed, 42 skipped

The five are the same five, by name: the cross-language conformance boundary row, the two Rust resolution tests in upstream's own subsystem, the JVM artifact test, and `analyze_diff_gives_introduced_and_deleted_symbols_their_whole_callee_list`, which upstream broke itself in `71f03d40`. The merged tree runs 105 more tests than upstream and passes all of them.

The comprehensive gate ran as well; disk permitted it (154 GB free after the build):

    $ BIFROST_SEMANTIC_INDEX=off uv run --python 3.12 -- \
        cargo nextest run --workspace --all-targets --features nlp,python --no-fail-fast
    Summary [161.086s] 9867 tests run: 9862 passed (2 slow), 5 failed, 44 skipped

Same five failures, no others. `BIFROST_SEMANTIC_INDEX=off` because tests must not download models or start indexer threads.

    $ cargo test --workspace --doc                                  -> ok
    $ cargo fmt --check                                             -> clean
    $ scripts/with-isolated-cargo-target.sh cargo clippy \
        --workspace --all-targets --all-features -- -D warnings     -> clean

What went right. The three-cluster classification in the preserved inventory held exactly: 53 conflicts, in the predicted files. Taking upstream verbatim for A1 cost less arc work than feared, because most of the arc's edits in those files were coupled to the deletion anyway; only three independent items had to be hand-carried out (Rust's `prefetch_import_targets`, the C# structured-import port, and Go's `go_import_path`). The compiler found every `ImportInfo` literal that needed `is_global`, because the struct has no `Default`.

What went wrong, and the lesson. Resolving a conflicted file with `git checkout --theirs -- <path>` looks like "take their side of the conflicts" and is not: it replaces the whole file, discarding every hunk that auto-merged. Eighteen files were treated that way before the loss surfaced as an unresolved import in `searchtools/mod.rs`; all eighteen were re-merged with `git checkout -m --` and resolved hunk by hunk. In a merge where one side has moved most of the code, that mistake is close to invisible, because the file is *supposed* to look mostly like theirs. Use `git checkout -m --` and edit the markers.

What is unfinished. The Rust usage rewrite is not in this tree; `RustUsageIndex` lives again, and with it the 88 second, 30 gigabyte whole-workspace build. That is the deliberate shape of Phase 1, not a regression against the merge base, but it is a regression against `bifrost-nlp-ft` as it stood at `0a53a550`, and it stays one until Phase 2 lands. Nine tests and seven source files are parked, all listed above.

### Phase 2

Not started. Owner-gated.



## Context and Orientation

You have a Git repository at `/mnt/optane/bifrost-nlp`. It builds a code-intelligence server called Bifrost, written in Rust, exposed over the Model Context Protocol so that coding agents can ask it questions like "where is this symbol used" and "show me this file's summary".

Its Cargo workspace members are listed in the root `Cargo.toml`. The ones this plan touches are:

`crates/bifrost-core` holds types with no dependency on any other Bifrost crate: `ProjectFile` (a workspace-relative file identity), `CodeUnit` (a declaration identity), `ImportInfo` (one parsed import), `ParsedFile` (everything one parse produced), the SQLite cache in `cache_db.rs`, the profiling spans in `profiling.rs`, and the memoization helper `PoolSafeMemo` in `analyzer/pool_memo.rs`. The rule that keeps this crate at the bottom of the dependency graph is in `CLAUDE.md` and is enforced by `scripts/check-workspace-dependencies.mjs`.

`crates/bifrost-analysis` holds the analyzer framework: the `IAnalyzer` trait, the multi-language dispatcher, the SQLite-backed store, the workspace lifecycle, and the search tools. It is the largest compilation unit in the workspace.

`crates/bifrost-rust`, `crates/bifrost-go`, `crates/bifrost-jvm`, `crates/bifrost-js-ts`, `crates/bifrost-python`, `crates/bifrost-ruby`, `crates/bifrost-csharp`, `crates/bifrost-cpp` and `crates/bifrost-php` hold the per-language parsing and resolution code. These are new upstream. Before the split, all of this lived under `crates/bifrost-analysis/src/analyzer/<language>/`. What remains at those old paths is mostly test-only shims; for example `crates/bifrost-analysis/src/analyzer/rust/cargo_routes.rs` went from 3,583 lines to 194, and its remaining content is one `#[cfg(test)] mod tests`.

`crates/bifrost-mcp` holds the MCP server, the search-tools service, and the project file watcher.

Tests live in two places. Unit tests are `#[cfg(test)] mod tests` inside the crate sources. Integration tests are grouped into harness binaries under `tests/`, one directory per suite with a `main.rs` that lists its members with `mod` lines: `tests/suite_usages/`, `tests/suite_symbols/`, `tests/suite_issues/`, `tests/suite_analyzers/`, `tests/suite_cross_language/`. The inventory of suites is `.agents/docs/test-harness-consolidation-2026-07.md`.

The analyzer cache is a SQLite database. Its schema is applied by numbered migration files under `crates/bifrost-core/migrations/cache/`, each registered in the `CACHE_MIGRATION_SQL` array in `crates/bifrost-core/src/cache_db.rs`, with a compile-time assertion that the array length equals `CURRENT_MIGRATION_VERSION`. The database file name embeds that version, so a version bump means a fresh file rather than an in-place upgrade of an incompatible layout.

Separately from migrations, the cache carries an **epoch salt**: a string that is hashed into every persisted blob's key. When the *meaning* of a persisted value changes without the SQL schema changing, bumping the salt string invalidates the affected rows. There are two relevant salts in `crates/bifrost-analysis/src/analyzer/store/epoch.rs`: a blob-store salt for the serialized analysis payload, and a detector salt listing the detector-semantics changes.

### What each side did

The arc's five design records describe its work in full. In summary:

`.agents/plans/rust-usage-index-v2.md` replaced `RustUsageIndex` with per-file store rows plus lazy walks. Its milestones 1 to 5 added `crates/bifrost-analysis/src/analyzer/rust/facts.rs`, `usage_walks.rs`, `usage_queries.rs`, `fact_catch_up.rs` and `usage.rs`, added migrations `0016-rust-usage-facts.sql` and `0017-rust-module-routes.sql`, and finally deleted 2,054 lines of index in commit `259b7496`.

`.agents/plans/usage-graph-streaming.md` made Rust reference resolution per-site instead of per-file, let a cancelled scan stop inside a walk rather than after it, and stopped the scan once the callsite cap is proven.

`.agents/plans/watcher-git-event-exemption.md` stopped the project watcher from reacting to its own Git-directory writes.

`.agents/plans/store-schema-cleanup.md` merged the `import_details` table into `import_statements` as one row per binding, in migration `0018-import-bindings.sql`, and removed the now-redundant `ParsedFile::import_statements` string vector.

`.agents/plans/searchtools-too-broad-scope-guards.md` added "too broad" guards to `get_summaries`, `get_symbol_sources` and `search_symbols`.

Upstream, over the same period, split the nine language crates out, split the `IAnalyzer` trait into `CodeUnitIndex` plus `IAnalyzer`, moved `PoolSafeMemo` into `bifrost-core` and made it public, moved `capabilities.rs` into `bifrost-core`, added migration `0016-optional-fact-manifest.sql`, added `binder_span` to `ImportInfo`, rewrote Go's import handling as free functions over an `import_tables()` structure, and built inverse import shadow resolution (`46e7bf58`) and include-expansion routes (`649bebcb`) on top of `RustUsageIndex`.

### The collision, stated plainly

The arc deleted `RustUsageIndex`. Upstream moved it to `crates/bifrost-rust/src/usage_index.rs` and added 837 lines of new capability to it. Upstream's `include_routes` is a field on that struct, built by the same whole-workspace `RustUsageIndex::build(rust, parallel)` call. There is no narrow slice of upstream's new work that can be adopted without the index.

Keeping the arc's deletion breaks upstream's 837 new lines and loses their two new regression suites, `tests/suite_usages/rust_include_inverse_regression.rs` and `tests/suite_usages/rust_top30_inverse_regression.rs`, roughly 430 lines pinning SpacetimeDB, Candle, Tokenizers and cross-Cargo cases. Keeping upstream's index undoes usage v2 and restores the 88 second, 30 gigabyte build.

The owner's resolution is neither: rebuild upstream's include-expansion routes on the v2 substrate, using upstream's new suites as the acceptance tests. That is Phase 2. Phase 1 takes upstream's side so that the tree compiles and Phase 2 has a green starting point with those suites already passing.

## Conflict inventory

Re-running the merge produces exactly the 53 conflicts the preserved inventory recorded. They fall into three clusters.

### Cluster A1 - the Rust usage fork (13 files, resolve to upstream verbatim)

    crates/bifrost-analysis/src/analyzer/rust/usage_index.rs      (we deleted it; upstream modified it)
    crates/bifrost-analysis/src/analyzer/rust/mod.rs
    crates/bifrost-analysis/src/analyzer/rust/cache.rs
    crates/bifrost-analysis/src/analyzer/rust/cargo_routes.rs
    crates/bifrost-analysis/src/analyzer/rust/graph_support.rs
    crates/bifrost-analysis/src/analyzer/rust/hierarchy.rs
    crates/bifrost-analysis/src/analyzer/rust/imports.rs
    crates/bifrost-analysis/src/analyzer/rust/diagnostics.rs
    crates/bifrost-analysis/src/analyzer/usages/rust_graph.rs
    crates/bifrost-analysis/src/analyzer/usages/rust_graph/inverted.rs
    crates/bifrost-analysis/src/analyzer/usages/rust_graph/resolver.rs
    crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs
    crates/bifrost-rust/src/declarations.rs

Three arc changes inside these files are independent of usage v2 and must be ported by hand into the language crate rather than lost. They are rows in the port table below: Rust's `prefetch_import_targets`, the allocation cuts in `rust_crate_root_package` and `rust_package_components`, and the removal of the `parsed.import_statements` pushes.

### Cluster A2 - contingent mechanics (9 files)

    crates/bifrost-core/src/cache_db.rs                            migration numbering
    crates/bifrost-analysis/src/analyzer/store/epoch.rs            salt arithmetic
    crates/bifrost-analysis/src/analyzer/store/mod.rs              persistence of imports and facts
    crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs   write path
    crates/bifrost-analysis/src/analyzer/workspace.rs              warm and catch-up
    crates/bifrost-analysis/src/analyzer/mod.rs                    dedicated build pool
    crates/bifrost-core/src/analyzer/pool_memo.rs                  dedicated build pool, keyed memo
    crates/bifrost-mcp/src/searchtools_service.rs                  scope guards and index call
    crates/bifrost-analysis/Cargo.toml                             dependency union

### Cluster C - independent (31 files)

    Cargo.lock
    crates/bifrost-analysis/src/analyzer/csharp/imports.rs
    crates/bifrost-analysis/src/analyzer/csharp/mod.rs
    crates/bifrost-analysis/src/analyzer/global_usage_definition_index.rs
    crates/bifrost-analysis/src/analyzer/go/imports.rs
    crates/bifrost-analysis/src/analyzer/go/mod.rs
    crates/bifrost-analysis/src/analyzer/i_analyzer.rs
    crates/bifrost-analysis/src/analyzer/java/adapter.rs
    crates/bifrost-analysis/src/analyzer/java/imports.rs
    crates/bifrost-analysis/src/analyzer/java/mod.rs
    crates/bifrost-analysis/src/analyzer/javascript/mod.rs
    crates/bifrost-analysis/src/analyzer/kotlin/imports.rs
    crates/bifrost-analysis/src/analyzer/multi_analyzer.rs
    crates/bifrost-analysis/src/analyzer/python/imports.rs
    crates/bifrost-analysis/src/analyzer/ruby/imports.rs
    crates/bifrost-analysis/src/analyzer/scala/imports.rs
    crates/bifrost-analysis/src/analyzer/store/liveness.rs
    crates/bifrost-analysis/src/analyzer/symbol_lookup.rs
    crates/bifrost-analysis/src/analyzer/typescript/mod.rs
    crates/bifrost-analysis/src/searchtools/sources.rs
    crates/bifrost-analysis/src/searchtools/tests.rs
    crates/bifrost-core/src/analyzer/capabilities.rs
    crates/bifrost-core/src/gitblob.rs
    crates/bifrost-core/src/profiling.rs
    crates/bifrost-go/src/declarations.rs
    crates/bifrost-go/src/graph/resolver.rs
    crates/bifrost-go/src/hierarchy.rs
    crates/bifrost-jvm/src/kotlin/declarations.rs
    src/bin/most_relevant_files.rs
    tests/suite_issues/main.rs
    tests/suite_usages/main.rs

Four of these have a non-obvious resolution and are called out here so nobody has to rediscover them.

`crates/bifrost-core/src/profiling.rs`: both sides fixed the same hot path, differently, and both fixes are wanted. The arc reads the `BIFROST_TIMING` environment variable *by value*, so that `BIFROST_TIMING=0` means off; before that fix, merely having the variable set to anything cost the D4 harness 247,000 span events. Upstream caches the environment read in a `OnceLock` so `env::var_os` stays off the per-candidate path. The resolution is to cache the arc's value-based test inside upstream's `OnceLock`.

`crates/bifrost-core/src/analyzer/capabilities.rs`: the arc adds `prefetch_import_targets`; upstream adds the three-valued `import_reachability` and its `ImportReachability` type. Union them. Note that this file is at a path `CLAUDE.md` says it should not be at; see the decision log and the `CLAUDE.md` correction step.

`crates/bifrost-core/src/gitblob.rs`: the arc adds `all_working_tree_paths` (native Git status enumeration); upstream adds end-of-line and filter-attribute object-id correctness. Union.

`tests/suite_issues/main.rs` and `tests/suite_usages/main.rs`: union the `mod` lines and keep them alphabetical.

### The silent-loss shape

Upstream did not always *move* a file. For several files it copied the implementation into a language crate and shrank the original into a test shim. Git's rename detection then maps our change onto the shim, which still exists, so the merge reports no conflict at all while the real implementation quietly takes upstream's version.

The files where upstream shrank the analysis-side copy by more than half are:

    analyzer/common.rs                   561 -> 295
    analyzer/cpp/imports.rs              425 -> 161
    analyzer/csharp/adapter.rs           241 -> 116
    analyzer/csharp/imports.rs           342 -> 120
    analyzer/csharp/mod.rs             2,252 -> 1,419
    analyzer/go/imports.rs               364 -> 164
    analyzer/java/adapter.rs             209 -> 90
    analyzer/java/imports.rs             897 -> 388
    analyzer/javascript/mod.rs         2,805 -> 765
    analyzer/kotlin/imports.rs           523 -> 129
    analyzer/python/imports.rs         1,192 -> 193
    analyzer/ruby/imports.rs             594 -> 161
    analyzer/rust/cache.rs                69 -> 49
    analyzer/rust/cargo_routes.rs      3,583 -> 194
    analyzer/rust/diagnostics.rs       1,008 -> 369
    analyzer/rust/graph_support.rs     1,989 -> 279
    analyzer/rust/hierarchy.rs           574 -> 169
    analyzer/rust/imports.rs             780 -> 83
    analyzer/rust/usage_index.rs       3,463 -> 241
    analyzer/scala/imports.rs            763 -> 370
    analyzer/typescript/mod.rs         2,385 -> 829
    analyzer/usages/rust_graph/inverted.rs   1,262 -> 46
    analyzer/usages/rust_graph/resolver.rs   1,535 -> 35

Every arc change to one of these files must be checked against the language crate, not against the shim. For the Rust ones the A1 decision settles the question. For the rest, the technique is a three-way merge by hand:

    git show db9f60c3:<old analysis path>     > /tmp/base
    git show 0a53a550:<old analysis path>     > /tmp/ours
    git show origin/master:<new crate path>   > /tmp/theirs
    git merge-file -p /tmp/ours /tmp/base /tmp/theirs > /tmp/merged

Read the result before writing it anywhere. Upstream's drift beyond the mechanical move is small for most of these files, so the three-way port usually applies cleanly.

## Plan of Work: Phase 1

Phase 1 produces a tree that compiles, passes the featureless workspace suite modulo a known baseline, and contains every arc fix except the Rust usage rewrite. Do the steps in order; each leaves the tree in a state you can reason about, even though only the last one leaves it compiling.

### Step 1: resolve the A1 cluster to upstream

For the twelve content conflicts, take upstream's version of the file. For `crates/bifrost-analysis/src/analyzer/rust/usage_index.rs`, which the arc deleted and upstream modified, restore upstream's version.

    cd /mnt/optane/bifrost-nlp
    git checkout --theirs -- <path>            # for the twelve UU files
    git checkout origin/master -- crates/bifrost-analysis/src/analyzer/rust/usage_index.rs
    git add -- <those paths>

Then move the arc's dormant usage-v2 sources out of the Rust module directory so nothing tries to compile them and nothing pretends they are live. Keep them under a clearly-named directory that Cargo never builds, `.agents/phase2/rust-usage-v2/`, and record every one in the "Dormant v2 sources" table. Phase 2 restores them.

### Step 2: resolve the C cluster

Work through the 31 files. Most are a union of two unrelated changes. Take the four called out above with their stated resolutions. For each of the language `imports.rs` files, the arc's change is the single line `is_global: false` added to an `ImportInfo` literal, which must survive alongside upstream's new `binder_span` field on the same literal; where upstream moved the literal into a language crate, add the field there too, because `ImportInfo` has no `Default` and a missing field is a compile error that will find them all for you.

### Step 3: resolve the A2 cluster

Renumber the arc's migrations and settle the salts.

Rename `crates/bifrost-core/migrations/cache/0016-rust-usage-facts.sql` to `0017-rust-usage-facts.sql`, `0017-rust-module-routes.sql` to `0018-rust-module-routes.sql`, and `0018-import-bindings.sql` to `0019-import-bindings.sql`. In `crates/bifrost-core/src/cache_db.rs`, set `CURRENT_MIGRATION_VERSION = 19` and make `CACHE_MIGRATION_SQL` end with, in order, the materialization-records migration, upstream's `OPTIONAL_FACT_MANIFEST_SQL`, then `RUST_USAGE_FACTS_SQL`, `RUST_MODULE_ROUTES_SQL`, `IMPORT_BINDINGS_SQL`. The compile-time assertion `const _: () = assert!(CACHE_MIGRATION_SQL.len() == CURRENT_MIGRATION_VERSION)` must hold. The cache file name becomes `bifrost_cache.v19.db`.

The two Rust fact migrations create tables that nothing reads in Phase 1. That is deliberate; see the decision log.

In `crates/bifrost-analysis/src/analyzer/store/epoch.rs`, set the blob-store salt to `analyzer-blob-store-v9-import-bindings-with-binder-span` and make the detector salt the union of the two sides' lists **excluding** the arc's two Rust-usage tokens, which Phase 2 adds.

In `crates/bifrost-analysis/src/analyzer/store/mod.rs`, the import write path keeps the arc's one-row-per-binding shape from `0019-import-bindings.sql` and gains a `binder_span` column carrying upstream's new datum. Nothing writes the arc's Rust fact tables in Phase 1.

In `crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs` and `workspace.rs`, take upstream's Rust warm and catch-up paths (A1 consequence) and keep every arc change that is not about Rust usage facts.

In `crates/bifrost-core/src/analyzer/pool_memo.rs`, union upstream's move-and-publicize with the arc's `dedicated_build_pool` / `spawn_on_dedicated_build_pool` and the keyed single-flight memo.

### Step 4: port the arc's topology-independent fixes

This is the substance of Phase 1. Each row of the port table below names a fix, the file it lived in on the arc, the file it must live in now, and the test that proves it. Work the table top to bottom. When upstream has independently fixed the same defect, adopt upstream's fix and record that in the table rather than layering a second one. When upstream's reorganization removed the seam a pin was written against, move the pin to the new seam rather than deleting it.

### Step 5: compile

Iterate until these succeed:

    cargo fmt
    cargo check --workspace --all-targets
    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

`--workspace` is mandatory. The root manifest sets `default-members = ["."]`, so without it clippy lints only the facade package and a broken `#[cfg(test)]` module inside a member crate passes unnoticed.

### Step 6: validate

The comparison point is the upstream baseline, already measured in a scratch worktree at `/mnt/containers/bifrost-upstream-baseline`, checked out at `ce8857c8`.

    cargo nextest run --workspace --all-targets --no-fail-fast

gave 9,741 tests run, 9,737 passed, 4 failed at `ce8857c8`. That commit is two behind the tip actually merged, so the baseline was re-measured at `b48412bf` in the same worktree:

    Summary [216.913s] 9744 tests run: 9739 passed (10 slow), 5 failed, 42 skipped

Upstream's five failures at its own tip are:

    suite_cross_language   code_query_resolution_conformance::an_unindexed_declared_dependency_is_a_boundary_row_rather_than_an_empty_answer
    suite_symbols          diff_analysis_test::analyze_diff_gives_introduced_and_deleted_symbols_their_whole_callee_list
    suite_symbols          searchtools_service::scan_usages_by_reference_finds_exact_rust_scoped_members_inside_macros
    suite_symbols          get_definition_test::rust_scoped_owner_resolution_preserves_namespace_and_canonical_identity
    brokk-bifrost-analysis analyzer::jvm::java_artifact::tests::source_and_class_jars_share_declaration_ids_and_keep_distinct_origins

The two Rust ones are upstream's own, in the subsystem this merge takes from upstream; they are tolerated, not merge damage. Do not read `origin/master`'s Rust arc as green. The `diff_analysis` one is newer still: upstream's `71f03d40` added rename detection by body similarity, and the fixture renames `Retired` to `Fresh` with an identical body, so the new detector reports a rename where the test expects an introduction and a deletion.

Run the same featureless command on the merged tree. Acceptance is: green except for those four, plus whatever the cfg-ignored v2 table accounts for. Any other failure is merge damage and must be fixed before the commit.

Then run doctests, because nextest does not:

    cargo test --workspace --doc

Then the all-features clippy in an isolated target, as in Step 5. Then, if disk permits, the comprehensive gate:

    uv run --python 3.12 -- cargo test --features nlp,python

Record whether it ran and what it said either way. An NLP build can use tens of gigabytes per worktree; check free space first and do not run one concurrently with a sibling worktree's.

### Step 7: commit

Stage only the files this work changed. Do not run `git add -A`. Commit on `bifrost-nlp-ft`. Do not push. Do not start Phase 2.

## Port table

Each row: the fix, where it lived on the arc, where it lives after the merge, the pin that proves it, and the outcome. Every row is `landed` unless stated otherwise.

| Fix | Arc home | New home | Pin | Status |
| --- | --- | --- | --- | --- |
| Watcher `.git` event exemption | `bifrost-mcp/src/project_watcher.rs` | unchanged | `project_watcher` unit tests | landed (auto-merged) |
| No watcher for a one-shot invocation | `bifrost-mcp/src/project_watcher.rs`, `mcp_core.rs` | unchanged | `project_watcher` unit tests | landed (auto-merged) |
| `get_summaries` too-broad guard | `bifrost-analysis/src/searchtools/summaries.rs` | unchanged | `tests/suite_symbols/searchtools_too_broad_scope.rs` | landed |
| `get_symbol_sources` too-broad guard | `bifrost-analysis/src/searchtools/sources.rs` | unchanged, interleaved with upstream's source-byte budget | same | landed |
| `search_symbols` candidate cap | `bifrost-analysis/src/searchtools/mod.rs` | unchanged | same | landed |
| Directory-listing instrumentation, no per-call clone | `bifrost-analysis/src/searchtools/sources.rs` | unchanged | same | landed |
| Guards advertised in tool descriptions | `bifrost-mcp/src/searchtools_service.rs` | unchanged | same | landed |
| Liveness stat revalidation of the startup scan | `bifrost-analysis/src/analyzer/store/liveness.rs` | unchanged | `editing_file_changes_bulk_oid_after_the_first_projection`, `bulk_projection_resolves_a_file_created_after_the_first_projection` | adopted upstream's `WorkingTreeIdentity::clean_index_oid`; both arc pins pass against it |
| Single-flight definition-candidate row read | `bifrost-analysis/src/analyzer/pool_memo.rs` | `bifrost-core/src/analyzer/pool_memo.rs`, made `pub` | `pool_memo` unit tests | landed |
| Single-flight per-file export index | same | same | same | landed |
| `KeyedPoolSafeMemo` | same | same | `pool_memo` unit tests | landed |
| Dedicated build pool spawn helper | same | same | `pool_memo` unit tests | landed as `pub` core API; sole caller was the v2 catch-up, so Phase 2 restores the call site |
| SQLite reader state not sized by core count | `bifrost-core/src/cache_db.rs` | unchanged | `cache_db` unit tests | landed (auto-merged) |
| `BIFROST_TIMING` read by value | `bifrost-core/src/profiling.rs` | unchanged | `profiling` unit tests | landed, cached inside upstream's `OnceLock` |
| scan_usages resolution budget and fan-out gate (#1839) | `bifrost-analysis/src/searchtools/scan_usages.rs` | unchanged | `tests/suite_issues/issue_1839_scan_usages_resolution_budget.rs` | landed |
| Deadline reaches the reads it spends its budget in | `analyzer/i_analyzer.rs` (`AnalyzerQueryContext`) | unchanged | same | landed (auto-merged) |
| Bare-name fan-out verdict outranks a spent budget | `analyzer/symbol_lookup.rs` | unchanged | same | landed |
| Fuzzy resolve budget and stop reasons | `analyzer/symbol_lookup.rs` | unchanged | `searchtools_fuzzy_symbol_lookup.rs` | landed; nearly lost to a whole-file `--theirs`, see Surprises |
| ProjectFile precomputed hash and interned root | `bifrost-core/src/analyzer/model.rs` | unchanged | `model` unit tests | landed (auto-merged) |
| Canonicalize the workspace root once | `bifrost-core/src/analyzer/project.rs`, `store/liveness.rs` | unchanged | `the_workspace_root_is_canonicalized_once_not_once_per_file` | landed |
| Batched import-target prefetch, trait side (#1748) | `analyzer/capabilities.rs` | `bifrost-core/src/analyzer/capabilities.rs`, unioned with upstream's `import_reachability` | `tests/suite_usages/issue_1748_candidate_discovery_batching.rs` | landed |
| Batched import-target prefetch, Rust override | `analyzer/rust/imports.rs` | `bifrost-analysis/src/analyzer/rust/imports.rs`, over `brokk_bifrost_rust::imports` | same | landed (hand-ported; A1 reverted the shim) |
| Batched import-target prefetch, multi-language reach | `analyzer/multi_analyzer.rs`, `usages/candidates.rs` | unchanged | same | landed |
| `has_complete_symbol_lookup_index` | `analyzer/i_analyzer.rs` + twelve language mods | `bifrost-core/src/analyzer/code_unit_index.rs` + twelve `CodeUnitIndex` impls + `MultiAnalyzer` | `issue_1063_...`, `issue_1758_...`, `php_in_a_mixed_workspace_keeps_the_conclusive_miss_gate` | landed; the `MultiAnalyzer` delegation is load-bearing, without it all three fail |
| Identifier-index seek by suffix pattern (#1688) | `analyzer/i_analyzer.rs`, `symbol_lookup.rs`, store | `bifrost-core/src/analyzer/code_unit_index.rs`; `search_definitions_with_literal` renamed to `search_definitions_by_suffix_pattern` across every implementor | `searchtools_fuzzy_symbol_lookup.rs` | landed |
| Decorated identifier spellings (#1063) | `analyzer/common.rs`, store, `symbol_lookup.rs` | unchanged | `issue_1063_decorated_identifier_spellings_resolve_without_a_full_declaration_scan` | landed |
| Drop lookup spellings the storage contract cannot hold | `store/mod.rs`, `tree_sitter_analyzer.rs`, `core/fq_name.rs` | unchanged | `issue_1748_double_colon_spellings_do_not_seek_for_rust`, `issue_1748_dotted_rust_spellings_are_all_still_sought` | landed; the dotted pin re-pointed at upstream's crate-aware fq name |
| `::` declarable as name text, not a join | `*/adapter.rs`, `symbol_lookup.rs` | language crates | `issue_1748_scala_colon_named_declarations_are_never_dropped` | landed |
| Store schema step 1: one import row per binding | `store/mod.rs`, `0018-import-bindings.sql` | `store/mod.rs`, `0019-import-bindings.sql` | frozen-equivalence, schema and cost tests | landed; `import_count` dropped from `blob_meta`, upstream's optional-fact manifest kept |
| `binder_span` as a column | `store/mod.rs`, migration | unchanged | store import round-trip tests | landed; the arc already did this, see Surprises |
| `ParsedFile::import_statements` removal | `tree_sitter_analyzer.rs` and every adapter | `bifrost-core/src/analyzer/parsed_file.rs` and five language crates | `import_statements` derivation tests | landed; `IAnalyzer::import_statements` now collapses equal adjacent binding snippets |
| C++ include-target index from the workspace listing (#1758) | `analyzer/cpp/imports.rs`, `tree_sitter_analyzer.rs` | unchanged, unioned with the analyzed set | `cpp_include_target_resolves_a_present_but_unanalyzed_header`, `cpp_include_targets_resolve_hit_miss_and_duplicate_basenames` | landed; reconciled with upstream's #1837, see Surprises |
| Fuzzy ambiguity from indexed candidates (#1758) | `analyzer/symbol_lookup.rs` | unchanged | `issue_1758_fuzzy_resolution_decides_ambiguity_without_a_full_declaration_scan` | landed |
| Test files classified where the scan reads them | `searchtools/`, analyzer | unchanged | test-file classification tests | landed (auto-merged) |
| Definition index not copied into a normalized twin | `analyzer/global_usage_definition_index.rs` | unchanged | its unit tests | landed; upstream's move of `BoundedDefinitionLookup` to core kept |
| Native Git status for working-tree paths | `bifrost-core/src/gitblob.rs` | unchanged | `all_working_tree_paths_use_index_and_dirty_overlay` | landed; upstream's EOL and filter-attribute correctness kept alongside |
| Updated path resolved against the working tree | `bifrost-mcp/`, `store/liveness.rs` | unchanged | its unit tests | landed |
| Go import path from the structured path, not the raw snippet | `analyzer/go/imports.rs` | `bifrost-go/src/imports.rs` as `go_import_path`; `extract_go_import_path` removed | `bifrost-go` and Go analyzer tests | landed (hand-ported) |
| Go receiver-selector indexed lookup | `usages/go_graph/resolver.rs` | `bifrost-go/src/graph/resolver.rs` | `bifrost-go` unit tests | landed (auto-merged) |
| C# `global using` recorded by the parser | `analyzer/csharp/{imports,mod}.rs` | `bifrost-csharp/src/{imports,syntax}.rs` | C# import and diagnostics tests | landed (hand-ported); `csharp_using_namespace` now reads a structured path, and the `strip_prefix("global ")` / `contains('=')` text tests are gone |
| `ImportInfo::is_global` | `bifrost-core/src/analyzer/model.rs` | unchanged | as above | landed (auto-merged); the field was propagated to every `ImportInfo` literal in the nine language crates |
| Batched active symbol scans across languages | `store/mod.rs`, `bifrost-nlp/src/active_index.rs` | unchanged | `active_symbol_candidate_scan_batches_languages` | landed; upstream wrote the same test independently, the two copies were deduplicated in favour of the stronger one |
| Bounded pathological complete-file parsing | `tree_sitter_analyzer.rs` | unchanged | its unit tests | landed; `file_state_from_parsed` restored as the shared assembly point |
| No panic on invalid tree-sitter byte offsets | `tree_sitter_analyzer.rs` | `bifrost-core/src/analyzer/tree_walk.rs` | `expanded_comment_start_ignores_non_boundary_offsets` | landed; upstream had already taken the arc's backward walk but kept an assert |
| Avoid the eager Rust usage warm in a large mixed session | `bifrost-mcp/src/searchtools_service.rs` | unchanged | `a_one_shot_service_does_not_start_the_usage_index_warm_at_startup` | landed; `StartupIndexWarm::OnDemand` now gates upstream's index warm, and `usage_index_ready` answers from the warm thread's own state |
| Rust usage v2 (milestones 1 to 5) | `analyzer/rust/{facts,usage,usage_walks,usage_queries,fact_catch_up}.rs`, store, workspace | parked, see "Dormant v2 sources" | parked, see "Parked v2 tests" | **deferred to Phase 2 by owner decision** |
| Per-site Rust reference resolution and cancellable walks | `usages/rust_graph/*`, `get_definition/rust.rs` | reverted with A1 | `issue_1230_rust_scan_complexity` (reverted to upstream's form) | **deferred to Phase 2** |
| Cargo routes composed from per-blob rows (#1793) | `analyzer/rust/cargo_routes.rs`, store | reverted with A1 | parked store tests | **deferred to Phase 2** |

## Parked v2 sources and tests

The owner asked for arc tests that cannot pass under Phase 1 to be `#[ignore]`d rather than deleted, with a tracking table. `#[ignore]` was not available for these: an ignored test still has to *compile*, and every one of them names a type or method (`RustUsageFacts`, `AnalyzerStore::rust_usage_facts`, `WorkspaceAnalyzer::rust_usage_facts_warm`) that Phase 1 does not define. They are therefore preserved verbatim, outside the Cargo build, under `.agents/phase2/rust-usage-v2/`. Nothing is deleted, every file names the plan that parked it, and each row below names the Phase 2 step that restores it.

Three MCP tests were an exception: their claims survive the change of mechanism, so they were rewritten in place rather than parked, and they run today.

### Dormant v2 sources

| File | Arc path | Phase 1 location | Restored by |
| --- | --- | --- | --- |
| `facts.rs` | `bifrost-analysis/src/analyzer/rust/facts.rs` | `.agents/phase2/rust-usage-v2/facts.rs` | Phase 2, "Restore the write path" |
| `usage.rs` | `bifrost-analysis/src/analyzer/rust/usage.rs` | `.agents/phase2/rust-usage-v2/usage.rs` | Phase 2, "Restore the read path" |
| `usage_walks.rs` | `bifrost-analysis/src/analyzer/rust/usage_walks.rs` | `.agents/phase2/rust-usage-v2/usage_walks.rs` | Phase 2, "Restore the read path" |
| `usage_queries.rs` | `bifrost-analysis/src/analyzer/rust/usage_queries.rs` | `.agents/phase2/rust-usage-v2/usage_queries.rs` | Phase 2, "Restore the read path" |
| `fact_catch_up.rs` | `bifrost-analysis/src/analyzer/rust/fact_catch_up.rs` | `.agents/phase2/rust-usage-v2/fact_catch_up.rs` | Phase 2, "Restore the write path" |
| `rust_*` row shapes, writer, reader, inverted lookups | `bifrost-analysis/src/analyzer/store/mod.rs` | `.agents/phase2/rust-usage-v2/store-rust-facts.rs` | Phase 2, "Restore the write path" |
| `TreeSitterAnalyzer::persist_live_blobs` | `bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs` | `.agents/phase2/rust-usage-v2/tree-sitter-analyzer-persist-live-blobs.rs` | Phase 2, "Restore the write path" |

### Parked v2 tests

| Test | Arc home | Why it cannot run in Phase 1 | Re-enabled by |
| --- | --- | --- | --- |
| `rust_fact_tables_record_exports_imports_modules_and_occurrences` | `store/mod.rs` | `AnalyzerStore::rust_usage_facts` does not exist | Phase 2, "Restore the write path" |
| `rust_fact_tables_answer_the_inverted_name_lookups` | `store/mod.rs` | the inverted lookups do not exist | Phase 2, "Restore the write path" |
| `rust_fact_rows_cascade_with_their_blob` | `store/mod.rs` | nothing writes fact rows | Phase 2, "Restore the write path" |
| `rust_fact_rows_are_stable_across_a_re_analysis_of_the_same_content` | `store/mod.rs` | nothing writes fact rows | Phase 2, "Restore the write path" |
| `rust_module_route_tables_record_scopes_routes_gates_and_item_macros` | `store/mod.rs` | `RustModuleRouteFacts` does not exist | Phase 2, "Restore the write path" |
| `rust_module_route_rows_cascade_with_their_blob` | `store/mod.rs` | as above | Phase 2, "Restore the write path" |
| `rust_module_route_rows_are_stable_across_a_re_analysis_of_the_same_content` | `store/mod.rs` | as above | Phase 2, "Restore the write path" |
| `batched_module_route_facts_match_the_per_blob_read` | `store/mod.rs` | `AnalyzerStore::rust_module_route_facts` does not exist | Phase 2, "Restore the write path" |
| `rust_usage_readiness_and_warmth_are_distinct_and_vacuous_without_rust` | `analyzer/workspace.rs` | v2's readiness/warmth distinction has no meaning while an index is built | Phase 2, "Restore the read path" |

Two arc unit tests inside A1 files were reverted with their files rather than parked, because A1 takes upstream's side verbatim: `rust/hierarchy.rs`'s `warm_query_indexes_builds_the_hierarchy_and_catches_up_the_usage_facts` (upstream's `..._builds_hierarchy_and_usage_indexes_ahead_of_demand` is the same claim against the index) and the arc's edit to `tests/suite_issues/issue_1230_rust_scan_complexity.rs::import_expansion_resolves_module_files_once_per_specifier` (upstream's form makes the same claim about construction rather than about answering). Both are recoverable from `0a53a550`.

### Rewritten rather than parked

| Test | Arc claim | Phase 1 form |
| --- | --- | --- |
| `workspace_startup_runs_the_usage_index_warm_and_reports_its_readiness` | `get_active_workspace` reports the session's own readiness, not a constant | reads `WorkspaceSession::usage_index_ready`, which answers from the warm thread instead of from the fact catch-up |
| `a_one_shot_service_does_not_start_the_usage_index_warm_at_startup` | a synchronously constructed service spends no startup on Rust usage work | asserts `usage_index_warm.is_none()` under `StartupIndexWarm::OnDemand` |
| `a_request_racing_the_startup_usage_index_warm_returns_the_warm_answer` | a racing request returns the settled answer, not a partial one | waits on `usage_index_ready` instead of on fact warmth |

## Plan of Work: Phase 2 (owner-gated)

**Do not begin Phase 2 without explicit owner authorization.** Phase 1 must be reviewed first. This section exists so that the authorization decision can be made against a concrete plan, and so that Phase 1's choices can be checked against what Phase 2 needs.

Phase 2's goal, in user terms: the same workspace that Phase 1 makes correct also becomes fast and small again, without giving up upstream's include-expansion resolution. Concretely, a large Rust workspace that has already been analyzed answers a usage question after a sub-second catch-up rather than an 88 second, 30 gigabyte index build, and upstream's SpacetimeDB, Candle, Tokenizers and cross-Cargo include cases still resolve correctly.

The acceptance tests are already in the tree after Phase 1: upstream's `tests/suite_usages/rust_include_inverse_regression.rs` and `tests/suite_usages/rust_top30_inverse_regression.rs`, roughly 430 lines from commits `46e7bf58` and `649bebcb`. Phase 2 is done when those pass against the v2 substrate, together with every test in the cfg-ignored table above, re-enabled.

The work, in outline:

Restore the dormant v2 sources from `.agents/phase2/rust-usage-v2/` into `crates/bifrost-rust/src/`, adjusting their module paths and visibility for the crate boundary. They were written against `crates/bifrost-analysis/src/analyzer/rust/`, where `super::` reached the analyzer; in `bifrost-rust` the analyzer is reached through the crate's own interface, so every `super::`-relative path needs review. Re-declare them in `crates/bifrost-rust/src/lib.rs`.

Restore the write path: `crates/bifrost-core/src/analyzer/parsed_file.rs` regains a `rust_usage_facts` field, the Rust declaration walk populates it from the tree it already holds, and `crates/bifrost-analysis/src/analyzer/store/mod.rs` persists it into the `rust_usage_facts` and `rust_module_routes` tables that Phase 1 already created. No new migration is needed. Add the two detector salt tokens `per-file-usage-facts-2026-08` and `cargo-route-facts-2026-08` at this point, because this is where Rust detector semantics actually change.

Restore the read path: replace each `RustUsageIndex` accessor with the corresponding lazy walk, in the order the arc's own milestones used, so that each step is independently testable. `.agents/plans/rust-usage-index-v2.md` records that order and the counters that proved each step.

Rebuild include-expansion routes on the v2 substrate. This is the genuinely new design work and the reason Phase 2 is not a revert. Upstream's `include_routes` is a whole-workspace map built eagerly; the v2 equivalent is per-file include facts written as store rows at parse time, plus a lazy walk that follows Cargo, module, host-import and nested-include provenance on demand. Read `crates/bifrost-rust/src/usage_index.rs` as it stands after Phase 1 to recover the exact provenance kinds and the resolution order, then express the same resolution as a walk. Keep upstream's two regression suites passing at every step; they are the specification.

Delete `RustUsageIndex` last, exactly as the arc's Milestone 5 did, once nothing reads it.

Finally, re-enable every row of the cfg-ignored table, remove `.agents/phase2/rust-usage-v2/`, and update this plan's `Outcomes & Retrospective`.

## Validation and Acceptance

Phase 1 is accepted when all of the following hold, in the working tree at `/mnt/optane/bifrost-nlp` on branch `bifrost-nlp-ft`:

`cargo fmt` produces no diff.

`cargo clippy --workspace --all-targets --all-features -- -D warnings`, run through `scripts/with-isolated-cargo-target.sh`, exits zero.

`cargo nextest run --workspace --all-targets --no-fail-fast` fails only the tests in the upstream baseline, and nothing else. That baseline is **five** tests at upstream tip `b48412bf`, not the four recorded at `ce8857c8`; the fifth, `analyze_diff_gives_introduced_and_deleted_symbols_their_whole_callee_list`, was broken by upstream's own `71f03d40` and reproduces on `origin/master` unaided. Re-measure the baseline against the upstream commit actually being merged, never against an older one.

`cargo test --workspace --doc` passes.

Every row of the port table reads `landed` or `adopted upstream's` with its pin named and passing.

The merge commit and the port commits exist on `bifrost-nlp-ft` and nothing has been pushed.

## Idempotence and Recovery

The merge can be abandoned at any point with `git merge --abort`, which returns the tree to `0a53a550` with no loss; the arc's commits are all reachable from the branch tip and nothing in this plan rewrites history. Never stash: the owner's rule, and also a practical one, because a stash during a conflicted merge is easy to lose.

Re-running a step is safe. Conflict resolution is idempotent because `git checkout --theirs` and hand edits both converge on the same file contents. The migration renumbering is not idempotent if run twice with different numbers, so check `CURRENT_MIGRATION_VERSION` before editing it: it should read 16 before the edit and 19 after.

If the featureless suite shows a failure outside the baseline and the cause is not obvious, bisect against the merge by checking the same test on `origin/master` in the scratch worktree at `/mnt/containers/bifrost-upstream-baseline` and on `0a53a550`. A test that fails on both sides is not merge damage.

## Interfaces and Dependencies

After Phase 1, these must exist:

In `crates/bifrost-core/src/analyzer/capabilities.rs`, the capability trait carries both sides' additions:

        fn prefetch_import_targets(
            &self,
            files: &[ProjectFile],
            import_infos: Option<&HashMap<ProjectFile, Vec<ImportInfo>>>,
            cancellation: &CancellationToken,
        ) { }

        fn import_reachability(&self, ...) -> ImportReachability;

In `crates/bifrost-core/src/analyzer/model.rs`, `ImportInfo` carries both sides' fields, `is_global: bool` and `binder_span: Option<Span>`, and `crates/bifrost-core/src/analyzer/parsed_file.rs` no longer declares `import_statements`.

In `crates/bifrost-core/src/cache_db.rs`, `CURRENT_MIGRATION_VERSION` is 19 and `CACHE_MIGRATION_SQL` has 19 entries.

In `crates/bifrost-analysis/src/analyzer/store/epoch.rs`, the blob-store salt is `analyzer-blob-store-v9-import-bindings-with-binder-span`.

In `crates/bifrost-core/src/analyzer/pool_memo.rs`, `PoolSafeMemo` is public and the dedicated build pool spawn helper is available to `bifrost-analysis`.

In `crates/bifrost-rust/src/imports.rs`, `RustAnalyzer` overrides `prefetch_import_targets`.

## Artifacts and Notes

The merge, reproduced from a clean tree, reports the expected shape:

    $ git merge origin/master
    Automatic merge failed; fix conflicts and then commit the result.
    $ git status --short | grep -c '^UU\|^DU'
    53

The three-way port recipe for a file upstream copied and shrank, using Go's imports as the example:

    git show db9f60c3:crates/bifrost-analysis/src/analyzer/go/imports.rs > /tmp/base
    git show 0a53a550:crates/bifrost-analysis/src/analyzer/go/imports.rs > /tmp/ours
    git show origin/master:crates/bifrost-go/src/imports.rs              > /tmp/theirs
    git merge-file -p /tmp/ours /tmp/base /tmp/theirs

## Revision notes

2026-08-09: first version, written before executing Phase 1, from the preserved merge-state inventory, the arc's commit list and design records, and a re-run of the merge against upstream tip `b48412bf`. The port table, the cfg-ignored table and the dormant-sources table are filled in as Phase 1 proceeds; they are empty here because the work has not started.
