# Index C++ declarations from explicit include roots

This ExecPlan is a living document. Maintain it under `.agents/PLANS.md`.
Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` current while work continues.

## Purpose / Big Picture

C++ policy runs cannot currently finish when a call enters a standard or
third-party header. Bifrost reports every such route as `external_unknown`.
That result makes call dispatch unknown. Exhaustive policy evaluation then
reports `PartialDiscovery` and exits as unreliable.

After this work, Bifrost can read external headers from explicit include roots
in `compile_commands.json`. It can publish their declarations as a semantic
pack. A call such as `std::vector::push_back` can then resolve as
`external_indexed`. If discovery proves the external route but no pack is
active, Bifrost reports `external_declared_unindexed`. Missing or conflicting
compile evidence remains `external_unknown`.

The final proof uses a local fake system root. The test does not read host
headers. With the pack active, an exhaustive policy finishes without
`PartialDiscovery`. Without the pack, the same call remains honestly
inconclusive.

## Progress

- [x] (2026-08-11 05:52Z) Diagnosed the C++ external-boundary path and selected one staged pack design.
- [x] (2026-08-11 05:52Z) Created this ExecPlan.
- [x] (2026-08-11 05:56Z) Implemented compile-context external-root discovery and configuration agreement.
- [x] (2026-08-11 06:00Z) Implemented one structured C++ external-header declaration extractor.
- [x] (2026-08-11 06:24Z) Added bounded C++ header discovery, exact source-set production, and host activation.
- [x] (2026-08-11 07:18Z) Bound direct external member routes to exact owner and member facts.
- [x] (2026-08-11 07:18Z) Added fake-system-root indexed and declared-unindexed boundary tests.
- [x] (2026-08-11 08:42Z) Passed focused tests, all workspace-target Clippy, and the complete featureless analysis library test.
- [x] (2026-08-11 08:42Z) Completed specialist review and corrected all accepted findings.

## Surprises & Discoveries

- Observation: C++ already records explicit include roots for each source file.
  Evidence: `CppCompileContext` in
  `crates/bifrost-cpp/src/compile_context.rs` records project roots, system
  roots, forced includes, and macros.

- Observation: C++ diagnostics already use `external_declared_unindexed` for
  an unresolved angle include.
  Evidence: `prove_include_closure` in
  `crates/bifrost-cpp/src/diagnostics.rs` emits that boundary, but definition
  tracing does not read the result.

- Observation: The minimal boundary status cannot satisfy the policy
  acceptance test.
  Evidence: `dispatch.rs` maps `external_declared_unindexed` to `Unproven`.
  Only `external_indexed` can make this external dispatch complete.

- Observation: Existing compile contexts lost the compiler's include-search
  order by storing `-I`, `-iquote`, and `-isystem` in two separate vectors.
  Evidence: The new private ordered root list preserves the old public fields
  while resolving the first matching external header correctly.

- Observation: A C++ `CodeUnit::short_name()` can include its enclosing type
  chain, such as `vector.push_back`.
  Evidence: The shared `CodeUnit::terminal_name()` accessor now exposes the
  final structured segment without parsing the rendered name.

- Observation: A dependency-root limit could silently remove C++ include roots.
  Evidence: Discovery now reports `limit.dependencies`, marks the result
  incomplete, and records the suppressed root count.

- Observation: Generated overlay symbols use virtual model locations.
  Evidence: The overlay now retains the compiled locator path as private query
  data, so C++ can bind a fact to the exact included header without changing
  the public location contract.

- Observation: A relative header locator cannot identify an include root.
  Evidence: Two roots can contain the same path. Resolution now compares both
  the root-derived package identity and the exact root-relative path.

- Observation: Scanning a complete system include tree is not bounded by the
  number of useful headers.
  Evidence: Discovery now follows only the direct and transitive include
  closure from workspace C++ sources. It applies file, depth, byte, record,
  and cancellation limits to that closure.

## Decision Log

- Decision: Implement both boundary strengths through one pack pipeline.
  Rationale: Retained discovery supplies `external_declared_unindexed`.
  Activated facts upgrade the same route to `external_indexed`. A separate
  declaration census would scan headers twice and could disagree with packs.
  Date/Author: 2026-08-11 / Codex.

- Decision: Index only explicit include roots in the first implementation.
  Rationale: Implicit root discovery differs across Clang, GCC, and MSVC.
  Guessing paths is unsafe. Running a compiler inside diagnostics, policy
  evaluation, or tool requests is also prohibited. A later host-owned probe
  can add implicit roots with its own contract.
  Date/Author: 2026-08-11 / Codex.

- Decision: Require all applicable compile configurations to agree.
  Rationale: One source can have several compile commands. Different include
  targets cannot support one exact external declaration claim.
  Date/Author: 2026-08-11 / Codex.

- Decision: Keep language parsing in `brokk-bifrost-cpp` and semantic-pack
  integration in `brokk-bifrost-analysis`.
  Rationale: The C++ crate must not depend on the large analysis crate.
  Analyzer, store, and semantic-model types belong in analysis.
  Date/Author: 2026-08-11 / Codex.

- Decision: Resolve external members only after structured receiver typing.
  Rationale: A bare `push_back` match can select an unrelated owner. The C++
  resolver now derives `std::vector` from the parameter type AST, then queries
  members of that exact overlay owner.
  Date/Author: 2026-08-11 / Codex.

- Decision: Treat a miss in a complete active pack as proved absence.
  Rationale: A miss in a partial pack is inconclusive. A complete pack has
  enough data to reject a miss without a `PartialDiscovery` result.
  Date/Author: 2026-08-11 / Codex.

## Outcomes & Retrospective

Compile-context discovery, reachable-header extraction, semantic-pack
production, and direct external member resolution are complete. Review fixes
preserve root identity, prevent root escape, apply one pack-wide record budget,
keep nested access visibility, support transitive headers, and distinguish a
complete-pack miss from incomplete evidence.

Validation passed with `cargo test -p brokk-bifrost-cpp`, the focused external
analysis tests, `cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test -p brokk-bifrost-analysis --lib`. The last command passed 1,722
tests and ignored 7 tests.

## Context and Orientation

A compilation database is the `compile_commands.json` file at a workspace
root. Each entry describes one compiler command for one source file.
`crates/bifrost-cpp/src/compile_context.rs` reads supported arguments without
executing the command. It retains every distinct configuration for a source.

An include root is a directory searched for a header. An external root is an
explicit system root, or an explicit `-I` root outside the workspace. The
workspace-only include index in `crates/bifrost-cpp/src/imports.rs` must remain
separate. External headers do not become workspace `ProjectFile` values or
workspace `CodeUnit` values.

A semantic pack is a deterministic set of external facts. Declaration packs
hold types, members, ownership, and hierarchy. The producer interface is in
`crates/bifrost-analysis/src/analyzer/semantic_model/producer.rs`. Host-owned
activation is in `crates/bifrost-analysis/src/analyzer/workspace.rs`.
Diagnostics and query requests only read already-published state.

The resolution trace in
`crates/bifrost-analysis/src/analyzer/usages/get_definition/trace.rs` refines
an unresolved external route. Its C++ arm currently always returns
`ExternalUnknown`. The workspace dispatch oracle in
`crates/bifrost-analysis/src/analyzer/semantic/workspace_oracle/dispatch.rs`
maps an indexed route to complete dispatch.

Boundary rules after this change are exact:

* `external_indexed` means all applicable configurations reach one header,
  and the active pack contains the exact type or member.
* `external_declared_unindexed` means all applicable configurations prove an
  external include route, but no active pack contains the exact target.
* `external_unknown` means compile evidence is absent, incomplete, or
  conflicting.

Pack completeness is separate from symbol presence. A partial pack can prove
a fact that it contains. A miss in a partial pack cannot prove absence.
Preprocessor conditions, parse failures, and limits must set partial
completeness and produce typed diagnostics.

## Plan of Work

The first milestone extends `CppCompileContext` in
`crates/bifrost-cpp/src/compile_context.rs`. It adds structured external roots
and an agreement result for include resolution across every configuration.
Relative paths remain relative to each compile-command directory until they
are made absolute. Containment checks use normalized or canonical paths.
Diagnostics retain useful original paths. The parser supports existing `-I`,
`-iquote`, and `-isystem` forms. It also records explicit sysroot evidence
only when the command gives a final searchable include root. It does not guess
directories below a sysroot.

The second milestone adds
`crates/bifrost-cpp/src/external_declarations.rs`. This is the only new scanner
for external headers. It uses tree-sitter fields and an explicit traversal
stack. It emits neutral C++ records for namespaces, class-like types, direct
bases, fields, constructors, methods, signatures, and source-relative header
paths. It does not use regular expressions, string splitting, or delimiter
walks to replace AST structure. It reports partial extraction for parse errors,
unsupported preprocessing, cancellation, or limits.

The third milestone adds `CppHeaderSourceSet` to `ExternalArtifactKind` in
`crates/bifrost-analysis/src/analyzer/semantic_model/producer.rs`. All
exhaustive matches gain the new variant. The producer reuses the existing
bounded source-set reader. It converts neutral C++ records into `TypeFact`,
`MemberFact`, and hierarchy or receiver facts. It uses the shared declaration
identity helpers. It applies stable hashing, path containment, byte limits,
record limits, diagnostic limits, and cancellation.

The same milestone adds one compact C++ external module under
`crates/bifrost-analysis/src/analyzer/cpp/`. Split it only if discovery,
production, and overlay identity are each independently large. Discovery reads
the compile contexts and creates one deterministic dependency per agreed
external root and configuration identity. It never starts a compiler or build
tool. Preparation retains discovery evidence even when no pack becomes active.

The fourth milestone adds `Cpp` to `DependencyPackEcosystem` in
`crates/bifrost-analysis/src/analyzer/workspace.rs`. Its stable label is `cpp`.
Its language is `Language::Cpp`. A change to `compile_commands.json`
invalidates its published state. Host activation routes through the C++
discovery resolver and adapter.

The fifth milestone binds C++ overlay facts into external resolution. Add one
C++ identity helper that resolves a written type through namespaces, aliases,
and template structure. For member calls, first resolve the structured
receiver type. Then query members for that exact owner. Never select a member
by bare name. Require a direct or proven transitive include route from the
source file to the fact's header. Do not convert external facts into fake
workspace code units.

Update the C++ arm in `boundary_evidence`. An exact overlay target returns
`ExternalIndexed` and its stable external ID. Retained discovery without an
exact active fact returns `ExternalDeclaredUnindexed`. Missing or conflicting
evidence returns `ExternalUnknown`. Reuse this classification in semantic
diagnostics so diagnostics and policy resolution cannot disagree.

The last implementation milestone adds behavior tests. Unit tests cover root
parsing, external `-I`, explicit system roots, forced includes, duplicates,
agreement, and disagreement. Extractor tests cover namespaces, templates,
overloads, fields, constructors, bases, similar names, parse errors,
preprocessor limits, and cancellation. Producer tests cover stable facts,
owner identities, partial output, containment, and cancellation.

Add `tests/suite_semantic/cpp_standard_library_pack.rs` and register it in
`tests/suite_semantic/main.rs`. Use `InlineTestProject`. The fixture contains
`src/main.cpp`, `fake-sysroot/include/vector`, and `compile_commands.json`.
The fake header declares `std::vector<T>` and `push_back`. The activated case
must resolve the external member and complete without `PartialDiscovery`. The
no-pack case must report `ExternalDeclaredUnindexed`. The no-context case must
report `ExternalUnknown`.

## Concrete Steps

Run all commands from the repository root:
`/Users/dave/.codex/worktrees/bb59/bifrost`.

After each milestone, run formatting and its focused tests. Commit only files
from that milestone. Use a multiline commit body that states the behavior,
reason, and validation evidence.

For compile-context work, run:

    cargo fmt --check
    cargo test -p brokk-bifrost-cpp compile_context

For external declaration extraction, run:

    cargo fmt --check
    cargo test -p brokk-bifrost-cpp external_declarations

For pack production and activation, run the applicable tests after their
exact names exist:

    cargo test --test suite_semantic cpp_standard_library_pack
    cargo test --test suite_semantic boundary_evidence

After all focused tests pass, run:

    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings

Do not enable `nlp`. This work does not affect semantic search.

## Validation and Acceptance

The new compile-context tests must fail before the milestone and pass after
it. They must show exact agreement behavior across multiple configurations.

The extractor tests must compare structured declarations. They must include
realistic near misses. A type with the same short name in another namespace
must not satisfy an exact owner lookup. A conditional declaration must make
the surface partial unless its guard is expressed.

The end-to-end fixture is the final acceptance proof. With C++ pack activation,
the `<vector>` call must name the stable external member ID for
`std::vector::push_back`. The exhaustive policy report must not contain
`PartialDiscovery`. With discovery but no active pack, the same route must be
`ExternalDeclaredUnindexed` and the policy must remain inconclusive. With no
compile evidence, it must be `ExternalUnknown`.

All behavior must work on Windows and Unix-like systems. Tests use only paths
inside their temporary project. They do not assume `/usr/include`, an Apple
SDK, Visual Studio, or a compiler installation.

## Idempotence and Recovery

All discovery and production reads are deterministic and bounded. Repeating
activation for unchanged inputs produces the same artifact key and facts.
No step edits external headers or executes a build tool.

If an implementation milestone fails, keep its focused failing test and
record the failure in `Surprises & Discoveries`. Correct the shared pipeline
instead of adding a language-specific text fallback. A partial pack is valid
only when it carries a typed reason. Do not publish incomplete output as
complete.

If the worktree contains unrelated user changes, do not stage them. Stage
milestone files by explicit path. Never use `git add -A`.

## Artifacts and Notes

Issue 1872 is a child of issue 1877. The parent describes the mechanical
failure chain from `external_unknown` to unreliable policy status. Recent
precedents are JVM pack binding from issue 1893 and JVM member facts from
issue 1900. Go source-set production is the closest discovery and artifact
model.

## Interfaces and Dependencies

In `crates/bifrost-cpp/src/compile_context.rs`, expose a typed result for one
include across all applicable configurations. It must distinguish agreement,
disagreement, and missing evidence. Do not encode this result as a Boolean.

In `crates/bifrost-cpp/src/external_declarations.rs`, expose neutral records
and one bounded extraction entry. These types can depend on
`brokk-bifrost-core`, tree-sitter, paths, and ordinary Rust collections. They
must not depend on `brokk-bifrost-analysis`.

In `crates/bifrost-analysis/src/analyzer/cpp/`, implement the semantic-model
adapter and overlay lookup. It maps neutral C++ records to existing semantic
facts. It owns no second parser.

In `crates/bifrost-analysis/src/analyzer/workspace.rs`, add the `Cpp` ecosystem
to every exhaustive match. Preserve the host-owned activation rule.

In C++ definition resolution, return external target IDs through the existing
boundary trace. Do not manufacture workspace declarations.

Plan revision note, 2026-08-11: Created the initial plan after repository and
issue diagnosis. The plan selects one shared discovery and extraction path for
both staged boundary results.

Plan revision note, 2026-08-11: Completed the compile-context milestone. The
implementation now distinguishes missing, undeclared, conflicting, and agreed
external angle includes across every configuration. Nine focused tests pass.

Plan revision note, 2026-08-11: Completed the external declaration extractor.
It reuses the production C++ declaration walk, preserves exact owner identity,
and reports preprocessor, parse, and record-limit partiality. Three focused
tests pass.
