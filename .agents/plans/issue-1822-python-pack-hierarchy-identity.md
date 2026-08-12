# Resolve Python pack hierarchy identities

This ExecPlan is a living document. Maintain it under `.agents/PLANS.md`.

## Purpose / Big Picture

Python dependency packs must connect a class to its real base class. Today, `class Child(Base)` stores only `Base`. The semantic model can only guess that name. It cannot prove that a member is absent from the complete class surface. After this change, the producer will use imports and lexical declarations to store the qualified base identity. An end-to-end test will show a complete pack hierarchy and a proved missing-member result.

## Progress

- [x] (2026-08-12T22:07Z) Confirmed issue #1822 and found the raw `type_ref` call in `crates/bifrost-analysis/src/analyzer/python/external.rs`.
- [x] (2026-08-12T22:07Z) Confirmed that the producer already reads structured imports in source order.
- [x] (2026-08-12T22:07Z) Added failing producer and end-to-end tests for local and imported base classes.
- [x] (2026-08-12T22:07Z) Resolved base references with source-ordered lexical declarations and structured import bindings.
- [x] (2026-08-12T22:07Z) Ran focused tests, both Python semantic modules, formatting, and targeted Clippy.
- [ ] Commit, pull, push to `origin/master`, and close issue #1822.

## Surprises & Discoveries

- Observation: The producer records imported names as pack members, but it does not retain their qualified targets.
  Evidence: `PythonApiCollector::visit_import` stores only `owner.name` and a guard.

- Observation: The same structured relative-import operation serves workspace files and pack module identities.
  Evidence: The new producer test resolves `.bases.Base` to `widgets.bases.Base` through the shared package-based helper.

## Decision Log

- Decision: Qualify only hierarchy references in this change.
  Rationale: Issue #1822 concerns base-class edges. Signature types have different forward-reference and type-parameter rules.
  Date/Author: 2026-08-12 / Codex

- Decision: Use parser-derived `PythonImportDetails` for import targets.
  Rationale: The repository prohibits source-text parsing when the syntax tree already supplies structure.
  Date/Author: 2026-08-12 / Codex

- Decision: Keep a source-ordered hierarchy binding table in `PythonApiCollector`.
  Rationale: Classes and imports both bind names. Recording each binding when the walk reaches it preserves Python shadowing better than a file-wide name search.
  Date/Author: 2026-08-12 / Codex

## Outcomes & Retrospective

The producer now emits stable declared identities for local, aliased, relative, and namespace-imported base classes. A complete pack hierarchy now supports a proved missing-member result. One delivery step remains: commit, push, and close issue #1822.

## Context and Orientation

`crates/bifrost-analysis/src/analyzer/python/external.rs` reads `.py` and `.pyi` dependency artifacts. It creates `TypeFact` records. Each class record has `HierarchyFact` edges. A `TypeRef::Named` edge contains source spelling. A `TypeRef::Declared` edge contains the stable identifier of a qualified declaration.

`PythonApiCollector` walks one artifact in source order. It receives the artifact module name. It also reads imports through `brokk_bifrost_python::imports::python_import_infos_from_node`. The shared import module exposes `PythonImportDetails`, so the producer can get a structured module, imported name, and alias.

`tests/suite_semantic/python_dependency_pack.rs` tests the producer and its activated overlay. The acceptance test must use the normal offline dependency-pack path. It must not run Python or use the network.

## Plan of Work

First, add a producer test with `Base` and `Child(Base)` in one stub. Assert that the child edge is `TypeRef::Declared` for the stable identity of `module.Base`. Add an import control for an aliased base.

Next, add an end-to-end dependency-pack test. Put both classes in the selected pack. Ask semantic diagnostics about a missing member on `Child`. The result must contain an `Absent` member proof. It must not contain a name-resolved incomplete reason.

Then, extend `PythonApiCollector` with qualified import targets. Record each target by lexical owner and local binding name. When the collector visits a class, convert the parsed base `TypeRef` to a declared identity when a lexical declaration or import binding resolves its root. Preserve generic arguments. Leave unresolved names as `TypeRef::Named`, so incomplete surfaces stay honest.

Use the shared Python relative-import resolver. If it needs a module-identity entry point, add that entry point beside `resolve_python_relative_module` in `crates/bifrost-python/src/imports.rs`. Do not parse import text.

## Concrete Steps

Run all commands from `/mnt/optane/bifrost-fird`.

    cargo test --test suite_semantic python_dependency_pack::<new-test> -- --nocapture

Expect the new test to fail before implementation. After implementation, run:

    cargo test --test suite_semantic python_dependency_pack -- --nocapture
    cargo fmt --all
    cargo clippy -p brokk-bifrost-python --all-targets -- -D warnings
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings
    cargo clippy --test suite_semantic -- -D warnings

Commit only the plan, producer, shared import helper if changed, and tests. Pull without rebase. Push the current branch to `origin/master`.

## Validation and Acceptance

The producer test must show a declared edge to the qualified base identity. The end-to-end test must show a complete inherited surface and one proved missing-member result. Existing incomplete-base tests must still pass for a base no pack publishes.

## Idempotence and Recovery

The tests use temporary directories and offline artifacts. Repeated runs are safe. If a test fails, keep the plan current and use the focused command again. Do not remove user files or change branches.

## Artifacts and Notes

The current faulty call is in `PythonApiCollector::visit_definition`:

    target: type_ref(base, self.source, self.limits.max_signature_depth)

The corrected result for a base in module `widgets` must contain the identifier from:

    type_declaration_id(TypeIdentity { ecosystem: "python", name: "widgets.Base" })

## Interfaces and Dependencies

Keep `TypeRef` and the semantic-pack schema unchanged. Use `TypeRef::Declared { id, arguments, nullable }`. Use `type_declaration_id` and `TypeIdentity` for stable identifiers. Use `python_import_infos_from_node` and `python_import_details` for import syntax. Do not add a crate dependency.

Plan update note: Created this plan after confirming issue #1822. It records the current producer seam and the required end-to-end proof.

Plan update note: Implemented import-aware hierarchy bindings. The focused producer test, 11 dependency-pack tests, 20 Python diagnostic tests, formatting, and targeted Clippy passed.
