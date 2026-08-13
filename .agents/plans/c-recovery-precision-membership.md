# Back valid recovered C references with a precision-only membership frontier

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document follows `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

The forward-versus-inverse differential runner checks whether every exact inverse usage hit also belongs to an independently collected source membership set. In C, macros and preprocessor recovery can place valid, structured references beneath a tree-sitter `ERROR` node. The conservative probe census intentionally does not propose sites from such a region, but today its same restricted ranges are also reused as inverse membership, so 904 valid C hits are falsely reported as inverse-precision failures. After this change, the probe census remains unchanged while a C-only, recovery-aware membership path backs structured type, member, call, value, and active-macro references beneath `ERROR`. Declarations, binders, labels, editor-only definition hits, and arbitrary recovery fragments remain ineligible or unbacked.

## Progress

- [x] (2026-08-13 15:02Z) Read #2089, `.agents/PLANS.md`, the full-ledger correction, the generic census-membership code, C authoritative inverse batching, and C resolver role helpers.
- [x] (2026-08-13 21:12Z) Introduced a bounded C recovery-membership collector in `brokk-bifrost-cpp` using structured AST roles and exact macro-environment evidence.
- [x] (2026-08-13 21:12Z) Reused the authoritative C batch visibility to augment precision membership without changing the census probe frontier.
- [x] (2026-08-13 21:12Z) Made binding/declaration usage-hit kinds precision-ineligible and added behavior coverage for recovered type/member references, macro definition/formal exclusions, and candidate-cap failure.
- [x] (2026-08-13 21:31Z) Ran focused validation, committed `695339c9`, rebuilt the release runner, and exact-replayed six representative corpus witnesses with zero actionable findings and zero file errors.
- [ ] Push the implementation and plan closure, publish the exact evidence, and close #2089 without waiting for the complete C campaign.

## Surprises & Discoveries

- Observation: probe sampling and inverse membership currently call the same function.
  Evidence: `src/reference_differential/mod.rs::collect_census_membership` reparses each audited file and calls `census_identifier_ranges`, exactly as `collect_sampled_sites` does. `CandidateFrontier::Census` stops at every `ERROR`, so both products lose all identifiers beneath recovery.

- Observation: C inverse analysis already builds the expensive structured state needed by membership.
  Evidence: `compare_inverse` creates one `CppAuthoritativeUsageBatch` over the union of C caller roots. The batch owns a `VisibilityIndex`, and that index already reconstructs include visibility, macro definition order, conditional uncertainty, and `#undef` at exact bytes.

- Observation: two of the original 651 partial-ledger findings are definition hits rather than source references.
  Evidence: the full audit identifies SPDK `spdk_nvme_ctrlr_is_discovery` and `spdk_nvme_ctrlr_alloc_qid` with `UsageHitKind::Definition`. Definition sites are editor linkage, not inverse precision evidence, and must be excluded by kind rather than inserted into reference membership.

- Observation: all 904 full-ledger precision findings are wholly contained by tree-sitter-cpp `ERROR` ranges.
  Evidence: the completed ledger at Bifrost `fcd83045` has 904 inverse-precision rows in 49 files; the original 651 remain unchanged and the 253 added rows have the same recovery mechanism.

- Observation: a valid macro argument can preserve an ordinary selected-member subtree even when the surrounding declaration-shaped parse is recovered as `ERROR`.
  Evidence: `DISCARD(const int = state->timestamp)` expands to a valid constant while tree-sitter-cpp retains `state->timestamp` as a `field_expression` below `ERROR`. Both authoritative inverse lookup and the recovery-membership collector identify the exact `timestamp` range.

## Decision Log

- Decision: keep `census_identifier_ranges` and `CandidateFrontier::Census` unchanged.
  Rationale: the probe census grades every proposed site. Entering raw recovery regions would revive #1784 false probes from parser artifacts. #2089 concerns independent inverse backing, so the correction belongs in a separate precision-only product.
  Date/Author: 2026-08-13 / Codex

- Decision: put C recovery-role classification in `brokk-bifrost-cpp`, and expose it through `CppAuthoritativeUsageBatch` to the differential runner.
  Rationale: the language crate owns tree-sitter-cpp roles, declaration-name detection, callable/type/member wrappers, and the macro environment. The analysis batch already owns the one visibility index for the inverse phase, so this placement avoids duplicating C semantics in the generic runner or building a second workspace visibility index.
  Date/Author: 2026-08-13 / Codex

