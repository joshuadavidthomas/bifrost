# Select Rust associated trait implementations by argument applicability

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Rust permits several implementations of one conversion trait for the same output type, such as `From<f32> for Value` and `From<f64> for Value`. At a call like `Value::from(float)`, the receiver identifies `Value` and the argument type identifies the applicable `From` implementation. Bifrost currently stops after receiver and method-name lookup, returning every `Value.from` implementation as one resolved target group. FIRD then reports an inverse miss even though the forward answer is semantically overbroad.

After this change, public definition lookup uses tree-sitter call, binding, generic-argument, and implementation nodes to select the applicable associated trait implementation when the argument evidence proves one. Calls with incomplete generic evidence or genuinely overlapping applicability remain ambiguous rather than being selected by name or source order. The serde-json `Value::from(float)` production witness must resolve only to `impl From<f32> for Value::fn from(f: f32)` and round-trip through an exact inverse hit.

## Progress

- [x] (2026-08-13 23:35Z) Read issue #2035, confirmed assignment to `jbellis`, recovered its exact serde-json ledger row, and mapped the scoped associated-item resolver plus existing Rust expression/type helpers.
- [x] (2026-08-14 00:22Z) Added a behavior-focused `InlineTestProject` regression covering exact `f32`/`f64`, `&mut T -> &T`, `&[T; N] -> &[T]`, generic input, equal applicability, and exact inverse separation.
- [x] (2026-08-14 00:31Z) Persisted AST-derived callable parameter type spellings and applied one-argument associated-call applicability after Cargo-target scoping in both forward definition lookup and inverse Rust graph lookup.
- [x] (2026-08-14 00:44Z) Passed the focused issue tests, neighboring definition and usage controls, all 53 `brokk-bifrost-rust` tests, formatting, focused isolated Clippy, dependency validation, and `git diff --check`.
- [x] (2026-08-14 00:45Z) Rebuilt the release differential runner and obtained a dirty-head precommit serde-json proof: one `From<f32>` target, one exact inverse hit, `consistent=1`, and actionable zero.
- [ ] Rebuild from the committed clean head and preserve the final exact serde-json replay evidence.
- [ ] Commit, push directly to `master`, close #2035 with evidence, and update this plan.

## Surprises & Discoveries

- Observation: The overbroad result is produced before the trait-specific resolver's uniqueness rule can help.
  Evidence: `rust_focused_terminal_scoped_declaration_outcome` first calls `RustReferenceContext::resolve_scoped`, expands the resulting `Value.from` FQN through `support.fqn`, and accepts all thirteen implementations. Only an empty direct set reaches `resolve_scoped_associated_item_matching`.
- Observation: Rust function signature metadata records parameter labels and arity but not per-parameter type identities.
  Evidence: `crates/bifrost-rust/src/declarations.rs::rust_signature_metadata` publishes `ParameterMetadata` labels and dispatch extensibility without `callable_parameter_types`; implementation applicability must therefore use declaration AST nodes or add a persisted structured contract deliberately.
- Observation: Existing `rust_expression_type_fqn` resolves indexed nominal types but primitive annotations such as `f32` have no CodeUnit FQN.
  Evidence: `rust_resolve_type_node_fqn` returns only types found through local/import/FQN indexes. The production argument is a parameter `float: f32`, so primitive and generic type structure must remain available independently of indexed nominal identity.
- Observation: Cargo-target scoping can re-expand same-FQN overloads after an earlier definition filter.
  Evidence: Applying argument applicability before `rust_scope_forward_candidates_to_cargo_target` still returned every `Value.from` implementation in the production replay. Moving applicability after target scoping reduced the result to `From<f32>`.
- Observation: The inverse graph's same-FQN identity intentionally collapses overload signatures, and its exact-candidate shortcut bypassed applicability for trait implementation methods.
  Evidence: The strengthened integration test initially attributed the `f64`, mutable-reference, and slice-coercion calls to the requested `f32` implementation. Enumerating the owner/member family and requiring exact `CodeUnit` equality after a unique applicability selection separated the overloads while retaining existing identity behavior elsewhere.

## Decision Log

