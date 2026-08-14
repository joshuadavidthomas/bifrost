# Resolve PHP member calls on direct factory results

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

After this change, PHP find-references and whole-workspace usage graphs will understand calls such as `makeProduct()->consume()`, `Factory::makeProduct()->consume()`, and `self::makeProduct()->consume()`. Bifrost already navigates these call sites forward because the factory declaration has a structured return type, but inverse analysis currently omits them. The result is visible by running the new issue regression and representative one-site reference-differential replays: the member token must be an exact inverse hit for the member declared on the factory's return type, while same-named members on another type and dynamically typed factories remain excluded.

## Progress

- [x] (2026-08-13 16:30Z) Read `.agents/PLANS.md`, issue #2026, the shared PHP receiver evaluator, both PHP graph walkers, and adjacent factory/chain tests.
- [x] (2026-08-13 16:30Z) Confirmed the shared failure: `instance_receiver_type_fq_name` has no direct free-function or scoped-call base case, while two assignment-only helpers duplicate that return-type logic.
- [x] (2026-08-13 17:15Z) Added `direct_call_return_type_fq_name` and routed direct receiver roots plus both assignment seeders through it.
- [x] (2026-08-13 17:15Z) Added targeted and whole-workspace behavior coverage with positive free/scoped/relative calls and typed/dynamic near misses.
- [x] (2026-08-13 17:20Z) Passed formatting, focused PHP regressions, the workspace dependency check, and PHP/PHP-analysis clippy.
- [x] (2026-08-13 17:25Z) Rebuilt the release differential runner and replayed representative Ramsey, php-code-coverage, MathPHP, and Respect witnesses; all four are consistent exact inverse hits with zero actionable findings.
- [x] (2026-08-13 17:35Z) Committed as `f354af7e`, merged the intervening release-version correction, revalidated at the merged head, pushed `4bc3f7b5` to `master`, commented exact evidence, and closed #2026.

## Surprises & Discoveries

- Observation: Both inverse surfaces already share `crates/bifrost-php/src/graph/syntax.rs::instance_receiver_type_fq_name`; the missing behavior is not two separate scanner defects.
  Evidence: `crates/bifrost-php/src/graph/extractor.rs::receiver_expression_type` and `crates/bifrost-php/src/graph/inverted.rs::receiver_type_fqn` both delegate to that function.

- Observation: Assigned factory results already work through two independent helpers, so adding a third implementation inside the receiver walk would preserve a correctness drift.
  Evidence: `extractor.rs::assignment_receiver_type` and `inverted.rs::assignment_receiver_type_fqn` each separately resolve free and scoped calls and then inspect declared return types.

- Observation: `PhpGraphSource::facts` provides a unique callable-FQN return answer that should precede physical declaration fallback.
  Evidence: `PhpCallableFacts::callable_return_type_fqn` represents the usage-facts index's collapsed answer, while `declared_callable_return_type_fq_name` can recover a return from one physical declaration and its signature.

- Observation: The shared evaluator also made parenthesized factory assignments consistent between the two inverse surfaces.
  Evidence: the targeted assignment helper already recursed through `parenthesized_expression`; the inverted helper did not. Reusing the direct-call helper exposed and closed that parity gap without a separate interpretation.

## Decision Log

- Decision: Put direct free/scoped-call return inference in `crates/bifrost-php/src/graph/syntax.rs` and reuse it from the receiver walk plus both assignment seeders.
  Rationale: This is the shared language-owned structured seam used by targeted and inverted scans. It avoids three interpretations of PHP namespace fallback, `self`/`static`/`parent`, callable uniqueness, and return facts.
  Date/Author: 2026-08-13 / Codex

- Decision: Accept only literal parser nodes for callable names and static scopes; dynamic callable expressions remain unknown.
  Rationale: The issue requests structured return inference, and a text fallback would invent receiver types when PHP dispatch is dynamic or ambiguous.
  Date/Author: 2026-08-13 / Codex

- Decision: Prefer the callable-FQN return fact, then require one physical callable declaration before using declaration/signature return metadata.
  Rationale: The facts index already collapses consistent declarations. When it has no answer, exactly one physical declaration is the strongest remaining proof; multiple declarations without a collapsed fact must fail closed.
  Date/Author: 2026-08-13 / Codex

- Decision: Use one new `tests/suite_usages/issue_2026_php_direct_factory_receivers.rs` module for both targeted and whole-workspace behavior.
  Rationale: The repository requires a new integration-test module for new behavior, and one InlineTestProject can prove parity and precision without duplicating the fixture.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The implementation now gives direct free-function, explicit scoped, `self`, and parenthesized factory results the same structured receiver typing as assigned factory results. Targeted usages return exactly the four `Product.consume` member ranges in the regression fixture. Whole-workspace edges reach `Product.consume` for the four positive callers, reach only `Other.consume` for typed decoys, and make no claim for untyped or dynamic factories.