- Decision: augment membership only when `ReferenceDifferentialConfig.corpus_language == "c"`.
  Rationale: both C and C++ use `Language::Cpp`, but permissive recovery shapes and tag/macro rules are compilation-language-sensitive. Extending a C-specific acceptance frontier to C++ would silently broaden a different product contract.
  Date/Author: 2026-08-13 / Codex

- Decision: classify `Import`, `Reexport`, `Definition`, and `OverrideDeclaration` hits as precision-ineligible before membership lookup.
  Rationale: these are bindings or declaration relationships, not literal source references that the census is intended to back. `Reference` and `SelfReceiver` remain eligible because both represent source-reference occurrences.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation now separates the two contracts that the old shared census range set conflated. `CandidateFrontier::Census` remains conservative and still contributes no probe below `ERROR`. During inverse comparison, the already-built C authoritative batch supplies a bounded, structured recovery-only range set backed by the same macro visibility facts used by C usage resolution. The runner merges that set only for an explicitly C corpus and treats incomplete membership as unavailable rather than complete. Definition/import/reexport/override hits are excluded before reference-precision grading.

The focused fixture proves the important non-vacuous case end to end: authoritative inverse analysis actually returns both a recovered type reference and a recovered selected-member reference; neither range appears among probe sites; and the differential emits no inverse-precision finding. The low-level collector test also pins macro definition/formal rejection and all-or-nothing cap behavior. Existing #1784 misparsed-region probing remains unchanged.

Six exact production replays now cover the original partial ledger and the completed-ledger supplement. strongSwan validates a recovered type, libarchive and Git validate selected members under large/preprocessor recovery envelopes, pgBackRest validates a macro token whose parser role resembles a declaration, SPDK validates definition-kind ineligibility, and CycloneDDS validates a supplemental recovered member. Every run completed with one consistent site, zero inverse-precision findings, zero candidate truncation, and zero file errors. This is sufficient focused acceptance for the requested push; the full 904-key replay remains intentionally deferred to the broader campaign.

## Context and Orientation

`src/reference_differential/mod.rs` implements the command-line differential runner. A probe is a source range sent to forward definition lookup. The census probe seed uses the maximal grammar identifier set but stops at tree-sitter recovery nodes, because those nodes can assign misleading roles. Separately, after forward targets resolve, the runner asks `UsageFinder` or the C authoritative batch for inverse usages. An inverse-precision finding means an inverse hit names the target literally but its exact `(start, end, text)` tuple is absent from `CensusMembership`.

`crates/bifrost-analysis/src/analyzer/reference_candidates.rs` owns the generic probe frontiers. `CandidateFrontier::Census` must not change. `crates/bifrost-analysis/src/analyzer/usages/cpp_graph/shared.rs` owns `CppAuthoritativeUsageBatch`, one inverse-phase object that prepares C/C++ caller roots and builds `VisibilityIndex` once. `crates/bifrost-cpp/src/graph/resolver.rs` owns shared C/C++ AST and visibility helpers such as `is_declaration_name`, `is_call_callee_node`, `function_terminal_node`, `type_reference_hit_node`, and exact macro binding queries. These are the correct primitives for a C-only recovery membership collector.

A membership range is “backed” when a language-aware traversal independently recognizes the exact source token as a credible reference occurrence. “Ineligible” means the inverse hit itself is a declaration/import relationship and therefore is outside precision grading. “Unavailable” means parsing or the configured candidate cap prevented a complete membership set; the runner must suppress precision accusations for that entire file rather than treating a partial set as complete.

## Plan of Work

In `crates/bifrost-cpp/src/graph/resolver.rs`, add a bounded collector for `.c` files that walks only `ERROR` subtrees iteratively. Outside `ERROR`, the existing census set already supplies membership. Within recovery, admit exact structured reference leaves and terminal ranges: non-declaration type nodes; the `field` child of a `field_expression`; real call callees; ordinary identifiers anchored in expression positions; and macro tokens only when the exact `VisibilityIndex` says a macro may be active. Reject `is_declaration_name`, macro definition names and formals, labeled and `goto` labels, missing/zero-width nodes, and identifiers that reach `ERROR` without passing through a recognized type/member/call/expression structure. Keep traversal and candidate accounting bounded; return a complete vector or an unavailable/limit result, never a partial success.

