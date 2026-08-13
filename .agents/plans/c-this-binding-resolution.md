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
- [ ] Commit, push, replay representative corpus witnesses, publish evidence, and close #2092.

## Surprises & Discoveries

- Observation: the declaration side already has enough structured information and does not need recovery text parsing.
  Evidence: tree-sitter-cpp parses `int read_field(struct S *this)` with a `parameter_declaration` whose pointer declarator terminates in an ordinary `identifier`. Existing `extract_variable_name` therefore yields `this`, and existing typed-binding seeders can resolve `struct S`.

- Observation: only use sites receive the C++ keyword node kind.
  Evidence: in `consume(this)`, `this->field`, and `this != 0`, the grammar emits a `this` node. The forward resolver, targeted graph extractor, and bulk inverted graph all special-case that kind as an enclosing C++ class rather than consulting their already-seeded local binding engines.

- Observation: direct parameter navigation is resolved before the C++ language-specific reference route.
  Evidence: the InlineTestProject query at `consume(this)` navigates to the indexed `struct S *this` parameter through the shared lexical-definition layer. The language-specific C branch remains necessary for recovered/unindexed binder shapes and returns the canonical local-value diagnostic when it can prove a shadow without a navigable declaration.

## Decision Log

- Decision: make source dialect explicit with one shared `is_c_source_file` helper based on the exact project-relative `.c` extension.
  Rationale: `Language::Cpp` intentionally serves both C and C++, and the repository already uses the `.c` extension as its conservative proof of plain-C semantics. Centralizing that existing rule prevents the three resolver surfaces from drifting. Headers remain dialect-ambiguous and retain C++ behavior until compilation-language projections can prove otherwise.
  Date/Author: 2026-08-13 / Codex

- Decision: reinterpret only the structured `this` node in proven C source; do not rewrite source text or alter the parser.
  Rationale: the AST already distinguishes the declaration and use-site shapes. Looking up the exact spelling in the local binding engine preserves source order, scope, shadowing, and type precision. Missing or ambiguous bindings naturally fail closed.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The structured implementation and focused behavior gate are complete locally. Proven `.c` sources resolve `this->field` through the visible typed binding on forward and targeted-inverse surfaces; the bulk inverse implementation uses the same rule. Nested C bindings preserve lexical precedence, missing bindings fail closed, and `.cpp` implicit receivers remain unchanged. Commit, push, corpus replay, publication, and issue closure remain.

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

## Interfaces and Dependencies

Expose `pub fn is_c_source_file(file: &ProjectFile) -> bool` from `brokk_bifrost_cpp::graph::resolver`. It must depend only on the core `ProjectFile` path API. No new crate or dependency is needed. Forward analysis imports the helper from the language crate; target-specific and bulk graph code call it within the same crate.
