# ExecPlan: demand-driven require-model taint

## Outcome (2026-08-12): corpus one-pass reached by scaling the eager path, not by full demand

What landed (commits 7796e08cb, 363ff108e) makes the OWASP pathtraver subset run
in one pass with zero false greens, but it is **not** the Stage C lazy provider
below. Two findings redirected the work:

1. **Full demand (Stage C) is blocked.** The require-model fallback
   (`value_flow/plan.rs` `visit_fallback_outputs`) reads plan-global
   `bounded_globals` / `by_component` at every unmodeled-call boundary; those are
   complete only after every procedure has loaded. A lazy per-procedure plan
   cannot serve that read mid-solve without an Option-C solver-core change, which
   is out of scope. The lazy `ValueFlowProvider` was prototyped and abandoned.

2. **Endpoint seeding (Stage B) sacrifices cross-procedure completeness.**
   Forward-from-source and backward-to-sink miss a flow whose source and sink sit
   in sibling callees joined only through a common caller (the Stage-A
   cross-procedure fixture). So the all-procedures roots were kept for
   completeness.

Instead the eager materialization was made to **scale** by fixing three shared
budgets that abstained "by accumulation" at corpus scale (each honest per-file
cost summed into one request-wide cap):
- per-region `SemanticBudget` reset (was: `nested_entries` crossed 1M at ~76 files),
- `max_materialized_files` sized to the workspace, not the 256 per-query IDE cap
  (the endpoint enumeration is O(corpus) and the content cache bounds real cost to
  distinct files),
- per-batch reset of the witness-reconstruction lanes (a per-finding cap of 1,024
  witness steps had been threaded request-wide),
plus non-fatal catch-and-skip of a root whose closure exceeds its per-region
budget (its file reports `not_analyzed`, never a false clean), and compile-wide
work-report accounting across the resets.

Result on pathtraver (268 cases, one pass): `not_analyzed` 268 -> 48; analyzed 0
-> 220; TP 0 -> 6, FP 8; **false greens 0** throughout. Guardrail (Stage A
known-answer fixtures) green. The remaining 48 abstain on a capability gap (an
unsupported procedure value-flow snapshot), not a budget. The one-pass TP (6) is
below the batched-eager baseline (9); that gap is not yet attributed per case
(modeling/capability vs a per-region-vs-shared difference) and is the open
follow-up. The eager roots loop is therefore **not** retired (Stage D), because
the lazy replacement is blocked and endpoint seeding is incomplete.

The Stages B-D below are the original design and are kept for context; read the
outcome above for what is actually true today.

## Motivation (for David)

Require-model taint cannot run at corpus scale. On the OWASP Benchmark taint
subset, one policy compile over more than about 76 files exhausts the 1,000,000
`nested_entries` semantic budget and abstains. The pathtraver subset (268 cases)
cannot run in one pass at all. Batched into sub-55-file chunks it finds 9 true
positives with zero false greens, but the budget drops 91 cases before it can
analyze them. So today the honest verdict on a real repository is "we could not
check it," and that blocks every competitive taint number.

The cause is eager materialization. The require-model compile builds the whole
reachable workspace before it solves. `discover_value_flow` roots at every
procedure of every artifact, materializes each root's forward closure, and folds
it into one immutable `ValueFlowPlan` under one shared budget. The cost is
proportional to the workspace, even though each OWASP flow is local to one
servlet.

The key finding is that the layer underneath is already demand-driven and
already honest. The IFDS/IDE tabulation pulls a callee's evidence through the
`IcfgProvider` only when the frontier crosses that call. It seeds source facts on
demand as the zero fact reaches a source point. It emits a clean verdict only on
a fixpoint with no open boundary, and it abstains otherwise. The bounded,
content-keyed cache (`CompleteSemanticArtifactCache` on `CompleteValueCache`)
already exists and is the intended demand primitive. The only eager part is the
policy-layer pre-pass and the whole-closure plan it feeds.

That pre-pass is not a reasoned constraint. The cache existed a day before the
production adapter landed, the adapter uses a separate uncached path for
value-flow, and no commit or comment cites a reason to materialize the whole
workspace. It reads as an eager first cut, not a discovered requirement. So this
change does not fight the architecture; it finishes what the substrate already
supports.

The fix is to make the pre-pass demand-driven. Seed discovery from source and
sink sites, not from every procedure. Give each source-to-sink region its own
budget. Feed the already-lazy solve from a `ValueFlowProvider` backed by the
existing content cache, so a compile materializes only the slice each region
touches and reuses unchanged procedures across queries. This scales to any
corpus and matches how CodeQL and Infer scale: pay the materialization once,
then answer each query on demand.

The guarantee is the non-negotiable part. The invariant is that demand findings
equal eager findings. A differential test enforces it, is built before any
behavior change, and gates every stage. Anything the demand solve cannot afford
or cannot model abstains, exactly as today. We never turn an honest abstain into
a silent clean. That failure mode would destroy the "no false green" positioning,
which is the whole reason the tool is worth building.

The ask: this refactors the production taint adapter and sits on shared
analyzer-core surface near dataflowbench. We land it in small stages behind a
mode flag, keep the eager path as the differential oracle until the demand path
is proven on the full corpus across revisions, and merge master often. The one
real risk is a hidden completeness dependency in the whole-region completeness
fold (`value_flow/plan.rs:752-868`). If you know of one, say so; otherwise the
differential test is how we catch it.

---

## Context

Require-model taint abstains at corpus scale. `discover_value_flow`
(`crates/bifrost-policy/src/taint_policy.rs`) roots discovery at every procedure
of every materialized artifact (taint_policy.rs:592-617), eagerly materializes
each root's forward closure through `oracle.procedure_relations` / `resolve_call`
/ `call_bindings` (taint_policy.rs:857-994), and folds the whole result into an
immutable `ValueFlowPlan` (`value_flow/plan.rs:540-1000`), charged against one
shared `SemanticBudget` whose `nested_entries` lane is flattened to 1,000,000 by
`selector_compiler.rs::semantic_work_limits`.

