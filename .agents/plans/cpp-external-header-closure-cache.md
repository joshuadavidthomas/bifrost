# Cache the external C++ header closure once per reference file

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept current. This document follows `.agents/PLANS.md`.

## Purpose / Big Picture

Forward lookup in a C++ source that mentions many unresolved external-looking members should discover its reachable angle-bracket header closure once, not reread and reparse that closure for every member. After this change, all external member and boundary lookups for one reference file in one analyzer generation reuse one immutable closure outcome. MuJoCo's `plugin/usd_decoder/usd_decoder.cc`, which added 668 seconds after every other forward file had completed, should finish without becoming a single-worker tail.

## Progress

- [x] (2026-08-13 09:32Z) Captured the exact #2095 acceptance criteria and inspected the external boundary/member call chain, compile-context resolver, prepared syntax API, and pool-safe memo primitives.
- [x] (2026-08-13 09:39Z) Added a generation-scoped, keyed, pool-safe external-header closure memo to `CppAnalyzer`.
- [x] (2026-08-13 09:41Z) Reused prepared workspace syntax and added cancellation-safe complete/unavailable closure construction.
- [x] (2026-08-13 09:47Z) Added behavior, update, cancellation, missing/conflicting-context, and deterministic work-count coverage.
- [x] (2026-08-13 09:52Z) Passed focused unit, integration, compile, clippy, format, dependency, and diff validation.
- [x] (2026-08-13 10:08Z) Committed and pushed the closure cache as `db1bebb6`, merged at `578ce51f`.
- [x] (2026-08-13 10:37Z) Removed recursive local-binding reconstruction from initializer inference and added a 16-link behavior/cost regression.
- [x] (2026-08-13 10:41Z) Passed six focused resolver unit tests, two existing C++ navigation controls, analysis all-target clippy, format, workspace dependency validation, and diff checks.
- [ ] Commit and push the follow-up, replay pinned MuJoCo, publish evidence, and close #2095.

## Surprises & Discoveries

- Observation: `external_boundary_evidence` and `external_member_resolution` both call the same uncached helper.
  Evidence: `crates/bifrost-analysis/src/analyzer/cpp/external.rs` calls `directly_reached_external_headers` before inspecting the semantic overlay, and that helper reads the workspace file, parses it with a fresh tree-sitter parser, then parses every reached external header.

- Observation: The workspace source already has an immutable prepared syntax snapshot paired with its exact source text.
  Evidence: `CppAnalyzer::prepared_syntax` returns `PreparedSyntaxTree`, whose `source()` and `tree()` are guaranteed to share one snapshot. The existing language helper reparses only because it accepts a string instead of an existing root node.

- Observation: A keyed pool-safe cell is necessary even though the build itself is serial.
  Evidence: forward files are processed by rayon workers. Same-file member queries can race on a cold closure. `PoolSafeMemo::get_or_try_build_pool_independent_while` collapses that race without a worker convoy and declines to publish an interrupted build.

- Observation: The exact replay falsified the assumption that repeated external-header closure construction was the only MuJoCo tail.
  Evidence: at pushed commit `578ce51f`, files 1-313 completed in 52.3 seconds, but file 314 was still running after eleven minutes. A live GDB stack showed one active worker repeatedly cycling through `cpp_seed_active_path`, `cpp_seed_variable_declaration`, `cpp_infer_type_from_value`, and `cpp_field_receiver_type_units`; the seven peer workers were parked. The external-header closure builder was absent from the stack.

- Observation: initializer inference discards the local-binding engine it is currently building.
  Evidence: `cpp_seed_binding` owns the already-seeded `LocalInferenceEngine<CppType>`, but `cpp_infer_type_from_value` resolves a field-call receiver through `cpp_field_receiver_type_units`, whose identifier path calls `cpp_bindings_before` from the root. A chain of inferred local initializers therefore rebuilds every prior binding recursively and can grow exponentially.

## Decision Log

- Decision: Key the memo by `ProjectFile` inside `CppAnalyzer`.
  Rationale: the analyzer instance defines the generation and owns one immutable compile-context set. Rebuilding `CppAnalyzer` on update or project replacement discards both contexts and closure cells, so the remaining varying input is the exact reference file.
  Date/Author: 2026-08-13 / Codex

