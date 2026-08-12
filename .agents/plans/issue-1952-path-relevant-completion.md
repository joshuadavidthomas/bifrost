# Make value-flow completion depend on path-relevant semantic gaps (#1952)

## Purpose

Bifrost's production taint route can prove a scalar source-call-to-sink-call flow in
many languages, but it cannot say so. Issue #1951 showed that all 27 executable
DataFlowBench cases come back `partial_discovery`: positives keep their candidate
finding but the run never completes, and negatives never earn a clean verdict.

Issue #1952 isolates the shared cause. The workspace oracle opens every value-flow
snapshot for a language when any of eleven semantic capabilities is unavailable,
without asking whether the missing capability can affect the analyzed procedure at
all. Python, Ruby, C/C++, Java, JavaScript, and TypeScript all omit at least
`StaticMemory`, so every snapshot in those languages is permanently open and every
taint run is permanently inconclusive. Separately, some adapters (Python is the
clearest) publish an `Unknown` dynamic-dispatch gap on every call site, including
calls whose target the adapter itself proved statically, which opens dispatch even
for a direct call to a known local function.

After this change, the smallest balanced scenario -- `x = source(); sink(x)` as the
positive and `source(); sink(constant)` as the negative -- completes in supported
languages: the positive with one proven finding, the negative with no finding and a
`Complete` run. Real gaps (an attribute store on the path in Python, an unresolved
callee, aliasing or exceptional-flow gaps) still produce typed incomplete results,
and when a run stays incomplete the policy output now names the first retained
cause instead of a bare `partial_discovery`.

## Background: how completion flows through the system

A "procedure snapshot" is the per-procedure list of value-flow relations
(assignments, parameter/return flow, memory loads and stores) that
`ValueFlowOracle::procedure_relations` in
`crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/value_flow.rs`
projects from the validated semantic IR. Its `SemanticOutcome` (Complete, Unknown,
Unsupported, ...) becomes a `SemanticInputStatus` when the production policy
compiler (`crates/bifrost-policy/src/taint_policy.rs`, `discover_value_flow`)
collects snapshots and call bindings into `ValueFlowInput` values.

`ValueFlowPlan::with_limits_and_call_behavior`
(`crates/bifrost-analysis/src/analyzer/value_flow/plan.rs`) merges every input
status into `discovery_status` and computes `discovery_complete` (all inputs
Complete with exhaustive coverage, all source/sink evidence proven and complete).
The taint client (`crates/bifrost-analysis/src/analyzer/taint/client.rs`,
`TaintSummaryResult::from_result`) ANDs `plan.discovery_complete()` with the
solver-side check `ValueFlowPlan::execution_result_complete`. When the AND is
false, `solve_and_project_batch` in `crates/bifrost-policy/src/taint_policy.rs`
lowers the run to `PolicyRunCompletion::Inconclusive([PartialDiscovery])`.

Two facts make the fix safe:

* The semantic IR validator
  (`crates/bifrost-analysis/src/analyzer/semantic/ir/validation.rs`,
  `memory_location_capability` and `require_capability`) rejects any artifact that
  contains a Field/Static/Index/Capture memory row without the corresponding
  capability. A procedure in a language without `StaticMemory` therefore cannot
  silently contain static-memory facts; the capability is provably unused.
* Language adapters emit per-construct `SemanticGap` rows (with typed `impacts`)
  at every construct they cannot lower -- e.g. Python attribute/subscript
  assignment emits an `Assignments`/`Unsupported` gap whose impacts include
  `ValueFlow`. The snapshot already sweeps those gaps and opens when a gap's
  impacts intersect {ValueFlow, ReturnTransfer, HeapRead, HeapWrite}. The gaps,
  not the blanket capability table, are the honest per-procedure evidence.

## Milestone 1: scope the snapshot capability check

`value_flow_capabilities_are_open` in
`crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/value_flow.rs`
currently opens the snapshot when any of eleven capabilities is unavailable. Split
the list:

* Scalar-core capabilities stay a blanket requirement: `Values`, `Assignments`,
  `Allocations`, `LocalFlow`, `ParameterFlow`, `ReceiverFlow`, `ReturnFlow`.
  Without these the relation stream itself cannot be trusted, and every shipped
  adapter declares them at least Partial, so this preserves current behavior.
* Memory-family capabilities -- `FieldMemory`, `StaticMemory`, `IndexMemory`,
  `Captures` -- open the snapshot only when the procedure actually retains a
  memory-location row of that kind (or, for `Captures`, a capture binding). The
  IR validator makes "unavailable and used" impossible, so in practice these no
  longer open the snapshot; the check documents the rule that a *used* but
  unavailable capability must still open.

