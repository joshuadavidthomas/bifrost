# Make analyzer refresh persistence incremental and overlap it with parsing

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be updated as work proceeds. Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

A language-analysis epoch changes whenever parser output stored on disk is no longer compatible with the current analyzer. Today an epoch change synchronously deletes every old row for that language, then parses every missing file into memory, waits for all parsing to finish, and writes each parsed file through its own SQLite transaction. On the 49.7-million-line RMerl C corpus this produced a 6.5 GiB write-ahead log, about 6.7 GiB of newly free database pages, roughly 18.1 million logical row writes for the first 49,233 blobs, and long phases that used only one CPU despite a 120-thread parser pool.

After this work, changing an epoch will make old rows invisible immediately without a multi-gigabyte synchronous delete. Parsing and CPU-heavy preparation of persistence rows will stay parallel, completed results will flow through a bounded channel, and one SQLite writer will commit adaptive batches. A forced corpus refresh should keep parsing CPUs busy while persistence begins early, use bounded memory, perform far fewer commits, and preserve exact per-blob visibility and retry behavior.

## Progress

- [x] (2026-07-14) Profiled the RMerl refresh and separated the quadratic C++ declaration straggler from the later persistence bottlenecks.
- [x] (2026-07-14) Measured the current database and write cardinality: 10.314 GiB main DB, 6.510 GiB WAL, 6.737 GiB freelist, and about 18.1 million logical rows for 49,233 completed C++ blobs.
- [x] (2026-07-14) Added deterministic failing contracts for generation visibility, parse/write overlap, and bounded transaction count; retain failure-isolation coverage for the writer implementation.
- [x] (2026-07-14) Filed and assigned #772 for epoch invalidation and #773 for the persistence pipeline.
- [x] (2026-07-14) Implemented #772 monotonic generation visibility, captured-token reads and writes, transactional migration, and bounded best-effort reclamation.
- [x] (2026-07-14) Implemented #773 parallel preparation, an eight-item bounded result channel, deterministic parse/persist overlap, and one adaptive batching writer.
- [ ] Validate forced-refresh production behavior; unit, integration, migration, race, failure-isolation, formatting, and all-target/all-feature clippy gates are complete.
- [ ] Record pushed commits, close the assigned issues, and update the reference-differential campaign plan.

## Surprises & Discoveries

- Observation: The low-load refresh was not caused by a missing Rayon pool.
  Evidence: GDB showed 120 workers. During the parse tail, 119 slept while one worker spent about an hour in `cpp::declarations::has_matching_declaration`; issue #771 removed that separate quadratic algorithm.

- Observation: Epoch deletion and result persistence are distinct serial costs.
  Evidence: `AnalyzerStore::ensure_language_epoch_value` deletes all old language blobs in one cascading transaction. Later, `TreeSitterAnalyzer::reconcile_file_states` first collects every Rayon result into a `Vec`, then calls `AnalyzerStore::write_parsed_blob` serially once per file.

- Observation: Parallel SQLite writers are not the answer.
  Evidence: The analyzer owns one SQLite connection and SQLite WAL mode still permits only one writer. Multiple writer threads would queue on the same connection or SQLite write lock while increasing memory and retry pressure.

- Observation: Batch size cannot be a fixed blob count alone.
  Evidence: The largest completed RMerl blob contained about 327,480 logical child rows, while the average completed blob contained about 369 logical rows. Batches must also be bounded by prepared row count and payload bytes.

- Observation: The epoch contract confirms that logical invalidation is currently inseparable from physical deletion.
  Evidence: `cpp_epoch_change_hides_old_rows_without_synchronous_physical_deletion` preserved correct public absence after an epoch flip, but physical counts changed from `[1, 1, 2]` to `[0, 0, 0]` through the cascading delete.

- Observation: The persistence contracts independently expose both scheduling and transaction boundaries.
  Evidence: With one parser latched after a peer result was ready, persistence-start remained `0`; separately, 257 small blobs under a 64-blob target produced 257 transactions instead of 5.

- Observation: Generation filtering is wider than the root `blobs` table.
  Evidence: Hydration uses many separate metadata and satellite-table queries; candidate SQL, integrity-count subqueries, path-symbol snapshots, JS/TS import eligibility, GC, and direct range projections can all leak or mix stale facts unless they share one captured current generation.

