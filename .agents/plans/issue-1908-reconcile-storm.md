# Move C++ out-of-line member reconciliation from query time to index time

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

The canonical rules for ExecPlans in this repository are in `.agents/PLANS.md`, relative to the repository root. Maintain this document in accordance with that file.

## Purpose / Big Picture

Today, when someone asks Bifrost for a C++ symbol such as `llvm.DAGTypeLegalizer.ExpandIntOp`, the analyzer answers partly by scanning. It reads every declaration in the workspace whose last name-part is `ExpandIntOp`, and then decides, one candidate at a time, whether that candidate's real identity is the name that was asked for. The cost of answering therefore depends on how many unrelated things in the repository happen to share the last name-part, not on how big the answer is. On the LLVM and Clang tree the last name-part `g` is shared by 2,898 declarations, and `foo` by 6,490.

After this change, that decision is made once per declaration when the file is indexed and stored on the declaration's row. Answering the same question becomes an indexed lookup: `WHERE reconciled_fq = ?`. The cost becomes proportional to the answer.

You can see it working two ways. First, a counter that already exists says so: `CppAnalyzer::reconcile_candidate_evaluation_count_for_test()` must read zero for a warm query, where today it reads the size of the candidate set. Second, an `EXPLAIN QUERY PLAN` assertion must show the lookup using an index on the new column rather than scanning.

This is fix C of issue #1908. Fixes A, B, and D shipped on 2026-08-10 and are described under "Context and Orientation" below; this plan is only the remaining layer, and it is deliberately **not** started, because it needs three measurements first.

## Progress

- [x] (2026-08-10) Fix A shipped: `get_symbol_sources` and `get_summaries` resolve bare identifiers under a real `FuzzyResolveBudget` capped at `SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES` (200). Commit "Bound the bare-identifier fuzzy fan-out in get_symbol_sources and get_summaries (#1908 fix A)".
- [x] (2026-08-10) Fix B shipped: the C++ reconcile memo is keyed by `CppReconcileGroupKey` (member identifier plus owner terminal) instead of by queried fq name. Commit "Key the C++ reconcile memo by member identifier and owner terminal (#1908 fix B)".
- [x] (2026-08-10) Fix D shipped: the request cancellation token reaches the fuzzy resolver's per-key poll and the reconcile candidate loop, and a stopped build publishes nothing. Commit "Thread the request deadline into get_symbol_sources and get_summaries (#1908 fix D)".
- [ ] Measurement 1: how many C++ declarations actually need re-keying, on LLVM and on Chromium. Not started.
- [ ] Measurement 2: index-time cost of the include-closure walk over every file that contains an out-of-line member, on LLVM. Not started.
- [ ] Measurement 3: whether the existing reverse-include index can drive invalidation when a header changes. Not started.
- [ ] Decide, from those three numbers, between a column on the declaration row and a sparse side table. Not started.
- [ ] Implement. Not started; do not begin before the three measurements exist and are recorded in `Surprises & Discoveries`.

## Surprises & Discoveries

- Observation: the reported wall time in the #1908 incident trace is inflated roughly 1.8x by the timing instrumentation itself, and that instrumentation overhead is its own latent problem, independent of this plan.

  Evidence: `profiling::Scope` in `crates/bifrost-core/src/profiling.rs` emits an `eprintln!` on BEGIN and another on END. Each takes the process-global stderr lock and issues a write syscall. Request 152 in the incident produced 4.89M candidate spans, so about 9.8M locked writes, plus one `format!("cpp.reconcile.candidate[{}]", unit.fq_name())` per candidate, where `fq_name()` allocates a `String`. Decomposing that request's 270.2 s wall: genuinely-real work was `cpp.reconcile.lookup` 96.3 s plus `cpp.reconcile.role` 12.4 s plus `cpp.reconcile.visible` 5.8 s plus `sql_definition_candidates.*` 26.9 s, about 141 s; instrumented per-candidate span time was 47.7 s; about 76 s was unattributed between spans, which is where the label `format!` lives. With `BIFROST_TIMING` off the same request would have cost roughly 150 s rather than 270 s.

  Consequence for this plan: every "before" number quoted from the incident trace is an upper bound, and any measurement taken for this plan must either run with timing off or state that it did not. The bug was real; the multiplier was not.

  Consequence beyond this plan: this is a noted follow-up, not part of fix C. Serializing every span through one global lock means enabling `BIFROST_TIMING` on a hot path changes the shape of what it measures, so the tool is least trustworthy exactly where it is most needed. A fix would buffer per thread and flush, or drop the per-candidate span entirely and keep only the aggregate note. Do **not** address it inside this plan; open it separately so it can be measured on its own.

