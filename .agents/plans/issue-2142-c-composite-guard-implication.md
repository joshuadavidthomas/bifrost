# Prove C composite preprocessor guard implication

This ExecPlan is a living document maintained under `.agents/PLANS.md`. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

## Purpose / Big Picture

C callables declared for a set of build configurations must be visible from references whose active configuration is a subset of that declaration set. After this fix, Bifrost represents supported `defined`/identifier/negation/conjunction/disjunction guards structurally, recognizes De Morgan equivalence and sound implication, and still fails closed for unsupported or contradictory expressions.

## Progress

- [x] (2026-08-14) Reduced libarchive `match_owner_name_mbs` and `compression_unsupported_encoder` to Boolean equivalence/implication fixtures and filed self-assigned #2142.
- [x] (2026-08-14) Added the canonical structured Boolean guard form and sound implication check to both ordinary and callable-specific guard visibility.
- [x] (2026-08-14) Added low-level normalization/implication coverage and an InlineTestProject with equivalence, implication, contradictory, and unrelated cases.
- [x] (2026-08-14) Ran focused and broad gates and replayed both production rows from the clean pushed head. The focused tests, all 90 C/C++ crate tests, all seven callable-activation controls, formatting, focused Clippy, dependency validation, and both exact production replays pass.
- [x] (2026-08-14) Committed and pushed implementation commit `dc1b8c5a6`; preserved compact checksummed clean-head evidence. Issue closure follows the evidence-plan commit.

## Surprises & Discoveries

- Observation: exact equality is too strict even when no macro mutates.
  Evidence: `!defined(WIN32) || defined(CYGWIN)` is the negation of `defined(WIN32) && !defined(CYGWIN)`, but the current opaque expression plus outer negation representation treats them as different atoms.
- Observation: visibility needs implication, not only equivalence.
  Evidence: `!A || !B` at a fallback reference implies the declaration condition `!A || !B || !C`; requiring equal guard atoms hides a declaration that exists in every configuration reaching the reference.
- Observation: callable activation had a second exact-membership check after ordinary candidate visibility.
  Evidence: the first implementation made the equivalence fixture pass while the implication fixture still returned `no_definition`; routing both sites through one implication predicate made both pass without weakening contradictory or unrelated controls.

## Decision Log

- Decision: build a canonical negation-normal Boolean expression from tree-sitter nodes and use a conservative structural implication relation.
  Rationale: AST construction obeys the structured-analysis contract. Canonical sorting proves commutative equivalents, negation pushes through `&&`/`||`, and structural implication can prove the subset cases without an arbitrary-expression evaluator.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

Implementation commit `dc1b8c5a6` is pushed to `origin/master`. The clean release runner SHA-256 is `c000f93ce4ce5d594cfa7bde05e32455e9dbd4dadff28a882c8eebfa41ae0e3e`. Both issue-owned libarchive rows completed as resolved and consistent with exact inverse hits, actionable zero, no skipped or truncated targets, no candidate-limit overflow, no inverse-precision findings, and no file errors. The compact manifest is `/mnt/optane/tmp/bifrost-fird/final-dc1b8c5a/issue-2142-clean-replay-manifest-dc1b8c5a.jsonl` (SHA-256 `9c48d4025c77011ad41cdc4cc71fc6b54f04db25171ac2b546957c8169a51276`); its checksum inventory has SHA-256 `e95a1a7546e84254b8437b2417325869d9a3bdf16db6e8d723a0a91663cfda56`. The purpose is met.

## Context and Orientation

`crates/bifrost-cpp/src/graph/resolver.rs::PreprocessorGuard` currently stores composite expressions as `Expression(String)` or `NegatedExpression(String)`. `guard_requirements_hold_at_reference` uses exact `HashSet::is_subset`, and callable visibility calls that predicate. `simple_preprocessor_expression_guard` already receives the tree-sitter expression node and is the structured construction seam.

## Plan of Work

Add a canonical Boolean expression type with defined, undefined, truthy, falsy, opaque, negated-opaque, all, any, and constants as needed. Flatten same-kind conjunctions/disjunctions, sort and deduplicate operands, and push negation to atoms. Parse only tree-sitter `defined`, identifiers, parentheses, unary `!`, and binary `&&`/`||`; preserve unsupported subexpressions as opaque normalized AST-node text.

Extend reference visibility so the conjunction of active supported guards may structurally imply each required supported guard. Exact equality remains the fallback for opaque legacy guards. Keep macro-stability checks unchanged.

Add low-level canonicalization/implication tests and register an `InlineTestProject` regression covering De Morgan equivalence, disjunctive widening, and contradictory/unrelated controls.

## Concrete Steps

From `/mnt/optane/bifrost-fird`, run:

    cargo test -p brokk-bifrost-cpp boolean_guard
    cargo test --test suite_issues -- issue_2142
    cargo test --test suite_analyzers -- cpp_callable_activation_visibility
    cargo test -p brokk-bifrost-cpp
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Then rebuild `bifrost_reference_differential` in release mode and replay libarchive bytes 47678..47698 and 73435..73466 with `--cache-mode ephemeral`.

## Validation and Acceptance

Equivalence and implication positives must resolve; contradictory and unrelated controls must remain unresolved. Each production replay must be `completed`, `resolved`, `consistent`, inverse-exact, free of truncation and file errors, and exit with actionable zero.

## Idempotence and Recovery

Tests are read-only and replays use ephemeral caches. Preserve raw outputs until a compact checksummed clean-head manifest exists. Retry interrupted runs to a new revision-specific output path.

## Artifacts and Notes

The raw rows are in `/mnt/optane/tmp/bifrost-fird/final-3643963/c-target-complete-supplement-3643963-raw-ledger.jsonl`. Current exact failures are `/tmp/fird-libarchive-match-owner-current.jsonl` and `/tmp/fird-libarchive-compression-current.jsonl`.

Plan revision note (2026-08-14): Created after closing #2139 and reducing the remaining libarchive guarded-call rows.

Plan revision note (2026-08-14): Recorded implementation commit, complete local gates, and checksummed clean pushed-head production evidence.

## Interfaces and Dependencies

Change only shared C/C++ preprocessor guard representation and visibility. Update cache-size accounting for the new in-memory shape. Add no dependency, crate, schema, epoch, or identity change.