- Observation: A clock-based linger is not a valid batching boundary on a contended machine.
  Evidence: Root and independent review rejected an initial 2 ms receive timeout because interarrival gaps could silently restore one transaction per blob. The accepted pipeline uses an explicit `AllStarted` producer marker, adaptive caps, and deterministic production-path transaction-count tests instead.

- Observation: An epoch string cannot serve as publication identity, and checking only the current generation at write time is insufficient.
  Evidence: The accepted #772 design allocates a fresh monotonic token even for A to B to A, returns the complete storage-key token map from one immediate cutover transaction, and validates that captured map inside every read snapshot and mutation transaction. Stale prepared results remain authoritative in memory but are terminal and are never retried forever.

- Observation: Minimal generation tagging keeps migration metadata-only but makes same-OID replacement part of the mutation budget.
  Evidence: Generation is stored on root blobs and path-symbol rows while satellite tables retain their existing keys. A B-generation write therefore deletes an old A root for the same OID; adaptive batching now counts both cascaded old rows and new rows, isolates one oversized replacement for progress, and reports the combined mutation cost.

## Decision Log

- Decision: Treat epoch invalidation and streaming/batching as separate issue boundaries even though one ExecPlan coordinates them.
  Rationale: Generation visibility changes query correctness and reclamation, while the writer pipeline changes scheduling, memory, transactions, and failure handling. Each must be independently reducible and reviewable.
  Date/Author: 2026-07-14 / Codex

- Decision: Use generation-aware visibility instead of synchronous cascade deletion on epoch mismatch.
  Rationale: An epoch flip must be fast and atomic. Old rows can remain physically present as unreachable generations and be reclaimed incrementally after new data becomes visible.
  Date/Author: 2026-07-14 / Codex

- Decision: Keep exactly one SQLite writer and parallelize preparation around it.
  Rationale: Parsing, declaration flattening, sorting, lookup-key normalization, and serialization are CPU work and can use Rayon. SQLite mutation is inherently single-writer; a dedicated writer provides ordering and backpressure without lock contention.
  Date/Author: 2026-07-14 / Codex

- Decision: Bound writer batches by blob count, logical row count, and payload bytes.
  Rationale: Production blob sizes are highly skewed. A three-dimensional cap prevents one giant blob or many medium blobs from recreating multi-gigabyte transactions.
  Date/Author: 2026-07-14 / Codex

- Decision: Use a fresh monotonic generation identity in addition to the epoch string, and capture it before parsing or hydrating.
  Rationale: Analyzer versions can race or move from epoch A to B and later A again. Prepared writes must reject a generation that is no longer current, and multi-query hydration must run under one read snapshot so metadata and satellite rows cannot come from different generations.
  Date/Author: 2026-07-14 / Codex

- Decision: Do not use elapsed-time assertions for concurrency acceptance.
  Rationale: This machine has variable contention. Latches, channels, returned transaction statistics, and direct database visibility prove scheduling and batching deterministically.
  Date/Author: 2026-07-14 / Codex

- Decision: Bound prepared flow with the configured parser parallelism, an eight-item channel, and the capped writer batch, and expose observed item and payload high-water marks.
  Rationale: Each active parser can necessarily own one result. Keeping the post-worker queue small and the writer batch bounded removes the all-corpus retention barrier while making the conservative item bound and real payload high-water mark observable.
  Date/Author: 2026-07-14 / Codex

## Outcomes & Retrospective

Implementation is complete for both #772 and #773; the forced production refresh remains. Parsing and CPU-heavy SQLite-row preparation run on Rayon workers, prepared results stream through a bounded channel, and one writer commits adaptive batches. Epoch cutover atomically publishes fresh monotonic generation tokens without deleting old rows, and analyzers remain bound to the complete captured token map for all reads and writes. Bounded best-effort reclamation and generation-qualified GC remove stale rows later without changing the outcome of already committed mutations. Root and independent review rejected and repaired clock-based batching, oversized buffering, partial token maps, stale lazy-index poisoning, retryable stale dirty states, mixed WAL snapshots, uncounted old-row deletion cost, and cleanup failures reported as primary failures.

## Context and Orientation

