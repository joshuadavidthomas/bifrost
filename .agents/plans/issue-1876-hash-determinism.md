# Add a total C++ lookup order and randomized hash check

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current. Maintain this plan according to `.agents/PLANS.md`.

## Purpose / Big Picture

C++ lookup must not change when a hash table uses a different iteration order. One local sorter now leaves ties between distinct `CodeUnit` values. The normal build also uses a fixed fast hash seed, so CI cannot expose a new unsorted selection path.

After this change, the C++ lookup sorter gives every distinct unit a stable order. A `randomized-hash` check mode uses standard randomly seeded hash tables. The scheduled benchmark workflow runs the existing C++ determinism reference test in that mode.

## Progress

- [x] (2026-08-12) Confirmed `sort_lookup_units` omits kind, package boundary, synthetic state, and structured segment kind.
- [x] (2026-08-12) Added a total stable comparator and a unit regression.
- [x] (2026-08-12) Added the randomized hash feature through core, analysis, and the facade.
- [x] (2026-08-12) Added the scheduled C++ reference test in randomized mode.
- [x] (2026-08-12) Ran focused tests, formatting, and targeted Clippy.
- [x] (2026-08-12) Committed as `810f4b43`, integrated origin/master, and pushed the fix.
- [ ] Close #1876 after the final plan update reaches origin/master.

## Surprises & Discoveries

- Observation: `CodeUnit::cmp` is not a complete fallback for this sorter.
  Evidence: It compares rendered package and short names. Two structured `FqName` values can render the same text while their segment kinds differ.

- Observation: The existing #1836 regression already acts as a small reference corpus.
  Evidence: `tests/suite_issues/issue_1836_cpp_resolution_determinism.rs` runs ten content-irrelevant workspace variants and requires one stable definition path.

## Decision Log

- Decision: Keep the current rendered-name, signature, and source order as the primary order. Add all missing identity fields as tie breakers.
  Rationale: This preserves existing answer preference while making ties impossible for unequal units.
  Date/Author: 2026-08-12, Codex.

- Decision: Compare structured name segments by text and an explicit stable kind rank.
  Rationale: Process-local `SegmentId` values depend on insertion order and cannot be a deterministic sort key.
  Date/Author: 2026-08-12, Codex.

- Decision: Add `randomized-hash` as a non-default Cargo feature.
  Rationale: Production keeps the current fast hasher. CI can opt into standard random seeds without a source edit.
  Date/Author: 2026-08-12, Codex.

## Outcomes & Retrospective

The implementation is complete in commit `810f4b43`. The total-order unit test passes. The ten-variant #1836 reference corpus passes with randomly seeded hash tables. Formatting and targeted Clippy pass. The daily benchmark workflow now runs this corpus with random hash seeds. The commit is on origin/master.

## Context and Orientation

`crates/bifrost-cpp/src/graph/resolver.rs` builds visible lookup families from hash maps. `sort_lookup_units` sorts each family before a caller can select one member. Its current key is rendered fully qualified name, signature, and source path. A `CodeUnit` identity also contains declaration kind, structured qualified-name segments, package segment count, and synthetic state.

`crates/bifrost-core/src/hash.rs` defines the hash map and set aliases used across Bifrost. They normally use `FxBuildHasher`. Cargo feature unification means one core feature changes those aliases for every workspace client in the test process.

`.github/workflows/benchmark.yml` runs on a daily schedule. It is the correct place for a periodic randomized check that is too costly and redundant for each pull request.

## Plan of Work

In `crates/bifrost-cpp/src/graph/resolver.rs`, extend `sort_lookup_units`. Preserve its first three keys. Then compare `CodeUnitType`, package segment count, synthetic state, and each structured qualified-name segment. Resolve a segment through the global interner. Compare its text and a stable explicit rank for `SegmentKind`.

Add a unit test in the same file. Create tied units that differ only by kind, package boundary, or synthetic state. Sort several input orders and require one strictly ordered output.

In `crates/bifrost-core/src/hash.rs`, use `RandomState` aliases under `randomized-hash`. Keep Fx aliases otherwise. Declare the feature in core. Propagate it through analysis and the facade, so the command is simple.

Add a `hash-determinism` job to `.github/workflows/benchmark.yml`. Run only the focused #1836 suite test with `--features randomized-hash`.

## Concrete Steps

Run these commands from `/mnt/optane/bifrost-fird`:

    cargo test -p brokk-bifrost-cpp sort_lookup_units --lib -- --nocapture
    cargo test --features randomized-hash --test suite_issues cpp_mirrored_specialization_resolves_to_one_declaring_header -- --nocapture
    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-core --all-targets --features randomized-hash -- -D warnings
    cargo clippy -p brokk-bifrost-cpp --all-targets --features brokk-bifrost-core/randomized-hash -- -D warnings
    cargo clippy --test suite_issues --features randomized-hash -- -D warnings
    git diff --check

The comparator test and the #1836 reference test must pass.

## Validation and Acceptance

The comparator must return a non-equal order for every unequal fixture unit. Reversing or rotating the input must produce the same output. The #1836 reference must resolve `Holder<T, int>.by_int` to `alpha/holder.h` under random hash seeds.

The normal build must still use Fx hashing. The randomized mode must require an explicit feature. Formatting and targeted Clippy must pass.

## Idempotence and Recovery

The tests use temporary projects and leave no files behind. Cargo commands can run again. The feature changes no cache or persisted data. Stage only the plan, manifests, hash alias, C++ resolver, and workflow.

## Artifacts and Notes

The #1836 test is the checked-in reference corpus for this mode. It includes mirrored specializations, unrelated comments, extra files, reversed declaration order, and repeated baseline runs.

## Interfaces and Dependencies

Add one internal Cargo feature named `randomized-hash`. Do not add a new dependency. Use `std::collections::hash_map::RandomState` through the standard two-parameter `HashMap` and `HashSet` aliases.

Do not compare process-local `SegmentId` numbers. Resolve each segment and compare stable text plus stable kind rank.

Plan revision, 2026-08-12: Created after the #1876 audit confirmed a local comparator gap and an existing scheduled workflow target.
