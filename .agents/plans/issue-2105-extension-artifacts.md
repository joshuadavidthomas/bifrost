# Emit and validate reproducible extension artifacts

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub issue #2105, "Extension API: define reproducible run manifests and canonical research artifacts." It is limited to domain-neutral API, schema, validation, encoding, and reproduction behavior inside Bifrost. Publication of the external extension template and its `CITATION.cff` is deferred until Bifrost migrates to Apache-2.0; the contract and hermetic Bifrost fixtures can be completed before that migration.

## Purpose / Big Picture

After this change, an independent extension can publish a small canonical manifest beside its request, observation, semantic snapshot, and result files. The manifest identifies the exact Bifrost engine and semantic versions, source workspace, extension, configuration, limits, cache state, diagnostics, work, completion, and every referenced file by content hash. A reader can validate the bundle without executing analysis, then ask Bifrost to reproduce it. Reproduction either emits byte-equivalent canonical deterministic artifacts or returns a typed list of exact prerequisites that are unavailable or different.

The contract distinguishes three purposes. A conformance run proves a declared behavior against checked expectations. A development experiment records exploratory execution without claiming preregistration or independent confirmation. A confirmatory result records a locked protocol and declared deviations without claiming that the manifest itself validates the study design. These labels change validation requirements and permitted claims, not analyzer semantics.

A developer can observe the behavior by running one hermetic fixture through the direct Rust API and canonical JSON dispatcher twice. Both runs produce the same manifest and deterministic artifact hashes. Changing one source byte, Bifrost feature, extension configuration byte, semantic-pack identity, request limit, observation file, or component completion changes the appropriate hash or produces a field-specific validation/reproduction mismatch. Volatile timing and host measurements remain declared records outside the deterministic core and cannot alter the run identity.

## Progress

- [x] (2026-08-13 12:55Z) Read live issue #2105, `.agents/PLANS.md`, the epic plan, and the #2100/#2101 API plans.
- [x] (2026-08-13 12:55Z) Audited the extension version/generation design, semantic artifact lifecycle matrix, evaluation and reproduction documentation, framed Git provenance, rollout artifacts, benchmark reports, and byte-stable OWASP artifact precedent.
- [x] (2026-08-13 12:55Z) Fixed the canonical bundle, manifest identity, validation, reproduction, purpose classifications, deviation semantics, and exact test matrix in this plan.
- [x] (2026-08-13 13:05Z) Reconciled this plan with #2104's `ObservationDocument`, `ObservationMappingResult`, producer/configuration identity, canonical JSON/JSONL codecs, terminal outcomes, work, limits, and planned document/result digests.
- [ ] When #2104 source lands, confirm its concrete digest accessors and canonical media/schema identifiers match the plan; reference them rather than duplicating observation fields.
- [ ] Implement the manifest domain model, canonical codec, strict validator, and bundle index in `brokk-bifrost-runtime::extension`.
- [ ] Implement capture helpers that bind #2100 identity and #2101/#2104 canonical artifacts without importing analyzer internals.
- [ ] Implement read-only bundle verification and reproduction with typed prerequisite mismatches and atomic output publication.
- [ ] Add hermetic model, codec, tamper, completeness, purpose, deviation, cross-platform path, direct/serialized parity, and clean-process determinism tests.
- [ ] Update public reproduction/evaluation documentation; defer external-template CI and `CITATION.cff` acceptance until license migration.

## Surprises & Discoveries

- Observation: Bifrost already documents a useful run manifest, but it is illustrative prose rather than a versioned public schema or shared validator.
  Evidence: `docs/src/content/docs/reproduce-analysis.md` gives JSON examples and verification steps but no Rust model, canonical byte contract, or reproduction mismatch vocabulary.