`src/analyzer/store/mod.rs` owns the SQLite schema and `AnalyzerStore`. Parsed syntax facts are normalized across `blobs`, `blob_meta`, code-unit, range, signature, import, child, type-identifier, and related tables. Foreign keys use cascading deletes. `AnalyzerStore::ensure_language_epoch_value` stores the current language epoch and currently deletes all blobs for that language when the value changes. `AnalyzerStore::write_parsed_blob` opens a transaction and writes one complete `FileState` row by row.

`src/analyzer/tree_sitter_analyzer.rs` owns parsing and reconciliation. `TreeSitterAnalyzer::analyze_files` uses a configured Rayon thread pool and returns a complete `Vec<(ProjectFile, Option<FileState>)>`. `TreeSitterAnalyzer::reconcile_file_states` waits for that vector, then serially calls `persist_or_mark_dirty`, which calls `write_parsed_blob_with_retries`. A `FileState` is the in-memory result for one file. A parsed blob is the content-addressed persisted form of that state. A prepared blob in this plan means an owned, SQLite-ready representation whose expensive flattening, sorting, normalization, and serialization have already occurred outside the writer thread.

The cache uses content object IDs, so multiple live paths can refer to one parsed blob. A blob becomes query-visible only when its blob record and completion metadata are committed. Existing dirty-state fallback keeps an in-memory `FileState` when persistence fails; that behavior must remain intact.

`src/analyzer/store/epoch.rs` computes the epoch string. Epoch computation does not need redesign; only how the store makes a new epoch current and reclaims old generations changes.

The campaign driver is `src/bin/bifrost_reference_differential.rs`. It builds a persisted `WorkspaceAnalyzer` and is the end-to-end acceptance surface. Corpus commands that touch `/mnt/T9/repo-clones/.../.brokk` must run outside the filesystem sandbox.

## Plan of Work

First, add deterministic regressions before production changes. In `src/analyzer/store/mod.rs`, extend the epoch tests so an epoch change immediately hides old blobs from every public lookup/hydration path but leaves their physical content rows present immediately after the flip. In `src/analyzer/tree_sitter_analyzer.rs`, create a test harness with two parse jobs: a slow job blocks on a latch and a fast job completes. The fast job must reach a persistence-start observer before the slow latch is released. Add a writer batching contract that sends 257 small prepared blobs through a maximum-64-blob configuration and observes five committed transactions. Add an injected invalid prepared range or equivalent constraint failure to prove good peers commit while only the bad blob enters dirty fallback after retry isolation.

Second, add generation-aware storage. Extend the store schema so language-scoped blob visibility is associated with a fresh monotonic generation identity, separate from the possibly repeated epoch string. An epoch flip must atomically mark a fresh generation current without deleting content rows. Every store query that answers whether a blob exists, hydrates parsed state, enumerates candidates, reads metadata, reconstructs live path state, validates parsed-blob integrity, or derives path-symbol eligibility must filter through the same captured current generation; partial filtering is a correctness bug. Multi-statement hydration must use one SQLite read snapshot. Prepared writes carry their captured generation and fail rather than publish into a newer generation. Persist new blobs into the current generation. Add incremental reclamation that deletes unreachable old-generation rows in bounded transactions. Reclamation may run during later writes or an explicit maintenance call, but must never delay the epoch flip with a corpus-wide cascade. Schema migration and epoch tests must prove other languages remain visible.

Third, separate preparation from SQLite mutation. Introduce `PreparedParsedBlob` in `src/analyzer/store/mod.rs` or a tightly related private module. It should own the blob key, current generation, normalized/sorted stored units, ranges, signatures and metadata, imports, children, type identifiers, serialized metadata, estimated logical row count, and payload byte count. Move CPU-heavy work currently performed inside `write_parsed_blob` into a pure preparation function that requires no SQLite connection and can run on Rayon workers. Avoid cloning source text or `CodeUnit` graphs after ownership can be moved.

Fourth, replace the collect-all barrier. Change the reconciliation path so parser workers prepare successful results and send them through a bounded channel. The channel capacity must be configurable internally for tests and bounded by a conservative production default; if a weighted bound is practical, account for prepared payload bytes, otherwise combine a small item capacity with the writer's byte caps. One dedicated writer receives prepared blobs while parsing continues. It starts a transaction, accumulates until any cap is reached, writes all blobs, marks each complete in the same transaction, commits, and reports outcomes back to reconciliation. The caller retains only enough state to seed the small transient cache and to reconstruct dirty fallback for failures.

