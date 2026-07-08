# Count static-member-qualified type references in scan_usages (C#, Java, C++, Scala, Python)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `.agents/PLANS.md` (repository root relative path), which defines the rules for ExecPlans.

## Purpose / Big Picture

When a user asks bifrost's `scan_usages` tool for the usages of a class, the answer must include places where the class name appears as the qualifier of a static member access — `FileNameBuilder.CleanFileName(...)` in C#, `BaseClass.staticMethod()` in Java, `Foo::CONST` in C++, `Foo.staticMethod(x)` in Scala, `Foo.static_helper()` in Python. Today five language strategies silently drop these references: they are not reported as hits, not reported as unproven hits, and not reflected in any diagnostic. The scan simply behaves as if the reference does not exist.

This is a real, user-visible false negative discovered while investigating GitHub issue #543. Concretely: against the Radarr repository (a large C# codebase, clone available at `/home/jonathan/Projects/brokkbench/clones/Radarr__Radarr`, sha `187dd79b9c9ddb27e8eebfb6f6dfa4ef8dd1c3ee`), `scan_usages` for the class `NzbDrone.Core.Organizer.FileNameBuilder` returns only 7 hits — all of them test fixtures of the form `class XFixture : CoreTest<FileNameBuilder>` (a generic type argument, which is handled) — and misses every one of the roughly 11 production call sites, all of which reference the class only as a static-access qualifier, e.g. `src/NzbDrone.Core/Download/UsenetClientBase.cs:43` (`FileNameBuilder.CleanFileName(...)`) and `src/NzbDrone.Core/Organizer/FileNameValidation.cs:46` (`FileNameBuilder.MovieTitleRegex`).

After this change, a class-target `scan_usages` query counts static-qualifier references in all languages that have the idiom, with the proven/unproven proof-tier semantics the tool already uses elsewhere. The fix is one behavioral contract applied per language, not a C#-only point fix.

Languages that already handle this correctly and must not regress: JS/TS (`src/analyzer/usages/js_ts_graph/extractor.rs:590-599`, pinned by `tests/usages_js_ts_graph_test.rs:736-758` — the `Ky.create('url')` test), Rust (`src/analyzer/usages/rust_graph/extractor.rs:221-244`, unguarded identifier scan), PHP (`src/analyzer/usages/php_graph/extractor.rs:105-118`, unguarded name scan), Ruby (`src/analyzer/usages/ruby_graph/extractor.rs:36-101`, pinned by `tests/usages_ruby_test.rs:751-779`). Go is not applicable: it has no static-member-on-type idiom (methods take explicit receivers; `pkg.Name` is a package qualifier, a different concept).

## Progress

- [ ] Milestone 1: failing tests pinning the contract in C#, Java, C++, Scala, Python; passing tests confirming JS/TS, Rust, PHP, Ruby already conform.
- [ ] Milestone 2: C# fix (reference implementation of the shape).
- [ ] Milestone 3: Java and C++ fixes.
- [ ] Milestone 4: Scala and Python fixes.
- [ ] Milestone 5: end-to-end verification against the Radarr worktree; update `.agents/docs/scan-usages-hardab80-inconsistent-results.md` with the issue-543 verdicts; close-out.

## Surprises & Discoveries

- Observation: of the four "inconsistent results" cases in issue #543, three were benchmark-extraction artifacts (calls recorded against mutated or mid-edit working trees, or with a `paths` filter the extraction's dedup key dropped) and one was already fixed by the issue-528 proof-tier work. The only genuine engine defect found is the static-qualifier blind spot this plan fixes, which is a silent undercount, not an inconsistency.
  Evidence: five repeated runs of `scan_usages` for `NzbDrone.Core.Organizer.FileNameBuilder.TitleRegex` at the pristine sha return 4/4/4/4/4 hits deterministically; the class-level query returns 7/7/7 but never any of the ~11 static-access production sites.

## Decision Log