- Observation: keying the reconcile memo purely by member identifier -- the shape the #1908 root-cause note originally sketched -- regresses #1566.

  Evidence: the in-crate test `reconcile_skips_same_named_members_of_unrelated_classes_1566` in `crates/bifrost-analysis/src/analyzer/cpp/tests.rs` asserts `visible_type_units_build_count_for_test() == 1` after one member query. A per-identifier key makes that count 2 on the four-file fixture, and on Chromium it would restore the ~75 s per-member-query cost #1566 removed, because the first query for an identifier would have to reconcile every same-named candidate in the repository. Fix B therefore kept the owner terminal in the key. Fix C removes the question entirely, since nothing is reconciled at query time at all.

## Decision Log

- Decision: ship fixes A, B, and D first and record C as a separate, unstarted plan.

  Rationale: A and D needed no new evidence and bound the damage. B removed the quadratic with a contained change to two memo cells. C changes the persisted schema and the indexing path, and its cost is unknown in exactly the three ways listed under `Progress`. Shipping it on the same evidence would have traded a query-time storm for a possible write-time one.

  Date/Author: 2026-08-10, Claude (Fable) for jbellis.

- Decision: do not substitute a text or regular-expression shortcut for the include-visible class table when moving reconciliation to index time.

  Rationale: the repository's design rule (see `CLAUDE.md`, "Design philosophy") prohibits replacing available structure with string scanning, and the structure is available here: `cpp.visible_type_units(file)` already returns the class table, memoized per file. A text shortcut would also silently change which declarations reconcile, with no way to tell a miss from a genuine absence.

  Date/Author: 2026-08-10, Claude (Fable) for jbellis.

## Outcomes & Retrospective

Not started. Fill this in when the measurements exist, and again at implementation.

## Context and Orientation

Read this section as if you have never seen this repository.

Bifrost analyzes source code and answers structural questions about it. For C++ it has a problem that does not arise in most languages: a member function can be *declared* in a header and *defined* in a `.cpp` file, and the definition's own text does not always say which class it belongs to in a form the parser can resolve on its own. The classic case is a `.cpp` file that says `using namespace foo;` and then `int Outer::Inner::method() const { ... }`. Read in isolation, that definition looks like it belongs to a class chain `Outer::Inner` in no namespace. It actually belongs to `foo::Outer::Inner`.

Bifrost calls the identity the parser assigns from the file alone the **provisional identity**, and the identity that the wider program actually gives it the **canonical identity**. Turning the first into the second is called **reconciliation**. The code that does it is `cpp_reconcile_definition_identity` in `crates/bifrost-cpp/src/identity.rs`. It works by asking which classes are visible to the defining file through its `#include` closure -- the **include-visible class table**, returned by `CppSource::visible_type_units(file)` and memoized per file on the analyzer -- and then re-partitioning the definition's qualifier against that table. `crates/bifrost-cpp/src/reconcile.rs` holds the pure re-partitioning function, `reconcile_out_of_line_member_identity`.

