# Preserve Rust scoped type references in inverse lookup

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Rust forward lookup resolves structured `self::module::Type`, imported-module `module::Type`, and local-module type paths in aliases and associated-type positions. Inverse lookup must return those exact reference sites rather than losing them to generic relative-module reconstruction or same-name declarations.

## Progress

- [x] (2026-08-14) Preserved the three production witnesses and filed/assigned #2131.
- [x] (2026-08-14) Replayed all three witnesses on clean master after #2130; each now has an exact, consistent inverse hit.
- [x] (2026-08-14) Added and validated a reduced regression covering all three structured path shapes and same-name near misses.
- [ ] Publish the regression, replay clean closure evidence, and close #2131.

## Surprises & Discoveries

- Observation: #2131 shares the multi-segment inverse-resolution defect fixed for #2130 rather than requiring another resolver change.
  Evidence: At clean Bifrost `b84a50cd756279eea37f7e1061db1197367a6ea8`, Diesel `StartFrame`, `AnsiSqlArrayComparison`, and `Posts` all replay as `consistent` with exact inverse ranges and actionable zero.

## Decision Log

- Decision: Add issue-specific regression coverage without another production-code change.
  Rationale: The shared fix already restores the production behavior. A focused reduction is still required to pin `self::`, imported module-qualified, and local-module paths independently with same-name decoys.
  Date/Author: 2026-08-14, Codex.

## Outcomes & Retrospective

Pending clean pushed-head closure evidence.

The focused regression, three relevant inverse routing controls, all 54 `brokk-bifrost-rust` tests, formatting, focused Clippy for `brokk-bifrost-rust` and `brokk-bifrost-analysis`, and the workspace dependency check pass at the implementation candidate.

## Context and Orientation

The shared implementation is in `crates/bifrost-rust/src/usage.rs::usage_reference_at`. The focused integration regression belongs in `tests/suite_issues/` and uses `InlineTestProject` plus forward definition lookup and authoritative inverse `UsageFinder` queries.

Baseline exact evidence is represented by Diesel sites `diesel/src/expression/functions/aggregate_expressions.rs` bytes 16605..16615, `diesel/src/sqlite/backend.rs` bytes 2373..2395, and `diesel_bench/benches/rust_orm_benches.rs` bytes 11527..11532.

## Plan of Work

Create one inline Rust project with a self-qualified alias path, an imported module-qualified associated type, a local-module alias path, and sibling same-name declarations. Resolve each terminal forward, require its exact expected target, then require the authoritative inverse query to contain the exact source range.

## Concrete Steps

Run from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_issues -- issue_2131_rust_scoped_type_inverse
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

## Validation and Acceptance

All three reduced sites must resolve forward to one expected declaration and round-trip to an exact inverse hit while the decoys remain unselected. The three production rows must replay `consistent`, exact, complete, and actionable zero on a clean pushed head.

## Idempotence and Recovery

Tests and ephemeral exact probes are safe to repeat. Preserve final reports and checksums under a new head-scoped directory in `/mnt/optane/tmp/bifrost-fird/`.

## Interfaces and Dependencies

No public API, dependency, or analysis epoch change is expected because the shared resolver-only fix is already published.
