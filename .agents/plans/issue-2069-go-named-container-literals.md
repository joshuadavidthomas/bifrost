# Resolve Go elided literals through named container types

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

After this change, navigation and usage lookup for a field inside an elided Go struct literal will retain the field owner even when the outer array, slice, or map has a declared name. For example, `type JumpTable [256]*operation` followed by `JumpTable{ADD: {execute: opAdd}}` will resolve `execute` to `operation.execute`. The exact rank-31+ witness in issue #2069 and inline behavior tests will demonstrate the result.

## Progress

- [x] (2026-08-13 19:40Z) Reproduced the exact `JumpTable` witness on current master and traced both forward and targeted inverse failures.
- [x] (2026-08-13 19:50Z) Chose a structured declaration-fact design that preserves the named container's underlying type without parsing rendered signatures.
- [x] (2026-08-13 20:25Z) Published the underlying structured type identity with Go type declarations, bumped the Go analysis epoch, and exposed container-path syntax from the shared Go AST module.
- [x] (2026-08-13 20:37Z) Consumed the fact in forward definition lookup and the targeted Go usage index without adding a text fallback.
- [x] (2026-08-13 20:45Z) Added and passed direct, nested, pointer-element, map-key/value, decoy-owner, and unresolved-owner behavior tests; #2070/#2071 controls also pass.
- [x] (2026-08-14 01:35Z) Passed focused Go definition/usage tests, the core structured-type test, formatting, dependency validation, diff hygiene, and clippy for core, Go, and analysis.
- [x] (2026-08-14 01:42Z) Rebuilt the release differential runner and replayed the canonical family plus every baseline `go_literal_owner_unresolved` site: 23 sites are now consistent and the other 61 remain fail-closed under separate owner gaps.
- [x] (2026-08-14 01:48Z) Created the detailed checkpoint commit for the validated implementation.
- [ ] Push, comment, and close #2069.

## Surprises & Discoveries

- Observation: the existing direct-container helper intentionally returns no owner for `JumpTable{ADD: {execute: ...}}` because the syntax only says `JumpTable`; it cannot peel the array element until the declaration's underlying `[256]*operation` fact is available.
  Evidence: exact occurrence `fa4c8623fb905339` still reports `go_literal_owner_unresolved` on current master.

- Observation: forward lookup already carries a richer path that distinguishes container element/value, map key, and nested keyed-field steps, while the targeted inverse helper currently collapses the path to one optional syntax node.
  Evidence: `GoCompositeOwnerStep` in `crates/bifrost-analysis/src/analyzer/usages/get_definition/go.rs` has three variants; `composite_literal_owner_type_for_key` in `crates/bifrost-go/src/graph/ast.rs` returns only `Option<Node>`.

- Observation: a named map's elided value reaches forward lookup through the richer `KeyedValue` step rather than the simpler container-element step.
  Evidence: the first complete fixture run resolved arrays and slices but left `NamedMap{"item": {Field: ...}}` unresolved; teaching the keyed-value branch to load the same underlying identity made the complete fixture pass.

- Observation: `SignatureMetadata` is stored as a bincode blob, so adding even a defaulted optional field changes the persisted wire shape for every language.
  Evidence: `serialize_signature_metadata_blob` and `deserialize_signature_metadata_blob` in `crates/bifrost-analysis/src/analyzer/store/mod.rs` use bincode; the store epoch salt therefore moved from v9 to v10 rather than relying only on a Go salt.

- Observation: the issue's stated 17-site family understates the canonical ledger. The filed `JumpTable` mechanism has 16 exact go-ethereum rows, and the same root fix clears seven additional named-container rows in Hugo and Inspektor Gadget.
  Evidence: exact ephemeral replay of all 84 baseline sites carrying `go_literal_owner_unresolved` produced 23 consistent results and 61 still-actionable results. The 23 consist of 16 `JumpTable` fields, one Hugo named-map value, and six Inspektor Gadget named-map or named-container values.

## Decision Log