- Observation: current evidence artifacts use several incompatible conventions. Some include `generated_at`, ordinary serde field order, or local checkout paths; the OWASP artifact intentionally omits timestamps for byte stability; lifecycle benchmarks use framed tree fingerprints and exact corpus revisions.
  Evidence: `SemanticDiagnosticRolloutArtifact` contains `generated_at`; `BenchmarkRunReport` contains `generated_at`, `manifest_path`, and checkout paths; `.agents/docs/owasp-benchmark-taint-bakeoff-2026-08.md` explicitly states that omitting a timestamp makes equal inputs byte-identical.

- Observation: #2100 and #2101 already define the identities this issue must embed rather than recompute: extension API/build identity, immutable workspace generation and capability report, request digest, snapshot digest, limits, work, proof, completion, and canonical JSON/JSONL.
  Evidence: their ExecPlans bind generation to captured analyzer state and define one shared Rust/serialized relation model with deterministic digests.

- Observation: the lifecycle matrix treats incomplete, cancelled, truncated, stale, or corrupt artifacts as non-authoritative and non-reusable.
  Evidence: `.agents/docs/semantic-artifact-lifecycle-matrix.md` requires exact identity/completeness and says incomplete or corrupt persisted values are misses. A run bundle may preserve such an outcome as evidence, but cannot call it complete.

- Observation: `benchmark_provenance.rs` correctly length-frames fields, hashes dirty-tree bytes, and excludes declared output paths; it returns `Option`, which is acceptable for an internal benchmark but insufficient for a public conformance manifest.
  Evidence: missing Git commands or unreadable files currently collapse to `None`; #2105 validation instead needs a typed identity-unavailable result.

- Observation: #2104 defines one canonical `ObservationDocument` and one canonical `ObservationMappingResult`, with a terminal outcome for every admitted record and domain digests over the document/result.
  Evidence: `.agents/plans/issue-2104-observation-mapping.md` defines `encode_observation_document_json`, JSONL mapping framing, exact/ambiguous/unmapped/stale/unsupported/truncated outcomes, and names #2105 as the consumer of its digests, provenance, limits, work, and completeness.

## Decision Log

- Decision: Define a bundle as `manifest.json` plus external content-addressed files; never embed large graph, observation, or result payloads in the manifest.
  Rationale: the manifest remains inspectable and bounded while SHA-256 descriptors authenticate arbitrary canonical artifacts. This matches the issue's explicit non-goal and permits streaming verification.
  Date/Author: 2026-08-13 / Codex

- Decision: Reuse #2101's canonical JSON writer rules and `StableDigest` validation instead of creating another JSON canonicalization dialect.
  Rationale: one object-key ordering, integer, path, enum, and digest rule prevents an extension result from changing bytes merely because it moved between relation and manifest codecs.
  Date/Author: 2026-08-13 / Codex

- Decision: Separate deterministic `identity` and `evidence` from optional `volatile` measurements. The manifest digest excludes the `volatile` object, but the canonical full-manifest file hash includes it.
  Rationale: elapsed time, timestamps, host names, CPU models, peak RSS, and process IDs can be useful evidence but cannot participate in reproducible run identity. Exclusion must be structural and declared by schema, never an arbitrary caller-provided JSON pointer list.
  Date/Author: 2026-08-13 / Codex

- Decision: A manifest has one `purpose`: `conformance`, `development_experiment`, or `confirmatory_result`; purpose-specific fields are validated rather than treated as free-form labels.
  Rationale: conformance needs expectation identity; a development experiment must not claim a locked protocol; a confirmatory result needs a preregistered/locked protocol identity and explicit deviations. The format remains domain-neutral and does not decide whether a research design is valid.
  Date/Author: 2026-08-13 / Codex

- Decision: Preserve every analytical status, but permit manifest-level `complete` only when every required component is complete and every completeness-affecting deviation is absent.
  Rationale: an incomplete result is publishable evidence of an incomplete run. It is not an error to encode, but cannot be promoted by relabeling its aggregate.
  Date/Author: 2026-08-13 / Codex

