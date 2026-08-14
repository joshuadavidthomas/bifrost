# Make the Rust reference census grade only real definition gaps

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

The reference differential deliberately probes more syntax than the normal analyzer index so it can discover missing code-intelligence behavior. In Rust, that broad census currently reports declaration names, lexical locals, lifetime parameters, and standard-library `Ok` and `Err` uses as product defects merely because an unrelated same-spelled declaration exists in the same file. After this change, those sites will either be excluded as non-references or recorded as structured, adjudicated negative answers, while genuine workspace gaps such as wildcard-imported enum variants remain actionable. A focused inline Rust project and an exact serde-json replay will demonstrate the difference.

## Progress

- [x] (2026-08-13 21:25Z) Read issue #2036, `.agents/PLANS.md`, the Rust resolver, the reference-candidate frontier, the census grader, and the preserved serde-json ledger.
- [x] (2026-08-13 21:25Z) Confirm that `Ok` and `Err` false positives are value-namespace uses promoted by same-file associated type aliases, not missing Result declarations.
- [x] (2026-08-13 22:03Z) Added an `InlineTestProject` regression for item and pattern declarations, locals, lifetimes, value/type namespace collisions, an import boundary, and a wildcard enum variant.
- [x] (2026-08-13 22:03Z) Implemented shared Rust declaration and binding-role exclusion plus canonical adjudicated diagnostics for proven lexical answers.
- [x] (2026-08-13 22:03Z) Made Rust same-file tier evidence respect the reference namespace and the structured owner of scoped paths.
- [x] (2026-08-13 22:03Z) Passed focused tests, all Rust crate tests, both affected crate clippy checks, dependency checks, exact replays, and the complete serde-json census comparison.
- [x] (2026-08-13 22:09Z) Committed as `c8a8dce74`, published via merge head `b42e383df`, attached exact evidence, and closed #2036 without waiting for full CI.

## Surprises & Discoveries

- Observation: The false `Ok` and `Err` evidence is not an enum variant with the wrong owner. Serde-json declares many associated types named `Ok` or `Err`; the declaration extractor represents type aliases as field-shaped CodeUnits marked by `RustAnalyzer::is_type_alias`. The census currently erases that namespace distinction.
  Evidence: `search_symbols` on the pinned serde-json checkout reports entries such as `serde_json.ser.Serializer.Ok` with `is_type_alias: true`, while the ledger sites are `call_expression>identifier` and `tuple_struct_pattern>identifier` value uses.
- Observation: The one preserved Rust ambiguity is a cfg-dependent `N` type with several physical definitions and type aliases at the same FQN. It is a real ambiguous answer, not joint blindness, but JavaScript ambiguity can represent a product precedence bug. The grader therefore must treat ambiguity as adjudicated only for this Rust change, not globally.
  Evidence: occurrence key `dda07d11538664110660b1f53697371479e0d28817ebc11e0607dc27a8debbbc` has status `ambiguous`, diagnostic `ambiguous_definition`, and eight cfg-variant targets.
- Observation: A bare Rust pattern identifier is not sufficient proof of a local binding. Tree-sitter gives wildcard-imported unit variants such as `Solidus` the same pattern shape as a binder, which is the open #2032 defect. The declaration graph can conservatively separate ordinary binders by retaining a pattern probe only when an exact same-file declaration is itself an `enum_variant` AST node.
  Evidence: the first implementation either retained all binder noise or risked excluding #2032. The final serde-json replay excludes the `key` tuple binder even though an enum-owned field has that spelling, while retaining `Solidus`, `Backspace`, `FormFeed`, and `CarriageReturn` as tier-2 Missing.
- Observation: The 10,000-site bounded sample cannot be compared by total actionable count after declaration exclusions because newly freed heap slots admit previously unsampled sites. Closure evidence must compare the 314 historical occurrence coordinates against the new report.
  Evidence: the final complete run has 284 actionable sites overall, but the exact historical-key join shows every #2036 family corrected and 54 intentionally residual historical keys owned by #2032, #1895, or other issues.

