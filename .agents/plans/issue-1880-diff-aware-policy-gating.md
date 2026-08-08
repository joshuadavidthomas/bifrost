# Diff-aware policy gating (issue #1880)

## Purpose

Today `bifrost --policy-pack bifrost.code-smells` fails a repository for every finding, including findings that existed long before the change under review. A continuous-integration gate built on that behavior blocks every pull request against a legacy codebase until someone repairs or suppresses all historical debt. Every incumbent product treats "fail only on what this change introduced" as the default CI contract.

After this work, a user can run:

    bifrost --root . --policy-pack bifrost.code-smells --format sarif --output out.sarif --diff-base origin/master

and the run evaluates the same policies twice — once against the committed content of `origin/master` (the "base") and once against the working tree (the "head") — joins the findings by their stable identities, marks each head finding as `new` or `persisting`, records which base findings are `fixed`, and computes the exit status from the new findings only. A pull request that introduces one finding into a repository with two hundred pre-existing ones fails with exactly that one finding gating.

"Finding identity" means the 32-byte digest `PolicyFindingId` that Bifrost already computes for every finding. It hashes only content-derived facts (workspace-relative path, semantic owner key, a SHA-256 of the matched source bytes, a small per-slice ordinal) under the domain string `bifrost-policy-finding/v1`. It contains no absolute path, revision, timestamp, or run-local handle, so the same finding in unchanged content produces the same identity at both revisions. This checkout-independence is the property the whole feature rests on.

## Orientation: the pieces this plan touches

The policy engine lives in `crates/bifrost-policy`. Its public entry points are in `src/coordinator.rs` (`evaluate_policy_inputs` and siblings, all funneling into `evaluate_policy_inputs_with_limits` at line ~622 and `evaluate_prepared_policy_inputs` at ~687). The CLI wrapper is `src/bin/bifrost.rs` (`run_policy_mode` at ~771). The MCP server wrapper is `crates/bifrost-mcp/src/searchtools_service.rs` (`RunPolicyParams` at ~649, `prepare_run_policy_with_cancellation` at ~3349, `execute_prepared_run_policy` at ~3548). The canonical report is `PolicyReportDocument` in `crates/bifrost-policy/src/report.rs:1599` (schema_version 3, a manual `Serialize` impl at ~1770 that names 11 fields). Renderers read only that document: `render/human.rs`, `render/sarif.rs`, and a serde pass-through for JSON in `render/mod.rs:83`.

Finding identity is `crates/bifrost-policy/src/finding_identity.rs` (`PolicyFindingId` at line 928; note that `src/identity.rs` is the *policy-document* identity, a different thing). Suppressions already join stored records to findings by `(policy_id, finding_id)` in `apply_policy_suppressions`, `crates/bifrost-policy/src/coordinator.rs:1322-1390`, and attach a per-finding decision object. That function is the template for the diff join: same key, same attachment pattern, same top-level review vector.

Evaluating a second revision already has production precedent: `crates/bifrost-analysis/src/diff_analysis.rs` resolves a revision with `repo.revparse_single`, exports its blobs into a private temporary directory (`RevisionImage::materialize`, line ~739, using `RevisionTempDir` at ~803 with 0700 permissions and cleanup on drop), builds a `FileSetProject` over the exported files, and analyzes it with `WorkspaceAnalyzer::build_ephemeral`. This plan reuses that mechanism rather than inventing a second one.

One fact constrains what this plan may promise: the CLI policy path builds its analyzer with an in-memory store (`WorkspaceAnalyzer::build` -> `AnalyzerStore::open_in_memory`, see `crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs:242`). The `.bifrost/cache` blob-OID database is only attached on the `build_persisted` path used by MCP and LSP sessions. Therefore v1 diff mode performs a full second in-memory analysis of the base image. That is correct, just not cached. Making the base evaluation warm through the shared blob-OID store is an explicit non-goal here and a follow-up performance issue; do not silently switch the coordinator to `build_persisted` as a side effect of this plan.

