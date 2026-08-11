# Preserve taint witness truncation causes for direct nested calls (issue #1954)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md` at the repository root.

## Purpose / Big Picture

Bifrost's production taint route (a `.rqlp` policy compiled to a taint analysis plan and solved over the interprocedural control-flow graph) currently reports the smallest supported finding shape, `sink_one(source_one())` in Python, with a truncated witness. The retained witness is in fact a complete nine-step path with zero omitted steps. The truncation state is fabricated by a cascade that collapses several distinct internal markers into one public "truncated" bit. After this change, the direct nested-call fixture produces a public witness with no omitted steps and no truncation, the internal and public results carry the exact truncation cause when truncation does occur, and real limit exhaustion still reports honestly.

A reader can see the behavior with the new test module:

    cargo test --test suite_bench_policy issue_1954

Before this change, the production policy run marks each retained witness `truncated=true` and the policy projection reports one fabricated omitted step. After the change, those witnesses report `truncated=false`, `omitted_steps_lower_bound=0`, and no `witness_truncated` incomplete reason, while a run under deliberately tiny reconstruction limits still reports truncation with a typed cause.

## Reproduction and root cause (verified on this tree, 2026-08-11)

The fixture is the issue's production fixture, indexed as `app.py` in a temporary Python workspace:

    def source_one():
        return "one"

    def sink_one(value):
        pass

    def run():
        sink_one(source_one())

The policy is the shared DataFlowBench shape: a taint policy whose source selector is `(call :callee (name "source_one"))` bound to `return-value`, and whose sink selector is `(call :callee (name "sink_one"))` with dangerous operand `(argument :index 0)`, with `(call-modeling :unmodeled optimistic)`. Running `evaluate_policy_inputs_with_analyzer` (crate `brokk-bifrost-policy`) over this fixture reproduces the issue's observation: one public finding, two retained witnesses of nine steps each, both flagged truncated.

The internal state, dumped from `TaintFindingReport`:

    finding proven=false complete=false qualities={bits:1}
      origins: witness_truncated=true count=1
      witness steps=9 truncated=false omitted=0 alternatives_truncated=true retention_truncated=false

The nine steps run entirely inside `run` (`Seed`, five intraprocedural edges, `CallToNormalContinuation` over the `source_one` call, the source-introduction edge producing the carrier fact, and the `CallToNormalContinuation`/`CallToExceptionalContinuation` edge over the `sink_one` call producing the sink meeting fact). The witness is complete: `truncated=false`, `omitted_steps_lower_bound=0`.

The defect chain, in order:

1. First internal marker. Production solves with `WitnessRetentionLimits::best_effort(1, ...)` (`crates/bifrost-policy/src/taint_policy.rs`, `solve_and_project_batch`), so the witness arena retains at most one alternative derivation per path edge and path quality. The solver derives the same reached facts through several same-quality derivations: the optimistic call-to-continuation edge versus the matched call/return summary application through the bodied callee, and normal versus exceptional continuation variants. When a second same-quality derivation is staged, `WitnessArena::stage_candidate` in `crates/bifrost-analysis/src/analyzer/dataflow/witness.rs` returns `WitnessAdmission::Truncated` and calls `alternatives.mark_truncated(quality)`. `mark_alternatives_truncated` then stamps `alternatives_truncated=true` on the retained node. This was verified by temporarily printing the rejected candidates: they include a matched `SummaryApplication` with a `NormalReturn` edge from the callee, and duplicate-shaped continuation edges with different predecessors.
2. Reconstruction spread. `WitnessStore::reconstruct` ORs every visited node's `alternatives_truncated` into the reconstructed `SummaryWitness::alternatives_truncated`. Any witness whose path touches a busy path edge inherits the flag even though its own steps are complete.
3. Collection conflation. `reconstruct_origins` in `crates/bifrost-analysis/src/analyzer/taint/finding.rs` folds `witness.alternatives_truncated()` into both `witness_truncated` (per-finding origin status) and `budget.truncated` (report-level `collection_truncated`, which makes `TaintFindingReport::is_complete()` false).
4. Public projection conflation. `project_taint_finding_report_bounded` in `crates/bifrost-analysis/src/analyzer/structural/search/witness_projection.rs` sets the public witness `truncated = witness.truncated() || witness.alternatives_truncated() || witness.retention_truncated() || omitted_steps > 0` and degrades the witness quality completeness to `Partial` with reason "taint witness evidence is truncated".
5. Policy fabrication. `project_summary_witness` in `crates/bifrost-policy/src/witness_projection.rs` fabricates `omitted = 1` when `alternatives_truncated` (or `retention_truncated`) is set and no step was actually dropped. This is the issue's observed "nine steps and one omitted step". `project_taint_report` in `crates/bifrost-policy/src/taint_policy.rs` then adds `FindingIncompleteReason::WitnessTruncated`.

