# Keep VS Code policy reports compatible with the managed server

This ExecPlan is a living document. Keep the `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` sections current. Maintain this file in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

The VS Code policy command must accept the canonical report from the exact Bifrost binary in the same extension release. After this change, a schema-version-3 report with one finding appears in the policy results view. An unsupported schema produces an error that gives both the observed version and the one supported version. Shared Rust and TypeScript tests prevent either side from changing the contract alone. The release gate also exercises the packaged extension with its pinned binary before publication.

## Progress

- [x] (2026-08-12 12:05Z) Read issue #1997 and inspect the current branch, server report model, editor decoder, tests, and release jobs.
- [x] (2026-08-12 12:18Z) Define a shared schema-version-3 one-finding report artifact and check it in Rust and TypeScript.
- [x] (2026-08-12 12:18Z) Update the TypeScript types and decoder for schema version 3, including required version-3 fields.
- [ ] Add editor tests for all result states (completed: existing empty, incomplete, unsupported, failed, suppressed, and diagnostic tests pass with schema 3; remaining: make their schema-3 contract intent explicit where useful).
- [ ] Add a packaged-extension and pinned-binary release smoke test.
- [ ] Run focused Rust, TypeScript, script, formatting, and workflow checks.

## Surprises & Discoveries

- Observation: The issue branch started clean and one commit ahead of `origin/master`.
  Evidence: `git rev-list --left-right --count HEAD...origin/master` returned `0 1`.
- Observation: Report schema 3 adds required `execution` and `scope` fields. It can also add `diff`, `packs`, and `baseline` reviews.
  Evidence: `PolicyReportDocument::serialize` in `crates/bifrost-policy/src/report.rs` writes these fields.
- Observation: The real one-finding LSP response uses empty execution progress for a completed fast request.
  Evidence: The focused LSP test produced `total_elapsed_ms: 0`, empty stage and progress arrays, and null termination fields.

## Decision Log

- Decision: Support only the current report schema, version 3.
  Rationale: The managed binary and extension have one release version. A single schema prevents unsafe partial decoding.
  Date/Author: 2026-08-12 / Codex
- Decision: Use one committed JSON response artifact in both Rust and TypeScript tests.
  Rationale: A shared artifact crosses the language boundary and fails when the canonical Rust wire shape changes.
  Date/Author: 2026-08-12 / Codex

## Outcomes & Retrospective

The editor now consumes schema 3 and rejects other versions with a precise version message. One shared artifact crosses the Rust LSP and TypeScript decoder tests. The release-level packaged smoke remains.

## Context and Orientation

`crates/bifrost-policy/src/report.rs` owns `PolicyReportDocument`, the canonical Rust report and its JSON serializer. `crates/bifrost-lsp/src/lsp/server.rs` returns this document from the `bifrost/runPolicy` request. `editors/vscode/src/rql_policy.ts` defines the TypeScript response model, validates unknown server data, and supplies data to the editor results view. `editors/vscode/test/rql-policy.test.ts` tests this decoder and its display helpers. `.github/workflows/release.yml` packages the VSIX file and controls publication.

A VSIX is a ZIP archive that contains the released VS Code extension. A pinned binary is the Bifrost release named by `bifrost.binaryVersion` in the extension manifest. A contract test proves that two components agree on the same data shape.

## Plan of Work

First, add a small canonical one-finding response artifact below `tests/fixtures/policy-report/`. Add a Rust test near the LSP policy request tests. It must run the real request, normalize only values that change between runs, and compare the complete result to the artifact. The TypeScript test must load the same artifact and pass it through the exported decoder and results helpers.

Second, update `editors/vscode/src/rql_policy.ts`. Set the supported schema constant to 3. Model and validate the required schema-version-3 `execution` and `scope` fields. Preserve optional `diff`, `packs`, and `baseline` values because the current results view does not interpret them. Keep strict validation for every field that the view reads. Split schema mismatch reporting from other malformed responses so the user sees the observed and supported versions.

Third, extend `editors/vscode/test/rql-policy.test.ts`. Cover a real one-finding artifact, a complete empty report, incomplete and unsupported runs, failed runs, suppressed findings, report and run diagnostics, and a deliberately unsupported schema. Confirm that the mismatch message contains both versions.

Finally, add a release smoke script under `scripts/`. It must inspect the packaged VSIX manifest, obtain its pinned binary, start that exact binary in LSP mode against a temporary TypeScript workspace, initialize the protocol, call `bifrost/runPolicy`, and pass the response through a decoder built from the packaged extension source. Add the smoke after packaging and before artifact upload in `.github/workflows/release.yml`. Keep all temporary files below the system temporary directory and stop the server on every exit path.

## Concrete Steps

Run all commands from the repository root `/Users/dave/.codex/worktrees/8f27/bifrost`.

Inspect focused changes with:

    git diff -- .agents/plans/issue-1997-vscode-policy-report-contract.md editors/vscode crates/bifrost-lsp tests/fixtures scripts .github/workflows

Run the Rust contract test with a focused test name. Run editor checks with:

    cd editors/vscode
    npm test

Run script tests with `node --test` for each new or changed script test. Run Rust formatting with:

    cargo fmt --check

## Validation and Acceptance

The shared artifact must contain report schema 3 and one unsuppressed finding. The Rust test must prove that a real `bifrost/runPolicy` request produces its complete shape. The TypeScript test must accept it and project that finding for display.

Editor tests must accept complete zero-finding, incomplete, unsupported, failed, suppressed, and diagnostic-bearing reports. A report with schema 99 must fail. `runRqlPolicy` must show an error that includes `99` and `3`.

The release smoke must package a VSIX, read its `bifrost.binaryVersion`, use that exact binary, receive one finding, and run the packaged decoder and results projection without error. Publication jobs must depend on this smoke through the validated VSIX artifact.

## Idempotence and Recovery

Tests and artifact comparisons are safe to repeat. The release smoke must use a unique temporary directory and remove it through a process-exit cleanup handler. It must not edit the extension manifest or the downloaded binary. If artifact generation fails, keep the last reviewed artifact and print the generated JSON to a temporary file for comparison.

## Artifacts and Notes

Issue #1997 reports this incompatible pair:

    Rust PolicyReportDocument::SCHEMA_VERSION = 3
    TypeScript PolicyReport.schema_version = 2

The old editor error says only that an updated server is required. The new error must identify the actual contract mismatch.

## Interfaces and Dependencies

In `editors/vscode/src/rql_policy.ts`, export one numeric `SUPPORTED_POLICY_REPORT_SCHEMA_VERSION` constant with value 3. Keep `isRqlPolicyResponse(value: unknown): value is RqlPolicyResponse` as the decoder entry point. Add a small schema-version inspection helper only if it keeps `runRqlPolicy` clear and gives the precise mismatch message.

Use Node built-ins for artifact loading and smoke orchestration. Do not add an npm dependency. Use existing LSP test support in `crates/bifrost-lsp/tests/bifrost_lsp_server.rs` for the Rust producer test.

Revision note: Created the plan after reading issue #1997 and the current Rust, TypeScript, CI, and release paths.

Revision note: Updated progress after the schema-3 decoder and shared Rust/TypeScript contract tests passed.