The per-gap sweep in `procedure_relations` is unchanged: relevant gaps still open
the snapshot and still type the outcome (Unsupported/Unknown/Unproven/Ambiguous)
through `merge_gap_quality`.

Acceptance: a Python/Java/Rust procedure with only scalar locals, calls, and
returns yields `SemanticOutcome::Complete` with `CandidateCoverage::Exhaustive`;
the same procedure with an attribute store (Python) still yields a typed
non-complete outcome.

## Milestone 2: discharge avoidable dispatch gaps for proven static targets

`resolve_call` in
`crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/dispatch.rs`
already discharges a call-scoped `DynamicDispatch` gap when every retained
candidate is dispatch-closed (`closed_dispatch_discharges_gap`). Add a second
discharge arm: an `Unknown` or `Unproven` `DynamicDispatch` gap is also discharged
when the semantic call row's `declared_targets` is
`CallableTargetResolution::Proven(target)` and every retained candidate is exactly
that target with `ProofStatus::Proven` and `EvidenceCompleteness::Complete`
evidence. A `Proven` resolution is the adapter's own static-target assertion, so
the blanket per-call gap is avoidable; `Unsupported`, `Ambiguous`, and
`ExceededBudget` gaps keep standing, and calls without a proven declared target
are untouched.

Target matching: `CallableTarget::Local(id)` matches a candidate whose procedure
lives in the caller's artifact under that id; `Unmaterialized`/`External(locator)`
match on the candidate's semantic locator path and declaration.

Acceptance: for `sink_one(first)` in Python, `resolve_call` returns a Complete
outcome with one proven candidate and no `Unresolved` boundary, so the call
binding input status is Complete and no dispatch `SummaryBoundary` blocks
`execution_result_complete`.

## Milestone 3: retain the first incomplete cause in the plan

`ValueFlowPlan` gains a typed, retained cause for the first input (in the plan's
deterministic sorted order) that breaks `discovery_complete`:

    pub enum ValueFlowIncompleteCause {
        Snapshot { procedure: ProcedureHandle, status: SemanticInputStatus },
        SnapshotCoverage { procedure: ProcedureHandle },
        CallBinding { call: CallSiteHandle, callee: ProcedureHandle, status: SemanticInputStatus },
        CallBindingCoverage { call: CallSiteHandle, callee: ProcedureHandle },
        SourceEvidence { .. }, SinkEvidence { .. },
    }

Retained in `with_limits_and_call_behavior`, exposed as
`first_incomplete_cause()`, carried through `union_observations` (first plan with
a cause wins), included in `PartialEq`/`Hash` alongside the fields it derives
from. `Snapshot`/`CallBinding` carry the typed `SemanticInputStatus`, so an
`Unsupported { capability }` cause survives to the policy layer.

## Milestone 4: type the production completion

In `solve_and_project_batch` (`crates/bifrost-policy/src/taint_policy.rs`), when
`retained.report()` is neither complete nor proven-by-summary, replace the bare
`PartialDiscovery` with a reason derived from the plan cause: `CapabilityIncomplete`
when the first cause carries `SemanticInputStatus::Unsupported`, otherwise
`PartialDiscovery`; and attach one Warning/RunIncomplete `PolicyDiagnostic` whose
message renders the retained cause (procedure or call locator plus status label).
A solver-side incompleteness with a complete discovery keeps today's plain
`PartialDiscovery`.

## Milestone 5: tests

New module `tests/suite_semantic/value_flow_path_relevant_completion.rs` (add
`mod value_flow_path_relevant_completion;` to `tests/suite_semantic/main.rs`),
built on `tests/common/value_flow_conformance.rs`:

* Balanced positive and negative for Python, Java, and Rust: source call ->
  returned value -> sink argument 0 (positive, expect Complete discovery, complete
  result, sink Reached with a proven meeting); source call with unused result and
  a constant sink argument (negative, expect Complete discovery, complete result,
  sink NotReached). These are also the "unrelated missing capability" cases, since
  all three languages omit `StaticMemory` (Rust also omits Field/Index memory).
* A relevant-incomplete case: the Python positive routed through an attribute
  store (`holder.slot = source(); sink(holder.slot)`-shaped) keeps a typed
  non-complete discovery status and an Inconclusive sink outcome, and
  `first_incomplete_cause()` names the flow procedure.
* Plan-focused tests asserting `first_incomplete_cause()` is None for the
  balanced cases and Some(typed cause) for the relevant-incomplete case.