The important property, and the whole basis of this plan, is that **reconciliation of one declaration does not depend on the query**. It depends only on the declaration itself, on its own file's include-visible class table, and on its own file's `using namespace` directives. You can verify this by reading `cpp_reconcile_definition_identity` at `crates/bifrost-cpp/src/identity.rs`: every input it takes is derived from `unit` and `unit.source()`.

Despite that, reconciliation runs at query time. `CppAnalyzer::definitions(fq_name)` in `crates/bifrost-analysis/src/analyzer/cpp/mod.rs` folds in a set of **re-keyed** declarations: synthetic `CodeUnit`s that carry the canonical identity but the definition's real `.cpp` source, so that a canonical query resolves the declaration and its definition together. Those come from `CppAnalyzer::reconciled_definitions(fq_name)`, which after fix B works like this:

1. `cpp_reconcile_group_key(fq_name)` (in `crates/bifrost-cpp/src/identity.rs`) parses the queried name into a member identifier -- its last segment -- and an owner terminal -- the last dot-or-`$`-separated component of the segment before it, or `None` for a single-segment name.
2. `cpp_reconcile_candidates` reads every declaration in the workspace whose terminal identifier matches, via `CodeUnitIndex::lookup_candidates_by_identifier`, and buckets them by their own owner terminal. Memoized per member identifier on the analyzer in `reconcile_candidates_by_identifier`.
3. `cpp_reconcile_group` reconciles the candidates in the bucket the query names, and groups the re-keyed results under each one's canonical fq name. Memoized per `CppReconcileGroupKey` in `reconciled_definitions_by_group`.
4. The query is a map lookup into that group.

That is still a scan. Its input is "every declaration sharing the last name-part", which has nothing to do with the size of the answer.

The three fixes already shipped bound the damage but do not remove the scan:

* Fix A gives `get_symbol_sources` and `get_summaries` a real fan-out budget, so a bare identifier that more than `SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES` (200) declarations answer is reported by its count instead of expanded. The constant is in `crates/bifrost-analysis/src/searchtools/mod.rs`.
* Fix B re-keys the memo so that K queries sharing one member identifier share one candidate scan instead of running K of them.
* Fix D threads the request's cancellation token down to the candidate loop, so a scan that overruns the deadline stops and publishes nothing.

What none of them change: a single cold query for a hot identifier still reads and walks the whole same-terminal candidate set once.

The store is SQLite. The schema and its views are the interface (see `CLAUDE.md`, "SQL and the analyzer store"). Declarations live in the analyzer cache database, written during indexing. This plan adds a persisted fact to that write path.

## Plan of Work

Do not write code until the three measurements below exist. Each one can change the design.

**Measurement 1 -- how many declarations actually need re-keying.** Reconciliation is a no-op for the overwhelming majority of C++ declarations: a definition written as `ns1::ns2::Klass::method` reconciles to exactly its provisional identity and contributes nothing. Count, on LLVM and on Chromium, how many C++ declarations have a reconciled identity that differs from the provisional one. If the count is small relative to total C++ declarations, a sparse side table keyed by declaration is better than a column on every row: it is smaller, and its absence is meaningful rather than a default value. If the count is large, a column is better, because a side table would need a join on the hot path.

Take the count by building the workspace with a temporary instrumentation counter incremented in `cpp_reconcile_group` at `crates/bifrost-cpp/src/identity.rs` wherever `unit.fq_name() != canonical_fq`, driven over every declaration rather than over one group. Run with `BIFROST_TIMING` unset, for the reason recorded in `Surprises & Discoveries`. Record both the differing count and the total C++ declaration count.

**Measurement 2 -- index-time cost of the include-closure walk.** Reconciliation needs the include-visible class table of the *defining* file. At index time that means one include-closure walk per file that contains an out-of-line member. #1566 measured a single whale file's walk at 1-2 s; the open question is how many distinct files pay it on LLVM. Count the distinct files containing at least one out-of-line member on LLVM, and time `build_cpp_visible_type_units` over that set with a cold `visible_type_units_by_file` cache. The counter `CppAnalyzer::visible_type_units_build_count_for_test()` already records builds; use it rather than adding a new one. Report the total added wall time against the current full-index time for LLVM. If it is a large fraction, the plan needs a lazier variant -- reconcile on first read of a file rather than at index time -- and this plan must be revised before implementation.

