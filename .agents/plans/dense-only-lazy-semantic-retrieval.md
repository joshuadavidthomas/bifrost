# Dense-only, lazily resolved semantic retrieval

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan is maintained in accordance with `.agents/PLANS.md` in the repository root. Read that file before you revise this plan.

## Purpose / Big Picture

Bifrost has an optional semantic code search tool named `semantic_search`. It is built by the Cargo feature `nlp` and lives in the crate `crates/bifrost-nlp`. Today, before the tool can answer its first question in a process, it builds a large in-process data structure called the "active index". That structure copies every code chunk of the current checkout into SQLite `TEMP` tables, builds a full-text (BM25) search index over them, and loads a parallel copy into Rust maps. Issue #1904 measured the cost: 1,731 seconds of readiness wait across 106 query runs, with a worst single wait of 451 seconds on Firefox, charged to whoever asked first. Issue #1929 measured a floor of about 100 CPU seconds on LLVM even with a warm page cache, of which about 31 CPU seconds was the BM25 full-text build alone.

Two independent facts make that work unnecessary.

First, the owner A/B tested "hybrid" retrieval (dense vector search fused with BM25 lexical search) against deeper dense-only arms and found hybrid a net loss: it cost more tokens and money for no measurable task benefit (the numbers are quoted in the Decision Log). BM25 is therefore being removed outright, not made optional. This deletes the single most expensive hydration step.

Second, the persistent per-repository cache database already holds everything retrieval needs: chunk rows (`semantic_file_chunks`) keyed by `(blob_oid, rel_path, chunk_ord)` with an index on `vector_hash`, and quantized vectors (`semantic_vectors`) keyed by `vector_hash`. The only thing that is specific to one checkout is the answer to "which `(rel_path, blob_oid)` pairs are live right now", and the indexer already computes exactly that map during its git identity walk. There is no need to project the cache through that map ahead of time. Retrieval can score vectors straight out of the persistent table and check liveness per hit, skipping hits that belong to a blob the working tree no longer has.

After this change, becoming ready means: walk git for the identity map, open the cache database. Nothing else. A user starting Bifrost on a warmed repository gets a queryable `semantic_search` in the time it takes to walk the tree, instead of waiting minutes for a TEMP-table projection and an FTS build that will be thrown away when the process exits.

You can see it working by running the semantic test suite and by timing the two commands in `Validation and Acceptance`: `semantic_search_status` reports `phase: "ready"` promptly, and the first `semantic_search` call returns hits whose files and line spans are correct even when the cache still contains chunks for older blobs of edited files.

## Progress

- [x] (2026-08-10 09:10Z) Read issues #1904 and #1929, `AGENTS.md` sections "Index readiness design", "SQL and the analyzer store", "Semantic search (nlp toolset)", and the whole of `crates/bifrost-nlp/src`.
- [x] (2026-08-10 09:40Z) Confirmed the persistent schema already carries the index the lazy path needs (`semantic_file_chunks_by_vector`) and that `materialize_missing` reads only persistent tables.
- [x] (2026-08-10 10:20Z) Wrote this plan.
- [x] (2026-08-10 11:30Z) Milestone 1: deleted BM25 (module, FTS build, fusion leg, tokenizer version, `fts_tokens` writes).
- [x] (2026-08-10 12:10Z) Milestone 2: replaced `ActiveIndex` with `LiveSet` lazy retrieval in `crates/bifrost-nlp/src/retrieval.rs`.
- [x] (2026-08-10 12:40Z) Milestone 3: status/readiness semantics and the persistent-schema migration `0022`.
- [x] (2026-08-10 13:30Z) Milestone 4: tests and gate.
- [x] (2026-08-10 15:10Z) Fail-before evidence captured against 960ec591 in a throwaway detached worktree.
- [x] (2026-08-10 16:05Z) Before/after measurement on a synthetic 100k-file corpus (see `Outcomes & Retrospective`).

## Surprises & Discoveries

- Observation: the persistent chunk table already had the exact index the lazy per-hit lookup needs, so no schema addition was required for retrieval; the only schema change is subtractive.
  Evidence: `crates/bifrost-core/migrations/cache/0014-semantic-file-documents.sql` creates `CREATE INDEX semantic_file_chunks_by_vector ON semantic_file_chunks(vector_hash);`.