Fifth, preserve failure semantics. If a batch fails, roll it back and isolate the bad blob by bisection, savepoints, or bounded individual retry. Good blobs from the failed batch must still become complete; only irreducibly failing blobs enter `dirty_file_states`. A crash cannot expose a partial blob. Existing immediate retry policy may remain for transient busy errors, but transaction boundaries and result reporting must be explicit and testable.

Finally, expose internal build telemetry sufficient for end-to-end proof. Record phase durations and counts for epoch flip, reclamation, parse/prepare, persist batches, committed blobs, transactions, logical rows, and prepared bytes. Do not make wall clock a correctness gate, but use it to compare the same forced-refresh snapshot before and after. Update the reference-differential plan with the measured outcome.

## Concrete Steps

Work from `/home/jonathan/Projects/bifrost` on the existing branch. Do not create or switch branches.

Run the focused failing contracts before implementation and retain their exact failure text in this plan:

    cargo test --lib language_epoch -- --nocapture
    cargo test --lib persistence_pipeline -- --nocapture

After each milestone, run:

    cargo fmt --check
    git diff --check
    cargo test --lib analyzer::store --features nlp,python
    cargo test --lib analyzer::tree_sitter_analyzer --features nlp,python

Run affected integration suites and lint with isolated target storage when concurrent work could hold Cargo locks:

    scripts/with-isolated-cargo-target.sh cargo clippy --all-features --lib --test cpp_analyzer_test -- -D warnings

Before pushing Rust changes, run the repository gate when practical:

    cargo clippy --all-targets --all-features -- -D warnings

The known unrelated modified `tests/rust_analyzer_goto_definition.rs` must not be staged. If its pre-existing lint remains the only full-clippy failure, record that and retain the affected isolated clippy proof.

For end-to-end validation, build the release driver and run a forced persisted refresh outside the sandbox. Use the same RMerl command and limits recorded in `.agents/plans/reference-differential-corpus.md`, writing a new JSONL path rather than overwriting prior evidence. Capture process CPU utilization, peak RSS, epoch-flip duration, parse/prepare duration, persist transaction count, logical rows, and WAL/main DB sizes. Then rerun warm and confirm no correctness findings were introduced.

## Validation and Acceptance

The epoch contract passes only if changing one language epoch makes every old blob for that language unavailable through normal store queries immediately, preserves other languages, and leaves old physical rows present until bounded reclamation runs. The flip itself performs a bounded number of SQL statements independent of corpus size.

The overlap contract passes only if the fast prepared blob reaches persistence-start while the slow parser is still blocked. No elapsed-time threshold may substitute for the latch ordering.

The batching contract passes only if 257 small blobs with a 64-blob cap commit in five transactions, all 257 become complete, and reported statistics match. Row/byte cap tests must also force a batch boundary before the blob-count cap.

The failure contract passes only if one intentionally invalid blob cannot expose partial rows, good peers become complete after isolation, and exactly the bad blob is returned to dirty fallback.

The memory contract passes only if the pipeline is bounded and no longer holds every parsed `FileState` until the last file finishes. A test or telemetry must show the number/bytes of in-flight prepared results never exceeds the configured bound.

The production refresh is accepted only if it completes with the same analyzer correctness surface, has no hour-long `has_matching_declaration` straggler, begins persistence before parsing finishes, avoids a synchronous multi-gigabyte epoch-delete WAL burst, and materially reduces transaction count from one per blob. Wall-clock improvement is supporting evidence, not the correctness criterion.

## Idempotence and Recovery

Tests and preparation functions must be safe to rerun. Schema migration must be transactional. If migration fails, the previous cache must remain readable or be rejected cleanly without partial generation state. Old-generation reclamation is idempotent: rerunning it after interruption continues from remaining unreachable rows.

The writer must roll back failed batches. Retrying a prepared blob is safe because its content key and generation are deterministic and completion is atomic. If an external corpus run is interrupted, rerun the same command with the same output path only when resumable record rules allow it; otherwise use a fresh `/tmp` filename and preserve prior evidence.

Do not manually delete the RMerl cache while a corpus process is attached. Stop or let the process finish first, then use the explicit forced-refresh mechanism or a fresh snapshot for validation.

## Artifacts and Notes

