# ExecPlan: authored summaries close boundaries for unmaterialized external calls (#1978)

## Context

Require-model taint abstains on every flow that passes through a JDK library
transform (`java.net.URLDecoder.decode`, `String.trim`), including the canonical
OWASP path-traversal case BenchmarkTest00001. The first true positives already
work for native flows (direct source to sink, string concat). The blocker is
that an authored procedure summary never binds to such a call.

Root cause (proven by experiment and a full pipeline trace):

- An `ExactExternalProcedureTarget` is produced only by materializing the
  callee's program semantics from a real artifact (a workspace source or a
  classpath dep) -- see `workspace_oracle/dispatch.rs:320-331`, symbol built by
  `exact_external_procedure_target` at `dispatch.rs:2018-2040`, which needs a
  `CodeUnit` declaration. A JDK `java.base` method is neither a workspace source
  nor a dep, so it never materializes. `declaration_facts` overlays do not
  produce targets. Confirmed: declaring `URLDecoder` in framework-decls changed
  nothing (`0 external target(s) to match`). Adding the JDK to the classpath
  would defeat the purpose of the summary packs.
- The callee's syntactic identity IS computed during dispatch
  (`DefinitionLookupOutcome::resolved_reference_target()` ->
  `ResolvedReferenceSite.text`, the dotted callee chain such as
  `java.net.URLDecoder.decode`) but is dropped before any boundary at
  `usages/call_relations.rs:479` and `:948-954`.
- `has_receiver` and argument count are available for free from
  `SemanticCallSite` (`ir/model.rs:620-621`).

So the fix is to let an activated summary close a boundary for an external
callee that never materializes: carry the callee identity to the boundary,
match it against activated summaries by a reduced canonical identity, and apply
the summary at both discovery and solve time.

## Scope of this cut

Handle FULLY QUALIFIED external calls first, where the owner FQN is present
verbatim in the callee text (this is exactly BenchmarkTest00001:
`java.net.URLDecoder.decode(...)`). Instance-method transforms whose owner needs
type resolution (`s.trim()`) and unqualified calls needing import resolution are
an explicit follow-up, not this cut. State that boundary in the code and the
tests.

## Canonical identity (owner decision, settled)

Match an unmaterialized external callee to a summary by:

  (language, owner-FQN, member-name, arity, has_receiver)

NOT by the parameter-typed symbol. Parameter type spellings are not recoverable
for an unmaterialized callee, so arity is the only parameter signal. Overloads
that differ only by parameter type at the same arity cannot be distinguished for
an unmaterialized callee; document this in the match site. The artifact path is
provenance only, not part of the match.

## Design

1. Carry the callee identity to the dispatch boundary.
   - `usages/call_relations.rs`: stop discarding the reference target. Extend the
     boundary carrier so the External/UnresolvableImportBoundary arm retains the
     dotted callee text (see the drop at `:474-482` and `:948-954`). Prefer a
     structured `(owner_fqn, member, arity, has_receiver)` if it is clean to
     build here; otherwise carry the dotted text plus arity/receiver and
     structure it one layer up.
   - Lift it into the semantic boundary model (`analyzer/semantic/oracle/model.rs`
     `DispatchBoundaryKind`, and `workspace_oracle/dispatch.rs` where
     `low_level_boundary` maps `CallDispatchBoundaryKind::External ->
     DispatchBoundaryKind::External(None)`; the identity should ride an
     `External(Some(UnmaterializedExternalCallee))`-style form or a sibling
     field). Arity/receiver come from the `SemanticCallSite` row, owner+member
     from the callee text.

2. Synthesize a canonical `SemanticLocator` for the unmaterialized callee from
   (language, owner-FQN, member, arity, has_receiver). Both the boundary (for the
   solve) and the summary key (for discovery binding) must construct the SAME
   locator so they compare equal. Reuse the existing `DeclarationLocator` /
   `SemanticLocator` machinery (`ExternalSummaryTarget::matches` at
   `reusable_summary.rs:1356-1362` already matches a boundary locator to a summary
   key on mount/path/language/declaration; make the synthetic locator line up
   with what the pack target lowers to).

3. Make the activated-summary index findable by the canonical identity.
   - `semantic_model/runtime.rs` `ProcedureSummaryTargetKey` and its index
     (`MatcherIndexes::build`, ~`:591-614`) are keyed by (language, path, symbol,
     has_receiver, parameter_count). Add a parallel lookup by (language,
     owner-FQN, member, arity, has_receiver) derived from the same authored
     `SummaryTarget`. Parse owner-FQN + member from the authored symbol
     (`java.net.URLDecoder.decode(...)` -> owner `java.net.URLDecoder`, member
     `decode`); arity from `parameter_count`.

4. Bind at discovery.
   - `bifrost-policy/src/taint_policy.rs` `discover_value_flow`: collect
     unmaterialized-external identities from boundaries (alongside the existing
     `boundary.exact_external_target()` at `:924-930`). In
     `bind_external_summaries` (`:1008-1165`), match those identities against the
     activated summaries by the canonical identity, and build the same lowered
     `ExternalSemanticSummarySet` entry, anchored to the synthetic locator.
   - `verify_target` (`semantic_model/summary_binding.rs:344-384`) currently
     requires `target.path == binding.artifact.path()`. For an unmaterialized
     binding there is no resolved artifact path; relax this to the canonical
     identity for the unmaterialized case without weakening the materialized
     path.

5. Apply at solve.
   - `analyzer/value_flow/plan.rs` `visit_boundary_transfers` (`:1720-1768`) uses
     `external_summary_for_boundary` -> `external_summaries.summary_for(
     boundary.target_locator())`. Ensure the unmaterialized boundary now returns
     the synthetic locator from `target_locator()`, so the existing solve path
     applies the summary transfer and the RequireModel abstain at `:1766` no
     longer fires for a summarized call.

## Acceptance (hermetic, featureless)

Add an `InlineTestProject` taint test (suite_bench_policy) that does NOT rely on
the OWASP corpus:
- A workspace method whose parameter is a taint source, a FULLY QUALIFIED call to
  an external method with no workspace body (e.g. `com.example.Ext.wrap(x)`), and
  a sink on the result.
- Activate a procedure summary for `com.example.Ext.wrap` with transfer
  parameter 0 -> return.
- Assert: WITHOUT the summary the run is Inconclusive with no finding; WITH the
  summary a finding is produced and the run is complete for that boundary.

Then (reviewer, out of band) confirm end to end on the real corpus:
BenchmarkTest00001 pathtraver produces a finding.

## Validation

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings` (featureless)
- Featureless taint + semantic-model suites, and the existing summary-binding
  tests must stay green (no regression to materialized-external binding).

## Out of scope / follow-ups

- Instance-method and unqualified external calls (owner via type/import
  resolution).
- Overload disambiguation by parameter type for unmaterialized callees.
- Aligning the golden pack authoring contract to the canonical identity (the
  reduced identity may let the pack drop parameter-typed symbols later).
