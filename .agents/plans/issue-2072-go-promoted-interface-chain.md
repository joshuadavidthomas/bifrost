# Resolve Go methods promoted through embedded struct and interface chains

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

After this change, finding usages of a Go interface method will include calls made through a concrete struct that embeds another struct which in turn embeds the interface. This is ordinary Go method promotion: a call such as `outer.Get()` may legally dispatch to `Provider.Get` even though `Outer` does not declare `Get` itself. The focused issue test and the eight recorded Hugo/Harness corpus sites demonstrate the behavior end to end.

## Progress

- [x] (2026-08-14 05:10Z) Located the existing embedded-member index and nearest-depth member resolver in `crates/bifrost-go/src/graph/resolver.rs`.
- [x] (2026-08-14 05:22Z) Added same-package precedence coverage and a failing narrow-scope cross-package `InlineTestProject` regression.
- [x] (2026-08-14 05:31Z) Separated scan candidates from resolution inputs so a query builds compact Go type/import facts from the whole analyzed workspace but retains and scans only candidate/target trees.
- [x] (2026-08-14 05:56Z) Passed both issue tests, all 57 Go usage-graph tests, Go crate tests, formatting, dependency checks, and focused all-target clippy.
- [x] (2026-08-14 06:02Z) Rebuilt the release runner and replayed all eight #2072 occurrence keys; each completed with `actionable=0`.
- [ ] Commit, push, comment on #2072 with evidence, and close it.

## Surprises & Discoveries

- Observation: The generic nearest-depth helper is already recursive and cycle-aware, and #1269 already proves interface-to-interface promotion.
  Evidence: `go_indexed_member_candidates_at_nearest_depth_with_path` recursively follows `embedded_field_type_fqns`, while `go_graph_strategy_finds_promoted_interface_method_calls_through_embedded_interfaces` passes on the current tree. The defect therefore lies in the concrete embedded-struct facts or target-specific receiver matching rather than a wholly absent promotion algorithm.
- Observation: The missing variable was authoritative scan scope, not package syntax by itself.
  Evidence: The two-hop cross-package fixture passed when `candidate_files` contained the whole project and returned zero hits when it contained only `caller.go`, matching the exact differential runner's one-file inventory. `build_go_graph` built both its retained scan trees and `GoEdgeIndex` from that narrow set plus the target file, so the intermediate embedded struct was absent.

## Decision Log

- Decision: Preserve the existing nearest-depth, ambiguity, and direct-override semantics and repair the structured promotion inputs or shared owner match rather than adding a name-only fallback.
  Rationale: Go permits promotion only through a specific embedding chain. The graph already contains the right precedence abstraction, and bypassing it would incorrectly claim ambiguous peer embeddings or methods hidden by a nearer declaration.
  Date/Author: 2026-08-14 / Codex
- Decision: Pass all analyzed Go files as resolution inputs while keeping `candidate_files` as the only reporting/retained-tree scope.
  Rationale: Declaration and import facts are required to type a candidate reference even when their source files are not allowed to produce hits. The implementation uses the full file list only as an inventory, parses the candidate/target packages and their transitive workspace imports, and drops every non-candidate tree after compact facts are built. This preserves candidate-bounded work for unrelated packages.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

The root fix and behavior tests are complete. Narrow authoritative scans now recover methods promoted across package boundaries without reporting hits outside the candidate scope. All focused tests and eight exact corpus replays pass. Commit/push and issue-tracker closure are the only remaining administrative steps.

## Context and Orientation

`crates/bifrost-go/src/graph/resolver.rs` builds `GoEdgeIndex`, a compact workspace index used by Go usage analysis. Its `embedded_field_type_fqns` map records the declared types embedded by each struct or interface. Its `direct_member_fqns` map records fields and methods declared directly by an owner. `go_unique_indexed_member_candidate_at_nearest_depth` walks those maps and returns a method only when the nearest promoted declaration is unique.

`TargetSpec::new` uses that lookup to enumerate receiver types compatible with a queried method. `crates/bifrost-go/src/graph/extractor.rs` then scans source files, infers receiver types, and records a call only when its receiver is compatible with the target. Issue #2072 reports seven Hugo sites and one Harness site where a concrete struct embeds another struct and the inner struct embeds the queried interface, but targeted inverse usage lookup omits the call.