**Measurement 3 -- invalidation.** Editing a header changes the include-visible class table of every file that includes it, directly or transitively, and therefore can change the reconciled identity of every out-of-line member in those files. Confirm that the existing `reverse_include_index` on `CppAnalyzer` (`crates/bifrost-analysis/src/analyzer/cpp/mod.rs`) can enumerate that set, and measure its size for a few realistic headers on LLVM -- a widely included one such as a core ADT header, and a narrow one. If a common header reaches most of the tree, index-time reconciliation trades a query-time storm for a write-time one on every header edit, and the plan must be revised to reconcile lazily per file with a validity stamp instead.

**Then, and only then, the implementation.** Persist the reconciled canonical fq name (and the provisional identity it re-keys, so the re-keyed unit can still be mapped back for ranges and signature metadata, as `CppReconciledDefinitionIndex::provisional_of` does today) as rows written when the file is indexed. Whether that is a column on the declaration row or a sparse side table is decided by measurement 1. Add an index on the reconciled name. Replace `CppAnalyzer::reconciled_definitions` with a lookup against it. Delete `reconcile_candidates_by_identifier` and `reconciled_definitions_by_group`, the two memo cells fix B added, and delete `cpp_reconcile_candidates`, `cpp_reconcile_group`, and `cpp_reconcile_group_key` from `crates/bifrost-cpp/src/identity.rs` -- keeping `cpp_reconcile_definition_identity`, which is the per-declaration function the indexer will call. Pin the query plan with an `EXPLAIN QUERY PLAN` assertion, per `CLAUDE.md`.

This subsumes fix B and removes the `lookup_candidates_by_identifier` term entirely. It also removes the cache-epoch problem the moka cells carry.

## Concrete Steps

Working directory is the repository root, `/mnt/optane/bifrost-nlp` in the environment where this plan was written.

Take measurement 1 and 2 against real corpora. The C++ corpora used for #1908 and #1748 are the CodeScaleBench workspaces `llvm-project--a8f3c97d` and a Chromium checkout; if they are not present, any full LLVM checkout is sufficient for measurement 1 and 2, and measurement 3 needs only LLVM.

Build the measurement binary from the existing reference-differential harness rather than a new one where possible:

    cargo build --release --bin bifrost_reference_differential

For a one-off smoke count that must not write a cache database, use `--cache-mode ephemeral`; for a warmed or resumable campaign use the default `--cache-mode persisted`.

Run every measurement with `BIFROST_TIMING` unset:

    env -u BIFROST_TIMING <command>

Record each result in `Surprises & Discoveries` with the exact command, the corpus revision, and whether the run was cold or warm.

## Validation and Acceptance

The measurements are accepted when all three are recorded in `Surprises & Discoveries` with commands and numbers, and the choice between a column and a side table is recorded in `Decision Log` with the number that decided it.

The implementation is accepted when all of the following hold.

Run the focused suite and expect it to pass:

    cargo nextest run -p brokk-bifrost --test suite_issues issue_1908

Add to `tests/suite_issues/issue_1908_reconcile_storm.rs` a test that resolves a reconciled canonical name on a warm analyzer and asserts `analyzer.reconcile_candidate_evaluation_count_for_test() == 0`. That test must fail before the change -- today the count is the size of the candidate bucket -- and pass after. The existing tests in that file must keep passing unchanged, in particular `issue_1908_regrouping_reconciliation_answers_exactly_what_per_fq_reconciliation_did`, which pins that a reconciled name answers with exactly its header declaration and its `.cpp` definition and nothing else.

The existing reconcile guards must keep passing unchanged:

    cargo nextest run -p brokk-bifrost-analysis --lib cpp::tests