- Decision: Deviations are canonical typed records with affected components, expected and observed digests or values, justification, and completeness impact. They are never silently ignored during reproduction.
  Rationale: confirmatory reporting needs honest protocol drift. A declared deviation explains a mismatch but does not make bytes equal or restore completeness automatically.
  Date/Author: 2026-08-13 / Codex

- Decision: Reproduction is two-phase and read-only until prerequisites validate: preflight returns all deterministic mismatches in canonical order, and execution writes to a new staging directory before atomic publication.
  Rationale: fail-fast hides useful mismatch information, while executing with the wrong engine/source/configuration can create misleading artifacts. Staging prevents a failed rerun from overwriting the evidence being verified.
  Date/Author: 2026-08-13 / Codex

- Decision: The reproduction API accepts caller-supplied resolvers for engine, source checkout, extension executable/package, and artifacts; it does not clone repositories, download packages, install toolchains, or execute arbitrary manifest command strings.
  Rationale: those operations require host policy and network authority. A typed plan can identify exactly what is missing without turning a data manifest into a shell script.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not add a crate. Put the public model/codec in `brokk-bifrost-runtime::extension` and keep host filesystem/process adapters in the root facade only if they need facade-owned facilities.
  Rationale: this work extends the existing extension ownership boundary and creates no new compilation, publication, or dependency boundary.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Planning is complete. The current repository has strong ingredients—framed provenance, exact semantic identities, canonical relation plans, lifecycle completeness rules, and public methodology—but no single authoritative extension artifact contract. Implementation awaits #2100/#2101 source and must align observation descriptors with #2104. External template use and `CITATION.cff` remain deferred by the Apache-2.0 constraint, not removed from final epic acceptance.

## Context and Orientation

All paths are relative to the Bifrost repository root.

`crates/bifrost-runtime/src/extension/` is the public package seam selected by #2100. It will own `ExtensionApiVersion`, compatible version negotiation, `StableDigest`, `WorkspaceGeneration`, `ExtensionCapabilityReport`, extension-owned paths, limits, work, diagnostics, completion, and canonical JSON. #2101 adds `SemanticRelationRequest` and `SemanticRelationSnapshot`, with `request_digest` and `snapshot_digest`, canonical JSON and JSONL, and stable source-backed identities. #2104 will add a generic external observation document and a mapping result. #2105 references those exact canonical values and hashes; it must not serialize internal `WorkspaceAnalyzer`, stores, arenas, dense IDs, MCP, or LSP values.

A canonical artifact is a file whose bytes are uniquely determined by its validated domain value. Canonical JSON uses compact UTF-8, lexicographically ordered object keys, schema-defined array order, decimal integers, lowercase tagged enums and SHA-256, no insignificant whitespace, and one trailing LF. Canonical JSONL follows #2101's record order and terminal-summary rule. Binary or extension-defined files may be referenced, but they are opaque content artifacts unless their descriptor names a registered canonical media type and schema.

`crates/bifrost-analysis/src/analyzer/benchmark_provenance.rs` demonstrates type/length-framed SHA-256 over Git commit, tracked diff, and untracked path/content. It excludes the output under construction. Use the extension `StableDigest` wrapper and domain separation; do not copy optional/stringly error behavior into the public validator.

`.agents/docs/semantic-artifact-lifecycle-matrix.md` states when cached artifacts are exact and complete. The run manifest reports cache behavior; it does not promote, persist, or bless a cache. Valid values distinguish `fully_cold`, `persistent_source_reused`, `process_memory_reused`, `artifact_reused`, and `rebuilt`, with evidence naming which stores/artifacts were present. Never infer cold/warm from elapsed time or process count.

`docs/src/content/docs/evaluation-evidence.md` distinguishes executable conformance from performance and accuracy studies. `docs/src/content/docs/reproduce-analysis.md` lists the engine, source, query/policy, environment, limits, cache state, typed output, and verification material a reader needs. Replace its illustrative manifest with this authoritative schema while retaining its warnings about proof, partiality, and secrets. `docs/src/content/docs/cite-bifrost.md` explains software attribution. A manifest references citation metadata by hash/path but is not itself a citation or proof of study quality.

