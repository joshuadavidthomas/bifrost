# Resolve C parameters named `this` as typed local bindings

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept current. This document follows `.agents/PLANS.md`.

## Purpose / Big Picture

Plain C permits a function parameter named `this`. Bifrost parses C with the shared C++ grammar, which represents reference occurrences of that spelling as the special C++ `this` node. As a result, navigation and usage analysis interpret `this->field` as an implicit C++ receiver even in a `.c` file and cannot recover the parameter's declared struct type. After this change, a proven `.c` source with `struct S *this` will resolve `this->field` through the typed local parameter, while the same spelling in `.cpp` retains normal implicit-receiver behavior. A C reference without a visible typed `this` binding will remain unresolved rather than guessing an owner.

## Progress

- [x] (2026-08-13 11:13Z) Read #2092, the C/C++ forward and inverse resolver paths, and the repository testing rules.
- [x] (2026-08-13 11:18Z) Parsed the minimal C fixture with the production C++ grammar and confirmed that the declaration is an ordinary identifier while reference occurrences are `this` nodes.
- [x] (2026-08-13 11:27Z) Added shared C-source dialect detection and routed `this` reference nodes through local bindings in forward, targeted inverse, and bulk inverse analysis.
- [x] (2026-08-13 11:34Z) Added InlineTestProject behavior tests for ordinary, direct, nested-shadow, wrong-owner, missing-binding, targeted-inverse, and C++ implicit-receiver behavior.
- [x] (2026-08-13 11:39Z) Passed the three new issue tests, C/C++ receiver regressions, both affected crates' all-target clippy, formatting, dependency validation, and diff checks.
- [x] (2026-08-13 12:02Z) Committed and pushed the ordinary typed-binding fix as `e866c7bd` in merge `4496c7f9`, rebuilt the release runner, and replayed the three representative witnesses.
- [x] (2026-08-13 12:17Z) Added shared structured recovery for an exact active function-like macro whose replacement is one local declaration, and used it in forward, targeted-inverse, and bulk-inverse binding engines.
- [x] (2026-08-13 12:24Z) Passed the four #2092 behavior tests, established C/C++ receiver regressions, affected-crate all-target clippy, dependency validation, formatting, and diff checks.
- [x] (2026-08-13 12:48Z) Replayed the production pgBackRest macro witness, preserved known macro definitions across unrelated unavailable includes, and pinned identical guarded re-includes, conflicting conditional definitions, and `#undef` controls.
- [x] (2026-08-13 12:53Z) Re-ran resolver unit tests, the four #2092 integration tests, affected-crate all-target clippy, dependency validation, formatting, and diff checks after the visibility correction.
- [x] (2026-08-13 13:01Z) Amended checkpoint `e283a796`, rebuilt the release runner, and replayed all three exact witnesses from clean pinned repositories: two resolved with exact inverse hits and the macro-local value was canonically adjudicated, with zero actionable findings, precision findings, or file errors.
- [x] (2026-08-13 13:07Z) Pushed `259fed16` to `origin/master`, published focused and exact-replay evidence, and closed #2092 without waiting for the full corpus report, as requested.

## Surprises & Discoveries

- Observation: the declaration side already has enough structured information and does not need recovery text parsing.
  Evidence: tree-sitter-cpp parses `int read_field(struct S *this)` with a `parameter_declaration` whose pointer declarator terminates in an ordinary `identifier`. Existing `extract_variable_name` therefore yields `this`, and existing typed-binding seeders can resolve `struct S`.

- Observation: only use sites receive the C++ keyword node kind.
  Evidence: in `consume(this)`, `this->field`, and `this != 0`, the grammar emits a `this` node. The forward resolver, targeted graph extractor, and bulk inverted graph all special-case that kind as an enclosing C++ class rather than consulting their already-seeded local binding engines.

- Observation: direct parameter navigation is resolved before the C++ language-specific reference route.
  Evidence: the InlineTestProject query at `consume(this)` navigates to the indexed `struct S *this` parameter through the shared lexical-definition layer. The language-specific C branch remains necessary for recovered/unindexed binder shapes and returns the canonical local-value diagnostic when it can prove a shadow without a navigable declaration.

- Observation: two of the three exact corpus witnesses use an ordinary typed binding, but the third creates the binding through a function-like macro.
  Evidence: at pushed `4496c7f9`, strongSwan `this->usercert` and pgBackRest `this->varList` became consistent with exact inverse hits. PgBackRest `ASSERT(this != NULL)` remained missing because its function body contains `THIS(StorageAzure);`, where `#define THIS(type) type *this = thisVoid`; the AST has a call expression rather than a declaration even though the active macro environment contains the exact declaration template.

