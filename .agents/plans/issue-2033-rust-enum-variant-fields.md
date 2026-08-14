# Index and navigate Rust enum struct-variant fields

This ExecPlan is a living document maintained under `.agents/PLANS.md`. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must stay current.

## Purpose / Big Picture

After this change, named fields declared by a Rust enum struct variant, such as `Compound::Map { ser: value }`, are indexed beneath the exact variant and both definition lookup and inverse usage lookup navigate the `ser` initializer label. Fields belonging to two variants with the same spelling remain distinct, ordinary struct fields retain their behavior, and function-local unindexed struct owners remain fail-closed.

## Progress

- [x] (2026-08-13 23:02Z) Read #2033 and traced enum declaration extraction, field-label definition lookup, and targeted inverse member scanning.
- [x] (2026-08-13 23:16Z) Added a pre-fix failing end-to-end InlineTestProject regression covering declarations, forward lookup, targeted inverse lookup, sibling variants, an ordinary struct, a tuple variant, and a local unindexed owner.
- [x] (2026-08-13 23:22Z) Indexed named fields under each enum variant, routed them as member targets, resolved their value-namespace owners in the targeted scan, and bumped the Rust analysis epoch.
- [x] (2026-08-13 23:29Z) Ran the focused issue and neighboring enum tests, all 53 Rust crate tests, focused Clippy, formatting, dependency checks, and all three serde-json exact replays; every check passed and every replay reported one consistent exact inverse hit with zero actionable findings.
- [ ] Commit, push, close #2033, and publish the completed plan evidence.

## Surprises & Discoveries

- Observation: enum variants already use the ordinary field `CodeUnit` kind and are indexed beneath their enum, but `visit_rust_class_like` stops after indexing the variant node and never visits the variant's `field_declaration_list` body.
  Evidence: `crates/bifrost-rust/src/declarations.rs::visit_rust_class_like` dispatches an `enum_variant` to `visit_rust_field`; `visit_rust_field` records that variant but does not descend into its `body` field.

- Observation: forward lookup worked as soon as declarations existed, but targeted inverse required two additional topology corrections. A named variant field's direct parent is the field-like variant rather than the enum class, so `is_member_target` did not route it through field scanning; and the written owner `Compound::Map` occupies Rust's value namespace rather than the type namespace used for ordinary struct owners.
  Evidence: the first post-extraction run resolved all forward labels but returned no inverse hit for `Compound.Map.ser`. Extending member routing through an exact enum-variant parent and comparing the structured value-namespace owner made the test pass.

- Observation: the public whole-workspace `usage_graph` catalog excludes field declarations, so its inverted edge pass is not a consumer of this field target today. The measured differential uses authoritative targeted `UsageFinder` scanning, which the regression covers end to end.
  Evidence: `WorkspaceUsageCatalog::build_with_cancellation` retains classes and callables, not fields, in `crates/bifrost-analysis/src/analyzer/usages/workspace_graph.rs`.

## Decision Log

- Decision: reuse `visit_rust_field` recursively for the named fields in an enum variant body.
  Rationale: the grammar exposes those declarations as ordinary `field_declaration` children. Reusing the existing declaration path preserves signatures, ranges, package anchors, parents, and persistence without creating a second field interpretation.
  Date/Author: 2026-08-13 / Codex

- Decision: parent each named field to its exact variant rather than directly to the enum.
  Rationale: Rust permits the same field spelling on multiple variants. Exact variant ownership produces distinct identities such as `Compound.Map.ser` and `Compound.Other.ser`, which is required for correct navigation and inverse precision.
  Date/Author: 2026-08-13 / Codex

- Decision: keep the variant as a field-like value declaration and teach member routing and structured field-owner matching about the existing `field -> enum_variant -> enum` topology.
  Rationale: changing the variant's CodeUnit kind would alter established enum-constructor behavior. The grammar and declaration graph already carry enough structure to recognize this exact case without weakening ordinary type-owner matching.
  Date/Author: 2026-08-13 / Codex

- Decision: bump the Rust per-language analysis epoch.
  Rationale: persisted blobs created before this change contain enum variants but omit their named field children. Reusing those rows would make warm workspaces disagree with fresh workspaces.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Named enum struct-variant fields are now source-backed declarations under their exact variant identities. Forward lookup and authoritative inverse lookup both resolve the three measured serde-json `Compound::Map.ser` labels, while ordinary struct fields, sibling variant identity, tuple variants, and unindexed local structs retain their prior behavior. The implementation is validated and awaits commit/publication; the final commit, push, and issue closure will be recorded here.

## Context and Orientation

