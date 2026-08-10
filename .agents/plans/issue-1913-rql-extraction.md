# Extract the analyzer-independent RQL syntax crate

This ExecPlan follows `.agents/PLANS.md`. It is a living plan. Keep the
`Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes &
Retrospective` sections current during the work.

## Purpose / Big Picture

Move the analyzer-independent RQL syntax face into `brokk-bifrost-rql`.
This face parses S-expressions and JSON, builds the typed query intermediate
representation, validates it, formats S-expressions, and provides source
hover metadata. RQL users keep the same syntax and query results.

The new dependency direction is `brokk-bifrost-core` to
`brokk-bifrost-rql` to `brokk-bifrost-analysis`. The analysis crate keeps
execution because execution reads analyzer state, stores, and language
modules. A successful check is a featureless workspace build and test run with
the new crate in the dependency graph.

## Progress

- [x] (2026-08-10) Read `CLAUDE.md`, `.agents/PLANS.md`, and issue #1913.
- [x] (2026-08-10) Mapped the query directory and found one execution-coupled
  `EdgeFilter::matches` method.
- [ ] Create `crates/bifrost-rql/Cargo.toml` and its library root.
- [ ] Move the pure query modules and shared S-expression parser.
- [ ] Move the bounded registration-reference value types needed by the IR.
- [ ] Reconnect analysis execution and compatibility re-exports.
- [ ] Update workspace call sites and direct crate dependencies.
- [ ] Add the dependency graph rule and release inventory entry.
- [ ] Run all required featureless validation commands.
- [ ] Commit the skeleton, move, and graph/inventory changes in logical commits.

## Surprises & Discoveries

- Observation: `ir.rs` is almost pure syntax, but `EdgeFilter::matches` accepts
  `ReferenceEdgeRow` from analysis.
  Evidence: the method at the old `query/ir.rs` line 155 names
  `crate::analyzer::structural::reference_edges::ReferenceEdgeRow`.
- Observation: the query IR uses `ProtocolRef`, `ValueFlowPlanRef`, and
  `TaintResultRef`, which were defined in the analysis-owned registration
  context.
  Evidence: the old `query/ir.rs` imports them from
  `structural::analysis_context`, while the reference values themselves have
  no analyzer state.
- Observation: the query S-expression frontend uses the public shared parser
  in `crates/bifrost-analysis/src/sexp/`.
  Evidence: the old `query/sexp.rs` imports `crate::sexp::{Expr, ExprKind,
  ParseError, ParsedSexp, parse_sexp}`.

## Decision Log

- Decision: Move the complete pure query module set, including `source/`, to
  `bifrost-rql`.
  Rationale: these modules use core registries and third-party syntax helpers,
  but they do not read an analyzer, store, grammar, or language implementation.
  Date/Author: 2026-08-10, Codex.
- Decision: Move the shared S-expression parser and formatter to
  `bifrost-rql::sexp`, then re-export it from analysis for compatibility.
  Rationale: the RQL parser cannot depend on analysis, and the parser is
  schema-neutral syntax code already used by RQLP and Rune IR.
  Date/Author: 2026-08-10, Codex.
- Decision: Move the three bounded registration-reference value types with the
  syntax crate and re-export them from `analysis_context`.
  Rationale: the typed IR needs these values, but the values do not need the
  analyzer-owned registration sets. This preserves the existing analysis API
  while keeping the new crate independent of analysis.
  Date/Author: 2026-08-10, Codex.
- Decision: Replace the old inherent `EdgeFilter::matches` execution helper
  with an analysis-side function and update its execution caller.
  Rationale: Rust does not allow an inherent implementation for a type owned by
  another crate. The row matcher is execution code and must stay in analysis.
  Date/Author: 2026-08-10, Codex.
- Decision: Keep RQL schema and policy schema versions unchanged.
  Rationale: this change moves code only and does not alter accepted syntax or
  query meaning.
  Date/Author: 2026-08-10, Codex.

## Outcomes & Retrospective

At completion, record the files moved and kept, the number of updated RQL use
paths, dependency-check behavior, validation results, and any coupling that
remained in analysis. Note that crates.io bootstrapping and trusted publisher
configuration remain release-owner work before the next version release.

## Context and Orientation

The workspace has a foundation crate at `crates/bifrost-core/`. It owns the
language-independent analyzer data model, schema-version registry, and
normalized structural vocabularies such as kinds, roles, edges, occurrences,
materialization, and resolution. The large engine crate at
`crates/bifrost-analysis/` owns analyzers, stores, grammars, and query
execution.

The old RQL syntax module is
`crates/bifrost-analysis/src/analyzer/structural/query/`. Its pure modules are
`decode.rs`, `features.rs`, `ir.rs`, `json.rs`, `schema.rs`, `sexp.rs`,
`source/`, and their tests. `ir.rs` contains the public typed query model and
one method that directly matches an analysis reference-edge row. The latter
must remain in analysis.