The manifest file is always named `manifest.json` at the bundle root. Every artifact path is a normalized `/`-separated relative path beneath that root, has no empty, `.` or `..` component, no backslash, NUL, drive prefix, or symlink escape, and is unique after normalization. The manifest cannot list itself as a content artifact, preventing a recursive hash.

## Manifest Contract

The deterministic manifest contains `schema_version`, negotiated extension API, purpose, run identity, engine identity, workspace identity, extension identity, activated semantic identities, execution contract, components, deviations, aggregate status, and `manifest_digest`. It may also contain a `volatile` object whose fields are schema-defined and excluded from `manifest_digest`.

The engine identity contains Bifrost package version, exact executable/build commit when known, dirty state plus tree fingerprint when dirty, build profile, sorted feature set, target triple, extension API version, semantic IR version set, sorted adapter identities, and the exact capability-report digest plus canonical capability-report artifact. A conformance or confirmatory result rejects an unknown commit/build identity; a development experiment may use a content-addressed dirty tree, but never only `dirty: true`.

The workspace identity contains repository URL or a typed non-VCS source identity, exact commit, dirty state and patch/tree digest when dirty, submodule identities, canonical roots, exclusions/inclusions, source inventory digest, immutable #2100 generation envelope and digest, dependency fingerprints, and generated/vendor policy. Absolute checkout paths are volatile diagnostics only.

The extension identity contains normalized name, semantic version, exact commit or package digest, dirty tree fingerprint when applicable, extension API compatibility, configuration artifact descriptor, and configuration semantic digest. Hash exact canonical configuration bytes even when the extension also publishes a normalized semantic hash; retain both. Secret values must never appear. A caller must replace secret-dependent configuration with a stable non-secret identity or declare reproduction unavailable.

Activated semantics contain sorted semantic-pack/catalog records: pack/catalog ID, version, schema, semantic hash, manifest content hash, source kind, and active-set digest. Record the capability report observed for the run, not merely what the engine version usually supports.

The execution contract contains canonical request artifact(s), #2104 `ObservationDocument` artifact(s), #2101 relation snapshot(s), #2104 `ObservationMappingResult` artifact(s), extension result artifact(s), declared interfaces, finite limits, cache declaration, environment identities that affect results, and exact command/API operation as inert display metadata. Preserve every per-record mapping terminal outcome; an aggregate manifest cannot collapse ambiguous, unmapped, stale, unsupported, or truncated records into a count alone. The actual reproduction operation is a typed API request, not shell evaluation of the display command.

Every component descriptor contains a unique role, normalized relative path, media type, schema name/version where applicable, byte length, raw-file SHA-256, canonical-domain digest where the format defines one, completion status, proof/completeness summary, diagnostics count and digest, work summary and digest, and dependencies on other component roles/digests. Sort descriptors by role then path. Reject dependency cycles, dangling roles, duplicate paths, mismatched byte lengths/hashes, and a declared canonical type whose decoded value does not re-encode to the exact bytes.

Aggregate status is `complete`, `incomplete`, `cancelled`, `exceeded_budget`, `unsupported`, `stale`, or `failed`. `complete` requires every purpose-required component to exist and be complete, all content and domain hashes to validate, capability/configuration/generation identities to agree, and no completeness-affecting deviation. Preserve incomplete component bytes and boundaries; never replace them with an empty complete file.

The manifest digest uses domain `bifrost-extension-run-manifest-v1` and length-frames the canonical deterministic manifest with `manifest_digest` and `volatile` omitted. The full file hash is ordinary SHA-256 over emitted `manifest.json` bytes and may change when volatile measurements change. Reproduction equality compares deterministic manifest digest and deterministic component descriptors first; callers may separately compare volatile fields under a declared measurement protocol.

## Plan of Work

### Milestone 1: implement the public domain model and canonical validator