- Decision: add a generic `underlying_type_identity` field to `SignatureMetadata`, populated by Go type declarations when tree-sitter provides a supported structured type.
  Rationale: the declaration fact must survive persisted analyzer startup and bounded lookup. Reusing `return_type_identity` would falsely turn a named container's element into an embedded supertype and corrupt Go method promotion.
  Date/Author: 2026-08-13 / Codex

- Decision: retain the complete structured identity in the Go edge index and apply the full container-step path at the reference site.
  Rationale: storing only an immediate element FQN loses anonymous nested arrays and maps. A flat identity can peel several structured container layers before resolving the final nominal owner.
  Date/Author: 2026-08-13 / Codex

- Decision: do not add source-text or terminal-name fallback.
  Rationale: a same-named field on an unrelated struct is not evidence. The exact owner must come from the literal AST, the named type declaration, and the indexed member relation.
  Date/Author: 2026-08-13 / Codex

- Decision: bump the global analyzer blob-store epoch as well as the Go semantic salt.
  Rationale: old bincode payloads cannot be safely decoded as the enlarged `SignatureMetadata` struct. A Go-only generation turnover would leave other languages able to read shifted metadata fields.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation preserves named Go container shape as structured declaration metadata and consumes it in both forward definition lookup and targeted inverse usage lookup. The behavior fixture covers array, nested array, slice, map value, map key, unrelated same-named fields, and unknown external owners.

All 16 exact go-ethereum `JumpTable` ledger rows now report one resolved forward target, an exact inverse hit, `classification=consistent`, and zero actionable findings. A broader replay of all 84 baseline `go_literal_owner_unresolved` sites found seven additional consistent named-container sites (one Hugo and six Inspektor Gadget), for 23 repaired sites total. The remaining 61 still report missing owners and therefore demonstrate that the change does not use a field-name fallback.

Focused validation passed:

    cargo test --test suite_issues -- issue_2069_go_named_container_literals:: --nocapture
    cargo test --test suite_issues -- issue_2070_go_nested_map_keys:: issue_2071_go_elided_map_key_owner::
    cargo test --test suite_symbols -- get_definition_test::go_
    cargo test --test suite_usages -- usages_go_graph_test::
    cargo test -p brokk-bifrost-core structured_type_identity_tests::flat_identity_operations_are_stack_safe_on_deep_types
    cargo fmt --all -- --check
    node scripts/check-workspace-dependencies.mjs
    cargo clippy -p brokk-bifrost-core -p brokk-bifrost-go -p brokk-bifrost-analysis --all-targets -- -D warnings
    git diff --check

## Context and Orientation

Go uses `keyed_element` syntax for both struct field labels and map or array keys. `crates/bifrost-go/src/graph/ast.rs` interprets that syntax for both usage scanners. `crates/bifrost-analysis/src/analyzer/usages/get_definition/go.rs` performs forward navigation and has access to persisted `CodeUnit` and `SignatureMetadata` rows. `crates/bifrost-go/src/graph/resolver.rs` builds `GoEdgeIndex`, a compact tree-free set of facts used by targeted and whole-workspace usage scans. `crates/bifrost-go/src/graph/extractor.rs` is the targeted scanner used by the reference differential.

A structured type identity is a bounded, flat representation of a type such as `[256]*operation` or `map[Key]Value`. It lives in `crates/bifrost-core/src/analyzer/model.rs` and can select an array/slice element, a map value, or a map key without reparsing source text.

## Plan of Work

Extend `SignatureMetadata` in `crates/bifrost-core/src/analyzer/model.rs` with an optional structured underlying-type identity, defaulted for backward-compatible deserialization, plus builder and accessor methods. In `crates/bifrost-go/src/declarations.rs`, attach that fact to each `type_spec` whose type node can be represented structurally. Keep embedded-field metadata separate.

In `crates/bifrost-go/src/graph/ast.rs`, expose the ordered array/slice/map steps between an elided literal field and its explicit outer type. Preserve the existing helper as a convenience for callers that only need direct syntax, but let the targeted scanner retain a named root plus the unapplied steps.

