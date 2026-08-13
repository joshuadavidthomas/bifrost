# Unify C++ Macro-Wrapped Namespace Identities

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must stay current during the work.

Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

C++ projects such as fmt open a namespace with macros such as `FMT_BEGIN_NAMESPACE`. Tree-sitter can represent the same logical namespace differently in different files. Bifrost then gives one template or class more than one fully qualified name. After this change, declarations under the same macro-wrapped namespace will use one structured namespace identity. Forward navigation and inverse usage will agree on that identity.

The focused regression will define the same macro-wrapped namespace in two headers. One header will declare a primary template. The other will use a dependent nested alias from that template. The analyzer must index both files under one namespace and resolve the alias reference to the primary template. An exact fmt FIRD replay must also contain the covering inverse hit.

## Progress

- [x] (2026-08-12 22:10Z) Read issue 1967 and locate the existing macro-namespace recovery paths.
- [x] (2026-08-13 04:01Z) Replayed the exact fmt `carrier_uint` site at fmt commit `6dac6cad052d6593f5fa07529d912f3bd0d6bb11`. The old result targeted `dragonbox.float_info` and was missing.
- [x] (2026-08-13 04:25Z) Added a cross-file inline regression for the primary-template identity and complete inverse result.
- [x] (2026-08-13 04:39Z) Extended the structured sentinel reparse through the following flattened namespace.
- [x] (2026-08-13 04:58Z) Ran focused inverse and namespace tests, formatting, and targeted Clippy checks.
- [x] (2026-08-13 04:48Z) Replayed the exact fmt witness. It targets `detail::dragonbox.float_info`, has no missing result, and contains the exact inverse hit.
- [ ] Commit, pull, push, and close issue 1967.

## Surprises & Discoveries

- Observation: The resolver already has target-guided macro-namespace recovery in `crates/bifrost-cpp/src/graph/resolver.rs`, and the inverse scanner has orphaned-namespace recovery in `crates/bifrost-cpp/src/graph/extractor.rs`.
  Evidence: Searches find `flattened_macro_namespace_components`, `recovered_macro_namespace_name`, and `collect_orphaned_namespace_envelopes`. Issue 1967 still reports different indexed names across files, so a later resolver fallback cannot correct the stored identity.

- Observation: The exact `format.h` syntax tree makes `FMT_END_EXPORT namespace detail { ... }` one false function. It then flattens the next `namespace detail` into the same sibling list.
  Evidence: The tree has a `function_definition` at lines 963 through 967. Its next siblings are the comment, `namespace`, `detail`, and `{` tokens at line 969. `float_info` therefore lost the `detail` owner before this change.

- Observation: The first exact replay took 49 seconds and reported `dragonbox.float_info` with classification `missing`. The fixed replay took 50.9 seconds and reported `detail::dragonbox.float_info` with classification `unproven` and an exact inverse hit.
  Evidence: `/tmp/issue1967-before-e72fdb20.jsonl` and `/tmp/issue1967-after-e72fdb20.jsonl` use the same fmt commit and exact byte range. The remaining `unproven` grade describes the dependent alias proof. It is not a missing inverse result.

## Decision Log

- Decision: Fix declaration ownership before indexed identity construction. Do not add a target-guided inverse name fallback.
  Rationale: Issue 1967 shows one logical template stored under incompatible namespace names. A later fallback would preserve the bad index and could join unrelated suffix matches.
  Date/Author: 2026-08-12 / Codex

- Decision: Use the shared inline project harness for the reduced cross-file test.
  Rationale: The test needs only a few headers and source files. `InlineTestProject` is the required default for this shape.
  Date/Author: 2026-08-12 / Codex

- Decision: Extend the existing sentinel reparse from a structured following `namespace` token. Let tree-sitter supply the namespace end.
  Rationale: The damaged tree keeps the namespace keyword, name, and opening brace as sibling nodes. A bounded reparse restores the namespace without a source-text brace scan.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The declaration index now stores both fmt symbols under `detail::dragonbox`. The focused cross-file test confirms this identity and confirms a covering authoritative inverse hit.

The exact fmt FIRD replay now resolves the witness to `detail::dragonbox.float_info`. It reports zero missing sites and retains the exact `carrier_uint` range as unproven. This is the correct bounded result for the dependent alias reference.

Focused tests passed for issue 1967, issue 1825, macro field terminators, and recovered export classes. Formatting passed. Targeted Clippy passed for `brokk-bifrost-cpp` and the `suite_usages` test target.

## Context and Orientation

`crates/bifrost-cpp/src/declarations.rs` walks the C++ tree-sitter syntax tree and creates `CodeUnit` declarations. A `CodeUnit` is Bifrost's indexed symbol record. Its fully qualified name contains the namespace and class ownership used by navigation and usage analysis.