Under `crates/bifrost-runtime/src/extension/artifacts/`, add cohesive `model.rs`, `validation.rs`, and `codec.rs` modules, re-exported from `extension`. Reuse #2100 versions, paths, digests, capability and completion types and #2101 codec primitives. Do not expose `serde_json::Value` as a public contract. Use private fields and fallible constructors so Rust callers cannot create a manifest that decoding would reject.

Define schema major 1 and reject unknown major versions before interpreting fields. Permit additive optional minor fields only through negotiated #2100 compatibility; strict decoding rejects duplicate keys and unknown required enum values. Validate semantic versions, full commits, lowercase 64-hex digests, sorted/deduplicated features and identities, positive fixed-width counts, normalized paths, artifact DAGs, purpose-required fields, purpose/status compatibility, component completion, and recomputed digests.

Implement one canonical encoder and strict decoder. Decoding always validates and re-encoding a canonical input must reproduce identical bytes. Noncanonical but semantically accepted input may decode and normalize only through an explicit `decode_and_canonicalize_manifest`; `decode_canonical_manifest` rejects any byte difference. This distinction prevents a verifier from accidentally authenticating bytes different from those it hashed.

Milestone acceptance includes pure model tests for every invariant and golden manifests for complete conformance, incomplete development experiment, and confirmatory result with a non-completeness-affecting declared deviation.

### Milestone 2: capture exact Bifrost and extension artifacts

Add a `RunManifestBuilder` that starts from an `ExtensionWorkspace` description so generation and capability identity cannot be manually retyped. It accepts an explicit `EngineBuildIdentity` produced by the packaged build, an `ExtensionIdentity`, a purpose contract, cache declaration, and validated canonical component bytes. Builder methods for #2101 requests/snapshots and #2104 observation/mapping results call their canonical encoders and derive descriptors directly. Never accept a caller-supplied digest for bytes already available.

Make capture fail closed. A missing build commit, source revision, configuration identity, requested semantic version, or required artifact returns `ManifestBuildError` with a stable field path and no manifest. Development experiments may deliberately select typed `Unpinned` identities only where the schema allows them, and the resulting aggregate cannot be `complete` for conformance/confirmatory purposes.

Define cache state as structured evidence: process relationship, persistent-store disposition, semantic artifact disposition, warmup count, and a stable declaration such as `fully_cold` or `process_memory_reused`. Validate combinations; for example, `fully_cold` cannot declare persisted hydration or same-process reuse. This reports existing behavior and creates no persistence promise.

Milestone acceptance builds a manifest entirely from fixture domain values, proves every required identity is captured, and changes exactly the intended descriptor/run digest for each one-field mutation.

### Milestone 3: verify bundles and reproduce without guessing

Add `verify_extension_bundle(root, limits)` with bounded manifest bytes, artifact count, per-file bytes, total bytes, and decode work. Open paths relative to a canonical bundle root, reject symlinks and escapes, hash while streaming, validate canonical registered formats, verify the component DAG, then validate the manifest aggregate. Verification performs no analysis and returns all findings in stable field/path order.

Define `ReproductionResolver` traits for locating an exact Bifrost engine/build, source workspace revision/tree fingerprint, extension build, semantic packs/catalogs, environment identities, and input artifacts. `plan_reproduction` compares every prerequisite before execution and returns either `Ready(ReproductionPlan)` or `Unavailable(ReproductionMismatchReport)`. Mismatch kinds include schema/API incompatibility, engine version/commit/features/build, source repository/commit/tree/submodule/root/exclusion, workspace generation, adapter/IR/capability, dependency, semantic pack/catalog, extension version/commit/configuration, environment, missing artifact, content/canonical digest, cache contract unavailable, and unsupported operation. Each record contains field path, expected, observed when safe, and a remediation hint; secrets and absolute private paths are redacted.