Production `.rqlp` coverage extends
`tests/suite_bench_policy/taint_policy_adapter.rs`: the sibling-callee scalar case
flips from Inconclusive/no-findings to Complete with one policy finding, and a new
balanced-negative test asserts Complete with zero findings. Existing scenario
expectations in `tests/common/value_flow_scenarios.rs` and its callers are updated
wherever they honestly flip (Unknown -> Complete for scalar shapes); flips are
reviewed case by case so no genuinely open case is weakened.

## Validation

From the repository root:

    cargo test --test suite_semantic -- value_flow_path_relevant_completion::
    cargo test --test suite_semantic -- value_flow_client::
    cargo test --test suite_bench_policy -- taint_policy_adapter::
    cargo test --test suite_cross_language -- code_query_value_flow::
    cargo test -p brokk-bifrost-analysis
    cargo fmt

All commands must pass featureless (no `nlp`).

## Progress

- [x] ExecPlan written.
- [x] Milestone 1: capability check scoped in workspace_oracle/value_flow.rs
      (scalar core blanket; memory family scoped to retained rows).
- [x] Milestone 2: dispatch gap discharge in dispatch.rs -- declared-Proven
      targets, and resolver-proven clean receiverless target sets; C++
      virtual dispatch keeps its open arm.
- [x] Milestone 3: ValueFlowIncompleteCause retained on ValueFlowPlan, plus
      SnapshotDiscovery classification (complete / refinable / incomplete)
      embedded into execution_result_complete.
- [x] Milestone 4: typed completion + cause diagnostic in the policy route;
      CapabilityIncomplete for Unsupported causes; runs with candidate
      findings that lost origin evidence degrade to inconclusive with a
      diagnostic instead of claiming a clean Complete.
- [x] Milestone 5: new suite module value_flow_path_relevant_completion
      (6 tests, all green); conformance scenario expectations updated case
      by case; icfg unit tests updated; taint adapter suite updated with a
      new balanced-negative production test.
- [x] Extra (root-cause fix): resolved calls project their own call-to-return
      continuation edges in the summary driver; exact-index memory relations
      keep partial completeness; implicit-abort discharge restricted to
      Unsupported-kind gaps.
- [x] Expectation updates across suite_semantic (icfg_contract,
      semantic_language_conformance, typestate_client,
      typestate_production_summary, measure_semantic_oracles,
      value_flow_scenarios), suite_cross_language, suite_bench_policy,
      bifrost_policy_cli.
- [ ] Two suite_semantic tests remain red and need diagnosis, not blind
      expectation flips:
      1. (fixed) semantic_value_language_contract binding coverage now
         expects Exhaustive; dispatch openness stays asserted on the
         dispatch result. python_deferred expectations updated (no
         Unresolved boundary, deduplicated continuation edges).
      2. (fixed) taint reusable-summary parity: the fresh solve's extra
         findings were byte-identical duplicates produced by the multiplied
         continuation entries reaching one sink with identical facts;
         collect_taint_findings_with_limits now skips a finding equal to one
         already retained. The public query row status is likewise
         plan-aware: ValueFlowPlan::public_semantic_status recomputes the
         coverage's semantic-status merge minus the abort-only
         exceptional-exit boundaries the completion logic discharges, so
         projections and completion agree.
- [x] Re-run: suite_semantic 888 passed, suite_bench_policy 355 passed,
      suite_cross_language 499 passed, brokk-bifrost-analysis --lib 1722
      passed, bifrost_lsp_server 203 passed, cargo fmt clean.
- [x] Final commit.

## Surprises & Discoveries

* (pre-implementation research) The IR validator already forbids memory rows
  without their capability (`validate_procedure` calls `require_capability` per
  memory-location row), so the memory-family arm of the new check is provably
  defensive rather than a behavioral hole.
* (pre-implementation research) Python emits its blanket `Unknown`
  dynamic-dispatch gap on every call site regardless of `declared_targets`
  (`crates/bifrost-analysis/src/analyzer/python/semantic.rs`, call lowering);
  this is exactly the avoidable gap named in the issue, and a central discharge
  in `resolve_call` fixes all adapters at once.