In particular `reconcile_skips_same_named_members_of_unrelated_classes_1566` must still hold, and after this change it should hold trivially, because no query-time class-table build happens at all.

Add an `EXPLAIN QUERY PLAN` assertion for the new lookup, asserting that it uses the new index and does not scan the declaration table. Place it beside the other query-plan pins in the store tests.

Finally, run the featureless pre-push gate:

    cargo fmt
    cargo nextest run --workspace --all-targets --no-fail-fast
    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Do not enable the `nlp` feature for this work; it is unrelated to semantic search, and an `nlp` build can use tens of GiB per worktree.

## Idempotence and Recovery

The measurements are read-only and repeatable. Use `--cache-mode ephemeral` for one-off counts so no cache database is written; if a persisted cache is warmed and later suspect, delete `.bifrost/cache/bifrost_cache.v<N>.db` under the corpus root and re-run.

The implementation changes the persisted schema, so it needs a cache version bump; a workspace with an older cache must rebuild rather than read a database that has no reconciled column. Verify by pointing the new binary at a cache warmed by the previous version and confirming it rebuilds cleanly instead of returning wrong answers.

Temporary instrumentation counters added for the measurements must be removed before the implementation lands. Keep them on a scratch branch, not on `bifrost-nlp-ft`.

## Artifacts and Notes

The incident evidence is `ccx-incident-108`, at

    /mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/symbols-nlp-grepbad-r26/archives/ccx-incident-108/luna-symbols-nlp-r1-1786292854-4161539.zip

with `anvil-stderr.txt` (22,629,891 lines, 1.5 GB), `anvil-trace.jsonl`, and `result.json`. The per-request storm shape, for the record:

    R152  NOTE cpp.reconcile.candidates[g]    n=2898   x 1,277 occurrences
    R179  NOTE cpp.reconcile.candidates[foo]  n=6490   x   598 occurrences
    R160  NOTE cpp.reconcile.candidates[g]    n=2898   x   132 occurrences

The synthetic reproduction that fixes A, B, and D were pinned against is in `tests/suite_issues/issue_1908_reconcile_storm.rs`. Its fix-B fixture, `divergent_namesakes`, builds `count` classes each declaring `shared`, each with its out-of-line definition in its own `.cpp` under a `using namespace`. Measured on that fixture with K=12 and N=24:

    before fix B:  12 identifier-index scans, 288 candidate evaluations
    after  fix B:   1 identifier-index scan,   24 candidate evaluations

After fix C both numbers must be zero for a warm query.

## Interfaces and Dependencies

At the end of the implementation milestone, `crates/bifrost-analysis/src/analyzer/cpp/mod.rs` must still expose

    fn reconciled_definitions(&self, fq_name: &str) -> Arc<CppReconciledDefinitionIndex>

with unchanged observable behavior, so that `definitions`, `get_definitions`, `ranges`, and `signature_metadata` need no edit. Its body becomes a store lookup.

`crates/bifrost-cpp/src/identity.rs` must still expose

    pub struct CppReconciledDefinitionIndex {
        pub rekeyed: Vec<CodeUnit>,
        pub provisional_of: HashMap<CodeUnit, CodeUnit>,
    }

and

    fn cpp_reconcile_definition_identity(
        cpp: &dyn CppSource,
        unit: &CodeUnit,
        using_by_file: &mut HashMap<ProjectFile, Arc<Vec<String>>>,
    ) -> Option<ReconciledIdentity>

which becomes the function the indexer calls per declaration. `cpp_reconcile_candidates`, `cpp_reconcile_group`, `cpp_reconcile_group_key`, `CppReconcileCandidates`, and `CppReconcileGroupKey` are deleted.

Do not add a new workspace crate for this work. The write path belongs beside the existing C++ indexing code, and the store layer belongs where the other analyzer store code already lives; see `CLAUDE.md`, "Crate dependency boundaries".