Rust declarations are extracted in `crates/bifrost-rust/src/declarations.rs`. `visit_rust_class_like` handles structs, enums, unions, and traits. For an enum body it sends every `enum_variant` to `visit_rust_field`, which creates a field-like declaration such as `Compound.Map`. Tree-sitter represents a struct-shaped variant's named declarations in the variant's `body` field, a `field_declaration_list` containing ordinary `field_declaration` nodes. Those children are currently skipped, so no `Compound.Map.ser` declaration exists.

Definition lookup classifies a field initializer or pattern label through `crates/bifrost-rust/src/field_roles.rs`. `rust_struct_field_name_outcome` in `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs` resolves the written owner and then looks up `{owner}.{field}`. Targeted inverse scanning in `crates/bifrost-analysis/src/analyzer/usages/rust_graph/extractor.rs` uses the same structured field-role helper. The variant owner needs value-namespace resolution, while ordinary struct owners retain type-namespace resolution. Neither route gains a textual fallback.

## Plan of Work

Add `tests/suite_issues/issue_2033_rust_enum_variant_fields.rs` and register it in the consolidated suite. The fixture declares an ordinary struct and an enum with two struct variants that both declare `ser`. It constructs every owner and also contains a function-local struct control. Assert indexed parent identities, forward navigation for each label, and exact inverse usage ranges for each field without cross-variant leakage. Assert the local control remains unresolved.

Then update `visit_rust_field`. After recording an `enum_variant`, inspect its structured `body`. For `field_declaration_list`, iterate named children and recursively pass only `field_declaration` nodes to `visit_rust_field` with the variant `CodeUnit` as parent. Do not descend into tuple-variant ordered fields because they have no named label to navigate. Keep traversal bounded to one grammar-owned list and preserve test-region taint. Route fields under a proven enum variant through member scanning, and match the written variant owner through the existing structured reference context's value namespace.

Validate the new issue test, existing enum-variant usage tests, all `brokk-bifrost-rust` tests, formatting, focused Clippy, and the dependency checker. Rebuild the release differential runner and replay the three #2033 serde-json records with ephemeral cache. Each must become `consistent` with an exact inverse hit.

## Concrete Steps

From `/mnt/optane/bifrost-fird` run:

    cargo test --test suite_issues -- issue_2033_rust_enum_variant_fields:: --nocapture
    cargo test --test suite_usages -- usages_rust_graph_test::rust_graph_strategy_resolves_enum_variants_as_associated_fields --exact
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs

Use the exact rerun commands from the full Rust ledger for all three occurrence keys owned by #2033, with fresh output paths and `--cache-mode ephemeral`.

## Validation and Acceptance

The new test must fail before extraction changes because `Compound.Map.ser` and `Compound.Other.ser` are absent. After the change, both declarations must exist beneath their exact variants; forward queries on their initializer labels must return the matching FQN; inverse queries must return only that variant's label. `Record.ser` must remain correct, and the function-local `Adapter.writer` label must remain a structured unresolved answer rather than binding to an unrelated field.

All three production rows must report zero actionable findings and exact inverse ranges. Focused Clippy, formatting, dependency boundaries, and the Rust language-crate suite must pass before publication.

## Idempotence and Recovery

All edits and tests are repeatable. The declaration change affects regenerated analyzer facts and requires no schema migration because it emits more ordinary declaration rows. If a persisted-cache test reveals stale data, use the existing Rust analysis epoch mechanism rather than deleting user caches. Do not broaden extraction to local struct declarations or tuple fields as part of this issue.

## Artifacts and Notes

The production shape is:

    enum Compound<'a> {
        Map { ser: &'a mut Serializer, ... },
    }
    Compound::Map { ser: self, ... }

Before the change the `ser` label reports `unresolved_struct_owner` even though `Compound` and `Compound.Map` are indexed.

## Interfaces and Dependencies

No public interface or dependency changes. The emitted declaration graph gains source-backed field `CodeUnit`s parented to enum-variant field `CodeUnit`s. Existing `CodeUnitIndex`, definition lookup, and usage graph APIs consume them unchanged.

Plan revision note (2026-08-13 23:02Z): created after tracing #2033 to the missing declaration-extraction descent and before adding the regression.

Plan revision note (2026-08-13 23:22Z): recorded the pre-fix failure, the forward-only intermediate result, the targeted inverse topology corrections, and the required Rust epoch invalidation.

Plan revision note (2026-08-13 23:29Z): recorded completed focused validation and the three exact serde-json replays at bytes 7744..7747, 7871..7874, and 9629..9632. Each replay resolved `serde_json.ser.Compound.Map.ser`, returned an exact inverse hit, and reported `actionable=0`.
