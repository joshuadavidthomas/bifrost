# Require evidence for JavaScript and TypeScript qualified receivers

This ExecPlan is a living document. Maintain the `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` sections during implementation.

Follow `.agents/PLANS.md` from the repository root when you update this document.

## Purpose / Big Picture

Bifrost must not send a qualified JavaScript or TypeScript reference to an unrelated declaration. A qualified reference has a receiver before the dot and a member after it. For example, `settings.encodeUrl` has receiver `settings` and member `encodeUrl`.

Today, the resolver can lose receiver identity. It can find a declaration such as `formattedJson.settings.encodeUrl` because both paths contain `settings` and `encodeUrl`. This gives an incorrect definition when `settings` is not bound at the reference site.

After this change, each qualified result will require structured receiver evidence. A matching name will not count as evidence. Unknown or unsupported receiver shapes will return `no_definition`.

The change must preserve valid local object properties, imports, constructor receivers, exact `this` owners, classic-script globals, and TypeScript ambient globals.

## Progress

- [x] (2026-08-11 05:52Z) Diagnosed the active Bruno failure and its June 19 design origin.
- [x] (2026-08-11 05:52Z) Confirmed the original TypeScript regression tests pass on the starting revision.
- [x] (2026-08-11 05:56Z) Added reduced failing tests for unbound nested receivers and related positive cases.
- [x] (2026-08-11 06:13Z) Refactored qualified resolution to admit candidates only through structured receiver evidence.
- [x] (2026-08-11 06:13Z) Removed the two-segment assignment permission guard and unproven same-file fallback.
- [x] (2026-08-11 06:25Z) Ran focused tests, all 862 symbol-definition tests, formatting, and featureless workspace Clippy.
- [x] (2026-08-11 06:25Z) Completed security, maintainability, senior, operations, and architecture reviews.

## Surprises & Discoveries

- Observation: The original TypeScript cases are not currently broken.
  Evidence: The tests `typescript_unknown_global_members_do_not_guess_project_definitions` and `typescript_this_member_uses_exact_enclosing_class_with_duplicate_names` pass.

- Observation: The active Bruno case is JavaScript, although issue #682 has a TypeScript title.
  Evidence: `packages/bruno-toml/src/tomlToJson.js` reads unbound `settings.encodeUrl` near an assignment to `formattedJson.settings`.

- Observation: The JavaScript declaration builder supports nested assignment paths, but the safety guard accepts only two FQN segments.
  Evidence: `crates/bifrost-js-ts/src/javascript.rs` builds arbitrary-depth FQNs. `jsts_unbound_assigned_property_shape` destructures exactly two segments.

- Observation: The reduced Bruno fixture fails only for the same-file nested candidate.
  Evidence: `javascript_unbound_receiver_does_not_match_nested_same_file_member` resolves `formattedJson.settings.encodeUrl`. The cross-file form returns `no_definition`.

- Observation: Four valid JavaScript routes depended on the removed fallback.
  Evidence: The full JavaScript definition test set first exposed schema-builder fields, object methods, and exact top-level assignments. Exact lexical-owner and full-chain scope checks now preserve them.

- Observation: TypeScript class names were absent from `JsTsLexicalBindingIndex`.
  Evidence: `typescript_static_method_call_resolves_to_static_definition` failed until `pattern_binder_identifiers` accepted a class-name `type_identifier`.

- Observation: The first implementation expanded local member candidates twice on common dotted lookup paths.
  Evidence: Specialist review found unused TypeScript expansion and eager JavaScript expansion. The final lookup order avoids both operations.

## Decision Log

- Decision: Fix the receiver-evidence boundary instead of extending the segment-count guard.
  Rationale: A depth-only change fixes one fixture but keeps unsupported declaration shapes permissive.
  Date/Author: 2026-08-11 / Codex

- Decision: Keep receiver-admission policy in `brokk-bifrost-analysis`.
  Rationale: The policy uses `IAnalyzer`, declaration ranges, lookup ordering, and outcome semantics. These dependencies belong in the analysis crate.
  Date/Author: 2026-08-11 / Codex

- Decision: Reuse existing structured AST helpers from `brokk-bifrost-js-ts`.
  Rationale: `static_member_receiver`, `direct_property_definitions`, and `JsTsLexicalBindingIndex` already provide the required structure.
  Date/Author: 2026-08-11 / Codex