`execute_reproduction` accepts only a validated ready plan. It creates a caller-chosen new staging directory, invokes typed extension operations rather than a shell string, captures canonical artifacts, verifies them, and compares deterministic descriptors. It publishes the staged directory through atomic rename only on success. A mismatch returns the exact component and expected/actual hashes/status; it never updates the original expected bundle or calls a changed result equivalent.

Milestone acceptance proves preflight returns multiple canonically ordered mismatches without execution, exact prerequisites execute once, a changed source/config/limit/component reports the correct mismatch, and a failed rerun leaves both expected bundle and destination absent/unchanged.

### Milestone 4: prove semantics and update public guidance

Add `tests/suite_semantic/extension_artifacts.rs` and register it in `tests/suite_semantic/main.rs`. Use `InlineTestProject` and small canonical fixture files. If the #2100 package-boundary suite lives in `crates/bifrost-runtime/tests`, put pure public-model golden tests there and keep workspace execution tests in `suite_semantic`; do not create a root test binary without process-isolation need.

Test three purpose contracts. Conformance requires expectation and comparison artifacts and rejects deviations that affect the asserted behavior. Development experiments accept explicit unpinned/dirty identities but never claim conformance or confirmatory completion. Confirmatory results require a locked protocol artifact and preserve every deviation; a completeness-affecting deviation forces incomplete aggregate status. None of the fixtures or public enums may contain `Defects4J`, fault localisation, suspiciousness, Top-k, or paper-specific vocabulary.

Test canonical stability in fresh processes and on Linux/macOS/Windows CI. Construction order, hash-map seed, native separator, checkout path, file mtime, and volatile timestamp must not change deterministic digest or deterministic component bytes. Volatile changes must change the full manifest file hash while preserving `manifest_digest`. Source, engine feature, adapter/IR, dependency, pack, request limit, observation, extension configuration, or deterministic result changes must change the appropriate deterministic digest.

Update `docs/src/content/docs/reproduce-analysis.md` to the authoritative schema, directory layout, verification/reproduction API, mismatch examples, and redaction rules. Update `evaluation-evidence.md` with the three purpose classes and bounded claims. Update `cite-bifrost.md` to explain the engine citation plus manifest identity. Document that external template CI and `CITATION.cff` become acceptance work after Apache-2.0 migration; do not add or publish that repository here.

## Concrete Steps

Work from the repository root. Before implementation, inspect current branch/worktree and live #2100/#2101/#2104 state. Do not create or switch branches without explicit user authorization. Update this plan with their exact merged type names and commits.

After model/codec work, run focused runtime tests, using the exact target established by #2100:

    cargo fmt
    cargo test -p brokk-bifrost-runtime extension_artifacts
    node scripts/check-workspace-dependencies.mjs

After bundle and reproduction integration, run:

    cargo test --test suite_semantic extension_artifacts
    cargo test --test suite_semantic extension_semantic_relations
    scripts/check-workspace-packages.sh

Run a fixture command twice in separate processes, save into `mktemp -d` directories, compare every deterministic artifact with `cmp`, and compare `manifest_digest`. Then supply distinct one-field mutations and retain the typed mismatch snapshots as golden tests. Use Rust integration tests for cross-platform CI rather than relying only on shell behavior.

Before pushing implementation, run:

    cargo fmt --check
    scripts/pre-push-gate.sh

If an individual Clippy run is necessary, use the managed helper and required workspace scope:

    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Do not enable NLP for focused validation. Do not write manually named Cargo target directories. Record exact commands, pass counts, deterministic hashes, mismatch examples, package archive hash, and CI links in this plan.

## Validation and Acceptance

Issue #2105's Bifrost API scope is complete only when direct evidence proves all statements below.

A public schema and one shared Rust/JSON validator reject missing identities, unsupported schema/API majors, incompatible minor requirements, malformed/non-lowercase hashes, unsafe paths, duplicate roles/paths, artifact dependency cycles, canonical-byte mismatches, dangling descriptors, and aggregate `complete` with any required incomplete component or completeness-affecting deviation.

