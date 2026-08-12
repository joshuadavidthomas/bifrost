# Render concise witness paths in policy output

This ExecPlan is a living document. Maintain it under `.agents/PLANS.md`. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

## Purpose / Big Picture

The default human policy report currently hides the path that explains a taint or other path-based finding. After this change, each concise finding with retained witnesses shows one short, ordered source-to-sink path. Each row includes the typed step kind, a clickable source location when present, and a terminal-safe label. The concise report states when it omits steps or alternate paths. JSON, SARIF, non-path findings, and verbose audit output stay unchanged.

## Progress

- [x] (2026-08-12 13:45Z) Read issue 1998, fetched the remote, and inspected the human renderer and policy tests.
- [x] (2026-08-12 14:02Z) Added the bounded primary-witness presenter to `crates/bifrost-policy/src/render/human.rs`.
- [x] (2026-08-12 14:08Z) Added focused rendering tests for ordering, escaping, truncation, alternate paths, ANSI output, size limits, and audit-data exclusion.
- [x] (2026-08-12 14:15Z) Ran formatting and focused featureless tests. All selected tests passed.
- [x] (2026-08-12 14:21Z) Reviewed the diff and ran the built-in code-smell pack. The scan completed, with one unrelated active finding in `crates/bifrost-policy/src/taint_policy.rs:1246`.

## Surprises & Discoveries

- Observation: Canonical witnesses already contain ordered typed steps, optional locations, terminal labels, and lower-bound truncation counts.
  Evidence: `BoundedWitness::steps` preserves construction order, while `write_finding` already renders that order in verbose output.

- Observation: Taint witnesses retain source and propagation steps, but they do not retain a typed sink step.
  Evidence: `project_taint_witnesses` maps seed, edge, and summary-gap steps. The canonical finding primary location is the sink, so concise output adds it as the final display row.

- Observation: The full built-in policy scan remains slow and has one unrelated active finding.
  Evidence: The first run took about 32 seconds. A warm repeat took about 12 seconds. Both completed without diagnostics. The active finding is `bifrost.performance.sort-in-loop` at `crates/bifrost-policy/src/taint_policy.rs:1246`. Open issue #1452 tracks self-repository `run_policy` latency.

- Observation: Focused Clippy validation is clean.
  Evidence: `cargo clippy -p brokk-bifrost-policy --all-targets -- -D warnings` completed without warnings.

## Decision Log

- Decision: Present the first canonical witness as the primary witness.
  Rationale: The canonical report owns deterministic witness selection and order. The renderer must not create a second ranking policy.
  Date/Author: 2026-08-12 / Codex

- Decision: Add a renderer-only step limit and report all renderer and canonical omissions as a lower bound.
  Rationale: A retained witness can be much larger than a useful terminal explanation. A lower bound stays correct when upstream evidence is also truncated.
  Date/Author: 2026-08-12 / Codex

- Decision: Add the taint finding's primary location as a final sink display row.
  Rationale: The retained witness ends at the propagation source of its last edge. The finding primary is the canonical sink location and completes the human source-to-sink explanation without changing JSON or SARIF evidence.
  Date/Author: 2026-08-12 / Codex

## Outcomes & Retrospective

The concise human renderer now shows one ordered witness with at most 12 rows. Taint paths end with a clickable sink row. Missing locations use a dash. Labels and paths use the existing terminal escaping. The report gives exact alternate counts when it can and lower bounds when canonical retention was truncated. Focused unit, policy-rendering, and model-backed taint tests pass. JSON, SARIF, verbose audit output, and non-path concise density remain unchanged.

## Context and Orientation

`crates/bifrost-policy/src/render/human.rs` writes both concise and verbose human reports. `write_concise_finding` currently writes only severity, primary location, an optional symbol, and the message. `write_finding` writes every retained witness and its audit metadata. A witness is a `BoundedWitness` from `crates/bifrost-policy/src/finding.rs`. It contains ordered `WitnessStep` values. Each step has a `WitnessStepKind`, an optional `PolicySourceLocation`, a label, and evidence references.

The canonical report can retain more than one witness. It can also state that it omitted witnesses. An individual witness can state that it omitted steps. Concise output must show one primary path and must not show witness IDs, evidence IDs, hashes, or other audit data.

## Plan of Work

In `crates/bifrost-policy/src/render/human.rs`, call a new concise witness writer after the finding message. If the finding has no witness, write nothing new. If it has witnesses, select the first witness and write at most a fixed number of its ordered steps. Use the existing `witness_step_kind`, `write_location`, and `escape_terminal_text` functions. Give missing locations a stable dash marker. State a lower bound when the selected witness or the renderer omits steps. State the alternate-path count after the table, including the report's lower bound for witnesses omitted before rendering. Direct users to `--verbose` for complete retained evidence.

Add unit tests next to the renderer for exact table text, missing locations, escaped labels and paths, step truncation, and alternate witness counts. Extend the integration rendering tests where useful to prove non-path findings keep their current density and ANSI mode changes only the severity marker.

## Concrete Steps

Work from `/Users/dave/.codex/worktrees/8a4b/bifrost`.

Edit the renderer and tests, then run:

    cargo fmt
    cargo test --test suite_bench_policy policy_rendering
    cargo test -p brokk-bifrost-policy render::human::tests

The focused tests must pass without the `nlp` feature. Do not start model downloads or semantic index threads.

## Validation and Acceptance

A three-step witness must render source, propagation, and sink-equivalent terminal steps in stored order. Each location must use `path:line:column`. Labels must use the terminal escape convention. The output must contain no witness ID, evidence-reference ID, finding ID, semantic hash, or projection hash.

If the primary path has more steps than the concise limit, the report must say that it omitted at least the known count. If canonical witness retention was already truncated, include its lower bound. If more witnesses exist or were omitted, state the alternate-path count without printing those paths. The verbose output must remain byte-for-byte governed by its existing writer. JSON and SARIF writers must not change.

## Idempotence and Recovery

Formatting and tests are safe to repeat. The change has no data migration and no external side effect. If a size-bound test fails, reduce fixed presentation text or adjust the test bound without weakening `BoundedWriter` enforcement.

## Artifacts and Notes

The intended concise shape is:

    [warning]  Foo.java:14:9
        Attacker-controlled input reaches eval

        #  Kind         Location          Code / symbol
        1  source       Foo.java:14:20    userInput()
        2  propagation  Foo.java:14:14    relay(...)
        3  violation    Foo.java:14:9     eval(...)

    summary: 1 active finding; 0 suppressed findings; 1 complete policy run

## Interfaces and Dependencies

Do not add a dependency. Keep the helper private to `render::human`. It must accept the existing canonical report types and write through `BoundedWriter`, so serialized-size limits and I/O error mapping remain active.

Plan revision note: Created the plan after the initial issue and code inspection. It records the first-witness and bounded-step decisions before implementation.

Plan revision note: Updated after implementation and validation. It records the synthetic taint sink display row, test results, and the unrelated policy-check finding and latency.

Plan revision note: Added the final focused Clippy result before the checkpoint commit.
