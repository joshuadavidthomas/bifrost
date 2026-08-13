# Return Scala member candidates from duplicate supertypes

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md`.

## Purpose / Big Picture

Scala definition lookup currently reports no definition when a class extends two indexed physical declarations of one logical supertype. After this change, lookup will return the matching member from each physical declaration as an ambiguous answer. It will still stop without candidates when those declarations do not contain the requested member.

## Progress

- [x] (2026-08-12 16:05Z) Confirmed issue #1863 is open, local, and does not need owner input.
- [x] (2026-08-12 16:10Z) Found the candidate loss in `ScalaNameResolution`, the direct-ancestor cache, and exact-member lookup.
- [x] (2026-08-12 16:20Z) Preserved physical owner candidates in an analysis-local ancestor result.
- [x] (2026-08-12 16:25Z) Returned matching members from ambiguous owners at the blocked hierarchy level.
- [x] (2026-08-12 16:31Z) Updated the existing end-to-end regression and kept the fail-closed near miss.
- [x] (2026-08-12 16:35Z) Ran focused formatting, tests, and Clippy checks.
- [x] (2026-08-12 16:47Z) Committed, integrated `origin/master`, pushed, and closed issue #1863.

## Surprises & Discoveries

- Observation: `ForwardScalaNameResolver::resolve_candidate_tier` already collects the exact physical declarations.
  Evidence: It stores each declaration in `ScalaOwnerIdentity`, then replaces multiple identities with payload-free `ScalaNameResolution::Ambiguous`.

- Observation: The shared JVM ancestor result has a payload-free ambiguous case.
  Evidence: `ScalaDirectAncestorResolution::Ambiguous` in `crates/bifrost-jvm/src/scala/graph/namespace.rs` cannot carry physical declarations.

- Observation: The existing end-to-end fixture also supports a typed overload check.
  Evidence: Adding `overloaded(1)` returned both physical `replica.Base.overloaded` declarations as one ambiguous result.

## Decision Log

- Decision: Add an analysis-local detailed result and keep the shared JVM result unchanged.
  Rationale: Only forward Scala definition lookup needs the candidates. Changing the shared type would change unrelated JVM consumers.
  Date/Author: 2026-08-12 / Codex

- Decision: Inspect members only on the physical owners at the ambiguous hierarchy level.
  Rationale: This gives an honest candidate answer without continuing to a lower-precedence scope.
  Date/Author: 2026-08-12 / Codex

## Outcomes & Retrospective

The implementation preserves duplicate supertype declarations through the forward lookup cache. Field, parameterless method, and typed overload references return both physical members. The #1851 near miss still fails closed when those owners do not declare the requested member. Commit `6415b9ea` delivered the fix. Merge commit `ae89c3ab` put it on `master`, and both focused tests passed again after that merge.

## Context and Orientation

`crates/bifrost-analysis/src/analyzer/usages/get_definition/scala.rs` performs bounded Scala definition lookup. `ForwardScalaNameResolver` maps a visible type name to an indexed physical declaration. `ScalaLookupCache` stores direct supertype results for one request. `scala_exact_owner_member_candidate_units` walks owner members and supertypes by precedence.

The existing test `scala_forward_definition_preserves_physical_enclosing_owner_identity` is in `tests/suite_analyzers/scala_definition_precedence_test.rs`. Its external class extends `replica.Base`, which has separate JVM and JavaScript source declarations. Its final two assertions currently expect an empty answer.

## Plan of Work

Add a private name-resolution detail that retains ambiguous `ScalaOwnerIdentity` values. Use it only for direct supertype lookup. Add a private direct-ancestor detail with resolved, incomplete, and candidate-carrying ambiguous cases. Store this detail in `ScalaLookupCache`, and convert it to the shared payload-free result for existing callers.

Change exact-member lookup to inspect the requested member on each ambiguous owner. Return the collected declarations when one or more owners declare the member. Keep the current ambiguous failure when no owner declares it. Apply the same bounded candidate collection to typed overload lookup.

Update the existing end-to-end test. Require an ambiguous status, two definitions, the JVM and JavaScript paths, and the standard ambiguous-definition diagnostic. Run the existing duplicate-supertype near miss to prove lower scopes remain blocked.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`.

Run the focused test before and after the change:

    cargo test --test suite_analyzers scala_forward_definition_preserves_physical_enclosing_owner_identity -- --nocapture

Find and run the #1851 duplicate-supertype near miss by its test name. Then run:

    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings

## Validation and Acceptance

The external `count` and `ready` references must each return status `ambiguous`. Each answer must contain exactly two definitions from `jvm/replica/Base.scala` and `js/replica/Base.scala`.

The duplicate-supertype fixture without the requested member must still return no definition. It must not select a lower-precedence package or lexical declaration.

## Idempotence and Recovery

The tests and format checks are safe to repeat. If a test fails, keep the worktree changes and update this plan with the observed result.

## Artifacts and Notes

Issue #1863 records the intended answer shape. No external data migration or generated artifact is required.

## Interfaces and Dependencies

Do not change `ScalaDirectAncestorResolution` in `brokk-bifrost-jvm`. Add private detail types inside `crates/bifrost-analysis/src/analyzer/usages/get_definition/scala.rs`. Use existing `CodeUnit`, `ScalaOwnerIdentity`, sorting, and member-candidate helpers.

Revision note: Created this plan before implementation because the fix changes a multi-stage resolution contract.

Revision note: Updated the plan after implementation and focused validation. The results confirm the candidate answer and the fail-closed boundary.

Revision note: Recorded final delivery and issue closure after the post-merge validation passed.
