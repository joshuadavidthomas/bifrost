# Restore Rust inverse coverage through raw-identifier module routes

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

The Rust definition resolver can identify a physical nominal declaration through grouped imports and module re-exports whose source modules use raw identifiers such as `mod r#struct;`, but the inverse usage query cannot traverse that physical module edge. After this change, ordinary structured type positions round-trip through raw-identifier module routes while unrelated same-name declarations, lexical shadows, and other Cargo targets remain excluded.

## Progress

- [x] (2026-08-14 02:20Z) Completed the clean published-head target-complete supplement for all six previously truncated Rust repositories: 8,894 distinct targets queried, zero skipped targets, zero target truncation, and zero file errors.
- [x] (2026-08-14 02:25Z) Preserved the 1,337-row immutable ledger, identified 53 type/import-position forward-inverse misses, searched open issues, and filed/assigned #2128 with an exact Sway replay.
- [x] (2026-08-14 03:10Z) Added a reduced `InlineTestProject` regression spanning a physical re-exported nominal type, an indexed type alias, grouped imports, representative type positions, and same-name near misses.
- [x] (2026-08-14 03:35Z) Corrected raw-identifier normalization in persisted and live Cargo module routes, added extractor/usage-walk pins, and rolled the Rust analysis epoch.
- [x] (2026-08-14 04:45Z) Ran the full `brokk-bifrost-rust` suite (54 tests), focused analysis walk and issue integration tests, formatting, focused isolated Clippy, and the workspace dependency check; all passed.
- [x] (2026-08-14 04:05Z) Rebuilt the release runner and completed a dirty-head three-repository target-complete comparison. It cleared 31 raw-module-owned rows and retained 22 unrelated residuals with zero skipped or truncated targets.
- [x] (2026-08-14 05:20Z) Published `a90be07345102840f153bbe09b03ba09b5463aa4` to `origin/master`, rebuilt the clean release runner, replayed all 31 owned Sway rows, preserved a checksummed disposition manifest, and closed #2128 with evidence.

## Surprises & Discoveries

- Observation: The corrected #2035 implementation removed 275 forward/inverse findings from the diagnostic target-complete run, including the OpenDAL enum-variant regression; 103 forward/inverse findings remain on the corrected head.
  Evidence: The corrected raw ledger has 103 `forward_inverse` rows versus 378 in the discarded diagnostic ledger.
- Observation: At least 53 residual rows are structured type/import syntax rather than macro token trees or value calls.
  Evidence: The rows cover `reference_type`, `generic_type`, `type_arguments`, `trait_bounds`, `parameter`, `impl_item`, struct expression/pattern, use-list, and scoped type syntax across Sway, Meilisearch, and Diesel.
- Observation: The exact Sway witness has one clean forward target and no inverse diagnostic.
  Evidence: `TyStructDecl` at bytes `3622..3634` resolves to `sway_core.language.ty.declaration.struct.TyStructDecl`; exact replay is `missing` with one queried target, no truncation, and no diagnostic.
- Observation: The missing candidate edge begins at a raw-identifier module declaration, not at the grouped import or type-position matcher.
  Evidence: For the Sway witness, forward bare lookup found the exact physical identity while inverse seed resolution was unresolved. A reduced `mod r#struct; pub use r#struct::*;` fixture produced no candidate consumer until Cargo route extraction canonicalized `r#struct` to `struct`.
- Observation: The original 53-row type/import grouping contained more than one root cause.
  Evidence: The current dirty-head target-complete replay cleared 31 Sway rows but retained 22: three other Sway identities, eight Meilisearch identities, and eleven Diesel identities. Six Meilisearch `async-openai` rows are in detached files whose `mod` declarations are commented out. The issue title and acceptance set were narrowed on GitHub rather than crediting those survivors to this fix.
- Observation: The primary exact Sway witness is repaired without changing its forward identity.
  Evidence: `TyStructDecl` at bytes `3622..3634` is `consistent`, targets `sway_core.language.ty.declaration.struct.TyStructDecl`, has an exact inverse hit, and reports actionable zero in `/mnt/optane/tmp/bifrost-fird/issue-2128-dirty-exact/ty-struct-decl.jsonl` (SHA-256 `de3f1fff755c3169471f878a34385eb7163dfc9fe73ef1c818f2bcfc58007526`).

## Decision Log

- Decision: Treat macro token-tree references as outside #2128.
  Rationale: Matcher-aware macro namespace resolution is already owned by #1895, and combining it with ordinary parsed type positions would obscure two different structured routes.
  Date/Author: 2026-08-14, Codex.
- Decision: Require exact physical-declaration and alias identity in tests, not terminal-name equality.
  Rationale: The corpus contains same-name declarations and re-export layers; terminal matching would hide the identity loss that produced the differential.
  Date/Author: 2026-08-14, Codex.
- Decision: Normalize raw module names at both Cargo-route extraction paths and invalidate cached Rust facts.
  Rationale: Rust raw identifiers are alternate source spellings of the canonical identifier. Persisting `r#struct` constructs a nonexistent physical path and prevents the structured import graph from reaching `struct.rs`; warm route rows retain that bad spelling without an epoch rollover.
  Date/Author: 2026-08-14, Codex.