In `crates/bifrost-analysis/src/analyzer/usages/get_definition/go.rs`, when a container step reaches a named syntax type, resolve its exact type declaration, select the unique underlying identity from metadata, and continue peeling the step. Ambiguous declarations or identities return no owner.

In `crates/bifrost-go/src/graph/resolver.rs`, collect the same underlying identities from parsed type declarations into `GoEdgeIndex`, associated with declaration file and package context. Add a query that resolves a use-site type reference and applies an ordered container-step path before returning nominal owner FQNs. Use that query in `crates/bifrost-go/src/graph/extractor.rs` when a field label's owner path crosses a named container.

Add `tests/suite_issues/issue_2069_go_named_container_literals.rs` with `InlineTestProject` and register it in `tests/suite_issues/main.rs`. The fixture must cover a named array of pointers, nested containers, named map values, and a named map whose key is a struct. It must also prove that an unresolved external named container and an unrelated same-named field do not resolve. Assert both forward definitions and targeted inverse hit ranges.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`.

Run the focused behavior test:

    cargo test --test suite_issues -- issue_2069_go_named_container_literals:: --nocapture

Run existing structured literal controls:

    cargo test --test suite_issues -- issue_2070_go_nested_map_keys:: issue_2071_go_elided_map_key_owner::

Run Go definition and usage coverage, formatting, dependency validation, and clippy:

    cargo test --test suite_symbols -- get_definition_test::go_
    cargo test --test suite_usages -- usages_go_graph_test::
    cargo fmt --all -- --check
    node scripts/check-workspace-dependencies.mjs
    cargo clippy -p brokk-bifrost-go -p brokk-bifrost-analysis --all-targets -- -D warnings

Rebuild `bifrost_reference_differential` and replay occurrence `fa4c8623fb905339` with the ledger command, changing only the output path. Expect one forward target `core/vm.operation.maxStack`, an exact inverse hit, and zero actionable findings. Replay the remaining filed keys once their exact set is identified from the ledger.

## Validation and Acceptance

The new integration test must fail before the change with `go_literal_owner_unresolved` and pass afterward. Direct struct literals and the #2070/#2071 map-key distinctions must remain green. An unresolved named container must not resolve by matching a field spelling. The exact go-ethereum occurrence must change from one actionable census gap to a consistent forward/inverse result.

## Idempotence and Recovery

All edits and tests are repeatable. The corpus replay uses `--cache-mode ephemeral` and writes only its chosen output file. If a persisted cache predates the new metadata, the Go analysis epoch must invalidate it; do not accept a test that passes only from a fresh in-memory project.

## Artifacts and Notes

Pre-fix exact evidence:

    occurrence: fa4c8623fb905339
    source: core/vm/jump_table.go, maxStack in JumpTable{ ... {maxStack: ...} }
    result: forward resolved_sites=0, actionable=1
    diagnostic: go_literal_owner_unresolved

## Interfaces and Dependencies

`SignatureMetadata` gains `underlying_type_identity: Option<StructuredTypeIdentity>`, `with_underlying_type_identity`, `underlying_type_identity`, and `into_underlying_type_identity`. This is data only and keeps `brokk-bifrost-core` at the bottom of the dependency graph.

The Go AST module exposes a small public container-step enum and an owner-path result containing the explicit type node plus ordered steps. The Go edge index owns structured Go facts but adds no crate dependency. Analysis consumes the core metadata through its existing `GoDefinitionProvider` abstraction.

Revision note (2026-08-13): Initial plan created after reproducing the current exact witness and auditing forward and targeted inverse seams.

Revision note (2026-08-13): Recorded the completed declaration, forward, inverse, epoch, and behavior-test milestones and the named-map keyed-value discovery.

Revision note (2026-08-14): Recorded the completed local gate and exact 23-of-84 broad corpus audit, including the ticket's stale 17-site count.