- Decision: Keep the first implementation resolver local to `brokk-bifrost-analysis` and do not add a dependency from `brokk-bifrost-rust` back to analysis.
  Rationale: Argument inference needs `IAnalyzer`, indexed stores, and current source context, which repository dependency rules require to stay in analysis. The grammar crate may expose reusable syntax-only helpers if more than one analysis client needs them.
  Date/Author: 2026-08-13, Codex.
- Decision: Filter only with positive structured applicability evidence; unknown or tied applicability retains the original candidate set.
  Rationale: The user requires generic and genuinely ambiguous calls to remain ambiguous. Dropping candidates because a type cannot be proven would turn incomplete analysis into a false definition claim.
  Date/Author: 2026-08-13, Codex.
- Decision: Do not parse `CodeUnit::signature()` or split Rust type text.
  Rationale: Repository design rules require tree-sitter fields and existing structured analyzer data. Declaration extraction now stores the exact type-node spelling in signature metadata, and call argument types come from expression and binding AST nodes. The resolver uses signature text only to join a `CodeUnit` to its already-structured metadata row, not to parse a type.
  Date/Author: 2026-08-13, Codex.
- Decision: Bump the Rust analysis epoch for persisted callable parameter type spellings.
  Rationale: The implementation needs declaration-side type evidence after source trees are no longer resident, including persisted warm analyzers. Old cache rows do not contain that field and must not silently retain the #2035 behavior.
  Date/Author: 2026-08-14, Codex.
- Decision: Share the same applicability selector between forward and inverse analysis, but require a unique positive winner for inverse attribution.
  Rationale: Forward lookup may honestly retain a plural candidate set when evidence is unknown or tied. An inverse query for one exact implementation cannot claim a call unless applicability uniquely selects that same exact `CodeUnit`.
  Date/Author: 2026-08-14, Codex.

## Outcomes & Retrospective

The implementation and precommit production acceptance are complete. Final clean-head replay, publication, and issue closure remain pending.

## Context and Orientation

The public forward resolver is `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs`. A focused `Value::from` terminal reaches `rust_focused_terminal_scoped_declaration_outcome`, which resolves the owner and member and returns the candidate functions. The shared Rust graph resolver in `crates/bifrost-rust/src/graph/resolver.rs` can resolve direct and trait-associated items by owner and visibility, but it intentionally has no `IAnalyzer`-dependent call-argument inference.

Each candidate implementation method has signature metadata extracted from its exact tree-sitter `function_item`. The metadata now includes the exact spelling of every parameter type node, so primitive and compound type syntax remains available after persistence. The call tree has a `call_expression` whose `function` field covers `Value::from` and whose `arguments` field contains the input expression.

The current source already has iterative expression-type and binding-type inference helpers. Those helpers must be extended or accompanied by a small structured type-evidence representation that preserves primitives, references, arrays/slices, generic applications, and type parameters without reparsing strings. Exact equality is strongest. Rust coercions accepted by the regression, such as a shared reference to `String` applying to an implementation over `&str` if the analyzer can structurally prove that coercion, are weaker but still positive evidence. An unconstrained type parameter or unknown expression provides no narrowing evidence.

The exact production row is occurrence key `48323c8cdf4acb70992e3b349a1afeba37727ba6b7bdbbe722d556265e7292e1` in `/mnt/optane/tmp/bifrost-fird/final-63a1912a/rust-ranks31-44-63a1912a-raw-ledger.jsonl`. It targets serde-json `827a315bf2198558f0325b07bcc1e2cd973aba2f`, `src/value/ser.rs` bytes `4432..4436`, with source `Ok(Value::from(float))`. The baseline returns thirteen `serde_json.value.Value.from` methods, including `From<f32>` and `From<f64>`.

## Plan of Work

Create `tests/suite_issues/issue_2035_rust_associated_trait_applicability.rs`, register it in `tests/suite_issues/main.rs`, and use `InlineTestProject`. Define an output type and several one-argument trait implementations with the same associated method name. Exercise typed `f32` and `f64` parameters and require the exact implementation signatures. Exercise an input whose type is a function type parameter and require the unresolved candidate set to remain plural. Add a structured coercion case and a case where two candidates have equal supported applicability; neither may be collapsed to an arbitrary single implementation.