- Decision: Scope #2128 to the 31 production rows actually cleared by raw-module normalization.
  Rationale: The target-complete before/after comparison is a stronger ownership boundary than the initial syntax-shape grouping. The 22 surviving rows require separate forward-identity, detached-file, alias, import-site, or scoped-type analysis.
  Date/Author: 2026-08-14, Codex.

## Outcomes & Retrospective

#2128 is complete and closed. Raw module identifiers are canonicalized in both persisted and live Cargo-route extraction, and the Rust epoch invalidates route facts written with the old spelling. The clean published-head Sway replay queried all 1,513 targets with no skipped or truncated targets. All 31 owned rows have resolved forward targets and exact inverse hits: 28 are `consistent` references and three are `editor_only` imports. The 22 surviving rows from the initial broad grouping were not credited to this fix and remain in the campaign residual audit.

## Context and Orientation

Forward Rust lookup lives in `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs`. Inverse lookup is coordinated by `crates/bifrost-analysis/src/analyzer/usages/rust_graph.rs`; candidate discovery and AST matching are in `rust_graph/extractor.rs`, with shared seed and path resolution in `crates/bifrost-rust/src/usage.rs` and `crates/bifrost-rust/src/graph/resolver.rs`.

The clean baseline target-complete report is `/mnt/optane/tmp/bifrost-fird/final-84bd058f/rust-target-complete-84bd058f.jsonl`, SHA-256 `39b480783d65dbbd8b637ed6ca75951716e94062a1ba6eb32454d257b666c630`. Its raw ledger is `/mnt/optane/tmp/bifrost-fird/final-84bd058f/rust-target-complete-84bd058f-raw-ledger.jsonl`, SHA-256 `00f4ab1f74412218b0482335873152ee2751519b1e850a912e91f7fe722e9c12`.

The dirty-head three-repository comparison is `/mnt/optane/tmp/bifrost-fird/issue-2128-dirty-three-target-complete.jsonl`, SHA-256 `32a6c539e88d3932a2bcbdb87c58946cb31f57c8c1b0eddcf9dafd2401dbbbd9`. Its raw ledger is `/mnt/optane/tmp/bifrost-fird/issue-2128-dirty-three-target-complete-raw-ledger.jsonl`, SHA-256 `a89d7c2e3cc6b8228ccc32322ebb6300c878e8c3477dd401ddc7e55bcd342654`.

## Plan of Work

Add one behavior-focused issue-suite test using the shared inline project harness. Obtain each target through the public exact forward resolver, then query inverse usages and assert exact path/byte recovery for a grouped re-exported nominal type and an indexed type alias. Include an unrelated same-name declaration and assert it is not returned.

Determine whether the loss occurs in seed construction, candidate-file discovery, or AST matching. Correct the smallest shared structured layer. Do not scan source text as a substitute for imports or tree-sitter nodes. Preserve namespace, visibility, lexical-shadowing, Cargo-target, and bounded-candidate behavior.

Run the focused test and nearby Rust import, alias, definition, and usage controls; then run the Rust crate tests, formatting, focused Clippy, dependency validation, and differential check. Rebuild the release runner from a committed clean head and replay all issue-owned production rows with `--cache-mode ephemeral`.

## Concrete Steps

Run from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_issues -- issue_2128_rust_reexported_type_inverse
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

## Validation and Acceptance

The focused test must prove exact inverse recovery for every asserted forward target and reject the decoy. The production replay set must be resolved and consistent, with exact inverse path/byte hits and actionable zero. Any row shown to be an invalid forward identity or an already-owned unsupported boundary must be recorded separately rather than counted as fixed.

## Idempotence and Recovery

Tests and ephemeral exact replays are safe to repeat. Preserve the immutable full report and raw ledger. Write replay output to new head-scoped paths under `/mnt/optane/tmp/bifrost-fird/`. Commit only #2128 files on the current branch, push directly to `origin/master`, and close the issue only after clean published-head evidence exists.

## Artifacts and Notes

Exact Sway witness: `/mnt/optane/tmp/bifrost-fird/final-84bd058f/exact/17399bbbcb0241bb.jsonl`, SHA-256 `a4ca0968afb35b2c8700a1961d825a42e568a535042b5b4921c376599f1bf577`.

Clean closure report: `/mnt/optane/tmp/bifrost-fird/final-a90be073/issue-2128-sway-target-complete-a90be073.jsonl`, SHA-256 `4a5672681de89efbed478dc853dc37c502bd5d220312e7c07b17ae041f384bcf`.

Clean 31-row disposition manifest: `/mnt/optane/tmp/bifrost-fird/final-a90be073/issue-2128-owned-rows-a90be073.jsonl`, SHA-256 `9b3068d763c5b8e3bc75c41a23d90f90f05e57dcaff8060a3d2d09a2eb440533`.

Clean release runner SHA-256: `730db6cc2ab56c06ea1608355fd9868959ce8ef352f48e14813a204631f475f3`.

## Interfaces and Dependencies

No public API or new dependency is expected. An analysis epoch bump is required only if persisted Rust facts change.