- Observation: keeping the candidate pool bounded during the scan removes a second, unmeasured cost. The old scan accumulated one `([u8; 32], f32)` pair for every distinct active vector in a `Vec` before ranking: roughly 36 bytes times millions of chunks per query. The bounded heap holds at most `8 * vector_limit` entries.
  Evidence: `crates/bifrost-nlp/src/query.rs` before this change, `let mut hash_scores: Vec<([u8; 32], f32)> = Vec::new();` filled from every `scan_vectors` batch.

- Observation: early termination of candidate resolution is exact, not approximate. Candidates are visited in descending score and a symbol's score is defined as its best chunk's score, so once `vector_limit` distinct live symbols have been collected no later candidate can displace any of them.

- Observation: `ALTER TABLE ... DROP COLUMN` works on a `WITHOUT ROWID, STRICT` table in the bundled SQLite as long as the column is not part of the primary key, an index, a CHECK, or a foreign key. `fts_tokens` is none of those, so the migration does not have to rebuild `semantic_file_chunks` (which on a large cache would be a multi-gigabyte table copy).
  Evidence: migration `0022-drop-bm25-lexical-columns.sql` applies cleanly against a cache built by migration 14, verified by `cargo nextest run -p brokk-bifrost-nlp`.

- Observation: the old vector scan was not a scan. `ActiveIndex::scan_vectors` selected from the TEMP `active_vectors` table joined to `semantic_vectors`, so every vector cost a B-tree probe into the persistent table. The lazy path reads `semantic_vectors` directly, which is a bare `SCAN`. On the synthetic 300k-vector corpus that alone moved the query's scan phase from about 915 ms to about 835 ms even though the new scan visits strictly more rows (it does not pre-filter to the active set).

- Observation: dropping `fts_tokens` shrank the synthetic 300k-chunk cache from 414.0 MiB to 263.0 MiB, a 36% reduction. The exact fraction is corpus-dependent -- the synthetic chunks carry about 480 bytes of tokens each, which is in the range `fts_text` produced for a small function but is not a measurement of any real repository.

- Observation: the 11 `bifrost_benchmark_run` failures seen in the first featureless run were an artifact of running the suite against a dirty working tree. That harness compares the benchmark binary's build identity against the checkout and refuses a `-dirty` mismatch. Re-running with the measurement scaffolding removed gave 9,953 passed and 0 failed.
  Evidence: `benchmark harness build identity d00b41e2... does not match current checkout d00b41e2...-dirty.19904a61...; rebuild both bifrost and bifrost_benchmark`.

## Decision Log

- Decision: delete BM25 entirely rather than gate it behind a knob.
  Rationale: the owner A/B tested hybrid against a deeper dense arm and found hybrid a net loss. A disabled-but-present lexical arm would keep the `fts_tokens` write cost, keep the FTS build code alive, and keep a second scoring scale in the response for no benefit. The house rule against `mode` flags applies.
  Date/Author: 2026-08-10, Claude (owner-directed).

- Decision: drop `semantic_file_chunks.fts_tokens` and `cache_state.bm25_tokenizer_version` with migration `0022` instead of leaving them unused.
  Rationale: `fts_tokens` is a per-chunk text column that exists only to feed the deleted FTS index; on a large cache it is a significant fraction of the chunk table. No persistent *table* served BM25 alone, so the migration is two `DROP COLUMN` statements and no data loss for retrieval. Leaving dead columns would violate "the schema and its views are the interface".
  Date/Author: 2026-08-10, Claude.

- Decision: do not bump any embedding or chunker salt.
  Rationale: `embed_fingerprint` and `chunker_version` describe what is stored; neither the documents nor the vectors change. Only the retrieval path and a lexical-only column change. A salt bump would needlessly discard warmed multi-gigabyte caches. `bm25_tokenizer_version` is not bumped either -- it is removed.
  Date/Author: 2026-08-10, Claude.