- Decision: fix per-language type-reference gates rather than introducing a single shared "is this a type reference" abstraction.
  Rationale: there is no existing shared choke point — `src/analyzer/usages/traits.rs` defines only strategy-level interfaces, and each language hand-rolls its own gate with different AST vocabularies (tree-sitter node kinds differ per grammar). Inventing a cross-grammar abstraction now would be speculative; the generalization we enforce instead is a single behavioral contract (same test shape in every language) plus a shared implementation recipe (qualifier position check, then not-a-local-binding check, then existing type resolution, then proven/unproven tiering). If the per-language patches come out structurally identical, a follow-up may factor the "seed bindings and reject local-bound qualifiers" step into a helper; that is optional and should not block landing.
  Date/Author: 2026-07-08 / Claude (planning session for issue #543).
- Decision: report a resolved qualifier as a proven hit, an unresolvable-but-plausible qualifier as an unproven hit, and a qualifier that resolves to something else (a local variable, a different type) as no hit.
  Rationale: this is the established proof-tier idiom from the issue-528 work; see the idiom citations in Context and Orientation. Silently dropping plausible references is exactly the bug class this repository's design philosophy forbids.
  Date/Author: 2026-07-08 / Claude.

## Outcomes & Retrospective

(To be written at completion.)

## Context and Orientation

bifrost is a Rust code-analysis server. Its `scan_usages` tool, given a fully qualified symbol name such as `NzbDrone.Core.Organizer.FileNameBuilder`, scans a workspace for references. Per-language "usage graph strategies" live under `src/analyzer/usages/<lang>_graph/`, each with an `extractor.rs` (walks tree-sitter ASTs over candidate files, emitting hits) and a `resolver.rs` (name and type resolution helpers). The scan target carries a `TargetKind` (`Type`, `Method`, `Field`, `Constructor`); this plan concerns `TargetKind::Type` — queries whose target is a class, struct, interface, trait, or object.

Two terms of art:

- "Static-qualifier reference": an identifier that names a type and appears only as the left side (receiver, qualifier, scope) of a member access whose member is static — `Foo.bar()` where `Foo` is a class, not a variable. In tree-sitter grammars the qualifier is usually a plain `identifier` node (not a `type_identifier`), which is why type-reference gates that filter on node kind or parent shape miss it.
- "Proof tiers": `scan_usages` distinguishes proven hits (structural resolution confirmed the reference denotes the target) from unproven hits (structurally plausible but resolution was ambiguous or incomplete). The sinks are per-language functions named `push_hit`/`push_unproven_hit` (C#, Java, C++) or `record_hit`/`record_unproven_hit` (JS/TS, Python) in each `extractor.rs`/`hits.rs`. Representative idiom: `src/analyzer/usages/csharp_graph/extractor.rs:247-262` (member scan: `SymbolResolution::Precise` that matches → `push_hit`; `Ambiguous | Unknown` → `push_unproven_hit`).

Where each affected language drops the reference today (verified at master commit `26ddbbc`):

- C#: `scan_type_reference` (`src/analyzer/usages/csharp_graph/extractor.rs:115-135`) returns early unless `is_type_reference_node(node)` holds. `is_type_reference_node` (`src/analyzer/usages/csharp_graph/resolver.rs:978-1023`) walks parents accepting `type`/`return_type` field slots, `object_creation_expression`, declaration self-references, and the wrapper kinds `qualified_name | generic_name | nullable_type | array_type | type_argument_list | base_list` — but never `member_access_expression`. An identifier that is the `expression` field of a `member_access_expression` falls to `return false`.
- Java: `maybe_record_type_hit` (`src/analyzer/usages/java_graph/extractor.rs:236-259`) only considers nodes of kind `type_identifier | scoped_type_identifier | generic_type`. In tree-sitter-java, the `object` field of a `method_invocation` or `field_access` is a plain `identifier`, so `BaseClass.staticMethod()` is never examined. (The fixture `tests/fixtures/testcode-java/ClassUsagePatterns.java:26` already contains `BaseClass.staticMethod();` but no test asserts it as a class usage.)
- C++: `maybe_record_type_hit` (`src/analyzer/usages/cpp_graph/extractor.rs:261-296`) does visit the whole `qualified_identifier` node for `Foo::CleanFileName(x)` (its text matches via `name_mentions`, which is structured on the node text), but then `resolves_to_type(file, "Foo::CleanFileName", target)` fails because a method-qualified path is not a type name, and when the target is visible in the file the `else if !is_visible → push_unproven_hit` branch does not fire either. The `scope` field of the `qualified_identifier` (node kind `namespace_identifier`) is never resolved on its own.
- Scala: the `TargetKind::Type` arm of `scan_identifier` (`src/analyzer/usages/scala_graph/extractor.rs:531-547`) requires `is_type_like_reference` (`src/analyzer/usages/scala_graph/syntax.rs:44-53`), which accepts `type_identifier` nodes, constructor-like positions, and parent kinds `type | generic_type | parameterized_type | extends_clause`. The `value` field of a `field_expression` (the `Foo` in `Foo.staticMethod(x)`) is a plain `identifier` with parent `field_expression` — rejected, with no unproven fallback.
- Python: `handle_member_expression`'s class-target branch (`src/analyzer/usages/python_graph/extractor.rs:608-617`) records `Foo.attr` as a class usage only when `ctx.binds_target(object_text, node)` AND `ctx.target_class_method_named(attribute_text)` — and the latter (defined around `python_graph/extractor.rs:493-506`) requires the attribute to be a method decorated `@classmethod`. Static methods and class-level constants (`Foo.static_helper()`, `Foo.LIMIT`) are dropped even though `binds_target` already proves `Foo` denotes the target class. Test `tests/usages_python_graph_test.rs:213-249` pins the classmethod case; the sibling `@staticmethod` in the same fixture is never asserted against the class target.

Reusable machinery the fixes should lean on (do not invent new resolution):

- C#: `seed_visible_bindings_at` (`src/analyzer/usages/csharp_graph/resolver.rs:87-96`) populates a `LocalInferenceEngine<String>` with locals and parameters in scope, so "is this identifier a local binding" is answerable via `bindings.resolve_symbol`. `class_field_receiver_type` (`resolver.rs:915-926`) answers "is this identifier a field/property of the enclosing class". `resolves_to_target_at` (`resolver.rs:611-621`) and `resolve_type_fq_name_at` (`resolver.rs:624-647`) resolve a bare name to a type through import/visibility analysis — the same calls `scan_type_reference` already makes for accepted nodes.
- Java: `receiver_matches_target` (`src/analyzer/usages/java_graph/resolver.rs:163-201`) shows the existing bare-identifier receiver handling for member targets, including `ctx.bindings.resolve_symbol` for the local-binding check; `resolve_type_from_node` in the same strategy resolves type names.
- Shared: `src/analyzer/usages/local_inference.rs` (`LocalInferenceEngine`, `SymbolResolution`) is the cross-language binding tracker already used by C#, Java, Python, JS/TS, and C++.
- The JS/TS implementation (`src/analyzer/usages/js_ts_graph/extractor.rs:590-599`) is the model to copy: for a class target, if the object of a member expression is a simple identifier and it binds to the target (binding check already excludes shadowing locals), record a hit unconditionally on the property name.

Test conventions: analyzer tests that need small ad hoc projects must use the shared inline harness `tests/common/inline_project.rs` (`InlineTestProject`) rather than handwritten tempdir setup — see the "Analyzer Test Guidance" section of the repository's `CLAUDE.md`. Per-language usage-graph tests live in `tests/usages_<lang>_graph_test.rs` / `tests/usages_ruby_test.rs` / `tests/usage_graph_<lang>_test.rs`; read one before writing new cases to match local style.

## Plan of Work

The contract, identical in every language: given a `TargetKind::Type` query for class `T`, an identifier `X` that (a) appears as the qualifier of a member access (`X.member`, `X::member`), (b) is not bound to a local variable, parameter, or (where the language has them) an instance field visible at that point, and (c) resolves to `T` through the language's existing type resolution, must produce a proven hit anchored at `X`. If (a) and (b) hold and the short name matches the target but resolution is ambiguous or unavailable, produce an unproven hit. If `X` is bound to a local/parameter/field, or resolves to a different type or a namespace, produce nothing — the existing member-target machinery handles instance receivers separately.

Chained accesses: only the leftmost qualifier can be a bare type reference (`Foo.CONST.length()` — `Foo` qualifies `CONST`; the result of `Foo.CONST` qualifies `length`). All fixes below anchor on the identifier node itself, which is inherently leftmost when its parent's qualifier field is the identifier directly; nested member accesses have a member-access node (not an identifier) in qualifier position and are skipped naturally.

C# (`src/analyzer/usages/csharp_graph/`): in `extractor.rs`, extend `scan_type_reference` (or add a sibling branch it calls) so that an `identifier` node that is the `expression` field of a `member_access_expression` is a candidate. Before resolving, seed bindings with `seed_visible_bindings_at` and reject the candidate if `bindings.resolve_symbol` finds a local/parameter, or if `class_field_receiver_type` says the name is a field/property of the enclosing class (in `foo.Bar()` where `foo` is a field, `foo` is an instance receiver, not a type). Then reuse the existing `type_reference_resolves_to_target` path: resolution success → `push_hit`; short-name match without resolution (mirror the `SymbolResolution::Ambiguous | Unknown → push_unproven_hit` shape from `scan_member_reference` at `extractor.rs:247-262`) → `push_unproven_hit`. Beware namespace qualifiers: in `System.Console.WriteLine`, tree-sitter parses `System.Console` as a nested `member_access_expression`, so the identifier `System` is in qualifier position; it must not become a hit for a class named `System` in another namespace unless resolution actually lands on the target — trust `resolves_to_target_at`, do not add name-only shortcuts for the proven tier.

Java (`src/analyzer/usages/java_graph/`): in `extractor.rs`, extend `maybe_record_type_hit` (or the dispatch that feeds it) to also consider `identifier` nodes whose parent is a `method_invocation` with this node as the `object` field, or a `field_access` with this node as the `object` field. Check `ctx.bindings.resolve_symbol` first (the same call `receiver_matches_target` uses) and bail if the identifier is a local binding. Then resolve via the strategy's existing type resolution (`resolve_type_from_node` operates on type nodes; the implementer should locate or add the bare-name variant used by import/visibility resolution — follow how `maybe_record_import_hit` resolves names) and emit `hits::push_hit` on match, `hits::push_unproven_hit` when the short name matches but resolution is inconclusive. Note `field_access` qualifiers can also be types from a chain like `Foo.CONST` used as `Foo.CONST.length()` — the `object` of the outer invocation is the whole `field_access`, not an identifier, so only the inner `field_access` sees the identifier; the natural anchoring rule above applies. Java has no instance-field-receiver ambiguity to worry about beyond locals because an unqualified field used as receiver still resolves through `resolve_symbol`'s scope handling — verify this while implementing, and if unqualified instance fields are not in the binding engine, add the same enclosing-class-field check C# uses.

C++ (`src/analyzer/usages/cpp_graph/`): in `extractor.rs`, inside `maybe_record_type_hit`, when the visited node is a `qualified_identifier` whose full text fails `resolves_to_type`, additionally resolve the node's `scope` field text (e.g. `FileNameBuilder` out of `FileNameBuilder::CleanFileName`) with the same `ctx.visibility.resolves_to_type(ctx.file, scope_text, &ctx.spec.target)` call; on success `push_hit` anchored at the scope node. Preserve the existing unproven branch semantics: if the scope text matches the target short name (`name_mentions`) but the target is not visible in the file, `push_unproven_hit`. C++ has no local-binding ambiguity for `X::member` — the `::` operator syntactically requires a type or namespace on the left — so no binding check is needed; a namespace with the same name as the target class is disambiguated by `resolves_to_type`. Handle nesting (`Outer::Inner::member`) by resolving the longest type prefix: the `scope` field of a nested `qualified_identifier` is itself a `qualified_identifier`; iterate rather than recurse, per the repository's stack-safety guidance.

Scala (`src/analyzer/usages/scala_graph/`): extend `is_type_like_reference` in `syntax.rs` (or add a branch in the `TargetKind::Type` arm of `scan_identifier` in `extractor.rs:531-547`) to accept an `identifier` that is the `value` field of a `field_expression`. The existing proven check in that arm is `ctx.visibility.type_names.contains(text)` — keep it, and add the local-binding guard: `scan_identifier` already maintains `ctx.bindings` (see `seed_value_binding_identifier` and the shadow declarations in the same function), so reject when `ctx.bindings.resolve_symbol(text)` is a known local. This matters in Scala more than elsewhere because `val config = ...; config.load()` has an identifier in exactly the same AST position. Scala objects (`object Foo`) are the primary beneficiaries — `Foo.method` where `Foo` is an object is that object's dominant usage form; confirm `type_names` includes objects (the existing test `tests/usages_scala_graph_test.rs:194-322` exercises `pkg.Utility$.help` as a member target against those call sites, so object plumbing exists).

Python (`src/analyzer/usages/python_graph/`): in `extractor.rs:608-617`, drop the `ctx.target_class_method_named(attribute_text)` conjunct so that `Foo.attr` records a class-target hit whenever `object.kind() == "identifier"`, `ctx.binds_target(object_text, node)` holds, and the existing namespace-import exclusion (`ImportEdgeKind::Namespace` with matching `local_name`) passes. `binds_target` already performs real resolution (import-aware, shadow-aware), so the extra method-name gate only causes false negatives for `@staticmethod` calls and class-constant reads. Before deleting it, run `git log -S target_class_method_named` to learn why the gate was added and check which tests pin it; if it was guarding against a specific false-positive class (e.g. module objects that `binds_target` mistakes for classes), preserve that specific guard rather than the blanket method-name requirement, and record the finding in the Decision Log.

Do not modify JS/TS, Rust, PHP, Ruby, or Go behavior. Add pinning tests where missing (PHP and Rust lack a static-qualifier class-target test; JS/TS and Ruby already have them).

## Concrete Steps

All commands run from the repository root `/home/jonathan/Projects/bifrost` unless stated otherwise.

Milestone 1 — tests first. For each of C#, Java, C++, Scala, Python, add a test to the language's existing usages test file (`tests/usages_csharp_graph_test.rs`, `tests/usages_java_graph_test.rs`, `tests/usages_cpp_graph_test.rs`, `tests/usages_scala_graph_test.rs`, `tests/usages_python_graph_test.rs`) using `InlineTestProject` with two files: one declaring a class with a static method and a class-level constant, one referencing them only as `Foo.staticMethod()` / `Foo.CONST` (adjust idiom per language: `Foo::method()` and `Foo::CONST` for C++; an `object Foo` for Scala; a `@staticmethod` and a class attribute for Python). Assert a class-target `scan_usages`-level result (drive whatever level the neighboring tests in that file drive — most exercise the graph strategy through the same public entry the existing tests use) contains proven hits at both reference sites. Add a negative case in the same test: a local variable with the same name as another class, used as `local.method()`, must not produce a class hit. Also add the missing pinning tests for PHP (`Target::make()` against the class target, reusing the fixture near `tests/usages_php_graph_test.rs:572-630`) and Rust (`Foo::assoc_fn()` against the struct target). Run:

    cargo test --test usages_csharp_graph_test --test usages_java_graph_test --test usages_cpp_graph_test --test usages_scala_graph_test --test usages_python_graph_test --test usages_php_graph_test --test usages_rust_graph_test

Expect the five new tests for the affected languages to FAIL (that is the point) and the PHP/Rust pins to PASS. Commit the tests only after confirming the failures are the expected missing-hit shape, marked `#[ignore]` is NOT acceptable — land them in the same commit as each language fix instead if the repository's CI would otherwise break; the recommended flow is one commit per milestone with tests and fix together.

Milestones 2-4 — implement per language as described in Plan of Work, in the order C# (reference), then Java and C++, then Scala and Python. After each language, run that language's usages tests plus the full `usage_graph_*` and `usages_*` suites for that language, then the repository gates:

    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings   # on CUDA-capable machines; use `cargo clippy-no-cuda` on machines without nvcc

Commit per milestone on the current branch (no new branches), with a multiline message explaining the why.

Milestone 5 — end-to-end proof against real code. A pristine Radarr worktree already exists at `/tmp/claude-1000/-home-jonathan-Projects-bifrost/c71f95cb-473e-4305-be2b-1fed2d709ecc/scratchpad/repro-radarr` (detached HEAD at `187dd79b9c9d`); if it is gone, recreate it with:

    git -C /home/jonathan/Projects/brokkbench/clones/Radarr__Radarr worktree add <scratch-path> 187dd79b9c9ddb27e8eebfb6f6dfa4ef8dd1c3ee

Then:

    cargo build --release
    BIFROST_SEMANTIC_INDEX=off target/release/bifrost --root <worktree> --tool scan_usages --args '{"symbols":["NzbDrone.Core.Organizer.FileNameBuilder"]}'

Acceptance: the result now includes production hits in `src/NzbDrone.Core/Parser/IsoLanguages.cs` (line 102), `src/NzbDrone.Core/Organizer/FileNameValidation.cs` (lines 46 and 63), `src/NzbDrone.Core/Download/UsenetClientBase.cs` (line 43), `src/NzbDrone.Core/Download/TorrentClientBase.cs` (line 199), `src/NzbDrone.Core/Download/Clients/Pneumatic/Pneumatic.cs` (lines 48 and 74), and the three Blackhole clients (`UsenetBlackhole.cs:42`, `TorrentBlackhole.cs:53,72`, `ScanWatchFolder.cs:56,97`) — in addition to the 7 pre-existing `CoreTest<FileNameBuilder>` test hits. Field-level queries must be unchanged: `{"symbols":["NzbDrone.Core.Organizer.FileNameBuilder.TitleRegex"]}` still returns exactly 4 hits (FileNameBuilder.cs lines 125, 178, 554, 568). Run each query twice to confirm determinism. Record the transcript in Artifacts and Notes.

Finally, update `.agents/docs/scan-usages-hardab80-inconsistent-results.md` appending a "Verdicts (2026-07-08)" section: case 1 fixed at head by the issue-528 proof tiers (commit `be51910`); case 2 benchmark artifact (target sha `980f874af13f` is the commit that deletes the queried fields; hit rows ran the parent checkout `88ce0b196333`); case 3 benchmark artifact for the reported flip, but it surfaced the static-qualifier gap this plan fixes; case 4 benchmark artifact (agent-deleted file mid-trajectory; a `paths` argument dropped by the extraction dedup key). Note the benchmark-side recommendation: extraction should key rows by full `raw_arguments` plus a content fingerprint of the working tree at call time, not the task's nominal sha.

## Validation and Acceptance

Behavioral acceptance, per language: the new inline-project test in each of the five affected languages fails before that language's fix and passes after; the negative (same-named local variable) case passes throughout; the JS/TS, Ruby, PHP, Rust pins pass throughout; the full `cargo test` suite for all `usages_*` and `usage_graph_*` targets passes; `cargo fmt` produces no diff and clippy (with the platform-appropriate invocation) reports no warnings. End-to-end acceptance is the Radarr transcript in Milestone 5: class query grows from 7 test-only hits to include the enumerated production sites, field query stays at exactly 4, both deterministic across repeated runs.

## Idempotence and Recovery

All steps are additive code and test changes; re-running tests and rebuilds is safe. The Radarr worktree is disposable — `git -C /home/jonathan/Projects/brokkbench/clones/Radarr__Radarr worktree remove <path>` cleans it up, and it can be recreated from the clone at any time. If a language fix causes false positives in the wider suite (most likely: namespace or module qualifiers resolving to same-named classes), do not weaken the test; tighten the resolution gate for that language — the proven tier must go through real resolution, and only the unproven tier may rest on name matching.

## Artifacts and Notes

Pre-fix baseline (bifrost `26ddbbc`, Radarr `187dd79b9c9d`), for comparison after Milestone 5:

    scan_usages {"symbols":["NzbDrone.Core.Organizer.FileNameBuilder"]}
    → 7 hits, all `NzbDrone.Core.Test/OrganizerTests/**/*Fixture.cs` (`CoreTest<FileNameBuilder>`); zero production sites.

    scan_usages {"symbols":["NzbDrone.Core.Organizer.FileNameBuilder.TitleRegex"]}
    → 4 hits: FileNameBuilder.cs:125,178,554,568 (correct; must remain identical post-fix).

## Interfaces and Dependencies

No public API changes. All edits are internal to `src/analyzer/usages/{csharp,java,cpp,scala,python}_graph/{extractor,resolver}.rs` plus `scala_graph/syntax.rs`, using only existing helpers: `LocalInferenceEngine`/`SymbolResolution` from `src/analyzer/usages/local_inference.rs`, per-language `seed_visible_bindings_at`/`resolve_symbol`/`resolves_to_target_at`/`resolves_to_type`/`binds_target`, and the per-language proven/unproven hit sinks. No new dependencies. Tree-sitter grammars are unchanged; the fixes read existing AST fields (`expression`, `object`, `scope`, `value`) rather than parsing text — consistent with the repository's prohibition on string-scanning fallbacks.