The live pre-fix evidence was:

    analyzer threads: 120 during parse
    parse tail: 119 sleeping, 1 in has_matching_declaration for about one hour
    main DB: 11,074,551,808 bytes
    WAL: 6,990,086,792 bytes
    freelist: 1,766,113 of 2,703,748 pages (65.3%, about 6.737 GiB)
    completed C++ blobs sampled: 49,233
    logical rows represented: about 18,111,369 before secondary-index mutations
    largest completed blob: about 327,480 logical rows

Issue #771 and commit `8b8f8eca` address only the separate quadratic declaration-identity scan. Issue #772 owns generation-aware epoch visibility and bounded reclamation. Issue #773 owns parallel preparation, streaming, adaptive batching, and failure isolation. Do not use the #771 comparison reduction as proof that this persistence plan is complete.

The accepted #773 validation before its checkpoint is:

    real analyzer overlap latch: pass
    real 257-file analyzer path: 257 committed blobs, 5 transactions
    adaptive row and payload caps: pass
    failed-batch rollback and singleton isolation: pass
    preparation failure: terminal Persist progress and one dirty blob
    legacy/prepared Ruby, Scala, and TypeScript parity: pass
    tree_sitter module with nlp,python: 22 passed
    store module with nlp,python: 29 passed; only the intentional #772 reduction fails
    broad library gate: 802 passed, 3 ignored; three sandbox-blocked process tests pass outside the sandbox
    affected all-feature clippy, formatting, and diff check: pass

The accepted #772 validation before its checkpoint is:

    store generation and persistence tests: 38 passed
    tree_sitter analyzer tests: 24 passed
    populated v3 to v4 analyzer and semantic migration: pass
    A to B to A non-revival and two-connection publisher serialization: pass
    stale register, prepared, legacy, path, dirty, and lazy-index behavior: pass
    same-OID hydration read snapshot consistency: pass
    strict multi-key TS and TSX captured-token completeness: pass
    logical-row reclamation and oversized replacement accounting: pass
    full all-feature library: 814 passed, 3 ignored; four sandbox process failures pass unrestricted
    isolated all-target and all-feature clippy, formatting, and diff check: pass

## Interfaces and Dependencies

Use existing Rust standard-library synchronization and channel facilities unless the repository already depends on a bounded-channel crate that clearly fits. Do not introduce an async runtime. Keep SQLite access in `AnalyzerStore` and use `rusqlite` transactions.

At completion, the private store layer should have an owned prepared representation equivalent to:

    struct PreparedParsedBlob {
        key: ParsedBlobKey,
        generation: GenerationId,
        estimated_rows: usize,
        payload_bytes: usize,
        // owned normalized rows required by the existing schema
    }

The exact row fields should reuse existing stored row types rather than duplicate schema models. Preparation must be callable without holding the store connection mutex.

The writer should accept explicit limits equivalent to:

    struct PersistBatchLimits {
        max_blobs: usize,
        max_rows: usize,
        max_payload_bytes: usize,
    }

It should return statistics and per-blob outcomes equivalent to:

    struct PersistBatchStats {
        transactions: usize,
        committed_blobs: usize,
        logical_rows: usize,
        payload_bytes: usize,
    }

Names may change during implementation, but these responsibilities and observable contracts must remain. Store query APIs must take the current generation into account internally so callers cannot accidentally read stale epochs.

Revision note (2026-07-14): Created this ExecPlan from live RMerl profiling after #771 isolated the earlier parse straggler. The plan deliberately separates generation visibility from the bounded persistence pipeline and makes deterministic scheduling, batching, failure, and reclamation contracts prerequisites for implementation.

Revision note (2026-07-14): Recorded the failing reductions and assigned issue boundaries: #772 for constant-time logical epoch cutover and #773 for the bounded streaming persistence pipeline.

Revision note (2026-07-14): Recorded the reviewed #773 implementation, the rejection of clock-based batching and oversized buffering, deterministic production-path overlap and five-transaction proofs, failure isolation, bounded-flow telemetry, rich persistence parity, and broad validation. #772 and the forced production refresh remain open.

Revision note (2026-07-14): Recorded the reviewed #772 implementation: metadata-only generation migration, atomic monotonic multi-key publication, captured-token snapshots and mutations, terminal stale handling, generation-safe GC, bounded reclamation and replacement accounting, concurrency/migration tests, and full validation. Only the forced production refresh remains.
