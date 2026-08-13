# Expose bounded source-backed value dependence

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub issue [#2103](https://github.com/BrokkAi/bifrost/issues/2103), an API-only child of epic #2099. It assumes #2100 supplies the stable extension workspace/version/generation boundary and #2101 supplies bounded semantic relation requests, snapshots, stable node occurrences, evidence, boundaries, diagnostics, work accounting, and canonical JSON/JSON Lines. If their merged names differ from the illustrative names here, adapt module paths while preserving every semantic invariant in this plan.

## Purpose / Big Picture

After this change, an independent extension can ask which source-backed definitions or carriers may influence which source-backed uses or observations inside a bounded semantic snapshot. The result adds `value_dependence` edges to the same graph returned for control flow, calls, returns, and control dependence. Each edge says what kind of transfer contributed, which source and target occurrences it connects, whether the may-flow is proven or uncertain, whether discovery is complete, and which semantic events, call bindings, heap/access-path facts, summaries, and solver witness support it.

The extension does not construct a `ValueFlowPlan`, choose policy-specific source and sink labels, import run-local carrier IDs, or parse source text. It selects source-backed seeds, scope, direction, relation kinds, and finite limits through #2101. Bifrost automatically derives definition and observation events from its validated semantic intermediate representation, reuses its demand value-flow provider, interprocedural control-flow graph, call bindings, heap/access-path model, summaries, and bounded solver, then projects the result to stable public identities.

The behavior is visible in checked-in fixtures. For a small function containing assignments, a branch merge, an overwrite, field and index stores/loads, and a return, a canonical snapshot shows only definitions that may reach each use. A complete empty result means no matching influence exists within the requested scope. Ambiguous dispatch, unmodeled calls, summary-authored proof, alias uncertainty, unsupported constructs, cancellation, and limits instead produce typed incomplete boundaries, so missing edges never masquerade as exhaustive absence.

## Progress

- [x] (2026-08-13 14:20Z) Read `.agents/PLANS.md`, the #2099 epic plan, the #2101 semantic relation plan, live issue #2103, and its empty comment thread.
- [x] (2026-08-13 14:45Z) Audited the existing semantic value-flow relation model, stable carrier keys, demand provider/cache, plan construction, call bindings, heap/access paths, summary integration, solver uncertainty, completion logic, result meetings, fixtures, and measurement tests at commit `4496c7f95`.
- [x] (2026-08-13 15:10Z) Fixed the contract as source-backed may-dependence occurrences produced through the #2101 snapshot model, distinct from raw carrier-transfer rows and from policy-specific source/sink queries.
- [ ] Add stable public value-dependence subtypes, occurrence roles, evidence kinds, boundaries, request refinements, and validators to the #2101 model.
- [ ] Implement automatic source/use observation discovery and bounded intraprocedural may-dependence through existing semantic events and value-flow transfer machinery.
- [ ] Add precise overwrite/merge and heap/access-path treatment with typed alias and unsupported boundaries.
- [ ] Add bounded interprocedural parameter, receiver, normal/exceptional return, call-result, and summary projections only where existing ICFG/binding/summary evidence proves the advertised quality.
- [ ] Integrate canonical JSON/JSONL, direct-versus-serialized parity, behavior fixtures, near misses, property/reference checks, and lifecycle measurements.
- [ ] Run focused featureless tests, dependency checks, formatting, package seam validation when applicable, and the pre-push gate before any authorized push.
- [ ] Record commits, test counts, canonical hashes, measurement evidence, PR/CI state, and issue closure evidence here.

## Surprises & Discoveries

- Observation: `ValueFlowSnapshot` is already a validated, bounded procedure-local transfer graph, but it is not itself a public value-dependence answer.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/oracle/value_flow.rs` stores relations such as assignment, parameter, memory load, and memory store between `ValueFlowEndpoint` carriers at one semantic event. It does not pair a reaching definition occurrence with a later use occurrence.

- Observation: Stable carrier identity already exists independently of run-local dense IDs.
  Evidence: `crates/bifrost-analysis/src/analyzer/value_flow/model.rs` defines `ValueFlowCarrierKey` over semantic locators, ports, allocations, call results, scoped roots, and bounded access-path selectors. `ValueFlowCarrierId`, `ValueFlowSourceId`, and `ValueFlowSinkId` are explicitly run-local dense IDs.

- Observation: The existing policy-free solver already propagates a source-sensitive fact across local relations, calls, returns, summaries, and incomplete boundaries, and deliberately establishes only may-flow.
  Evidence: `value_flow/client.rs` carries `ValueFlowSourceId` through `ValueFlowCarrierId`; uncertain proof/completeness marks the fact uncertain. `value_flow/result.rs` exposes `ValueFlowMayStatus::{Proven,Unproven}` while `ValueFlowMustStatus` has only `NotEstablished`.

- Observation: Current client construction expects caller-supplied `ValueFlowSourceSpec` and `ValueFlowSinkSpec`; exposing that plan would make each extension encode a policy and violate #2103.
  Evidence: `ValueFlowPlan::try_new` accepts explicit sources and sinks, and tests build them by selecting semantic relations. #2103 instead needs an internal automatic observation inventory whose result is the extension graph.

- Observation: Completion already distinguishes discovery input status, coverage, ambiguous dispatch, source/sink evidence, fixed-point termination, and authored summaries.
  Evidence: `ValueFlowIncompleteCause` identifies snapshot, binding, coverage, and evidence causes; `ValueFlowSummaryResult::sink_outcome` returns `NotReached` only when discovery is complete and otherwise returns `Inconclusive`; `is_proven_by_authored_summaries` is separate from derived completeness.

- Observation: Heap semantics already preserve exact versus summary access paths, point-sensitive aliasing, strong-update justification, and weak-update reasons.
  Evidence: `semantic/oracle/model.rs` defines root-plus-selector `AccessPath` with `Exact` or `Summary` tail; `semantic/workspace_oracle/heap.rs` validates alias exclusivity and records why a store must use a weak update. This must be projected, not reimplemented textually.

- Observation: The value-flow cache retains only complete content-keyed procedure snapshots and call bindings; incomplete results are never published as ready cache entries.
  Evidence: `value_flow/provider.rs` keys snapshots and bindings by semantic artifact fingerprints plus procedure/call identities and publishes only `SemanticOutcome::Complete`.

- Observation: Existing language parity covers twelve production adapters, but individual relation subtypes remain capability-dependent.
  Evidence: `DIRECT_VALUE_FLOW_READY_LANGUAGES` lists Java, Go, C++, JavaScript, TypeScript, Python, Rust, PHP, Scala, C#, Ruby, and Kotlin; semantic capability tables and gaps still determine whether particular heap, call, exceptional, capture, or language-defined behavior is complete.

- Observation: Bifrost MCP code-intelligence tools advertised by the installed skills were unavailable in this task, so the audit used bounded `rg` and `sed` reads.
  Evidence: the available tool inventory exposed no Bifrost search, summary, or source calls.

## Decision Log

- Decision: Define public value dependence between source-backed occurrences, not between bare carriers and not between arbitrary policy labels.
  Rationale: One carrier can be defined and observed at several program points. A carrier-only edge loses overwrite, merge, phase, and evidence semantics; a caller-selected source/sink plan merely republishes internal policy machinery. Occurrences preserve the exact defining/using event and remain useful across extension domains.
  Date/Author: 2026-08-13 / Codex

- Decision: A value-dependence edge is a may-dependence claim only. The API never emits must-dependence, nondependence, path feasibility, or kill proof.
  Rationale: The current solver proves may reachability and intentionally reports `ValueFlowMustStatus::NotEstablished`. Strong updates can remove old definitions from the propagated state, but that does not establish a general must-flow contract.
  Date/Author: 2026-08-13 / Codex

- Decision: Reuse #2101 `SemanticRelationEdge { kind: ValueDependence }`; add a structured `ValueDependenceSubtype` and value-dependence evidence records rather than a parallel result type or codec.
  Rationale: One graph permits downstream joins with control flow and control dependence, one completeness model, one canonical ordering, and one JSON/JSONL schema. A second graph would create identity and boundary drift.
  Date/Author: 2026-08-13 / Codex

- Decision: Intraprocedural value dependence is the required stable baseline. Interprocedural projections are additive and enabled only for relation families whose call bindings, ICFG continuations, and summaries provide source-backed evidence under the request limits.
  Rationale: Issue #2103 explicitly requires a precisely specified local relation and conditional bounded interprocedural support. Unsupported or incomplete call families must remain boundaries instead of being guessed.
  Date/Author: 2026-08-13 / Codex

- Decision: Automatic observation discovery treats semantic events as definitions, uses, or both according to one declarative registry beside the semantic effect vocabulary.
  Rationale: Assignment, parameter, receiver, return, load/store, call result, capture, and language-defined operations must be interpreted consistently by execution, validation, documentation, and tests. Private duplicated match lists would drift. The registry consumes structured `SemanticEffect`, `ValueFlowRelationKind`, ports, phases, and source mappings; it never scans text.
  Date/Author: 2026-08-13 / Codex

- Decision: Definition occurrence identity includes stable carrier key, stable semantic program-point occurrence, observation phase, event ordinal, role, and generation/artifact validity. Use occurrence identity uses the same ingredients with a use role.
  Rationale: The same carrier before and after effects, or at two events, represents different semantic observations. Dense carrier, point, source, and sink IDs are local aliases and cannot be serialized as stable identity.
  Date/Author: 2026-08-13 / Codex

- Decision: Preserve both transfer evidence and acquisition completeness. An edge’s proof is the weakest proof on its witnessed propagation; snapshot status is incomplete if any selected route, requested direction, or relevant definition/use inventory remains open.
  Rationale: A genuine may-flow can be supported by partial evidence, while unrelated open discovery can make the result nonexhaustive. Combining the concepts would either overstate the edge or erase useful partial results.
  Date/Author: 2026-08-13 / Codex

- Decision: Exact strong updates kill prior definitions only when existing heap/update evidence proves exclusivity. Weak updates retain old and new definitions and mark alias/access-path approximation in evidence.
  Rationale: Overwrite fixtures must reflect real update semantics without claiming whole-program points-to or replacing the heap oracle. Summary tails, wildcard indices, ambiguous objects, or nonexclusive aliases cannot justify a strong kill.
  Date/Author: 2026-08-13 / Codex

- Decision: External authored summaries can support an edge but cannot be labeled derived proof. The edge records `external_summary` evidence and authored provenance; result status distinguishes derived-complete from complete-under-authored-summaries.
  Rationale: Current value-flow results already separate these cases. Flattening them to “proven complete” would misrepresent the origin of the claim and impede reproducibility.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not persist value-dependence snapshots in #2103. Reuse complete content-keyed substrate caches and measure cold/warm/retained/output costs under the existing evidence protocol.
  Rationale: Epic issue #2099 and prior persistence work require promotion evidence before new persistence. A public API must not turn internal cache reuse into a storage guarantee.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Planning is complete; no implementation exists. The principal outcome is a stable definition of value dependence as bounded source-backed may influence, produced automatically from existing semantic structure and returned through #2101. The implementation must still prove automatic observation semantics, overwrite/merge correctness, heap uncertainty, interprocedural boundaries, serialization parity, and bounded cost before #2103 can close.

## Context and Orientation

All paths are relative to the Bifrost repository root.

`crates/bifrost-analysis/src/analyzer/semantic/ir/` contains Bifrost’s validated language-neutral semantic intermediate representation. A program point is a position in a procedure’s execution graph. Each point contains ordered semantic events such as assignment, value flow, allocation, memory load, memory store, invocation, capture, return, and throw. Adapters attach exact source mappings, evidence, proof, completeness, and semantic gaps.

`crates/bifrost-analysis/src/analyzer/semantic/oracle/value_flow.rs` projects a procedure’s events into `ValueFlowSnapshot`. A `ValueFlowRelation` is a direct transfer at one event between endpoints. Its kinds are assignment, parameter, receiver, normal return, exceptional return, allocation, memory load, memory store, capture, and language-defined flow. Its endpoints are semantic values, procedure ports, or abstract memory locations. Each row includes the exact point/event ordinal, an evidence-backed oracle relation, proof, and completeness.

An abstract memory location is not a concrete runtime address. `semantic/oracle/model.rs` describes it with an abstract object and a bounded access path: a root plus field or index selectors. An exact access path names all known selectors; a summary path admits unknown or omitted selectors. `semantic/workspace_oracle/heap.rs` answers point-sensitive location, alias, and update questions. A strong update proves one store overwrites the prior contents of one exclusive abstract location. A weak update means other aliases or locations may remain, so old and new definitions both remain possible.

`crates/bifrost-analysis/src/analyzer/value_flow/model.rs` converts internal endpoints to `ValueFlowCarrier` and stable `ValueFlowCarrierKey`. The stable key uses semantic locators, procedure ports, allocations, call results, scoped memory roots, and structured access-path selectors. The dense `ValueFlowCarrierId`, `ValueFlowSourceId`, and `ValueFlowSinkId` exist only inside one plan/solve.

`crates/bifrost-analysis/src/analyzer/value_flow/provider.rs` demand-materializes procedure snapshots and call bindings. `WorkspaceValueFlowProvider` is bound to one workspace analyzer generation and a shared `ValueFlowCache`. Cache keys use content-addressed semantic artifact identities. Only complete results are retained; incomplete, cancelled, or budget-exceeded acquisition is returned to the caller and not cached as complete.

`crates/bifrost-analysis/src/analyzer/value_flow/plan.rs` validates and canonicalizes snapshots, call bindings, explicit source/sink observations, external summaries, curated call models, fallback call behavior, and limits. It records the first typed incomplete discovery cause and prevents incomplete discovery from becoming a clean negative. `client.rs` supplies a source-sensitive distributive data-flow problem over the existing summary/ICFG solver. `result.rs` turns solver meetings into proven or unproven may-flow outcomes, retains witness reconstruction, and reports `NotReached` only after complete discovery.

The current client is policy-free in transfer semantics but still expects a caller to identify source and sink events. #2103 adds an extension producer that inventories all relevant definitions and uses inside the bounded request scope. A definition occurrence is a source-backed event that creates or writes a carrier state. A use occurrence is a source-backed event that observes or consumes a carrier state. One event may be both, such as a compound read-modify-write. A value-dependence edge means the source definition may reach the target use under the modeled control, call, heap, and summary semantics.

#2101 owns `SemanticRelationRequest` and `SemanticRelationSnapshot`. A request contains source-backed seeds, finite scope, direction, relation kinds, and budgets. A snapshot contains stable node occurrences, typed edges, boundaries, diagnostics, work, generation, proof, completeness, evidence, and canonical serialization. #2103 must extend this model; it must not expose `ValueFlowPlan`, `ValueFlowCarrierId`, `SummaryDataflowResult`, semantic artifact `Arc`s, stores, language modules, MCP, or LSP.

## Plan of Work

### Milestone 1: extend the portable relation contract

In the #2101 public extension model, keep `SemanticRelationKind::ValueDependence` and add a structured `ValueDependenceSubtype`. Stable subtypes are `assignment`, `parameter`, `receiver`, `normal_return`, `exceptional_return`, `allocation`, `field_store`, `field_load`, `index_store`, `index_load`, `static_store`, `static_load`, `capture`, `call_argument`, `call_receiver`, `call_result`, `summary_transfer`, `merge`, and `language_defined`. A subtype describes the contributing transfer family, not a claim of exclusivity. If one dependence path uses several families, retain a canonical nonempty ordered chain of contributing subtypes rather than selecting one arbitrarily.

Add `ValueOccurrenceRole::{Definition,Use,Observation}` to the public semantic node occurrence. A definition/use occurrence contains stable semantic node ID, stable carrier identity, program-point identity, `BeforeEffects` or `AfterEffects`, semantic event ordinal, exact source mapping or structured synthetic mapping, and a canonical role discriminator. A carrier identity is an opaque public digest plus a structured display projection sufficient to distinguish value, port, allocation, call result, and access path; decoding validates the digest from that projection. Do not expose internal handles or dense IDs.

Add evidence kinds for `semantic_value_flow`, `call_binding`, `icfg_transfer`, `heap_location`, `alias_relation`, `update_strength`, `procedure_summary`, `curated_call_model`, and `solver_witness`. Every value-dependence edge has at least one semantic transfer record and one bounded witness record. The witness is a canonical sequence of stable occurrence IDs and contributing transfer/call/summary evidence; it is not a dump of solver facts or worklists. If witness reconstruction exceeds its own positive limits, retain the edge as unproven/partial with a typed `witness_limit` boundary.

Extend #2101 limits with positive `max_value_definitions`, `max_value_uses`, `max_value_dependence_edges`, `max_value_flow_carriers`, `max_value_flow_relations`, `max_summary_applications`, `max_solver_steps`, `max_solver_facts`, `max_witness_steps`, `max_access_path_selectors`, and `max_alias_candidates`. Map them to existing plan, semantic, oracle, solver, and witness ledgers. The request remains invalid if any dimension is zero or exceeds the package’s declared hard ceiling. Each seed shares the same request ledger; seeds cannot reset work.

Add typed boundary kinds `value_inventory_limit`, `value_flow_relation_limit`, `value_flow_carrier_limit`, `solver_fact_limit`, `solver_step_limit`, `summary_application_limit`, `witness_limit`, `alias_uncertainty`, `access_path_summary`, `weak_update`, `unmodeled_call`, `incompatible_summary`, and `value_flow_unsupported`. Boundaries state affected direction/subtype, source occurrence when available, limit/attempted/work values when applicable, and exact evidence. A complete snapshot cannot contain a completeness-affecting value boundary.

At milestone completion, pure model tests construct a proven edge, an unproven alias edge, a complete empty result, and an incomplete empty result. Canonical JSON and JSONL distinguish all four and reject a dependence edge whose endpoints lack definition/use roles, whose evidence is empty, whose stable carrier digest disagrees with its projection, or whose aggregate proof is stronger than any witness step.

### Milestone 2: inventory source-backed definitions and uses

Add `crates/bifrost-analysis/src/analyzer/semantic/value_dependence.rs` as a runtime-independent producer of internal source-backed dependence and evidence records. `brokk-bifrost-runtime::extension` owns the #2101 public projection and translates these records through checked constructors; analysis must not name runtime extension types. Do not put extension wire types in the semantic oracle or value-flow solver. Add a declarative observation registry beside the `SemanticEffect`/`ValueFlowRelationKind` vocabulary so new event kinds cannot silently skip extension handling. Each registry row declares which endpoint is read before effects, which endpoint is defined after effects, its transfer family, source-mapping rule, and required semantic capabilities.

Inventory assignment source as a use and target as a definition. Inventory procedure receiver/parameter ports as entry definitions and their materialized values as definitions connected through receiver/parameter transfer. Inventory return and throw operands as uses and normal/exceptional return ports as definitions. Inventory allocation objects as definitions and allocation results as definitions connected by allocation transfer. Inventory memory-store values and base/index operands as uses, with the target location as an after-effects definition. Inventory memory-load location, base, and index operands as uses, with the result value as an after-effects definition. Inventory call receiver and arguments as uses before invocation, call result/exceptional result as definitions at the matching continuation, capture sources as uses, and capture ports as definitions. Language-defined relations are supported only when their adapters attach stable source mappings and explicit proof/completeness; otherwise publish `value_flow_unsupported`.

Use semantic source mappings and existing point/event identities. If an internal carrier has no source-backed mapping, create a structured synthetic occurrence anchored to the exact enclosing semantic event with a nonempty reason such as `procedure_parameter_port` or `abstract_field_location`. Synthetic identity includes the anchor and reason and never pretends to be an exact source range. If neither exact nor anchored synthetic mapping is possible, emit a missing-semantics boundary rather than an anonymous node.

Canonicalize the inventory by stable occurrence identity. Identical carriers at different points/phases remain distinct. Deduplicate only identical observations produced by the same event and role. Apply definition/use limits after canonical ordering and reserve one boundary slot before truncation so incompleteness remains visible.

At milestone completion, fixtures for at least Rust and TypeScript (or two languages with richer current capability evidence at implementation time) enumerate assignment, parameter, receiver where supported, return, field/index load/store, call result, and merge observations without parsing text. Near misses prove declaration-only values, nonexecuted type syntax, similarly named fields, and comments/strings do not create observations.

### Milestone 3: compute precise intraprocedural may dependence

Build one internal `ValueDependencePlan` from the automatic inventory and existing `ValueFlowSnapshot`; do not expose or serialize it. Reuse `ValueFlowCarrierKey`, semantic CFG/ICFG nodes, the shared solver budget types, and witness reconstruction. Refactor shared transfer mechanics out of `ValueFlowProblem` only when needed so policy-selected and automatic clients execute identical assignment, load/store, and local transfer semantics. Do not clone a second solver and do not use source text.

Seed a distinct definition fact at each definition occurrence. Propagate it along the procedure’s validated CFG. At each transfer relation, move or copy the reaching definition to its target carrier with the weakest proof/completeness accumulated from the definition, transfer, CFG edge, heap evidence, and source mapping. At each use occurrence, emit a may-dependence meeting for every reaching definition of the observed carrier. Canonically deduplicate meetings by stable definition, use, subtype chain, proof, completeness, and evidence digest.

Implement overwrite with existing update semantics. A local assignment to the same canonical local carrier kills older definitions after effects. A memory store kills older location definitions only when the heap oracle returns a proven-complete strong update for the exact path and exclusive object. Weak updates, summary paths, wildcard indices, alias breadth truncation, or unproven exclusivity retain both old and new definitions, weaken proof/completeness, and attach the matching typed boundary/evidence. At CFG joins, union reaching definitions; label an edge’s subtype chain with `merge` only when its witness crosses a join at which more than one definition of the observed carrier reaches.

Return `complete empty` only when observation inventory, value-flow snapshot, CFG traversal, heap queries, witness reconstruction, and all relevant capability checks are complete for the requested scope/direction. Convert `SemanticOutcome`, `SemanticInputStatus`, semantic gaps, plan incomplete causes, solver termination, cancellation, and every exceeded ledger into #2101 status/boundaries. Never infer absence from `ValueFlowSummaryResult::meetings().is_empty()`; use its completion logic.

At milestone completion, behavior tests prove straight-line assignment, transitive assignment, use-before-definition absence, overwrite kill, branch merge with two reaching definitions, loop-carried may dependence, field separation, exact index separation, wildcard-index uncertainty, strong and weak updates, load-after-store, return dependence, and complete versus incomplete empty results. A small independent reference implementation over generated finite local CFGs and carrier gen/kill facts compares edge sets for exhaustive graphs within a documented bound; it exists only in tests and does not parse source.

### Milestone 4: add bounded interprocedural projections

Integrate with the #2101 bounded ICFG acquisition. For each entered call within `max_call_depth`, reuse `WorkspaceValueFlowProvider::call_bindings` and existing `CallBindings` evidence. Project actual argument to formal parameter as `call_argument`, actual receiver to receiver port as `call_receiver`, normal-return port to call result as `call_result`, and exceptional-return port to exceptional result when the language adapter and continuation provide it. Preserve passing mode, group coverage, candidate coverage, call context, proof, completeness, and relation evidence.

For materialized callees, propagate distinct definition facts through the existing context-respecting ICFG call/return edges; do not flatten call contexts. For external or intentionally summarized callees, use only compatible `SemanticProcedureSummary`, `ExternalSemanticSummarySet`, or curated models already accepted by `ValueFlowPlan`. Each edge includes summary schema/semantics/context/behavior identity, model/content hash, origin, transfer, proof, completeness, and whether evidence is derived or authored. An authored-complete summary can close a boundary only under the existing `ProvenBySummary` tier and must remain distinguishable in public status.

Ambiguous dispatch emits edges for supported candidates with their individual quality and an `ambiguous_dispatch` boundary for residual uncertainty. Unmaterialized or unmodeled external calls produce `unmodeled_call`; incompatible summaries produce `incompatible_summary`; call-depth, target, binding, continuation, summary, solver, and provider limits preserve partial edges and typed boundaries. Fallback call behavior may keep conservative flows internally, but any edge relying on it is unproven/partial and cites fallback evidence; it cannot close discovery.

Interprocedural support is additive by subtype and language. Capability reporting names each supported family. If receiver, exceptional return, reference output, heap summary, capture, deferred execution, or language-defined binding is unavailable, return an explicit unsupported/incomplete boundary for that family rather than fabricating a textual mapping.

At milestone completion, fixtures cover parameter, receiver, normal return, exceptional return where supported, call result, reference/heap side effect, two dispatch candidates, unmodeled external call, compatible authored summary, incompatible summary, call-depth truncation, and caller/callee overwrite isolation. A request restricted to procedure scope never enters callees.

### Milestone 5: serialize, validate, and measure the public behavior

Extend the #2101 validator and canonical codecs. Value-dependence subtypes and evidence use stable lowercase strings. Canonical ordering is relation kind, subtype chain, stable source occurrence, stable target occurrence, call context, proof, completeness, then evidence digest. Carrier projections recursively sort access-path selectors in semantic order without reordering the path itself. JSON and JSONL decode through the same validated domain model; unknown stable variants are rejected according to #2100 compatibility policy.

Add `tests/suite_semantic/extension_value_dependence.rs` using `InlineTestProject`, with fixture modules only if size warrants it. Extend #2101 direct-versus-serialized and repeated-process tests so requests selecting `value_dependence` produce equal validated snapshots and byte-identical canonical encodings. Test incoming, outgoing, and both directions; procedure, file, and bounded-call scope; multiple seeds; mixed relation requests with control flow/control dependence; stable identity across unchanged reopen; identity change after source/config/adapter/dependency changes; every new limit; cancellation before and during solve; malformed JSON/JSONL; and tampered evidence/digest rejection.

Add lifecycle measurement following `docs/src/content/docs/evaluation-evidence.md`, `tests/suite_semantic/measure_dataflow_lifecycle.rs`, `tests/suite_semantic/measure_summary_lifecycle.rs`, and `.agents/docs/semantic-artifact-lifecycle-matrix.md`. Record exact Bifrost commit, corpus revision/content hash, platform, build profile, API/schema/adapter/summary versions, request digest/limits, cold definition, two warmups, seven retained samples, elapsed median/spread, semantic/provider/solver work, definitions, uses, dependence edges, witness steps, boundaries, diagnostics, canonical bytes, retained bytes for inventory/plan/result/cache, cache hit/miss counters, and peak RSS where available. Measure at least one intraprocedural and one bounded interprocedural request. Do not propose persistence unless a later issue reviews this evidence against promotion gates.

At milestone completion, the canonical artifact consumer from #2101 can request and render value dependence without private imports. All focused tests, package seam checks when public archive contents changed, dependency checks, formatting, pre-push validation, review, and CI pass.

## Concrete Steps

Work from the Bifrost repository root. Before implementation, verify `git status --short --branch`, refresh issue #2103 and overlapping pull requests, and confirm that the authorized branch contains merged or compatible #2100 and #2101 contracts. Do not create or switch branches without explicit user direction.

Inspect the exact merged #2101 type/module paths and update this plan if they differ. Implement milestone 1 and run its owning package tests, expected to resemble:

    cargo test -p brokk-bifrost-runtime extension::value_dependence
    cargo test --test suite_semantic extension_semantic_relations

Expect model tests to accept complete and incomplete examples as distinct and reject malformed roles, carrier digests, evidence, limits, and aggregate quality.

Implement milestone 2 and run:

    cargo test --test suite_semantic extension_value_dependence -- observation_inventory
    cargo test --test suite_semantic semantic_value_language_contract

Expect two-language fixtures to inventory every supported named family and every near miss to remain absent.

Implement milestone 3 and run:

    cargo test --test suite_semantic extension_value_dependence -- intraprocedural
    cargo test --test suite_semantic value_flow_client
    cargo test --test suite_semantic dataflow_clients

Expect the overwrite fixture to remove the old local/exact-location definition only after proven strong update, the merge fixture to retain both branch definitions, and generated reference comparisons to produce no mismatches.

Implement milestone 4 and run:

    cargo test --test suite_semantic extension_value_dependence -- interprocedural
    cargo test --test suite_semantic value_flow_client
    cargo test --test suite_semantic dataflow_summaries
    cargo test --test suite_semantic icfg_contract

Expect parameter/receiver/return/call-result edges on supported fixtures and typed incomplete boundaries for ambiguity, unmodeled calls, incompatible summaries, and limits.

Implement milestone 5 and run:

    cargo test --test suite_semantic extension_value_dependence
    cargo test --test suite_semantic extension_semantic_relations
    node scripts/check-workspace-dependencies.mjs
    node --test scripts/check-workspace-dependencies.test.mjs

If #2101/#2100 places public extension code in a published package whose archive consumer changes, also run:

    scripts/check-workspace-packages.sh

Run the measurement harness in release mode with its documented corpus/environment arguments. Save only portable aggregate JSON and intentionally checked-in small golden outputs, never machine-specific absolute paths.

Run final local validation:

    cargo fmt
    cargo test -p brokk-bifrost-runtime
    cargo test --test suite_semantic

This work does not affect NLP or Python bindings, so routine focused testing stays featureless. Before an authorized push, check available disk and run:

    scripts/pre-push-gate.sh

If all-feature Clippy is run separately, use:

    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Record exact commands, pass counts, property-test seed/bounds, canonical JSON/JSONL hashes, measurement identity/results, package hashes where relevant, PR/CI links, and issue state in this plan.

## Validation and Acceptance

Issue #2103 is complete only when direct current evidence proves every statement below.

An extension request selects `value_dependence` through #2101 and receives edges in the same bounded snapshot as other relations. The Rust and serialized APIs expose stable source-backed definition/use occurrences, not `ValueFlowPlan`, carrier/source/sink dense IDs, semantic artifact handles, solver facts, worklists, stores, adapters, MCP, or LSP.

Each edge means may influence, has a nonempty ordered subtype chain, exact source/target mappings, direction, stable carrier projections, proof, completeness, and contributing semantic/heap/call/summary/witness evidence. No public field suggests must-flow, path feasibility, or score propagation.

Behavior fixtures prove assignment, parameter, receiver, normal/exceptional return where supported, field/index load/store, static memory where supported, call result, allocation, capture or explicit unsupported status, overwrite, branch/loop merge, and language-defined flow or explicit unsupported status. Near misses prove that names/text do not substitute for semantic structure.

Local overwrite kills old definitions only under proven exact update semantics. Weak updates, wildcard or summary access paths, and alias uncertainty retain conservative may edges with weakened quality and typed boundaries. Independent generated finite-graph checks agree with intraprocedural gen/kill results within their documented exhaustive bound.

Interprocedural edges exist only through bounded ICFG, exact call bindings, compatible summaries, or explicit curated models. Ambiguous dispatch, unmodeled external calls, incompatible/partial summaries, unavailable continuations, call-depth limits, and unsupported binding families remain typed boundaries. Authored-summary proof remains distinguishable from derived proof.

A zero-edge result is authoritative absence only when inventory, semantic snapshots, relevant capabilities, heap/call discovery, solver fixed point, witnesses, requested direction/scope, and every limit are complete. Otherwise the snapshot is incomplete and contains the exact boundary/diagnostic cause.

Every positive limit and cancellation path is tested. Truncation retains a terminal observable boundary. Shared request ledgers prevent multiple seeds or callees from resetting allowances. Incomplete results never enter a complete-result cache.

Canonical JSON and JSONL round-trip to the same validated domain snapshot, produce deterministic bytes across fresh processes and shuffled construction order, reject malformed/tampered value evidence, and match direct Rust execution for the same immutable generation.

Cold, warm, retained-memory, cache, solver-work, edge-count, witness-count, and output-byte measurements follow the existing evidence protocol for local and bounded interprocedural fixtures. No new persistence or whole-program points-to structure is introduced.

Focused suites, value-flow/summary/ICFG regressions, package/archive seam where affected, dependency checks, formatting, full pre-push gate, review, and CI are green. The issue and this plan link implementation evidence. Publication and license migration remain separately authorized work.

## Idempotence and Recovery

Value-dependence requests are read-only against one immutable workspace generation and safe to repeat. Automatic inventories, plans, witnesses, and canonical responses derive entirely from generation identity plus request/summary/model identities and finite limits.

If acquisition, solving, or witness reconstruction is cancelled or exceeds a limit, preserve validated partial edges and emit the exact typed boundary. Never cache that partial result as complete. If the generation changes, reject the request before resolving stable occurrences. If a public occurrence cannot be anchored to exact or structured synthetic source identity, return a missing-semantics boundary rather than inventing an ID.

If a refactor needs shared solver logic, first extract the existing transfer function with unchanged value-flow-client tests, then use it from the extension producer. Keep the old client passing throughout. Do not land a temporary text/regex implementation.

If canonical bytes change intentionally, apply #2100 compatibility rules, update golden fixtures through the canonical encoder, and record old/new hashes and rationale here. Never hand-edit hashes. JSONL file wrappers write a sibling temporary file, flush, validate terminal summary/digest, and atomically rename.

If measurement is interrupted, discard the sample and rerun the documented process. Use the normal target or `scripts/with-isolated-cargo-target.sh`; do not create manually named `/tmp/bifrost-*` targets. No destructive migration, publication, external repository, tag, or license action belongs to this plan.

## Artifacts and Notes

Initial implementation evidence at `4496c7f95`:

    crates/bifrost-analysis/src/analyzer/semantic/oracle/value_flow.rs
        validated procedure-local transfer rows and evidence-backed relation kinds

    crates/bifrost-analysis/src/analyzer/semantic/oracle/model.rs
        bounded exact/summary access paths and point-scoped abstract locations

    crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/heap.rs
        alias candidates, exclusivity evidence, strong/weak update semantics

    crates/bifrost-analysis/src/analyzer/value_flow/model.rs
        stable ValueFlowCarrierKey and run-local dense carrier/source/sink IDs

    crates/bifrost-analysis/src/analyzer/value_flow/provider.rs
        demand snapshots/call bindings and complete-only content-addressed cache

    crates/bifrost-analysis/src/analyzer/value_flow/plan.rs
        canonical transfer plan, call/summary integration, typed incomplete causes

    crates/bifrost-analysis/src/analyzer/value_flow/client.rs
        policy-free source-sensitive may-flow transfer over summary/ICFG solver

    crates/bifrost-analysis/src/analyzer/value_flow/result.rs
        may proof, witness reconstruction, complete-negative versus inconclusive

    tests/suite_semantic/value_flow_client.rs
        local, call, external summary, heap, and language-defined behavior examples

    tests/suite_semantic/dataflow_clients.rs
    tests/suite_semantic/dataflow_summaries.rs
        solver budget, boundary, summary, and completion contracts

Implementation must append concise evidence here: one stable definition/use pair, one assignment edge, one killed overwrite, one merge with two definitions, one weak-update uncertainty, one call/return edge, one authored-summary edge, one incomplete empty response, canonical hashes, property-test bound/seed, local/interprocedural measurements, and PR/CI links.

## Interfaces and Dependencies

Use the #2100 workspace/version/generation types and #2101 relation snapshot model. Extend, rather than duplicate, equivalents of these public types:

    pub enum SemanticRelationKind {
        ControlFlow,
        Call,
        Return,
        ControlDependence,
        ValueDependence,
    }

    pub enum ValueOccurrenceRole {
        Definition,
        Use,
        Observation,
    }

    pub struct StableValueCarrier {
        digest: StableDigest,
        projection: ValueCarrierProjection,
    }

    pub enum ValueCarrierProjection {
        Value { locator: StableSemanticNodeId, role: Box<str>, ordinal: Option<u32> },
        Port { procedure: StableSemanticNodeId, kind: ValuePortKind },
        Allocation { locator: StableSemanticNodeId },
        CallResult { call: StableSemanticNodeId, result: Box<Self>, callee: StableSemanticNodeId },
        ScopedRoot { kind: ValueScopedRootKind, locator: StableSemanticNodeId },
        Location { root: Box<Self>, selectors: Box<[ValueSelector]>, exact: bool },
    }

    pub struct ValueOccurrence {
        pub node: SemanticNodeOccurrence,
        pub carrier: StableValueCarrier,
        pub role: ValueOccurrenceRole,
        pub phase: ValueObservationPhase,
        pub event_ordinal: u32,
    }

    pub enum ValueDependenceSubtype {
        Assignment,
        Parameter,
        Receiver,
        NormalReturn,
        ExceptionalReturn,
        Allocation,
        FieldStore,
        FieldLoad,
        IndexStore,
        IndexLoad,
        StaticStore,
        StaticLoad,
        Capture,
        CallArgument,
        CallReceiver,
        CallResult,
        SummaryTransfer,
        Merge,
        LanguageDefined,
    }

The existing #2101 edge retains the value-specific payload without weakening common invariants:

    pub struct SemanticRelationEdge {
        pub source: SemanticNodeOccurrenceId,
        pub target: SemanticNodeOccurrenceId,
        pub kind: SemanticRelationKind,
        pub detail: SemanticRelationDetail,
        pub proof: RelationProof,
        pub completeness: RelationCompleteness,
        pub evidence: Box<[SemanticRelationEvidence]>,
    }

    pub enum SemanticRelationDetail {
        ValueDependence {
            subtypes: Box<[ValueDependenceSubtype]>,
            source: ValueOccurrence,
            target: ValueOccurrence,
            may: ValueDependenceMayStatus,
        },
        // existing control/call/return/control-dependence details
    }

Do not add a separate `value_dependence()` API if #2101 already dispatches selected relation kinds. The existing call should suffice:

    impl ExtensionWorkspace {
        pub fn semantic_relations(
            &self,
            request: &SemanticRelationRequest,
            cancellation: &ExtensionCancellation,
        ) -> Result<SemanticRelationSnapshot, ExtensionError>;
    }

Internally, add an analysis-owned producer that consumes the existing substrate and returns the #2101 projection builder’s domain records, not wire JSON:

    pub(crate) struct ValueDependenceProducer<'a> {
        workspace: &'a WorkspaceAnalyzer,
        value_flow: WorkspaceValueFlowProvider<'a>,
    }

    impl ValueDependenceProducer<'_> {
        pub(crate) fn project(
            &self,
            selection: &ResolvedRelationSelection,
            request: &mut SemanticRequest<'_>,
            limits: &SemanticRelationLimits,
            output: &mut SemanticRelationProjectionBuilder,
        ) -> Result<ValueDependenceCompletion, SemanticProviderError>;
    }

Reuse `ValueFlowCarrierKey`, `ValueFlowProvider`, `ValueFlowSnapshot`, `CallBindings`, semantic heap/update oracles, ICFG/dataflow solver, procedure summaries, curated models, finite ledgers, and witness reconstruction. Refactor visibility or shared internal helpers only as necessary. Never expose `ValueFlowPlan`, `ValueFlowCarrierId`, `ValueFlowSourceId`, `ValueFlowSinkId`, `SummaryDataflowResult`, `Arc` identity, or solver state.

No new crate or external dependency is required. #2103 depends strictly on #2100 and #2101. It may land independently of #2102 because control dependence and value dependence share only the #2101 graph contract. #2104 depends on stable #2101 node identities and can map observations before or after #2103, but any observation-driven value-dependence workflow requires #2103. #2105 records #2103 request/result/schema/summary/model identities and completeness once #2103 exists. The template/research consumers must not claim value-dependence support until the published package version containing #2103 is resolvable.

Plan revision note (2026-08-13): Created the initial API-only #2103 plan after inspecting the live issue, #2099/#2101 plans, and current semantic value-flow, heap, call-binding, summary, solver, completion, cache, fixture, and measurement substrate. The plan defines stable source-backed may-dependence occurrences, automatic policy-free observation discovery, exact-versus-weak overwrite behavior, conditional bounded interprocedural projection, one shared #2101 graph/codec, and explicit dependency ordering without a new crate or persistence promise.

Plan revision note (2026-08-13): Reconciled package direction with #2100/#2101. Analysis produces runtime-independent dependence records; `brokk-bifrost-runtime::extension` owns stable public types, validation, and projection.