The layer underneath is already demand-driven and honest:
- IFDS/IDE tabulation pulls callee evidence lazily via `IcfgProvider` only when
  the frontier crosses a call (`dataflow/summary.rs` `cached_call_transfers`
  :1100, `cached_exit_profile` :1148; `icfg.rs` :190-219). Recursion converges by
  the incoming/end-summary fixpoint; a missing reusable summary falls back to
  lazy tabulation (summary.rs:1757-1764).
- The solve is seeded at the root and generates source facts on demand
  (`taint/client.rs:476-496`, `ide.rs:1451`).
- A clean `Complete` is emitted only on `SolverTermination::FixedPoint` with an
  empty coverage boundary set (`value_flow/plan.rs:1580`,
  `dataflow/summary_result.rs:507-512`), else
  `PolicyRunCompletion::Inconclusive/PartialDiscovery` (`taint_policy.rs:1504-1526`).

## Non-negotiable invariant

Demand findings MUST equal eager findings on every fixture and corpus. The
pruning is the algorithm's own reachability, never a heuristic. Anything the
demand solve cannot afford or model becomes an honest abstain. The differential
test enforces this and is the reason the work is safe.

## Approach (full demand-driven; staging is an implementation detail)

### Stage A - regression net first (the guardrail)
Lock current correct behavior into tests BEFORE changing anything. NO runtime
mode flag and NO dual-run harness (eager abstains at scale, so it is not a useful
oracle; asserting demand == eager would only bake in eager's limits). Instead:
- **Unit: known-answer fixtures** via `InlineTestProject` + `taint_policy_adapter.rs`.
  Hand-built cross-procedure (source and sink in sibling callees joined through a
  common caller), recursion, sanitizer-cleared, and direct fixtures, each
  asserting the exact expected `taint_findings()` and `PolicyRunCompletion`. I
  own the ground truth because I build the fixture; verify the expected values
  against current behavior once at authoring time. These pass on current (eager)
  behavior and must keep passing after every later stage.
- **Corpus: OWASP labels, not eager.** The guard is computed against
  `expectedresults-1.2.csv`, using the existing `Scoreboard` (`src/owasp_benchmark.rs`,
  which already exposes `false_greens` and TPR): (1) zero false greens - no real
  case reports `Complete` with no finding; (2) TPR >= the batched-eager baseline
  (>= 9 pathtraver TPs). This guard cannot run until demand can execute the corpus
  in one pass, so it is applied in Stage B's verification, not authored now.

### Stage B - per-region budgets + endpoint seeding (unblocks the corpus number)
Replace the all-procedures roots (taint_policy.rs:592-617) with seeding from
source sites (forward) and sink sites (backward); a region is the
forward-from-source intersect backward-to-sink slice, complete for source->sink
flows. Give each region its own `SemanticBudget` (no shared cap). Verify Stage-A
differential stays green, then the OWASP pathtraver subset runs in one pass with
zero budget drops.

### Stage C - lazy value-flow provider + bounded content cache (full demand)
Introduce `ValueFlowProvider` mirroring `IcfgProvider`, yielding a procedure's
value-flow snapshot (`local_rules`) and a call's bindings (`call_rules`) on
demand; route `TaintFlowProblem` (`taint/client.rs:441-448`) to read from it, not
the pre-built plan. Back it with `CompleteValueCache`
(`complete_value_cache.rs:66-108`) keyed by
`(SemanticArtifactKey.fingerprint(), ProcedureId)` via `ContentIdentity`
(`semantic/ids.rs:410-431,847`), placed beside `semantic_cache`
(tree_sitter_analyzer.rs:2140), carried across `update()` (:9935). Solve the two
whole-closure obstacles: lazy per-procedure carrier identity (replace the dense
global interning at plan.rs:878-899) and incremental region completeness (replace
the whole-closure fold at plan.rs:752-868 by surfacing the coverage set the
solver already tracks at summary_result.rs:507-512). Abstain machinery, budget
threading, seeding, and recursion need no change.

### Stage D - retire the eager path
When the differential is green on unit + full corpus across many revisions,
delete the eager roots loop and whole-closure plan construction. Keep the
differential test permanent.

## Out of scope
Summary memoization (cache the graph, never the fixpoint) - a later optimization
if measured query overlap warrants it. Instance/import-qualified summary binding
(#1981) - orthogonal.

## Verification
1. Stage-A differential green on cross-procedure/recursion/sanitizer fixtures.
2. After B: OWASP pathtraver in one pass; record TPR/FPR/J; zero `not_analyzed`.
3. After C: differential green on the full OWASP taint subset, case for case.
4. Honesty audit: every eager-abstain case abstains under demand (hard assertion,
   the false-green guard).
5. `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; taint +
   semantic featureless suites; OWASP behind `--features release-tooling`.

## Coordination / risk
Refactors the production taint adapter (#1343) on shared analyzer-core surface.
Land in small stages, merge master often. No runtime mode flag; the guardrail is
the Stage-A known-answer fixtures plus the OWASP false-green/TPR guard, both
authored before behavior changes. Modify the discovery in place per stage; the
fixtures catch any regression. Top risk: a hidden completeness dependency in the
whole-region fold (plan.rs:752-868); the known-answer cross-procedure/recursion
fixtures are the mitigation. If demand ever finds fewer flows than the fixtures
expect, or reports a `Complete` clean on a labeled-vulnerable OWASP case, stop -
it is a correctness bug, not a tuning knob.