## Semantics (decided; do not relitigate without recording a decision-log entry)

The join key is `(policy_id, PolicyFindingId)`, strong identities only. A head finding whose identity also appears in the base is `persisting`; otherwise it is `new`. A base identity absent from the head is `fixed` and is recorded in a top-level summary, not as a phantom finding object.

Weak-identity findings never join across revisions (their keys are snapshot-local by construction). A weak head finding classifies as `new` and its diff decision records `weak_identity: true`. Weak findings already force their run to carry `PolicyIncompleteReason::StableAnchorUnavailable`, so reliability handling needs no new rule for them.

The gate change is one predicate. `threshold_exceeded` at `coordinator.rs:1141-1145` currently counts findings with no suppression and no scope decision at or above `fail_on`. In diff mode it additionally requires the finding's diff disposition to be `new`. Suppressions and scope still apply first: a suppressed new finding does not gate, exactly as a suppressed finding does not gate today.

An unreliable base evaluation never silently passes the gate. If the base evaluation would exit with status 2 by the same rules as any policy run (`report_exit_status` conditions: termination, report diagnostics, non-reliable or non-exhaustive completions), diff mode degrades to full gating: every head finding gates as if `--diff-base` had not been given, a report diagnostic with code `DiffBaseUnreliable` states the degradation and embeds the base run's incomplete reasons, and the top-level diff summary records `degraded: true`. Degrading rather than failing keeps a broken base from masking new findings, and the loud diagnostic keeps it from being mistaken for a clean diff run. The head run's own reliability is judged exactly as today.

Two known identity limitations are accepted and documented rather than solved here. A pure file rename re-keys every finding in the file (the path is part of the digest), producing a fixed-plus-new pair. Duplicate identical source slices under one owner are distinguished by an ordinal, so inserting a duplicate above an existing one can shift ordinals and misclassify one pair. Both are edge cases Sonar-class tools also get wrong; note them in the docs page.

The policy semantic hash is deliberately not part of the join: editing unrelated policy metadata between base and head does not orphan the join, because `PolicyFindingId` hashes `policy_id` but not `policy_hash`.

The `--diff-base` value is any revision string `git rev-parse` would accept, resolved with `revparse_single` and peeled to a commit. The CLI does not compute merge-bases; a pull-request workflow passes the merge-base explicitly (GitHub provides it, or `git merge-base HEAD origin/main`). If the workspace root is not inside a Git repository, or the revision does not resolve, the run fails with a diagnostic and exit status 2 — an unresolvable base is an unreliable diff request, never a silent full run.

## Milestone 1: baseline evaluation and the join inside the coordinator

At the end of this milestone, `PolicyEvaluationOptions` carries an optional diff base and `evaluate_prepared_policy_inputs` produces classified findings, demonstrable through a Rust test that evaluates two in-memory revisions.

Add to `PolicyEvaluationOptions` (`coordinator.rs:98`) a `with_diff_base(revision: String)` builder alongside `with_suppressions`/`with_scope`. Thread it into `evaluate_prepared_policy_inputs`. When present, after the head evaluation produces its runs but before suppressions attach: open the repository enclosing the workspace root (`git2::Repository::discover`), resolve the revision, materialize the base image with the `diff_analysis` machinery (`RevisionImage::materialize` and `Snapshot::resolve` need their visibility raised from module-private to `pub` in `crates/bifrost-analysis/src/diff_analysis.rs`; they are already checkout-safe), build the base analyzer with `FileSetProject` plus `WorkspaceAnalyzer::build_ephemeral`, and evaluate the identical policy inputs and options (minus the diff base, minus suppressions and scope — the base needs raw identities only) through the same internal evaluation function the head used. Collect `HashSet<(PolicyId, PolicyFindingId)>` from the base runs plus the base report's reliability verdict, then drop the base image.