## Decision Log

- Decision: Use `brokk_bifrost_rust::graph::ast::is_rust_declaration_name` as the single structured declaration-role source and expose it through the analysis reference-candidate facade.
  Rationale: The language crate already owns the complete tree-sitter field interpretation. Duplicating function/constant/type-alias lists in the runner would drift and violate the repository's shared-helper rule.
  Date/Author: 2026-08-13 / Codex
- Decision: Canonicalize proven Rust locals, lifetimes, local type parameters, and explicit `self` receiver focuses to `LOCAL_VARIABLE_REFERENCE_DIAGNOSTIC_KIND`; canonicalize declarations to `DECLARATION_OR_IMPORT_SITE_DIAGNOSTIC_KIND`.
  Rationale: These are resolved negative answers under the existing cross-language contract. Keeping Rust-only spellings makes the generic grader misclassify them.
  Date/Author: 2026-08-13 / Codex
- Decision: Preserve broad census membership, but exclude declaration names before the bounded sample heap and filter same-file Rust evidence by `RustReferenceNamespace`. When the focused token is the terminal of a scoped path, also require the candidate's indexed parent to match the owner resolved from that structured path.
  Rationale: This removes non-reference probes without weakening inverse-membership accounting, fixes bare `Ok`/`Err` through namespace facts, prevents a scoped token from borrowing evidence from an unrelated owner, and leaves bare wildcard-imported variants visible as genuine gaps.
  Date/Author: 2026-08-13 / Codex
- Decision: Keep only Rust `Ambiguous` census outcomes inconclusive in this issue.
  Rationale: The preserved witness is an honest cfg ambiguity, while other languages have known actionable ambiguity defects. A global rule would hide those defects.
  Date/Author: 2026-08-13 / Codex
- Decision: Exclude parser-classified Rust pattern binders from census probes unless the exact spelling has a same-file declaration whose source AST node is an enum variant.
  Rationale: Pattern binders are declaration roles, but a parent-owner heuristic also admits named fields inside enum variants. Inspecting the declaration node itself is the smallest structured rule that removes local binder noise while keeping #2032 visible.
  Date/Author: 2026-08-13 / Codex
- Decision: Preserve the legacy same-file-name evidence for identifiers directly inside `token_tree` and `token_repetition` nodes.
  Rationale: Their namespace and binding roles are deliberately unresolved under #1895. Applying the new namespace filter there would silently demote that design issue instead of fixing it.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation is complete and locally validated. The new integration test passes, as do 26 existing reference-differential tests, the affected symbol diagnostic test, all 53 `brokk-bifrost-rust` unit tests, `-D warnings` clippy for both `brokk-bifrost-rust` and `brokk-bifrost-analysis`, the workspace dependency checker, formatting, and `git diff --check`.

The final featureless release replay audited all 39 serde-json Rust files with no file errors or inverse-precision findings. Joining the original 314 occurrence coordinates against the new report produced these exact results:

- 29 item declaration names are absent from the sample.
- 80 true local/binder rows are corrected: 56 binder declarations are absent and 24 proven local references are inconclusive with no tier.
- 42 lifetime rows are inconclusive with canonical `local_variable_reference` diagnostics and no tier.
- 107 non-macro `Ok`/`Err` value sites are tier 3 and inconclusive rather than borrowing type-alias evidence.
- The eight-target cfg-dependent `N` remains ambiguous, inconclusive, and ungraded.
- Four unit wildcard variants remain tier-2 Missing; the two tuple wildcard variants remain in the other actionable residue, preserving all six #2032 witnesses.
- The macro-token residue remains visible for #1895. One token-tree occurrence is now inconclusive because the lexical index independently proves it is the local `ser` binding; the remaining 26 macro rows stay actionable.

