# Project precise taint display paths

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

Maintain this plan according to `.agents/PLANS.md`.

## Purpose / Big Picture

Concise taint findings currently print one canonical witness without interpretation. This output repeats locations and labels declarations as sources.

After this change, concise output will show a small semantic path. It will start at the selected source expression.

The path will end at the sink expression. It will keep useful call, return, sanitizer, and transform boundaries.

JSON, SARIF, and verbose human output will keep the canonical witness. The new projection will not change canonical evidence.

## Progress

- [x] (2026-08-13 06:52Z) Read issue 2090 and its acceptance criteria.
- [x] (2026-08-13 06:52Z) Fetched `origin/master` and inspected the current worktree state.
- [x] (2026-08-13 06:52Z) Traced taint witness projection, finding assembly, and concise rendering on `origin/master`.
- [x] (2026-08-13 06:52Z) Selected a separate, non-serialized display-path design.
- [x] (2026-08-13 07:45Z) Continued on the user-selected issue branch at `fcd830452`.
- [x] (2026-08-13 07:45Z) Added bounded display-path types and deterministic selection logic.
- [x] (2026-08-13 07:45Z) Projected source-backed labels while the workspace analyzer was available.
- [x] (2026-08-13 07:45Z) Attached the selected path without changing canonical serialization or retention.
- [x] (2026-08-13 07:45Z) Rendered contiguous concise rows and typed omission or incompleteness text.
- [x] (2026-08-13 07:45Z) Added the exact Java relay fixture and synthetic ranking and bounds tests.
- [x] (2026-08-13 07:45Z) Ran formatting, focused tests, workspace clippy, and the featureless workspace suite.
- [x] (2026-08-13 07:45Z) Prepared a multiline checkpoint commit on the authorized current branch.

## Surprises & Discoveries

- Observation: The current worktree has a detached `HEAD` at `cea3a19a0`.
  Evidence: `git status --short --branch` prints `## HEAD (no branch)`.

- Observation: The checkout is 139 commits behind `origin/master` and one commit ahead.
  Evidence: `git rev-list --left-right --count HEAD...origin/master` prints `1 139`.

- Observation: The concise witness table does not exist in the checked-out files.
  Evidence: It exists on `origin/master` in `crates/bifrost-policy/src/render/human.rs`.

- Observation: The selected origin already has the exact source call location.
  Evidence: `project_taint_origins` uses `origin.origin().value_flow_key().site()`.

- Observation: Summary witness steps retain structured call and return data.
  Evidence: `SummaryWitnessStep` retains `kind`, `source`, `target`, `origin`, and `boundary`.

- Observation: The Bifrost code-navigation tools are unavailable in this session.
  Evidence: The available tool list contains no Bifrost MCP tool.

- Observation: The Bifrost policy-checking tools are also unavailable in this session.
  Evidence: The active tool list contains neither `list_policies` nor `run_policy`.

- Observation: The exact Java relay projects five useful rows.
  Evidence: The rows are source, call, propagation, return, and sink. All labels use indexed source slices.

- Observation: A modeled external witness can contain only generic propagation rows at endpoint or enclosing ranges.
  Evidence: The concise path keeps source and sink, states incompleteness, and verbose output keeps all 15 canonical rows.

- Observation: The featureless workspace suite has five unrelated JavaScript symbol failures on this checkout.
  Evidence: Four expected file-qualified FQNs but received local FQNs. One fuzzy search expected the file prefix. An isolated rerun reproduced one failure. No changed file is in JavaScript analysis or symbol lookup.

## Decision Log

- Decision: Keep display paths separate from `BoundedWitness`.
  Rationale: A display path is presentation data. A bounded witness is canonical analysis evidence.
  Date/Author: 2026-08-13 / Codex

- Decision: Store the display path on `PolicyFinding` with `#[serde(skip)]`.
  Rationale: Concise rendering needs the path. JSON and SARIF must remain unchanged.
  Date/Author: 2026-08-13 / Codex

- Decision: Build labels from structured semantic locators and indexed source slices.
  Rationale: The projection has exact spans and source snapshots. The terminal renderer does not.
  Date/Author: 2026-08-13 / Codex

- Decision: Do not charge display-path bytes to the canonical report-retention budget.
  Rationale: Presentation data must not remove canonical findings or evidence.
  Date/Author: 2026-08-13 / Codex

- Decision: Give display data separate fixed bounds.
  Rationale: The cache must remain small without changing canonical retention decisions.
  Date/Author: 2026-08-13 / Codex

- Decision: Stop before source edits on this checkout.
  Rationale: Repository rules prohibit branch changes without user authorization. The required code exists only on current master.
  Date/Author: 2026-08-13 / Codex

- Decision: Treat the user-selected branch as authorization to continue from `fcd830452`.
  Rationale: The user explicitly moved the worktree to a feature branch and requested plan execution.
  Date/Author: 2026-08-13 / Codex

