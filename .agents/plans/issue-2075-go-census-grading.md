# Make Go census grading distinguish answers from analyzer gaps

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

The reference differential runner currently reports ordinary Go locals, Go's predeclared language symbols, and field labels whose composite-literal owner is unavailable as missing analyzer definitions whenever an unrelated declaration with the same spelling exists in the file. After this change, those sites remain visible in the census but are classified honestly: locals and predeclared symbols are adjudicated answers with no evidence tier, while an owner-unresolved literal label remains an inconclusive tier-3 boundary. A literal label whose exact workspace owner is known must remain actionable if the named field is missing, so the change cannot hide real Go resolver defects.

## Progress

- [x] (2026-08-13 20:20Z) Audited issue #2075, the saved Go ledger, the Go forward resolver, and the runner's census tier classifier.
- [x] (2026-08-13 20:31Z) Added canonical diagnostics for Go local bindings and predeclared symbols.
- [x] (2026-08-13 20:31Z) Required exact owner evidence before a Go composite-literal label can use a coincidental same-file declaration as tier-2 evidence.
- [x] (2026-08-13 20:37Z) Added an end-to-end census behavior test for the three negative groups, a known-owner positive control, and package shadowing precedence.
- [ ] (2026-08-13 20:45Z) Run validation (completed: focused tests, Go crate tests, dependency check, focused clippy, and five exact corpus replays; remaining: complete current rank-31+ Go census).
- [ ] Commit, push, comment with evidence, and close #2075.

## Surprises & Discoveries

- Observation: The Go resolver already proves local shadowing, including a message of the form ``name is shadowed by a local Go binding``, but emits the generic `no_indexed_definition` diagnostic. The runner therefore grades an answer as joint blindness.
  Evidence: `crates/bifrost-analysis/src/analyzer/usages/get_definition/go.rs::resolve_go` checks `GoReferenceResolution::shadowed` before returning the generic diagnostic.

- Observation: The composite-literal resolver already distinguishes an unknown exact owner (`go_literal_owner_unresolved`) from a known owner that lacks a direct field (`no_indexed_definition`). The grading correction can preserve that distinction without re-resolving owners in the runner.
  Evidence: `go_keyed_composite_label_outcome` emits the two diagnostics on separate structured branches.

- Observation: The saved baseline contains 84 owner-unresolved label rows, while the issue describes 64 non-product boundaries. Twenty baseline rows belonged to analyzer gaps fixed by the preceding Go issue sequence. Final acceptance must use the current binary and retain the known-owner control rather than blindly suppress all 84 historical rows.

- Observation: Ordinary Go expression locals now resolve through the shared lexical-definition route before language dispatch, producing `resolved` with a lexical definition and no CodeUnit target. The 63a ledger predates that behavior. The Go-specific `shadowed` branch still needs the canonical local diagnostic for shapes the shared route does not classify.
  Evidence: The initial end-to-end fixture returned `forward_status=resolved`, `targets=[]`, and `tier=None` for a parameter use.

## Decision Log

- Decision: Reuse the Go crate's complete predeclared-name registry and expose it as a public language helper.
  Rationale: Go permits package and local declarations to shadow predeclared names. The resolver will consult the helper only after ordinary package, local, and import resolution is exhausted, so user declarations retain precedence and the list is not duplicated.
  Date/Author: 2026-08-13 / Codex

- Decision: Add a canonical `predeclared_symbol_reference` diagnostic and use the existing canonical `local_variable_reference` diagnostic for Go shadowed bindings.
  Rationale: These are negative but complete answers. The generic adjudication predicate is the shared contract used by the runner, so correct diagnostics prevent false grading without language-specific suppression.
  Date/Author: 2026-08-13 / Codex

- Decision: Demote same-file evidence only when the Go resolver emitted `go_literal_owner_unresolved`.
  Rationale: The diagnostic proves the structured owner is unavailable. A same-spelled declaration elsewhere in the file cannot establish reachability. The separate known-owner/missing-field branch remains tier 2 and actionable.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The root behavior is implemented and focused validation passes. Exact replays prove that `error` and `len` are adjudicated with `predeclared_symbol_reference`, an external-owner label is tier 3, and two historical product gaps now resolve consistently through earlier Go fixes. The full current rank-31+ Go census and issue-tracker closure remain.

## Context and Orientation

`crates/bifrost-analysis/src/analyzer/usages/get_definition/go.rs` implements Go goto-definition. It asks the Go language crate for lexical and package resolution facts, then returns a `DefinitionLookupOutcome`. Some `NoDefinition` outcomes are failures to find an expected indexed declaration; others are adjudicated answers, meaning the resolver proved the token names a local binder or language-provided symbol that is deliberately not indexed.

`crates/bifrost-analysis/src/analyzer/usages/get_definition/mod.rs` owns the canonical diagnostic strings and `is_adjudicated_answer_diagnostic_kind`, the shared predicate that tells consumers which negative outcomes are complete answers.

`crates/bifrost-go/src/diagnostics.rs` already owns the complete Go predeclared-name registry. A predeclared name is supplied by the Go language itself, such as `error`, `len`, or `append`; it is not a workspace declaration unless source code shadows it.

`src/reference_differential/mod.rs` implements the reference differential runner. Its census mode samples syntax identifiers, asks goto-definition for each one, and grades unresolved sites. Tier 1 or 2 means a reachable or same-file declaration is evidence of a likely analyzer gap. Tier 3 means no such evidence exists and remains inconclusive. Today `classify_census_gaps` reduces all declarations in a file to a set of terminal names, which is unsound for a Go keyed literal label if the literal's owner could not be resolved.

