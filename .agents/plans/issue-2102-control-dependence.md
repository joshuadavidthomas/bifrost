# Derive bounded, evidence-carrying control dependence

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub issue #2102, "Semantic analysis: derive postdominators and typed control-dependence edges." It is deliberately limited to the general semantic API. It contains no fault-localisation ranking, suspiciousness, benchmark-corpus interpretation, or research-specific behavior.

## Purpose / Big Picture

After this change, a caller of the bounded semantic relation API introduced by #2101 can ask which source-backed program points are conditionally governed by a branch, loop test, switch arm, exceptional transfer, or cleanup transfer. The result is a finite set of typed control-dependence edges. Each edge identifies the controlling point, the governed point, and the exact CFG edge whose choice supplies the control evidence. The result also states whether the answer is complete; an incomplete or nonterminating CFG can never masquerade as an authoritative empty result.

A developer can see this working by running the focused semantic tests and rendering an `if/else` fixture. The renderer must show two distinct controlling-edge identities from the predicate to the respective branch points, no dependence from either branch point to the post-branch join, and a complete outcome. A fixture with an unresolved continuation or an entry-reachable terminal non-exiting region must instead render an explicit boundary and incomplete outcome.

## Progress

- [x] (2026-08-13 11:24Z) Read issue #2102, `.agents/PLANS.md`, the epic plan, the validated semantic IR, CFG algorithms, ICFG implementation, renderer, and the prior #819 lifecycle evidence.
- [x] (2026-08-13 11:24Z) Fixed the algorithm contract, exit and incompleteness semantics, independent oracle, tests, and performance evidence in this issue-specific plan.
- [x] (2026-08-13 11:30Z) Reconciled this plan with the #2101 ExecPlan's `SemanticRelationRequest`, `SemanticRelationSnapshot`, `SemanticRelationKind::ControlDependence`, edge, boundary, diagnostic, codec, and `ExtensionWorkspace::semantic_relations` shapes.
- [ ] When #2101 source lands, confirm that the implemented names match its plan and that the v1 boundary vocabulary includes `non_exiting_region`; amend #2101 first if it does not.
- [ ] Add the production postdominator and control-dependence implementation with cancellation, work limits, deterministic output, and no recursion.
- [ ] Add the independent exhaustive oracle and algorithm-level generated-graph comparison tests.
- [ ] Project source-backed control-dependence edges and typed incomplete boundaries through the #2101 bounded semantic snapshot API and renderer.
- [ ] Add behavior fixtures and near misses for every source form in the issue acceptance criteria.
- [ ] Run focused tests, deterministic repeated-run checks, and the release performance/retained-memory campaign; retain its machine-readable evidence and decision note.

## Surprises & Discoveries

- Observation: `crates/bifrost-analysis/src/analyzer/semantic/cfg_algorithms.rs` already contains an iterative Cooper-Harvey-Kennedy dominator implementation, added for structural flow-state work after the #819 no-go record. Postdominators should reuse its dense graph conventions and budget/cancellation machinery but must not pretend that forward dominators answer the reverse multi-exit problem.
  Evidence: `dominators` computes over one forward entry, while `ProcedureSemantics` has distinct `normal_exit_point()` and `exceptional_exit_point()` accessors.

- Observation: every validated procedure has exactly one entry, one normal exit, and one exceptional exit, even when one or both exits are unreachable. Dead source is retained but `ProcedureCfgBuilder::seal_unreachable_regions` prevents an entry-unreachable region from reconnecting to live control or a real exit.
  Evidence: `find_boundaries` rejects any boundary count other than `[1, 1, 1]`; `seal_unreachable_regions` removes dead-to-live and dead-to-real-exit edges.

- Observation: a control edge always cites proven evidence, but its cited `EvidenceCompleteness` may be partial, and semantic gaps presently have no control-topology-specific impact variant.
  Evidence: `validate_control_edges` rejects `ProofStatus::Unproven`; `SemanticGapImpact` currently lists dispatch, call, return, value, heap, and aliasing concerns only.