Focused validation passed: the two new issue tests, the three adjacent PHP receiver/assignment regressions, `scripts/check-workspace-dependencies.mjs`, and clippy for `brokk-bifrost-php` plus `brokk-bifrost-analysis`. Four release-runner exact replays completed with `classification=consistent`, `inverse_hit.exact_range=true`, and `inverse_precision_unbacked_hits=0`:

- Ramsey UUID `fromBytes`, SHA-256 `faba998dd8e8c2429114f1090aae80dca99537cd9fcdbcff8aa44739e69ce851`.
- MathPHP `getMatrix`, SHA-256 `e625a993695daff7689761af7cd6d1ae237b87b914cec28f55befa6d9cfcb10b`.
- php-code-coverage `asFloat`, SHA-256 `40f5cc6d16d589b68af0cd9b1d22359bc8fce79de8ac474c4630e0fdc04e1287`.
- Respect Validation `asAdjacentOf`, SHA-256 `eef498039a718a3e5d25d2bd8bfe28f5c84ab754b8b0d52186a9dd96e8b2b952`.

The change deliberately leaves dynamic callable receivers and calls without a proven declared return type unknown. No source-text fallback or new dependency was introduced.

## Context and Orientation

PHP usage analysis has two inverse surfaces. The targeted surface starts from one declaration and scans likely files for references; it lives in `crates/bifrost-php/src/graph/extractor.rs`. The whole-workspace surface walks each PHP file and emits graph edges; it lives in `crates/bifrost-php/src/graph/inverted.rs`. Both are language-owned and receive a `PhpGraphSource`, which combines the dispatching analyzer's declaration index with `PhpCallableFacts`, a small interface for declared return-type facts.

An instance receiver is the expression to the left of `->` or `?->`. `crates/bifrost-php/src/graph/syntax.rs::instance_receiver_type_fq_name` evaluates such expressions iteratively so deeply chained source cannot overflow the Rust stack. It understands typed variables, `new`, parentheses, fields, and member-call chains. It does not recognize a direct `function_call_expression` or `scoped_call_expression` as the root. Therefore `Factory::make()->consume()` never obtains the `Product` receiver type even when `Factory.make` declares `Product` as its return.

PHP free function names have namespace fallback rules. `crates/bifrost-php/src/aliases.rs::resolve_php_function_node` converts a literal parser node into ordered `PhpCallableCandidates`: the current namespace candidate first and the global candidate second. `PhpCallableCandidates::first_indexed` selects the first candidate actually declared in the workspace. Static calls use `static_member_parts` and `static_scope_type_fq_name`; the latter interprets `self`, `static`, and `parent` from the lexically enclosing class. These existing structured helpers must be reused rather than parsing source text.

The assignment seeders currently demonstrate the desired behavior but duplicate it. `extractor.rs::assignment_receiver_type` and `inverted.rs::assignment_receiver_type_fqn` recognize factory calls only after assigning them to a variable. The implementation will replace their free/scoped branches with the same helper that powers direct receiver roots, preserving object creation and parenthesized assignment behavior.

## Plan of Work

In `crates/bifrost-php/src/graph/syntax.rs`, add a public function that accepts the PHP source provider, `PhpGraphSource`, one call-expression node, source text, the file's `PhpFileContext`, and an optional lexically enclosing class FQName. For a `function_call_expression`, read its `function` field, resolve it with `resolve_php_function_node`, select the first indexed callable candidate, and resolve that callable's declared return type. For a `scoped_call_expression`, use `static_member_parts`, require a literal member name, resolve the scope with `static_scope_type_fq_name`, build the callable FQName, and resolve its return type. For any other node or dynamic name, return `None`.

Factor a private callable-FQName return helper in the same module. It first asks `analyzer.facts.callable_return_type_fqn`. If absent, it queries `analyzer.index.definitions`, retains functions, requires exactly one, and delegates to `declared_callable_return_type_fq_name`. This retains signature recovery and class-relative `self`/`static` return behavior.

Add the two direct-call kinds as base cases in `instance_receiver_type_fq_name`. Compute the enclosing owner only for a scoped call and store a successful return FQName in the iterative resolver's `resolved` map. Do not recurse into call arguments or dynamic callable expressions.

In `crates/bifrost-php/src/graph/extractor.rs`, replace the free/scoped branches of `assignment_receiver_type` with the shared helper. Continue handling object creation and parentheses locally. In `crates/bifrost-php/src/graph/inverted.rs`, do the same for `assignment_receiver_type_fqn` and remove its local callable-return helper if no caller remains. Update stale comments that call chained receivers a known recall gap.