* (implementation) The adapters do NOT prove `declared_targets` for the
  balanced fixtures -- Python, Java, and Rust all leave the resolution Unknown
  and emit per-call `Calls`/`CallableReferences` "requires whole-program
  dispatch refinement" gaps with ValueFlow impact, plus per-point
  implicit-exception/implicit-Drop gaps with the conservative CONTROL_FLOW
  impact profile. The refinement the gaps demand is performed later by the
  workspace resolver (`resolve_call` retains Proven candidates). The scoping
  therefore has three parts: (1) binding sweeps ignore call-target refinement
  gaps because dispatch status already carries target uncertainty and the
  caller/callee snapshots keep the same gaps; (2) the plan discharges a
  snapshot's Unknown openness when its only relevant gaps are call-target
  refinement gaps whose calls have complete bindings in the same plan (the
  plan's own refinement answers them) or abort-only implicit-exception gaps;
  (3) implicit-exception gaps are discharged only when no abort path in the
  procedure runs user code, so handlers and cleanup bodies keep the run open.

## Decision Log

* Scalar-core capabilities keep the blanket check. Every shipped adapter declares
  them at least Partial, so no behavior changes today, and an adapter that cannot
  represent scalar flow at all must not produce "complete" snapshots.
* The memory-family arm checks actual memory rows instead of being deleted
  outright, so the rule "a used but unavailable capability opens the path"
  remains written in code even though the validator makes it unreachable.
* The dispatch discharge only fires for gap kinds Unknown and Unproven with a
  `Proven` declared target and fully proven retained candidates. Ambiguous,
  Unsupported, and ExceededBudget dispatch gaps always keep standing, satisfying
  "do not close a real open dispatch gap".
* The retained cause lives on `ValueFlowPlan` (not the taint layer) so the
  direct, JSON CodeQuery, RQL, and production routes share one cause identity; it
  participates in Eq/Hash because it derives deterministically from the inputs.
* Negative-side production coverage must assert Complete with zero findings on
  the balanced negative fixture, which is the honesty-critical direction; the
  fixture must provably have no path.
* (implementation) Caller-side facts crossed resolved calls only by accident:
  the blanket dispatch gaps produced Unresolved boundaries whose projection
  created the call-to-return continuation edges. Removing the gaps exposed
  that a fact interleaved across a resolved call (`data = source(); noop();
  sink(data)`) was silently dropped -- and, with completion now closing, a
  completed run reported a false clean negative. The summary driver now
  projects continuation edges for resolved calls; the Python balanced
  positive keeps an interleaved `noop()` to pin this permanently.
* (implementation) Java's exact-index scenario would have completed while
  losing the `values[i]` store-to-load may-flow: two mentions of the same
  index are distinct values, so the exact-index join is not value-proven.
  Exact-index memory relations now keep partial completeness and the index
  scenarios stay typed incomplete (Unproven for Java; TypeScript keeps
  Unknown from its own gaps).
* (implementation) The sibling-callee production finding retains no source
  origin evidence through the summary join, so the policy projection cannot
  emit it even though the run solves cleanly. The run is degraded to
  inconclusive with a diagnostic; the retention defect is tracked as its own
  follow-up (spawned task; #1951 witness/origin family).
* (implementation) JS/TS call bindings stay non-complete on their own
  evidence quality even with dispatch fully proven, so the TS balanced pair
  is out of scope here; Python, Java (complete) and C (typed incomplete for
  the initializer-transfer gap) carry the balanced suite.
* (implementation) The exceptional-flow conformance scenarios now prove the
  flow into a catch block through the resolved call's exceptional
  continuation edge: TypeScript with a proven-complete witness, Java with an
  unproven-partial one (its adapter's evidence is partial).

## Outcomes & Retrospective

Implemented end to end. The balanced scalar scenario completes honestly:
Python and Java positives retain one proven meeting and negatives report a
clean NotReached through the direct solver, and the production `.rqlp`
route reports Complete for the balanced negative (a new adapter test pins
it). Path-relevant gaps keep typed incomplete results throughout: the
Python attribute store, the C initializer-transfer gap, Ruby's open-class
dispatch, C++ virtual and PHP runtime dispatch, exact-index memory joins,
JS/TS binding evidence, and every handler-reachable implicit-exception
gap. Runs that stay incomplete carry the first retained cause as a typed
value and a rendered diagnostic, and CapabilityIncomplete replaces
partial_discovery when the cause is a missing capability.

The deepest discovery was that caller-side facts crossed resolved calls
only as a side effect of blanket dispatch-gap boundaries; removing the
gaps exposed a latent false-clean-negative. Resolved calls now project
their own continuation edges for problems that opt in (value flow and
taint), the reference IDE solver mirrors the opt-in, and the balanced
Python fixture keeps an interleaved call to pin the bypass forever.
Duplicated findings from the multiplied entries deduplicate at
collection, and the public query surfaces cap per-row completion by the
run's own completion so an exhausted query can never hand out clean
negatives.

Remaining, tracked separately: the sibling-callee origin-evidence loss
through summary joins (spawned follow-up task; #1951 family) keeps that
one production case honestly inconclusive, and re-running the pinned
DataFlowBench matrix against a clean build belongs to #1951's acceptance
gate.