- Decision: Encode evidence in positive helper return boundaries instead of a marker enum.
  Rationale: An enum that wraps an already-built candidate vector would not enforce provenance. Exact-chain, lexical-owner, receiver-provider, import, construction, and global helpers now perform admission directly.
  Date/Author: 2026-08-11 / Codex

- Decision: Add `type_identifier` to structured binding patterns.
  Rationale: TypeScript class declarations use this AST kind. Without it, the lexical evidence index cannot prove local static receivers.
  Date/Author: 2026-08-11 / Codex

- Decision: Pin the fail-closed rule in both JavaScript and TypeScript fixtures.
  Rationale: Both languages share the admission ladder. Separate tests protect the language-specific branches.
  Date/Author: 2026-08-11 / Codex

## Outcomes & Retrospective

The receiver-evidence implementation fixes the reduced Bruno fixture. It removes the name-only fallback that caused the recurring defect.

All 59 JavaScript definition tests pass. All 51 TypeScript definition tests pass. The complete definition module passes 862 tests. Featureless workspace Clippy passes without warnings.

Five specialist reviews found no correctness, security, portability, or component-boundary defect. The reviews found two duplicate member-expansion paths and one TypeScript test gap. The final change removes the duplicate work and adds the test.

## Context and Orientation

The main resolver is `crates/bifrost-analysis/src/analyzer/usages/get_definition/js_ts.rs`. Its `resolve_js_ts` function handles JavaScript and TypeScript definition requests.

For a dotted reference, the function tries several routes. Imports and receiver analysis carry receiver identity. Other fallback routes use flat identifier or fully qualified name indexes. A flat index can find a declaration by spelling without proving that its owner is visible at the reference site.

`generic_member_candidates` starts from `support.file_identifier(file, qualifier)`. That query can return any same-file declaration whose terminal identifier matches the receiver. `jsts_member_candidates` then appends the requested member and asks the FQN index for matches. This can turn `settings.encodeUrl` into `formattedJson.settings.encodeUrl`.

`jsts_unproven_same_file_dotted_candidates` and `jsts_exact_dotted_candidates` are later name-based fallbacks. The helper `jsts_unbound_assigned_property_shape` tries to reject unsafe assignment targets. It recognizes exactly two FQN segments. A nested target bypasses the guard because an unsupported shape returns `None`, which callers treat as permission.

The structured helpers are in `crates/bifrost-js-ts/src/syntax.rs`. `JsTsLexicalBindingIndex` proves which binding is visible at a byte. `static_member_receiver` decomposes a member expression into a root and ordered member nodes. `direct_property_definitions` finds definitions for a property from AST ranges.

The integration tests are in `tests/suite_symbols/get_definition_test.rs`. Use `InlineTestProject` for each small JavaScript or TypeScript fixture.

## Plan of Work

First, add a reduced Bruno fixture. The fixture will assign `formattedJson.settings.encodeUrl` but read unbound `settings.encodeUrl`. The test must report `no_definition`. Add a cross-file form to prove project-wide suffix matches also fail closed.

Add positive fixtures before changing production code. Prove that an exact nested chain still resolves when the root is bound. Prove that an assignment resolves only after an earlier definition in the same lexical scope. Keep the existing object-literal, classic-script, import, `this`, and ambient-global tests as preservation coverage.

Next, refactor the dotted-reference ladder in `resolve_js_ts`. Admit candidates only from imported owners, exact lexical evidence, precise analyzed members, constructed owners, same-scope assignments, classic-script globals, or TypeScript globals.

Do not add every route to one large function. Keep the existing precedence. Convert each successful route into evidence immediately before it returns an outcome. Unknown evidence must reach `no_definition`.

Remove the unconditional late return of `generic_member_candidates`. Do not let `support.file_identifier(file, qualifier)` authorize a qualified result by itself. For a lexical binding, use the exact reference receiver chain, binding scope, definition range, and declaration file.

Replace `jsts_unproven_same_file_dotted_candidates`, `jsts_js_unbound_assigned_property_candidate_requires_exact_receiver`, and `jsts_unbound_assigned_property_shape` with one positive same-scope assignment helper. The helper must compare every structured receiver segment. It must require the same root binding, the same lexical fallback scope, and a definition before the reference.