The release report is `/tmp/issue-2036-serde-final-2.jsonl`; the coordinate join is `/tmp/issue-2036-final-comparison-2.tsv`. The implementation was committed as `c8a8dce74`, published via merge head `b42e383df`, documented in issue comment `#issuecomment-5286973760`, and #2036 was closed as completed.

## Context and Orientation

`src/reference_differential/mod.rs` implements the corpus runner. A census probe begins with every structured identifier-like range, removes known declaration sites, runs forward definition lookup, and grades unresolved results. Tier 1 means a bare or self-member call has same-file declaration evidence; tier 2 means a weaker site has same-file evidence; tier 3 has no such evidence and remains exploratory. The current `same_file_names: HashSet<String>` loses both Rust namespaces and owners.

`crates/bifrost-analysis/src/analyzer/reference_candidates.rs` owns the reusable identifier frontiers and language-specific exclusion helpers exposed to the root runner. `crates/bifrost-rust/src/graph/ast.rs` already defines `is_rust_declaration_name` and `rust_reference_namespace` from tree-sitter nodes. Rust has separate type, value, macro, and module namespaces; a same spelling in a different namespace is not binding evidence.

`crates/bifrost-analysis/src/analyzer/usages/get_definition/mod.rs` defines canonical diagnostic constants and `is_adjudicated_answer_diagnostic_kind`. `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs` currently emits Rust-specific strings including `local_binding`, `local_lifetime`, `local_receiver`, `local_type_parameter`, and `declaration_site`. The differential correctly treats only the canonical cross-language constants as answered.

The behavioral regression belongs in a new `tests/suite_semantic/issue_2036_rust_census_grading.rs` module registered in `tests/suite_semantic/main.rs`. It must use `InlineTestProject`, run a census-seeded `ReferenceDifferentialConfig`, and inspect exact source offsets.

## Plan of Work

First add the integration fixture. Include a function, constant, and type alias declaration and assert those name ranges never become sampled sites. Include a lifetime and ordinary local use and assert they carry the canonical local diagnostic with no tier. Include a value call named `Ok` and an unrelated associated type alias named `Ok`; assert the call receives tier 3 rather than actionable same-file evidence. Include a scoped value reference whose terminal collides with a declaration beneath another owner and assert it also lacks evidence. Include cfg-alternative same-name type declarations and assert an ambiguous forward answer remains inconclusive. Finally include a local enum imported with `use Kind::*` and a bare variant pattern; if forward resolution still exhibits #2032, assert that the site remains sampled and actionable rather than disappearing under this grading change.

Then expose a public `rust_is_declaration_name` wrapper from `crates/bifrost-analysis/src/analyzer/reference_candidates.rs`, backed directly by the Rust language helper. Use it in `collect_sampled_sites` beside the existing Rust field-declaration exclusion, incrementing the existing declaration exclusion count.

In `get_definition/rust.rs`, import the canonical local and declaration diagnostic constants from the parent module and replace the Rust-only spellings at every site whose message proves a declaration or lexical binding. Update existing symbol tests that intentionally pin the old diagnostic spelling.

In `classify_census_gaps`, retain the actual same-file CodeUnits in addition to the terminal-name set. For Rust only, find the exact named node for the sampled range and use `rust_reference_namespace` to accept compatible declaration kinds: functions and non-alias fields for values, classes and type aliases for types, macros for macros, and modules/classes/type aliases for path prefixes. If the node is the `name` field of a scoped identifier or scoped type identifier, resolve its `path` through the file's forward Rust reference context and require the candidate's structural parent FQN to equal that resolved owner. For bare sites, namespace compatibility is sufficient; this intentionally keeps #2032's bare wildcard-variant witnesses actionable. Leave every other language's grading unchanged.