The manifest records exact Bifrost package/build/commit/dirty tree/features/profile/target, adapter and semantic IR versions, capability report, workspace repository/revision/roots/exclusions/source inventory/generation/dependencies, extension version/commit/configuration, active semantic packs/catalogs, cache evidence, requests, observations, relation snapshots, results, limits, diagnostics, work, completion, deviations, and content/domain hashes. Secrets, absolute stable paths, dense IDs, analyzer internals, and large embedded graph payloads are absent.

Canonical manifest and registered artifact bytes are identical across repeated fresh processes and supported operating systems for equal deterministic inputs. Construction order, checkout location, mtime, and declared volatile fields do not change `manifest_digest`. Every deterministic validity mutation changes the expected identity or yields a precise validation error.

Bundle verification is bounded, rejects tampering and path escape, recomputes all hashes and canonical encodings, and does not execute analysis. Reproduction preflight checks all prerequisites and returns a canonically ordered typed mismatch report. Exact preflight executes through typed APIs, publishes atomically, and recreates equal deterministic component descriptors and manifest digest or reports the exact differing prerequisite/component.

Conformance, development experiment, and confirmatory result fixtures enforce distinct required fields and permitted claims. The manifest never claims that it validates methodology or runtime causality. Incomplete analytical results remain publishable as incomplete and never become authoritative absence or complete aggregates.

Direct Rust and serialized dispatcher routes produce equal validated manifests and mismatch reports. Package-archive consumers import only documented runtime extension APIs, dependency gates remain green, and focused plus pre-push validation passes.

Public documentation describes the authoritative schema, bundle layout, redaction, cache declarations, purpose semantics, verification, reproduction, mismatch handling, software citation, and bounded claims. The external template's CI use and `CITATION.cff` remain explicitly deferred until Apache-2.0 migration; therefore #2105 may finish its API scope while the parent epic remains open for that publication proof.

## Idempotence and Recovery

Manifest construction, encoding, and bundle verification are pure/read-only over caller-provided values and files. Repeating them is safe. Reproduction never overwrites the expected bundle. It requires a nonexistent or explicitly empty destination, stages beside that destination, verifies before publish, and atomically renames on the same filesystem.

On interruption, retain no valid-looking published result; a staging directory may be inspected and then removed explicitly. On hash or canonicalization failure, report the path and expected/observed digest without rewriting it. On missing prerequisites, return the complete mismatch report without analysis work. On schema incompatibility, reject rather than guessing conversion. An intentional incompatible semantic or canonical-encoding change requires a new manifest schema major and golden compatibility tests.

If #2104's eventual observation schema differs from assumptions here, reference its canonical document and mapping-result digests through generic registered component descriptors; do not copy fields into a competing observation model. If #2100/#2101 source lacks a required exact identity or canonical encoder, amend those contracts rather than scraping debug output.

## Artifacts and Notes

The expected API-only fixture layout is:

    extension-run/
      manifest.json
      inputs/request.json
      inputs/observations.jsonl
      snapshots/relations.jsonl
      mappings/observations.jsonl
      results/result.json
      protocols/expectations.json

Only files relevant to the selected purpose are required. The manifest references each by normalized path and digest. Human README files and citation metadata may be additional content descriptors but are not analyzer-result components.

An illustrative reproduction mismatch is:

    schema: bifrost-extension-reproduction-mismatch-v1
    status: unavailable
    mismatches:
      - kind: engine_feature_set
        path: engine.features
        expected: [python]
        observed: []
        remediation: provide the recorded Bifrost build
      - kind: source_commit
        path: workspace.repository.commit
        expected: 012345...
        observed: abcdef...
        remediation: provide a clean checkout at the recorded commit

Actual canonical output is JSON from the shared codec, not this prose rendering.

## Interfaces and Dependencies

