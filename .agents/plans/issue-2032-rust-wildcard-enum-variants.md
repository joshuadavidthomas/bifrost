# Resolve Rust enum variants before the pattern-binder fallback

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Rust permits enum variants to enter the value namespace through explicit and glob imports. Tree-sitter represents a bare unit variant in a match arm with the same identifier pattern shape as a new local binding. Bifrost currently adjudicates that shape as local before import resolution, so four unit variants and two tuple variants in serde-json cannot navigate. After this change, a visible imported enum variant will resolve first, while a true binder with no matching imported variant remains a canonical local answer.

## Progress

- [x] (2026-08-13 22:15Z) Read #2032 and audited the Rust exact-role, lexical-binding, visible-import, namespace, and enum-variant helpers.
- [x] (2026-08-13 22:28Z) Added a failing `InlineTestProject` covering plain and `self::` glob imports, an explicit renamed import, unit/tuple variants, and a true binder.
- [x] (2026-08-13 22:28Z) Added the structured variant-before-binder seam and extended both ordinary and `self`/`super` glob routing with exact enum-owner lookup.
- [x] (2026-08-13 22:28Z) Passed focused tests, all Rust crate tests, both affected crate clippy checks, dependency validation, formatting, and all six pinned serde-json exact replays.
- [ ] Commit, push master without waiting for full CI, attach evidence, and close #2032.

## Surprises & Discoveries

- Observation: The general Rust visible-import resolver already understands scoped glob imports, export membership, namespace filtering, physical routes, and precedence. The defect is earlier: `rust_exact_reference_role_outcome` returns `local_binding` for a parser-classified pattern identifier before that resolver runs.
  Evidence: `Solidus` remains tier-2 Missing after #2036, while `rust_visible_import_resolution` already returns `GlobResolved` candidates and `rust_role_accepts_imported` admits enum variants in the value/callable namespace.
- Observation: Serde-json's production import is `use self::CharEscape::*`, not the unqualified `use Token::*` shape in the first reduced fixture. The scoped-glob path resolved only package/module declarations and deliberately filtered enum-owned members, so the unqualified fixture passed before the pinned witness did.
  Evidence: the first release replay still returned `local_binding` for `Solidus`. Extending `rust_scoped_glob_forward_import_candidates` with the same resolved-owner enum-variant lookup made all six exact commands consistent.

## Decision Log

- Decision: Add a focused pattern-variant lookup immediately before the conservative exact-role binder fallback, using `rust_visible_import_resolution` and retaining only declarations whose source AST node is `enum_variant`.
  Rationale: Moving all binder adjudication after general lookup would broaden unrelated fallback behavior. The focused seam reuses established import precedence, handles unit and tuple variants uniformly, and lets a true binder fall through unchanged when no visible enum variant exists.
  Date/Author: 2026-08-13 / Codex
- Decision: Teach both non-scoped and `self`/`super` glob candidate builders to query `{resolved_owner}.{reference}` and retain only exact enum-variant declarations.
  Rationale: Export traversal is module-oriented and cannot enumerate enum members. The owner is already resolved structurally from the import path, so the exact FQN lookup is sound; filtering by declaration AST kind prevents `use Type::*` from exposing arbitrary fields or methods.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Implementation and local validation are complete. The new suite-symbol regression passes for unqualified and `self::` glob imports, an explicit renamed variant, unit and tuple constructors, and a true binder. The adjacent #2036 regression and existing same-file enum tuple-pattern differential remain green, all 53 Rust crate tests pass, both affected crates pass clippy with warnings denied, and dependency/fmt/diff checks pass.

All six pinned serde-json commands now exit zero with `actionable=0`: `Solidus`, `Backspace`, `FormFeed`, `CarriageReturn`, and both `AsciiControl` sites resolve to their exact `serde_json.ser.CharEscape.*` declaration, and inverse usage contains the exact reference range. Publication remains to be recorded after commit and push.

## Context and Orientation

The forward Rust resolver lives in `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs`. `resolve_rust_unscoped` calls `rust_exact_reference_role_outcome` before its normal bare-name import tiers. `lexical_scope::is_pattern_binding_identifier` is intentionally conservative and therefore reads unqualified enum variants as possible binders. `rust_visible_import_resolution` is the authoritative scope-aware import resolver; `rust_declaration_is_enum_variant` validates the declaration's source AST role.

The regression belongs in `tests/suite_symbols/issue_2032_rust_wildcard_enum_variants.rs`, registered in `tests/suite_symbols/main.rs`, and must use `InlineTestProject`.

## Plan of Work

First create one Rust fixture with an enum containing unit and tuple variants, a glob import, an explicit renamed or direct variant import, and a true match binding. Query each focused token through `get_definitions_by_location`. The imported variants must resolve to the exact enum-owned field declaration; the true binder must remain a local/declaration answer rather than resolving to an unrelated same-named item.

Then add a helper beside the Rust exact-role logic. It should accept only a focused identifier that `is_pattern_binding_identifier` recognizes, call `rust_visible_import_resolution` at the exact byte in the value namespace, accept `Resolved` or `GlobResolved`, filter candidates through the existing enum-variant declaration predicate, and return `candidates_outcome` only when nonempty. All boundary, ambiguity, and unbound cases return `None` so the existing binder or import logic remains authoritative.

Finally run the focused suite and existing Rust definition/import regressions, build the featureless release differential, and replay the six serde-json coordinates. Each must resolve forward to the exact variant and round-trip through inverse usage without a new precision finding.

## Concrete Steps

From `/mnt/optane/bifrost-fird`:

    cargo test --test suite_symbols -- issue_2032_rust_wildcard_enum_variants:: --nocapture
    cargo test --test suite_semantic -- issue_2036_rust_census_grading:: --nocapture
    cargo test -p brokk-bifrost-rust
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings
    cargo fmt --all -- --check
    git diff --check

Build `bifrost_reference_differential` in release mode and run the stored exact commands for the four unit variants and the two `AsciiControl` tuple-pattern sites from `/mnt/optane/tmp/bifrost-fird/final-63a1912a/smoke-rust-serde-json-ledger.jsonl`.

## Validation and Acceptance

The focused test must fail before the production change and pass afterward. Both glob- and explicit-imported unit/tuple variants must resolve to the exact enum variant. A true variable-binding near miss must remain local or declaration-site adjudicated. The six pinned corpus records must become consistent exact-range forward/inverse results with no inverse-precision finding.

## Idempotence and Recovery

All tests and exact replays are repeatable and use temporary or ephemeral cache storage. If origin moves, merge origin/master rather than rebasing. Stage only the #2032 implementation, test, and this plan.

## Artifacts and Notes

The pinned input is `/mnt/optane/tmp/bifrost-fird/final-63a1912a/inputs/rust/serde-rs__json` at `827a315bf2198558f0325b07bcc1e2cd973aba2f`. The source ledger is `/mnt/optane/tmp/bifrost-fird/final-63a1912a/smoke-rust-serde-json-ledger.jsonl`.

Plan revision note (2026-08-13 22:15Z): Initial plan created after the adjacent #2036 closure and direct audit of the existing Rust import/variant resolver.

Plan revision note (2026-08-13 22:28Z): Updated after implementation and exact corpus replay. Added the production-only `self::Enum::*` discovery, the constrained owner-FQN lookup decision, and complete focused/corpus validation results.
