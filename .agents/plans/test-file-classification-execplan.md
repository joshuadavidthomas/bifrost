# Add file-level test classification (`classify_test_files`) and fix `contains_tests` false positives

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `.agents/PLANS.md` at the repository root.

Tracking issue: https://github.com/BrokkAi/bifrost/issues/604

## Purpose / Big Picture

Bifrost exposes a `contains_tests` API (Rust function, MCP tool, and Python client method) that answers "does this file contain test code?" using per-language content detection. Issue 604 documents two real failures for consumers that need to identify the *complete test surface* of a repository (for example, a hermetic grader that resets all test files to a reference state before evaluating a candidate change):

1. False negatives: test fixtures and helpers that live under conventional test roots but contain no runnable test method return `false`. Confirmed examples: `test/Core.Test/Auth/AutoFixture/RegisterFinishRequestModelFixtures.cs` in `bitwarden/server` (an AutoFixture customization class with no `[Fact]`/`[Test]` attribute) and helpers like `app/src/test/java/ai/brokk/testutil/TestService.java` in `BrokkAi/brokk`.
2. False positives: ordinary production files return `true`. Confirmed mechanism: the Java detector is a raw substring scan, so `"@Test"` matches the JetBrains `@TestOnly` annotation and `".Test"` matches imports such as `org.jetbrains.annotations.TestOnly` — both present in `app/src/main/java/ai/brokk/project/MainProject.java` and `app/src/main/java/ai/brokk/agents/ReviewAgent.java` of `BrokkAi/brokk`.

After this change, three things are true:

- A new public API `classify_test_files` (Rust function in `src/searchtools.rs`, MCP tool in the `cli` toolset, Python client method) classifies each file as `test`, `test_support`, `production`, or `ambiguous` using path conventions plus semantic detection, suitable for identifying an authoritative test surface.
- `contains_tests` remains the semantic "does this file contain runnable test code" predicate, but its detectors no longer fire on substrings inside comments, string literals, imports, or unrelated identifiers (no more `@TestOnly` false positives), and its documentation no longer claims fitness for hermetic test-surface identification.
- The three near-duplicate private path heuristics currently scattered across the codebase are replaced by one shared implementation, so every internal consumer (symbol search ranking, commit analysis, secret detection suppression) agrees on what a test path is.

## Progress

