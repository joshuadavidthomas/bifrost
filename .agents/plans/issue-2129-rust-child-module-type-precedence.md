# Prefer physical child-module types during Rust bare-name lookup

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It is maintained in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Rust bare type lookup in a physical child module can currently select an identically named type from its parent module. After this change, a type reference written in `child.rs` resolves to the declaration in that physical child module, even when the surrounding `impl` is logically owned by the parent's same-named destination type. Explicitly qualified parent references and unrelated same-name declarations retain their existing identities.

## Progress

- [x] (2026-08-14) Preserved the exact Sway witness and filed/assigned #2129.
- [x] (2026-08-14) Added a reduced `InlineTestProject` regression with parent, child, and sibling same-name declarations; it reproduced the exact wrong parent identity before the fix.
- [x] (2026-08-14) Traced the forward precedence error to the unbound bare-name arm and made physical current-module candidates precede the logical enclosing-owner fallback.
- [x] (2026-08-14) Passed the focused issue test, six module/import/Cargo-target controls, all 54 `brokk-bifrost-rust` tests, formatting, focused isolated Clippy, dependency validation, and diff checks.
- [x] (2026-08-14) Published `88303400b5b04cd3d0da72c680a7799d42fa6f21`, rebuilt the clean release runner, and replayed the exact Sway witness as one resolved/consistent exact inverse hit with actionable zero.
- [ ] Close #2129 with the committed fix and clean replay evidence.

## Surprises & Discoveries

- Observation: The incorrect target is a valid indexed declaration rather than an unresolved boundary.
  Evidence: Sway `forc-pkg/src/source/path.rs` bytes `2782..2788` resolves to `forc_pkg.source.Pinned` in `source/mod.rs`; the physically local declaration is `forc_pkg.source.path.Pinned` in the reference file.
- Observation: The current-module filter was already correct after #2031; it was never reached for this site.
  Evidence: the `Unbound` type arm called `resolve_in_enclosing_scopes` first. The enclosing method's logical FQN is beneath the parent destination type, so the generic shrinking-scope lookup returned `source.Pinned` before `rust_current_module_candidates` could consider `source/path.rs`.

## Decision Log

- Decision: Pin exact declaration identity and source path in the regression.
  Rationale: Both declarations have the same terminal name, so terminal-only assertions would preserve the defect.
  Date/Author: 2026-08-14, Codex.
- Decision: Reorder only the unbound bare-name arm; preserve explicit and glob import precedence.
  Rationale: Rust physical module scope determines an unqualified local declaration. Explicit imports remain authoritative, and the existing glob arm already lets a physical current-module item win.
  Date/Author: 2026-08-14, Codex.

## Outcomes & Retrospective

The implementation is complete and published. Unbound bare Rust names now consult declarations in their physical syntax module before the generic logical enclosing-scope fallback. The reduced regression proves the child type wins while an explicit `super::Pinned` still resolves to the parent. The clean production replay resolves the Sway parameter type to `forc_pkg.source.path.Pinned`, finds the exact inverse range, and reports no actionable discrepancy.

## Context and Orientation

Rust forward resolution lives in `crates/bifrost-analysis/src/analyzer/usages/get_definition/rust.rs`. Bare type lookup first considers explicit imports, enclosing scopes, and `rust_current_module_candidates`. #2031 already made current-module candidate membership follow physical syntax scope. #2129 concerns precedence when an enclosing logical owner has the same terminal name as a declaration in the physical child module.

The production witness is `/mnt/optane/tmp/bifrost-fird/final-3643963/exact/pinned.jsonl`, SHA-256 `400a4893af16b5a218c5f42091d6963a17c952fdd49285aa2c9fcc753be79f99`. It pins Bifrost `3643963d05adac938932760cb4a745050f4710e0` and Sway `545b4e0fa7b1cc4c8c485998fd8674f2407a4267`.

The clean closure replay is `/mnt/optane/tmp/bifrost-fird/final-88303400/exact/issue-2129-pinned.jsonl`, SHA-256 `36f06908b0b5dea31bb7b42d86afe5e1070477bb1b52af94b8892d11eb1c0007`. It pins clean Bifrost `88303400b5b04cd3d0da72c680a7799d42fa6f21` and contains one resolved, consistent, exact inverse hit with no diagnostics or limit invalidation.

## Plan of Work

Add a behavior-focused issue test using `InlineTestProject`. Model a parent module and physical child file that both declare `Pinned`, plus a sibling decoy. Inside the child, implement a conversion from bare `Pinned` to explicitly qualified `super::Pinned`; assert that the parameter type resolves only to the child declaration and the destination resolves only to the parent declaration.

Trace which precedence tier selects the parent declaration. Correct the Rust-specific caller using existing AST scope and Cargo module-route structures. Do not weaken explicit-import precedence, invent text scanning, or modify declaration identities.

## Concrete Steps

Run from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_issues -- issue_2129_rust_child_module_type_precedence
    cargo test -p brokk-bifrost-rust
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-rust -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    cargo build --release --bin bifrost_reference_differential

## Validation and Acceptance

The reduced fixture must resolve the bare child-module type and the qualified parent type to their exact declarations while excluding the sibling decoy. Existing Rust current-module, import-precedence, and Cargo-target controls must remain green. The clean published-head exact Sway replay must be resolved and consistent with an exact inverse hit and actionable zero.

## Idempotence and Recovery

Focused tests and ephemeral exact replays are safe to repeat. Preserve the immutable baseline artifact and write new evidence beneath a head-scoped directory in `/mnt/optane/tmp/bifrost-fird/`. Commit only #2129 files on the current branch and push directly to `origin/master`.

## Interfaces and Dependencies

No public API, dependency, or analysis epoch change is expected unless the investigation proves that persisted facts must change.
