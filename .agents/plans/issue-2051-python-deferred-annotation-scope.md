# Route Python definition lookup through structured annotation scope

This ExecPlan is a living document maintained under `.agents/PLANS.md`.

## Purpose / Big Picture

Python annotations postponed by `from __future__ import annotations` must resolve in annotation/type scope, not by runtime statement activation. After this change, forward lookup on a later class such as `Page` inside `tuple[Page | None, ...]` resolves to the indexed class, and quoted forward references use the same structured owner/import rules. Ordinary runtime expressions keep timeline semantics, external types remain boundaries, and strings inside `Literal[...]` remain values rather than type references.

## Progress

- [x] (2026-08-13 17:45Z) Read #2051, the exact Page ledger records, both get-definition routes, the Python graph annotation resolver, and deferred-annotation parser.
- [x] (2026-08-13 17:45Z) Reconciled the exact 158 sites as 116 simple type identifiers, 31 union operands, and 11 generic operands under postponed annotation semantics.
- [x] (2026-08-13 18:10Z) Added one definition-at-site annotation resolver that preserves the focused identifier while reusing graph owner/import facts.
- [x] (2026-08-13 18:15Z) Routed ordinary Python get-definition through it before runtime binding-timeline lookup; kept the separately budgeted bounded route unchanged rather than introducing an unbounded graph-index read into that contract.
- [x] (2026-08-13 18:20Z) Added InlineTestProject behavior coverage for future annotations, quoted/nested types, runtime controls, external boundaries, and `Literal` values.
- [x] (2026-08-13 18:30Z) Passed focused tests, dependency checks, clippy, and two exact Page replays.
- [x] (2026-08-13 18:45Z) Committed as `3544acd6`, merged the intervening FQName identity work, revalidated at the merged head, pushed `26df7d89` to `master`, commented exact evidence, and closed #2051.

## Surprises & Discoveries

- Observation: the campaign's 158 sites are not opaque string leaves. They are normal `identifier` nodes whose evaluation is postponed by the future import.
  Evidence: the canonical Python ledger partitions the family into `type>identifier` (116), `binary_operator>identifier` (31), and `generic_type>identifier` (11).

- Observation: `brokk-bifrost-python::graph::resolver::annotation_reference_candidates` already implements class-owner, named-import, top-level declaration, namespace, and quoted annotation rules for inverse analysis.
  Evidence: get-definition instead sends annotation identifiers through `python_visible_module_binding_candidates`, where postponed references use an end-of-module cutoff and can report a real class name as locally bound but without an indexed runtime-value definition.

## Decision Log

- Decision: Reuse the language-owned annotation resolver rather than changing generic module-binding visibility.
  Rationale: postponed evaluation is specific to annotation syntax. Runtime reads before a class declaration must remain governed by statement activation.
  Date/Author: 2026-08-13 / Codex

- Decision: Resolve the exact focused annotation segment, including parsed quoted annotations, instead of returning every type named by the surrounding annotation.
  Rationale: a compound annotation such as `"A | list[B]"` exposes separate navigation sites; combining all declarations would manufacture ambiguity.
  Date/Author: 2026-08-13 / Codex

- Decision: Keep `Literal["value"]` and arbitrary strings outside the annotation route.
  Rationale: the existing deferred-annotation parser deliberately rejects Literal values, and forward lookup must preserve that semantic boundary.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not route the budgeted bounded resolver through `PythonGraphSource` until that language surface accepts a bounded definition provider.
  Rationale: the current graph source exposes the dispatching `CodeUnitIndex`; using it from `resolve_python_bounded` would silently bypass row and cancellation budgets. The product path for #2051 is the ordinary batch resolver.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The ordinary definition resolver now delegates exact annotation sites to the language-owned annotation scope before runtime timeline lookup. The new behavior test resolves future-annotation union/generic operands, a quoted later class, an imported class, and both ordinary and quoted nested types. It also proves that a runtime read before the class declaration, `Literal["Page"]`, and an unresolved qualified annotation do not fall through to an unrelated declaration. A discovered pre-existing near miss - builtin `list` versus a later same-named method - remains fail-closed by excluding functions from annotation type candidates.

Focused validation passed: the new issue test, imported annotation lookup, builtin annotation collision, Literal annotation inverse coverage, the workspace dependency check, and clippy for `brokk-bifrost-python` plus `brokk-bifrost-analysis`. Two exact Sanic replays now resolve solely to `webapp.display.page.page.Page`, classify consistent, carry exact inverse hits, and report zero precision findings: union operand SHA-256 `a6e6c395ce034e37ad1351a2451bf1246bd8a81dd50589f3620953489fa34ff9`; return annotation SHA-256 `ba377988b075c97f59f69bbd79773944512ce52dd457726b811d571b88868c11`.

## Context and Orientation

Ordinary Python definition lookup lives in `crates/bifrost-analysis/src/analyzer/usages/get_definition/python.rs`. Its identifier branch checks lexical shadowing and the module binding timeline before imports and same-file declarations. That is correct for runtime expressions but not postponed annotations. The bounded entry point has an even simpler file-identifier fallback.

Structured annotation knowledge lives in `crates/bifrost-python/src/graph/resolver.rs`. It resolves bare annotation symbols in the enclosing class, named imports, and top-level module declarations, and it handles namespace-qualified and nested class annotations. `crates/bifrost-python/src/syntax.rs` reparses the exact interior range of a quoted annotation and excludes `Literal[...]` values.

## Plan of Work

Add a language-owned function that accepts the original annotation node plus the exact focused byte range. For ordinary annotations it identifies the focused bare or qualified segment from the existing tree. For a `string_content` node it uses the shared deferred-annotation parse, whose nodes retain original source offsets, to identify the same segment. Reuse the existing bare and qualified annotation resolution helpers with the original source node as the lexical class-scope anchor. Return only declarations for the focused segment.

Call this function near the start of ordinary Python get-definition, after declaration/non-reference rejection and before runtime lexical/module binding logic. A nonempty precise annotation answer returns immediately. An empty structured annotation answer remains `no_definition` rather than falling through to unrelated runtime declarations. Keep the bounded route on its budget-aware provider until the language annotation API accepts that provider explicitly.

Add a new issue test module using `InlineTestProject`. Cover a future-annotations module with a class declared after module and method annotations, unions and generic operands, a quoted simple type, an enclosing/nested class type, and an imported type. Verify a runtime expression before the class remains unresolved, an external annotation retains its boundary/no-definition behavior, and `Literal["Page"]` is not navigated as the class.

## Validation and Acceptance

Run focused issue tests, existing Python imported-annotation and builtin-annotation controls, `cargo fmt`, the workspace dependency checker, and clippy for Python plus analysis. Build the release differential runner and replay at least the Sanic Page union and one generic/type-position witness with ephemeral strict caching. Require one exact class target, `classification=consistent`, exact inverse range, and zero precision findings.

No new crate or dependency is required. The fix must remain in the Python language resolver plus analysis-side routing; it must not parse annotation text with regular expressions or delimiter splitting.

Revision note (2026-08-13 17:45Z): Created after reconciling #2051's issue description with the exact 158-site ledger and locating the existing structured inverse resolver bypassed by forward lookup.

Revision note (2026-08-13 18:30Z): Recorded the focused annotation implementation, bounded-provider decision, behavior controls, validation, and exact Page replay hashes.

Revision note (2026-08-13 18:45Z): Recorded the pushed commit, post-merge validation, GitHub evidence comment, and issue closure.