Create `tests/suite_usages/issue_2026_php_direct_factory_receivers.rs` and register it in `tests/suite_usages/main.rs`. Build one InlineTestProject containing `Product::consume`, `Other::consume`, a free `makeProduct(): Product`, `Factory::makeProduct(): Product`, a relative `self::makeProduct(): Product`, and parentheses around a direct factory call. Add wrong-return-type factories that return `Other`, plus an untyped/dynamic factory expression. Query `Product.consume` through `PhpUsageGraphStrategy` and require exactly the positive member ranges. Run `usage_graph_at` over the same project and require each positive caller to edge to `Product.consume`, each typed decoy to edge only to `Other.consume`, and dynamic/untyped callers to create no `Product.consume` edge.

## Concrete Steps

Work from `/mnt/optane/bifrost-fird`.

After implementing the shared helper and tests, run:

    cargo fmt --all
    cargo test --test suite_usages -- issue_2026_php_direct_factory_receivers
    cargo test --test suite_usages -- usages_php_graph_test::php_graph_follows_call_receiver_types_and_nullsafe_members
    cargo test --test suite_usages -- usages_php_graph_test::php_graph_infers_self_static_and_parent_factory_assignment_results
    cargo test --test suite_usages -- usage_graph_php_test::object_sensitive_factory_receiver_resolves_only_declared_return_type
    node scripts/check-workspace-dependencies.mjs
    cargo clippy -p brokk-bifrost-php -p brokk-bifrost-analysis --lib -- -D warnings

Expect all focused tests to pass, formatting to make no further changes, the dependency checker to exit zero, and clippy to finish without warnings.

Build the one-shot differential binary in the repository target directory because exact replay commands use that stable path:

    cargo build --release --bin bifrost_reference_differential

Select representative occurrence records from the canonical PHP rank-31+ ledger, preserve their pinned repository roots and exact byte ranges, replace only the output path with fresh `/tmp/issue2026-...jsonl` files, and execute them with `--cache-mode ephemeral --strict`. Each output must have `forward_status=resolved`, `classification=consistent`, `inverse_hit.exact_range=true`, and `inverse_precision_unbacked_hits=0`.

## Validation and Acceptance

The new integration test must fail before production edits because neither targeted nor whole-workspace inverse analysis can type a direct free/scoped factory result. It passes only when all positive member tokens are exact `Product.consume` references on both surfaces. A same-named `Other.consume` is the primary precision guard. Dynamic or untyped factory results must remain unproven, demonstrating that the implementation did not add a name-only fallback.

The existing assigned-factory and chained-member tests must remain green. They prove that moving direct-call inference into the shared helper does not regress variable seeding, `self`/`static`/`parent`, nullsafe chains, or declared field receiver types.

Representative production replays are the final acceptance. At least one site each from Ramsey UUID, php-code-coverage, MathPHP, and Respect Validation must become consistent with an exact inverse hit. The issue may be closed after focused tests and these exact replays; the user explicitly asked not to wait for the full CI report.

## Idempotence and Recovery

All source edits and tests are deterministic and may be rerun. Exact corpus commands use ephemeral caches and fresh output paths, so they do not mutate repository inputs or confuse old evidence with new results. If an exact replay fails, keep the output as diagnostic evidence, update this plan's `Surprises & Discoveries`, and fix the shared structured seam rather than adding a corpus-specific exception. Stage only the plan, PHP source files, and the new test module when committing.

## Artifacts and Notes

Canonical baseline evidence is under `/mnt/optane/tmp/bifrost-fird/final-63a1912a/`. The issue reports 77 occurrences across 41 files and 33 target FQNames. The baseline Bifrost revision is `63a1912a09616d8d10e389126fa371ebadc4cc2a`.

The intended direct-call helper shape is:

    pub fn direct_call_return_type_fq_name(
        php: &dyn PhpSource,
        analyzer: PhpGraphSource<'_>,
        node: Node<'_>,
        source: &str,
        ctx: &PhpFileContext,
        enclosing_owner: Option<&str>,
    ) -> Option<String>

Names may change during implementation if the existing module vocabulary suggests a clearer one, but the single shared structured interpretation is mandatory.

## Interfaces and Dependencies

No new crate or third-party dependency is required. `brokk-bifrost-php` already owns `PhpSource`, `PhpGraphSource`, `PhpCallableFacts`, `PhpFileContext`, the PHP tree-sitter grammar helpers, namespace callable resolution, and the two graph walkers. `brokk-bifrost-analysis` remains responsible only for constructing `PhpGraphSource` and exposing integration surfaces. The change must preserve the dependency rule that language code does not depend upward on `brokk-bifrost-analysis`.

Revision note (2026-08-13 16:30Z): Created this self-contained plan after confirming #2026's direct-call receiver gap, the duplicated assignment-only implementations, the callable-facts precedence, and the targeted plus whole-workspace acceptance surfaces.

Revision note (2026-08-13 17:25Z): Recorded the completed shared implementation, the parenthesized-assignment parity discovery, focused validation, and four exact production replay hashes.

Revision note (2026-08-13 17:35Z): Recorded the pushed commit, post-merge validation, GitHub evidence comment, and issue closure.
