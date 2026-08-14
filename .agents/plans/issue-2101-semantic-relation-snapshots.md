# Expose bounded evidence-carrying semantic relation snapshots

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub issue [#2101](https://github.com/BrokkAi/bifrost/issues/2101), an API-only child of epic #2099. It builds on the semantic intermediate representation from #814, the persistence evidence rules from #817, the bounded interprocedural control-flow graph from #818, and the supported extension application boundary from #2100. It does not publish binaries, repositories, or research algorithms and does not require an Apache-2.0 migration.

## Purpose / Big Picture

After this change, an extension can ask Bifrost for a finite semantic graph around explicitly named source locations and receive a portable, deterministic result. The result identifies every node by a content-scoped source identity, describes every returned relation with proof and completeness, and states exactly where acquisition stopped. An empty complete result means the requested relation is absent in the selected finite scope; an empty incomplete result does not.

The same logical request is available as Rust values and canonical JSON or JSON Lines. A checked-in fixture demonstrates that both routes produce the same canonical projection while dense graph numbers remain convenient aliases confined to one response.

## Progress

- [x] (2026-08-13 13:00Z) Read `.agents/PLANS.md`, the epic plan, live issue #2101, and the current semantic identity, provider-outcome, ICFG, rendering, and lifecycle-evidence implementations.
- [x] (2026-08-13 13:00Z) Fix the public identity, bounds, proof/completeness, boundary, canonical serialization, test, and measurement design in this issue-specific plan.
- [x] (2026-08-13 16:53Z) Re-read the repository and ExecPlan instructions, refreshed live #2100/#2101 and PR state, confirmed no overlapping #2101 PR, and waited for #2100's validated local checkpoint `3f5a9d676` rather than duplicating its contract.
- [x] (2026-08-13 17:04Z) Attached this worktree to `dave/issue-2101-semantic-relation-snapshots`, genuinely stacked first on #2100 checkpoint `3f5a9d676`; ready parent PR #2113 subsequently published exact head `0800ef224` for the next rebase.
- [x] (2026-08-13 17:15Z) Implemented the first complete versioned request/result model, positive finite semantic-specific limits, canonical node ordering/local IDs, typed evidence and boundaries, authoritative-absence semantics, and canonical JSON/JSONL codecs.
- [ ] Project bounded control-flow, call, and return relations without exposing semantic artifacts or run-local handles.
- [x] (2026-08-13 17:15Z) Implemented canonical JSON and JSONL codecs on the shared domain value; focused runtime extension tests pass 7/7, including strict request decoding, JSON/JSONL equivalence, digest binding, and complete-versus-incomplete empty results.
- [x] (2026-08-13 17:32Z) Rebased the stack onto ready #2100 PR #2113 exact head `0800ef224`, fixed canonical endpoint remapping and truncation after the final-parent tests exposed dangling aliases, then passed all runtime tests (2 unit, 1 runtime, 8 extension, 3 doctests), `cargo fmt --check`, and the workspace dependency graph check.
- [x] (2026-08-13 17:48Z) Ran `scripts/pre-push-gate.sh`: all-features Clippy and all doctests passed; nextest stopped after 4,991 passes on the unrelated pre-existing `typescript_type_reference_prefers_interface_over_same_named_const` definition-resolution failure, leaving 5,562 tests unrun.
- [x] (2026-08-13 17:52Z) Pushed `dave/issue-2101-semantic-relation-snapshots` and opened ready stacked PR #2116 against #2100 PR #2113's branch; notified the #2102 task that the public child head is available.
- [ ] Add behavior, identity, limit, cancellation, stale-generation, canonicalization, and route-equivalence tests.
- [ ] Add cold, warm, retained-byte, edge-count, and canonical-byte measurements following the existing evidence protocol.
- [ ] Run focused tests, formatting, dependency checks, the pre-push gate, and record exact evidence here.

## Surprises & Discoveries

- Observation: `SemanticLocator` is deliberately remappable and explicitly not a cache-validity key, while `SemanticArtifactKey` is the complete per-file validity identity.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/ids.rs` documents those two contracts and makes the artifact key cover mount, normalized relative path, language, source revision, adapter semantics version, IR version, configuration fingerprint, and dependency fingerprint.

- Observation: the existing ICFG is already demand-materialized and bounded, but its `IcfgNodeId`, `IcfgEdgeId`, `ProcedureHandle`, `ProgramPointHandle`, and call context are snapshot- or artifact-instance-local.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/icfg.rs` calls the result a bounded dense slice; `crates/bifrost-analysis/src/analyzer/semantic/ir/artifact.rs` makes handle equality include `Arc` identity even when durable artifact keys agree.

- Observation: existing ICFG edge evidence is richer than a simple adjacency list but is not yet a portable evidence graph. It carries proof and completeness plus an optional dispatch-boundary kind, while exact source mappings and contributing evidence remain reachable through internal handles.
  Evidence: `IcfgEdge` in `crates/bifrost-analysis/src/analyzer/semantic/icfg.rs` contains dense endpoints, kind, call-site origin, `ProofStatus`, `EvidenceCompleteness`, and optional boundary; `Evidence` and `SourceMapping` live in the semantic IR.

- Observation: three independent limit systems must be unified at the extension boundary rather than accidentally reset: ICFG topology limits, semantic materialization work, and cross-provider file/traversal work.
  Evidence: `IcfgSnapshotLimits`, `SemanticBudget`, and `SemanticExecutionBudget` are separate current types, and nested semantic requests share the execution ledger by clone.

- Observation: there is no authoritative semantic-relation JSON/JSONL codec today. Existing semantic rendering is diagnostic and test-oriented, and incidental `serde_json` calls do not define canonical wire bytes.
  Evidence: targeted search found no semantic relation schema or JSONL dispatcher in `crates/bifrost-analysis/src/analyzer/semantic` or `crates/bifrost-runtime/src`.

- Observation: #2100's validated checkpoint already owns the stable extension workspace, generation, compatibility, cancellation, completion, capability, generic JSON envelope, and a deliberately minimal source-backed control-flow seam.
  Evidence: local parent commit `3f5a9d676` passed focused behavior tests and the archive-only runtime consumer before this branch was stacked. #2101 must extend those runtime-owned types instead of creating a second workspace or outcome contract.

- Observation: canonical node ordering must remap every edge endpoint through the node's stable identity, and topology truncation must discard edges whose endpoints were not retained.
  Evidence: the first final-parent test run failed with `Execution("dangling input edge target")` under a one-node budget. Remapping old aliases after sort and filtering edges to retained nodes made the final-parent extension suite pass 8/8.

- Observation: the full pre-push gate is currently red outside the extension surface.
  Evidence: after 4,991 nextest passes, `get_definition_test::typescript_type_reference_prefers_interface_over_same_named_const` expected kind `class` but received the same-named exported const as kind `field`. This branch changes only `brokk-bifrost-runtime::extension`; all-features Clippy, doctests, runtime tests, formatting, and dependency checks pass.

## Decision Log

- Decision: Depend on #2100's versioned extension workspace and immutable workspace generation; do not expose #2101 directly from `WorkspaceAnalyzer`.
  Rationale: generation validation, compatibility negotiation, and cancellation ownership belong to the supported application seam. Building a second entry point would create parallel contracts and leak implementation ownership.
  Date/Author: 2026-08-13 / Codex

- Decision: Define a stable node identity as a versioned, domain-separated digest over the workspace generation digest, semantic artifact fingerprint, semantic locator, precise source mapping, semantic role, and a canonical local discriminator.
  Rationale: the locator alone intentionally survives source remapping and therefore cannot prove validity. Artifact-local IDs and handles cannot cross materializations. Binding the portable locator and source span to the complete artifact fingerprint and generation prevents silent joins across source, adapter, configuration, dependency, or workspace changes.
  Date/Author: 2026-08-13 / Codex

- Decision: Use source-backed seed selectors only: exact stable node IDs from the same generation, or normalized mount/path plus nonempty byte or line/column range and optional role/language constraints. Reject name-only and whole-workspace seeds.
  Rationale: the issue requires bounded demand materialization. Names are ambiguous and a scope without a finite source seed can imply eager workspace traversal.
  Date/Author: 2026-08-13 / Codex

- Decision: Make every request limit positive and mandatory on the wire; Rust convenience constructors may fill documented finite defaults.
  Rationale: omitted or zero-as-unlimited fields would violate the no-unbounded-traversal contract. Mandatory wire values also make serialized reproduction independent of library defaults.
  Date/Author: 2026-08-13 / Codex

- Decision: Model relation kinds as a closed versioned vocabulary for v1: control flow, call, return, control dependence, and value dependence. The #2101 implementation publishes control flow, call, and return; the latter two kinds return typed unsupported boundaries until #2102 and #2103 provide them.
  Rationale: downstream clients can implement one stable decoder before all producers exist, while unsupported never masquerades as an empty relation.
  Date/Author: 2026-08-13 / Codex

- Decision: Separate edge-local proof and completeness from result-level acquisition status and typed boundaries.
  Rationale: a proven edge can occur in an incomplete snapshot, and a complete snapshot can contain an edge based on explicitly partial evidence. Collapsing these dimensions loses information.
  Date/Author: 2026-08-13 / Codex

- Decision: Canonical JSON is compact UTF-8 JSON with one trailing LF, fixed schema field names, decimal integers, lowercase tagged-enum labels, SHA-256 digests as 64 lowercase hexadecimal characters, no floats, no null-valued optional fields, and lexicographically sorted object keys. Arrays are sorted by specified semantic keys before encoding.
  Rationale: `serde_json` preserves insertion order but does not by itself define a stable semantic order. A precise canonical form enables byte comparison and hashing without importing a second canonicalization package.
  Date/Author: 2026-08-13 / Codex

- Decision: Canonical JSONL is a header record, zero or more node records, zero or more edge records, zero or more boundary records, zero or more diagnostic records, and one terminal summary record, each encoded by the same canonical JSON rule with one LF.
  Rationale: records can be streamed within an already finite byte budget, while the terminal summary authenticates counts, work, completeness, and the canonical snapshot digest. Missing the terminal record is always invalid/incomplete.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not persist snapshots in this issue.
  Rationale: #817 requires measured promotion gates. #2101 records cold/warm and memory evidence but demand-builds request results; any persistence proposal is separate work after the gates pass.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Planning is complete. The current semantic IR and ICFG can supply the initial relation vocabulary, but a portable generation-bound identity projection and authoritative codec do not exist yet. Implementation depends on #2100 providing the extension workspace, generation identity, capability report, cancellation entry point, and shared version envelope. #2102 and #2103 depend on this schema but are not prerequisites for publishing v1 control-flow/call/return results because their relation kinds can report explicit unsupported boundaries.

## Context and Orientation

All paths are relative to the Bifrost repository root. `crates/bifrost-analysis/src/analyzer/semantic/ir/` owns Bifrost's validated, language-neutral per-file semantic artifact. An artifact contains procedures, program points, control edges, call sites, evidence, gaps, and source mappings. Its dense numeric IDs are indices into one immutable artifact and are not public durable identities.

`crates/bifrost-analysis/src/analyzer/semantic/ids.rs` owns portable identity ingredients. `SemanticArtifactKey` is the exact validity envelope for one file artifact. Its `fingerprint()` is a domain-separated SHA-256 digest of mount, workspace-relative path, language, content revision, adapter semantics version, semantic IR version, configuration fingerprint, and dependency fingerprint. `SemanticLocator` identifies a semantic role through declaration structure and a source anchor, but is remappable and therefore insufficient alone for validity.

`crates/bifrost-analysis/src/analyzer/semantic/icfg.rs` owns the demand-built interprocedural control-flow graph, abbreviated ICFG. It begins at one procedure, follows calls to a finite depth, and stops at node and edge limits. Its dense IDs are local aliases. Its edge vocabulary already distinguishes intraprocedural control flow, call, normal and exceptional return, and call-to-continuation edges. Its boundaries describe dispatch gaps, topology limits, and unavailable continuations.

`crates/bifrost-analysis/src/analyzer/semantic/provider.rs` owns finite semantic work accounting and `SemanticOutcome`. An outcome distinguishes complete, ambiguous, unknown, unsupported, unproven, exceeded-budget, and cancelled results while retaining partial values and exact work. The extension projection must preserve these states instead of flattening them into success/error.

`crates/bifrost-analysis/src/analyzer/semantic/render.rs` renders human/test views and is not the wire contract. Add the public model, validation, and schema codec in `brokk-bifrost-runtime::extension`, the supported module selected by #2100. Keep only runtime-independent acquisition algorithms and internal evidence records in `brokk-bifrost-analysis`, because analysis cannot depend upward on runtime. Runtime already depends on analysis and translates those internal records through checked public constructors. The extension-facing model must not depend on MCP, LSP, store rows, arenas, solver plans, or concrete language adapters.

`tests/suite_semantic/icfg_contract.rs` is the existing behavioral ICFG suite. Add new integration cases as `tests/suite_semantic/extension_semantic_relations.rs` and register exactly one `mod extension_semantic_relations;` in `tests/suite_semantic/main.rs`. Use `tests/common/inline_project.rs` for small multi-file fixtures. Put codec/package-boundary tests in the #2100-owned extension suite if that plan establishes a different public package harness; do not create a root `tests/*.rs` binary unless process isolation is required.

## Public Model and Invariants

The request envelope has `schema_version`, `api_version`, `expected_generation`, `seeds`, `scope`, `relations`, `direction`, and `limits`. `expected_generation` is mandatory and mismatch returns `StaleGeneration` before semantic work. `seeds` is nonempty. Deduplicate seeds by their canonical encoding. `scope` is one of `procedure`, `file`, or `bounded_calls`; `bounded_calls` requires a positive `max_call_depth`. `direction` is `outgoing`, `incoming`, or `both`.

`SemanticRelationLimits` contains positive `max_seed_matches`, `max_call_depth`, `max_nodes`, `max_edges`, `max_boundaries`, `max_diagnostics`, `max_output_bytes`, `max_materialized_files`, `max_traversal_steps`, and every `SemanticWork` dimension. A request that cannot encode its terminal summary within `max_output_bytes` is rejected as an invalid request; a request that reaches the limit after the summary reserve is admitted returns a truncated snapshot with an output-byte boundary. `max_call_depth` remains present for all requests so canonical requests do not gain conditional default semantics; procedure/file scopes validate but need not consume it.

The response envelope has `schema_version`, `api_version`, `generation`, `request_digest`, `nodes`, `edges`, `boundaries`, `diagnostics`, `work`, `limits`, and `snapshot_digest`; #2100's outer `ExtensionOutcome.completion` is the single authoritative completion field. Only `Complete` establishes authoritative absence, and only for the exact requested generation, seeds, scope, directions, relation kinds, and limits. Partial payloads remain present for every other completion when available. A generation mismatch is `ExtensionError::StaleGeneration` before semantic work, never a response completion or boundary.

A node contains a `local_id` and `stable_id`. `local_id` is a zero-based response-local integer assigned after canonical sorting. `stable_id` is a lowercase SHA-256 digest with domain `bifrost-extension-semantic-node-v1`. Hash length-delimited fields in this order: workspace-generation digest; `SemanticArtifactKey::fingerprint()`; canonical `SemanticLocator`; canonical source mapping kind and exact normalized range; semantic role; and a canonical local discriminator. For a program point the discriminator is the point kind plus its procedure-local canonical ordinal; for a call site it is call-site kind plus its canonical ordinal; for synthetic nodes it additionally includes the producer-defined synthetic kind. Never hash an `Arc` address, database generation row, dense artifact ID, dense ICFG ID, absolute filesystem path, hash-map iteration order, or call-context alias.

Call context is represented separately as an ordered outermost-to-innermost array of stable call-site IDs. Thus the same semantic program point has one stable semantic identity while context-sensitive ICFG occurrences remain distinct by the tuple `(stable_id, call_context)`. Canonical node ordering is stable ID, then lexicographic call-context IDs, then source mapping. Assign `local_id` only after this sort.

A node's source mapping contains mount identity, normalized workspace-relative path using `/`, language, UTF-8 byte range `[start,end)`, one-based line/column range for display, mapping kind, semantic role, artifact fingerprint, and source revision. Byte offsets are authoritative; line/column values are checked derivatives and mismatches fail decoding. Synthetic mappings include the exact enclosing source mapping plus a nonempty synthetic reason and never pretend to have an exact byte range.

An edge contains `source`, `target`, `kind`, optional subtype, source mappings, evidence records, `proof`, and `completeness`. Endpoints use local aliases in the payload but canonicalization and validation resolve them to stable `(stable_id, call_context)` identities. Edge kind is one of `control_flow`, `call`, `return`, `control_dependence`, or `value_dependence`; subtype carries current distinctions such as normal/exceptional return and concrete `ControlEdgeKind`. Edge ordering is kind, subtype, stable source occurrence, stable target occurrence, origin mapping, proof, completeness, then evidence digest. Deduplicate only byte-identical canonical edges; parallel edges with different evidence remain distinct.

`proof` is `proven` or `unproven` with a nonempty reason. `completeness` is `complete` or `partial` with a nonempty reason. Each evidence record contains its exact source mappings, proof, completeness, and an evidence kind; evidence is sorted by its canonical digest. An edge must have at least one evidence record. Aggregate edge proof cannot be stronger than its evidence, and aggregate completeness cannot be stronger than its evidence. A boundary never doubles as an edge.

A boundary contains a stable occurrence where acquisition stopped when available, an optional originating call site, a typed kind, affected relation kinds and directions, proof impact, completeness impact, exact limit/attempted/work values when applicable, evidence, and a nonempty diagnostic message. Boundary kinds in v1 are `dispatch_gap`, `missing_semantics`, `unsupported_relation`, `ambiguous_seed`, `cancelled`, `call_depth_limit`, `node_limit`, `edge_limit`, `boundary_limit`, `diagnostic_limit`, `output_byte_limit`, `semantic_work_limit`, `materialized_file_limit`, `traversal_step_limit`, `unavailable_continuation`, and `non_exiting_region`. `non_exiting_region` identifies entry-reachable CFG regions that cannot structurally reach a normal or exceptional exit; it makes zero derived dependence rows incomplete and must never be replaced by a synthetic escape edge. Unsupported capabilities from the language capability table become `unsupported_relation`; semantic IR gaps become `missing_semantics`; ICFG boundaries map without being attached to an invented edge. Stale generation is rejected before acquisition as #2100's typed error and never appears as a boundary.

Diagnostics are bounded structured records with a stable code, severity, message, optional source mapping, affected relation kinds, and evidence. If diagnostics overflow, retain the first canonical `max_diagnostics - 1` records and a terminal `diagnostic_limit` record; if the configured limit cannot hold that terminal record, reject the request. Apply the same reserved-slot rule to boundaries so truncation is always observable.

## Plan of Work

### Milestone 1: establish the versioned portable model

In the #2100 extension API module, add the request, result, stable identity, limits, relation vocabulary, proof/completeness, source mapping, evidence, boundary, diagnostic, work, and status types described above. Keep fields private where constructors must enforce invariants, but provide read-only accessors. Derive equality and ordering only when their semantics match canonical ordering; do not derive serialization directly on internal semantic types.

Add fallible constructors and a single validator used by Rust construction and decoding. It must reject empty seeds, zero limits, unknown major schema/API versions, duplicate local IDs, dangling endpoints, malformed hashes, absolute or non-normalized paths, invalid ranges, line/column disagreement, evidence-free edges, stronger aggregate evidence claims, missing terminal JSONL summaries, count/digest disagreement, and complete responses containing an incomplete boundary or diagnostic.

At the milestone end, pure model tests construct complete-empty and incomplete-empty responses and prove they are unequal, serialize differently, and cannot be confused through accessors.

### Milestone 2: project current ICFG relations

In `brokk-bifrost-analysis`, add one extension adapter next to the semantic service rather than inside `icfg.rs`. The adapter validates `expected_generation`, resolves each source-backed seed through existing semantic source indexes, checks requested relation capabilities, and invokes `WorkspaceIcfgProvider` with limits derived from the same request ledger. It must reuse one `SemanticRequest` and one `SemanticExecutionBudget` across all seeds, so each seed cannot reset work.

Project each `IcfgNodeKey` to stable semantic identity and explicit call context. Resolve every internal source mapping and evidence handle before returning; never retain an artifact `Arc` in the public snapshot. Map intraprocedural edges to `control_flow`, calls to `call`, normal/exceptional returns to `return`, and call-to-continuation edges to `control_flow` with explicit subtype. Preserve existing proof/completeness and contributing origin evidence. Map every `SemanticOutcome`, `SemanticGap`, dispatch boundary, continuation boundary, and limit to the public status/boundary model.

For `control_dependence` and `value_dependence`, consult the extension capability report. Before #2102/#2103 land, emit typed unsupported boundaries for the selected seeds and mark the snapshot unsupported or partial as appropriate; do not silently omit those edge kinds. At the milestone end, fixtures must show a complete edge, a partial/unproven edge, a dispatch gap, an unsupported relation, and a complete empty requested direction.

### Milestone 3: implement one canonical codec and two framings

Place codec code beside the public model, not in MCP or LSP. Build an explicit canonical value writer that sorts object keys and writes integers, booleans, strings, arrays, and objects according to the rule in the Decision Log. Do not depend on Rust struct declaration order or map iteration. Decode through strict intermediate wire structs with unknown-field rejection, then pass through the same public validator.

Canonical JSON writes one complete response object. Canonical JSONL writes the prescribed record sequence and terminal summary. The terminal summary includes the counts, result status, work, limits, request digest, generation, and a digest of all preceding canonical record bytes. Conversion between JSON and JSONL must pass through the same validated domain value, not one codec parsing the other's incidental text.

Expose functions equivalent to `encode_relation_request_json`, `decode_relation_request_json`, `encode_relation_snapshot_json`, `decode_relation_snapshot_json`, `write_relation_snapshot_jsonl`, and `read_relation_snapshot_jsonl`. If #2100 provides a generic dispatcher, add one `semantic_relations` request variant and return the same domain outcome. At the milestone end, shuffled construction order and JSON object key order decode to the same domain value and re-encode to identical bytes.

### Milestone 4: prove boundaries, equivalence, and cost

Add `tests/suite_semantic/extension_semantic_relations.rs` using `InlineTestProject`. Cover Linux- and Windows-shaped path inputs without assuming host separators. Include at least two language adapters already capable of producing a rich ICFG, selected from the capability table at implementation time. Use tiny checked-in source strings for behavior and a fixed public corpus only for measurement.

Behavior tests must cover outgoing, incoming, and both directions; procedure, file, and bounded-call scopes; multiple seeds; normal and exceptional flow; call and return; call context; a complete empty result; ambiguity; missing semantics; unsupported requested kinds; stale generation; cancellation before work and during work; each topology/output/diagnostic/boundary/provider budget; deterministic deduplication; source edit identity change; unchanged reopen identity stability; adapter/configuration/dependency fingerprint identity change; malformed wire input; JSON round trip; JSONL round trip; repeated-process byte equality; and direct-versus-serialized domain equality.

Add a measurement test following `docs/src/content/docs/evaluation-evidence.md`, `tests/suite_semantic/measure_dataflow_lifecycle.rs`, and `.agents/docs/semantic-artifact-lifecycle-matrix.md`. Report exact Bifrost commit, fixture/corpus revision and content hash, platform, build profile, API/schema/adapter versions, request digest and limits, cold definition, warmup count, retained sample count, median and spread for elapsed time, semantic work, node/edge/boundary/diagnostic counts, estimated retained bytes, peak RSS where available, and canonical JSON/JSONL bytes. Run one cold process and the documented two-warmup/seven-retained-process protocol. This is observability, not a persistence promotion experiment.

## Concrete Steps

Work from the Bifrost repository root. First verify that #2100's public extension seam is merged or available on the authorized implementation branch. Refresh issue #2101 and search for overlapping pull requests. Do not create or switch branches unless the user explicitly authorizes it.

Inspect the current capability vocabulary and select two supported fixture languages. Implement each milestone in order, updating `Progress`, `Surprises & Discoveries`, and the `Decision Log` after each stopping point. Use `cargo fmt` after Rust edits. Run focused validation after each milestone, substituting the exact package target established by #2100:

    cargo test --test suite_semantic extension_semantic_relations
    cargo test --test suite_semantic icfg_contract
    cargo test --test suite_semantic semantic_cfg_contract
    node scripts/check-workspace-dependencies.mjs

Run the measurement harness in release mode with its documented environment and corpus arguments. Save only small canonical fixture outputs and aggregate JSON reports that the repository's existing evidence conventions permit; do not check in machine-specific absolute paths.

Before pushing, run:

    cargo fmt --check
    scripts/pre-push-gate.sh

If a standalone package/archive seam changed, also run the #2100 clean-consumer package test. Do not enable NLP for focused #2101 validation. The pre-push gate owns comprehensive features according to repository instructions.

Record here the exact commit, commands, pass counts, canonical hashes, benchmark command, corpus identity, medians, retained bytes, and CI links. Only then update the child issue or close it.

## Validation and Acceptance

Issue #2101 is complete only when current evidence proves all of the following behavior.

1. Every accepted request has nonempty source-backed seeds and explicit positive finite budgets; no accepted combination can mean an unbounded whole-workspace traversal.
2. Stable node IDs survive an unchanged clean reopen and construction-order changes, and change when source content, adapter semantics, configuration, dependencies, or workspace generation changes. Dense node/edge IDs and call-context aliases are demonstrably response-local.
3. Initial control-flow, call, and return edges carry exact endpoint/source mappings, proof, completeness, and at least one contributing evidence record. Control/value dependence requests are explicit unsupported results until their producers exist.
4. Dispatch gaps, missing semantics, ambiguity, unsupported relations, stale generations, cancellation, call depth, every count/byte/work limit, and unavailable continuations appear as typed boundaries or diagnostics.
5. A complete empty result is accepted only with no completeness-affecting boundary. Incomplete empty results retain their reason and never satisfy the `authoritative_absence` accessor.
6. Canonical JSON and JSONL decode to the same domain result, re-encode byte-identically, reject malformed/inconsistent data, and produce deterministic hashes in separate processes.
7. Calling the Rust API and dispatching the equivalent serialized request against the same immutable generation yield equal validated domain values and canonical snapshot digests.
8. The tests include positive and realistic near-miss fixtures for at least two supported languages and path behavior independent of the host operating system.
9. Cold, warm, retained-byte, edge-count, boundary-count, output-byte, and work measurements follow the named evidence protocol and record all identity and limit inputs.
10. No public type exposes `SemanticArtifact`, arenas, stores, language modules, solver plans, `Arc` identity, MCP, LSP, or a persistence promise, and workspace dependency checks remain green.

## Idempotence and Recovery

Relation requests are read-only against one immutable extension workspace generation and can be repeated safely. Canonical encoders write to a caller-owned writer; any CLI file wrapper must write a sibling temporary file, flush it, and atomically rename only after the terminal summary and digest are complete. A missing or invalid JSONL terminal summary is rejected rather than interpreted as a partial success.

If cancellation or a limit interrupts acquisition, return the validated partial snapshot and typed boundary without caching it as complete. If the workspace generation changes, fail before using seed IDs or materializing semantic artifacts. If a benchmark is interrupted, discard that sample and rerun through the documented harness; do not create manually named Cargo target directories. If canonical bytes change intentionally, update the schema version as compatibility rules require and record the reason and old/new fixture digests in this plan.

## Artifacts and Notes

Current implementation evidence inspected for this plan:

    crates/bifrost-analysis/src/analyzer/semantic/ids.rs
        SemanticLocator: remappable source-facing identity, not validity
        SemanticArtifactKey: exact per-file validity envelope and stable digest

    crates/bifrost-analysis/src/analyzer/semantic/ir/artifact.rs
        ProcedureHandle: artifact-instance equality includes Arc identity

    crates/bifrost-analysis/src/analyzer/semantic/icfg.rs
        IcfgSnapshot: bounded dense request result
        IcfgEdge: kind, endpoints, origin, proof, completeness, dispatch boundary
        IcfgBoundary: dispatch, limit, and continuation boundaries

    crates/bifrost-analysis/src/analyzer/semantic/provider.rs
        SemanticOutcome: complete/ambiguous/unknown/unsupported/unproven/budget/cancelled
        SemanticBudget and SemanticExecutionBudget: separate shared finite ledgers

    .agents/docs/semantic-artifact-lifecycle-matrix.md
        persistence requires exact identity, completeness, cost, memory, size, and invalidation evidence

An illustrative canonical JSONL shape is:

    {"api_version":"1.0","expected_generation":"...","record":"header","request_digest":"...","schema_version":"1.0","status":"complete"}
    {"local_id":0,"record":"node","stable_id":"...",...}
    {"completeness":{"kind":"complete"},"kind":"control_flow","proof":{"kind":"proven"},"record":"edge","source":0,"target":1,...}
    {"boundaries":0,"diagnostics":0,"edges":1,"nodes":2,"record":"summary","snapshot_digest":"...",...}

The example omits fields only for readability; golden fixtures must contain every required field and obey canonical key and record ordering.

## Interfaces and Dependencies

Use the version/API/generation/capability types established by #2100. The final public surface must provide equivalents of these types and signatures, with exact module paths adjusted to that merged boundary:

    pub struct SemanticRelationRequest;
    pub struct SemanticRelationLimits;
    pub struct SemanticRelationSnapshot;
    pub struct StableSemanticNodeId;
    pub struct SemanticNodeOccurrence;
    pub struct SemanticRelationEdge;
    pub struct SemanticRelationBoundary;
    pub struct SemanticRelationDiagnostic;

    pub enum SemanticRelationKind {
        ControlFlow,
        Call,
        Return,
        ControlDependence,
        ValueDependence,
    }

    impl ExtensionWorkspace {
        pub fn semantic_relations(
            &self,
            request: &SemanticRelationRequest,
            cancellation: &ExtensionCancellation,
        ) -> Result<ExtensionOutcome<SemanticRelationSnapshot>, ExtensionError>;
    }

    pub fn encode_relation_request_json(
        request: &SemanticRelationRequest,
    ) -> Result<Vec<u8>, RelationCodecError>;

    pub fn decode_relation_request_json(
        bytes: &[u8],
    ) -> Result<SemanticRelationRequest, RelationCodecError>;

    pub fn encode_relation_snapshot_json(
        snapshot: &SemanticRelationSnapshot,
    ) -> Result<Vec<u8>, RelationCodecError>;

    pub fn decode_relation_snapshot_json(
        bytes: &[u8],
    ) -> Result<SemanticRelationSnapshot, RelationCodecError>;

    pub fn write_relation_snapshot_jsonl(
        snapshot: &SemanticRelationSnapshot,
        writer: impl std::io::Write,
    ) -> Result<(), RelationCodecError>;

    pub fn read_relation_snapshot_jsonl(
        reader: impl std::io::BufRead,
    ) -> Result<SemanticRelationSnapshot, RelationCodecError>;

The public model may depend on `serde` or `serde_json` internally, SHA-256 support already selected by the repository, core path/range types, and #2100 extension types. It must not expose serde's generic `Value` as the contract. Runtime's projection adapter depends downward on the existing analysis semantic provider, ICFG, capability report, and source index. Analysis may expose narrow internal records or methods required for acquisition, but must not name runtime extension types. Neither analysis, runtime, nor the root facade may depend on an external extension.

Dependency order is strict: #2100 supplies the stable entry point and generation contract; #2101 supplies the snapshot vocabulary and initial control/call/return projection; #2102 and #2103 add producers for already-reserved relation kinds; #2104 maps external observations onto #2101 stable identities; #2105 embeds #2101 request/result digests and completeness into reproducible artifacts.

Plan revision note (2026-08-13): Created this issue-specific API-only plan after inspecting live issue #2101 and the current semantic identities, ICFG, provider outcomes, rendering seams, and lifecycle evidence rules. The plan fixes stable identity as generation plus exact artifact validity plus portable source locator, makes every wire limit explicit and finite, separates edge evidence from acquisition completeness, and defines one canonical domain model for Rust, JSON, and JSONL. Later the same day, added the `non_exiting_region` boundary required by #2102 so control-dependence derivation cannot turn an entry-reachable region with no structural exit path into authoritative absence or fabricate a synthetic escape edge.

Plan revision note (2026-08-13): Reconciled package ownership with #2100. `brokk-bifrost-runtime::extension` owns the stable model, schema codecs, and public projection; analysis owns only downward, runtime-independent acquisition algorithms and internal evidence records.

Plan revision note (2026-08-13): Recorded the genuine stacked implementation base after #2100 produced validated checkpoint `3f5a9d676`. This preserves the parent contract and makes the temporary PR dependency explicit until #2100 merges.

Plan revision note (2026-08-13): Recorded ready stacked PR #2116 and the exact validation boundary. Retarget it to `master` after #2113 merges, provided the rebase does not require semantic conflict resolution.
