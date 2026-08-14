# Fix PHP census probe roles and inverse membership (#2029)

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current while implementation proceeds.

## Purpose / Big Picture

The reference differential should stop presenting PHP namespace declaration segments and constant declaration names as definition probes. At the same time, inverse hits for a class used as a static-call scope and a method used through a nullsafe call must count as backed source references. After this change the census remains a maximal grammar inventory, probe selection excludes only structured declaration roles, and inverse precision uses its distinct membership frontier to retain the valid PHP references.

## Progress

- [x] (2026-08-13) Reproduced the two independent seams: declaration names pass the sampled-site filter, while inverse membership does not project the exact static-scope and nullsafe-member token ranges.
- [x] (2026-08-13) Added shared PHP AST role helpers and focused frontier tests.
- [x] (2026-08-13) Routed PHP declaration-role filtering through census probe selection without changing raw membership.
- [x] (2026-08-13) Extended PHP membership projection for static scopes and nullsafe member names, preserving the per-file cap.
- [x] (2026-08-13) Added an end-to-end `InlineTestProject` reference-differential regression with positive and near-miss assertions.
- [x] (2026-08-13) Passed focused featureless tests, dependency check, fmt, and clippy for touched crates.
- [x] (2026-08-13) Replayed the two exact Respect Validation witnesses and namespace/constant declaration controls.
- [x] (2026-08-13) Committed, merged current `origin/master`, passed post-merge focused tests, pushed as `a3aeae37`, commented with evidence, and closed #2029.

## Surprises & Discoveries

- The repository already separates `census_identifier_ranges` from `census_membership_identifier_ranges`; #2029 should extend that split rather than introduce another census mode.
- Analyzer declaration name ranges are insufficient for namespace segments because namespaces are scope facts rather than indexed CodeUnits. Constant declaration grammar ranges can also differ from the indexed declaration range, so both need AST-role filtering.
- In the Respect-shaped recovery, tree-sitter retains `Id` in an ERROR child immediately before the structured `::` separator, while the surrounding `scoped_call_expression.scope` field is incorrectly attached to `$this`. The membership helper therefore accepts the exact ERROR child only when it is the structured child immediately preceding `::`; arbitrary ERROR leaves remain excluded.

## Decision Log

- Decision: keep declaration tokens in the raw grammar census and remove them only from probe eligibility.
  Rationale: namespace declarations still supply scope facts, and raw candidate accounting should remain stable and honest.
- Decision: put reusable PHP role interpretation in `brokk-bifrost-php`, then call it from analysis.
  Rationale: the language crate already owns PHP declaration/reference semantics, and duplicating parent-kind lists in the runner would drift.
- Decision: keep membership overflow fail-closed for the whole file.
  Rationale: a partial membership set must never manufacture inverse-precision findings.

## Context and Orientation

`crates/bifrost-analysis/src/analyzer/reference_candidates.rs` owns the grammar-derived probe and membership frontiers. `src/reference_differential/mod.rs::collect_sampled_sites` filters declaration sites before bounded sampling, while `collect_census_membership` builds exact `(start, end, text)` membership used by inverse precision. PHP AST interpretation belongs in `crates/bifrost-php/src/graph/resolver.rs` or a nearby shared syntax module. Integration coverage belongs in a new member of `tests/suite_semantic` and must use `InlineTestProject`.

## Plan of Work

First add language-owned helpers that identify a PHP namespace declaration segment and a PHP `const_element` declaration name using tree-sitter fields/parents. Add a membership projection that returns exact ranges for the structured static-call scope and nullsafe member name that the inverse graph reports. Then use the declaration helper in `collect_sampled_sites`, after raw candidate accounting but before exact-site selection, and merge the additional membership ranges under the existing cap. Finally add unit and end-to-end tests proving declarations are excluded, ordinary namespace/type/member references remain, the two inverse hits are backed, and dynamic/static near misses are not broadened.

## Concrete Steps

Run focused tests through the existing workspace target unless isolation becomes necessary:

    cargo test -p brokk-bifrost-analysis reference_candidates::tests::php_
    cargo test --test suite_semantic -- issue_2029_php_census_roles
    cargo fmt --all
    cargo clippy -p brokk-bifrost-php -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs

Use `bifrost_reference_differential --cache-mode ephemeral` for exact replay so corpus workspaces are not mutated.

## Validation and Acceptance

The integration fixture must show that namespace declaration segments and class/module constant declaration names do not appear in `report.sites`, while the raw structured-candidate count still sees them and `declaration_sites_excluded` accounts for them. It must also produce zero inverse-precision findings for `Id` in `Id::fromValidator(...)` and `withInput` in `$this->adjacent?->withInput(...)`. Ordinary type references, static call names, normal/nullsafe member calls, and constant value uses remain eligible. Dynamic names and declaration/import roles must not be added to membership by a text fallback.

The exact Respect Validation replays at `src/Result.php` bytes `3379..3381` and `6081..6090` must no longer appear as inverse-precision findings. Representative namespace and constant declaration exact sites must be rejected as structured non-declaration references.

## Idempotence and Recovery

All edits and tests are repeatable. Exact corpus replays use ephemeral cache mode. If a candidate cap is exceeded, preserve the current `None`/unavailable file membership rather than a partial set. Stage and commit only files listed by this plan; unrelated shared-worktree edits remain untouched.

## Outcomes & Retrospective

The implementation now preserves the maximal raw census while excluding PHP namespace and constant binders before sampling. PHP membership descends ERROR subtrees only for exact nullsafe member-name fields and static-call scope envelopes. The Respect `Id` replay (`9db9054da565924ae3001efbafcda2c810a8ed72bc1152d96bbcd845c804dad7`) and `withInput` replay (`48993ff03957b4dbe8654ca3368e0995546cfa51e9b189949ab9ebf85e9a5ec5`) each report zero actionable and zero inverse-precision findings. Exact namespace and constant declaration replays now stop with `exact site did not match a structured non-declaration reference`, as intended. The fix was pushed as `a3aeae37`; the evidence comment is https://github.com/BrokkAi/bifrost/issues/2029#issuecomment-5284237791 and the issue is closed.