- Observation: #819 measured reusable CFG algorithms and explicitly rejected persistence. Its harness already supplies deterministic synthetic and real-corpus measurement patterns, but its algorithm inventory predates the current dominance consumer.
  Evidence: `.agents/docs/issue-819-cfg-algorithm-benchmark-2026-07-24.md` says derived results are request-local and records timing, exact work, retained bytes, and digests across a synthetic matrix, VS Code, and Spring PetClinic.

- Observation: the #2101 plan reserves `SemanticRelationKind::ControlDependence` and defines the shared public edge/evidence/completeness model, but its initial v1 boundary list has no `non_exiting_region` variant.
  Evidence: `.agents/plans/issue-2101-semantic-relation-snapshots.md` lists `unsupported_relation`, `missing_semantics`, topology limits, and other acquisition boundaries but not a live region with no structural path to either exit. #2101 must add that typed vocabulary before #2102 can preserve this case without abusing `missing_semantics`.

## Decision Log

- Decision: Define ordinary, termination-insensitive procedure-local control dependence by postdominance on the CFG induced by entry-reachable points that can structurally reach either real exit, with one synthetic exit joined from the normal and exceptional exits.
  Rationale: The two real exits are alternative terminal outcomes of one procedure. Joining both into a synthetic exit gives a single postdominator root without conflating normal and exceptional branch evidence. "Termination-insensitive" here means that a loop with a modeled path to an exit is analyzed from graph paths even if some runtime executions may loop forever; this API does not attempt path feasibility.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not invent synthetic escape edges for an entry-reachable point that cannot reach either real exit. Exclude that region from postdominator computation, emit a `NonExitingRegion` boundary with its source-backed member or representative set, and mark the relation snapshot incomplete.
  Rationale: Connecting an arbitrary member of a terminal strongly connected component to the synthetic exit makes the answer depend on a fabricated edge; connecting every member changes postdominance inside the component. Explicit incompleteness preserves sound evidence and prevents an empty result from claiming absence. This covers truly nonterminating terminal SCCs and malformed live dead-ends.
  Date/Author: 2026-08-13 / Codex

- Decision: Ignore entry-unreachable points for relation emission, but report their count in work/diagnostic metadata rather than treating them as incomplete live topology.
  Rationale: Control dependence is relative to procedure entry. The builder deliberately retains dead source and seals it from live control, so unreachable source cannot govern a possible execution from entry. Reporting the count keeps the behavior inspectable.
  Date/Author: 2026-08-13 / Codex

- Decision: Derive one result edge for each `(controlling CFG edge, governed point)` pair, not merely each `(controller point, governed point)` pair.
  Rationale: An `if`, switch, exception arm, or cleanup arm can have parallel typed successors. Retaining the rich CFG edge identity preserves branch kind, source, evidence, proof, and completeness and avoids collapsing distinct causes that share endpoints.
  Date/Author: 2026-08-13 / Codex

- Decision: Use the standard edge-walk form of control dependence after computing immediate postdominators. For each real CFG edge `A -> B` where `B` does not postdominate `A`, walk `runner = B` upward through immediate postdominators until `runner == ipdom(A)` and emit `(A -> B, runner)` for each visited real point.
  Rationale: This is the direct Ferrante-style relation over the immediate-postdominator tree, works for branches and loop exits, and naturally preserves the controlling rich edge. The walk is iterative and bounded. Synthetic-exit nodes and synthetic join edges are algorithm machinery and are never emitted.
  Date/Author: 2026-08-13 / Codex

- Decision: Treat any control-topology-affecting gap, partial control-edge evidence, cancellation, or algorithm/snapshot limit as relation incompleteness. Add a control-topology gap impact if #2101 has no equivalent typed boundary selector.
  Rationale: A missing successor can change both postdominators and the derived relation globally within the procedure. Returning locally plausible edges is useful as a partial result, but neither that set nor an empty set is authoritative.
  Date/Author: 2026-08-13 / Codex

- Decision: Keep postdominators and control dependence request-local. Do not add them to `SemanticArtifact`, SQLite, a global cache, or persisted snapshot state.
  Rationale: The immutable CFG is already compact. #819 established compute-on-demand as the lifecycle default; #2102 must first measure repeated cost and retained memory against a named consumer before proposing persistence.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Planning and source audit are complete. The repository has the required validated bidirectional CFG substrate, typed rich control edges, stack-safe algorithm controls, two real exits, and bounded renderers. It does not yet have postdominators, control-dependence result types, a control-topology gap impact, or #2101's public snapshot seam in this checkout. Implementation therefore remains pending and must begin only after the #2101 contract names and integration point are available.