- Decision: Keep generic intraprocedural rows only when they change the structured dataflow fact.
  Rationale: Same-fact rows at endpoint or enclosing ranges add noise. The canonical witness remains available in verbose, JSON, and SARIF output.
  Date/Author: 2026-08-13 / Codex

- Decision: Preserve call and return boundaries even when they share another row's location.
  Rationale: A shared source range does not make two different interprocedural events equivalent.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation now prepares one bounded display path for each retained taint finding. It selects a path by quality and stable witness identity.

Concise output uses exact source, call, propagation, return, and sink rows. JSON, SARIF, and verbose output still use canonical witnesses.

The policy crate passed 331 tests. The taint adapter suite passed all 28 tests. Workspace clippy passed with warnings denied.

The featureless workspace run passed all changed policy tests and most workspace suites. It stopped on five unrelated JavaScript symbol assertions in `suite_symbols`.

The Bifrost MCP policy check could not run because the plugin did not register `list_policies` or `run_policy` in this task.

## Context and Orientation

`crates/bifrost-policy/src/taint_policy.rs` converts analyzer taint results into policy findings. Function `project_taint_witnesses` converts `SummaryWitness` values into canonical `BoundedWitness` values.

`crates/bifrost-policy/src/witness_projection.rs` performs the shared canonical conversion. It assigns generic labels to canonical witness steps.

`crates/bifrost-policy/src/projection.rs` defines `ProjectedFindingReport`. This structure carries adapter results into final finding assembly.

`crates/bifrost-policy/src/evaluator.rs` validates projections. It then constructs `PolicyFinding` values from `ProjectedFindingReport`.

`crates/bifrost-policy/src/finding.rs` defines canonical findings and witnesses. These types provide JSON and SARIF evidence.

`crates/bifrost-policy/src/render/human.rs` renders concise and verbose human output. Concise output currently selects `finding.witnesses().first()`.

`tests/suite_bench_policy/taint_policy_adapter.rs` contains model-backed taint tests. Current concise assertions only check broad kind names.

A canonical witness is the exhaustive retained analysis artifact. A display path is a smaller presentation projection from that artifact.

A semantic locator identifies a source span and its semantic role. The workspace analyzer can read the matching indexed source snapshot.

## Plan of Work

Create `crates/bifrost-policy/src/display_path.rs`. Define bounded internal types for a path, a row, and a row kind.

Each row must contain a kind, a `PolicySourceLocation`, and a bounded label. The path must also record canonical incompleteness.

Limit one path to 12 rows. Limit each normalized label to 160 UTF-8 bytes.

Normalize labels without a regular expression. Collapse whitespace to one ASCII space and trim it.

Truncate labels only at a valid UTF-8 boundary. Use a stable ASCII suffix when truncation occurs.

Add candidate scoring and selection to `display_path.rs`. Keep input order inside each candidate path.

Score candidates by complete anchors first. Then score informative rows, duplicate noise, and enclosing-only rows.

Use the canonical witness identity as the final tie-breaker. Document the full tuple beside the comparison code.

Build each candidate with the selected taint origin as its source anchor. Use the finding primary location as its sink anchor.

For an `IcfgEdgeKind::Call`, use the structured call-site locator. Label the row from that source slice.

For normal and exceptional returns, keep a typed return row. Use the matching structured call-site locator.

For a summary gap, keep a typed return row. Mark canonical incompleteness when the witness is truncated.

For other edges, use the structured program-point locator. Do not infer flow from source text.

Remove identical semantic events at identical locations. Remove repeated generic rows at the same location.

For nested locations in one expression, keep the smallest informative row. Keep distinct call, return, sanitizer, and transform events.

Never reorder the remaining rows by file position. Preserve the canonical flow order.

Always insert the selected origin and sink. Do this even when canonical endpoints use wider spans.

Add source helpers to `crates/bifrost-policy/src/semantic_identity.rs`. Use `WorkspaceAnalyzer::analyzer().indexed_source` and exact locator spans.

Do not read workspace files from `render/human.rs`. The renderer must consume only the prepared display path.

Add an optional display path to `ProjectedFindingReport`. Set it only for taint projections.

Add an optional display path to `PolicyFinding`. Mark the field with `#[serde(skip)]`.

Keep `PolicyFinding::try_new` unchanged for canonical callers. Add a crate-private method that attaches validated display data afterward.

Exclude display data from canonical retained-size accounting. Enforce its separate fixed bounds during display projection.

In `evaluator.rs`, attach the prepared path after canonical finding validation succeeds. Other analysis types keep `None`.

Update concise rendering to prefer the prepared display path. Keep the present canonical fallback for synthetic non-taint findings.

Number rendered rows with `enumerate()`. Never use a canonical witness index as the visible row number.

Report only rows omitted by the display-row bound. Report canonical truncation as path incompleteness without a fabricated stage count.

Keep the alternate-path message. Count retained alternates relative to the selected witness, not relative to the first witness.

Add a Java relay test to `tests/suite_bench_policy/taint_policy_adapter.rs`. Use `InlineTestProject` as repository rules require.

The Java file must contain `userInput`, `relay`, `eval`, and `unsafe`. The policy must bind the source return and sink argument.

