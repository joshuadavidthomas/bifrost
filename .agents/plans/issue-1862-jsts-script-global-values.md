# Index JS and TS script values as program globals

This ExecPlan is a living document. Maintain it under `.agents/PLANS.md`.

## Purpose / Big Picture

Two JavaScript or TypeScript script files share one program scope. Today, a function in one script can use a function from another script. It cannot use a plain top-level value from that script. After this change, bare references to top-level scalar and destructured script values will resolve across files. Module values will stay file-private.

## Progress

- [x] (2026-08-12T22:07Z) Confirmed issue #1862 and the file-qualified field identity in both declaration walks.
- [x] (2026-08-12T22:07Z) Confirmed that `js_program_is_external_module` gives the shared script-versus-module decision.
- [x] (2026-08-12T22:07Z) Added failing JavaScript and TypeScript behavior tests with module and local-shadow near misses.
- [x] (2026-08-12T22:07Z) Emitted bare field identities only for program-scope script values, including destructured binders.
- [x] (2026-08-12T22:07Z) Bumped JavaScript and TypeScript store epoch salts. The existing epoch mechanism hashes each changed salt.
- [x] (2026-08-12T22:07Z) Ran focused tests, adjacent script and module tests, formatting, and targeted Clippy.
- [x] (2026-08-12T22:07Z) Committed the implementation as `4f8abee7`; delivery will push this plan closure with it.

## Surprises & Discoveries

- Observation: Top-level functions and function-valued variables already use bare identities. Plain fields alone use `<file>.<name>`.
  Evidence: `visit_js_variable_statement` and `visit_ts_value` select `file_scoped_field_fq` only for `CodeUnitType::Field` without a parent.

- Observation: Ambient `declare global` values need the program identity even though the containing file is a module.
  Evidence: The TypeScript visitor already gives `internal_module` named `global` special program-wide meaning.

## Decision Log

- Decision: Use `js_program_is_external_module` during indexing.
  Rationale: Forward lookup and inverse usage already use this structured test. One test keeps script and module meaning equal.
  Date/Author: 2026-08-12 / Codex

- Decision: Change destructured top-level script binders with scalar binders.
  Rationale: Both forms create program-scope values. Giving them different identities would keep the same defect for patterns.
  Date/Author: 2026-08-12 / Codex

- Decision: Pass the top-level identity function to shared visitors.
  Rationale: The extra argument keeps one implementation for scalar and destructured fields. It avoids a mode struct or duplicate visitor.
  Date/Author: 2026-08-12 / Codex

## Outcomes & Retrospective

JavaScript and TypeScript scripts now publish scalar and destructured fields under bare program identities. Module fields remain file-scoped. Local bindings still shadow program globals. The tests and targeted quality checks pass. The implementation is committed as `4f8abee7` and is ready for delivery.

## Context and Orientation

`crates/bifrost-js-ts/src/javascript.rs` and `crates/bifrost-js-ts/src/typescript.rs` create declaration `CodeUnit` records. A `CodeUnit` qualified name is the key used by cross-file bare-name lookup.

`crates/bifrost-js-ts/src/model.rs` contains shared identity helpers. `file_scoped_field_fq` creates `<file>.<name>`. A program-global field needs a one-segment member identity for `<name>`. Nested fields and module fields must keep their existing identities.

`crates/bifrost-js-ts/src/syntax.rs` provides `js_program_is_external_module`. It reads the syntax tree. An import, export, `require`, or CommonJS export makes a module. A file without these forms is a script.

`crates/bifrost-analysis/src/analyzer/store/epoch.rs` hashes per-language salts into persisted cache keys. This change alters JavaScript and TypeScript identities, so both salts must change.

## Plan of Work

Add one issue test in `tests/suite_issues`. Build two JavaScript scripts and two TypeScript scripts with a plain scalar and a destructured value. Resolve each bare reference from the other script. Also build module declarations and verify that an unrelated script cannot resolve them.

Compute the script decision once in each top-level parse function. Pass it to variable visitors. In `crates/bifrost-js-ts/src/model.rs`, extend the shared binder helper so a top-level program binder can use a bare member identity. Keep nested and module binders file-qualified.

Change scalar field construction in both dialect visitors with the same rule. Remove the obsolete limitation comment from `jsts_script_global_bare_candidates`.

Append a dated issue #1862 marker to both language salts. Add or update an epoch test if an existing historical-epoch pattern applies without broad test setup.

## Concrete Steps

Run commands from `/mnt/optane/bifrost-fird`.

    cargo test --test suite_issues issue_1862_jsts_script_global_values -- --nocapture
    cargo test --test suite_symbols javascript_bare_name_resolves_through_the_shared_script_global_scope -- --nocapture
    cargo fmt --all
    cargo clippy -p brokk-bifrost-js-ts --all-targets -- -D warnings
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings
    cargo clippy --test suite_issues -- -D warnings

The new test must fail before implementation and pass after it. The existing script-global test must stay green.

## Validation and Acceptance

Both dialects must resolve scalar and destructured values from another script. Each definition must have the bare qualified name and the declaring path. A module value must stay unresolved from a script. A local value must still win over a shared global.

## Idempotence and Recovery

The tests use temporary workspaces. Repeated runs are safe. The epoch change invalidates old cache rows without deleting them. Do not change branches or remove user data.

## Artifacts and Notes

The current failing identity is:

    Angular.js.isDefined

The required script identity is:

    isDefined

The module identity remains file-scoped.

## Interfaces and Dependencies

Do not add dependencies. Reuse `js_program_is_external_module`, `FqName`, `SegmentKind::Member`, and `js_ts_segment`. Keep the `CodeUnitType::Field` kind. Change only its top-level script name identity.

Plan update note: Created this plan after confirming issue #1862 and its shared JavaScript and TypeScript indexing seam.

Plan update note: Implemented program-scope value identities, added both dialect regressions, bumped both epochs, and recorded passing validation.

Plan update note: Recorded implementation commit `4f8abee7` and closed the plan before delivery.
