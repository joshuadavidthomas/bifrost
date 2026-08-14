# Establish the stable extension application boundary

This ExecPlan is a living document. Maintain it according to `.agents/PLANS.md` from the repository root. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current whenever work stops, a milestone completes, or the design changes.

This plan implements GitHub issue [#2100](https://github.com/BrokkAi/bifrost/issues/2100), the API-boundary child of epic #2099. It is deliberately limited to public API and package-seam work. It does not publish a package, create the external template repository, add research algorithms, or perform license migration. Later child issues extend the experimental semantic relation vocabulary through the boundary defined here.

## Purpose / Big Picture

After this change, an independent Rust application can depend on the published `brokk-bifrost-runtime` package, open a source workspace with platform-independent paths, learn exactly which languages and semantic capabilities are supported, compare an immutable workspace-generation identity, execute a bounded structural query, and execute one bounded source-backed semantic request. The application receives stable, typed metadata about evidence, completeness, diagnostics, work, provenance, cancellation, and limits without importing an analyzer, store, language adapter, MCP host, or LSP host.

The same request and result values have one canonical JSON representation. A caller can therefore run the application boundary in process or pass the same envelope to an out-of-process host later without inventing a second semantic contract. The first semantic operation is explicitly experimental and exists to prove the seam; #2101 owns the full bounded relation-snapshot vocabulary and its canonical JSON Lines projection.

The behavior is visible by building and running the package-archive consumer added to `scripts/check-workspace-packages.sh`. It opens a checked-in temporary fixture, prints the stable API version, generation, and capability report, executes one structural query and one bounded experimental control-flow request, and verifies that direct and serialized results are equal.

## Progress

- [x] (2026-08-13 13:30Z) Read `.agents/PLANS.md`, the #2099 epic plan, live issue #2100, and the related #2101 boundary.
- [x] (2026-08-13 13:45Z) Audited the root facade, `brokk-bifrost-runtime`, workspace construction, semantic identities, capability tables, bounded semantic outcomes, and package-archive consumer checks at commit `4496c7f95`.
- [x] (2026-08-13 14:05Z) Selected the existing publishable runtime crate as the stable extension package and fixed the ownership split between #2100 and #2101.
- [x] (2026-08-13 15:18Z) Re-read the repository and ExecPlan instructions, fetched `origin`, verified #2100 is open and assigned, found no overlapping pull request, and attached the clean worktree at current `origin/master` to `dave/issue-2100-extension-sdk-boundary`.
- [x] (2026-08-13 16:02Z) Implemented the stable version, workspace, identity, capability, limit, cancellation, metadata, diagnostic, provenance, and canonical JSON types in `crates/bifrost-runtime/src/extension/`.
- [x] (2026-08-13 16:28Z) Added an immutable source-capture boundary: `ExtensionWorkspace::open` freezes the bounded filesystem inventory in an `OverlayProject` snapshot before analyzer construction, and generation hashing reads only that same frozen project state without exposing stores, adapters, arenas, or language modules.
- [x] (2026-08-13 16:02Z) Wrapped bounded structural execution and added the explicitly experimental procedure-local control-flow seam with source-backed stable node identities.
- [x] (2026-08-13 16:31Z) Added focused unit/integration tests, rustdoc compile-fail privacy cases, an archive-only consumer and dependency-tree assertion, platform-neutral path fixtures, and a Linux/macOS/Windows CI matrix.
- [ ] Run focused featureless validation, package checks, dependency checks, formatting, and the full pre-push gate before any authorized push (completed: focused runtime 7/7, extension 5/5, compile-fail 3/3, Clippy, formatting, dependency checks 15/15, archive-only consumer; blocked outside this diff: the escalated full gate ran 4,990 passing tests before the existing deterministic `typescript_type_reference_prefers_interface_over_same_named_const` failure, which also failed alone; no analyzer source differs from `origin/master`).
- [ ] Record implementation commits, exact test counts, archive-consumer output, CI links, and review outcome here (completed: rebased commits, local evidence, ready PR #2113; remaining: CI and review outcome).

## Surprises & Discoveries

- Observation: The root `brokk-bifrost` package is not the minimal independent-application dependency because its manifest depends on and its `src/lib.rs` publicly re-exports both `brokk-bifrost-mcp` and `brokk-bifrost-lsp`.
  Evidence: `Cargo.toml` lists both transport crates; `src/lib.rs` exports `lsp`, MCP modules, `SearchToolsService`, and watcher types.

- Observation: `brokk-bifrost-runtime` already has the right dependency direction and protocol-neutral execution responsibility, but its crate documentation calls it an internal detail and its main type borrows `WorkspaceAnalyzer`.
  Evidence: `crates/bifrost-runtime/src/lib.rs` says “no stability guarantees”; `CodeIntelligenceRuntime<'a>` in `crates/bifrost-runtime/src/code_intelligence.rs` stores `&'a WorkspaceAnalyzer` and never owns workspace acquisition.

- Observation: Existing semantic foundations already distinguish complete, ambiguous, unknown, unsupported, unproven, exceeded-budget, and cancelled outcomes, and already carry total capability tables. They are suitable implementation inputs but are not serialized public extension contracts.
  Evidence: `crates/bifrost-analysis/src/analyzer/semantic/provider.rs` defines `SemanticOutcome<T>` and finite `SemanticBudget`; `semantic/capabilities.rs` defines total `SemanticCapabilities` over `SemanticCapability::ALL`.

- Observation: Existing `SemanticArtifactKey` is the correct per-artifact validity input, while ICFG node and edge IDs are dense snapshot-local aliases. Neither is a complete workspace-generation API.
  Evidence: `semantic/ids.rs` fingerprints mount, relative path, language, source revision, adapter, IR, configuration, and dependencies; `semantic/icfg.rs` allocates `IcfgNodeId` and `IcfgEdgeId` from vector indexes.

- Observation: The package-set script already unpacks every `.crate` archive and compiles facade and analysis-only consumers, so the strongest seam test belongs there rather than in a path-dependent example.
  Evidence: `scripts/check-workspace-packages.sh` creates temporary consumers against unpacked archives and local registry patches.

- Observation: The Bifrost MCP code-intelligence tools advertised by the installed skills were not callable in this task. Source inspection therefore used bounded `rg` and `sed` reads.
  Evidence: the available tool inventory contained no Bifrost search, summary, or source tool.

- Observation: The package archive gate's full-feature facade consumer is substantially slower than its new runtime-only extension consumer.
  Evidence: all 19 archives packaged successfully; after the full-feature build completed, the runtime-only consumer printed API major `1`, a generation digest, and `src/lib.rs`, and the complete script passed with a transport-free dependency tree.

- Observation: The sandboxed pre-push run cannot exercise three pipe-backed MCP benchmark tests or uv's Python cache.
  Evidence: the first run failed only with macOS `Operation not permitted` in `mcp_session.rs` and uv cache access. The escalated rerun passed those tests and continued through the full suite.

## Decision Log

- Decision: Stabilize `brokk-bifrost-runtime` as the independent-application package; do not create a new SDK crate for #2100.
  Rationale: Runtime already sits above analysis and policy, below both transports, is publishable, and exists specifically for protocol-neutral execution. A new crate would duplicate this ownership boundary, while the root facade would force transport dependencies on applications. This is an ownership and compatibility change, not a new compilation boundary.
  Date/Author: 2026-08-13 / Codex

- Decision: Put the supported contract under `brokk_bifrost_runtime::extension`, and re-export that module from `brokk_bifrost::extension` only as a convenience.
  Rationale: The module path makes the stable surface reviewable and prevents the runtime’s legacy broad `analyzer` and `policy` re-exports from becoming stable by implication. Direct runtime dependency proves that MCP and LSP are absent.
  Date/Author: 2026-08-13 / Codex

- Decision: Stability is per type and operation, not per crate. The version envelope, workspace lifecycle, generation, capability report, source identity, result metadata, diagnostic model, limits, cancellation behavior, and canonical JSON encoding are stable in API major 1. Semantic relation kinds and payloads live under `extension::experimental` until their owning issues promote them.
  Rationale: #2100 requires a durable application seam while #2101 through #2104 are still defining relation and observation vocabularies. Making all runtime exports stable would violate the issue’s non-goal; making nothing stable would provide no useful boundary.
  Date/Author: 2026-08-13 / Codex

- Decision: Compatibility negotiation uses an exact major version and an inclusive minor range. Unknown major versions are rejected. Additive optional fields and new experimental capability identifiers may increase the minor version; removing a field, changing meaning, or changing canonical encoding requires a new major.
  Rationale: A single integer cannot distinguish incompatibility from additive evolution, and Cargo package version is not the protocol schema version.
  Date/Author: 2026-08-13 / Codex

- Decision: `ExtensionWorkspace` owns one analyzer snapshot and never exposes it. Refresh is explicit and returns a new `ExtensionWorkspace`; no method mutates the generation in place.
  Rationale: Every result can then name one immutable generation, stale requests can be rejected deterministically, and extensions cannot accidentally retain analyzer references across updates.
  Date/Author: 2026-08-13 / Codex

- Decision: The generation digest is computed from the analyzer’s captured sorted source inventory plus canonical roots, analyzer configuration fingerprint, language/adapter/IR identities, dependency evidence, build identity, and extension API major. It must come from the same captured state the analyzer queries, not from a second filesystem walk before or after construction.
  Rationale: A second walk races edits and could label results with a generation that was never analyzed. A process-local ordinal cannot compare reopened workspaces or serialized results.
  Date/Author: 2026-08-13 / Codex

- Decision: The #2100 semantic seam implements only a bounded, procedure-local control-flow projection under `extension::experimental`; #2101 replaces or extends its experimental payload with the general relation snapshot.
  Rationale: #2100 acceptance requires one source-backed bounded semantic request, but #2101 explicitly owns seeds, directions, call depth, graph edges, boundaries, canonical JSONL, and performance evidence. Keeping the first payload experimental prevents this seam proof from prematurely freezing #2101’s design.
  Date/Author: 2026-08-13 / Codex

- Decision: Canonical JSON is emitted by library functions from the shared Rust request/result model. #2100 does not add stdin/stdout framing, a daemon, JSON-RPC, MCP, or LSP.
  Rationale: Protocol-neutral equivalence concerns values and encoding. Process hosting is a separate transport concern and would violate the minimal boundary.
  Date/Author: 2026-08-13 / Codex

- Decision: Freeze every bounded filesystem source into an `OverlayProject` snapshot before analyzer construction and derive the generation from that same frozen project.
  Rationale: This uses the repository's existing immutable project-snapshot abstraction, prevents post-build filesystem edits from racing generation construction, and avoids widening analyzer/store APIs merely to reveal internal file-state rows. A fresh `open` captures a fresh snapshot; an existing workspace remains immutable.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The existing runtime package, rather than a new crate or the transport-heavy facade, now owns the supported application seam. The implementation proves immutable generation behavior, public/private separation, serialized equivalence, bounded source-backed structural and experimental control-flow execution, and archive-only consumption without transport dependencies.

Implementation is now checkpointed around the supported `brokk_bifrost_runtime::extension` seam. Direct and JSON-decoded structural execution are byte-equivalent, experimental control flow produces source-backed nodes and edges, immutable generations change on a source edit, cancellation and noncanonical paths are typed, and an unpacked runtime-only consumer builds and runs without MCP or LSP. Remaining work is the analyzer-owned captured-state generation projection, compile-fail/cross-platform pins, completion of the full gate, and final review/CI evidence.

## Context and Orientation

All paths are relative to the Bifrost repository root.

The root `Cargo.toml` defines the workspace and the published `brokk-bifrost` facade. That facade depends on analysis, policy, runtime, semantic packs, MCP, and LSP. It remains the CLI and convenience facade, but it is too broad to be the minimum application dependency.

`crates/bifrost-runtime/Cargo.toml` defines the already-published `brokk-bifrost-runtime` package. `crates/bifrost-runtime/src/lib.rs` currently labels it internal and exports the full analysis module and policy crate. `crates/bifrost-runtime/src/code_intelligence.rs` provides protocol-neutral structural-query and policy execution against a caller-owned `WorkspaceAnalyzer`. The new `extension` module will own the stable surface; existing exports remain available but explicitly unstable and outside the compatibility promise.

`crates/bifrost-analysis/src/analyzer/workspace.rs` owns `WorkspaceAnalyzer`, which is either empty or a multi-language analyzer. It builds from a `Project` and `AnalyzerConfig`, routes semantic materialization, and creates the existing ICFG and oracle providers. It must expose only a narrow opaque bridge needed to construct the public generation and capability report. The bridge must not return `AnalyzerStore`, SQLite connections, concrete language analyzers, semantic arenas, or solver structures.

`crates/bifrost-core/src/analyzer/project.rs` defines `FilesystemProject`, normalized `Path`/`PathBuf` handling, source snapshots, and overlay revisions. `FilesystemProject::new` canonicalizes the root and detects languages. `ExtensionWorkspace::open` uses this project behavior but returns extension-owned errors and identities, so consumers never name `Project`, `ProjectFile`, or `WorkspaceAnalyzer`.

`crates/bifrost-analysis/src/analyzer/semantic/ids.rs` defines content-addressed semantic identities. A semantic artifact key includes mount, normalized workspace-relative path, language, disk or overlay content revision, adapter version, semantic IR version, configuration fingerprint, and dependency fingerprint. The new public identity types project these facts into strings/digests and source spans. They never expose artifact-local numeric arena handles.

`crates/bifrost-analysis/src/analyzer/semantic/capabilities.rs` defines a total per-language primitive capability table. “Total” means every known capability is reported as complete, partial, or unsupported rather than being omitted. The extension capability report adds operation stability and API availability without changing that semantic meaning.

`crates/bifrost-analysis/src/analyzer/semantic/provider.rs` defines finite semantic work budgets and typed incomplete outcomes. `crates/bifrost-analysis/src/analyzer/semantic/icfg.rs` defines demand-built bounded control-flow snapshots, but its dense node identifiers and internal handles are generation-local implementation details. The experimental #2100 projection renders source-backed nodes and retains proof, completeness, work, and boundaries.

A stable contract is one whose names, field meanings, variant meanings, and canonical encoding follow the compatibility policy. An experimental capability is discoverable and callable but may change incompatibly in a minor API revision; every request and result names its stability. “Canonical JSON” means UTF-8 JSON with one deterministic representation: object fields emitted by the model’s declared field order, arrays sorted by documented stable keys, no insignificant whitespace, a trailing newline only in file helpers, and no platform-native path separators.

## Plan of Work

### Milestone 1: make the runtime package’s stable surface explicit

In `crates/bifrost-runtime/src/lib.rs`, revise crate documentation to say that only `extension` is a supported application API. Add `pub mod extension;`. Keep `code_intelligence`, `analyzer`, and `policy` available for existing hosts, but document them as unstable implementation interfaces; do not re-export any of them from `extension`.

Create `crates/bifrost-runtime/src/extension/mod.rs`, `version.rs`, `identity.rs`, `capabilities.rs`, `limits.rs`, `outcome.rs`, `workspace.rs`, `structural.rs`, `codec.rs`, and `experimental.rs`. These files form one cohesive module, not separate crates. Add `serde`, `serde_json`, and `sha2` to `crates/bifrost-runtime/Cargo.toml`; reuse the workspace-pinned versions. Do not add MCP, LSP, SQLite, or language-crate dependencies.

Define API version and compatibility negotiation first. Deserialize requests only after checking the envelope version. Reject an unsupported major, a requested minimum minor newer than this library, an inverted minor range, duplicate/unknown required capabilities, and noncanonical path or digest syntax with typed `ExtensionCompatibilityError` or `ExtensionDecodeError` values. Unknown optional JSON fields are allowed within a major; unknown enum variants are rejected unless carried in the explicit experimental capability string namespace.

Add `pub use brokk_bifrost_runtime::extension;` to `src/lib.rs` so facade users can reach the same module. Do not copy types or define a parallel wrapper in the facade.

At milestone completion, `cargo check -p brokk-bifrost-runtime` succeeds and version unit tests prove current-version acceptance and unsupported-major rejection.

### Milestone 2: own an immutable workspace and publish exact generation/capability identity

Implement `ExtensionWorkspace::open` in `crates/bifrost-runtime/src/extension/workspace.rs`. It accepts one or more `PathBuf` roots through `ExtensionWorkspaceOptions`, validates positive open limits before filesystem work, canonicalizes roots using `Path`, and builds the existing filesystem-backed analyzer. For #2100, support one root in behavior tests and model roots as a nonempty ordered collection so multi-root support does not require a schema break. Do not accept string paths in the Rust API. Serialized paths are normalized slash-separated relative paths; absolute local roots appear only in noncanonical diagnostic display, never in stable node identity.

Add an opaque projection in `crates/bifrost-analysis/src/analyzer/workspace.rs` named `extension_snapshot_identity`. It returns a small public, doc-hidden `ExtensionSnapshotIdentity` declared next to `WorkspaceAnalyzer`. The value contains only sorted normalized root identities, sorted analyzed file content fingerprints, sorted languages, analyzer configuration fingerprint, adapter/IR identities, and dependency fingerprints. Derive it from the analyzer’s captured file states after a successful build so it cannot race a second filesystem read. If current analyzer internals cannot produce the complete set from captured state, add one shared internal iterator over immutable file-state identity; do not reread source and do not expose the iterator publicly.

The runtime hashes that projection with a length-delimited SHA-256 domain `brokk-bifrost-extension-workspace-generation-v1`, the Bifrost package/build identity, and extension API major. Store the resulting lowercase 64-hex digest in `WorkspaceGeneration`. Also retain the normalized generation inputs needed for provenance, but keep absolute roots separate from the canonical digest/display model. Add a debug assertion that re-hashing the stored envelope reproduces the digest.

Build `ExtensionCapabilityReport` from every language present in the workspace and every known semantic primitive. It reports `Complete`, `Partial`, or `Unsupported` for each `(language, relation)` pair and separately reports operation stability as `Stable` or `Experimental { since_minor }`. Empty/unknown-language workspaces still return a total report. Never infer unsupported from a missing table row.

At milestone completion, opening the same unchanged fixture twice produces the same generation; changing one source byte, analyzer option, adapter/IR identity test input, dependency fingerprint, or root identity changes it. Mutating a source after opening does not change the existing workspace’s generation; a fresh `open` produces a new generation. Unix and Windows path-spelling fixtures canonicalize to the same serialized relative identity without constructing invalid native paths on the host.

### Milestone 3: wrap bounded execution and establish the minimal semantic relation seam

In `crates/bifrost-runtime/src/extension/limits.rs`, define one validated `ExtensionLimits` value covering result items, result bytes, diagnostics, source bytes, semantic work dimensions, semantic files, traversal steps, call depth, nodes, and edges. Every dimension is positive and has a conservative default. Convert it internally to `CodeQueryExecutionLimits`, `SemanticBudget`, `SemanticExecutionBudget`, and `IcfgSnapshotLimits`. A conversion failure is a construction error, not an execution diagnostic.

Use the existing clonable `CancellationToken` behind a stable `ExtensionCancellation` wrapper. The wrapper exposes `new`, `cancel`, and `is_cancelled`; it does not expose test counters or implementation atomics. Each execution call accepts `&ExtensionCancellation`. A pre-cancelled request returns a typed cancelled outcome with the request generation and zero or bounded work, never a generic error.

In `structural.rs`, define a stable `StructuralRequest` that contains the API envelope, expected generation, typed `CodeQuery`, and `ExtensionLimits`. It is acceptable for `CodeQuery` itself to remain an existing public input type, but the result must be projected into extension-owned `StructuralResult` records rather than returning `CodeQueryResponse`, because current response types are serialize-only and expose a much broader evolving surface. Preserve source-backed path/range, evidence, completeness, diagnostics, work, limits, and provenance. Reject an expected generation different from the workspace before execution.

Define the minimal v1 `SemanticRelationRequest` and `SemanticRelationSnapshot` shapes that #2101 will extend, selecting procedure-local `ControlFlow` only. Its seed is a stable source location consisting of a normalized relative path and UTF-8 byte range, plus finite limits and expected generation. Resolve the containing procedure structurally, materialize its existing semantic CFG, and project program points and control edges into the single shared relation model. Every projected node has a stable semantic identity derived from generation, semantic artifact key, exact source anchor, semantic role, and stable local discriminator. Numeric node/edge aliases may be present for compact adjacency only and are documented as valid solely inside that snapshot. Project proof, completeness, exact source mappings, contributing evidence, semantic work, diagnostics, cancellation, and typed incomplete boundaries. #2101 adds scopes, directions, call/return relations, JSONL framing, and the remaining reserved relation kinds without creating a parallel semantic API.

Every successful or analytically incomplete call returns `ExtensionOutcome<T>` containing one authoritative completion field plus metadata: API version, operation stability, workspace generation, capability identity, limits, diagnostics, work, and provenance. Reserve `Result<_, ExtensionError>` for malformed requests, incompatible versions, stale generations, workspace acquisition failures, and violated internal invariants. Stale generation is always an error detected before semantic work; it is not also a result status or boundary.

At milestone completion, one Rust integration fixture opens a TypeScript or Rust project, runs a structural declaration query, runs procedure control flow, and asserts source-backed results. Near-miss seeds, unsupported language files, tiny limits, pre-cancellation, and stale generations produce their distinct typed outcomes.

### Milestone 4: define one canonical serialized value contract

In `codec.rs`, define `ExtensionRequest` and `ExtensionResponse` tagged enums over stable structural operations and explicitly namespaced experimental semantic operations. Implement `decode_request_json`, `encode_request_json`, and `encode_response_json`; all use the Rust domain values above. Do not create DTOs whose semantics can drift from the in-process types.

Canonicalize before encoding. Sort capabilities by language then stable capability identifier; diagnostics by code then source identity; structural results by normalized path, start byte, end byte, and stable identity; semantic nodes and edges by stable identity; provenance and evidence by their canonical hashes. Reject duplicate stable IDs and maps with duplicate JSON keys. Encode `u64`/`usize` work and limit values as JSON integers after checked conversion to the schema’s fixed-width unsigned representation. Normalize serialized relative paths to `/` on every host and reject `..`, absolute paths, empty segments, NUL, and backslashes at decode boundaries.

Direct and serialized execution share `ExtensionWorkspace::execute(ExtensionRequest, &ExtensionCancellation)`. The codec does not read files or execute requests. Add golden JSON fixtures under `crates/bifrost-runtime/tests/fixtures/extension/` for version negotiation, workspace description, structural results, complete semantic results, and each incomplete terminal state. JSON Lines is deferred to #2101 because it depends on final relation-record framing.

At milestone completion, decode-encode-decode preserves equality, two fresh processes emit byte-identical canonical responses for the unchanged fixture, and direct versus JSON-decoded requests produce equal domain results.

### Milestone 5: prove package and dependency isolation

Add focused runtime integration tests in `crates/bifrost-runtime/tests/extension/` with `main.rs` as the suite harness and one module each for `version`, `workspace`, `structural`, `semantic_relations`, and `codec`. Use `tests/common/inline_project.rs` only through the existing runtime test pattern when a small inline fixture is sufficient. Because these tests belong to a workspace crate, do not create a root `tests/*.rs` binary.

Add compile-fail UI cases under `crates/bifrost-runtime/tests/ui/extension/` using the repository’s existing compile-test mechanism if one exists at implementation time; otherwise add `trybuild` as a dev dependency only after confirming no shared helper exists. The positive case imports only `brokk_bifrost_runtime::extension`. Negative cases attempt to access the private analyzer field, construct `WorkspaceGeneration` with an unchecked digest, convert a dense semantic ID into a stable ID, and import MCP/LSP through runtime. Pin expected diagnostics narrowly enough to prove privacy without depending on compiler prose.

Extend `scripts/check-workspace-packages.sh` with an `extension-consumer` that depends only on the unpacked `brokk-bifrost-runtime` archive and its transitive package patches. Its source uses only `brokk_bifrost_runtime::extension`, opens a fixture path through `PathBuf`, prints description metadata, runs both required requests, round-trips canonical JSON, and exits nonzero on incomplete fixture results. Assert with `cargo tree -p bifrost-extension-package-consumer` that neither `brokk-bifrost-mcp` nor `brokk-bifrost-lsp` appears. Keep the root facade consumer unchanged.

Extend `scripts/check-workspace-dependencies.mjs` and its tests only as needed to pin that runtime cannot depend on MCP or LSP; the current allowed-dependency map already expresses this invariant, so prefer adding a regression assertion rather than changing the dependency set. No release-inventory edit is needed because no crate is added.

Add the archive consumer command to Linux, macOS, and Windows CI. The shell package-set script is Unix-only, so factor the extension-consumer generation/check into a small Node script using `path` and `spawnSync`, call it from the shell script, and call it directly on Windows. Do not normalize filesystem paths with string replacement inside the Rust API.

At milestone completion, the archive-only consumer runs with no Bifrost checkout in its dependency paths, the dependency tree contains no transports, and all three operating-system jobs pass.

## Concrete Steps

Work from the repository root. Before implementation, inspect `git status --short --branch`, refresh the live issue and overlapping pull-request state, and attach to an authorized branch. Repository instructions prohibit branch changes without explicit user direction.

Implement milestone 1 and run:

    cargo fmt --all -- --check
    cargo test -p brokk-bifrost-runtime extension::version

Expect the current API version to negotiate successfully and an unsupported major to return `ExtensionCompatibilityError::UnsupportedMajor`.

Implement milestone 2 and run the runtime workspace tests:

    cargo test -p brokk-bifrost-runtime --test extension workspace

Expect unchanged reopen equality, all named identity-input mutations to change the digest, existing-generation immutability after a disk edit, and a changed generation after reopen.

Implement milestone 3 and run:

    cargo test -p brokk-bifrost-runtime --test extension structural semantic_relations

Expect the positive fixture to return at least one source-backed structural result and at least one control-flow edge. Expect unsupported, stale, cancelled, and each exceeded limit to have different typed statuses.

Implement milestone 4 and run:

    cargo test -p brokk-bifrost-runtime --test extension codec

Run the canonical fixture executable twice into files created by `mktemp`, compare them with `cmp`, and compare SHA-256 hashes. Use the repository’s Rust test to make this platform independent in CI rather than adding a shell-only assertion.

Implement milestone 5 and run:

    node scripts/check-workspace-dependencies.mjs
    node --test scripts/check-workspace-dependencies.test.mjs
    scripts/check-workspace-packages.sh

The package script should end with a message that includes the archive-only extension consumer. Inspect its generated dependency tree and expect no line containing `brokk-bifrost-mcp` or `brokk-bifrost-lsp`.

Run formatting and focused featureless validation:

    cargo fmt
    cargo test -p brokk-bifrost-runtime
    cargo test --test suite_semantic

This issue does not touch NLP or Python. Do not enable `nlp` for routine focused validation. Before an authorized push, check disk space and run the repository gate:

    scripts/pre-push-gate.sh

If running all-feature Clippy separately, use the managed isolated target:

    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Record exact commands, results, test counts, package archive hashes, and CI URLs in `Progress` and `Artifacts and Notes` before closing #2100.

## Validation and Acceptance

#2100 is complete only when all of the following have direct evidence.

An unpacked-package consumer whose manifest directly depends only on `brokk-bifrost-runtime` compiles and runs. `cargo tree` proves MCP and LSP are absent. The consumer imports only `brokk_bifrost_runtime::extension` and cannot access a workspace analyzer, SQLite, language modules, semantic arenas, solver worklists, MCP, or LSP.

The consumer opens a fixture from a `PathBuf`, obtains an immutable deterministic `WorkspaceGeneration`, and receives a total capability report. Reopening unchanged source reproduces the generation; changing every declared validity input changes it. An already-open workspace retains its generation and rejects a request naming another generation.

The consumer executes one bounded structural query and one bounded experimental semantic control-flow request. Both render normalized source-backed paths and ranges and carry API version, generation, stability, proof/completeness, diagnostics, work, limits, and provenance. Complete absence cannot be confused with unsupported, unknown, unproven, truncated, exceeded-budget, cancelled, invalid seed, or stale generation.

The current API version is accepted. An unsupported major and too-new required minor are rejected before execution. Stable types and operations are distinguished in both Rust and JSON from experimental semantic capabilities.

Direct Rust and JSON-decoded requests produce equal domain responses. Canonical JSON is byte-identical across repeated fresh processes, and path fixtures demonstrate platform-independent serialized identities on Linux, macOS, and Windows. The schema contains no native absolute path in stable identities.

Compile-fail tests prove private implementation fields and dense IDs cannot be used as public stable identities. Dependency checks prevent a runtime-to-transport edge. The package archive includes every required extension source file and the archive-only consumer succeeds.

Focused tests, the package set, dependency checks, formatting, the pre-push gate, review, and CI are green. The issue and this plan link the implementation evidence. Package publication and release remain outside this issue and require separate authorization.

## Idempotence and Recovery

Workspace open and request execution are read-only with respect to source files. Repeating them is safe. Canonical encoders allocate and return bytes; file-writing examples use a temporary file followed by atomic rename and never overwrite a valid artifact before successful encoding.

If generation construction fails partway through workspace acquisition, return `ExtensionWorkspaceError` and drop the incomplete analyzer; do not publish a generation. If source identity cannot be derived from the same captured analyzer state, stop and correct the analysis bridge rather than hashing a second filesystem walk.

If a schema changes before release, update golden fixtures and compatibility tests together. An incompatible stable change increments the API major. An incompatible experimental payload change increments the minor and its `since_minor`/capability identity; it does not silently reuse the prior encoding.

Package checks create their temporary directories with `mktemp` and remove them through existing traps. Cargo validation uses the ordinary workspace target or `scripts/with-isolated-cargo-target.sh`; do not create manually named `/tmp/bifrost-*` targets.

No publication, tag, release, external repository, or license change is part of this plan. Failure in those areas cannot be “recovered” by doing them from this issue.

## Artifacts and Notes

Initial source evidence at `4496c7f95`:

    Cargo.toml
        facade depends on analysis, runtime, MCP, and LSP

    crates/bifrost-runtime/src/lib.rs
        runtime is publishable but currently documented as internal

    crates/bifrost-runtime/src/code_intelligence.rs
        protocol-neutral execution borrows WorkspaceAnalyzer

    crates/bifrost-analysis/src/analyzer/workspace.rs
        WorkspaceAnalyzer owns the analyzed generation and routes semantic providers

    crates/bifrost-analysis/src/analyzer/semantic/ids.rs
        SemanticArtifactKey fingerprints source, adapter, IR, configuration, and dependencies

    crates/bifrost-analysis/src/analyzer/semantic/capabilities.rs
        total complete/partial/unsupported capability table

    crates/bifrost-analysis/src/analyzer/semantic/provider.rs
        finite semantic work and typed incomplete outcomes

    crates/bifrost-analysis/src/analyzer/semantic/icfg.rs
        bounded ICFG uses dense snapshot-local IDs

    scripts/check-workspace-packages.sh
        archive creation and external-style temporary consumers

Implementation evidence must be appended here as concise transcripts: the negotiated version, one generation digest, a capability excerpt, one structural result, one experimental control-flow edge, distinct incomplete statuses, canonical JSON hash, transport-free dependency tree excerpt, package archive hash, and CI links.

Checkpoint and local validation evidence on 2026-08-13:

    commit 048117906
        Establish the extension SDK boundary

    completion commit
        Complete extension boundary validation (rebased onto 11f0c2f43)

    pull request
        https://github.com/BrokkAi/bifrost/pull/2113 (ready, base master)

    cargo test -p brokk-bifrost-runtime
        7 passed; 0 failed

    cargo test -p brokk-bifrost-runtime --test extension
        5 passed; 0 failed

    cargo test -p brokk-bifrost-runtime --doc
        3 compile-fail tests passed

    node --test scripts/check-workspace-dependencies.test.mjs
        15 passed; 0 failed

    scripts/check-workspace-packages.sh
        Packaged brokk-bifrost-runtime: 26164 bytes
        1 04373d64b684dcd8f8b377d23b6d7635f4a1b0a0cbaf20e79e119fbee2c7ca8f src/lib.rs
        archive-only extension consumer passed; cargo tree contained no MCP or LSP

    scripts/pre-push-gate.sh
        sandbox run: permission failures only
        escalated run: 4990 passed before unrelated TypeScript definition failure
        focused rerun reproduced the same existing analyzer failure alone

## Interfaces and Dependencies

In `crates/bifrost-runtime/src/extension/version.rs`, define these stable equivalents. Fields stay private; construction validates invariants.

    pub const EXTENSION_API_VERSION: ExtensionApiVersion;

    pub struct ExtensionApiVersion {
        pub major: u16,
        pub minor: u16,
    }

    pub struct ExtensionCompatibility {
        pub major: u16,
        pub minimum_minor: u16,
        pub maximum_minor: u16,
        pub required_capabilities: Box<[ExtensionCapabilityId]>,
    }

    pub enum ApiStability {
        Stable,
        Experimental { since_minor: u16 },
    }

    pub fn negotiate_extension_api(
        requested: &ExtensionCompatibility,
    ) -> Result<NegotiatedExtensionApi, ExtensionCompatibilityError>;

In `identity.rs`, define validated, serializable identity values. `StableDigest` is an extension-owned lowercase SHA-256 wrapper, not the analysis type with the same conceptual role.

    pub struct WorkspaceGeneration(StableDigest);
    pub struct StableSourceId(StableDigest);
    pub struct StableSemanticNodeId(StableDigest);
    pub struct NormalizedRelativePath(Box<str>);
    pub struct SourceSpan {
        pub path: NormalizedRelativePath,
        pub start_utf8_byte: u64,
        pub end_utf8_byte: u64,
    }

    pub struct WorkspaceGenerationEnvelope {
        pub schema: WorkspaceGenerationSchemaVersion,
        pub build_identity: Box<str>,
        pub roots: Box<[WorkspaceRootIdentity]>,
        pub source_inventory: StableDigest,
        pub analyzer_configuration: StableDigest,
        pub adapters: Box<[AdapterIdentity]>,
        pub dependencies: Box<[DependencyIdentity]>,
    }

In `capabilities.rs`, report a total table and stability separately.

    pub struct ExtensionCapabilityReport {
        pub generation: WorkspaceGeneration,
        pub languages: Box<[LanguageCapabilityReport]>,
        pub operations: Box<[OperationCapability]>,
    }

    pub enum CapabilitySupport {
        Complete,
        Partial,
        Unsupported,
    }

    pub struct OperationCapability {
        pub id: ExtensionCapabilityId,
        pub stability: ApiStability,
        pub support: CapabilitySupport,
    }

In `limits.rs`, expose validated construction rather than public mutable fields.

    pub struct ExtensionLimits;

    impl ExtensionLimits {
        pub fn new(values: ExtensionLimitValues)
            -> Result<Self, InvalidExtensionLimits>;
        pub fn values(&self) -> ExtensionLimitValues;
    }

    #[derive(Clone, Default)]
    pub struct ExtensionCancellation;

    impl ExtensionCancellation {
        pub fn new() -> Self;
        pub fn cancel(&self);
        pub fn is_cancelled(&self) -> bool;
    }

In `outcome.rs`, keep analytical incompleteness in the domain result.

    pub struct ExtensionOutcome<T> {
        pub completion: ExtensionCompletion,
        pub value: Option<T>,
        pub metadata: ExtensionResultMetadata,
    }

    pub enum ExtensionCompletion {
        Complete,
        Ambiguous,
        Unknown,
        Unsupported { capability: ExtensionCapabilityId },
        Unproven,
        Truncated { limit: ExtensionLimitKind },
        ExceededBudget { dimension: ExtensionWorkDimension },
        Cancelled,
    }

    pub struct ExtensionResultMetadata {
        pub api: ExtensionApiVersion,
        pub operation: ExtensionCapabilityId,
        pub stability: ApiStability,
        pub generation: WorkspaceGeneration,
        pub diagnostics: Box<[ExtensionDiagnostic]>,
        pub work: ExtensionWork,
        pub limits: ExtensionLimitValues,
        pub provenance: ExtensionProvenance,
    }

In `workspace.rs`, own the analyzer privately. Do not implement `Deref`, `AsRef<WorkspaceAnalyzer>`, or an analyzer accessor.

    pub struct ExtensionWorkspace {
        generation: WorkspaceGeneration,
        capabilities: ExtensionCapabilityReport,
        analyzer: WorkspaceAnalyzer,
    }

    impl ExtensionWorkspace {
        pub fn open(options: ExtensionWorkspaceOptions)
            -> Result<Self, ExtensionWorkspaceError>;
        pub fn generation(&self) -> &WorkspaceGeneration;
        pub fn capabilities(&self) -> &ExtensionCapabilityReport;
        pub fn describe(&self) -> ExtensionWorkspaceDescription;
        pub fn execute(
            &self,
            request: ExtensionRequest,
            cancellation: &ExtensionCancellation,
        ) -> Result<ExtensionResponse, ExtensionError>;
    }

In `structural.rs`, use an extension-owned result even when the input embeds the existing typed query.

    pub struct StructuralRequest {
        pub compatibility: ExtensionCompatibility,
        pub expected_generation: WorkspaceGeneration,
        pub query: CodeQuery,
        pub limits: ExtensionLimits,
    }

    pub struct StructuralResult {
        pub items: Box<[StructuralResultItem]>,
    }

In `relations.rs`, define the minimal shared semantic model. #2101 extends these same types rather than replacing them.

    pub enum SemanticRelationKind { ControlFlow }

    pub struct SemanticRelationRequest {
        pub compatibility: ExtensionCompatibility,
        pub expected_generation: WorkspaceGeneration,
        pub seed: SourceSpan,
        pub limits: ExtensionLimits,
    }

    pub struct SemanticRelationSnapshot {
        pub nodes: Box<[SemanticNodeOccurrence]>,
        pub edges: Box<[SemanticRelationEdge]>,
        pub boundaries: Box<[SemanticRelationBoundary]>,
    }

In `codec.rs`, expose value encoding without transport framing.

    pub enum ExtensionRequest {
        Structural(StructuralRequest),
        SemanticRelations(SemanticRelationRequest),
    }

    pub enum ExtensionResponse {
        Structural(ExtensionOutcome<StructuralResult>),
        SemanticRelations(ExtensionOutcome<SemanticRelationSnapshot>),
    }

    pub fn decode_request_json(bytes: &[u8])
        -> Result<ExtensionRequest, ExtensionDecodeError>;
    pub fn encode_request_json(request: &ExtensionRequest)
        -> Result<Vec<u8>, ExtensionEncodeError>;
    pub fn encode_response_json(response: &ExtensionResponse)
        -> Result<Vec<u8>, ExtensionEncodeError>;

`brokk-bifrost-runtime` continues to depend on `brokk-bifrost-analysis` and `brokk-bifrost-policy`; no new workspace crate is added. The runtime must remain forbidden from depending on MCP or LSP. The root facade re-exports `extension` but is not the acceptance consumer. External templates, observation formats, full semantic relation snapshots, control dependence, value dependence, run manifests, research consumers, version changes, publication, and license migration belong to later work.

Plan revision note (2026-08-13): Created the initial issue-specific plan after auditing the live #2100/#2101 requirements and current package, runtime, workspace, semantic identity, capability, outcome, and package-check seams. The plan selects the existing runtime crate, makes stable versus experimental ownership explicit, requires generation identity from captured analyzer state, and confines #2100 semantic implementation to a bounded experimental procedure-control-flow seam so #2101 retains ownership of the full relation schema.