- Decision: remove the `BIFROST_SEMANTIC_SEARCH_PROFILE` environment knob and its three profiles, and fix the dense leg at `2 * k` with the git co-edit leg at `k`.
  Rationale: the knob existed to run the A/B that has now concluded. Its results are recorded in `.agents/plans/bifrost-localizer-cim-eval.md` under "Results": at a constant nominal pool of `3 * k`, the arm budgets `k/k/k` (vector/BM25/co-edit) resolved 53.8%, `3k/0/0` resolved 54.6%, and `2k/0/k` resolved 54.9%, against a 52.0% no-semantic baseline. The best arm keeps the co-edit leg, so `2 * k` dense plus `k` co-edit becomes the only behavior. The total nominal pool is unchanged, so the caller's token cost does not move. Note the plan's own caveat that no arm's improvement over baseline is statistically established; what the campaign does establish is that the lexical leg cost tokens and money without a measurable benefit, which is what this change acts on.
  Date/Author: 2026-08-10, Claude.

- Decision: propagate a vector-scoring error instead of skipping the row.
  Rationale: the previous scan did `scorer.score(..).ok()`, silently dropping any vector the quantizer could not decode. The house rule forbids error handling with no recovery action, and a corrupt or wrong-dimension code blob is a structured failure the caller should see rather than a silently shorter result. Dimension drift cannot occur in practice because `ensure_index_compatible` wipes the vectors when the embedder fingerprint changes.
  Date/Author: 2026-08-10, Claude.

- Decision: over-retrieve by a factor of 8 over the dense leg depth, and stop resolving as soon as the leg is full.
  Rationale: the factor bounds work in the pathological case where many top-scoring vectors belong to dead blobs or collapse onto the same symbol. A warmed exact checkout has a near-zero dead fraction, so the pool is nearly always resolved after a handful of lookups; the bound only matters for a long-lived cache between garbage collections. Eight was chosen because a single symbol can legitimately own several chunk vectors and because the cost of the bound is at most `8 * 2 * k` indexed point lookups (1,600 at the maximum `k` of 100), which is microseconds each.
  Date/Author: 2026-08-10, Claude.

- Decision: rename the status field `indexed_chunks` to `indexed_files` rather than keep a chunk count.
  Rationale: an exact live chunk count is precisely the workspace-wide join this change exists to delete. The live file count is exact and free (it is the length of the identity map). `phase` and `pending_batches`, which the anvil readiness hook polls, are untouched.
  Date/Author: 2026-08-10, Claude.

## Context and Orientation

Everything below refers to files by their path from the repository root.

The optional feature `nlp` is off by default (`default = []` in the root `Cargo.toml`). It builds `crates/bifrost-nlp` and exposes two tools through the MCP server in `crates/bifrost-mcp`: `semantic_search` (described in `crates/bifrost-mcp/src/mcp_nlp.rs`, dispatched in `crates/bifrost-mcp/src/searchtools_service.rs` at `handle_semantic_search`) and `semantic_search_status` (`handle_semantic_search_status`).

The cache is a single SQLite database per primary git repository, shared by all of that repository's worktrees. Its path is resolved by `crates/bifrost-nlp/src/store.rs::semantic_db_path`. Its schema is created by the numbered migrations in `crates/bifrost-core/migrations/cache/`, applied by `crates/bifrost-core/src/cache_db.rs`. Three tables matter here, all created by `0014-semantic-file-documents.sql`:

`semantic_files(blob_oid, rel_path, language, materialized_at)` records that a given file content at a given path has been processed. `semantic_file_chunks(blob_oid, rel_path, chunk_ord, symbol, start_line, end_line, fts_tokens, vector_hash)` records one row per function found in that file, and has a secondary index `semantic_file_chunks_by_vector` on `vector_hash`. `semantic_vectors(vector_hash, dim, vector)` holds the quantized embedding bytes for each distinct embedding document. "Quantized" means the 512-dimensional float vector is compressed to one byte per dimension by `crates/bifrost-nlp/src/quant.rs`; a query is scored against those bytes directly.

The word "blob OID" means the 40-character hexadecimal git object id of a file's exact contents. The cache is content-addressed by `(blob_oid, rel_path)` because the workspace-relative path is part of the text that gets embedded, so the same bytes at two paths are two different documents.