Unknown namespace-opening macros can damage the syntax tree. `crates/bifrost-cpp/src/graph/resolver.rs` contains structured helpers that recover namespace components from these damaged trees. `crates/bifrost-cpp/src/graph/extractor.rs` uses related recovery during inverse usage scans. These paths can compensate during a query, but issue 1967 shows that declaration collection itself can still store different owners in different files.

The public behavior tests use `tests/common/inline_project.rs`. Existing macro namespace tests are in `tests/suite_analyzers/cpp_analyzer_test.rs`, `tests/suite_symbols/get_definition_test.rs`, and `tests/suite_issues/issue_1825_cpp_macro_namespace_callable.rs`. The new test must use one of the existing suite harnesses and add only one module entry if it needs a new issue file.

## Plan of Work

First, acquire the current fmt source and reproduce at least the `carrier_uint` witness from issue 1967. Record the fmt revision and the exact current result. Inspect the declarations for `float_info`, `cache_accessor`, and `carrier_uint` to identify the first point where their namespace owners differ.

Second, reduce that syntax to a small cross-file inline project. Preserve the macro opening and closing tokens, the nested ordinary namespaces, the primary template, and the dependent nested alias. Assert the indexed fully qualified names before asserting navigation and inverse usage. Use the exact fmt replay as the before-change failure proof because the smaller parser tree retains a normal namespace node.

Third, update declaration collection in `crates/bifrost-cpp/src/declarations.rs`. Reuse or move a structured macro-namespace helper when necessary. Do not parse source text with regular expressions, splitting, or delimiter scans. Apply the recovered namespace components when the declaration scope is formed, before `CodeUnit` construction. Keep ordinary namespaces, real functions, export macros, and unrelated all-caps tokens unchanged.

Fourth, run the new regression and the existing macro namespace tests. Replay the real fmt witnesses. Run `cargo fmt --all -- --check` and targeted Clippy for each changed crate and test suite. Commit only the changed files and this plan. Pull `origin/master`, push the current branch to `origin/master`, and close issue 1967 with exact evidence.

## Concrete Steps

Run all commands from `/mnt/optane/bifrost-fird`.

Acquire or locate fmt, then record its revision:

    git -C /tmp/fird-issue1967-fmt rev-parse HEAD

Build and run the exact FIRD witness with an ephemeral cache and a unique output path:

    cargo build --bin bifrost_reference_differential
    target/debug/bifrost_reference_differential run-repo --root /tmp/fird-issue1967-fmt --language cpp --cache-mode ephemeral --probe-seed census --tiers 1 --path include/fmt/format-inl.h --start-byte 12059 --end-byte 12071 --output /tmp/issue1967-before.jsonl

Run the focused test after it exists. The exact test name will be recorded here after reduction.

Run the required local gates:

    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-cpp --all-targets -- -D warnings
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings

## Validation and Acceptance

The reduced test must pass after the implementation. The exact fmt replay supplies the required before-change failure because tree-sitter repairs the smaller test syntax differently. The test must prove these behaviors:

The primary template and its dependent nested alias have the same recovered namespace prefix across files. Forward lookup returns the intended indexed symbol. Complete inverse usage includes the exact source range. An ordinary nested namespace without sentinel macros keeps its current name. An unrelated all-caps token does not create a namespace.

The real fmt witness must complete. It must not report a missing inverse hit caused by `dragonbox` versus `detail::dragonbox`. The issue comment must state the exact fmt revision, Bifrost commit, forward result, inverse result, and focused test commands.

## Idempotence and Recovery

All focused tests and FIRD commands are safe to repeat. Use unique `/tmp` output names because the differential runner resumes an existing output. If a pull creates a conflict, stop the push, resolve only files changed by this plan, rerun focused checks, and commit the merge. Do not remove unrelated worktree changes.

## Artifacts and Notes

Issue 1967 records these initial incompatible identities:

    include/fmt/format.h: dragonbox.float_info
    include/fmt/format-inl.h: detail::dragonbox.cache_accessor<double>$carrier_uint

The exact first witness is `include/fmt/format-inl.h` bytes `12059..12071`, token `carrier_uint`.

## Interfaces and Dependencies

Do not add a crate or dependency. Reuse tree-sitter `Node` fields and existing C++ namespace recovery helpers. The final implementation must place the shared structured interpretation in one C++ module if declaration collection and resolver code both need it.

Plan revision note, 2026-08-12: Created the plan after issue triage. It records the required source-level identity fix, real replay, reduced regression, and validation gates.

Plan revision note, 2026-08-13: Recorded the flattened namespace root cause, bounded reparse decision, exact before/after FIRD results, and completed local checks.