In `crates/bifrost-analysis/src/analyzer/usages/cpp_graph/shared.rs`, add a narrow read-only `CppAuthoritativeUsageBatch` method that gets the analyzer generation's prepared syntax for a file and calls the language collector with the batch's existing `VisibilityIndex`. It returns exact ranges only; it does not expose the visibility object or rebuild it.

In `src/reference_differential/mod.rs`, retain generic census membership collection before forward resolution. Change `compare_inverse` to receive mutable membership. After constructing the authoritative C batch and before parallel target scans, when the configured corpus language is exactly `c`, augment each relevant file's membership with the recovered ranges. Merge and deduplicate against the original tuples, enforce the configured total per-file candidate cap, and store `None` for unavailable or over-limit files. Files outside the C corpus retain the existing map unchanged.

Before membership lookup in `inverse_precision_findings`, reject `Import`, `Reexport`, `Definition`, and `OverrideDeclaration`. Add a unit test proving Definition never becomes an inverse-precision finding even with empty membership. Preserve the existing literal-name requirement and fail-closed `None` membership behavior.

Add `tests/suite_semantic/reference_differential_c_membership.rs` and register it in `tests/suite_semantic/main.rs`. Use `InlineTestProject` with `corpus_language: "c"`. Build a macro/preprocessor-recovered fixture containing a valid type and member/call reference beneath `ERROR` plus an intact trigger that lets inverse lookup discover the target. Assert that probe sites inside the recovery region remain absent, while inverse precision reports zero unbacked hits. Add declaration, macro formal/definition, label, and arbitrary direct recovery-child controls that remain outside membership. Add a small-cap run that becomes unavailable and emits no precision finding rather than using a partial set. Add lower-level language-crate tests if the integration fixture cannot distinguish a role directly.

## Milestones

The first milestone is the language collector. It ends when a bounded, iterative C-only traversal returns exact recovery reference ranges and rejects declaration/label/garbage near misses. A focused language-crate test must demonstrate both the positive and negative roles without changing generic census ranges.

The second milestone is runner integration. It ends when the authoritative inverse batch supplies recovered ranges to mutable precision membership, the original probe site count remains unchanged, and Definition hits are precision-ineligible. The new InlineTestProject differential test must fail before the change and pass after it.

The final milestone is production acceptance. Rebuild `bifrost_reference_differential` and replay representative strongSwan, libarchive, pgBackRest, SPDK, Git, and one supplemental finding with `--cache-mode ephemeral`. Every reference witness must become backed, Definition witnesses must become ineligible, probe counts for the exact runs must remain unchanged, and there must be no file error or new precision finding. Push and close #2089 with focused evidence without waiting for the full 904-key rerun; the complete C leg remains part of the broader FIRD goal.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`. Edit with `apply_patch`. Run:

    cargo fmt --all
    cargo test -p brokk-bifrost-cpp --lib graph::resolver
    cargo test --test suite_semantic -- reference_differential_c_membership::
    cargo test --test suite_semantic -- reference_differential::census_seed_grades_nothing_from_a_misparsed_region
    cargo test -p brokk-bifrost-analysis --lib reference_differential::tests::inverse_precision
    cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

After focused validation, build the release runner with:

    cargo build --release --bin bifrost_reference_differential

Use exact `rerun_command` values from the full C ledger, changing only the output directory to `/tmp/bifrost-fird-<revision>/c-exact` and keeping ephemeral cache mode.

## Validation and Acceptance

The new C differential test must show unchanged conservative sampling: no probe site from the intentionally recovered `ERROR` region. The same report must show exact inverse hits under that region backed by the recovery membership set and `inverse_precision_unbacked_hits == 0`. Declaration names, macro formals and definitions, labels, and unstructured recovery identifiers must not be admitted. If membership exceeds its cap or cannot be prepared, the file is unavailable and produces no precision accusation.

Existing JavaScript #1784 recovery-probe behavior must remain green, proving the generic census frontier did not broaden. Existing C/C++ usage and macro suites must remain green. Representative corpus replays must have clean trees, no file errors, and no inverse-precision finding for each exact key. Full CI and the complete 904-key campaign rerun are not blockers to the requested push.

## Idempotence and Recovery

