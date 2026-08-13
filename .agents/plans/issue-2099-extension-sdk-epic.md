# Publish a stable Bifrost extension SDK and public extension template

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub epic [#2099](https://github.com/BrokkAi/bifrost/issues/2099) and its native child issues [#2100](https://github.com/BrokkAi/bifrost/issues/2100) through [#2105](https://github.com/BrokkAi/bifrost/issues/2105). The child issues remain the units of implementation, review, and closure. This document is the dependency-ordered execution and acceptance record for the whole epic.

## Purpose / Big Picture

After this work, a Rust developer can build an independent application against published Bifrost packages, open a source workspace, inspect the exact workspace generation and semantic capabilities, request a finite source-backed relation snapshot, join external observations to stable semantic nodes, and emit deterministic evidence-carrying results. A Python or notebook consumer can perform the same logical request through a canonical JSON or JSON Lines representation without importing Rust implementation types.

A separate public `bifrost-extension-template` repository will prove that lifecycle without a Bifrost source checkout. Its example will consume extension-owned input, perform a small nontrivial analysis, preserve proof and incompleteness, emit a reproducible run manifest, and intentionally demonstrate cold construction followed by cache reuse. A skeletal downstream `bifrost-fl` consumer will prove that fault-localisation research can obtain observation mappings plus typed control- and value-dependence relations without adding Defects4J, suspiciousness formulas, ranking metrics, or paper-specific concepts to Bifrost.

## Progress

- [x] (2026-08-13 10:04Z) Verified live epic #2099 and child issues #2100 through #2105, including assignment and the native GitHub sub-issue hierarchy.
- [x] (2026-08-13 10:10Z) Fetched `origin`, confirmed the worktree is clean and detached at `4496c7f95`, and confirmed zero divergence from current `origin/master`.
- [x] (2026-08-13 10:31Z) Audited the completed #814 semantic IR, #817 lifecycle decisions, #818 bounded ICFG, and #1275 runtime/package extraction foundations.
- [x] (2026-08-13 10:31Z) Authored this dependency-ordered epic ExecPlan and fixed the initial public boundary decisions.
- [ ] Attach the worktree to a user-authorized branch before the first checkpoint commit; repository instructions prohibit creating or changing branches implicitly.
- [ ] Complete #2100: publish the minimal stable application boundary, compatibility policy, workspace lifecycle, capability report, cancellation/limits, and canonical request/result envelope.
- [ ] Complete #2101: expose bounded evidence-carrying semantic relation snapshots with stable external identities and equivalent in-process/serialized semantics.
- [ ] Complete #2102: derive and expose typed control dependence from validated procedure-local CFGs.
- [ ] Complete #2103: expose bounded source-backed value dependence using the existing semantic and value-flow substrate.
- [ ] Complete #2104: validate and map generic external observations onto stable semantic nodes.
- [ ] Complete #2105: define deterministic run manifests, canonical artifacts, validation, and reproduction diagnostics.
- [ ] Deferred until Bifrost migrates to Apache-2.0: publish and validate `bifrost-extension-template`, then prove the required seam with a skeletal `bifrost-fl` consumer.
- [ ] Run the full cross-platform, packaging, canonical-serialization, cold/warm, and downstream-consumer completion audit; close children and epic only when every acceptance item has direct evidence.

## Surprises & Discoveries

- Observation: The extracted runtime package is the correct dependency layer but is deliberately not a public SDK today.
  Evidence: `crates/bifrost-runtime/src/lib.rs` calls itself an internal implementation detail, tells consumers to depend on the root facade, and re-exports `brokk_bifrost_analysis::analyzer`. `CodeIntelligenceRuntime` borrows a caller-owned `WorkspaceAnalyzer`, so an independent application must already know private workspace construction and lifecycle details.

- Observation: Existing semantic foundations already carry most internal evidence needed by the public projection, but their identities and containers have the wrong lifetime for an extension contract.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/icfg.rs` stores proof, completeness, typed boundaries, call context, and finite node/edge/call-depth limits. Its `IcfgNodeId` and `IcfgEdgeId` are dense snapshot-local integers, while nodes contain internal `ProgramPointHandle` and `CallSiteHandle` values.

- Observation: Capability discovery is total and deterministic, but it reports adapter primitives rather than extension-facing relation availability.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/capabilities.rs` enumerates every capability with complete, partial, or unsupported support. Control dependence, value dependence, observation mapping, canonical serialization, and artifact-version support are not yet extension-facing capability dimensions.

- Observation: There is no analyzer-owned public workspace generation identity today.
  Evidence: `crates/bifrost-mcp/src/searchtools_service.rs` owns an `AtomicU64` generation for one host session, while `crates/bifrost-analysis/src/analyzer/store/mod.rs::GenerationId` is a private per-language SQLite counter. `WorkspaceAnalyzer` represents an immutable source snapshot but exposes neither value as a portable identity. The SDK generation must therefore bind source, configuration, adapters, and active dependency evidence instead of re-exporting a host counter or database row ID.

- Observation: Existing lifecycle evidence rejects persistence of raw per-file semantic artifacts, whole-workspace ICFGs, and request-local solver state.
  Evidence: `.agents/plans/issue-817-artifact-lifecycle-foundation.md` and `.agents/docs/semantic-artifact-lifecycle-matrix.md` classify bounded solver/ICFG state as ephemeral; the measured packed CFG candidate failed the absolute cold-write gate. The SDK must expose intentional cache reuse without promising persistence for artifacts that failed promotion.

- Observation: The Bifrost codebase-search skill is installed but its MCP tools were not exposed to this task during initial planning.
  Evidence: tool discovery returned no Bifrost MCP calls, so the architecture map used `rg`. This is a tooling availability limitation, not evidence about analyzer correctness or latency.

## Decision Log

- Decision: Implement the API DAG as #2100, then #2101, then #2102/#2103/#2104 independently; #2105 follows #2104 and incorporates whichever relation producers have landed.
  Rationale: Stable ownership, versioning, identities, limits, and serialized result semantics must exist before relation producers or observation mapping can depend on them. Control dependence, value dependence, and observation mapping share #2101 identities but do not depend on one another. Manifests require canonical observation artifacts and may add relation-specific fixture coverage as those producers land.
  Date/Author: 2026-08-13 / Codex

- Decision: Stabilize `brokk-bifrost-runtime::extension` as the supported Rust dependency; the root `brokk-bifrost` facade may re-export it for convenience but is not the acceptance seam.
  Rationale: Runtime already sits above analysis/policy and below both transports, is publishable, and provides the necessary compilation boundary without a new crate. Its existing broad exports remain explicitly outside the extension compatibility promise. An archive-only consumer must depend directly on runtime and prove that MCP and LSP are absent.
  Date/Author: 2026-08-13 / Codex

- Decision: Define one protocol-neutral data model and make Rust and JSON/JSONL representations encode that model; do not design parallel APIs.
  Rationale: The epic requires equivalent result semantics and stable identities. One versioned request/result model with deterministic canonical encoding prevents semantic drift between in-process and out-of-process consumers.
  Date/Author: 2026-08-13 / Codex

- Decision: Public semantic identity must be source-backed and content/generation scoped; dense graph IDs are aliases inside one artifact only.
  Rationale: Existing dense IDs optimize traversal but are allocation-order dependent. External observations, cached artifacts, and reproducible manifests need identities that cannot silently join across a different path, revision, content, adapter version, or workspace generation.
  Date/Author: 2026-08-13 / Codex

- Decision: Define workspace generation as an immutable versioned identity envelope and digest, not as a process-local ordinal.
  Rationale: The existing MCP ordinal is meaningful only inside one service lifetime and store generation IDs are internal per-language rows. A public extension must compare generations across in-process/serialized execution and reopened workspaces. The identity envelope must include canonical workspace roots, source snapshot identity, analyzer configuration, adapter/IR versions, and active dependency/semantic-pack evidence; a session-local ordinal may be reported separately for diagnostics but cannot be the join key.
  Date/Author: 2026-08-13 / Codex

- Decision: An empty edge set is authoritative only when the result is complete for the requested relation, scope, direction, and limits.
  Rationale: The existing semantic system already distinguishes proof, completeness, dispatch gaps, cancellation, and limits. The extension layer must preserve these distinctions so absence is never inferred from unsupported or truncated acquisition.
  Date/Author: 2026-08-13 / Codex

- Decision: Keep snapshots demand-built and finite, and report cache behavior instead of promising persistence.
  Rationale: #817 rejected eager whole-workspace and request-state persistence. A run manifest can state cold, memory-reused, persisted-source-reused, or rebuilt behavior without converting a measured no-go into a public storage guarantee.
  Date/Author: 2026-08-13 / Codex

- Decision: Until Bifrost migrates to Apache-2.0, execute only the Bifrost-owned API child issues #2100 through #2105 and defer publication of `bifrost-extension-template` and `bifrost-fl`.
  Rationale: The user explicitly narrowed the near-term program to API contracts while the current license remains in place. The API work is useful and independently reviewable inside Bifrost; publishing external adoption templates is the license-sensitive step and should wait. Each API child is assigned its own sub-session and issue-specific ExecPlan so ownership and dependencies remain visible.
  Date/Author: 2026-08-13 / Codex

- Decision: Implement each child issue in a separate user-visible Codex task with its own worktree after the six issue plans are reconciled and made available from `master` when required.
  Rationale: Planning agents share this worktree, but implementation branches will overlap in public contract files and need durable independent histories, CI, and review. Separate tasks prevent shared-worktree collisions. Use stacked pull requests only for genuine compile-time contract dependencies: #2101 may stack on #2100; #2102 and #2103 may stack on #2101; #2104 and #2105 should base on the lowest merged/public contract they actually require. Rebase each onto `master` and retarget it once its parent lands rather than preserving an artificial stack.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Planning is initialized and the live issue hierarchy is verified. API planning and implementation now proceed through one sub-session per child issue. External template and `bifrost-fl` publication are deferred until the Apache-2.0 migration, so the parent epic remains open after the API children complete. The first material outcome will be a standalone compile-and-run seam test for #2100 that uses only documented public exports and reports a versioned generation/capability envelope.

## Context and Orientation

All paths in this section are relative to the Bifrost repository root.

The root `Cargo.toml` defines a multi-package Rust workspace. The root package `brokk-bifrost` is the published compatibility facade and executable. `src/lib.rs` re-exports broad analysis, runtime, MCP, LSP, policy, and semantic-pack surfaces. `crates/bifrost-analysis` owns workspace construction, language analyzers, semantic IR, structural queries, bounded ICFGs, and value flow. `crates/bifrost-runtime` owns protocol-neutral query and policy orchestration, but its current documentation says it is internal. `crates/bifrost-mcp` and `crates/bifrost-lsp` are transport hosts and must not appear in the extension SDK contract.

A workspace generation is the immutable identity of one analyzed source state. An extension request must execute against exactly one generation and report that generation in its result. A semantic relation snapshot is a finite graph projection selected from source-backed seeds. It contains stable public nodes, typed edges, proof, completeness, diagnostics, work accounting, source mappings, and typed boundaries where acquisition stopped or could not prove a fact. A boundary is not an edge and is not proof of absence.

`crates/bifrost-analysis/src/analyzer/semantic/ir/` defines the validated language-neutral semantic artifact from #814. `semantic/capabilities.rs` reports total per-language primitive support. `semantic/icfg.rs` demand-materializes bounded context-respecting interprocedural control-flow snapshots from #818. `semantic/oracle/` and `value_flow/` expose bounded dispatch, heap, and value-flow machinery. These are implementation foundations; the SDK must project them without exposing arenas, stores, solver plans, language modules, or run-local IDs.

The external template and research consumer are separate repositories because Bifrost must never depend on an extension. They may depend on a pinned published `brokk-bifrost` package or invoke a pinned Bifrost executable through the canonical serialized contract. The template is domain-neutral. `bifrost-fl` owns test adapters, coverage formats, fault-localisation formulas, ranking, evaluation datasets, and paper-specific interpretation.

## Plan of Work

### Milestone 1: establish the supported application boundary (#2100)

Add a narrow public `extension` surface owned by the root facade or, only if package-boundary evidence requires it, a new publishable SDK crate. The surface owns API version constants, compatibility negotiation, `ExtensionWorkspace`, immutable `WorkspaceGeneration`, capability reporting, typed finite limits, cancellation, stable source identity, result metadata, and canonical request/result envelopes. `ExtensionWorkspace::open` accepts platform-independent `Path` input and internally constructs the project/analyzer; consumers do not import `WorkspaceAnalyzer`, `AnalyzerStore`, language analyzers, MCP, or LSP.

Keep experimental relation capabilities explicitly marked experimental without weakening the stable envelope and identity semantics. Add a repository-external-style fixture crate under an integration-test fixture or temporary package consumer that depends only on the package archive/public facade. It must open a fixture, print generation/capabilities, execute one structural request, serialize the response, and compile with private implementation fields inaccessible. Add dependency-boundary checks and update `CONTRIBUTING.md` release inventory only if a new publishable crate is actually added.

Acceptance is a package-built standalone consumer on Linux, macOS, and Windows paths plus a compatibility test that rejects an unsupported major API version and accepts the current version.

### Milestone 2: expose bounded semantic relation snapshots (#2101)

Define relation requests with one or more source-backed seeds, a finite scope, requested relation kinds, direction, maximum call depth, node/edge/work/diagnostic/byte limits, and cancellation. Define stable nodes from workspace revision identity, normalized workspace-relative path, content identity, semantic artifact/adapter identity, precise source span, semantic role, and a stable local discriminator. Treat artifact-local numeric IDs only as compact aliases stored beside—not instead of—the stable identity.

Project control-flow, call, and return edges first from the existing ICFG. Every edge carries relation kind/subtype, endpoints, exact source/evidence references, proof, and completeness. Every unsupported capability, dispatch gap, stale generation, cancellation, truncation, or exceeded budget becomes a typed boundary or diagnostic. Canonically sort all map/set-like output before JSON encoding. Define JSON and JSONL schemas from the same Rust model, then prove byte-stable round trips and semantic equivalence between direct Rust calls and serialized execution.

Acceptance includes positive, near-miss, unsupported, stale-generation, cancellation, and every-limit fixture. A complete empty result and an incomplete empty result must be distinguishable by construction and serialization.

### Milestone 3: add control and value dependence (#2102 and #2103)

Implement control dependence procedure-locally from validated CFG topology. Use an iterative stack-safe postdominator algorithm and an independent small reference/oracle over exhaustive generated graphs. Specify synthetic exit handling, multiple normal and exceptional exits, nontermination, unreachable points, cleanup/finally, malformed input, and incomplete CFGs before exposing edges. Carry the controlling edge/predicate evidence into each public relation edge. Do not infer control dependence from source nesting or text.

Implement value dependence by projecting the existing semantic events, carriers, access paths, call bindings, summaries, and bounded value-flow engine. Start with precisely specified intraprocedural may-dependence. Add bounded interprocedural call, receiver, parameter, return, and memory projections only when current ICFG/oracle evidence supports them. Preserve ambiguity, alias approximation, external-model proof, overwrite/merge behavior, work, and incomplete boundaries. Do not create a second textual flow engine and do not expose `ValueFlowPlan` or run-local carrier IDs.

Acceptance includes behavior fixtures for the forms named by #2102 and #2103, realistic near misses, independent algorithm checks for control dependence, and bounded performance/retained-memory evidence before any persistence proposal.

### Milestone 4: join external observations and emit reproducible artifacts (#2104 and #2105)

Define a generic versioned observation document containing subject/run identity, caller-owned outcome/category, repository revision, normalized path identity, observed source ranges or branch records, tool/format provenance, configuration hash, and explicit limits/completeness. Validate every record and produce one terminal mapping outcome: exact, ambiguous, unmapped, stale, unsupported, or truncated. Map structurally through Bifrost source/semantic indexes. Never treat zero mapped nodes as proof of unobserved code.

Define a canonical manifest and artifact set containing Bifrost package/build/feature/API/adapter versions, workspace repository/revision/roots/exclusions/generation/dependency fingerprints, extension identity and configuration hash, semantic pack/catalog identities, cache state, request limits, diagnostics, work, completion, deviations, and content hashes for referenced files. Validation rejects missing identities, malformed hashes, incompatible versions, and any claimed complete aggregate containing an incomplete component. A reproduction command either recreates byte-equivalent canonical outputs or gives a typed exact prerequisite mismatch.

Acceptance includes deterministic repeated runs, tamper/mismatch tests, two-language mapping fixtures, and distinct examples of conformance, development, and confirmatory-study manifests without paper-specific schema fields.

### Milestone 5: publish the template and downstream seam proof

Create the separate public `bifrost-extension-template` repository only after the required Bifrost package version is published. Include `Cargo.toml`, a compact SDK example, extension-owned observation/configuration fixture, positive/near-miss/unsupported/incomplete behavior tests, JSON/JSONL parity, one-command reproduction, cold/reopen/cache-reuse evidence, `CITATION.cff`, publication guidance, and Linux/macOS/Windows CI. Pin a published Bifrost dependency or executable; do not use a path into a Bifrost checkout.

The example analysis should be nontrivial but domain-neutral: combine mapped observations with bounded control/value relations to emit an evidence-preserving impact ordering or slice, without suspiciousness metrics or fault-localisation terminology. Then create or update a skeletal `bifrost-fl` repository that consumes the same published seam and proves it can obtain the relations and observation mappings required for typed dependence reranking while keeping all research algorithms downstream.

Acceptance requires cloning each repository into a clean directory with no Bifrost source checkout, running its documented one-command workflow, and comparing the emitted canonical artifacts with checked-in expected hashes.

## Concrete Steps

Work from the Bifrost repository root.

Before each child issue, refresh live GitHub state, confirm no overlapping implementation or pull request, attach the worktree to the authorized issue branch, and update this `Progress` section. Do not create or change branches without explicit user instruction.

For focused Rust work, run `cargo fmt` and the narrow package/test target that owns the change. Before any push, run:

    scripts/pre-push-gate.sh

When a narrower explicit command is required, use featureless tests unless the change touches Python or NLP. Run all-feature Clippy through the managed isolated target helper when practical:

    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

For each public package milestone, build package archives and compile a clean temporary consumer against the archives so local path dependencies cannot conceal a publication failure. Run `scripts/check-workspace-dependencies.mjs` after any dependency-boundary change.

For canonical artifacts, run the same fixture twice in fresh processes and compare bytes and SHA-256 hashes. Then change one identity field, one source byte, one limit, and one incomplete component independently and assert deterministic rejection or a different declared hash.

After each child issue passes review and CI, record the commit, pull request, exact test commands, artifact hashes, and remaining dependency state here before closing it. Do not close #2099 until clean-checkout template and `bifrost-fl` seam proofs pass against published artifacts.

## Validation and Acceptance

The epic is complete only when all of these statements have direct evidence:

1. A clean external Rust crate builds without a Bifrost checkout and imports only documented public packages.
2. The external crate opens a workspace, reports immutable generation and total extension-facing capabilities, executes structural and bounded semantic requests, and never imports stores, language modules, arenas, solver plans, MCP, or LSP.
3. Rust and canonical JSON/JSONL paths produce semantically equivalent, deterministically ordered results with stable identities.
4. Control-flow, call/return, control-dependence, and value-dependence snapshots preserve proof, completeness, source mappings, diagnostics, work, limits, generation, and provenance.
5. Every incomplete acquisition remains distinguishable from authoritative absence.
6. External observations produce exact, ambiguous, unmapped, stale, unsupported, or truncated terminal outcomes and join snapshots only through matching stable identities.
7. Run manifests are versioned, validated, content-addressed, byte-stable where declared, and reproducible or fail with an exact prerequisite mismatch.
8. The public template performs the complete eight-step lifecycle in #2099, includes all named files/tests/workflows, and passes public cross-platform CI.
9. A skeletal `bifrost-fl` consumer obtains required relations and observation mappings without private imports.
10. Bifrost contains no Joern dependency or copied Joern template/API and no Defects4J, SBFL, suspiciousness, Top-k, or paper-specific concepts in the public SDK.
11. Cold/warm/cache behavior and retained bytes are measured under the existing evidence protocol; no artifact is persisted without passing #817 promotion gates.
12. All six child issues are closed with linked implementation evidence, current CI is green, published package/template versions are resolvable, and the parent epic checklist/native hierarchy reflects reality.

## Idempotence and Recovery

All snapshot and artifact requests are read-only against one immutable generation and are safe to repeat. Canonical output files should be written through temporary files and atomically renamed by their owning CLI so interruption cannot leave a valid-looking partial artifact. Reproduction commands validate existing hashes before reuse and report mismatches instead of silently overwriting evidence.

If a schema changes incompatibly before release, increment its major version and retain explicit rejection tests for the former incompatible form; do not guess-convert. Additive compatible vocabulary changes keep the current major version. If a build or benchmark is interrupted, rerun it through `scripts/with-isolated-cargo-target.sh`; do not create manually named Cargo target directories. If a publication step fails, follow `CONTRIBUTING.md` release recovery and do not publish dependents until the exact prerequisite checksum is verified.

## Artifacts and Notes

The authoritative orchestration artifacts are this plan, live GitHub issues #2099 through #2105, child pull requests, CI runs, package registry metadata, canonical fixture outputs, and the two external repositories. Issue bodies describe requirements; merged source, passing behavior tests, released packages, and clean-checkout reproductions prove them.

Initial architecture evidence at `4496c7f95`:

    crates/bifrost-runtime/src/lib.rs: internal runtime, broad analyzer re-export
    crates/bifrost-runtime/src/code_intelligence.rs: borrowed WorkspaceAnalyzer query facade
    crates/bifrost-analysis/src/analyzer/semantic/capabilities.rs: total primitive capability table
    crates/bifrost-analysis/src/analyzer/semantic/icfg.rs: bounded dense ICFG with proof/completeness/boundaries
    crates/bifrost-analysis/src/analyzer/value_flow/: existing bounded value-flow substrate
    .agents/docs/semantic-artifact-lifecycle-matrix.md: persistence promotion/no-go record

## Interfaces and Dependencies

The exact module placement remains subject to the standalone package seam proof, but the public contract must expose equivalents of these domain types without implementation fields:

    pub const EXTENSION_API_VERSION: ExtensionApiVersion;

    pub struct ExtensionWorkspace;
    pub struct WorkspaceGeneration;
    pub struct ExtensionCapabilityReport;
    pub struct SemanticRelationRequest;
    pub struct SemanticRelationSnapshot;
    pub struct StableSemanticNodeId;
    pub struct ExtensionRunManifest;

    impl ExtensionWorkspace {
        pub fn open(root: impl AsRef<Path>, options: ExtensionWorkspaceOptions)
            -> Result<Self, ExtensionWorkspaceError>;
        pub fn generation(&self) -> &WorkspaceGeneration;
        pub fn capabilities(&self) -> &ExtensionCapabilityReport;
        pub fn structural_query(&self, request: &StructuralRequest)
            -> StructuralOutcome;
        pub fn semantic_relations(&self, request: &SemanticRelationRequest)
            -> SemanticRelationOutcome;
    }

The serialized dispatcher consumes the same versioned request values and returns the same outcome model. Serialization types may use `serde`, but transport framing, filesystem watching, MCP, LSP, and process hosting stay outside the SDK. The root facade may depend downward on analysis/runtime; analysis and runtime must never depend on an external extension repository. The template and `bifrost-fl` depend upward only on published public Bifrost packages or the canonical executable protocol.

Plan revision note (2026-08-13): Created the initial epic plan after live issue/sub-issue verification and an audit of the current #814/#817/#818/#1275 implementation. The first design fixes contract-first ordering, one shared Rust/serialized semantic model, stable source-backed identity, explicit incomplete absence, and measured cache behavior while leaving new-crate creation conditional on a demonstrated package boundary.

Plan revision note (2026-08-13): Narrowed active execution to Bifrost-owned API issues #2100 through #2105 at the user's request. Deferred the external template and `bifrost-fl` until Apache-2.0 migration and split API planning into one sub-session per child issue.

Plan revision note (2026-08-13): Recorded the implementation handoff model requested by the user: one user-visible Codex task and worktree per child issue, durable plans on `master` when needed, and stacked PRs only along real public-contract dependencies.