In `get_definition/rust.rs`, add the smallest shared structured helper needed to obtain argument type evidence and a candidate implementation's trait input type from tree-sitter nodes. Apply it only to a scoped associated call with multiple same-name function candidates. Rank/filter candidates by applicability. If one best applicability class remains, return only it. If several candidates share the best class or no candidate has positive applicability evidence, return the original candidate set. Preserve owner, role, visibility, and Cargo-target filtering already performed upstream.

Run the focused regression before and after the change. Inspect all changed code for stack-safe traversal, bounded support steps, and avoidance of source-text parsing. Run neighboring scoped associated-method tests and relevant Rust usage-graph tests, then the requested crate and static gates. Build the release runner from the committed clean head and replay the exact serde-json site with `--cache-mode ephemeral`; require one `From<f32>` target, `classification=consistent`, exact path/byte inverse recovery, actionable zero, and clean provenance.

## Concrete Steps

Run from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_issues -- issue_2035_rust_associated_trait_applicability
    cargo test --test suite_symbols -- rust_scoped_imported_associated_method_terminal_prefers_exact_method_over_owner_type
    cargo test --test suite_usages -- rust_graph_strategy_resolves_associated_method_and_const_without_receiver_inference
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

Replay occurrence `48323c8c...` against the pinned serde-json clone using the exact command shape recorded by the immutable ledger, a unique head-scoped output, and `--cache-mode ephemeral`.

## Validation and Acceptance

The focused test must prove that `Output::from(f32_value)` and `Output::from(f64_value)` each select one different implementation. It must prove the selected definition by implementation signature or exact declaration range, because every implementation intentionally shares one FQN. The generic and genuine-ambiguity controls must return more than one candidate or an explicit ambiguous outcome, never one arbitrary candidate. The coercion control must select only implementations Rust can apply under the modeled structured coercion.

The existing scoped associated-method and inverse graph controls must remain green. All Rust crate tests, formatting, focused Clippy, dependency checks, and diff checks must pass. The serde-json production row must return only `impl From<f32> for Value::fn from(f: f32) -> Self`, become consistent with an exact inverse hit at bytes `4432..4436`, and report zero actionable findings.

## Idempotence and Recovery

Tests and exact replays are safe to repeat. Keep each exact result in a new durable head-scoped path; never overwrite the immutable full Rust ledger. Use ephemeral analyzer caches for exact runs. Stage only files changed for #2035 and commit on the current branch; do not create or change branches. If a design cannot honestly distinguish one candidate from another, retain ambiguity and record the limitation rather than adding a textual fallback.

## Artifacts and Notes

The issue-owned raw row should be copied into a one-row checksummed manifest under `/mnt/optane/tmp/bifrost-fird/issue-2035-<head>/`. Accepted clean-head replay output and runner checksums belong beside it until final campaign manifests preserve them.

## Interfaces and Dependencies

No public API or new dependency was added. Declaration extraction in `brokk-bifrost-rust` populates the existing `SignatureMetadata::callable_parameter_types` field; the Rust analysis epoch invalidates older rows. Forward definition lookup and inverse Rust graph extraction share `rust_associated_call_applicable_candidates` inside `brokk-bifrost-analysis`.

Precommit evidence, 2026-08-14: `/mnt/optane/tmp/bifrost-fird/issue-2035-precommit-final/issue-2035-exact-replay.jsonl` has SHA-256 `51f60ee3b53ff849a5b8e530bc9170a17f7a82c90317710a1f1bfae472eb2f84`. It reports one resolved target, `impl From<f32> for Value::fn from(f: f32) -> Self { ... }`, `classification=consistent`, and an exact inverse hit at `src/value/ser.rs` bytes `4432..4436`; actionable is zero. The release runner SHA-256 is `bc844ddf145f6603774eaec1496f934b018422f061e3119d0e6710ebe127ee8d`. This artifact correctly records `bifrost_dirty=true` and is a precommit proof only.

Plan revision, 2026-08-13: Created the living #2035 plan after confirming issue ownership, exact production evidence, the direct-FQN early return, and the absence of persisted Rust parameter-type metadata. The plan requires positive structured applicability, ambiguity preservation, explicit coercion/generic controls, and clean exact production proof.

Plan revision, 2026-08-14: Recorded the implemented shared forward/inverse selector, the necessary Rust epoch bump, the Cargo-scoping and inverse-identity discoveries, complete focused/static validation, and the successful dirty-head production proof. Clean-head proof and publication remain required.
