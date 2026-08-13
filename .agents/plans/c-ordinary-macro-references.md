# Resolve ordinary C macro references through the active macro environment

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document follows `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

Bifrost already indexes C preprocessor macros and reconstructs their activation order through includes, conditional definitions, and `#undef`. Navigation nevertheless ignores an active object-like macro when it appears as an ordinary expression value such as `return BLOCK_SIZE`, `buffer[SAVE]`, or `flags & BYTES`. After this change, those uses will navigate to the exact active macro definition, targeted usage lookup will report the same exact occurrence, and the whole-workspace inverted graph will record the same macro identity. Declaration names, macro parameters, labels, inactive definitions, and ambiguous conditional definitions will continue to fail closed.

## Progress

- [x] (2026-08-13 13:10Z) Read #2093, `.agents/PLANS.md`, and the existing forward, targeted-inverse, bulk-inverse, and macro-environment paths.
- [ ] Define one shared structured macro-reference role and exact-activation resolution verdict in `brokk-bifrost-cpp`.
- [ ] Route forward ordinary identifiers, targeted macro scans, and bulk inverted edges through the shared verdict.
- [ ] Add InlineTestProject behavior coverage for the advertised expression surfaces and fail-closed controls.
- [ ] Run focused validation, commit, rebuild the release runner, replay representative corpus witnesses, push, publish evidence, and close #2093 without waiting for the full rank-31+ rerun.

## Surprises & Discoveries

- Observation: target-specific inverse scanning already considers macro identifiers in many AST positions.
  Evidence: `crates/bifrost-cpp/src/graph/extractor.rs::maybe_record_macro_hit` compares identifier text, calls `VisibilityIndex::macro_binding_matches_target_at`, and emits an exact or unproven hit. It rejects only the macro definition name locally, so declaration, parameter, label, and call-surface policy can drift from forward lookup.

- Observation: forward macro resolution is implemented as an analysis-local candidate helper and is called only after type lookup or from call-specific routes.
  Evidence: `crates/bifrost-analysis/src/analyzer/usages/get_definition/cpp.rs::cpp_macro_candidates` consults the macro environment, but the ordinary `CppReferenceNode::Identifier` branch ends after local, member, same-file field, and visible-field lookup without calling it.

- Observation: the bulk inverted C/C++ pass currently records types, calls, fields, and designated initializers but has no ordinary macro reference arm.
  Evidence: `crates/bifrost-cpp/src/graph/inverted.rs::record_reference` has macro-specific recovered-type handling but no active-macro resolution for an ordinary identifier.

## Decision Log

- Decision: centralize macro occurrence admission and exact target selection in `crates/bifrost-cpp/src/graph/resolver.rs`.
  Rationale: the language crate owns tree-sitter node roles, the active macro environment, include visibility, and indexed macro identities. Returning one shared verdict prevents forward, targeted, and bulk routes from interpreting the same occurrence differently. The helper will use AST parents and fields only; it will not scan source text or special-case macro names.
  Date/Author: 2026-08-13 / Codex

- Decision: resolve only ordinary non-call expression occurrences in #2093.
  Rationale: function-like macro calls are owned by #1812 and #1819, while preprocessor-condition activation/order is owned by #1960. The shared role helper will therefore reject a call expression's `function` child and all preprocessor declaration/parameter/condition roles. This keeps the fix small and prevents unrelated issue families from being silently reclassified.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

No production code has changed yet. The source audit confirms that this is a shared resolver gap rather than missing macro extraction. The next milestone is a language-crate verdict used by all three consumers.

## Context and Orientation

The repository uses tree-sitter-cpp for both `.c` and `.cpp` files. A macro definition is indexed as a `CodeUnit` whose kind is Macro. `VisibilityIndex` in `crates/bifrost-cpp/src/graph/resolver.rs` replays macro events in source and include order to build the environment visible immediately before any byte. Its `macro_binding_matches_target_at` method proves that the active binding is a particular indexed macro definition; `macro_name_may_be_bound_at` represents an uncertain environment.

Forward navigation lives in `crates/bifrost-analysis/src/analyzer/usages/get_definition/cpp.rs`. The ordinary identifier branch already rejects declaration and preprocessor-body roles, checks local bindings, and applies field precedence. Macro resolution belongs after those role and ordinary-language precedence checks but before the final no-definition answer.

Targeted inverse usage scanning lives in `crates/bifrost-cpp/src/graph/extractor.rs`. It receives one macro target and asks whether each candidate occurrence names that target. Bulk inverted edge construction lives in `crates/bifrost-cpp/src/graph/inverted.rs`; it walks a file once and records the exact callee identity for every supported reference. Both should call the same shared occurrence role and resolution verdict as forward navigation.

An “ordinary macro reference” in this plan means an identifier-like AST leaf used as an expression value, array bound, cast operand, initializer value, return value, or selected member spelling. It excludes the name and formal parameters of a macro definition, declaration/binder names, `goto` and labeled-statement labels, preprocessor condition tokens, and the function child of a call expression.

## Plan of Work

In `crates/bifrost-cpp/src/graph/resolver.rs`, add a public occurrence predicate that accepts only the structured ordinary roles above. Reuse the existing `is_declaration_name` helper and inspect exact parent fields for macro definitions, macro parameter lists, labels, calls, and preprocessor directives. Add a public resolution enum with `Resolved(CodeUnit)`, `Ambiguous`, and `Missing`. Add a `VisibilityIndex` method that receives the graph source, file, node, and source text; it rejects non-ordinary roles, reads the exact-byte macro environment, gathers visible indexed macro units with the same terminal, and uses `macro_binding_matches_target_at` to prove the active definition. One exact logical target resolves. More than one exact target or a possibly bound name without one provable target is ambiguous. No active binding is missing.

