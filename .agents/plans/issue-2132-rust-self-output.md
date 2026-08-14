# Resolve Rust Self associated types to the enclosing impl item

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Inside a Rust impl method, `Self::Output` names the associated type implemented by that same impl. Forward lookup must select the exact physical `type Output` item in the enclosing impl and inverse lookup must round-trip the reference, even when unrelated traits declare the same associated-type name.

## Progress

- [x] (2026-08-14) Preserved the Diesel production witness and filed/assigned #2132.
- [x] (2026-08-14) Confirmed the witness still resolves to unrelated `InternalJoinDsl.Output` on clean master after the upstream impl declaration-navigation fix.
- [x] (2026-08-14) Added a reduced forward/inverse regression with three competing `Alias<S>::Output` impl items and an unrelated trait implementation.
- [x] (2026-08-14) Resolved the exact enclosing impl item structurally, prevented Cargo-scope FQN expansion from widening lexical `Self` results, and completed focused/broad validation.
- [ ] Publish, replay the production witness on a clean release runner, and close #2132.

## Surprises & Discoveries

- Observation: Commit `4340c93a2f7e4762ae1b03287b2f833ca7853711` fixes navigation from an impl associated-type declaration but not a `Self::Output` reference inside an impl method.
  Evidence: Diesel bytes 6685..6691 still resolve to `diesel.query_dsl.join_dsl.InternalJoinDsl.Output` and replay actionable=1 at clean Bifrost `ccd269e94`.
- Observation: `rust_self_scoped_associated_type_candidates` first performs a name-based enclosing-scope lookup and only afterward checks whether that candidate's range lies inside the impl.
  Evidence: When the name lookup selects an unrelated same-name trait item, the range guard discards it and never asks the impl syntax for its own `type Output` item.
- Observation: Selecting the exact impl item was necessary but not sufficient in Diesel because the Cargo target filter expanded lexical `Self` results by FQN.
  Evidence: The first dirty replay selected the correct `LimitDsl` item and then appended `AppendSelection` and `InternalJoinDsl` items from `joins.rs`, all named `diesel.query_source.aliasing.Alias.Output`. After lexical `Self` expansion was disabled, the replay returned only the `LimitDsl` declaration and remained exact, consistent, complete, and actionable zero.

## Decision Log

- Decision: Resolve an explicitly declared associated type from the enclosing impl's tree-sitter body before using the generic scope walk.
  Rationale: The impl syntax is the authoritative ownership boundary for `Self`; matching the exact type-item node to its indexed CodeUnit avoids workspace name guessing and preserves existing fallback behavior for impls without an explicit item.
  Date/Author: 2026-08-14, Codex.
- Decision: Treat a lexical `Self` forward result as exact during Cargo target scoping instead of re-expanding it by analyzer FQN.
  Rationale: Rust permits many impl associated items to share the same analyzer FQN. The resolver has already selected the physical enclosing impl and its signature; target membership should validate that identity, not widen it to unrelated impls.
  Date/Author: 2026-08-14, Codex.

## Outcomes & Retrospective

The dirty production replay at Bifrost `ccd269e94` selected exactly one target, `impl LimitDsl for Alias<S>::type Output`, and returned a consistent exact inverse hit with no truncation, incompleteness, or actionable finding. Clean-head publication evidence remains pending.

## Context and Orientation

Forward Rust definition lookup lives in `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs`. The failing helper is `rust_self_scoped_associated_type_candidates`. The shared exact-node bridge `rust_associated_type_declaration_for_exact_node` lives in `crates/bifrost-rust/src/graph_support.rs` and is already used for navigation from impl associated-type declarations.

The production witness is Diesel `diesel/src/query_source/aliasing/dsl_impls.rs` bytes 6685..6691 in `fn limit(self, limit: i64) -> Self::Output`. Baseline forward lookup selects `diesel.query_dsl.join_dsl.InternalJoinDsl.Output`, which is not the enclosing impl's `type Output` item.

## Plan of Work

Add an `InlineTestProject` fixture with two traits declaring `Output`, a generic `Alias<S>` impl that declares its own `Output`, and an impl method returning `Self::Output`. Require forward lookup to select `Alias.Output`, verify its declaration range belongs to the enclosing impl item, then require an exact authoritative inverse hit.

In the resolver, locate the enclosing `impl_item`, inspect only its direct body items for a matching `type_item` or `associated_type`, and translate the exact syntax node through the existing graph-support helper. Retain the current scope lookup only when the impl has no exact declared item.

## Concrete Steps

Run from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_issues -- issue_2132_rust_self_output
    cargo test --test suite_symbols -- rust_self_associated_type_preserves_exact_same_file_impl_owner
    cargo test --test suite_symbols -- rust_associated_type_navigation_distinguishes_contract_and_implementation
    cargo test --test suite_symbols -- rust_rootless_associated_type_navigation_uses_trait_contract
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

## Validation and Acceptance

The reduced `Self::Output` reference must resolve to the enclosing generic impl's exact associated-type CodeUnit, reject unrelated trait items, and appear as an exact inverse hit. The Diesel witness must replay `consistent`, exact, complete, and actionable zero on a clean pushed head.

## Idempotence and Recovery

Focused tests and ephemeral exact replays are safe to repeat. Preserve clean closure evidence under a new head-scoped directory in `/mnt/optane/tmp/bifrost-fird/`.

## Interfaces and Dependencies

No public API, dependency, or analysis epoch change is expected. This is resolver-only and reuses an existing structured exact-node helper.