- Observation: the shared macro environment contains enough structure to recover this declaration without parsing the invocation or source statement as text.
  Evidence: `MacroDefinition::Function` preserves ordered formal parameters and the replacement. Parsing the replacement once inside a synthetic function body yields exactly one tree-sitter declaration, its declarator name, pointer depth, and a type node whose spelling is the formal parameter. The invocation supplies the corresponding structured argument node (`StorageAzure`).

- Observation: the exact-site corpus runner can know the active macro and later lose exactness because an included header is outside prepared syntax.
  Evidence: pgBackRest first applied the exact `THIS(type)` definition from `common/type/object.h`, then encountered `storage/azure/write.h` without prepared syntax. The macro environment correctly became globally uncertain, but its old representation replaced every known definition with `Unsupported`. Later guarded re-includes therefore could not prove that the same `THIS` definition remained a viable candidate.

## Decision Log

- Decision: make source dialect explicit with one shared `is_c_source_file` helper based on the exact project-relative `.c` extension.
  Rationale: `Language::Cpp` intentionally serves both C and C++, and the repository already uses the `.c` extension as its conservative proof of plain-C semantics. Centralizing that existing rule prevents the three resolver surfaces from drifting. Headers remain dialect-ambiguous and retain C++ behavior until compilation-language projections can prove otherwise.
  Date/Author: 2026-08-13 / Codex

- Decision: reinterpret only the structured `this` node in proven C source; do not rewrite source text or alter the parser.
  Rationale: the AST already distinguishes the declaration and use-site shapes. Looking up the exact spelling in the local binding engine preserves source order, scope, shadowing, and type precision. Missing or ambiguous bindings naturally fail closed.
  Date/Author: 2026-08-13 / Codex

- Decision: materialize macro-produced locals only from a structurally known active function-like macro whose replacement parses as one declaration.
  Rationale: this fixes the production `THIS(type)` shape at the macro-environment source of truth and lets all three binding engines share one interpretation. The replacement template is cached by definition identity; the declared type is instantiated from the exact AST argument node. A globally uncertain environment retains a previously known definition as provisional structured evidence, while a known `#undef` or conflicting conditional definition still replaces it with `Unsupported`. Arity mismatch, malformed or multi-statement replacements, and unsafe nested expansions fail closed. No regex, delimiter scan, or macro-name special case is involved.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

Issue #2092 is complete and closed. The ordinary typed-binding implementation and macro-produced local completion are pushed at `259fed16`. All focused validation and all three clean pinned exact replays pass: strongSwan `usercert` resolves to `private_nm_creds_t.usercert`, pgBackRest `varList` resolves to `VarStore.varList`, both have exact inverse hits, and pgBackRest's direct macro-local `this` is an adjudicated `local_variable_reference`. Per the user's instruction, publication did not wait for a full CI or full 813-site corpus report.

## Context and Orientation

`crates/bifrost-analysis/src/analyzer/usages/get_definition/cpp.rs` implements forward navigation. `cpp_seed_active_path` populates a `LocalInferenceEngine<CppType>` from parameters and earlier declarations. `cpp_receiver_type_units` currently resolves an `identifier` through that engine but resolves a `this` node through `cpp_enclosing_class` unconditionally. Direct focus on a C `this` token is currently rejected before the local-binding diagnostic route because `cpp_reference_node` does not classify `this` as an identifier reference.

`crates/bifrost-cpp/src/graph/extractor.rs` implements target-specific inverse usage scanning. Its `ScanCtx.bindings` contains typed local declarations. `receiver_type_units_with_budget` resolves identifiers through those bindings but treats `this` as the enclosing class, and its self-receiver helpers classify every `this` as C++ self before receiver typing.

`crates/bifrost-cpp/src/graph/inverted.rs` implements bulk inverse edges. Its per-file `LocalInferenceEngine<CodeUnit>` likewise seeds parameter types, but `receiver_type_unit` and `receiver_is_self_like` bypass the engine for `this`.

The shared dialect helper belongs in `crates/bifrost-cpp/src/graph/resolver.rs`, which already owns common graph syntax and binding helpers and already applies the same `.c` rule at several C/C++ semantic boundaries. It accepts a `ProjectFile` and returns true only when the project-relative extension is exactly `c`.

## Plan of Work

Add `is_c_source_file` to the shared C++ graph resolver and replace nearby duplicate extension checks where doing so is mechanically exact. In forward definition lookup, when the focused node is `this` in a C source, build local bindings at the use byte and return the canonical local-variable diagnostic if the binding is visible; otherwise return no indexed definition. In receiver typing, treat a C `this` node exactly like an identifier named `this`, including pointer/arrow unwrapping and the existing shadow verdict; preserve the current enclosing-class path for non-C files.

In the target-specific extractor, make self-like receiver classification dialect-aware. A C `this` must reach `ScanCtx.bindings` and resolve to its declared type; a C++ `this` and wrappers such as `(*this)` retain self-receiver classification. Apply the same rule in bulk inverted scanning before the same-owner shortcut and in `receiver_type_unit`.