- [x] (2026-07-10 00:00Z) Milestone 1: shared path-convention module `src/analyzer/test_paths.rs`, three duplicate heuristics replaced.
- [x] (2026-07-10 00:00Z) Milestone 2: semantic detectors tightened from substring scans to AST-based checks (Java, C#, Rust, Scala, PHP).
- [x] (2026-07-10 00:00Z) Milestone 3: `classify_test_files` public API + MCP tool + Python client + docs updates.
- [x] (2026-07-10 00:00Z) Milestone 4: internal consumers migrated to the unified classification where file-level semantics are wanted.
- [x] (2026-07-10 00:00Z) Milestone 5: regression tests mirroring issue 604's scenarios; full check suite green.

## Surprises & Discoveries

- Observation: the repo already knew `contains_tests` alone misses test-scoped files. `.agents/plans/search-symbols-ranking-execplan.md` records the decision to "supplement `analyzer.contains_tests(file)` with a path-based test heuristic inside `search_symbols`" — but the fix was scoped to symbol search only and never exposed publicly.
- Observation: the false positives in issue 604 come from `@TestOnly` (a production-code annotation from `org.jetbrains.annotations`) matching the `"@Test"` substring, and from imports like `ai.brokk.analyzer.TestFileHeuristics` matching `".Test"`. Evidence: `MainProject.java:66,246,251` and `ReviewAgent.java:17,66,131` on brokk master.
- Observation: `tests/csharp_test_detection_test.rs` already asserts that `Tests/IgnoreTests.cs` (a file under a `Tests/` folder with no runnable test) returns `false` from `contains_tests` — the false-negative behavior is locked in as *intended* for the semantic predicate. This is why the fix is a new classification API, not a change to `contains_tests` semantics.
- Observation: the storage caveat is real. `src/analyzer/persistence/reconcile.rs` hydrates unchanged files when the stored per-language epoch matches and `(mtime_ns, size)` still matches; `FileState.contains_tests` is serialized in `src/analyzer/persistence/payload.rs`, and detector source code is not otherwise an epoch input. Bumped the manual per-language epoch salts for Java, Rust, PHP, Scala, and C# with `ast-test-detection-2026-07` so stale persisted `contains_tests` bits are re-analyzed.
- Observation: the three private path heuristics had meaningful drift. `src/code_quality/secret_like_code.rs` knew about `src/test/`, but symbol search and commit analysis did not; the new shared `src/analyzer/test_paths.rs` now recognizes `testdata`, JVM `src/test`, C# `.Tests`-style project directories, and language filename conventions consistently.
- Observation: lowercasing all basenames before filename-convention matching made PascalCase suffix conventions too loose: `Audit.java` matched `IT.java`, `Latest.java` matched `Test.java`, and `Contest.cs` matched `Test.cs`. The suffix conventions used as PascalCase in the wild now match against the original basename case-sensitively.
- Observation: migrating `scan_usages` exclusion from semantic `contains_tests` to file classification did not require changing commit analysis beyond the Milestone 1 shared path module. Commit analysis still uses its existing `contains_tests || is_test_like_path` shape, which is sufficient for its current tests and keeps this milestone focused on `scan_usages`/`usage_graph` filtering.
- Observation: one existing usage-graph test placed a PHP consumer under `tests/` while asserting Composer PSR-4 type-reference behavior. After classification-based exclusion, that fixture is correctly treated as `test_support` and is omitted by default; the test now passes `include_tests=true` because the behavior under test is not production-only filtering.

## Decision Log

- Decision: keep `contains_tests` as a purely semantic, path-independent predicate; add a separate `classify_test_files` API for file-level classification.
  Rationale: existing tests (e.g. `tests/cpp_test_assertion_smells.rs:24-52`) deliberately lock in path independence; internal consumers like assertion-smell gating genuinely want the semantic bit; issue 604 itself asks for the two decisions to be separable.
  Date/Author: 2026-07-10 / Claude + Jonathan.
- Decision: classification `kind` is determined by path conventions (directory roots and per-language filename conventions) only; the semantic bit is reported alongside as a separate field, and distinguishes `test` from `test_support` *within* path-classified test files. Files at ambiguous paths (neither a recognized test root/filename nor a recognized production root) classify as `ambiguous`, a distinct fourth kind — they are NOT silently folded into `production`.
  Rationale: letting the semantic bit promote an ambiguous-path file to `test` would misclassify Rust files with inline `#[cfg(test)]` modules (a production-file feature) and reintroduce the false-positive class the issue complains about. Folding ambiguity into `production` was considered and rejected (Jonathan, 2026-07-10): it would present a guess as a definitive answer. Surfacing `ambiguous` honestly tells callers the path conventions were inconclusive; they see both `kind` and `contains_test_code` and decide per use case (a hermetic grader might treat `ambiguous` + `contains_test_code=true` differently from a ranking heuristic). Per-language filename conventions (`*_test.go`, `test_*.py`, `*.test.ts`, `FooTest.java`, `FooSpec.scala`) already catch test files in flat layouts, so `ambiguous` should be rare in conventionally laid-out repos.
  Date/Author: 2026-07-10 / Jonathan + Claude (revised same day from an earlier default-to-`production` rule).
- Decision: tighten the substring-based semantic detectors (Java, Rust, Scala, PHP; verify C# attribute matching against AST) rather than only documenting their looseness.
  Rationale: repository design philosophy explicitly bans string scanning where tree-sitter structure is available ("Do not replace parser support with small source-text mini parsers"), and the `LanguageAdapter::contains_tests` hook already receives the parsed tree. Ruby (line-oriented DSL detection) and C++ (test macros, which are token-level constructs that regexes match acceptably) are left as-is; record follow-ups if they misbehave in practice.
  Date/Author: 2026-07-10 / Claude.
- Decision: keep Java setup/teardown annotations (`BeforeEach`, `AfterEach`, `BeforeAll`, `AfterAll`) in the semantic detector's exact annotation set.
  Rationale: a file containing only setup/teardown helpers is still test code even when it has no directly runnable test method.
  Date/Author: 2026-07-10 / Codex.
- Decision: ScalaTest FlatSpec-style `should ... in {}` detection remains a structured AST-node-text check on `infix_expression` nodes.
  Rationale: the tree-sitter Scala grammar exposes this shape as nested infix expressions, and matching the operator chain precisely enough across ScalaTest variants would require more grammar-specific resolver work than Milestone 2 warrants. The retained check no longer scans the whole source; it only examines relevant infix-expression node text.
  Date/Author: 2026-07-10 / Codex.
- Decision: filename conventions that are PascalCase in normal use are matched case-sensitively against the original filename: Java `Test.java`/`Tests.java`/`TestCase.java`/`IT.java`, Scala `Test.scala`/`Spec.scala`/`Suite.scala`, C# `Test.cs`/`Tests.cs`, and PHP `Test.php`. Snake-case and separator-based conventions remain case-insensitive.
  Rationale: lowercasing PascalCase suffixes removes the word boundary those conventions rely on and creates production false positives such as `Audit.java`, `Latest.java`, and `Contest.cs`.
  Date/Author: 2026-07-10 / Codex.
- Decision: `Language::None` uses the same conservative split: separator/exact helper conventions stay case-insensitive, while PascalCase suffix conventions are checked case-sensitively against the original filename.
  Rationale: unknown-language fallback should remain useful for inferred or mixed workspaces without recreating the loose substring behavior that produced the Milestone 1 false positives.
  Date/Author: 2026-07-10 / Codex.

## Outcomes & Retrospective

Milestones 1 through 5 are implemented. The repository now has one shared path-convention implementation, tightened Java/C#/Rust/Scala/PHP semantic detectors, a public `classify_test_files` Rust/MCP/Python surface, and `scan_usages`/`usage_graph` exclusion based on positive `test` or `test_support` classification rather than semantic test-code detection alone. Regression coverage now exercises C# and Java test-support files, Java `@TestOnly` production code, Rust inline `#[cfg(test)]` ambiguity, Go filename-convention tests, MCP end-to-end serialization, and the `scan_usages` helper-under-test-root exclusion case. All required quality gates passed locally on 2026-07-10. One existing PHP usage-graph test needed to opt into `include_tests=true` because its consumer fixture lives under `tests/`.

## Context and Orientation

Bifrost is a Rust code-analysis server. Per-language analyzers plug into a shared tree-sitter engine:

- `src/analyzer/i_analyzer.rs` defines the `IAnalyzer` trait; `contains_tests(&self, file: &ProjectFile) -> bool` has a default `false` at lines 279-281.
- `src/analyzer/tree_sitter_analyzer.rs` defines `TreeSitterAnalyzer<A: LanguageAdapter>`. The `LanguageAdapter` trait has a `contains_tests(file, source, tree, parsed)` hook (around lines 108-116) — note it already receives the parsed tree-sitter `Tree` and the parsed declarations, so AST-based detection needs no new plumbing. The result is computed once per file during `analyze_file` (line ~769) and cached in `FileState.contains_tests` (line ~243). `StorageLanguageAdapter::storage_contains_tests`/`hydrate_contains_tests` (lines ~170-176) round-trip the cached bit through persistent storage.
- Each language has an adapter wiring a detection function: Java `src/analyzer/java/tests.rs:122-133` (substring scan: `"@Test"`, `".Test"`, `"@Rule"`, etc.), C# `src/analyzer/csharp/tests.rs:238-247` (regex on `[Test]`/`[Fact]`/`[Theory]` attributes), Python `src/analyzer/python/tests.rs:418-422`, Go `src/analyzer/go/tests.rs:326-355` (already AST-based), Rust `src/analyzer/rust/tests.rs:5-7` (substring `"#[test]"`/`"#[cfg(test)]"`), Scala `src/analyzer/scala/tests.rs:403-409` (substrings), Ruby `src/analyzer/ruby/tests.rs:6-25` (line prefixes), PHP `src/analyzer/php/tests.rs:216-235` (class identifier *contains* "test" — matches "Contest"), C++ `src/analyzer/cpp/tests.rs:189-198` (macro regexes), JS/TS `src/analyzer/js_ts/mod.rs:24-46` (filename `.test.`/`.spec.` OR source substrings).
- `MultiAnalyzer` (`src/analyzer/multi_analyzer.rs`) routes per-file to the right language delegate; its `contains_tests` is at lines 654-658.
- Public surface: `contains_tests()` function in `src/searchtools.rs:5696-5735` (its doc comment at line ~5713 claims fitness for "hermetic acceptance that resets test files" — the claim issue 604 refutes); MCP descriptor in `src/mcp_cli.rs:4-19`; dispatch in `src/searchtools_service.rs:410-412`; Python client `bifrost_searchtools/client.py:359-370`; docs `docs/src/content/docs/mcp.md` and `docs/src/content/docs/python-client.md`.
- Three near-duplicate private path heuristics exist: `is_test_like_path` in `src/searchtools.rs:5741-5760` (used by `is_test_candidate` at 5737, which ORs it with `contains_tests` for symbol-search filtering/ranking), `is_test_like_path` in `src/commit_analysis.rs:902-916`, and `looks_like_test_path` in `src/code_quality/secret_like_code.rs:776-794` (the only one that knows `src/test/`).

"Test root" in this plan means a directory convention that marks everything beneath it as part of the test surface: path segments `test`, `tests`, `__tests__`, `spec`, `specs`, `testdata`, JVM `src/test/...`, C# test-project directories whose name ends in `.Test`, `.Tests`, `.UnitTests`, `.IntegrationTests`. "Production root" means the converse marker, currently just JVM `src/main/...`. "Test filename convention" means per-language basename patterns: Go `*_test.go`; Python `test_*.py`, `*_test.py`, `conftest.py`; JS/TS `*.test.*`, `*.spec.*`; JVM `*Test.java`, `*Tests.java`, `*TestCase.java`, `*IT.java`, `*Test.scala`, `*Spec.scala`, `*Suite.scala`; Ruby `*_spec.rb`, `*_test.rb`, `spec_helper.rb`, `test_helper.rb`; PHP `*Test.php`; Rust none beyond directory conventions (integration tests live under `tests/`); C# `*Test.cs`, `*Tests.cs`; C++ `*_test.(cc|cpp)`, `*_unittest.(cc|cpp)`, `test_*.(cc|cpp)`.

## Plan of Work

### Milestone 1 — shared path-convention module

Create `src/analyzer/test_paths.rs` (registered in `src/analyzer/mod.rs`) exposing:

    pub enum PathTestVerdict { TestRoot, ProductionRoot, Ambiguous }

    /// Directory-convention verdict for a workspace-relative path.
    pub fn path_test_verdict(path: &str) -> PathTestVerdict

    /// Per-language (or language-agnostic when `language` is None/unknown)
    /// filename-convention check on the basename.
    pub fn has_test_filename_convention(path: &str, language: Language) -> bool

    /// The union used for "is this file test-like at all": TestRoot verdict
    /// OR filename convention. This is the replacement for the three
    /// duplicated private heuristics.
    pub fn is_test_like_path(path: &str, language: Language) -> bool

Implementation notes: operate on workspace-relative paths, normalize `\\` to `/` first (Windows), compare segments case-insensitively where conventions are case-insensitive in the wild (C# folder names), case-sensitively for JVM `src/test`. Use `Path`/component iteration or plain segment splitting of the already-normalized relative string — this is genuinely path (not source-code) parsing, so string segmentation is appropriate here and does not violate the no-mini-parser rule. Cover the segment set and filename conventions listed in Context and Orientation. `ProductionRoot` fires for paths containing the `src/main/` segment pair (JVM convention); everything else non-test is `Ambiguous`.

Replace the three duplicates: delete `is_test_like_path` from `src/searchtools.rs` and `src/commit_analysis.rs` and `looks_like_test_path` from `src/code_quality/secret_like_code.rs`; route all their callers through the new module (`is_test_candidate` in searchtools keeps its current OR-with-`contains_tests` behavior; commit_analysis and secret_like_code call `is_test_like_path`). Behavior deltas are expected to be small and in the direction of "more paths recognized as tests" (e.g. `testdata`, `src/test` now recognized everywhere); note any test churn in Surprises & Discoveries.

Unit tests live beside the module (`#[cfg(test)] mod tests` in `test_paths.rs`) covering: `test/Core.Test/Auth/AutoFixture/Fixture.cs` → TestRoot; `app/src/test/java/ai/brokk/testutil/TestService.java` → TestRoot; `app/src/main/java/ai/brokk/project/MainProject.java` → ProductionRoot; `src/lib.rs` → Ambiguous; `pkg/foo_test.go` → filename convention true; Windows-style input `app\\src\\test\\Foo.java` → TestRoot.

### Milestone 2 — tighten semantic detectors to AST

For each language below, rewrite the detection function to use the tree-sitter `Tree` (already passed into `LanguageAdapter::contains_tests`) instead of raw source substrings. Keep a cheap substring pre-filter *only* as a short-circuit optimization when profitable (if the source lacks the substring, the AST cannot contain the construct), never as the decision itself. Use iterative traversal (explicit stack) per repo convention, or reuse an existing shared tree-walk helper if one exists in `tree_sitter_analyzer.rs`/`common.rs`.

- Java (`src/analyzer/java/tests.rs`): a file contains tests iff the AST has (a) an annotation node (`marker_annotation`/`annotation`) whose name resolves to exactly one of `Test`, `ParameterizedTest`, `RepeatedTest`, `TestFactory`, `TestTemplate`, `Rule`, `ClassRule`, `Ignore`, `Disabled`, `Nested`, `BeforeEach`, `AfterEach`, `BeforeAll`, `AfterAll` (simple name or `org.junit...`-qualified name whose final segment matches), or (b) a class whose `superclass` clause names `TestCase` or `junit.framework.TestCase`. Exact-name matching kills `@TestOnly`. Decide whether the setup/teardown annotations (`BeforeEach` etc.) belong in the set: include them, because a file containing only `@BeforeEach` helpers is test *code* even if not directly runnable — record the choice in the Decision Log if changed.
- C# (`src/analyzer/csharp/tests.rs`): replace the regex with AST `attribute` nodes whose name (final identifier segment, `Attribute` suffix stripped) is exactly `Test`, `Fact`, `Theory`, `TestMethod`, `TestCase`. Preserve the existing positive/negative test matrix in `tests/csharp_test_detection_test.rs` (qualified forms, `Attribute` suffix, `[NotATest]` negative).
- Rust (`src/analyzer/rust/tests.rs`): match `attribute_item` nodes for `#[test]`, `#[cfg(test)]`, `#[tokio::test]`, `#[rstest]` (final path segment `test` or a `cfg` call with a `test` argument) instead of `source.contains(...)`. This stops firing on doc comments and string literals — bifrost's own `src/analyzer/rust/tests.rs` currently classifies itself as containing tests because its string literals mention `#[cfg(test)]`.
- Scala (`src/analyzer/scala/tests.rs`): match annotation nodes for `Test`/`org.junit.Test`, and call-expression nodes `test("...")`/`it("...")`/`property("...")` plus the ScalaTest infix `should ... in { }` shape at AST level, best effort. Where the tree-sitter Scala grammar makes an exact shape impractical, keep the check structured on AST node text of the *function being called*, not on whole-source substrings.
- PHP (`src/analyzer/php/tests.rs`): change class-name matching from "contains test (case-insensitive)" to structured rules: class extends a `TestCase`-named base, or class name ends with `Test`/`TestCase`, or a method name starts with `test` on such a class, or the existing `@test` docblock rule. "Contest"/"Latest" classes must no longer match.

Go and Python are already tight (AST / anchored regex on definitions); JS/TS source detection (`describe(`/`test(`/`it(`) can stay but should move to call-expression AST matching if trivially expressible with the existing parsed structures — otherwise leave and note it. Ruby and C++ stay as-is per the Decision Log.

Storage caveat: `FileState.contains_tests` is persisted and rehydrated (`storage_contains_tests`/`hydrate_contains_tests` in `tree_sitter_analyzer.rs:170-176`). Detector changes alter the computed bit, so stale stored values must not survive. Check how stored analysis is versioned/invalidated (see commit 69b4aa7c "Refactor analyzer queries for storage-ready results" and the storage module it touched); bump the format/version key if one exists, or confirm stored state is keyed by content hash such that re-analysis happens naturally. Record the finding in Surprises & Discoveries.

Update existing per-language detection tests to the tightened semantics and add the new negatives (Java `@TestOnly` file → false; Rust string-literal `"#[test]"` → false; PHP `Contest` → false).

### Milestone 3 — `classify_test_files` public API, MCP tool, Python client, docs

In `src/searchtools.rs`, next to `contains_tests`, add:

    #[derive(Debug, Clone, Deserialize)]
    pub struct ClassifyTestFilesParams { pub file_paths: Vec<String> }

    #[derive(Debug, Clone, Serialize)]
    pub enum TestFileKind { Test, TestSupport, Production, Ambiguous }   // serialized "test" | "test_support" | "production" | "ambiguous"

    #[derive(Debug, Clone, Serialize)]
    pub struct TestFileClassification {
        pub kind: TestFileKind,
        /// Semantic runnable-test detection for the same file (the
        /// `contains_tests` bit), reported so callers can separate
        /// "file-level surface" from "contains runnable tests".
        pub contains_test_code: bool,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct ClassifyTestFilesResult {
        pub classifications: BTreeMap<String, TestFileClassification>,
        pub unresolved: Vec<String>,
    }

    pub fn classify_test_files(analyzer: &dyn IAnalyzer, params: ClassifyTestFilesParams) -> ClassifyTestFilesResult

Classification rule (per the Decision Log): let `test_like = path_test_verdict(path) == TestRoot || has_test_filename_convention(path, language)`; if `test_like` and `contains_tests` → `Test`; if `test_like` and not → `TestSupport`; else if `path_test_verdict(path) == ProductionRoot` → `Production`; otherwise → `Ambiguous`. Resolve paths the same way `contains_tests()` does (reuse its resolution helper; unresolvable inputs go to `unresolved`).

Expose it: new descriptor `classify_test_files` in `src/mcp_cli.rs` in the `cli` toolset ("Classify each workspace file as test, test_support, production, or ambiguous for test-surface identification. Combines path conventions with semantic test detection; ambiguous means path conventions were inconclusive — consult contains_test_code. Use contains_tests for the purely semantic predicate."); dispatch arm in `src/searchtools_service.rs` beside the `contains_tests` arm; Python client method `classify_test_files(file_paths) -> dict[str, dict]` in `bifrost_searchtools/client.py`; rows in `docs/src/content/docs/mcp.md` and `docs/src/content/docs/python-client.md`.

Fix the overselling docs: in `src/searchtools.rs` (~5713) and `bifrost_searchtools/client.py` (359-370), remove the "hermetic acceptance that resets test files" justification from `contains_tests` and replace it with an explicit warning that `contains_tests` is semantic-only and cannot identify the full test surface (fixtures/helpers return false); point hermetic consumers at `classify_test_files` (`kind` of `test` or `test_support` is the test surface; `ambiguous` needs caller judgment, typically via `contains_test_code`). Same adjustment to the MCP descriptor text in `src/mcp_cli.rs`.

### Milestone 4 — migrate internal consumers

- `src/searchtools.rs` `is_test_candidate` (symbol search filtering + ranking): keep semantics (`contains_tests || is_test_like_path`) but on the shared module — done mechanically in Milestone 1; confirm ranking tests still pass.
- `src/searchtools.rs` `excluded_test_files` (used by `scan_usages`/`usage_graph` when `include_tests=false`, lines ~2840-2858): currently excludes only semantically-detected files, so usages inside fixtures/helpers leak into "non-test" results — the same false-negative class as issue 604. Switch to excluding files whose classification is `Test` or `TestSupport` (leave `Ambiguous` files included — exclusion is for known-test files only). This is a deliberate behavior change; add/adjust a scan_usages test showing a helper under a test root is excluded by default and included with `include_tests=true`.
- `src/commit_analysis.rs` (lines ~416, ~613): use the shared path module (Milestone 1); additionally, where it ORs with `contains_tests`, consider using the classification helper for consistency. Keep the change minimal if tests push back; record the outcome.
- `src/code_quality/secret_like_code.rs`: shared path module only (already done in Milestone 1).
- Assertion-smell gating (`report_test_assertion_smells` and per-language `find_test_assertion_smells`): intentionally stays on semantic `contains_tests` — smells are about runnable assertions. No change.

### Milestone 5 — regression tests from the issue

Using `InlineTestProject` (`tests/common/inline_project.rs`) per repo test guidance, add `tests/classify_test_files_test.rs` mirroring issue 604's "Suggested regression coverage":

- A C# AutoFixture-style customization class at `test/Core.Test/Auth/AutoFixture/Fixtures.cs` with no test attributes → `kind == TestSupport`, `contains_test_code == false`.
- A Java helper class at `src/test/java/x/TestService.java` with no `@Test` → `TestSupport`.
- A Java runnable test at `src/test/java/x/FooTest.java` with `@Test` → `Test`, `contains_test_code == true`.
- A Java production file at `src/main/java/x/MainProject.java` containing `@TestOnly` annotations and a `forTests(...)` helper → `Production`, `contains_test_code == false` (this asserts both the classification rule and the Milestone 2 detector tightening).
- A Rust `src/lib.rs` with an inline `#[cfg(test)] mod tests` → `Ambiguous`, `contains_test_code == true` (documents the two-field contract: `src/` is neither a test root nor a recognized production root, so path conventions are honestly inconclusive and the semantic bit carries the signal).
- A Go `pkg/foo_test.go` → `Test` via filename convention with no test-root directory.

Extend `tests/bifrost_mcp_server.rs` with an end-to-end `classify_test_files` call (mirror the existing `bifrost_cli_toolset_exposes_contains_tests` test at lines ~871-946).

## Concrete Steps

All commands run from the repository root `/home/jonathan/Projects/bifrost`.

    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings   # repo denies warnings; --all-features is safe (only nlp,python features exist)
    BIFROST_SEMANTIC_INDEX=off cargo test --test classify_test_files_test
    BIFROST_SEMANTIC_INDEX=off cargo test --test java_test_detection_test --test csharp_test_detection_test --test rust_test_detection_test --test scala_test_detection_test --test php_test_detection_test
    BIFROST_SEMANTIC_INDEX=off cargo test --test bifrost_mcp_server
    BIFROST_SEMANTIC_INDEX=off cargo test --features nlp,python    # full suite before declaring a milestone done; featureless `cargo test` silently skips the cfg(feature)-gated suites

Set `BIFROST_SEMANTIC_INDEX=off` when spawning the binary in new tests, per repo NLP test guidance. Commit at each milestone boundary on the current branch (no new branches), staging only files this plan touched.

## Validation and Acceptance

Acceptance is behavioral: after Milestone 3, calling the `classify_test_files` MCP tool on a workspace containing the Milestone 5 inline fixtures returns `test_support` for the C# AutoFixture file and the Java helper, `production` for the `@TestOnly`-bearing `src/main` file, `ambiguous` for the Rust `src/lib.rs` with inline tests, and `test` for the `@Test`-bearing file — while `contains_tests` on the same files returns false/false/false/true respectively. The new regression tests fail before their corresponding milestone and pass after (verify at least the Java `@TestOnly` negative genuinely fails against the pre-change detector before landing Milestone 2 — per repo practice, always confirm a regression test fails pre-fix). The full `cargo test` suite and clippy gate pass.

## Idempotence and Recovery

All changes are additive-or-replacing source edits guarded by the test suite; every step can be re-run safely. If a milestone's behavior change (notably the `scan_usages` exclusion change in Milestone 4) causes unexpected downstream test churn, land the milestone's mechanical part, record the churn in Surprises & Discoveries, and resolve before proceeding. Commits at milestone boundaries provide rollback points.

## Interfaces and Dependencies

No new external dependencies. New module `src/analyzer/test_paths.rs` (functions `path_test_verdict`, `has_test_filename_convention`, `is_test_like_path`); new public items in `src/searchtools.rs` (`ClassifyTestFilesParams`, `TestFileKind`, `TestFileClassification`, `ClassifyTestFilesResult`, `classify_test_files`); new MCP descriptor `classify_test_files` in `src/mcp_cli.rs`; new dispatch arm in `src/searchtools_service.rs`; new Python client method in `bifrost_searchtools/client.py`. `IAnalyzer` and `LanguageAdapter` signatures are unchanged — classification composes the existing `contains_tests` bit with the new path module at the searchtools layer, so no per-language adapter plumbing is added.

## Revision Notes

- 2026-07-10 (pre-implementation, Jonathan's review): changed ambiguous-path handling from "default to `production`" to a distinct `ambiguous` kind in `TestFileKind`. Reason: defaulting to `production` presents a guess as a definitive classification; surfacing ambiguity lets callers apply their own policy using the separately reported `contains_test_code` bit. Updated the Decision Log, the enum and classification rule in Milestone 3, the `scan_usages` exclusion rule in Milestone 4 (exclude only `Test`/`TestSupport`), the Rust inline-test expectation in Milestone 5, and Validation and Acceptance accordingly. Also corrected the Concrete Steps test commands: full-suite runs need `--features nlp,python` (featureless `cargo test` silently skips feature-gated suites) and `BIFROST_SEMANTIC_INDEX=off` on all test invocations.
- 2026-07-10 (Milestones 1 and 2 implementation, Codex): added the shared path-convention module and routed symbol search, commit analysis, and secret-like-code suppression through it; rewrote Java, C#, Rust, Scala, and PHP semantic detectors to use tree-sitter structure or existing parsed declarations; bumped affected analyzer epoch salts because unchanged persisted rows could otherwise hydrate stale `contains_tests` bits.
