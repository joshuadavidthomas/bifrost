# Restore Rust inverse hits for imported aliases and grouped import names

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Forward Rust lookup resolves imported nominal types and indexed type aliases, but inverse usage lookup omits some grouped import-name occurrences and cross-Cargo-package alias references. After this change, those structured sites round-trip to the exact forward identity, including mutually exclusive cfg alternatives, without admitting unrelated same-name declarations.

## Progress

- [x] (2026-08-14) Preserved eleven exact production residuals and filed/assigned #2130.
- [x] (2026-08-14) Added grouped-import, cross-package, cfg-alternative, and custom Cargo bench-target alias regressions.
- [x] (2026-08-14) Traced candidate discovery and AST authorization independently against Sway, Meilisearch, and Diesel.
- [x] (2026-08-14) Corrected scoped-path fallback, exact binder-target admission, and Cargo dependency reachability; focused tests and production representatives pass.
- [ ] Publish, replay all eleven production rows on a clean release runner, and close #2130.

## Surprises & Discoveries

- Observation: The alias declarations are indexed as field-kind CodeUnits while retaining `RustAnalyzer::is_type_alias` identity.
  Evidence: Meilisearch `InstanceUid`, `FieldId`, Diesel `Filter`, and `Bencher` all forward-resolve to field-kind targets with the expected alias FQNs.
- Observation: #2128 already proves same-crate alias references through grouped raw-module re-exports.
  Evidence: `issue_2128_rust_reexported_type_inverse` passes for `Option<Alias>`; the remaining aliases cross Cargo package boundaries or have cfg-alternative declarations.
- Observation: The nested Sway grouped import has no direct namespace binder for its first path segment because a module-level glob introduces that segment.
  Evidence: The forward reference context resolves `ast_elements::params::GenericTypeParameter`; the inverse module and explicit-binder routes were empty before the scoped forward-context fallback.
- Observation: Meilisearch's visible named binder resolves exactly to `meilisearch-types/src/lib.rs:InstanceUid`, but its auxiliary dependency-root list is empty. Diesel's custom `benches/lib.rs` target similarly resolves `super::Bencher` exactly while its inferred child module key does not nest beneath the alias domain.
  Evidence: Targeted traces showed exact scoped binder targets in both repositories and no inverse match until exact query roots were allowed to trust those binder targets.
- Observation: Cargo target relation intentionally reports dependency packages as disjoint because it measures shared compilation targets, not dependency reachability.
  Evidence: Diesel `Filter` resolves through dependency globs, and the existing `files_by_reachable_root` index includes the consumer although `target_relation` is not `Shared`.

## Decision Log

- Decision: Reduce grouped import occurrence handling separately from cross-package alias propagation.
  Rationale: One is an AST-site recording question and the other is a seed/candidate-route question. A shared issue may own both production groups, but the implementation must not presume one mechanism.
  Date/Author: 2026-08-14, Codex.
- Decision: Use the forward reference context only after authoritative binders and physical module routes fail for multi-segment paths, and require the result to canonicalize to this query's unique seed.
  Rationale: This recovers glob-introduced namespace prefixes without weakening explicit-import precedence or admitting name-only guesses.
  Date/Author: 2026-08-14, Codex.
- Decision: Treat an exact query root returned by a visible structured binder as authoritative; retain Cargo admission and inferred-domain checks for propagated aliases.
  Rationale: The binder already proves both identity and lexical visibility. Auxiliary route/domain models can be incomplete for inherited workspace metadata and custom Cargo target layouts, while propagated identities still need those guards.
  Date/Author: 2026-08-14, Codex.
- Decision: Ask the existing reachable-root index whether a file can reference a target rather than treating shared-target relation as dependency reachability.
  Rationale: This preserves independent target exclusions and avoids allocating the full candidate-file vector in the per-reference hot path.
  Date/Author: 2026-08-14, Codex.

## Outcomes & Retrospective

Dirty-tree diagnostic release replays at the intended implementation returned actionable zero for Sway `GenericTypeParameter`, Sway `GenericTypeArgument`, Meilisearch `InstanceUid`, Meilisearch `FieldId`, Diesel `Filter`, and all six issue-owned Diesel `Bencher` references. These are diagnostic only; closure requires the clean pushed-head replay of all eleven owned rows.

## Context and Orientation

Rust inverse candidate routing is coordinated by `crates/bifrost-analysis/src/analyzer/usages/rust_graph.rs`, with AST matching in `rust_graph/extractor.rs`. Shared identity propagation and Cargo import walks live in `crates/bifrost-rust/src/usage.rs` and `usage_walks.rs`; `graph/inverted.rs` materializes file-level inverse edges.

Clean representative baselines at Bifrost `3643963d05adac938932760cb4a745050f4710e0` are `/mnt/optane/tmp/bifrost-fird/final-3643963/exact/generic-type-parameter-import.jsonl`, `instance-uid.jsonl`, `filter-alias.jsonl`, and `bencher.jsonl`.

## Plan of Work

Add an `InlineTestProject` fixture for a nested grouped import whose leaf type has same-name decoys. Add a small Cargo workspace with a provider crate exporting a normal alias and mutually exclusive cfg aliases, and a consumer crate importing and referencing them. Resolve each site forward, then query inverse usages for the returned target and require the exact site.

Trace failures through candidate-file discovery before changing AST matching. Use Cargo routes, import binders, alias seed identities, and tree-sitter nodes only. Preserve namespace, visibility, cfg, lexical-shadowing, and independent-target controls.

## Concrete Steps

Run from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_issues -- issue_2130_rust_imported_alias_inverse
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

## Validation and Acceptance

Reduced fixtures must recover exact grouped import-name, ordinary alias, and cfg-alternative alias occurrences while excluding same-name decoys. Every one of the eleven issue-owned production rows must replay as `consistent` or intentional import `editor_only`, with an exact hit and actionable zero on a clean pushed head.

## Idempotence and Recovery

Tests and ephemeral exact replays are safe to repeat. Preserve baseline reports and write head-scoped closure evidence under `/mnt/optane/tmp/bifrost-fird/`. Commit only #2130 files and push directly to `origin/master`.

## Interfaces and Dependencies

No public API or dependency is expected. Bump the Rust analysis epoch only if persisted identity or route facts change.