- Decision: Cache a typed `Complete(headers)` or `Unavailable` outcome, but never cache cancellation.
  Rationale: missing/conflicting compile context, containment failure, read failure, and header/byte limits are stable complete answers for this analyzer generation and should not be recomputed. Cancellation is request-local incomplete work and must publish nothing.
  Date/Author: 2026-08-13 / Codex

- Decision: Add a tree-root variant of structured angle-include extraction in `brokk-bifrost-cpp` and keep the string API as a wrapper.
  Rationale: the language crate owns the AST interpretation. This avoids reparsing prepared workspace source without moving grammar-dependent code into analysis or introducing text scanning.
  Date/Author: 2026-08-13 / Codex

- Decision: Thread the already-seeded local-binding engine through initializer call-return inference instead of adding a depth cutoff.
  Rationale: the current engine is the exact structured state visible before the declaration being seeded. Rebuilding it is both redundant and the source of the tail; an arbitrary recursion limit would merely turn a provable receiver into an unknown result. Callers without an active seeding pass retain the ordinary root-based lookup.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The external-header closure implementation and its replay-discovered local-inference follow-up are complete locally. External member and boundary lookups share one typed closure outcome per reference file and analyzer generation. Initializer return-type inference now reuses the exact source-ordered binding engine already being constructed, so a chain of inferred receivers performs one binding build instead of recursively rebuilding each prefix. The old pushed binary completed MuJoCo file 314 at 733.0 seconds, a 680.7-second tail, which proves the cache alone did not meet acceptance. Commit, push, and a replacement replay remain.

## Context and Orientation

`crates/bifrost-analysis/src/analyzer/cpp/external.rs` determines whether an external semantic-pack symbol is reachable from a C++ reference. `directly_reached_external_headers` starts from literal, unconditional angle includes in the reference file, resolves them through the file's compile context, then follows literal angle includes in external headers under a 10,000-header and 32 MiB bound. It currently performs that work per lookup.

`crates/bifrost-analysis/src/analyzer/cpp/mod.rs` owns analyzer-generation caches and compile contexts. Every constructor and `with_updated_inner` creates fresh cache state. `KeyedPoolSafeMemo<ProjectFile, V>` supplies one single-flight cell per reference file. `PoolSafeMemo::get_or_try_build_pool_independent_while` is valid because the closure builder performs serial file reads and tree-sitter parses and never enters rayon.

`crates/bifrost-cpp/src/external_declarations.rs` owns the structured interpretation of external header syntax. `external_angle_include_paths` currently creates a parser from source text. It can delegate to a new function that accepts the already paired source and tree root; external files continue to use the string wrapper, while the workspace seed uses prepared syntax.

## Plan of Work

Introduce `ReachedExternalHeaders` next to the existing reached-header record. It represents `Complete(Vec<ReachedExternalHeader>)` or `Unavailable`. Add `external_header_closures: Arc<KeyedPoolSafeMemo<ProjectFile, ReachedExternalHeaders>>` to `CppAnalyzer`, initialize it in every construction/update path, and add test-support build and external-parse counters.

Refactor `directly_reached_external_headers` into a cache accessor and a serial builder. The accessor obtains the active query cancellation token, derives one `keep_going` predicate, and asks the file cell to build pool-independently while that predicate remains true. The builder checks the predicate before and during all include work. It seeds includes from `CppAnalyzer::prepared_syntax(file)`, not a disk reread or reparse. It resolves includes with the existing compile-context API, preserves containment and both existing bounds, sorts/deduplicates the final records, and returns `Unavailable` for stable incomplete evidence. Cancellation returns no value to the memo and therefore publishes nothing.

In the language crate, factor AST walking into `external_angle_include_paths_from_root(source, root)`. Preserve the exact unconditional literal `system_lib_string` policy and deterministic sort/dedup. Keep `external_angle_include_paths(source)` as the parse-and-delegate API used for external header text and pack discovery.