The background worker is `crates/bifrost-nlp/src/indexer.rs`. On a full build it asks git for the current `rel_path -> blob_oid` map of every analyzed file (`crates/bifrost-nlp/src/gitcache.rs::working_tree_oids`), materializes any pair the cache has never seen (`materialize_missing`, which reads only the persistent table through `SemanticStore::missing_files`), and then -- this is the part being replaced -- builds the "active index".

The active index is `crates/bifrost-nlp/src/active_index.rs`. `ActiveIndex::build` opens a second, read-only connection, creates four `TEMP` tables plus one FTS5 virtual table, inserts the whole identity map into `active_files`, joins that against `semantic_file_chunks` into `active_occurrences`, projects distinct vector keys into `active_vectors`, builds `bm25_idx` from the `fts_tokens` of every active chunk, then reads all of `active_occurrences` back into Rust `HashMap`s. `TEMP` tables die with the connection, so all of this is per-process and transfers nothing to the next process.

The query pipeline is `crates/bifrost-nlp/src/query.rs`. It scans every active vector, scores it, resolves each scored vector to its function occurrences through the Rust maps, runs a BM25 query over `bm25_idx`, and finally seeds a git co-edit ranking from the union of the two legs.

"Liveness" in this plan means: a chunk row is live for this worktree if the identity map says `rel_path` currently resolves to exactly that row's `blob_oid`. A chunk row for any other `blob_oid` of the same path is a leftover from an older version of the file and must never be returned.

## Plan of Work

The work is four milestones. Each leaves the tree compiling and its tests passing.

### Milestone 1: delete BM25

Delete `crates/bifrost-nlp/src/bm25.rs` and its `pub mod bm25;` line in `crates/bifrost-nlp/src/lib.rs`. Delete the constants `MAX_QUERY_TOKENS` and `BM25_TOKENIZER_VERSION` from `lib.rs`; both existed only for the lexical arm.

In `crates/bifrost-nlp/src/materialize.rs`, remove the `use super::bm25::fts_text;` import, the `fts_tokens` field of `PendingChunk`, its computation in `extract_file`, and its propagation in `write_group`.

In `crates/bifrost-nlp/src/store.rs`, remove the `fts_tokens` field from `FileChunkIn`, drop it from the `INSERT` in `put_files`, and reduce `ensure_index_compatible` to two contract strings (`fingerprint`, `chunker_version`).

In `crates/bifrost-nlp/src/indexer.rs`, drop `BM25_TOKENIZER_VERSION` from the `ensure_index_compatible` call.

In `crates/bifrost-nlp/src/query.rs`, delete the whole lexical leg: `bm25_symbol_candidates`, the `bm25_ranked` field of `SemanticSearchResult`, the `bm25` field of `RetrievalLegCounts`, the BM25 section of `render_text`, and the second argument of `build_seeds`. Delete the `SearchProfile` enum and the `BIFROST_SEMANTIC_SEARCH_PROFILE` environment variable; replace them with two constants: the dense leg takes `2 * k` and the co-edit leg takes `k`, the budgets of the winning arm in the sweep cited in the Decision Log.

In `crates/bifrost-mcp/src/mcp_nlp.rs`, the tool description mentions nothing lexical, so no change is needed there; verify this rather than assume it.

Update the Python client `bifrost_searchtools/models.py` (`SemanticSearchResult`, `SemanticSearchStatus`) and the test `python_tests/test_searchtools_client.py` to match the new wire shape.

### Milestone 2: lazy, liveness-checked retrieval

Create `crates/bifrost-nlp/src/retrieval.rs` and delete `crates/bifrost-nlp/src/active_index.rs`. The new module holds one type:

    pub struct LiveSet {
        oids: HashMap<String, String>,
        session: Mutex<Connection>,
    }

`LiveSet::open(store: &SemanticStore, path_to_oid: HashMap<String, String>) -> Result<Self, String>` takes ownership of the identity map the indexer already built and opens a read-only connection with `brokk_bifrost_analysis::cache_db::open_readonly_temp_connection`. It must issue no query against `semantic_files`, `semantic_file_chunks`, or `semantic_vectors`. That is the whole of hydration.

`LiveSet::apply_changes(&mut self, changed: &HashMap<String, String>, removed: &[String])` updates the map in place. There is no SQL and it cannot fail.