Finally make `census_gap_is_gradable` language-aware so Rust ambiguous outcomes retain `tier = None` and `Inconclusive`, while other languages preserve current behavior.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`.

Create the test and register it, then run it before production changes to confirm the intended assertions fail:

    cargo test --test suite_semantic -- issue_2036_rust_census_grading:: --nocapture

Implement the production changes with `apply_patch`, format, and rerun:

    cargo fmt --all
    cargo test --test suite_semantic -- issue_2036_rust_census_grading:: --nocapture
    cargo test --test suite_symbols -- rust_self_field_receiver_focus_reports_local_receiver_instead_of_owner_or_field_type --exact
    cargo test --test suite_semantic -- reference_differential:: --nocapture
    cargo clippy -p brokk-bifrost-rust --all-targets -- -D warnings
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Build the featureless release runner and replay representative preserved serde-json sites with `--cache-mode ephemeral`: one lifetime, one `Err` call, one `Err` tuple pattern, the cfg-ambiguous `N`, and one #2032 wildcard variant. The lifetime must become adjudicated with no tier, `Err` must be tier 3 and inconclusive, cfg `N` must be inconclusive with no tier, and the wildcard variant must remain actionable until #2032 is fixed. Then run the complete serde-json census and compare occurrence keys: all 29 declaration, 80 local, 42 lifetime, and 107 `Ok`/`Err` grading artifacts should clear without removing the six #2032 witnesses.

## Validation and Acceptance

The new integration test must fail before production edits and pass afterward. It is accepted when declaration names are absent from `report.sites`; local and lifetime sites have `local_variable_reference`, `tier == None`, and `Inconclusive`; a value `Ok` cannot borrow evidence from a same-file associated type alias; a scoped terminal cannot borrow evidence from another owner; a Rust ambiguous result has no tier; and the bare wildcard-imported enum variant remains a sampled actionable result if unresolved.

Focused Rust and analysis clippy must pass without warnings, and the workspace dependency checker must remain green. Exact corpus output must match the behavior above. No NLP or Python feature is relevant, so validation remains featureless.

## Idempotence and Recovery

All edits and test commands are repeatable. The corpus replays use ephemeral cache mode and write only under `/tmp`, so they do not modify the pinned repository. If a concurrent master update arrives before push, fetch and merge it into the current branch; do not rebase. Stage only the implementation, test, and this plan.

## Artifacts and Notes

The preserved input is `/mnt/optane/tmp/bifrost-fird/final-63a1912a/inputs/rust/serde-rs__json` at commit `827a315bf2198558f0325b07bcc1e2cd973aba2f`. The focused historical ledger is `/mnt/optane/tmp/bifrost-fird/final-63a1912a/smoke-rust-serde-json-ledger.jsonl` with 314 rows. Representative keys include lifetime `f72b9f3d5042461b448cb0cd6376020ae665872751797a021cae55a95cd66281`, `Err` call `7eb3858893482620f865f366046c13a544fd472839689012d82deeec1f1cfb5d`, and cfg ambiguity `dda07d11538664110660b1f53697371479e0d28817ebc11e0607dc27a8debbbc`.

## Interfaces and Dependencies

No dependency changes are required. At completion, `crates/bifrost-analysis/src/analyzer/reference_candidates.rs` must publicly expose a Rust declaration-name predicate backed by `brokk_bifrost_rust::graph::ast::is_rust_declaration_name`. The resolver must emit only existing canonical diagnostic constants. The runner may use the existing public `RustAnalyzer::forward_reference_context_of`, `RustReferenceContext` resolution methods, `RustReferenceNamespace`, `IAnalyzer::parent_of`, and `RustAnalyzer::is_type_alias`; it must not parse paths with string splitting or add a text-search fallback.

Plan revision note (2026-08-13): Initial self-contained plan created after issue, code, and preserved-ledger audit. The design uses existing Rust AST and reference-context facts so the fix corrects grading evidence rather than hiding unresolved sites.

Plan revision note (2026-08-13 22:03Z): Updated after implementation and final corpus validation. Added the enum-variant declaration-node distinction, the macro-token preservation decision, exact test results, and the historical occurrence-key comparison required by the bounded sample heap.

Plan revision note (2026-08-13 22:35Z): Recorded the earlier commit, push, evidence comment, and issue closure after noticing the publication checkbox remained stale.