The collector reads immutable prepared syntax and the inverse batch's immutable visibility index. Repeated runs are deterministic and do not write corpus caches when `--cache-mode ephemeral` is used. If range collection reaches the cap, discard the entire file result and record membership unavailable. If a replay is interrupted, rerun it to a new revision-specific output file; prior evidence remains usable.

## Artifacts and Notes

The original partial ledger is `/mnt/optane/tmp/bifrost-fird/final-63a1912a/c-partial9-raw-ledger.jsonl`, SHA-256 `8b37df7287b9c4ca238a2a8f3d2dc7b03ef59f3053aaed21f4e6de33db43ef1c`.

The completed full C ledger at Bifrost `fcd830452a078c69bb7e1f1d085c78ff447de7fe` has report SHA-256 `02b5422b1977f613e15371e828e8f181ee513a34e96e9fecd2be0e3e040f3bcc`, 904 inverse-precision findings in 49 files, and all 904 exact ranges beneath tree-sitter-cpp `ERROR` nodes. Supplemental counts are CycloneDDS 158, Fastfetch 85, Capstone 5, PyCryptodome 3, and dqlite 2.

Representative shapes are strongSwan `malloc_thing(private_host_t)`, libarchive `iso9660->opt.boot`, pgBackRest `configLocal->option[...]`, SPDK declaration type references, Git `istate->timestamp`, and macro-shaped `THIS(const Bz2Compress)`. Two SPDK rows are Definition hits and should clear through eligibility, not membership.

Focused acceptance artifacts from clean Bifrost `695339c91a8022b63ddec7718b7c1721ddd725c4` are under `/tmp/bifrost-fird-695339c9/c-exact/`. Each report has `status=completed`, one consistent site, `inverse_precision_unbacked_hits=0`, and an empty `file_errors` array:

- strongSwan `eb93c6d1645829ae.jsonl`: SHA-256 `71205b25a0dc0b320fea92d821cefa7c2a290edf18a85c70e8fc769358ee4a5c`.
- libarchive `f003f11bea101258.jsonl`: SHA-256 `57f229ad5824a87b422f34169400ce166eacb86bb0c747c3c379437816f73ede`.
- pgBackRest `6de35192d3dc946c.jsonl`: SHA-256 `0f7731ce80baadbb0e40c9653675f78f1e202703dca10afcf7ba95de8665ccc4`.
- SPDK definition `16d423f3a20b9acc.jsonl`: SHA-256 `60691843153b855e5da1f1a9e43d12dded729c27bbdfb20529f6bda38a1f7776`.
- Git `04f08137303d41a7.jsonl`: SHA-256 `81c36c13ddbd35377b5a0db78f92629c037da7856c676c1fa789e6967a560d75`.
- CycloneDDS supplemental `ad5ee6035c0fa0df.jsonl`: SHA-256 `2c5fefc13fb864f5eff3537201de223a544e38c7293b67938c7bc0d601b89152`.

Plan revision note (2026-08-13): Created after #2093 closure. The source audit established that the authoritative C inverse batch is the minimal reuse seam and that the generic probe frontier must remain untouched.

Plan revision note (2026-08-13 21:12Z): Updated after implementation and focused validation. The synthetic member witness was changed to a valid ignored macro argument, `DISCARD(const int = state->timestamp)`, so the grammar preserves a structured `field_expression` under `ERROR` and the test proves an actual inverse hit rather than membership-only bookkeeping.

Plan revision note (2026-08-13 21:31Z): Recorded checkpoint `695339c9` and six clean exact corpus replays. The supplemental CycloneDDS result demonstrates that the fix is not limited to the original partial-nine ledger.

## Interfaces and Dependencies

Add a small result type in `brokk-bifrost-cpp` for complete recovered ranges versus unavailable/limit exceeded, or use an equivalent `Option<Vec<Range>>` whose `None` contract is documented as unavailable. The collector must accept `&CppGraphSource`, `&VisibilityIndex`, `&ProjectFile`, a prepared tree root, source text, and the configured limit. It depends only on existing `brokk-bifrost-core` model types, tree-sitter, and C++ graph facts; no new crate or dependency is needed.

Add a public or crate-visible method on `CppAuthoritativeUsageBatch` that returns the collector result for one root file. Do not expose `VisibilityIndex` to the runner. Change `compare_inverse` only enough to augment mutable membership before parallel scans. Keep `brokk-bifrost-core` at the bottom of the dependency graph and do not enable NLP or Python features.
