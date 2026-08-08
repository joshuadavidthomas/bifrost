# Bulk baseline acceptance of existing findings (issue #1881)

## Purpose

A repository that adopts Bifrost today can carry hundreds to thousands of pre-existing findings. The only durable acceptance mechanism is the suppression store (`.bifrost/suppressions.json`), which is deliberately sized for governed, reviewed waivers: at most 512 records, a mandatory per-record prose reason, a 256 KiB document cap. Onboarding a legacy codebase through it is impossible by design. Diff-aware gating (#1880) removes the pressure from pull-request gates, but scheduled full runs, release gates, and burn-down tracking still need "accept everything that exists today, gate everything new."

After this work, a user can run:

    bifrost --root . --policy-pack bifrost.code-smells --accept-current

against a repository with, say, 600 findings. One `.bifrost/baseline.json` document is written containing the 600 strong finding identities plus one batch-level reason and acceptance metadata. Every later run of the same selection exits 0: the 600 findings remain in the report, each carrying a `baseline` decision, and none of them counts toward the failure threshold. A new finding introduced afterwards still gates. Editing a policy marks its baselined findings drifted in the audit output without reactivating them. A finding proven absent by a later exhaustive run is reported stale. The two mechanisms coexist: suppressions and directory scope claim findings first; the baseline claims only what they did not.

"Finding identity" means the 32-byte digest `PolicyFindingId` (`crates/bifrost-policy/src/finding_identity.rs`) that hashes only content-derived facts under a domain string. Strong identities are checkout-independent; weak identities are snapshot-local and are never baselined.

## Orientation: the pieces this plan touches

The policy engine is `crates/bifrost-policy`. The suppression document module `src/suppression.rs` is the structural template for the new document kind: bounded workspace-confined loading via `read_workspace_document`, `deny_unknown_fields` wire structs, typed load errors, canonical sorting with duplicate rejection, and a malformed document mapping to a report diagnostic plus exit 2. The coordinator `src/coordinator.rs` owns the join-and-attach pattern (`apply_policy_suppressions` at ~1836, `apply_policy_scope` at ~1906, `apply_policy_diff` at ~1769), the gate predicate `threshold_exceeded` (~1403), and the retention loop that can mark decisions `result_omitted`. `src/finding.rs` owns the per-finding attachments (`suppression`, `scope`, `diff` at ~1959-2247) with `attach_*`/`clear_*` and `RetainedSize` accounting. `src/report.rs` owns the report document (schema_version 3, manual `Serialize` at ~2066 that counts fields and serializes optional review sections only when present — the #1880/#1868 additive pattern), the budget-charging builder (`set_diff` at ~2802 is the exemplar for charging an optional review), and the join validators (`validate_suppression_joins`, `validate_diff_joins`, `validate_scope_joins` at ~3365-3461).

The CLI is `src/bin/bifrost.rs` (policy flags must appear in `has_policy_syntax`, `option_requires_value`, the arg loop, the `--list-policies` exclusivity list, and the help text). The MCP server is `crates/bifrost-mcp/src/searchtools_service.rs` (`RunPolicyParams` at ~731) with the `run_policy` schema in `crates/bifrost-mcp/src/mcp_extended.rs` (~853, `additionalProperties: false`, pinned by a schema test at ~1306). Renderers are `crates/bifrost-policy/src/render/human.rs` and `render/sarif.rs`. Tests live in `tests/suite_bench_policy/` (`policy_suppression_evaluation.rs` is the library-level model; `bifrost_policy_cli.rs` holds the #1880 git-backed CLI tests and the exclusivity table at ~1632). Documentation pages are `docs/src/content/docs/static-analysis-policies.md` (test-enforced by `tests/suite_bench_policy/policy_docs.rs`), `cli.md`, and `ci-github-actions.md`.

Naming caution: the diff machinery's private base-identity summary in `coordinator.rs` is called `PolicyDiffBaseline`. It is unrelated to this feature. All new types here use the `PolicyBaseline*` prefix for the document kind.

## Semantics (decided; record any change in the decision log)

The document is a new kind, separate from suppressions: `.bifrost/baseline.json` by default, overridable with `--baseline-file` (CLI) and `baseline_file` (MCP `run_policy`). Entries are identity-only: per policy, a sorted list of strong finding-id hashes, plus one optional per-policy `policy_hash_at_acceptance`. The document carries exactly one batch-level `reason`, optional `accepted_by`, and `accepted_at` — no per-record prose. Schema:

    {
      "schema_version": 1,
      "reason": "one batch-level reason",
      "accepted_by": "optional author",
      "accepted_at": "2026-08-08",
      "policies": [
        { "policy_id": "...", "policy_hash_at_acceptance": "<64 hex, optional>",
          "finding_ids": ["<64 hex>", ...] }
      ]
    }

Caps: `MAX_POLICY_BASELINE_ENTRIES = 100_000` total finding ids and `MAX_POLICY_BASELINE_DOCUMENT_BYTES = 16 MiB`. Justification: the issue's size target is tens of thousands of entries; 100k is well above 50k and two decimal orders above the suppression cap while still loading in milliseconds. One pretty-printed entry line costs about 80 bytes (64 hex digits plus quotes, comma, and indentation), so 100k entries encode in about 8 MiB; 16 MiB doubles that for metadata and formatting slack. A single generation run cannot exceed the per-batch retained-findings cap (10,000), so the entry cap also leaves room for a hand-merged multi-selection document.

Generation is always explicit: the CLI flag `--accept-current` runs the selected policies and writes the baseline from the completed run's strong-identity findings. `--accept-current` forces `fail_on = Never` internally, so the run's exit status collapses to clean-versus-unreliable; the document is written only on a clean status. An unreliable or non-exhaustive run therefore refuses to define a baseline and exits 2 without writing. Weak-identity findings are never written; generation counts them and reports the exclusion on stderr (`bifrost: baseline accepted N findings into <path> (M weak-identity findings excluded)`). Findings already claimed by a suppression or scope decision are also not written: they are governed by their own mechanism, and writing them would let the baseline silently outlive a reviewed expiry. Regeneration overwrites the document wholesale; there is no merge and no auto-refresh. An existing valid baseline document participates in the generation run like any other run (harmless: generation collects strong unclaimed findings regardless of their baseline attachment), while an existing malformed one makes the run unreliable and refuses generation until repaired or removed.

Evaluation order is: suppressions, then scope, then baseline. A finding already suppressed or scoped is not claimed by the baseline; its entry is audited as `finding_claimed`. Baselined findings stay in the report with an attached `baseline` decision (`reason`, `accepted_by`, `accepted_at`, `policy_hash_state`) and stop counting toward `threshold_exceeded`. Composition with diff mode: gating counts findings that are new AND unclaimed by suppression, scope, and baseline; the baseline applies identically in full and diff runs. The diff-base evaluation loads the base image's own committed baseline document through the conventional path exactly as it loads the base's suppressions; attachments never change the identity join, and a malformed committed base document degrades the diff through the ordinary reliability rules.

Audit states mirror suppressions. Per entry: `match_state` in {`strong_finding` (applied), `finding_claimed` (present but already suppressed or scoped), `current_finding_not_strong`, `finding_absent` (stale — proven absent only by an exhaustive completed run), `policy_not_evaluated`, `policy_incomplete`}; `policy_hash_state` reuses `PolicySuppressionPolicyHashState` ({`matching`, `drifted`, `unknown`}); `applied`, `stale`, and `result_omitted` booleans. Drift does not reactivate a finding — a drifted entry still applies; it is visible in the audit so a policy edit can trigger re-review. A baseline never turns an unreliable run clean: the exit-status precedence is untouched, and a malformed or oversized document is a `baseline-load-failed` report diagnostic, which alone forces exit 2.

Reporting: a top-level `baseline: Option<PolicyBaselineReview>` on `PolicyReportDocument`, present only when a baseline document loaded, following the `packs` (#1868) and `diff` (#1880) additive pattern (manual-Serialize field-count bump, skip-when-absent, `RetainedSize` accounting, `SCHEMA_VERSION` stays 3). The review carries the document path, the batch metadata, exact counts per state (`entry_count`, `applied_count`, `claimed_count`, `not_strong_count`, `stale_count`, `policy_not_evaluated_count`, `policy_incomplete_count`, `drifted_count`, `result_omitted_count`), and a bounded entries list (`MAX_BASELINE_REVIEW_ENTRIES = 256`, sorted, with an `entries_truncated` flag like `PolicyDiffReview`'s fixed list). With up to 100k entries the full per-entry audit cannot live in the retained report, so the list retains only entries that need attention — anything other than applied-with-matching-hash — while every count stays exact. A baselined finding dropped by the retention budget increments `result_omitted_count` (flipping the entry flag when the entry is retained in the bounded list) and raises a `baseline-audit-retention-exceeded` diagnostic, mirroring the suppression behavior; the retention sort places baselined findings last, after unclaimed ones, because their identities are already durably recorded in the review counts.

SARIF representation (decision): a baselined finding emits the same `suppressions` array entry shape as a reviewed suppression — `kind: "external"`, `status: "accepted"`, `justification` = the batch reason — because SARIF consumers use that array to exclude results from gating, which matches the baseline's semantics exactly. The entry's property bag distinguishes the mechanism: `bifrost.decision: "baseline"` plus `bifrost.acceptedBy`, `bifrost.acceptedAt`, and `bifrost.policyHashState`. The run-level property bag gains `bifrost.baseline` carrying the review, next to `bifrost.suppressionReviews` and `bifrost.diffBaseline`. A finding is never both suppressed and baselined (ordering above), so the one-element array stays one-element.

## Milestone 1: document module, per-finding attachment, report review, coordinator join

At the end of this milestone `evaluate_policy_files` honors `.bifrost/baseline.json`: baselined findings stop gating, the report carries the review and per-finding decisions, and library tests demonstrate accept/drift/stale/malformed behavior.

Create `crates/bifrost-policy/src/baseline.rs` modeled on `suppression.rs`: constants (`DEFAULT_POLICY_BASELINE_PATH`, the two caps above, `MAX_POLICY_BASELINE_REASON_BYTES = 4096`, `MAX_POLICY_BASELINE_ACCEPTED_BY_BYTES = 256`, `MAX_POLICY_BASELINE_PATH_BYTES = 1024`, schema version 1); `PolicyBaselineSource`/`PolicyBaselineOptions`/`PolicyBaselineSourceError`; `PolicyBaselineDocument { schema_version, reason, accepted_by, accepted_at, policies }` with `PolicyBaselinePolicyRecord { policy_id, policy_hash_at_acceptance, finding_ids }` and an `entry_count()` accessor; wire structs with `deny_unknown_fields`; `load_policy_baseline_from_root` + `parse_policy_baseline_document` with typed `PolicyBaselineLoadError`/`PolicyBaselineDocumentError`/`PolicyBaselineValidationError` (document too large, unsupported schema version, too many entries, invalid ids and hashes with policy/entry indexes, blank or oversized reason and accepted_by, duplicate policies, duplicate finding ids); canonical sorting; `RetainedSize` impls. Reuse `PolicyEvaluationDate`, `AcceptedPolicyHash`, `validate_required_text`, and `parse_lower_sha256` from the existing modules; add `AcceptedPolicyHash::from_bytes` so generation can record `PolicySemanticHash` values. Also define here the review types (`PolicyBaselineMatchState`, `PolicyBaselineEntryReview`, `PolicyFindingBaseline`) and the generation constructor `PolicyBaselineDocument::from_completed_report(report, reason, accepted_by, accepted_at) -> (document, weak_excluded_count)` plus `to_canonical_json`.

`src/finding.rs`: `baseline: Option<PolicyFindingBaseline>` on `PolicyFinding` after `diff`, `#[serde(skip_serializing_if = "Option::is_none")]`, `None` in `try_new`, `baseline()`/`attach_baseline()` (rejecting non-strong identity and duplicates like `attach_suppression`)/`clear_baseline()`, `RetainedSize` contribution, and the two error variants.

`src/report.rs`: `PolicyBaselineReview` (fields as decided above; constructor asserts count consistency the way `PolicyDiffReview::new` does and truncates the sorted notable-entry list) plus `baseline: Option<PolicyBaselineReview>` on `PolicyReportDocument` and the builder: field, `set_baseline` charging bytes exactly like `set_diff`, `baseline_extra()` included in every retention fit check beside `diff_extra()`/`packs_extra()`, `mark_baseline_result_omitted(policy_id, finding_id)` mirroring `mark_suppression_result_omitted` but count-based, `finish()` pass-through, manual `Serialize` field-count bump, `RetainedSize`, and `validate_baseline_joins` (no review present implies no finding attachment; review present implies the number of retained findings with attachments equals `applied_count - result_omitted_count`).

`src/coordinator.rs`: `PolicyEvaluationOptions` gains `baseline: PolicyBaselineOptions` with `with_baseline` and an accessor plus retained-size accounting; load the document next to the suppression/scope loads (`Err` becomes a `BaselineLoadFailed` diagnostic; both new diagnostic codes — `BaselineLoadFailed`, `BaselineAuditRetentionExceeded` — join `PolicyReportDiagnosticCode` with serde snake-case names `baseline-load-failed` / `baseline-audit-retention-exceeded` following the existing rename scheme); `apply_policy_baseline(document, path, registry, runs)` runs after the report builder is constructed (so a suppression-audit preflight rollback settles first) and before `threshold_exceeded` is computed: per policy record compute the hash state once, build one id-to-index map over the run's findings, classify every entry, attach `PolicyFindingBaseline` to strong unclaimed matches, and return the review plus an index map for result-omission marking; extend `threshold_exceeded` with `finding.baseline().is_none()`; extend the retention sort key to place baselined findings last; in the retention loop mark omitted baselined findings; emit the retention diagnostic when any were omitted; `set_baseline` alongside `set_diff`/`set_packs`.

`src/lib.rs`: `mod baseline; pub use baseline::*;`.

Library tests: unit tests in `baseline.rs` for parsing, caps, and canonical rejection; integration tests in a new `tests/suite_bench_policy/policy_baseline_evaluation.rs` (module registered in that suite's `main.rs`) modeled on `policy_suppression_evaluation.rs`, covering accept-then-clean, precedence under suppression (claimed entry), drift without reactivation, stale on source edit, weak findings never applied, malformed document exiting 2 with the diagnostic, and JSON shape stability (no `baseline` key without a document).

Acceptance: `cargo test -p brokk-bifrost-policy` and `cargo test --test suite_bench_policy -- policy_baseline_evaluation::` pass.

## Milestone 2: renderers

SARIF (`render/sarif.rs`): populate the result-level `suppressions` array from either the suppression or the baseline attachment (an untagged property-set enum keeps the array element type single); add the run-level `bifrost.baseline` property serialized only when present. Human (`render/human.rs`): the concise filter also hides baselined findings; `write_finding` gains a `baseline:` stanza next to the suppression stanza; `write_summary` subtracts retained baselined findings from the active count and appends `; baseline: A accepted of E entries (D drifted, S stale) via <path>` plus a result-omitted count when non-zero; the verbose view lists non-applied bounded review entries after the suppression reviews.

Acceptance: rendering tests in `tests/suite_bench_policy/policy_rendering.rs` / `policy_sarif_rendering.rs` assert the three formats agree, and that a report without a baseline document emits neither the run property nor any suppression entries for unclaimed findings.

## Milestone 3: CLI flag, MCP parameter

CLI (`src/bin/bifrost.rs`): `--baseline-file PATH` (value-bearing, once-only, workspace-relative, mirroring `--suppressions-file`) and `--accept-current` (boolean, once-only). Both join `has_policy_syntax`; `--baseline-file` joins `option_requires_value`; both join the `--list-policies` exclusivity list and the help text. `--accept-current` rejects `--fail-on` (it forces Never) and `--diff-base` (a baseline is defined by a full run). On a clean run, `run_policy_mode` builds the document via `from_completed_report` (reason: "Bulk baseline acceptance of existing findings via --accept-current", `accepted_by` omitted, `accepted_at` = the evaluation date), writes it atomically (temp file beside the destination, persist-rename, parent directories created) to the resolved baseline path beneath the root, prints the acceptance line to stderr, and still renders the normal report. On a non-clean status it writes nothing and propagates exit 2.

MCP: `baseline_file: Option<String>` on `RunPolicyParams`, converted with `PolicyBaselineSource::explicit_portable` and threaded via `with_baseline`; schema property with `maxLength MAX_POLICY_BASELINE_PATH_BYTES`; the pinned schema test gains the property assertions. No MCP generation surface in this issue.

Acceptance: from a fixture repository, `--accept-current` writes the document and a second run exits 0; `bifrost --tool run_policy`-equivalent MCP invocations accept `baseline_file`.

## Milestone 4: end-to-end tests and documentation

CLI tests in `tests/suite_bench_policy/bifrost_policy_cli.rs`: accept-current on a fixture with more than 512 findings (one match policy over one generated source file with ~600 sites; strong identities disambiguate identical sites by ordinal) followed by a clean exit-0 run; a new finding after acceptance gating with exit 1; a policy edit marking entries drifted while still exiting 0; a malformed document exiting 2; an unreliable run (a selection whose run is non-exhaustive or diagnostic-laden) refusing `--accept-current` and writing nothing; the weak-exclusion count on stderr; exclusivity-table rows for the new flags (`--accept-current` with `--fail-on`, with `--diff-base`, doubled; `--baseline-file` doubled, missing value, absolute path; `--list-policies` with either flag); cross-format agreement (JSON and SARIF) on the baseline review and per-finding decisions; and diff-mode composition (baselined persisting findings plus one new finding gating exactly once).

Documentation: a baseline section in `docs/src/content/docs/static-analysis-policies.md` (runnable examples must run — the page is test-enforced by `policy_docs.rs`), the two flags in `cli.md`, and an onboarding recipe in `ci-github-actions.md` (accept-current once locally or in a setup job, commit `.bifrost/baseline.json`, keep `--diff-base` for PR gates and the baseline for scheduled full runs).

Validation for the whole feature (featureless only; never enable nlp):

    cargo fmt
    cargo test -p brokk-bifrost-policy
    cargo test --test suite_bench_policy
    cargo test -p brokk-bifrost-mcp
    cargo clippy --workspace --all-targets -- -D warnings

## Non-goals

Replacing reviewed suppressions (the two coexist with distinct semantics). Automatic baseline refresh (regeneration is always explicit). An MCP generation parameter. Baseline-aware rename tracking (the #1880 identity limitations apply unchanged). Merging baseline documents.

## Decision log

2026-08-08: Caps set to 100_000 entries / 16 MiB with the arithmetic recorded in Semantics; entries-only storage keeps one entry near 80 encoded bytes.

2026-08-08: Generation excludes findings claimed by suppressions or scope, so a reviewed expiry cannot be silently outlived by the bulk mechanism; evaluation mirrors this with the `finding_claimed` audit state.

2026-08-08: `--accept-current` forces `fail_on = Never` so clean-versus-unreliable is the only distinction; findings are expected input, not a failure. It rejects `--diff-base` and `--fail-on` outright.

2026-08-08: The per-entry audit is bounded to 256 notable entries (everything except applied-with-matching-hash) with exact counts for every state, because a 100k-entry audit vector cannot live inside the retained-report budget; `result_omitted_count` increments even for entries outside the bounded list, so the count stays exact while the list stays best-effort.

2026-08-08: Baselined findings sort last in the retention order (after suppressed/scoped, after unclaimed) because their identities are already durably recorded in the review; their omission is counted and diagnosed but must not crowd out unclaimed findings.

2026-08-08: SARIF renders baselined findings as `suppressions` entries (kind `external`, status `accepted`) with `bifrost.decision: "baseline"` in the property bag, so standard SARIF consumers exclude them from gating while the mechanism stays distinguishable; the existing suppression property bag is left unchanged (asymmetric marker, additive-safe).

2026-08-08 (implementation): `PolicyReportEvaluationContext` is left untouched; the review carries the document path. A report without a baseline document keeps its exact schema-version-3 byte shape, matching the `packs` exemplar.

2026-08-08 (implementation): The review's bounded entries list also retains `finding_claimed`, `current_finding_not_strong`, and `policy_incomplete` entries (not only stale/drifted/not-evaluated ones): anything that is not applied-with-matching-hash needs attention.

2026-08-08 (implementation): `apply_policy_baseline` runs after the report builder is constructed so the suppression-audit preflight rollback (which clears suppression and scope attachments) settles before the baseline decides what is claimed.

2026-08-08 (implementation): All baseline types including `PolicyBaselineReview` live in `crates/bifrost-policy/src/baseline.rs`, matching how `suppression.rs` owns `PolicySuppressionReview`; the milestone-1 sentence placing the review in `report.rs` followed the diff exemplar and was superseded by module cohesion.

2026-08-08 (implementation): The weak-identity exclusion count is proven by a unit test in `report.rs` that assembles a report with one strong and one weak finding through the builder, because ordinary match evaluation on the CLI fixtures never produces a weak anchor; the CLI test asserts the count's presence in the stderr acceptance line.

2026-08-08 (implementation): The shared JSON-decode message truncation helper `bounded_error_message` in `suppression.rs` was renamed `bounded_json_error_message` and made `pub(crate)` for reuse by the baseline loader, and `AcceptedPolicyHash` gained `from_bytes` so generation can record `PolicySemanticHash` values.

## Progress

- [x] (2026-08-08) ExecPlan written.
- [x] (2026-08-08) Milestone 1: `baseline.rs` document module (caps, typed errors, canonical load, review types, generation constructor, unit tests), `PolicyFinding.baseline` attachment, `PolicyBaselineReview` + builder charging (`set_baseline`, `baseline_extra`, `mark_baseline_result_omitted`) + `validate_baseline_joins`, coordinator load/join/gate/retention wiring with the two new diagnostic codes, and the `policy_baseline_evaluation.rs` suite (8 tests: accept-then-clean, new-finding gating, drift without reactivation, stale on source edit, suppression precedence, malformed and unselected documents, diff-mode composition, deterministic JSON shape). `cargo test -p brokk-bifrost-policy`: 304 passed.
- [x] (2026-08-08) Milestone 2: SARIF result-level suppressions entries from the baseline attachment (untagged property union, `bifrost.decision: "baseline"`), run-level `bifrost.baseline` property; human concise filter, verbose baseline stanza, verbose review section with per-state counts and bounded entries, summary counts. Tests: `policy_rendering::baseline_mode_agrees_across_concise_verbose_and_json`, `policy_sarif_rendering::baseline_findings_emit_suppressions_and_run_level_review` (offline SARIF 2.1.0 schema-validated).
- [x] (2026-08-08) Milestone 3: CLI `--baseline-file` and `--accept-current` in all parse sites plus help; acceptance forces fail-on Never, writes atomically only on a clean status, reports the stderr acceptance line with the weak-exclusion count, and rejects `--fail-on`/`--diff-base`; MCP `run_policy.baseline_file` (params, bounded schema property, pinned schema test). Verified end to end on a scratch fixture (gate 1 -> accept 0 -> gate 0).
- [x] (2026-08-08) Milestone 4: CLI end-to-end tests (600-finding onboarding beyond the 512 suppression cap with idempotent regeneration and post-acceptance gating; JSON/SARIF cross-format agreement on the review plus drift without reactivation; malformed document exit 2 with `baseline-load-failed` and acceptance refusal that leaves the document untouched; an unreliable run writing nothing; eight exclusivity-table rows), the weak-exclusion unit test, docs sections in `static-analysis-policies.md` (enforced by `policy_docs::baseline_documentation_states_the_contract`), `cli.md`, and `ci-github-actions.md`. Validation: `cargo test -p brokk-bifrost-policy` 305 passed; `cargo test --test suite_bench_policy` 346 passed; `cargo test -p brokk-bifrost-mcp` 148 passed across targets; `cargo clippy --workspace --all-targets -- -D warnings` clean.

## Surprises & Discoveries

- Observation: any retained-finding omission already marks its run inconclusive and therefore unreliable, so the `result_omitted` baseline path can only occur inside runs that already exit 2 — exactly like suppressions; the acceptance fixture must simply stay within the retention budgets (600 findings of a match policy do).
  Evidence: `PolicyReportBuilder::record_omitted_finding` -> `mark_inconclusive`; `report_exit_status` returns 2 for non-reliable completions.
- Observation: `--accept-current` with `fail_on = Never` makes exit status 2 cover both unreliability and non-exhaustiveness (`report_exit_status` treats non-exhaustive runs as unreliable when the threshold is not exceeded), which is precisely the refusal semantics the issue asks for.

## Outcomes & Retrospective

2026-08-08: All four milestones are implemented and validated on this branch. A user can onboard a legacy repository in one `--accept-current` step — demonstrated end to end with 600 findings, beyond the 512-record suppression cap — after which the same selection exits 0 with every accepted finding still visible and audited; a new finding gates; a policy edit surfaces as drift without reactivation; a fixed finding surfaces as a stale entry; and the document obeys the suppression store's trust rules (malformed = diagnostic + exit 2; an unreliable run can neither define a baseline nor be turned clean by one). The issue's four acceptance criteria each have a dedicated test.

What deviated from the plan's letter (each recorded in the decision log): the review types live in `baseline.rs` rather than `report.rs`; the weak-exclusion count is proven by a crafted-report unit test rather than a CLI fixture, because match evaluation does not produce weak anchors on these fixtures; two small shared-helper changes in `suppression.rs` (renamed `bounded_json_error_message`, new `AcceptedPolicyHash::from_bytes`).

Lesson learned: the additive-report-field pattern is now three deep (`diff`, `packs`, `baseline`) and each field must be charged in five builder fit paths plus the emergency-reservation recalculation; a future fourth field should probably fold the optional reviews into one accounted collection.