Define next to the suppression types: `PolicyFindingDiff { disposition: FindingDiffDisposition, weak_identity: bool }` with `FindingDiffDisposition::{New, Persisting}` (serde lowercase). Do not reuse the name `FindingClassification`; that name is already the CWE-style taxonomy in `crates/bifrost-policy/src/classification.rs:33`. On `PolicyFinding` (finding.rs, after the `scope` field at ~1915): `diff: Option<PolicyFindingDiff>`, `None` in `try_new` (~2064), with `diff()`, `attach_diff()`, and `clear_diff()` accessors mirroring `attach_scope` (~2166), a `#[serde(skip_serializing_if = "Option::is_none")]` attribute, and a `RetainedSize` contribution (finding.rs:2302-2319) — the retention budgets are byte-exact and an unaccounted field under-counts silently.

The join itself is a sibling of `apply_policy_suppressions`: iterate head runs, look up each finding's `(policy_id, id)` in the base set, attach the decision. Base identities not consumed by the join become the fixed list. Since `PolicyRun` keeps findings sorted by id (finding.rs:2444-2447), the per-run join can be a set lookup without re-sorting.

Acceptance: a unit test in the policy crate builds two small in-memory workspaces (one file, one finding; then the same file plus a second offending line), runs the internal join, and asserts one `persisting`, one `new`, and — reversing the direction — one fixed entry. Run with:

    cargo test -p brokk-bifrost-policy

## Milestone 2: report document, gate, and renderers

At the end of this milestone the classified result is visible in all three formats and the exit status obeys the new rule, demonstrable by running the CLI from milestone 3 or the report-level tests in `tests/suite_bench_policy/`.