## Plan of Work

First, make `brokk_bifrost_go::diagnostics::is_predeclared_go_name` a documented public helper. Add `PREDECLARED_SYMBOL_REFERENCE_DIAGNOSTIC_KIND` beside the existing local and declaration-site diagnostics in `crates/bifrost-analysis/src/analyzer/usages/get_definition/mod.rs`, and make the adjudication predicate recognize it.

Next, update `resolve_go` so its proven local-shadow branch returns `LOCAL_VARIABLE_REFERENCE_DIAGNOSTIC_KIND`. After package members, dot imports, and external import boundaries are exhausted, consult `is_predeclared_go_name`. If true, return `NoDefinition` with the new canonical diagnostic. This ordering is mandatory because Go declarations may shadow predeclared symbols.

Then define a shared constant for the existing `go_literal_owner_unresolved` diagnostic and use it in both the resolver and runner. In `classify_census_gaps`, do not treat a same-file terminal-name collision as evidence when the language is Go and that diagnostic is present. The site still receives tier 3 and remains in the report. Do not change the known-owner `no_indexed_definition` branch.

Add `tests/suite_semantic/issue_2075_go_census_grading.rs` and register it in `tests/suite_semantic/main.rs`. Use one `InlineTestProject` fixture containing a parameter/local colliding with a field name, predeclared `len` and `error` colliding with unrelated field declarations, an unresolved external literal owner whose label collides with a same-file field, and a known workspace literal owner missing that same field. Assert canonical diagnostics and classifications at the exact reference offsets.

## Concrete Steps

Run all commands from `/mnt/optane/bifrost-fird`.

Implement and format:

    cargo fmt --all

Run focused behavior and unit validation:

    scripts/with-isolated-cargo-target.sh cargo test --test suite_semantic -- issue_2075_go_census_grading
    scripts/with-isolated-cargo-target.sh cargo test --test suite_semantic -- issue_2074_go_declaration_probe_eligibility
    scripts/with-isolated-cargo-target.sh cargo test --test suite_semantic -- reference_differential::census_scala_adjudicated_local_binder_is_not_graded_as_a_gap
    scripts/with-isolated-cargo-target.sh cargo test -p brokk-bifrost-go
    node scripts/check-workspace-dependencies.mjs

Run focused featureless clippy before pushing:

    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-go -p brokk-bifrost-analysis --all-targets -- -D warnings

Build the release runner, replay representative saved rows with `--cache-mode ephemeral`, then rerun the Go rank-31+ leg at its pinned repository revisions. Expect locals and predeclared symbols to be adjudicated, external-owner labels to be tier 3, and the known-owner control to stay tier 2 Missing.

## Validation and Acceptance

The new semantic test must fail before implementation because the local and predeclared sites are graded Missing and the unknown-owner label receives tier 2. It must pass afterward with these exact properties:

* The ordinary local reference has a resolved lexical answer with no CodeUnit targets, no tier, and classification `Inconclusive`. Any site reaching Go's secondary shadow branch uses diagnostic `local_variable_reference`.
* The unshadowed Go predeclared `len` and `error` references have diagnostic `predeclared_symbol_reference`, no tier, and classification `Inconclusive`.
* The unresolved-owner literal label has diagnostic `go_literal_owner_unresolved`, tier 3, and classification `Inconclusive`, even though an unrelated same-file field shares its spelling.
* The known-owner literal label that is not a field of that owner keeps `no_indexed_definition`, tier 2, and classification `Missing`.

Focused Go resolver and runner tests, formatting, dependency checks, and clippy must pass. Representative exact corpus replays must exhibit the same dispositions. The full current rank-31+ Go census must clear the 146 issue-owned non-product rows without suppressing current confirmed analyzer defects.

## Idempotence and Recovery

All edits and test commands are repeatable. Isolated Cargo targets are removed by the repository helper. Corpus smoke tests use ephemeral cache mode and do not mutate repository clones. If a corpus replay exposes a known-owner literal among the historical owner-unresolved set, preserve it as actionable and refine the structured resolver; do not broaden the grading exclusion.

Commit only the files named in this plan. If the remote advances before push, fetch and merge it into the current branch, rerun the focused tests affected by conflict resolution, and push the merge without rebasing.

## Artifacts and Notes

The baseline ledger is `/mnt/optane/tmp/bifrost-fird/final-63a1912a/go-ranks31-50-63a1912a-raw-ledger.jsonl`. It records the original diagnostics and exact replay commands. Its historical owner-unresolved count exceeds the issue's final non-product count because earlier Go product fixes were not present at baseline.

## Interfaces and Dependencies

`crates/bifrost-go/src/diagnostics.rs` must expose:

    pub fn is_predeclared_go_name(name: &str) -> bool

`crates/bifrost-analysis/src/analyzer/usages/get_definition/mod.rs` must expose the canonical diagnostic constants through the existing analysis API and recognize both local and predeclared answers in `is_adjudicated_answer_diagnostic_kind`.

No new crate or dependency is required. The facade runner already depends on the analysis and Go language crates through existing workspace boundaries.

Plan revision note (2026-08-13): Initial plan created after source and ledger audit. It chooses canonical diagnostic normalization plus a diagnostic-backed owner-evidence gate so current known-owner defects remain visible.

Plan revision note (2026-08-13): Recorded that the shared lexical-definition route has already superseded the historical generic local misses. The acceptance now preserves that stronger resolved answer while retaining canonical diagnostics for the secondary Go shadow path.

Plan revision note (2026-08-13): Recorded the completed implementation, focused validation, and exact replay outcomes before the checkpoint commit. The full corpus rerun remains the final evidence milestone.
