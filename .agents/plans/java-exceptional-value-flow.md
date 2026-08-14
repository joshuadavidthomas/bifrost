# ExecPlan: Java exceptional control flow in value-flow

## Motivation

Every OWASP servlet `doPost` value-flow snapshot is Inconclusive, so the
pathtraver subset reaches **0 Complete verdicts out of 268** and abstains on
254/268 cases. The cause is that Java's CFG lowering marks the implicit
exceptional edges from throwing operations to their `catch`/`finally` handlers as
gaps rather than modeling them. Across 221 analyzed cases the first incomplete
cause splits: 83 `unsupported (exceptional_control_flow)`, 77 `unknown`
(catch-type refinement), 60 `unproven`.

For OWASP path-traversal the taint flows on the NORMAL path (cookie -> param ->
fileName -> `new File(...)`), which the analyzer already finds (6 TP). So this
work mainly converts **Inconclusive -> Complete** -- decisiveness, not new
detections -- but that is exactly the abstention gap the corpus run exposes, and
it makes the fake-case negatives confident instead of abstained.

## Current state (verified)

- `try_statement` (`java/semantic/control.rs:1466`) lowers the structure: a
  `dispatcher` point, edges dispatcher -> each catch entry (route-to-all, an
  over-approximation), and `finally` cleanup regions with normal-path completion.
- It does NOT feed the dispatcher from the try body's throwing points, so the
  catch is CFG-unreachable: the IFDS solver never processes it with the try's
  taint state, and a catch-block sink of a pre-try-tainted local is not found.
- The incompleteness is reported honestly (Inconclusive) via gaps:
  - dispatcher catch-type match: `ExceptionalControlFlow`, kind `Unknown`
    (control.rs:1533).
  - per-operation implicit exceptions, kind `Unsupported`: calls, `field_access`
    / `array_access` and runtime operators (`implicit_exception_gap`,
    control.rs:2026), for-each iterator, resource acquire/close, `synchronized`
    monitor, implicit super-constructor, `assert`, bound method-reference.

## Non-negotiable invariant

No false green. A real flow through a `catch` or `finally` must never yield
Complete-with-no-finding. The model over-approximates (routes the try's taint
state to every reachable handler) so no flow is missed; a case reaches Complete
only when its exceptional dataflow is soundly traced. Any residual it cannot
model stays a narrowed gap (Inconclusive), never a silent clean.

## Design

Sound conservative model first (over-approximate, never miss):

1. **Feed handlers from the try body.** An exception can occur at any throwing
   point in the try, so the handler's incoming taint state is (at least) the join
   of the try body's points. Add an exceptional route from the try region to the
   dispatcher (and into `finally` on the exceptional path) so the solver
   propagates the try's facts into the handlers. This is the core change; it
   makes catch/finally reachable with sound state.
2. **Catch dispatch stays route-to-all.** The existing over-approximation never
   misses a flow, so once handlers are fed, the `Unknown` catch-type gap becomes
   a precision note, not an incompleteness -- close it for taint (type refinement
   only narrows, it cannot add a missed flow).
3. **Per-operation implicit exceptions.** With the try-region -> handler route in
   place, an implicit exception from any operation in the try is already covered
   by the region route, so the per-operation `Unsupported` gaps that only assert
   "this op can throw into the handler" can be removed. Gaps that assert an
   unmodeled *value effect* (resource close order/suppression, monitor effects)
   stay until modeled.
4. **Exception-object taint** (`throw new E(tainted)` -> `catch (e)` ->
   `sink(e.getMessage())`): a later increment. Until then keep a narrowed gap
   scoped ONLY to exception-object provenance, so a flow that depends on it stays
   Inconclusive (honest), never Complete.

## Staging

- **A - guardrail fixtures first.** `InlineTestProject` + `taint_policy_adapter`,
  Java: (1) taint assigned before a try, sunk in the `catch`, must never be
  Complete-clean (false-green guard -- passes today as Inconclusive, must become
  found+Complete after B, must FAIL on a gap-removed-but-unsound build); (2) same
  through `finally`; (3) a try with no tainted value reaching any sink reaches
  Complete-clean (a confident negative, no spurious finding). These define
  correctness before any lowering change.
- **B - feed handlers + remove the covered gaps.** Implement design 1-3. Verify
  guardrail green, OWASP pathtraver `false_greens=0`, Complete count rises from 0,
  TP >= 6, and the catch/finally fixtures now find their flow.
- **C - exception-object provenance**, then close the residual gap.

## Out of scope
Precise per-catch type selection (route-to-all is sound); resource
close-order/suppression value effects; `synchronized` monitor value effects.
These keep narrowed gaps and stay Inconclusive until separately modeled.

## Verification
Guardrail green; OWASP pathtraver `false_greens=0`, Complete > 0, TP >= 6, no
real case regresses to a worse verdict; `cargo fmt`; `cargo clippy --workspace
--all-targets -- -D warnings`; featureless taint + semantic suites; OWASP behind
`--features release-tooling`.

## Coordination / risk
Touches `java/semantic/control.rs` (CFG lowering) and the value-flow completeness
fold. Top risk is an unsound gap removal that turns a missed exception-path flow
into a Complete-clean false green; the Stage-A catch/finally fixtures plus the
corpus false-green guard are the mitigation and gate every stage. If a real case
ever reaches Complete-clean, stop -- it is a correctness bug, not a tuning knob.
