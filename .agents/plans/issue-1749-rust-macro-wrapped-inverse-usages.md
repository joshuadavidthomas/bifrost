# Fix inverse usages for Rust items inside macros

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

Maintain this plan under `.agents/PLANS.md`.

## Purpose / Big Picture

Bifrost indexes Rust items that appear inside item-position macro calls. Forward definition lookup can resolve these items. Inverse usage lookup can omit references from other files. After this change, inverse lookup will return the same cross-file type and member references.

## Progress

- [x] (2026-08-11 09:00Z) Read issue 1749 and its reduced four-file example.
- [x] (2026-08-11 09:15Z) Locate the declaration and inverse-usage paths.
- [x] (2026-08-11 10:10Z) Add end-to-end regression tests for wrapped structs, methods, and traits.
- [x] (2026-08-11 10:35Z) Make declaration syntax lookup find items inside nested macro token trees.
- [x] (2026-08-11 10:50Z) Run focused Rust, usage, and analyzer tests.
- [x] (2026-08-11 11:05Z) Run the final focused clippy and formatting checks.

## Surprises & Discoveries

- Observation: The declaration collector already reparses item macro interiors.
  Evidence: `crates/bifrost-rust/src/declarations.rs` indexes wrapped items with exact source ranges.
- Observation: Usage declaration facts search only the main Rust tree.
  Evidence: `rust_named_declaration_node` climbs from the saved range to the outer `macro_invocation`, then finds no matching item name.
- Observation: The Rust grammar does not always expose the macro token tree through an `arguments` field.
  Evidence: `child_by_field_name("arguments")` returned no node for the reduced `cfg_rt!` call. The shared `rust_macro_invocation_arguments` fallback found the named `token_tree` child.

## Decision Log

- Decision: Fix the shared structured declaration-node lookup.
  Rationale: Visibility, declaration kind, value constructors, and cfg facts all use this lookup. A scan-specific exception would leave these readers inconsistent.
  Date/Author: 2026-08-11, Codex.
- Decision: Reparse the enclosing macro token-tree region.
  Rationale: This uses tree-sitter structure and preserves original byte ranges. It does not use source-text parsing.
  Date/Author: 2026-08-11, Codex.

## Outcomes & Retrospective

The implementation now finds declaration syntax inside item macro arguments. The reduced struct, method, and trait inverse references pass. Rust crate tests, existing macro tests, formatting, and focused clippy pass.

## Context and Orientation

`crates/bifrost-rust/src/declarations.rs` indexes Rust declarations. It reparses item-position macro arguments and saves exact ranges for declarations inside them.

`crates/bifrost-rust/src/graph_support.rs` maps a saved `CodeUnit` back to a tree-sitter declaration node. Usage analysis calls this mapping to read visibility and declaration kinds.

`crates/bifrost-rust/src/usage_queries.rs` builds symbol identities and visibility domains. Inverse lookup uses these domains to decide which files can refer to a target.

The defect occurs because the main tree sees the wrapped item as token-tree content. The saved declaration range therefore maps to a token node, not to a normal Rust item node.

## Plan of Work

Add `tests/suite_usages/issue_1749_rust_macro_wrapped_inverse.rs`. Use `InlineTestProject` with separate macro, declaration, and consumer files. Query usages for the wrapped struct and its method. Require exact reference hits in the consumer file.

In `crates/bifrost-rust/src/graph_support.rs`, add a structured lookup that first uses the main syntax tree. If that fails, find the enclosing macro invocation, reparse its argument region with the existing region parser, and inspect the declaration while the reparse tree lives. Route node predicates and visibility through this lookup.

Update `crates/bifrost-rust/src/usage_queries.rs` to use the same lookup for cfg and constructor facts. Update `crates/bifrost-rust/src/hierarchy.rs` for impl members inside wrapped impl blocks.

## Concrete Steps

Run commands from `/Users/dave/.codex/worktrees/e72f/bifrost`.

First run the new test:

    cargo test --test suite_usages -- issue_1749_rust_macro_wrapped_inverse

Before the fix, expect the test to report missing cross-file reference hits.

After the fix, run the same command and expect all issue 1749 tests to pass.

Then run:

    cargo fmt --check
    cargo test -p brokk-bifrost-rust

## Validation and Acceptance

The new test must find the `SpawnMeta` type reference and `new_unnamed` method reference in `src/pool.rs`.

The existing Rust crate tests must pass. Formatting must have no changes.

## Idempotence and Recovery

All test commands are safe to repeat. The change has no data migration. Revert only the changed files if the implementation fails.

## Artifacts and Notes

The reduced source shape comes from issue 1749. A passthrough `macro_rules!` call wraps a public struct and its implementation in `src/trace.rs`. Another module imports and uses that struct.

## Interfaces and Dependencies

Use tree-sitter and `crate::lexical_scope::parse_rust_region_tree`. Do not add dependencies. Keep file and byte handling platform independent.

Revision note, 2026-08-11: Updated the plan after implementation. Added the token-tree field discovery, hierarchy scope, tests, and final results.