## Context and Orientation

All paths are relative to the Bifrost repository root.

`crates/bifrost-analysis/src/analyzer/semantic/ir/model.rs` defines language-neutral program points and rich `ControlEdge` rows. A rich edge carries source and target point IDs, a `ControlEdgeKind` such as `ConditionalTrue`, `SwitchCase`, `Exceptional`, or `Cleanup`, a source mapping, and an evidence row. `crates/bifrost-analysis/src/analyzer/semantic/ir/artifact.rs` freezes those rows into `ControlFlowGraph`, whose outgoing and incoming adjacency are deterministic and share the same edge table. A `ProcedureSemantics` exposes the immutable graph plus its entry, normal-exit, and exceptional-exit point IDs.

`crates/bifrost-analysis/src/analyzer/semantic/cfg_algorithms.rs` contains stack-safe request-local graph algorithms. `DenseBidirectionalGraph` abstracts dense nodes, rich edge identities, canonical successors, canonical predecessors, and endpoint lookup. `CfgAlgorithmRequest` owns independently bounded node and edge visits plus cancellation. Existing algorithms return no partial result on cancellation or budget failure. `dominators` is a forward one-entry implementation; it is useful as a coding and testing pattern, but postdominance requires a reverse view with a synthetic root and explicit handling of points that cannot reach either exit.

A node `P` postdominates node `N` when every finite CFG path from `N` to either procedure exit passes through `P`. The immediate postdominator of `N` is the closest strict postdominator of `N`. A governed point is control-dependent on a controlling CFG edge when choosing that edge can determine whether the governed point is encountered. The relation is structural and may describe possible control; it does not claim runtime causality or path feasibility.

`crates/bifrost-analysis/src/analyzer/semantic/icfg.rs` demand-builds bounded interprocedural snapshots and demonstrates how proof, completeness, work, limits, and boundaries survive projection. Control dependence remains procedure-local: callers may request it for procedures reached through #2101, but the postdominator algorithm must run on each underlying `ProcedureSemantics`, never on call-context-expanded ICFG nodes. This prevents call-context duplication from changing the mathematical relation.

`crates/bifrost-analysis/src/analyzer/semantic/render.rs` provides deterministic bounded S-expression-like rendering for semantic IR and ICFG snapshots. Extend the #2101 relation renderer, or this module if #2101 deliberately owns rendering here, rather than creating an unbounded debug printer.

`crates/bifrost-analysis/src/analyzer/semantic/cfg_algorithms/benchmark.rs` and `.agents/docs/issue-819-cfg-algorithm-benchmark-2026-07-24.md` are the measurement model. Tests should reuse `tests/common/inline_project.rs` for small language fixtures. New integration tests belong below an existing `tests/<suite>/` harness, with one `mod` entry in that harness's `main.rs`; do not add a root `tests/*.rs` binary unless process isolation is demonstrably necessary.

Issue #2102 depends directly on #2101's semantic relation request/result contract. It does not depend on #2103 value dependence, #2104 observation mapping, #2105 manifests, the public template, or Apache-2.0 migration. Those later consumers may request this relation but must not shape its semantics.

## Plan of Work

### Milestone 1: add a bounded postdominator primitive

In `crates/bifrost-analysis/src/analyzer/semantic/cfg_algorithms.rs`, add a private graph view that reverses a `DenseBidirectionalGraph` and adds one synthetic node. Its outgoing edges in the reversed view correspond to incoming real CFG edges; the synthetic node has deterministic synthetic edges to the normal and exceptional exits that are reachable from entry. Do not allocate a copied real graph and do not expose the synthetic node or edge through public semantic APIs.

Before building the view, compute forward reachability from the procedure entry and reverse reachability from both real exits. A live point is one reachable from entry. An analyzable point is live and can reach at least one exit. Entry-unreachable points are outside the relation. A live point that cannot reach an exit makes the outcome incomplete and belongs to a deterministic non-exiting-region boundary. Group these points with the existing iterative SCC algorithm so the boundary can describe terminal cyclic regions and singleton dead ends without recursion; the algorithm may still return partial dependence edges for analyzable points.