`LiveSet::top_candidates(&self, scorer: &CodeScorer, pool: usize) -> Result<Vec<(Key, f32)>, String>` streams `SELECT vector_hash, vector FROM semantic_vectors` in batches, scores each batch in parallel with rayon, and merges the results into a bounded max-of-worst heap of size `pool`. It returns at most `pool` candidates in descending score order. Bounding the heap is what keeps a query's peak memory independent of corpus size.

`LiveSet::resolve_live(&self, vector_hash: &Key) -> Result<Vec<FunctionHit>, String>` runs, for that one hash only:

    SELECT blob_oid, rel_path, symbol, start_line, end_line
    FROM semantic_file_chunks
    WHERE vector_hash = ?1

and keeps only rows where `self.oids.get(rel_path) == Some(blob_oid)`. This is the per-hit lookup that replaces the upfront join, and it is why the plan needs no `active_occurrences`.

`LiveSet::live_file_count(&self) -> usize` returns `self.oids.len()`, for status reporting.

In `crates/bifrost-nlp/src/query.rs`, the dense leg becomes: embed the query, build the scorer, take `top_candidates(scorer, 8 * vector_limit)`, then walk the candidates in order calling `resolve_live`, recording each symbol's best score and its file, and stopping as soon as `vector_limit` distinct symbols have been collected. `vector_limit` is `2 * k` and the co-edit leg takes `k`, per the arm sweep cited in the Decision Log. If the whole pool is consumed without filling the leg, push a note saying so; a merely short candidate list (a store with fewer vectors than the pool) is not worth a note.

In `crates/bifrost-nlp/src/indexer.rs`, change the shared handle from `Arc<RwLock<Option<ActiveIndex>>>` to `Arc<RwLock<Option<LiveSet>>>`, rename the accessor `active_index()` to `live_set()`, and make `full_build` call `LiveSet::open` and `update_files` call `apply_changes`.

### Milestone 3: readiness semantics and the schema migration

The four phases (`starting`, `ready`, `failed`, `closed`) all still describe something real after the change, so the phase vocabulary does not shrink; what changes is that `ready` is reached without the projection step. Rename `SemanticIndexStatus::indexed_chunks` to `indexed_files` and populate it from `LiveSet::live_file_count`. Leave `pending_batches`, `phase`, `materialized_files`, and `materialize_total_files` alone: the anvil readiness hook polls `phase == "ready"` and `pending_batches == 0` and must keep working.

Add `crates/bifrost-core/migrations/cache/0022-drop-bm25-lexical-columns.sql`:

    ALTER TABLE semantic_file_chunks DROP COLUMN fts_tokens;
    ALTER TABLE cache_state DROP COLUMN bm25_tokenizer_version;

and register it in `CACHE_MIGRATIONS` in `crates/bifrost-core/src/cache_db.rs`. The schema version is the migration count and is asserted at compile time there, so the expected count constant must be bumped in the same edit.

### Milestone 4: tests

Adapt the existing tests and add four pins. They are described with their fail-before behavior in `Validation and Acceptance`.

## Concrete Steps

Run everything from the repository root `/mnt/optane/bifrost-nlp`.

Focused crate tests while iterating:

    cargo nextest run -p brokk-bifrost-nlp

The whole semantic tool suite, which needs the feature:

    cargo nextest run --features nlp --test suite_semantic semantic_search

The featureless workspace baseline, which must stay at zero failures:

    cargo nextest run --workspace --all-targets --no-fail-fast

Formatting, doctests, and lints:

    cargo fmt
    cargo test --doc --workspace
    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Check free disk before any all-features or `nlp` build, because those builds are tens of gigabytes per target directory:

    df -h /mnt/optane /tmp

## Validation and Acceptance

Four new tests pin the new behavior. Each is written so that it fails against the pre-change tree wherever the mechanism admits it.

The first is `dead_blob_candidate_is_skipped_for_the_live_one` in `crates/bifrost-nlp/src/retrieval.rs`. It writes chunks for two blobs of the same path -- an old one and the one the identity map declares live -- and asserts that resolving the old blob's vector returns nothing while resolving the live blob's vector returns the live symbol with the live path. It fails before the change because `LiveSet` does not exist; the equivalent guarantee in the old code came from the TEMP projection, which is what the new code must reproduce without the projection.