Assert the exact display rows, locations, labels, kinds, and complete plain output. Do not use substring-only path checks.

Capture canonical JSON for the noisy witness. Prove that attaching and rendering the display path does not change it.

Add a synthetic unit test in `display_path.rs`. Include duplicate locations, nested spans, and a real call/return boundary.

Give the synthetic test two alternate witnesses. Give them different display quality and stable identities.

Mark one witness truncated. Assert deterministic selection, stable ordering, contiguous rendering, and explicit incompleteness.

Add terminal-control characters to one synthetic label. Assert visible escaping in plain output.

Run the renderer with a small byte limit. Assert `PolicyRenderError::SerializedReportLimit`.

## Concrete Steps

Work from `/Users/dave/.codex/worktrees/6ef0d63d-2949-4f45-b2dd-055bac59b97f/bifrost`.

First, get explicit authorization for a branch change. Attach this worktree to an issue branch based on current `origin/master`.

Confirm the base and worktree:

    git status --short --branch
    git rev-list --left-right --count HEAD...origin/master

Expect an attached issue branch and no unexpected changes.

Implement the internal types and projection. Format the Rust code:

    cargo fmt

Run the focused policy tests:

    cargo test --test suite_bench_policy taint_policy_adapter
    cargo test -p brokk-bifrost-policy display_path
    cargo test -p brokk-bifrost-policy render::human

The exact package name must match the workspace manifest. Adjust only the `-p` value if needed.

Run featureless workspace checks for the changed crates:

    cargo test --workspace --no-default-features
    cargo clippy --workspace --all-targets --no-default-features -- -D warnings

Do not enable `nlp`. This change does not affect semantic search.

Inspect the final changes:

    git status --short
    git diff --check
    git diff --stat

Create a multiline checkpoint commit. Stage only files changed for issue 2090.

## Validation and Acceptance

The Java relay test must print exactly one source, one relay stage, and one sink. All locations must name exact call expressions.

No row can label `unsafe()` or the `relay()` declaration as a source. Duplicate generic rows cannot appear.

Row numbers must start at one and increase by one. A synthetic sink cannot inherit the canonical step count.

The alternate-witness test must select the better candidate. Reversing candidate construction order must not change the result.

The nested-span test must keep the smallest informative row. It must still keep distinct call and return boundaries.

The truncation test must state that the canonical path is incomplete. It must not claim an invented display-stage count.

The terminal test must make controls visible. It must not emit raw escape or line-break characters from labels.

The size-bound test must fail with `PolicyRenderError::SerializedReportLimit` at the configured byte limit.

Serialized JSON before and after display-path attachment must compare equal. SARIF code flows must still match canonical witnesses.

Verbose human output must still show canonical witness IDs, steps, evidence references, and truncation data.

## Idempotence and Recovery

Source projection is deterministic and has no external side effect. Repeated test runs can reuse the normal Cargo target.

Do not reset, rebase, or switch this detached checkout without explicit authorization. Preserve the current Kotlin commit.

If a test exhausts disk space, stop it. Use `scripts/cleanup-bifrost-tmp.sh` in dry-run mode before any cleanup.

If display projection fails, omit only the optional display cache. Do not weaken or remove the canonical witness.

## Artifacts and Notes

The inspected master revision was `fcd830452` on 2026-08-13.

Current concise selection on master is:

    if let Some(witness) = finding.witnesses().first() {
        write_concise_witness(output, finding, witness)?;
    }

Current taint labels on master are `taint source`, `taint propagation`, and `taint summary boundary`.

The desired relay display is:

    #   Kind         Location        Code / symbol
    1   source       Foo.java:14:20  userInput()
    2   propagation  Foo.java:14:14  relay(...)
    3   sink         Foo.java:14:9   eval(...)

An explicit call or return row is valid when the semantic witness contains that boundary.

## Interfaces and Dependencies

In `crates/bifrost-policy/src/display_path.rs`, define crate-private bounded display types.

Provide a selector with this conceptual interface:

    pub(crate) fn select_taint_display_path(
        candidates: impl IntoIterator<Item = TaintDisplayCandidate>,
    ) -> Option<TaintDisplayPath>;

Provide a workspace-backed candidate projector with this conceptual interface:

    pub(crate) fn project_taint_display_candidate(
        workspace: &WorkspaceAnalyzer,
        origin: &SemanticLocator,
        sink: &SemanticLocator,
        witness_id: &WitnessId,
        witness: &SummaryWitness,
    ) -> Result<TaintDisplayCandidate, String>;

The exact arguments can include display-name fallbacks. They must not include raw file-system access.

`TaintDisplayPath` must expose its rows, selected witness identity, incompleteness, and meaningful omitted-stage count.

`PolicyFinding` must expose a crate-visible display-path accessor for `render/human.rs`. It must not expose this data through serialization.

Use only current workspace dependencies. Do not add a crate or an external dependency.

Plan revision note: Updated after implementation and validation. The user-selected branch enabled the complete issue 2090 change.