Generalize the current Cooper-Harvey-Kennedy implementation only if doing so reduces real duplication without Boolean mode parameters. Prefer a separate `postdominators` entry point and a small shared private fixed-point helper over a mode flag. The result retains dense immediate-postdominator parents for real analyzable points, total work, and the synthetic-root parent internally. It offers iterative `immediate_postdominator` and reflexive `postdominates` queries. Real exits should have the synthetic exit as their internal parent and therefore report no real immediate postdominator; the synthetic node itself is never accepted by consumer methods.

Every traversal, fixed-point pass, parent-chain intersection, SCC grouping, and canonical emission must charge node/edge work and check cancellation. If cancellation or the CFG algorithm budget is exceeded, return the existing typed `CfgAlgorithmError` with no falsely complete result. Test a 100,000-point chain and a deep parent-chain query to prove stack safety.

Milestone acceptance is an algorithm-level suite that computes expected immediate postdominators for a straight line, diamond, nested diamond, loop with exit, switch fan-out, distinct normal and exceptional exits, an unreachable component, and a non-exiting SCC. Permuting input edges must leave real results and work counts deterministic.

### Milestone 2: derive typed control-dependence rows and prove them independently

Add request-local production types near the postdominator implementation or in a tightly related `control_dependence.rs` module if the code is no longer cohesive. The result owns canonical rows keyed by the controlling rich `ControlEdgeId` and governed `ProgramPointId`, an index suitable for deterministic iteration, algorithm work, entry-unreachable count, and typed incomplete boundaries. Do not add a field to `ProcedureSemantics`.

For each real edge `A -> B` between analyzable points, skip it when `B` postdominates `A`. Otherwise walk from `B` through real immediate postdominators until the immediate postdominator of `A`. Emit each visited governed point with the original edge ID. If the walk reaches the synthetic root before the expected stop, fail an assertion in debug builds and return a typed contract error in production; a validated and correctly constructed postdominator tree cannot do that. Canonically sort rows by controller point, rich edge order, then governed point, and deduplicate only exact duplicate rows.

Implement the independent test oracle only under `#[cfg(test)]` and do not call production postdominators. For graphs of up to six real nodes, enumerate directed edge subsets bounded to a practical count, choose distinct entry/normal-exit/exceptional-exit nodes, and retain cases where all live nodes reach an exit. The oracle enumerates all simple finite paths from each node to either exit with an explicit stack and a visited bitset, computes a node's postdominator set by intersecting nodes on all enumerated paths, and defines edge control dependence directly: governed `Y` is dependent on edge `A -> B` when `Y` postdominates `B` and does not strictly postdominate `A`, with the standard immediate-tree frontier expansion checked against the production rows. Keep this implementation intentionally set-based and structurally unlike Cooper-Harvey-Kennedy.

Exhaustively compare production and oracle postdominator sets and control-dependence rows. Include parallel rich edges with distinct labels in hand-authored tests because a simple adjacency oracle alone cannot prove evidence preservation. Seeded larger random graphs may supplement exhaustive enumeration but cannot replace it. Persist any discovered counterexample as a minimal named regression fixture.

Milestone acceptance is exact equality for all retained generated graphs, deterministic results across repeated/permuted runs, and typed failures for invalid boundaries, cancellation, and each work dimension.

### Milestone 3: project through the bounded semantic snapshot API

After #2101 lands, implement its already reserved `SemanticRelationKind::ControlDependence` producer and extend its finite request limits only if existing node, edge, work, diagnostic, and byte limits cannot account for this derivation. Add `non_exiting_region` to `SemanticRelationBoundary`'s v1 typed vocabulary as part of #2101 if it is not present in source. A request seeded at a source-backed procedure or point resolves the owning `ProcedureHandle`, materializes that validated procedure, runs the procedure-local algorithm once per artifact-scoped procedure handle, and maps local points and the controlling edge to #2101 stable semantic node and evidence identities. If multiple call-context nodes in a bounded ICFG refer to the same procedure point, emit one stable semantic row because #2101 defines call context separately from stable semantic identity; only add occurrence-context projections if the request explicitly selects bounded-call occurrences. Never recompute postdominance over the ICFG.