The witness limits are not exhausted: reconstruction used 19 expansions and 19 emitted steps against limits of 1,024 policy steps and 4,096 reconstruction steps. Strict retention (`WitnessRetentionLimits::new(64)`) on the same plan retains the identical first witness with `alternatives_truncated=false`, proving the marker is purely a consequence of the one-alternative production cap, not of any missing evidence.

Also confirmed: `witness_retention_truncated=false` (the best-effort sidecar was never abandoned), termination is `FixedPoint`, and the third internal finding is the callee-entry seeded re-observation of the same sink (a one-step `Seed` witness at `sink_one`'s entry with the meeting fact, no origins); it contributes no public witness and is unchanged by this plan.

Key vocabulary, defined once: a "path edge" is one solver relation "entry fact at procedure entry reaches fact at point"; "alternatives" are the retained witness derivations for one path edge and quality; a "meeting fact" is the taint fact recording that a tainted carrier met a sink; "projection" is the conversion of retained internal findings into public query results (`CodeQueryTaintFinding`) or policy findings; "matched call and return" means a witness that enters a callee through a `Call` edge and leaves through the paired `NormalReturn`/`ExceptionalReturn` edge of the same call site, as opposed to stepping over the call with a call-to-continuation edge.

## Design

The correction separates three orthogonal facts about a retained witness that the current code collapses into one bit:

- The witness's own step sequence is incomplete (steps were omitted). This is real truncation. It keeps degrading findings, and it now carries a typed cause.
- Sibling alternatives were not retained (`alternatives_truncated`). With the production cap of one alternative per quality this is the normal state for any program with branches. It must not mark the witness, the finding, the collection, or the projection as truncated or incomplete. It remains reported as its own field.
- Downstream budgets dropped or bounded evidence (collection budgets, projection step/byte budgets). These remain real truncation with their own typed causes.

New typed cause on the summary witness (`crates/bifrost-analysis/src/analyzer/dataflow/witness.rs`):

    pub enum WitnessTruncationCause {
        ReconstructionStepLimit,
        ReconstructionExpansionLimit,
        ReusableSummaryOmitted,
        RetentionExhausted,
    }

`SummaryWitness` replaces its `truncated: bool` field with `truncation_cause: Option<WitnessTruncationCause>`; `truncated()` returns `self.truncation_cause.is_some()`; the first cause encountered during reconstruction is retained. `retention_truncated_marker` carries `RetentionExhausted`. The existing invariant `truncated == (omitted_steps_lower_bound > 0)` is kept as a debug assertion.

Taint finding collection (`crates/bifrost-analysis/src/analyzer/taint/finding.rs`): `TaintOriginStatus` gains `witness_truncation_cause: Option<TaintWitnessTruncationCause>`, retaining the first cause. The taint-level enum wraps the witness-level causes and adds `CollectionBudget` (the finding-collection witness/step/expansion/byte budgets refused a reconstruction) and `QualityNotRetained` (the solver did not retain the requested quality's witness). `reconstruct_origins` stops folding `alternatives_truncated` into `witness_truncated` and into the collection budget's `truncated` flag; it keeps folding real witness truncation (`witness.truncated()`, which includes the retention marker) and budget refusals.

Public projection (`crates/bifrost-analysis/src/analyzer/structural/search/witness_projection.rs`): the public witness `truncated` becomes `witness.truncated() || witness.retention_truncated() || omitted_steps > 0`. `CodeQueryTaintWitness` gains `truncation_cause: Option<String>` with stable labels (`reconstruction_step_limit`, `reconstruction_expansion_limit`, `reusable_summary_omitted`, `retention_exhausted`, `projection_step_limit`, `projection_byte_limit`) so public diagnostics identify the cause without unstable IDs. The completeness reason names the cause. `alternatives_truncated` stays a separate public field with unchanged meaning.

Policy projection (`crates/bifrost-policy/src/witness_projection.rs` and `taint_policy.rs`): remove the fabricated `omitted = 1` for alternatives/retention flags. A retention marker witness has zero steps and already projects as an omitted witness through the existing `None` path. `BoundedWitness` keeps its invariant that `truncated` implies `omitted > 0`, which now holds naturally because internal truncation always implies a positive omitted lower bound. The `include_retention_truncation` parameter becomes unnecessary and is removed if the value-flow/typestate callers agree; otherwise it is kept for those callers and simply no longer consults `alternatives_truncated` for taint. Policy `FindingIncompleteReason::WitnessTruncated` is emitted only for real witness truncation.

Out of scope, per the issue: completion accounting (`partial_discovery`, owned by #1952), selector binding (#1953), the unproven proof state caused by honest Python dynamic-dispatch evidence, and the value-flow/typestate clients' own projections. This plan does not raise any limit.

## Milestones

Milestone 1: reproduce and pin the defect. Add `tests/suite_bench_policy/issue_1954_direct_call_witness.rs` (registered in `tests/suite_bench_policy/main.rs`) with the production fixture and policy above. This milestone is complete when the dump shows the truncation chain described under root cause. (Done during diagnosis; the dump tests are replaced by behavior tests in milestone 3.)

Milestone 2: typed causes and conflation fixes. Implement the design above across the four files. The workspace builds featureless; existing taint tests in `tests/suite_bench_policy/taint_policy_adapter.rs` are updated where they encoded the old conflation (notably `canonical_public_taint_witnesses`, which forced `omitted=1` when only `alternatives_truncated` was set, mirroring the policy fabrication).

Milestone 3: behavior tests. The new test module asserts, for the direct nested-call fixture through the full production route: exactly one public finding with one origin and one sink meeting; at least one retained witness; every public witness has matched call/return structure (a stack discipline over its steps: every `Call` step is closed by a return step of the same call-site origin, and no return appears without its call), no repeated step (no identical source/target/fact quadruple appears twice, which is what an artificial cycle would look like), `truncated=false`, `omitted_steps_lower_bound=0`, `retention_truncated=false`, and no truncation cause; the policy report carries no `witness_truncated` incomplete reason; and the JSON policy rendering agrees. A second test drives `collect_taint_findings_with_limits` with deliberately tiny reconstruction limits over a strict-retention solve of the same plan and asserts the typed cause (`ReconstructionStepLimit` or `CollectionBudget`) is preserved, proving real exhaustion is not hidden. A third assertion pins that `alternatives_truncated` remains visible as its own public field.

Milestone 4: validation and commit. `cargo fmt`, focused featureless tests (`cargo test --test suite_bench_policy`, plus the analysis crate's dataflow/taint unit tests), and a commit on the current branch.

## Progress

- [x] (2026-08-11) Read issues #1951 and #1954; explored witness reconstruction (`witness.rs`), collection (`finding.rs`), projection (`witness_projection.rs` x2), and the production solve path (`taint_policy.rs`).
- [x] (2026-08-11) Reproduced the Python nested-call truncated witness through the production policy route; captured internal and public state; identified the first internal marker (`stage_candidate` alternatives cap) and each collapse point; verified limits are not exhausted and strict retention removes the flag.
- [x] (2026-08-11) Milestone 2: typed `WitnessTruncationCause` on `SummaryWitness` (first cause retained, `truncated()` derived); `TaintWitnessTruncationCause` with `CollectionBudget`/`QualityNotRetained`/`Reconstruction(..)` on `TaintOriginStatus`; alternatives no longer fold into `witness_truncated`/`collection_truncated`; public `CodeQueryTaintWitness.truncation_cause` with stable labels plus `projection_step_limit`/`projection_byte_limit`; policy `project_summary_witness` no longer fabricates one omitted step (the now-unused `include_retention_truncation` parameter was removed for both taint and typestate callers).
- [x] (2026-08-11) Milestone 3: behavior tests in `tests/suite_bench_policy/issue_1954_direct_call_witness.rs`; `canonical_public_taint_witnesses` in `taint_policy_adapter.rs` now asserts truncation implies a positive omitted lower bound instead of re-fabricating one.
- [x] (2026-08-11) Milestone 4: `cargo fmt`; featureless `cargo clippy --workspace --all-targets -- -D warnings` clean (this required applying master's one-line `std::slice::from_ref` lint fix to `tests/suite_usages/issue_1819_cpp_macro_usages.rs`, identical to commit c8241e439 on master); suites green: suite_bench_policy 364 passed, suite_cross_language 499 passed, bifrost-policy units 312 passed, bifrost-analysis dataflow/taint units passed; committed on the current branch.

## Surprises & Discoveries

- The retained witness never visits the callees: taint is introduced at the caller-side return-value binding and meets the sink on the edge leaving the sink call, so the continuation-edge path is a legitimate context-respecting witness; the matched call/return derivation exists as the rejected second alternative.
- The issue's "one omitted step" is fabricated by the policy projection (`omitted = 1` when only `alternatives_truncated` is set), not by reconstruction; internally `omitted_steps_lower_bound` is zero.
- `canonical_public_taint_witnesses` in `taint_policy_adapter.rs` re-implements the same fabrication to make the public and policy projections comparable; it must change together with the fix.
- The third internal finding (callee-entry seeded re-observation, one-step `Seed` witness with the meeting fact, zero origins) is why `retained_witnesses=3` while only two witnesses reach the public projection.

## Decision Log

- Alternatives truncation is reclassified as an orthogonal signal rather than witness truncation, because with the production cap of one alternative per quality it fires for nearly any branching program and asserts nothing about the retained witness's own completeness. The field remains in internal and public shapes; only its aggregation changes. This is the minimal correction that fixes the fixture without raising any limit and without hiding retention or reconstruction losses.
- The typed cause is first-cause-wins, matching the issue's "retain the first internal reconstruction or retention cause"; later causes of the same reconstruction are strictly less informative for diagnosis and a set-valued cause would bloat every witness for no test-observable gain.
- "Missing predecessor" cases remain typed errors (`SummaryWitnessError::InvalidEvidence`) surfaced through `TaintFindingError::Witness`; they are not folded into the truncation cause because they indicate corrupted evidence, not bounded evidence.
- Public cause labels are stable strings on the witness rather than enum variants in the query schema, because they are result vocabulary (additive, no schema version change per the RQL maintenance rules) and must not expose unstable internal IDs.
- The solver is not changed to prefer matched call/return derivations over continuation derivations: which same-quality alternative is admitted first remains worklist-order-dependent, both are honest witnesses, and re-ranking retention is completion/solver territory that the issue explicitly fences off. The test asserts matched call/return structure (stack discipline), which both alternatives satisfy.

## Outcomes & Retrospective

- (2026-08-11) Complete. The direct nested-call fixture now projects complete untruncated witnesses through the production route: public witnesses report `truncated=false`, `omitted_steps_lower_bound=0`, no truncation cause; the policy JSON carries no `witness_truncated` incomplete reason and no fabricated omitted step; `alternatives_truncated` stays visible as its own field. Typed causes flow end to end and real exhaustion still reports: a two-step reconstruction limit yields `reconstruction_step_limit`, a one-witness collection budget yields `collection_budget` with an incomplete report, and the retention marker yields `retention_exhausted`. Remaining, owned elsewhere: completion accounting (#1952) keeps the finding unproven/partial through honest dynamic-dispatch evidence, and which same-quality alternative is retained first (continuation edge versus matched call/return) stays worklist-order-dependent. Lesson: the issue's "one omitted step" lived in the policy projection, not in reconstruction; reproducing at every layer (internal report, public projection, policy JSON) is what localized each collapse point.