In `crates/bifrost-analysis/src/analyzer/usages/get_definition/cpp.rs`, replace the analysis-local ordinary macro candidate interpretation with the shared verdict. Preserve the existing declaration/preprocessor rejection and local/member/global-field precedence, then return the resolved macro candidate before the final no-definition diagnostic. An ambiguous verdict returns an honest macro ambiguity/no-definition result without selecting a same-name definition.

In `crates/bifrost-cpp/src/graph/extractor.rs`, make `maybe_record_macro_hit` call the shared occurrence predicate and verdict. It emits a proven hit only when the resolved macro equals the requested target. An ambiguous verdict can retain the existing unproven behavior for visible matching targets; missing emits nothing. In `crates/bifrost-cpp/src/graph/inverted.rs`, add an early ordinary-macro arm before general identifier/type processing. A resolved macro records its exact FQN at the identifier range; ambiguity records only an unproven requested name where the bulk input asks for that terminal; missing falls through to ordinary language resolution.

Add `tests/suite_issues/issue_2093_c_ordinary_macro_references.rs` and register it in `tests/suite_issues/main.rs`. Build a small C project with `InlineTestProject`. Assert forward navigation and targeted inverse hits for active object-like macros used in an argument, binary expression, array bound, cast operand, initializer, return, and field selector. Assert controls for use before definition, use after `#undef`, conflicting conditional definitions, macro definition name, macro formal parameter, a local or ordinary nonmacro same-name occurrence outside macro activation, and a function-like macro call. Include bulk graph behavior if the public test harness can request macro nodes; otherwise add a language-crate unit test at the bulk resolver seam rather than widening public APIs solely for testing.

## Milestones

The first milestone is shared semantic resolution. It ends when one language-crate helper can distinguish an exact active ordinary macro occurrence from ambiguity, inactivity, declarations, parameters, labels, calls, and preprocessor conditions. Unit tests or the integration fixture must show that source order and `#undef` affect the verdict.

The second milestone is surface parity. It ends when forward navigation, targeted usage lookup, and bulk inversion consume the same verdict. The InlineTestProject behavior test must fail on the old implementation and pass on the new one for every advertised ordinary expression surface.

The final milestone is production acceptance. Rebuild `bifrost_reference_differential`, replay representative `SAVE`, `BLOCK_SIZE`, and `BYTES` findings from clean pinned repositories with ephemeral cache, and require no actionable result, no file error, and no new inverse-precision finding. Push and close #2093 with that focused evidence without waiting for the complete 843-site campaign rerun. The full rank-31+ rerun remains part of the broader active FIRD goal.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`. Edit with `apply_patch`. Run:

    cargo fmt --all
    cargo test -p brokk-bifrost-cpp --lib graph::resolver
    cargo test --test suite_issues -- issue_2093_c_ordinary_macro_references::
    cargo test --test suite_usages -- usages_cpp_graph_test::
    cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

After focused validation, build the release runner with:

    cargo build --release --bin bifrost_reference_differential

Use the exact `rerun_command` values from the complete C ledger, changing only the output path to a revision-specific directory under `/tmp` and retaining `--cache-mode ephemeral`.

## Validation and Acceptance

For each active object-like macro occurrence, `get_definitions_by_location` must return only the indexed macro definition active at that byte. Targeted usage lookup for that macro must contain the same exact occurrence range. Bulk inversion must record the same macro FQN when that surface is exercised.

A name before its definition, after `#undef`, under contradictory conditional definitions, or in a declaration/binder/label/call/preprocessor-condition role must not resolve through the ordinary macro seam. If a local or nonmacro declaration is the applicable language binding because no macro is active, existing local/field behavior must remain unchanged. Ambiguous macro state must never select one definition by iteration order.

Focused tests, affected-crate clippy, formatting, dependency validation, and diff checks must pass. Representative exact corpus replays must be clean and non-actionable with zero file errors and zero inverse-precision findings. Full CI and the complete 843-key rerun are not blockers to the requested push.

## Idempotence and Recovery

The implementation changes only deterministic AST and environment interpretation. Tests and exact corpus replays are safe to repeat. Macro template and environment caches are query-local and generation-scoped, so repeated analyzer builds do not publish partial state. If a replay is interrupted, rerun it into a new revision-specific output file. Keep prior artifacts for before/after evidence.

## Artifacts and Notes

The complete C ledger is:

    /mnt/optane/tmp/bifrost-fird/final-fcd83045/c-ranks31-50-fcd83045-raw-ledger.jsonl

The issue reports 843 unique census gaps: 842 ordinary argument/binary/assignment/array/cast/initializer/return positions and one macro-expanded member selector. Representative macro families include `SAVE`, `BLOCK_SIZE`, and `BYTES`.

Plan revision note (2026-08-13): Created after closing #2092 and auditing #2093's three resolver surfaces. The audit established that extraction and targeted matching already exist; the missing root is a shared ordinary-occurrence activation verdict.

## Interfaces and Dependencies

Add a public enum in `crates/bifrost-cpp/src/graph/resolver.rs` with resolved, ambiguous, and missing variants, and add a public `VisibilityIndex` method that returns it for an ordinary macro occurrence. The method depends only on `CppGraphSource`, `ProjectFile`, tree-sitter `Node`, and the source string already available to every consumer. Add a public structured occurrence predicate in the same module if callers need to distinguish “not an ordinary macro surface” before resolving. No new crate or dependency is required. `brokk-bifrost-core` remains dependency-free from other Bifrost crates, and no NLP or Python feature is involved.