Under `brokk_bifrost_runtime::extension`, define equivalents with private validated fields and read-only accessors:

    pub const EXTENSION_RUN_MANIFEST_SCHEMA: ManifestSchemaVersion;

    pub enum RunPurpose {
        Conformance(ConformanceContract),
        DevelopmentExperiment(DevelopmentExperimentContract),
        ConfirmatoryResult(ConfirmatoryContract),
    }

    pub struct ExtensionRunManifest;
    pub struct EngineRunIdentity;
    pub struct WorkspaceRunIdentity;
    pub struct ExtensionRunIdentity;
    pub struct ActivatedSemanticsIdentity;
    pub struct ExecutionContract;
    pub struct RunComponentDescriptor;
    pub struct RunDeviation;
    pub struct CacheStateDeclaration;
    pub struct VolatileRunMeasurements;
    pub struct ManifestDigest(StableDigest);

    pub struct RunManifestBuilder;

    impl RunManifestBuilder {
        pub fn from_workspace(
            workspace: &ExtensionWorkspace,
            engine: EngineBuildIdentity,
            extension: ExtensionRunIdentity,
            purpose: RunPurpose,
        ) -> Result<Self, ManifestBuildError>;

        pub fn add_canonical_component<T: CanonicalRunArtifact>(
            self,
            role: RunComponentRole,
            path: NormalizedRelativePath,
            artifact: &T,
        ) -> Result<Self, ManifestBuildError>;

        pub fn build(self) -> Result<ExtensionRunManifest, ManifestBuildError>;
    }

    pub fn encode_run_manifest_json(
        manifest: &ExtensionRunManifest,
    ) -> Result<Vec<u8>, ManifestCodecError>;

    pub fn decode_canonical_run_manifest_json(
        bytes: &[u8],
    ) -> Result<ExtensionRunManifest, ManifestCodecError>;

    pub fn verify_extension_bundle(
        root: &Path,
        limits: BundleVerificationLimits,
    ) -> Result<VerifiedExtensionBundle, BundleVerificationReport>;

    pub fn plan_reproduction<R: ReproductionResolver>(
        bundle: &VerifiedExtensionBundle,
        resolver: &R,
    ) -> Result<ReproductionPlan, ReproductionMismatchReport>;

    pub fn execute_reproduction(
        plan: ReproductionPlan,
        destination: &Path,
        cancellation: &ExtensionCancellation,
    ) -> Result<ReproductionOutcome, ReproductionError>;

`CanonicalRunArtifact` is sealed or otherwise restricted to Bifrost-registered canonical domain types. Opaque extension files use an explicit byte descriptor and cannot claim canonical-domain equivalence. Reproduction resolver traits describe lookup and typed execution; they never expose or execute arbitrary shell text.

The runtime model depends on #2100 versions, digests, normalized paths, workspace generation/capabilities, cancellation, work and completion; #2101 relation request/snapshot codecs and digests; #2104 observation and mapping codecs/digests; `serde`/`serde_json` internally; SHA-256 already used by the workspace; `semver`; and standard I/O/path types. It must not depend on MCP, LSP, Git libraries, a hosted tracking service, external extensions, or paper-specific packages. Git/process acquisition helpers may stay behind facade-owned adapters, but the validated public values stay in runtime.

Dependency order is: #2100 establishes the extension package and immutable identity; #2101 establishes canonical semantic request/snapshot artifacts; #2102/#2103 fill relation kinds without changing the manifest envelope; #2104 establishes canonical observation and mapping components; #2105 composes all of them. API model/codec work can begin after #2100/#2101; full reproduction fixtures wait for #2104. External template CI and `CITATION.cff` wait for Apache-2.0 publication authorization.

Plan revision note (2026-08-13): Created the initial API-only #2105 ExecPlan after auditing the live issue, extension plans, current provenance/evidence artifacts, lifecycle policy, and public reproduction guidance. It fixes one shared canonical codec, external content-addressed components, deterministic-versus-volatile identity, three purpose contracts, fail-closed aggregate completeness, typed multi-mismatch preflight, and atomic non-overwriting reproduction while deferring public-template citation proof until license migration.