Each public relation row must retain the controlling stable point, governed stable point, original branch/control kind, controlling source mapping, controlling evidence, proof, and completeness. Because validated CFG edges are proven, `proof` is normally `Proven`; do not hard-code that assumption into serialization. Completeness is the worst of the procedure artifact, relevant topology gaps, edge evidence, non-exiting-region state, and request limits.

Add a control-topology-specific `SemanticGapImpact` if no #2101 relation boundary can select gaps that may add or remove successors. Update the declarative total list, mandatory impact construction, renderer labels, and validation tests together. A topology-affecting gap yields an incomplete relation outcome even when production can emit partial rows. Unsupported normal, exceptional, cleanup, async, or non-local control capability must be represented according to the adapter capability table and gaps; it must not silently delete an arm.

Extend the compact bounded renderer to show relation kind, stable controller/governed source identity, branch kind, evidence, proof, completeness, work, and every boundary. Renderer truncation is separate from semantic acquisition truncation and must say which occurred. Render output must be deterministic and balanced at every byte limit.

Milestone acceptance is semantic equivalence between the in-process Rust result and #2101 canonical JSON/JSONL forms, including complete empty, incomplete empty, partial nonempty, cancelled, budget-exceeded, and stale-generation cases.

### Milestone 4: add behavior fixtures and lifecycle evidence

Use at least two existing language adapters with complete relevant control-flow capability, preferably Java and TypeScript because the #819 corpus evidence already covers those ecosystems. Add small inline positive and near-miss fixtures:

For `if/else`, prove each arm depends on its own typed predicate edge and the join does not. For nested branches, prove inner points preserve both their inner controlling edge and the applicable outer edge. For a loop with an exit, prove the body depends on the loop-entering edge and the post-loop point does not; include a `break` near miss whose successor topology must not be mistaken for an additional predicate. For switch or match, preserve distinct case-edge evidence, including two cases converging on one governed point. For early return, prove points on the continuing arm are governed while the common synthetic exit and unrelated postdominating points are not. For exceptions, prove handler/finally topology follows `Exceptional` and `Cleanup` rich edges and keep the normal arm distinct. For multiple exits, include paths to both normal and exceptional exits and assert the synthetic join is not rendered. Include entry-unreachable dead source and an entry-reachable non-exiting terminal SCC.

Near misses must include a straight-line CFG, a branch whose arms immediately reconverge without distinct governed work, sequential independent branches, source nesting with no corresponding conditional CFG edge, and a control-topology gap that produces zero rows but an incomplete outcome. Fixtures should assert stable source ranges and evidence, not only counts.

Extend the ignored release benchmark using the #819 harness patterns. Add postdominators and control dependence as separately labeled algorithms. Retain exact Bifrost commit and tree fingerprint, toolchain/OS/architecture, dataset revisions, cold time, repeated recomputation time, node and edge visits, emitted relation rows, retained shallow bytes, and a canonical SHA-256 result digest. Run the existing synthetic matrix plus the pinned VS Code and Spring PetClinic corpora. Add a branch-heavy generated graph, a 100,000-node chain, nested reducible loops, an irreducible SCC, exceptional/multiple exits, entry-unreachable points, and a non-exiting terminal SCC. Require identical digest, work, boundary counts, and retained-byte estimate across one cold run and at least three complete recomputations.

Write a dated machine-readable JSON artifact and companion decision note under `.agents/docs/`. The decision note must compare request-local compute/retained costs with snapshot size and observed repetition. Persistence remains rejected unless measured repetition and cost satisfy the promotion gates in `.agents/docs/semantic-artifact-lifecycle-matrix.md`; this issue does not itself authorize persistence.

## Concrete Steps

Work from the Bifrost repository root. First inspect the merged #2101 types and update the provisional interface names below. Record the exact #2101 commit and any changed assumptions in this plan.

Implement and format the algorithm slice, then run:

    cargo fmt
    cargo test -p brokk-bifrost-analysis cfg_algorithms

