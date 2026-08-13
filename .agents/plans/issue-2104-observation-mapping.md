# Map generic external observations onto stable semantic nodes

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub issue [#2104](https://github.com/BrokkAi/bifrost/issues/2104), an API-only child of epic #2099. It depends on #2100 for the supported extension workspace and immutable generation identity, and on #2101 for portable stable semantic node identities and canonical codec rules. It remains observation-neutral: Bifrost validates and maps source observations, while extensions own coverage-tool, profiler, tracer, test-runner, build-system, and research-specific adapters.

## Purpose / Big Picture

After this change, an extension can give Bifrost a versioned document saying that an external tool observed a source range or branch for a named subject and run. Bifrost validates the repository, revision, path, content, coordinates, provenance, and finite limits, then maps each input record to stable #2101 semantic node identities. Every input record receives exactly one terminal outcome: exact, ambiguous, unmapped, stale, unsupported, or truncated.

Extensions can safely join exact observation mappings to bounded semantic-relation snapshots without relying on line numbers, duplicate file contents, dense node aliases, or process-local handles. Zero mapped nodes remains an explicit mapping outcome and never means that code was unobserved.

## Progress

- [x] (2026-08-13 14:00Z) Read `.agents/PLANS.md`, the epic plan, #2101's issue plan, live issue #2104, and the current semantic identity and source-range projection code.
- [x] (2026-08-13 14:00Z) Fix the generic observation schema, terminal outcomes, exact range semantics, safe join, canonical serialization, fixtures, and dependency order in this plan.
- [ ] Implement the validated observation document and mapping result domain types on the #2100 extension boundary.
- [ ] Implement generation/path/content/range validation and bounded source-range-to-node mapping against #2101 identities.
- [ ] Add canonical JSON/JSONL codecs and direct-versus-serialized equivalence.
- [ ] Add two-language behavior fixtures and every terminal mapping outcome.
- [ ] Run focused tests, formatting, dependency checks, and the pre-push gate; record exact evidence here.

## Surprises & Discoveries

- Observation: Bifrost already has portable path and source identity ingredients, but not a generic observation document.
  Evidence: `WorkspaceRelativePath` in `crates/bifrost-analysis/src/analyzer/semantic/ids.rs` rejects absolute, parent, current, empty, non-UTF-8, and Windows-incompatible components. `SemanticArtifactKey` binds path, language, source revision, adapter, IR, configuration, and dependencies.

- Observation: current source-oracle projection chooses the narrowest semantic mapping that contains a requested range, and an absent match is unknown rather than exhaustive absence.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/source.rs` implements bounded `pointees_at_source` and `dispatch_at_source`, retains coincident/path-specialized results, and documents that no call site at a position is `Unknown`, never an empty proven set.

- Observation: the current source bridge is specialized to point-sensitive value and dispatch queries, so it cannot be reused as the observation mapping contract without broadening its semantics.
  Evidence: its private selectors inspect values, points, and call sites only, select containing mappings, and return internal handles. #2104 must enumerate portable #2101 node occurrences across requested roles and distinguish overlap from containment.

- Observation: two range representations coexist. Analyzer `Range` uses byte offsets and line numbers but no columns, while semantic `SourceSpan` carries validated byte offsets, lines, and byte columns plus a deterministic occurrence.
  Evidence: `crates/bifrost-core/src/analyzer/model.rs` and `crates/bifrost-analysis/src/analyzer/semantic/ids.rs`. The public observation schema must choose one authoritative coordinate system and validate derived display coordinates.

- Observation: mount identity currently derives from a normalized absolute root and therefore is not by itself portable across clean checkout locations.
  Evidence: `WorkspaceMountId::from_root` hashes `root.as_os_str().as_encoded_bytes()`. Observation documents must address a named mount from #2100's generation envelope, not recompute or serialize an arbitrary absolute root.

## Decision Log

- Decision: One observation document describes one immutable workspace generation, repository revision, subject, and run, and contains a bounded nonempty sequence of records.
  Rationale: mixing revisions or runs would make provenance and stale detection record-dependent and complicate reproducibility. Multiple documents remain easy to aggregate downstream.
  Date/Author: 2026-08-13 / Codex

- Decision: Keep `outcome` and `category` caller-owned canonical scalar labels and attributes; Bifrost never interprets pass/fail, counts, suspiciousness, coverage, or profiling meaning.
  Rationale: the upstream API must serve coverage, traces, profiles, and hybrid analyses without importing tool- or research-specific semantics.
  Date/Author: 2026-08-13 / Codex

- Decision: Require both logical path identity and content identity for every source record, in addition to the document revision and generation.
  Rationale: repository revision alone may describe dirty overlays poorly, and content hash alone cannot distinguish duplicate-content paths. The tuple prevents silent cross-file mapping.
  Date/Author: 2026-08-13 / Codex

- Decision: Use half-open UTF-8 byte ranges as authoritative. Optional one-based line and byte-column coordinates are checked derivatives, never alternative identity.
  Rationale: line-only observations are inherently broad and newline-sensitive. Adapters must resolve their native format against exact source bytes before ingestion; Bifrost then verifies rather than guesses.
  Date/Author: 2026-08-13 / Codex

- Decision: Define exact mapping as the complete, deterministic set of supported semantic node occurrences whose exact/enclosing source mappings intersect the observed range under explicit role rules. Exact does not mean cardinality one.
  Rationale: one source range legitimately spans several semantic points, and one line may contain several events. Calling such a complete set ambiguous would lose useful observations. Ambiguous is reserved for indistinguishable alternatives that the supplied identities and semantics cannot resolve.
  Date/Author: 2026-08-13 / Codex

- Decision: Comments and blank ranges map to `unmapped`, not to the nearest node. Synthetic nodes are included only through an explicit enclosing-source link and a request flag; generated source requires explicit generated provenance plus matching generated-file identity.
  Rationale: proximity inference invents execution evidence. Synthetic/generated semantics must remain transparent and opt-in.
  Date/Author: 2026-08-13 / Codex

- Decision: A branch record has an observed origin range and a caller-stable branch key, plus optional destination range. Bifrost maps origin and destination independently and does not infer taken/not-taken semantics.
  Rationale: branch numbering and outcomes vary by producer. The generic contract preserves identity and topology without taking ownership of a tool's interpretation.
  Date/Author: 2026-08-13 / Codex

- Decision: Reuse #2101's canonical JSON rules and JSONL terminal-summary framing, with one terminal mapping-result record for every input record in input identity order.
  Rationale: one canonicalization contract avoids drift. JSONL can stream finite results, while a required terminal document summary detects interruption and count/digest disagreement.
  Date/Author: 2026-08-13 / Codex

- Decision: Only `Exact` mappings are directly joinable to #2101 snapshots, and the join checks generation, stable identity schema, stable node ID, and call-context policy.
  Rationale: joining ambiguous, stale, unsupported, or partial mappings would silently overstate evidence. A successful empty join remains distinct from an unmapped input.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Planning is complete. Existing semantic locators and artifact fingerprints can support safe range mapping, but the current source-oracle helpers return internal handles for specialized questions and do not provide terminal per-record outcomes. Implementation therefore needs a bounded portable source-mapping index/projector layered on #2101, not a textual or nearest-line fallback.

#2104 is blocked on the identity and generation contracts from #2100/#2101. It is independent of the implementations of #2102 and #2103: observation mappings target stable semantic nodes, and exact mappings can later join any relation kind published by #2101. #2105 consumes the canonical observation document/result digests, provenance, limits, completeness, and generation.

## Context and Orientation

All paths are relative to the Bifrost repository root. `crates/bifrost-analysis/src/analyzer/semantic/ids.rs` defines normalized workspace-relative paths, content revisions, artifact validity fingerprints, semantic locators, source spans, source roles, and occurrence numbers. A semantic locator is remappable and not a validity key; a portable #2101 stable node ID binds it to the immutable workspace generation and exact artifact validity.

An observation is an external fact tied to source, such as a trace event, profile sample, or coverage range. A subject is the caller-owned entity being observed, such as a test case, workload, process, or session. A run is one caller-owned execution identity. Bifrost stores their opaque identities and categories but assigns no domain meaning.

`crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/source.rs` is useful implementation precedent. It materializes one file artifact, scans retained mappings under finite semantic budgets, and preserves uncertainty. It cannot be called as the public mapper because it specializes to values or calls, chooses containment for point queries, and returns artifact-local handles.

Add public domain, codec, validation, and projection types to `brokk-bifrost-runtime::extension` beside #2101 request/result types. Add a runtime-independent internal mapper inside `brokk-bifrost-analysis`, where `WorkspaceAnalyzer`, semantic artifacts, and source indexes are legal dependencies; it returns narrow internal mapping/evidence records and never names runtime extension types. Runtime translates those records through checked public constructors. Do not add mapping to MCP, LSP, language adapters, or the store. Do not add tool-format dependencies.

Add behavior tests in `tests/suite_semantic/extension_observation_mapping.rs` and register `mod extension_observation_mapping;` in `tests/suite_semantic/main.rs`. Use `tests/common/inline_project.rs`. Reuse #2101's external package/codec harness for public-boundary and serialized-route tests rather than creating a root test binary.

## Public Model and Invariants

`ObservationDocument` contains `schema_version`, `api_version`, `expected_generation`, `repository`, `subject`, `run`, `producer`, `configuration_hash`, `limits`, and `records`. The expected generation is mandatory. Repository identity contains caller-declared repository kind and stable identity, exact revision, named mount identity from the #2100 generation envelope, and optional dirty-overlay identity. Subject and run each contain a nonempty namespace and identifier plus caller-owned `category`, `outcome`, and a canonical map of scalar attributes. Scalar values are string, signed integer, unsigned integer, or boolean; floats and arbitrary nested JSON are forbidden.

Producer provenance contains nonempty format name/version, tool name/version, adapter name/version, and input content SHA-256. Bifrost does not recognize these names semantically, but includes them in document identity. `configuration_hash` is a lowercase SHA-256 digest over the extension-owned mapping configuration. Unknown fields and duplicate object keys are rejected.

Each record has a caller-unique `record_id`, kind, source identity, caller category/outcome/attributes, and optional timestamp or sequence represented as an integer with a declared unit. Source identity contains named mount, canonical workspace-relative path, file content SHA-256, language when known, generated-source provenance when applicable, and a nonempty half-open byte range `[start,end)`. Optional line/byte-column coordinates must equal values derived from the exact source bytes. A point event is represented by a nonempty token/event range, not an empty range.

Record kind is `range` or `branch`. A range record asks for all supported semantic node occurrences whose source mapping intersects the observed range. A branch record adds a nonempty caller-stable `branch_key`, an origin range, and an optional destination range; origin and destination each receive their own mapping component. Neither record encodes a Bifrost-defined coverage count or pass/fail bit.

`ObservationMappingLimits` contains positive `max_records`, `max_source_bytes`, `max_candidate_nodes`, `max_mapped_nodes_per_record`, `max_total_mapped_nodes`, `max_diagnostics`, `max_output_bytes`, `max_materialized_files`, `max_traversal_steps`, and every semantic-work dimension consumed by mapping. No zero or omitted wire value means unlimited. Reserve capacity for one terminal outcome per admitted record and the document summary before processing.

Every input record produces exactly one `ObservationMappingOutcome` with the original `record_id`, validated source identity, status, mapped occurrences, diagnostics, work, and completeness. Status is:

- `exact`: identities match, acquisition is complete, and the complete canonical mapped set is returned. The set may contain one or many nodes.
- `ambiguous`: identities are current, but two or more indistinguishable mapping alternatives remain after role and source-mapping rules. Return all bounded candidate groups and the discriminators that could resolve them.
- `unmapped`: identities are current, acquisition is complete, and no supported source-backed semantic node intersects the range. State a reason such as comment, blank source, excluded role, or no retained semantic mapping.
- `stale`: generation, repository revision, dirty-overlay identity, path-to-content binding, or file content differs. Do not return mapped nodes.
- `unsupported`: Bifrost cannot map the file/language/record kind/source-mapping kind under the current capability report. Do not downgrade this to unmapped.
- `truncated`: cancellation or a count, byte, file, traversal, semantic-work, diagnostic, or output limit prevented a complete terminal answer. Retain the canonical partial mapped/candidate set and exact boundary.

Malformed documents fail document validation and produce no mapping result. Valid documents always produce a terminal outcome for every admitted record, including records after cancellation; those unvisited records receive `truncated` with a cancellation boundary and zero work. Document-level status is complete only when all records are exact or unmapped and no completeness-affecting diagnostic exists. Ambiguous and unsupported are terminal but make the document incomplete for aggregate absence claims.

Candidate selection first verifies the generation and repository/mount/revision envelope, then resolves the exact logical path and checks its content hash before reading semantic mappings. It never searches other paths for matching bytes. Iterate stable #2101 source-backed node occurrences in canonical identity order and select source mappings by these rules: exact and enclosing mappings intersecting the observation range are eligible; a point/range containing several events returns every eligible occurrence; coincident mappings remain separate stable occurrences; duplicate semantic roles for one stable identity deduplicate; synthetic mappings require `include_synthetic=true` and include their exact enclosing source evidence; generated mappings require matching generated provenance and `include_generated=true`.

An exact join API accepts one `ExactObservationMapping` and one #2101 `SemanticRelationSnapshot`. It rejects generation or stable-ID-schema mismatch before examining nodes. It joins by stable semantic node ID. By default one observation maps to every matching call-context occurrence in the snapshot and reports the multiplicity; an optional exact call context may narrow it. It returns matched snapshot-local aliases plus stable IDs and explicit `not_present_in_snapshot` entries. `not_present_in_snapshot` means the mapping was exact but that bounded relation snapshot did not contain the node; it is not an unmapped observation and not proof that the node was unobserved.

## Plan of Work

### Milestone 1: define and validate the observation domain

Add the public document, source identity, provenance, record, limits, outcome, boundary, diagnostic, and exact-mapping types beside #2101's portable types. Keep invariant-bearing fields private and use one validator from constructors and decoders. Define stable document and record digests with separate versioned domains; hash canonical schema/API versions, generation, repository, subject, run, producer, configuration, limits, and record content. Preserve input record order for caller correlation, while canonical maps and mapped node sets use semantic sort order.

Validation rejects empty/duplicate record IDs, zero limits, malformed hashes, unknown major versions, absolute/non-normalized paths, absent path content hashes, reversed/empty/out-of-file/non-UTF-8 ranges, inconsistent coordinates, undeclared timestamp units, branch records without origin/key, generated records without provenance, unknown fields, and total input bytes beyond the declared limit.

At milestone end, pure tests prove document digest stability, identity sensitivity, strict decoding, and that no valid record lacks a terminal-outcome representation.

### Milestone 2: implement bounded source mapping

Add an analysis-owned mapper that borrows the #2100 extension workspace. Validate generation/repository/mount/revision once, then validate path-content binding per distinct path. Materialize each distinct file at most once under one shared semantic and execution budget. Build a request-local interval index over portable #2101 node source mappings if measurement justifies it; otherwise perform a bounded canonical scan. Do not add a persisted index in this issue.

Map by exact intersection rules, roles, and source-mapping kind. Use the semantic artifact's validated source mappings and #2101 stable-ID projector; do not parse source text to synthesize semantic nodes. Source bytes may be read only to validate hashes, coordinates, comment/blank unmapped reasons, and UTF-8 boundaries. Text scanning must never replace semantic mapping.

Publish one terminal outcome for every record. Preserve semantic materialization ambiguity, gaps, unsupported capability, cancellation, and all exceeded budgets. Sort mapped nodes by stable ID, call context, role, mapping kind, and range. At milestone end, one multi-event line maps to multiple nodes, a wide range maps all intersecting events, a comment and blank line are explicitly unmapped, equal content at two paths never cross-maps, and a source edit is stale.

### Milestone 3: canonical codecs and safe join

Implement canonical JSON and JSONL using #2101's writer and strict decoder. JSON is one full document/result object. JSONL begins with a document header, has one observation record per input, one mapping outcome per input in record order, and ends with a required summary containing counts, status, work, limits, input digest, generation, and digest of all preceding records. A missing, duplicate, or reordered outcome, or a missing summary, is invalid.

Expose direct mapping, serialized dispatch, and exact join APIs. Direct and serialized mapping must call the same domain mapper. The join accepts only the `Exact` variant by type, validates generation and stable-ID schema, preserves observation provenance, and reports snapshot absence separately. At milestone end, Rust/JSON/JSONL values compare equal and shuffled internal candidate construction produces byte-identical canonical output.

### Milestone 4: two-language conformance and boundaries

Choose two languages with rich, stable semantic source mappings at implementation time, preferably Java and Python because they exercise different syntax and motivate downstream adapters without importing their coverage formats. Use inline fixtures with comments, blank lines, two calls/assignments on one line, nested calls, a range spanning several points, synthetic cleanup or generated semantics where supported, same-content files under different paths, and a changed-source generation.

Test exact one-node and multi-node sets; ambiguous coincident alternatives; unmapped comment/blank/excluded-role ranges; stale generation/revision/path-content combinations; unsupported language and mapping kind; every count/byte/work limit; cancellation before and during mapping; diagnostics truncation; generated opt-in; synthetic opt-in; branch origin/destination mapping; JSON/JSONL round trips; separate-process byte equality; direct/serialized equivalence; exact join with zero/one/many snapshot occurrences; and every rejected unsafe join.

## Concrete Steps

Work from the Bifrost repository root after #2100 and #2101 are merged or available on an authorized branch. Refresh live issue/PR state, but do not create or switch branches without explicit user authorization. Read the final #2101 stable identity and codec modules before implementing; update this plan if their names differ.

Implement milestones in order. Add `tests/suite_semantic/extension_observation_mapping.rs` and its one harness registration. Run focused tests after each milestone:

    cargo test --test suite_semantic extension_observation_mapping
    cargo test --test suite_semantic extension_semantic_relations
    cargo test --test suite_semantic semantic_ir_contract
    node scripts/check-workspace-dependencies.mjs

Run `cargo fmt` after Rust edits. Build the clean external consumer/package fixture established by #2100 so private types and path dependencies cannot leak into the public mapping API. Before push, run:

    cargo fmt --check
    scripts/pre-push-gate.sh

Record exact commits, commands, pass counts, fixture canonical hashes, package evidence, and CI links here before closing #2104. This issue does not authorize package publication or deployment.

## Validation and Acceptance

Issue #2104 is complete only when current evidence proves all of these behaviors:

1. A canonical, versioned, tool-neutral observation document ingests caller-owned subject/run/category/outcome, repository/generation/path/content/range identity, producer provenance, configuration hash, and explicit finite limits without tool-specific dependencies.
2. Generation, revision, dirty-overlay, path, and content mismatches return stale and never search duplicate-content paths for a replacement.
3. Exact mapping returns the complete deterministic set for one or many semantic events; ambiguous alternatives, unmapped comments/blank lines, unsupported semantics, and truncated work remain distinct terminal outcomes.
4. Every valid input record has exactly one terminal result, including after cancellation or limit exhaustion. Zero mapped nodes is represented by unmapped, stale, unsupported, truncated, or a successful snapshot-absence join, never a boolean “unobserved.”
5. Synthetic and generated nodes follow explicit opt-in and provenance rules. A range spanning several points and a multi-event line map every eligible occurrence without nearest-line inference.
6. Branch origins and destinations map independently, preserve caller branch identity, and do not import coverage semantics.
7. Canonical JSON and JSONL round-trip to equal domain values, reject malformed/incomplete framing, and are byte-stable across processes and candidate construction order.
8. Direct and serialized routes produce equal mapping outcomes for checked-in Java and Python fixtures, or two equally capable languages selected and recorded during implementation.
9. Only exact mappings join snapshots. Generation/schema mismatches fail, call-context multiplicity is explicit, and absence from a bounded snapshot remains distinct from failed observation mapping.
10. Public APIs expose no semantic artifacts, stores, arenas, dense artifact handles, MCP/LSP, coverage formats, test frameworks, suspiciousness formulas, or persistence promise; dependency checks and the clean external consumer remain green.

## Idempotence and Recovery

Mapping is read-only against one immutable generation and safe to repeat. Process each document with one request-owned finite ledger. Never cache a stale, ambiguous, unsupported, cancelled, or truncated result as complete. Cache of exact request-local path hash verification may be reused only inside the same document and generation.

Canonical output wrappers write to a sibling temporary file and atomically rename only after every input outcome and the terminal summary have been written, flushed, and hashed. Missing summary or outcome records fail decoding. If cancellation occurs, finish bounded terminal results from already-reserved metadata without further semantic acquisition. If a schema changes incompatibly, increment its major version and retain rejection fixtures; do not guess-convert observation provenance or ranges.

## Artifacts and Notes

Current source evidence:

    crates/bifrost-analysis/src/analyzer/semantic/ids.rs
        WorkspaceRelativePath: portable slash-canonical path validation
        SemanticArtifactKey: exact artifact validity and content scope
        SemanticLocator/SourceAnchor: portable role, range, and occurrence

    crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/source.rs
        bounded narrowest-containing source projection
        coincident/path-specialized candidates retained
        absent mapping is unknown, never proven empty

    crates/bifrost-core/src/analyzer/usages/reference_site.rs
        UTF-8 boundary and line/column validation precedent

Illustrative terminal outcomes are:

    {"mapped_nodes":[{"stable_id":"..."}],"record_id":"sample-1","status":"exact"}
    {"candidates":[...],"record_id":"sample-2","status":"ambiguous"}
    {"reason":"comment_or_blank","record_id":"sample-3","status":"unmapped"}
    {"expected_content":"...","record_id":"sample-4","status":"stale"}
    {"capability":"semantic_source_mapping","record_id":"sample-5","status":"unsupported"}
    {"boundary":{"kind":"mapped_node_limit","limit":8,"attempted":9},"record_id":"sample-6","status":"truncated"}

These snippets omit required envelope, provenance, evidence, work, and completeness fields. Golden fixtures must use the full canonical schema.

## Interfaces and Dependencies

Use #2100's extension workspace/generation/version/capability types and #2101's stable node identity, source mapping, work, boundary vocabulary, canonical writer, and relation snapshot. The final public API must provide equivalents of:

    pub struct ObservationDocument;
    pub struct ObservationRecord;
    pub struct ObservationSourceIdentity;
    pub struct ObservationProducer;
    pub struct ObservationMappingLimits;
    pub struct ObservationMappingResult;
    pub struct ExactObservationMapping;

    pub enum ObservationMappingOutcome {
        Exact(ExactObservationMapping),
        Ambiguous(AmbiguousObservationMapping),
        Unmapped(UnmappedObservation),
        Stale(StaleObservation),
        Unsupported(UnsupportedObservation),
        Truncated(TruncatedObservationMapping),
    }

    impl ExtensionWorkspace {
        pub fn map_observations(
            &self,
            document: &ObservationDocument,
            cancellation: &ExtensionCancellation,
        ) -> Result<ExtensionOutcome<ObservationMappingResult>, ExtensionError>;
    }

    pub fn join_exact_observations(
        mapping: &ExactObservationMapping,
        snapshot: &SemanticRelationSnapshot,
        context: ObservationJoinContext,
    ) -> Result<ObservationSnapshotJoin, ObservationJoinError>;

    pub fn encode_observation_document_json(
        document: &ObservationDocument,
    ) -> Result<Vec<u8>, ObservationCodecError>;

    pub fn decode_observation_document_json(
        bytes: &[u8],
    ) -> Result<ObservationDocument, ObservationCodecError>;

    pub fn write_observation_mapping_jsonl(
        result: &ObservationMappingResult,
        writer: impl std::io::Write,
    ) -> Result<(), ObservationCodecError>;

    pub fn read_observation_mapping_jsonl(
        reader: impl std::io::BufRead,
    ) -> Result<ObservationMappingResult, ObservationCodecError>;

The public domain may use serde internally but never exposes generic JSON values. Do not add JaCoCo, LCOV, coverage.py, Defects4J, test-framework, profiler, or tracing dependencies. Analysis performs internal mapping because it owns semantic artifacts and source access, while runtime owns the supported API and checked projection. Core retains portable path/range primitives. The root facade may re-export the runtime module but is not the acceptance dependency. External extensions depend upward on published public packages; Bifrost never depends on them.

Dependency order is #2100 generation/application boundary, then #2101 stable semantic identity and canonical snapshot vocabulary, then #2104 mapping and joins. #2102 and #2103 can land before or after #2104 because they add relation producers without changing observation identity. #2105 depends on #2104's canonical input/result digests, producer provenance, limits, work, completeness, and terminal outcomes.

Plan revision note (2026-08-13): Created this API-only plan after inspecting live #2104, the #2099/#2101 plans, portable semantic identities, source spans, current bounded source projection, and range validation. The plan requires path plus content identity, makes exact mapping a complete deterministic set rather than cardinality one, gives every record one terminal outcome, and permits joins only from exact generation-compatible mappings.

Plan revision note (2026-08-13): Reconciled package ownership with #2100/#2101. Analysis owns only runtime-independent source mapping and evidence acquisition; `brokk-bifrost-runtime::extension` owns public observation types, codecs, validation, and projection.