The second is `hits_carry_the_live_file_and_span` in the same module: a chunk written with `start_line`/`end_line` must come back through the per-hit lookup with those exact values and the correct `rel_path`, proving the lazy `(file, span)` mapping.

The third is `hydration_reads_no_semantic_content_table`, the structural pin for "no occurrences-wide join at open". It builds a cache, then renames `semantic_files` and `semantic_file_chunks` out of the way through a separate writable connection, and asserts `LiveSet::open` still succeeds. This fails before the change with `no such table: semantic_file_chunks`, because `ActiveIndex::build` joins that table during hydration. It passes after, because hydration touches no content table at all. Two EXPLAIN QUERY PLAN pins accompany it: the per-hit lookup must report `SEARCH semantic_file_chunks USING INDEX semantic_file_chunks_by_vector`, and the candidate scan must report a plain `SCAN semantic_vectors` with no join.

The fourth is the status contract, updated in `tests/suite_semantic/semantic_search.rs`: after `wait_ready`, `phase` is `"ready"`, `pending_batches` is `0`, `indexed_files` equals the number of analyzed files, and a query issued at that moment returns hits. "Ready means queryable" is the assertion that matters.

Acceptance beyond the tests: on a warmed cache, `semantic_search_status` reaches `phase: "ready"` in the time of the git identity walk plus a database open, and the first `semantic_search` returns correct files and spans. The measurement table in `Outcomes & Retrospective` records the before/after.

## Idempotence and Recovery

Every step is repeatable. The migration is additive-by-subtraction and runs once; `rusqlite_migration` records the applied count in SQLite's `user_version`, so re-running the binary does not re-apply it. An older binary refuses a newer `user_version` rather than corrupting the cache, which is the documented behavior in `crates/bifrost-core/migrations/cache/README.md`.

If the migration must be abandoned, the recovery is to delete the cache file; every byte in it is derived data that rebuilds from git plus the model. Do not write a down migration -- the README forbids it for released files.

## Interfaces and Dependencies

In `crates/bifrost-nlp/src/retrieval.rs`, define:

    pub type Key = [u8; 32];

    pub struct FunctionHit {
        pub fqfn: String,
        pub path: String,
        pub start_line: Option<i64>,
        pub end_line: Option<i64>,
    }

    pub struct LiveSet { /* private */ }

    impl LiveSet {
        pub fn open(store: &SemanticStore, path_to_oid: HashMap<String, String>) -> Result<Self, String>;
        pub fn apply_changes(&mut self, changed: &HashMap<String, String>, removed: &[String]);
        pub fn live_file_count(&self) -> usize;
        pub fn top_candidates(&self, scorer: &CodeScorer, pool: usize) -> Result<Vec<(Key, f32)>, String>;
        pub fn resolve_live(&self, vector_hash: &Key) -> Result<Vec<FunctionHit>, String>;
    }

In `crates/bifrost-nlp/src/query.rs`, the response type becomes:

    pub struct SemanticSearchResult {
        pub vector_ranked: Vec<RankedSymbol>,
        pub coedit_ranked: Vec<RankedFile>,
        pub requested_leg_counts: RetrievalLegCounts,   // { vector, coedit }
        pub timings: SemanticSearchTimings,
        pub notes: Vec<String>,
    }

`retrieval_profile` and `bm25_ranked` are gone.

In `crates/bifrost-nlp/src/indexer.rs`:

    pub struct SemanticIndexStatus {
        pub indexed_files: usize,
        pub pending_batches: u64,
        pub phase: String,
        pub materialized_files: u64,
        pub materialize_total_files: u64,
    }

No new external dependency is introduced. `rayon`, `rusqlite`, and `fastrq` are already in `crates/bifrost-nlp/Cargo.toml`.

## Outcomes & Retrospective

The change landed as one commit on `bifrost-nlp-ft`. Net effect on the crate: 1,122 lines deleted (`crates/bifrost-nlp/src/bm25.rs` at 421 lines and `crates/bifrost-nlp/src/active_index.rs` at 701 lines) against 427 lines added in `crates/bifrost-nlp/src/retrieval.rs`, plus a 339-line `query.rs` reduced to roughly half its former size.

### Fail-before evidence