The old shared S-expression syntax module is
`crates/bifrost-analysis/src/sexp/`. It contains the byte-spanned parser and
width-aware formatter. The new crate will expose the same public items under
`bifrost_rql::sexp`.

The new package is `brokk-bifrost-rql`, with Rust library name
`brokk_bifrost_rql`. Its only workspace dependency is
`brokk-bifrost-core`. Third-party dependencies move with the syntax code.
The analysis crate depends on the new package.

## Plan of Work

First add the workspace member and create the new manifest and library root.
Match the workspace edition, Rust version, license, repository, homepage,
description, readme, keywords, categories, and `publish = true` settings of
`brokk-bifrost-core`. Add only the third-party dependencies required by the
moved modules.

Next move the query directory and the shared S-expression directory into the
new crate. Adjust module roots and imports to use
`brokk_bifrost_core::...` and local RQL modules. Move the three pure bounded
reference value types needed by the IR. Keep the registration sets and all
registration validation that uses analyzer artifacts in
`analysis_context.rs`; make it use the moved reference types.

Then add analysis-side execution code for reference-edge filtering. Update
the structural matcher, planner, search, policy, MCP, LSP, facade, and test
imports to use `brokk_bifrost_rql` where they consume syntax types. Re-export
the RQL module and moved syntax items from analysis where old public paths
remain part of the compatibility surface. Re-export the shared S-expression
module from analysis.

Remove syntax-only third-party dependencies from analysis only when no other
analysis module uses them. Do not change dependency versions.

Finally update `scripts/check-workspace-dependencies.mjs` and its unit test
fixture. Add `brokk-bifrost-rql` to the expected package names and allow only
`brokk-bifrost-core`; test both the valid core dependency and invalid analysis
dependency. Add the new package to the published crate inventory in
`CONTRIBUTING.md`, with publication order after core and before analysis.

## Concrete Steps

Run all commands from the repository root:

    cd /home/jonathan/Projects/bifrost/.claude/worktrees/codex-rql-extraction

Inspect the cut and references:

    rg -n "structural::query|bifrost_analysis::.*query|EdgeFilter|ProtocolRef|ValueFlowPlanRef|TaintResultRef" crates src tests

Create and edit files with `apply_patch`. Use `git mv` only for pure moves if
the resulting index preserves the rename. Stage only files changed by each
logical commit.

Run the required featureless checks:

    cargo fmt
    node --test scripts/check-workspace-dependencies.test.mjs
    node scripts/check-workspace-dependencies.mjs
    cargo build --workspace

Use `cargo nextest run --workspace --no-fail-fast` when `cargo nextest` exists.
Otherwise run:

    cargo test --workspace

Then run:

    cargo clippy --workspace --all-targets -- -D warnings

Do not enable `nlp`. Do not run `--all-features` clippy. If a known
`suite_mcp_cli` or `suite_usages` test flakes under load, rerun that test once
in isolation and record both results.

## Validation and Acceptance

Acceptance requires all requested commands to pass, except for a documented
known load-only test flake that passes on its isolated rerun. The workspace
dependency checker must accept `brokk-bifrost-rql` with core as its only
workspace dependency and must reject analysis as a dependency. The featureless
workspace build, test suite, and workspace clippy command must pass.

The existing RQL unit and source-validation tests must run from the new crate.
The canonical JSON output for representative RQL inputs must remain equal to
the pre-move output. No RQL or policy schema version file or editor grammar
semantics may change.

## Idempotence and Recovery

File moves and import edits are repeatable after checking `git status`. If a
move needs correction, preserve user changes and use focused patches. Do not
use destructive checkout or reset commands. If the cut cannot satisfy the
new crate's dependency restriction, stop and record the exact coupling rather
than adding a dependency on analysis.

The new crate is publishable but is not bootstrapped on crates.io in this task.
The release owner must bootstrap it and configure its trusted publisher before
the next version release.

## Artifacts and Notes

The final handoff must include the logical commits, moved and kept files, the
new manifest dependencies, the RQL use-path update count, the dependency-check
rule, the inventory row, every validation result, and the release-owner note.

## Interfaces and Dependencies

The new crate must provide these public surfaces:

    brokk_bifrost_rql::CodeQuery
    brokk_bifrost_rql::schema
    brokk_bifrost_rql::sexp
    brokk_bifrost_rql::validate_query_source
    brokk_bifrost_rql::query_source_help_at

It must also provide the existing typed IR, parser, decoder, formatter, schema
registries, source diagnostics, policy selector helpers, bounded query limits,
and moved S-expression parser types without analyzer dependencies.

The analysis crate must continue to provide execution functions and result
types. It may re-export syntax types from `brokk_bifrost_rql` for old callers,
but execution code must not move into the new crate.

## Plan Revision Note

2026-08-10: Initial plan. Added after issue and source inspection. It records
the mixed `EdgeFilter` method, the shared S-expression parser dependency, and
the pure registration-reference value types required by the typed IR.