Extend the existing external semantic-pack tests with a small source and fake include tree. Repeated boundary/member calls must report identical results while counters show one closure build and one parse per external header, not one per queried member. Add direct/transitive, conditional, missing/conflicting compile context, and limit/containment controls where existing fixtures already expose them. Add a custom-`keep_going` test that stops a cold build after it starts, then retries and proves a second complete build occurs; the interrupted value must not be published. Update the project source or analyzer and prove a new analyzer rebuilds the closure.

For the replay-discovered follow-up in `crates/bifrost-analysis/src/analyzer/usages/get_definition/cpp.rs`, allow call-return inference invoked from `cpp_seed_binding` to borrow the `LocalInferenceEngine<CppType>` that has already been seeded in source order. When a field-call receiver is an identifier, resolve it from that engine and preserve its shadow verdict; only callers outside an active binding pass build bindings from the root. Add a behavior regression with a long chain of `auto next = previous.member()` declarations. It must resolve the final member while a test-only inference counter proves work grows linearly rather than recursively rebuilding prefixes. Preserve field/member, parenthesized receiver, shadowing, and unknown-receiver controls.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`. Edit only with `apply_patch`, then run:

    cargo fmt --all
    cargo test -p brokk-bifrost-cpp external_angle_include_paths --lib
    cargo test -p brokk-bifrost-analysis analyzer::cpp::external::tests --lib
    cargo test -p brokk-bifrost-analysis analyzer::usages::get_definition::cpp::bounded_tests::initializer_inference_reuses_seeded_bindings --lib
    cargo check -p brokk-bifrost-cpp --all-targets
    cargo check -p brokk-bifrost-analysis --all-targets
    cargo clippy -p brokk-bifrost-core -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

After the focused gate, commit and push on the current branch. Rebuild `bifrost_reference_differential` in release mode and replay the exact pinned MuJoCo repository with the original census limits and a clean ephemeral clone. Record forward progress through all 314 files, total elapsed time, report cleanliness, finding counts, and hashes. Comment that evidence on #2095 and close it only after the replay completes.

## Validation and Acceptance

Behavior must remain exact for direct and transitive external headers, conditional include exclusion, missing/conflicting compile context, dependency/package identity, semantic-overlay filtering, and the existing header/byte bounds. A project/analyzer update must discard old closure state. An interrupted build must leave the cell empty and a later uncancelled query must construct the full answer.

The deterministic cost test must issue many member or boundary queries for one source and observe exactly one closure build. The prepared workspace source must be walked without a new parser. External headers must be parsed at most once in the closure build.

The initializer-inference regression must navigate through a chain of typed call-return bindings and prove that each declaration prefix is seeded once for the queried site. A local shadow or an unknown receiver must continue to fail closed; the optimization must not bypass source-order visibility.

The corpus acceptance is pinned MuJoCo steady forward completion through `plugin/usd_decoder/usd_decoder.cc` (file 314/314). Compare its post-fix time with the pre-fix 668.3-second single-file tail, require `status=completed`, `bifrost_dirty=false`, `repo_dirty=false`, and zero file errors.

## Idempotence and Recovery

All cache state is analyzer-generation local and non-persistent. Repeating a query returns the immutable published outcome. Repeating an interrupted query either joins a complete leader or retries after the incomplete leader publishes nothing. If tests or the corpus replay are interrupted, rerun them normally. Preserve old report artifacts and write the fixed replay to a new revision-specific supplement.

## Artifacts and Notes

Pre-fix MuJoCo evidence at Bifrost `3691bb01`:

    files 1-313: 45.0 seconds
    file 314 plugin/usd_decoder/usd_decoder.cc: completed at 713.3 seconds
    single-file tail: 668.3 seconds

Plan revision note (2026-08-13): Created from issue #2095 and the durable rank-31+ C++ replay after #2097 was fixed. The plan records the structured cache key, completion/cancellation contract, prepared-syntax seam, required parity controls, and exact MuJoCo acceptance replay.

Plan revision note (2026-08-13): Recorded the pushed cache implementation and the exact replay's independent recursive local-inference tail. The plan now includes the structured reuse fix and its behavior/cost acceptance rather than incorrectly treating cache publication as issue completion.