Add `tests/suite_issues/issue_2092_c_this_binding.rs` and register it in `tests/suite_issues/main.rs`. Use `InlineTestProject` with a `.c` file containing a named struct, a parameter named `this`, direct value uses, and member reads. Assert forward lookup of the member reaches the struct member and direct `this` navigates to the indexed parameter declaration through the shared lexical-definition layer. Assert targeted usage lookup attributes member references to the typed struct owner. Add a `.cpp` fixture proving implicit `this` behavior is unchanged. Add C controls for an untyped/missing `this` binding, an unrelated receiver, a narrower typed shadow where the grammar permits it, and wrong-owner same-named members.

## Milestones

The first milestone is shared behavior parity. It ends when forward, targeted inverse, and bulk inverse all consult a typed C binding for a `this` node while C++ self semantics remain unchanged. The new InlineTestProject tests must fail on the old implementation and pass on the new one.

The second milestone is production acceptance. Rebuild the release differential runner, replay the three representative exact sites from strongSwan and pgBackRest with ephemeral cache, and require the member targets or local-binding adjudication described by #2092. Then rerun the complete rank-31+ C leg at the original limits and verify that all 813 occurrence keys clear without adding C++ receiver regressions or inverse-precision findings.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`. Edit only with `apply_patch`, then run focused commands such as:

    cargo fmt --all
    cargo test -p brokk-bifrost-cpp --lib graph::resolver
    cargo test --test suite_issues -- issue_2092_c_this_binding::
    cargo test --test suite_symbols -- get_definition_test::cpp_
    cargo test --test suite_usages -- usages_cpp_graph_test::
    cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Run only focused tests during implementation. Before pushing, run the practical featureless pre-push subset above; this task does not touch NLP or Python and does not require those features.

## Validation and Acceptance

For a `.c` fixture with `struct S *this`, lookup at `field` in `this->field` must resolve only `S.field`. Lookup at the direct `this` argument/value occurrence must navigate to the indexed parameter declaration rather than report an unsupported C++ receiver. Targeted inverse analysis must attribute the member occurrence to `S.field`, not to an enclosing class or unrelated same-name owner. The bulk inverse implementation shares the same C binding rule even though the public workspace graph catalog intentionally excludes fields.

For `.cpp`, `this->field` inside a class must continue to resolve through the enclosing class, and same-owner usage classification must remain unchanged. A `.c` file without a visible typed `this` binding must not select a same-named global type, field, or arbitrary enclosing owner. Scope exit and a nearer typed binding must obey the existing local inference engine.

Corpus acceptance is zero remaining matches among the 813 #2092 keys after replay, with clean pinned repositories, completed status, zero file errors, and no widened C++ receiver behavior.

## Idempotence and Recovery

The change modifies only deterministic AST and graph interpretation. Tests and corpus replays are safe to repeat. Use a new revision-specific artifact path for each replay and preserve prior evidence for comparison. If a replay is interrupted, rerun with ephemeral cache for exact smoke evidence or the campaign's persisted mode for resumable full-corpus work.

## Artifacts and Notes

The baseline complete C ledger is:

    /mnt/optane/tmp/bifrost-fird/final-fcd83045/c-ranks31-50-fcd83045-raw-ledger.jsonl

Its audited partition contains 813 #2092 occurrence keys: 590 in strongSwan and 223 in pgBackRest. Representative keys begin `4b3458b8c61c`, `f70a87f6984d`, and `910e07ba919d`.

Minimal parser evidence:

    declaration: parameter_declaration -> pointer_declarator -> identifier (`this`)
    uses: argument_list -> this; field_expression.argument -> this; binary_expression.left -> this

Plan revision note (2026-08-13): Created from #2092 after confirming the exact tree-sitter-cpp declaration/use asymmetry and identifying the shared binding engines already present in all three resolver surfaces.

Plan revision note (2026-08-13): Recorded the implemented shared dialect/binding route and focused acceptance. Direct `this` navigation proved to be handled by the shared lexical-definition layer before the C++ resolver; member ownership still required the three C-aware receiver changes.

Plan revision note (2026-08-13): Recorded the first pushed checkpoint and exact replay. The third witness revealed the active `THIS(type)` declaration-macro variant, so the plan now includes shared structured macro-local materialization rather than treating the ordinary parameter fix as complete.

## Interfaces and Dependencies

Expose `pub fn is_c_source_file(file: &ProjectFile) -> bool` and `VisibilityIndex::function_macro_local_binding` from `brokk_bifrost_cpp::graph::resolver`. The macro-local result carries the declared name, structured invocation type node when parameterized, normalized type spelling, and declarator pointer depth. No new crate or dependency is needed. Forward analysis imports the shared APIs from the language crate; target-specific and bulk graph code call them within the same crate.