Add a top-level `diff: Option<PolicyDiffReview>` to `PolicyReportDocument`: the requested revision string, the resolved commit id, `degraded: bool`, counts of new/persisting/fixed, and the fixed entries as `(policy_id, finding_id)` pairs (bounded; reuse the report's existing truncation discipline if the list is large). The manual `Serialize` impl at report.rs:1770 names 11 fields — it becomes 12, serialized only when present for JSON compatibility. Add a `validate_diff_joins` sibling next to `validate_suppression_joins` (report.rs:1702-1704): every `diff` decision on a finding must be consistent with the presence of the top-level review, exactly as suppression reviews are validated. `SCHEMA_VERSION` stays 3: the addition is optional-field additive, and the repository rule is that compatible additions do not mint schema versions. The assertion at `tests/suite_bench_policy/policy_rendering.rs:268` therefore stays untouched.

Change the gate at `coordinator.rs:1141-1145`: when diff mode is active and not degraded, a finding only counts toward `threshold_exceeded` if its disposition is `New`. `report_exit_status` (coordinator.rs:1869) itself is unchanged; the unreliable precedence rules already do the right thing once the degradation diagnostic is a report diagnostic.

SARIF (`render/sarif.rs`): set the standard `baselineState` field on each result when diff mode is active — `"new"` for new, `"unchanged"` for persisting; fixed findings are not emitted as results, so `"absent"` is not produced. Also add `bifrost.diffDisposition` to `SarifResultProperties` (~486) and a `bifrost.diffBaseline` object to the run-level property bag (~921-942) carrying the review summary. Fingerprint emission is untouched — `partialFingerprints["bifrostFinding/v1"]` is already the same digest the join uses.

Human renderer (`render/human.rs`): the concise-mode filter at line ~135 (`suppression().is_none() && scope().is_none()`) additionally hides persisting findings when diff mode is active unless `--verbose`; the verbose `write_finding` gains a diff stanza next to the suppression stanza at ~254; `write_summary` (~2307) reports "N new, M persisting, K fixed against <base>" and the degraded warning when set.

Acceptance: `cargo test --test suite_bench_policy -- policy_rendering:: policy_sarif_rendering::` passes with new cases asserting the three formats agree on dispositions and counts.

## Milestone 3: CLI flag and MCP parameter

At the end of this milestone the feature is reachable end to end from both surfaces.

CLI (`src/bin/bifrost.rs`): `--diff-base <rev>`. It must be added in four places or it will not even parse as a policy invocation: the `has_policy_syntax` literal allowlist (~90-110), the `option_requires_value` list (~122), the exclusivity block (~479-515, including the `--list-policies` incompatibility list), and the arg loop itself (~323-436) using the same seen-twice guard idiom as `--evaluation-date`. `run_policy_mode` (~771) already has 11 parameters under `allow(too_many_arguments)`; fold the new state into a small params struct for the policy options in the same file rather than adding a twelfth argument — that is a mechanical refactor of one call site. Document the flag in the `OPTIONS` help block (~1073-1141). An unresolvable revision or non-git root maps to the existing early-return path that yields `POLICY_EXIT_UNRELIABLE`.

MCP (`crates/bifrost-mcp`): add `diff_base: Option<String>` to `RunPolicyParams` (searchtools_service.rs:649-663), thread it through `prepare_run_policy_with_cancellation` into the options (~3494-3497), and update the `run_policy` tool schema in `mcp_extended.rs:851-937` — the schema declares `additionalProperties: false`, so an undeclared parameter is rejected, and the schema test at mcp_extended.rs:1299 pins the shape. Because the base analyzer is built inside the coordinator, the MCP path needs no second runtime; the borrowed head snapshot is used exactly as before.

CI action (`.github/actions/policy-scan/action.yml`): a `diff-base` input, empty by default, appended as `--diff-base <value>` when non-empty; the docs page `docs/src/content/docs/ci-github-actions.md` gains the pull-request recipe passing `${{ github.event.pull_request.base.sha }}`.

Acceptance: from a git repository with one committed finding and one uncommitted new one,

    bifrost --root . --policy-pack bifrost.code-smells --diff-base HEAD --format json

exits 1, the JSON report shows one `new`, one `persisting`, and gating counts only the new one; the same command without `--diff-base` exits 1 with both findings gating.

## Milestone 4: end-to-end tests and documentation

Tests live in `tests/suite_bench_policy/bifrost_policy_cli.rs` (module list registered in that suite's `main.rs`). The existing fixtures never create git repositories — `InlineTestProject::build` is a bare tempdir — so the diff tests add git setup. Model the shell-git sequence on `crates/bifrost-nlp/src/store.rs:476-505` (init, config user, add, commit) or the `git2` style in `tests/suite_persistence/analyzer_sql_query_parity.rs:30` with its `commit_all` helper. The process harness to reuse is `fn bifrost(root)` / `run(root, args)` / `assert_status` at bifrost_policy_cli.rs:214-248.

Cover at minimum: new-vs-persisting classification and the exit-code difference with and without the flag; a suppressed new finding not gating; the degraded-base path (make the base evaluation unreliable, e.g. by pointing `--diff-base` at a commit whose content trips a policy-load diagnostic, and assert full gating plus the `DiffBaseUnreliable` diagnostic); an unresolvable revision exiting 2; the flag's arg-validation entries in the exclusivity table test at bifrost_policy_cli.rs:1632-1710; and cross-format agreement in the style of the suppression determinism test at line 565. For the rename limitation, one test documents current behavior (rename yields fixed-plus-new) so a future improvement changes a test deliberately rather than silently.

Documentation: `docs/src/content/docs/static-analysis-policies.md` gains a diff-gating section (that page is test-enforced by `policy_docs.rs`, so runnable examples must actually run); `docs/src/content/docs/cli.md` documents the flag; the CI page gains the PR recipe. State the two accepted identity limitations plainly.

Validation for the whole feature, from the repository root:

    cargo fmt
    cargo test --test suite_bench_policy
    cargo clippy --workspace --all-targets -- -D warnings

plus the full pre-push gate (`scripts/pre-push-gate.sh`) before pushing.

## Non-goals

Warm base evaluation through the persisted blob-OID store (follow-up performance issue; requires deciding whether the CLI policy path should write `.bifrost/cache` at all). Bulk baseline acceptance for scheduled full runs (#1881, a separate document kind). Rename-tracking or ordinal-stable identities (documented limitations). SARIF `baselineState: absent` phantom results.

## Decision log

2026-08-08: Base evaluation reuses `diff_analysis.rs` revision materialization (temp export + `FileSetProject` + `build_ephemeral`) instead of `git worktree add`, because it exists, is checkout-safe, and avoids mutating the user's worktree list. Consequence: no cache sharing in v1; accepted, since the CLI policy path has no persistent store today anyway.

2026-08-08 (implementation): Instead of raising `RevisionImage::materialize` and the `Snapshot` type to `pub`, `crates/bifrost-analysis/src/diff_analysis.rs` gained one composed public entry point, `export_revision(workspace_root, revision) -> RevisionExport`. Reason: `materialize` takes an explicit changed-path list, but the base policy evaluation needs the *entire* workspace subtree, so a tree walk had to be added anyway; composing it in `diff_analysis` keeps `git2` out of the policy crate and exposes the smallest possible surface (root, files, resolved commit id). `export_revision` also handles a workspace root that is a subdirectory of the repository work tree by descending to the matching subtree, so exported paths stay workspace-relative and identities join.

2026-08-08 (implementation): The base run's options are `PolicyEvaluationOptions::new(head evaluation date)` plus the head's `require_explicit_schema_versions` flag: no diff base (which would recurse), `fail_on` Never, and the head's suppression and scope configuration deliberately not forwarded. The conventional suppression/scope loaders still run against the base image; that is harmless for identity collection (attachments never change the identity set), and a base tree whose own committed suppression or scope document is invalid degrades loudly via the ordinary reliability rules. The degraded-base tests use exactly that vector (an invalid committed `.bifrost/suppressions.json` repaired in the working tree).

2026-08-08 (implementation): Clarification of the earlier degradation entry: because `report_exit_status` treats any report diagnostic as unreliable, a degraded diff run always exits 2 (never 1), whether or not the head has findings. That satisfies "an unreliable base never silently passes" and matches milestone 2's instruction that `report_exit_status` itself stays unchanged; the earlier sentence "only when the head is otherwise clean" was imprecise about the existing precedence and is superseded by this one.

2026-08-08 (implementation): When a diff base is requested but the head evaluation has no runnable policy (every input became a diagnostic), the base is not evaluated; the review records `degraded: true` with a diagnostic explaining why. The run is already unreliable in that state and there is nothing to classify.

2026-08-08 (implementation): `FindingDiffDisposition` and `PolicyFindingDiff` live in `crates/bifrost-policy/src/finding.rs` next to `PolicyFinding`'s suppression/scope attachment machinery rather than in `suppression.rs`; `PolicyDiffReview` and `PolicyDiffFixedFinding` live in `report.rs` next to the other review vectors. "Next to the suppression types" was read as proximity to the per-finding decision plumbing, which is what these files own.

2026-08-08 (implementation): Base identities are collected from the base run's *report* runs, which is safe because report-level finding omission always marks the run's completion non-reliable (`record_omitted_finding` -> `mark_inconclusive`), which makes the base exit 2 and degrades the diff. A base whose findings exceed the retention budget therefore can never silently misclassify persisting findings as new.

2026-08-08: Unreliable base degrades to full gating with a loud diagnostic rather than failing the run outright. Rationale: a broken base must not block detection of new findings, and must not be mistakable for a clean diff; the diagnostic makes the run itself exit 2 via the existing report-diagnostic rule only when the head is otherwise clean — findings still gate.

2026-08-08: Fixed findings are a top-level summary, not phantom finding objects, because `PolicyRun` invariants (sorted unique ids, retention accounting) are built around findings that exist in the evaluated snapshot.

2026-08-08: `SCHEMA_VERSION` stays 3 per the repository's additive-change rule.

## Progress

- [x] (2026-08-08) Milestone 1: options plumbing (`with_diff_base` + accessor + retained-size), base materialization (`export_revision` in `diff_analysis.rs`), baseline evaluation (`evaluate_policy_diff_baseline`), join (`apply_policy_diff`), gate predicate, degradation diagnostic, and four coordinator unit tests (join classification, git-backed gating incl. fixed, degraded base, unresolvable base / non-git root). `cargo test -p brokk-bifrost-policy`: 301 passed. Note: the report field, gate predicate, and `DiffBaseUnreliable` diagnostic from milestone 2's report/coordinator half were implemented together with milestone 1 because the join cannot be represented without them; milestone 2's remaining scope is the three renderers and their tests.
- [x] (2026-08-08) Milestone 2: SARIF renderer (`baselineState` new/unchanged on results, `bifrost.diffDisposition` result property, `bifrost.diffBaseline` run property, all absent in non-diff reports and schema-validated offline), human renderer (concise view hides persisting findings in non-degraded diff mode, verbose `diff:` stanza per finding, summary "N new, M persisting, K fixed against <base>" plus the degraded warning), and two cross-format suite tests (`policy_rendering::diff_mode_agrees_across_concise_verbose_and_json`, `policy_sarif_rendering::diff_mode_emits_baseline_state_and_run_level_diff_baseline`). Git fixture helpers moved to `tests/common/mod.rs` (`run_git`, `init_git_repo_with_identity`) for reuse by the milestone-4 CLI tests. `cargo test --test suite_bench_policy -- policy_rendering:: policy_sarif_rendering::`: 18 passed.
- [x] (2026-08-08) Milestone 3: `--diff-base` added to all four CLI parse sites (`has_policy_syntax`, `option_requires_value`, arg loop with the seen-twice guard, `--list-policies` exclusivity list) and documented in the OPTIONS help block; `run_policy_mode`'s 11 arguments folded into a `PolicyModeRequest` struct (the `allow(too_many_arguments)` is gone); MCP `RunPolicyParams.diff_base` with a 1..=256-byte bound (`MAX_RUN_POLICY_DIFF_BASE_BYTES`), threaded into the options, declared in the `run_policy` schema, and pinned by the schema test; the policy-scan action gained a `diff-base` input appended as `--diff-base <value>` when non-empty, and the CI docs page gained the PR recipe with `${{ github.event.pull_request.base.sha }}` plus the two identity limitations. Verified end to end against a scratch git repo: diff run exits 1 with only the new finding gating, unchanged worktree exits 0 with "0 new, 1 persisting", unresolvable revision exits 2, `--list-policies --diff-base` rejected.
- [ ] Milestone 4: git-backed CLI tests, degraded-base test, docs (policy page, CLI page, CI page).
- [ ] Full validation: suite_bench_policy green, featureless workspace clippy clean, pre-push gate before push.

## Surprises & Discoveries

- Observation: the milestone-1 join unit test uses two files (one shared, one added) rather than one file with a duplicated `target` function, because the match selector joins on the declared name and a same-file duplicate declaration would collide on FQN in the analyzer rather than produce a clean second finding.
  Evidence: `diff_join_classifies_new_persisting_and_fixed_findings` in `crates/bifrost-policy/src/coordinator.rs` asserts one persisting (`app.ts`), one new (`extra.ts`), and one fixed in the reversed direction.
- Observation: report-level finding omission implies a non-reliable run completion, so collecting base identities from the base report (rather than from pre-report runs) is loss-free: any base retention loss degrades the whole diff instead of misclassifying findings.
  Evidence: `PolicyReportBuilder::record_omitted_finding` calls `mark_inconclusive`, and `report_exit_status` returns 2 for non-exhaustive completions.

## Outcomes & Retrospective

- (to be written at completion)