The structural pin was run against the pre-change tree in a throwaway detached worktree at commit `960ec591`, with the same test body expressed against the old API (`ActiveIndex::build` instead of `LiveSet::open`):

    thread 'active_index::tests::hydration_reads_no_semantic_content_table' panicked at
    crates/bifrost-nlp/src/active_index.rs:609:10:
    hydration must not depend on the chunk tables: "no such table: semantic_file_chunks"
    Summary [0.095s] 1 test run: 0 passed, 1 failed, 65 skipped

The same assertion passes on the new tree. The other three pins are preservation pins, not fail-before pins: the old TEMP projection also filtered by exact `(blob_oid, rel_path)`, so dead-blob exclusion and correct file/span mapping were already true; what changed is the mechanism that has to keep them true.

### Measurement

Measured on the WSL host, CPU basis, release profile, synthetic corpus: 100,000 files, 3 chunks each, 300,000 distinct 512-dimension vectors, warm page cache, identity map supplied pre-built so the git walk (which this change does not touch) is excluded from both sides. The pre-change side carries about 480 bytes of `fts_tokens` per chunk so its FTS build is representative. Each query figure is the last of three consecutive iterations; the spread across iterations was under 6%.

The warmed LLVM and Firefox caches under `/mnt/T9/repo-clones/.codescale-cache-perrepo-r26` could not be used: they are cache schema v18, the pre-change tree writes v21, and this tree writes v22, and the schema version is part of the database file name, so no single warmed cache can serve both sides. Re-indexing either repository twice would mean hours of GPU embedding. The synthetic corpus is roughly a seventeenth of LLVM's 5.5M chunks, so the absolute hydration figure below should be read as a shape, not as an LLVM prediction.

    phase                    before (960ec591)   after      delta
    hydration to ready        3,344.8 ms          0.5 ms     -99.98%
    first-query vector scan     894.1 ms        804.3 ms     -10%
    first-query hit resolve      95.6 ms          0.3 ms     -99.7%
    first-query lexical leg      12.2 ms            n/a      removed
    first query total         ~1,002 ms         ~805 ms      -20%
    cache file size             414.0 MiB       263.0 MiB    -36%

Hydration after the change is one SQLite connection open. The hit-resolve column is not a like-for-like comparison of the same work: the old code resolved every one of the 300,000 scored hashes through its Rust maps before ranking, whereas the new code resolves candidates in score order and stops at the leg limit, so it performed 20 indexed point lookups.

### Gate

    cargo fmt                                                   clean
    cargo nextest run -p brokk-bifrost-nlp                      54 passed
    cargo nextest run --workspace --all-targets --no-fail-fast  9,953 passed, 0 failed, 42 skipped
    cargo check --features nlp --all-targets                    clean
    cargo nextest run --features nlp (semantic_search suite)     12 passed
    cargo test --doc --workspace                                0 doctests exist in this workspace
    scripts/with-isolated-cargo-target.sh cargo clippy
      --workspace --all-targets --all-features -- -D warnings   clean

### What remains

Issue #1929 also asked for the git identity map itself to be cached against the git index checksum, since the Firefox walk was 115.6 s over 170,889 paths. That is untouched here and is now the whole of hydration: with the active set gone, "hydrate faster" means "walk git less". Commit 2fb18154 (batching the walk's attribute questions) already reduced it; whether more is needed should be re-measured before any further work.

The `.agents/plans/bifrost-localizer-cim-eval.md` campaign should be re-run at some point against dense-only retrieval to confirm the arm choice holds now that the pool is no longer split three ways. That is the owner's call, not a prerequisite for this change.

## Revision note

2026-08-10: the plan originally proposed a dense leg of `3 * k`, reasoning only from the owner's summary that "the deeper semantic arm won". Reading the actual campaign results in `.agents/plans/bifrost-localizer-cim-eval.md` showed the best arm was `2 * k` dense plus `k` co-edit, not `3 * k` dense alone, so the constant and every reference to it were corrected before implementation. The `Outcomes & Retrospective` section was added after the gate and the measurement completed. Both changes are recorded in the Decision Log with their evidence, because a future reader deciding whether to re-tune these depths needs to know they came from a measured sweep rather than from taste.