The integration regression belongs in `tests/suite_issues/issue_2072_go_promoted_interface_chain.rs` and must be registered in `tests/suite_issues/main.rs`. It will use `InlineTestProject` and query `GoUsageGraphStrategy`, matching the user-visible usage-finding path.

## Plan of Work

First add the minimal three-file Go fixture: a package declaring interface `Provider`, a concrete `Inner` embedding `Provider`, a concrete `Outer` embedding `*Inner`, and calls through each receiver. Assert that querying `Provider.Get` returns the direct interface, one-hop, and two-hop call sites. Add controls where `Outer` declares its own `Get`, where two peer embedded interfaces provide `Get`, and where an unrelated same-named method exists; these sites must not be attributed to `Provider.Get`.

Run the focused test before changing production code and record the exact failure. Inspect the indexed embedding facts for the fixture. Correct the smallest shared structured seam in `crates/bifrost-go/src/graph/resolver.rs`: either ensure embedded struct fields resolve to canonical workspace type identities at every hop, or ensure the target-compatible receiver enumeration consumes the existing transitive nearest-depth result consistently. Keep traversal iterative or use the existing cycle-bounded helper; do not scan source text or compare only terminal names.

After the focused test passes, run the existing #1269 promotion test and the relevant Go graph suite to protect interface-only promotion and existing direct-member behavior. Build the release differential runner and replay the eight exact ledger commands with ephemeral caches. Each occurrence must receive an exact inverse hit and produce no actionable finding. Calls through a method's own receiver are expected to classify as `editor_only` because the usage surface deliberately labels them `SelfReceiver`; other calls classify as `consistent`.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`.

Add and run the focused regression:

    cargo test --test suite_issues -- issue_2072_go_promoted_interface_chain --nocapture

Run adjacent behavior coverage:

    cargo test --test suite_usages -- usages_go_graph_test::go_graph_strategy_finds_promoted_interface_method_calls_through_embedded_interfaces
    cargo test -p brokk-bifrost-go
    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-go -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs

For corpus acceptance, use the release `bifrost_reference_differential` binary and the eight `rerun_command` values stored in the final Go raw ledgers under `/mnt/optane/tmp/bifrost-fird/final-63a1912a`, replacing persisted cache mode with `--cache-mode ephemeral` for one-shot verification. Expect `inverse_hit.exact_range=true` and zero actionable findings for every occurrence.

## Validation and Acceptance

The new issue test must fail before the production fix by omitting the two-hop `Outer.Get` usage, then pass afterward. The direct interface and one-hop sites must remain present. A direct override must win, peer embeddings must remain ambiguous and unclaimed, and an unrelated same-named method must not appear.

The existing #1269 interface-embedding test must pass unchanged. All eight recorded Hugo/Harness sites must report an exact inverse hit and zero actionable findings after rebuilding the runner. Formatting, focused clippy, and the dependency-boundary check must pass before push.

## Idempotence and Recovery

All test and replay commands are safe to repeat. Corpus replays use ephemeral cache mode and therefore do not modify repository caches. If an implementation attempt changes unrelated behavior, revert only the files listed in this plan with an explicit patch; do not reset the worktree because other user changes may coexist.

## Artifacts and Notes

The representative production chains are Hugo `pageState -> *pageCommon -> page.PageMetaProvider` for methods such as `Path`, and Harness `npmjs.adapter -> *native.Adapter -> registry.Client` for `GetFileFromURL`.

## Interfaces and Dependencies

Retain `GoEdgeIndex::embedded_field_type_fqns`, `GoEdgeIndex::direct_member_fqns`, and `go_unique_indexed_member_candidate_at_nearest_depth` as the shared structured facts and precedence operator unless investigation proves one of their contracts is wrong. Do not add dependencies or a new crate. Keep `brokk-bifrost-core` free of language-specific code.

Plan revision note: Updated on 2026-08-14 after validation. The final design records a transitive import closure rather than parsing unrelated workspace packages, and the exact eight-site corpus result is now part of the acceptance evidence.