Keep classic-script global support. A classic script has no import or export module boundary. Cross-file global resolution must prove that the declaration receiver binds at program scope in another classic script. A module-local receiver must not pass this route.

Keep TypeScript global handling. `ts_exact_global_dotted_candidates` must continue to accept ambient declarations and UMD namespaces. Do not route TypeScript through the JavaScript project fallback.

Do not change FQN creation in `crates/bifrost-js-ts/src/javascript.rs`. It already records nested paths correctly. Do not add a new crate or dependency.

## Concrete Steps

Work from `/Users/dave/.codex/worktrees/6506/bifrost`.

Add the tests first:

    cargo test --test suite_symbols javascript_unbound_receiver_does_not_match_nested

Before the production change, expect at least the same-file Bruno test to fail with a resolved unrelated definition.

Implement the receiver-evidence refactor in `crates/bifrost-analysis/src/analyzer/usages/get_definition/js_ts.rs`. Run formatting after each coherent edit:

    cargo fmt

Run the new tests:

    cargo test --test suite_symbols javascript_unbound_receiver_does_not_match_nested
    cargo test --test suite_symbols javascript_same_scope_nested_assignment_resolves_exact_chain

Run preservation tests:

    cargo test --test suite_symbols javascript_same_file_object_literal_property_resolves_to_definition
    cargo test --test suite_symbols javascript_cross_file_member_assignment_resolves_to_definition
    cargo test --test suite_symbols javascript_dollar_prefixed_local_member_keeps_exact_identity
    cargo test --test suite_symbols javascript_unbound_namespace_assignments_do_not_become_exact_dotted_definitions
    cargo test --test suite_symbols typescript_unknown_global_members_do_not_guess_project_definitions
    cargo test --test suite_symbols typescript_cross_file_ambient_namespace_resolves_to_global_declaration
    cargo test --test suite_symbols typescript_cross_file_umd_namespace_resolves_to_global_declaration
    cargo test --test suite_symbols typescript_this_member_uses_exact_enclosing_class_with_duplicate_names
    cargo test --test suite_symbols typescript_this_member_uses_same_file_constructor_field_not_other_class

Run the complete definition module and lint gate:

    cargo test --test suite_symbols get_definition_test::
    cargo clippy --workspace --all-targets -- -D warnings

## Validation and Acceptance

The reduced Bruno test must return `no_definition` for unbound `settings.encodeUrl`. Its definitions field must be null.

The nested bound-chain test must resolve only `formattedJson.settings.encodeUrl`. The result must point to the exact earlier declaration.

The same-scope assignment test must resolve a read after the assignment. A read before the assignment must return `no_definition`.

The cross-file classic-script test must resolve when the receiver is a proven program-scope global. The equivalent module-file test must return `no_definition`.

All existing TypeScript regression tests must pass. The full `get_definition_test` module must pass. Featureless workspace Clippy must finish without warnings.

## Idempotence and Recovery

All edits are source and test changes. Commands are safe to repeat.

If a focused test exposes a valid route that the evidence model omitted, add explicit proof for that route. Do not restore a name-only fallback.

If a helper cannot interpret an AST shape, return no candidates. Record the unsupported shape in `Surprises & Discoveries` before adding support.

## Artifacts and Notes

Relevant history:

    fe862359a  Resolve JS TS object literal properties
    f53991cfe  Fix TypeScript dotted receiver resolution (#728)
    671e9b23b  Reject unbound JavaScript namespace assignment artifacts (#1277)
    82b3d144c  Fix exact JS and Rust forward reference identity

Issue #682 began with TypeScript symptoms. The current residual is JavaScript and requires the general receiver-evidence rule.

## Interfaces and Dependencies

Keep receiver evidence private to `crates/bifrost-analysis/src/analyzer/usages/get_definition/js_ts.rs`. Positive helper return boundaries enforce the evidence rule.

Use `brokk_bifrost_js_ts::syntax::JsTsLexicalBindingIndex` for binding proof. Use `brokk_bifrost_js_ts::syntax::static_member_receiver` for structured receiver chains. Use `direct_property_definitions` for target ranges.

Do not add dependencies. Do not move analysis policy into `brokk-bifrost-core` or `brokk-bifrost-js-ts`.

Revision note: Recorded final validation, specialist review results, performance corrections, and TypeScript regression coverage.