Expect all existing CFG algorithm tests plus the new postdominator, oracle, cancellation, budget, determinism, and stack-safety tests to pass. Use the actual package name from `crates/bifrost-analysis/Cargo.toml` if it differs at implementation time.

Implement projection and focused behavior tests, then run the owning integration suite. Discover the exact suite command from its harness and record it here; do not claim the whole feature from only unit tests. A typical command will resemble:

    cargo test --test semantic_ir -- control_dependence

Run the renderer tests and #2101 serialization parity tests with exact names once those names exist. Run the same fixture twice in fresh processes and compare canonical output bytes and hashes.

Run the release benchmark in an isolated target, with pinned corpus paths supplied through the existing `BIFROST_SEMANTIC_TS_REPO` and `BIFROST_SEMANTIC_JAVA_REPO` variables and a new issue-specific output path under `.agents/docs/`. Check available disk first. Do not enable `nlp`; this feature does not use it. Record the exact command, corpus commits, wall times, work, retained bytes, and output hash in this plan and the companion decision note.

Before pushing any Rust implementation, run:

    scripts/pre-push-gate.sh

If running an individual Clippy command is necessary, use:

    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Do not create a manually named Cargo target directory. Do not add ignore annotations to bypass failures.

## Validation and Acceptance

Issue #2102 is complete only when all of the following have direct evidence.

The production algorithm is iterative, cancellation-aware, independently bounded by node and edge work, deterministic, and stack-safe on at least 100,000 points. The exhaustive oracle agrees on every retained small graph and does not reuse the production implementation or its immediate-postdominator relation.

The specified relation holds for if/else, nested branches, loops, switch/match, early returns, exceptions, cleanup/finally, and multiple normal/exceptional exits in source-backed fixtures. Straight-line flow, reconverged arms, sequential independent branches, and mere lexical nesting remain near misses. Parallel typed branch edges retain distinct evidence.

Every public row preserves stable controller and governed identities, original controlling edge kind/source/evidence, proof, and completeness. Synthetic exit machinery is absent from the public result and renderer. Entry-unreachable points emit no relation rows. Entry-reachable points that cannot reach a real exit produce a typed non-exiting boundary and incomplete outcome.

A complete empty relation is constructible only when the relevant procedure topology and capabilities are complete, acquisition and derivation finish within limits, and no live non-exiting region exists. Any relevant gap, unsupported capability, partial edge evidence, stale generation, cancellation, or node/edge/work/diagnostic/byte limit yields an explicitly incomplete or terminal typed outcome. An incomplete empty result serializes differently from a complete empty result.

The relation is available through #2101's bounded semantic snapshot Rust API and canonical JSON/JSONL model. Direct and serialized results are semantically equivalent and deterministically ordered. The compact renderer is bounded, balanced, source-backed, and shows acquisition versus rendering truncation distinctly.

The dated benchmark evidence records performance, work, retained memory, boundaries, determinism, and provenance on synthetic and pinned real corpora. No persistence, global cache, artifact field, or database row is introduced without a separate evidence-backed lifecycle decision.

The focused unit and integration suites pass, `cargo fmt` is clean, and the full pre-push gate passes before any implementation push. The issue/PR description links the oracle evidence, behavior tests, benchmark artifact, and exact validation commands.

## Idempotence and Recovery

All derivation is read-only over an immutable validated semantic artifact and is safe to repeat. Cancellation and budget exhaustion return typed incomplete outcomes and do not mutate the artifact. Keep generated benchmark output in a temporary file until a complete run validates its schema and hash, then atomically move it to the dated `.agents/docs/` path.

If exhaustive graph enumeration becomes too slow, reduce the enumerated graph size or constrain edge counts while preserving complete enumeration of the declared domain; do not silently replace the oracle with random testing. Record the exact enumerated domain and count. If a counterexample appears, retain the smallest graph as a named unit test before fixing production.

If #2101's merged contract cannot express per-edge evidence, incomplete boundaries, or procedure-local stable identity, stop projection work and amend #2101 rather than adding a private side channel. Algorithm work can remain crate-internal and tested while the contract is corrected. If a language fixture exposes missing structured CFG support, preserve the incomplete result and fix or separately track the adapter gap; do not infer control dependence from source text.

## Artifacts and Notes

Current source evidence was inspected at Bifrost commit `4496c7f95` in a detached worktree. The only existing untracked file was the epic plan `.agents/plans/issue-2099-extension-sdk-epic.md`; this plan does not modify it.

The intended rendered shape is conceptually:

    (semantic-relation-snapshot
      :relation "control_dependence"
      :complete true
      (edge
        :controller <stable-source-backed-node>
        :governed <stable-source-backed-node>
        :control-kind "conditional_true"
        :control-source <source-mapping>
        :evidence <evidence>
        :proof "proven"
        :completeness "complete"))

For a non-exiting region, the same envelope instead includes an incomplete marker and a typed boundary such as `non_exiting_region`; it must not render `:complete true` merely because no dependence row was emitted.

## Interfaces and Dependencies

The public envelope names below align with the #2101 ExecPlan. Confirm them against merged source before implementation; they are prescriptive semantic shapes, not permission to create a parallel API.

In `crates/bifrost-analysis/src/analyzer/semantic/cfg_algorithms.rs` or a cohesive adjacent private module, provide equivalents of:

    pub(crate) struct Postdominators<Node>;

    pub(crate) fn postdominators<G>(
        graph: &G,
        entry: G::Node,
        normal_exit: G::Node,
        exceptional_exit: G::Node,
        request: &mut CfgAlgorithmRequest<'_>,
    ) -> Result<PostdominatorOutcome<G::Node>, CfgAlgorithmError<G::Node>>
    where
        G: DenseBidirectionalGraph;

    pub(crate) struct ControlDependenceRow<Edge, Node> {
        pub(crate) controlling_edge: Edge,
        pub(crate) governed: Node,
    }

    pub(crate) struct ControlDependenceResult<Edge, Node> {
        pub(crate) rows: Box<[ControlDependenceRow<Edge, Node>]>,
        pub(crate) boundaries: Box<[ControlDependenceBoundary<Node>]>,
        pub(crate) unreachable_points: usize,
        pub(crate) work: CfgAlgorithmWork,
    }

    pub(crate) fn control_dependence<G>(
        graph: &G,
        entry: G::Node,
        normal_exit: G::Node,
        exceptional_exit: G::Node,
        request: &mut CfgAlgorithmRequest<'_>,
    ) -> Result<ControlDependenceResult<G::Edge, G::Node>, CfgAlgorithmError<G::Node>>
    where
        G: DenseBidirectionalGraph;

`PostdominatorOutcome` must distinguish a fully exit-reaching graph from one with live non-exiting regions while retaining results for the analyzable subgraph. `ControlDependenceBoundary` must include at least `NonExitingRegion`; #2101 owns the outer limit, cancellation, unsupported, stale-generation, and serialization boundary vocabulary.

Through #2101, `SemanticRelationKind::ControlDependence` must select this derivation and emit `SemanticRelationEdge` values with `kind = control_dependence`. The edge's `source` is the controller occurrence, `target` is the governed occurrence, and `subtype` is the concrete `ControlEdgeKind`. Its evidence payload must be equivalent to:

    pub struct ControlDependenceEvidence {
        pub controller: StableSemanticNodeId,
        pub governed: StableSemanticNodeId,
        pub control_kind: ControlEdgeKind,
        pub control_source: StableSourceIdentity,
        pub evidence: RelationEvidence,
        pub proof: ProofStatus,
        pub completeness: EvidenceCompleteness,
    }

Do not expose `ProgramPointId`, `ControlEdgeId`, `ProcedureId`, `SemanticArtifact`, dense ICFG IDs, stores, language modules, MCP, or LSP in the extension contract. The projection depends downward on validated `ProcedureSemantics` and #2101 stable identities. Neither Bifrost nor this algorithm depends on an external extension or fault-localisation repository.

Plan revision note (2026-08-13): Created the initial issue-specific ExecPlan after inspecting the live issue and current CFG/ICFG implementation. The plan fixes a synthetic two-exit join, explicit non-exiting-region incompleteness, rich-edge-preserving Ferrante-style derivation, a structurally independent exhaustive path oracle, #2101-only public integration, and evidence-gated request-local lifecycle.
