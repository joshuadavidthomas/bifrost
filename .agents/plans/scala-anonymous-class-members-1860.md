# Model Scala self-type and anonymous-class members

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must stay current as work proceeds.

Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

After this change, Bifrost can navigate and scan usages for members declared inside a Scala anonymous class such as `new Task { def run = ... }`. The anonymous class gets a stable, source-backed synthetic owner. Its named members remain normal declarations. Existing Scala self-type access such as `this: Parser with Tokens =>` also stays covered by an issue-level regression test.

Users can verify the result through `get_definitions_by_location`, `scan_usages_by_location`, and the analyzer declaration index. A call from one anonymous-class member to a sibling overload must stay in that anonymous owner. The same member name in another anonymous class must not match.

## Progress

- [x] (2026-08-11 07:16Z) Read issue 1860 and fetch current `origin/master`.
- [x] (2026-08-11 07:16Z) Confirm that the issue branch starts at current `origin/master` and has no issue-specific commits.
- [x] (2026-08-11 07:16Z) Inspect Scala declarations, current self-type tests, Java synthetic lambda identities, and Kotlin anonymous-object policy.
- [x] (2026-08-11 08:08Z) Add a focused issue test that proves anonymous members are absent before the change and tests compound self-type member access.
- [x] (2026-08-11 08:37Z) Add a structured anonymous-class declaration owner and index its direct members.
- [x] (2026-08-11 09:11Z) Connect anonymous owner supertypes and sibling-member resolution without source-text parsing.
- [x] (2026-08-11 09:28Z) Preserve named refinement and replica source-set boundaries with focused regression tests.
- [x] (2026-08-11 09:56Z) Update the Scala cache epoch and run focused tests, formatting, and featureless workspace clippy.
- [x] (2026-08-11 09:58Z) Record final evidence and outcomes in this plan.
- [x] (2026-08-11 10:38Z) Inspect PR CI and update the obsolete self-type census witness to require the new resolved result.
- [x] (2026-08-11 11:08Z) Preserve unapplied generic function references while filtering invoked anonymous overloads.

## Surprises & Discoveries

- Observation: Current master already resolves a class member through a Scala self-type.
  Evidence: `scala_self_type_is_a_class_member_visibility_tier_not_an_ancestor` in `tests/suite_usages/usages_scala_graph_test.rs` covers `self: BoundMailbox => systemQueueGet` and precedence against the enclosing class hierarchy.

- Observation: Issue 1857 intentionally classifies anonymous-class members as members, but it still reports `no_indexed_definition` because no declaration owner exists.
  Evidence: `tests/suite_issues/issue_1857_scala_scope_boundaries.rs` names issue 1860 as the remaining model gap.

- Observation: Java provides the nearest identity precedent. It uses a synthetic source-position segment named `$anon$line:column` for a lambda owner. Kotlin explicitly excludes anonymous objects from its declaration tier.
  Evidence: `lambda_code_unit` in `crates/bifrost-jvm/src/java/declarations.rs` and the module contract in `crates/bifrost-jvm/src/kotlin/declarations.rs`.

- Observation: Current master handles one self-type bound, but it ignores later bounds in a compound self type.
  Evidence: The new `self: Parser with Tokens =>` regression initially resolved neither the call through `Parser` nor the field through `Tokens` correctly. The syntax helper returned one compound node instead of its component types.

- Observation: Publishing named types nested in an anonymous body changes existing refinement precedence.
  Evidence: `scala_usage_finder_bridges_anonymous_refinement_type_members` failed when the anonymous pass published a nested named type. Limiting the pass to direct methods, fields, and type aliases preserves the named-template boundary.

- Observation: The three primary Rust CI jobs failed on the same expected behavior change.
  Evidence: `census_scala_bare_call_keeps_self_type_member_evidence` still required `no_definition`, tier 1, and `Missing`. The implemented self-type model now returns `fx.Tokens.ws` with a `Consistent` classification.

- Observation: Applying call-shape filtering to a type-argument-only reference rejects an unapplied generic function.
  Evidence: Both Linux jobs failed `scala_unapplied_generic_function_resolves_to_definition` for `val reference = generic[Int]`. Type arguments select the generic declaration but do not supply a value argument list.

## Decision Log

- Decision: Treat self-type member visibility and anonymous-class declarations as separate acceptance areas.
  Rationale: Current master has structured self-type resolution. Anonymous members still have no `CodeUnit`. Combining both changes in resolver logic would hide the declaration-model gap.
  Date/Author: 2026-08-11 / Codex

- Decision: Give each anonymous class a synthetic class `CodeUnit` with a source-position `Nested` segment. Keep its direct methods, fields, and type aliases non-synthetic. Leave nested named templates with the existing named-template pass.
  Rationale: The source position makes two anonymous classes in one lexical owner distinct. The synthetic owner stays out of normal symbol completion, while its direct named children remain available for navigation. The direct-only boundary preserves existing refinement precedence.
  Date/Author: 2026-08-11 / Codex

- Decision: Attach the synthetic class to the smallest source-backed declaration range that contains the `instance_expression`.
  Rationale: This gives an anonymous class inside a field, method, or another anonymous class the correct lexical owner. It also avoids adding mode flags to the existing named-template visitor.
  Date/Author: 2026-08-11 / Codex

- Decision: Derive anonymous-class parent types from tree-sitter nodes and store them as normal Scala supertype facts.
  Rationale: Sibling-member lookup and inherited-member lookup need the existing hierarchy machinery. Repository rules prohibit parsing Scala type syntax with string splitting.
  Date/Author: 2026-08-11 / Codex

- Decision: Keep `target_is_physically_unique` fail-closed for replica declarations.
  Rationale: Issue 1860 records this as a policy question. The existing exact-source test proves the safe behavior. This task does not provide authority to weaken it.
  Date/Author: 2026-08-11 / Codex

- Decision: Expand compound self types into their component type nodes for both forward and inverse lookup.
  Rationale: Each bound contributes visible members. Storing or resolving the compound text as one type loses this relation.
  Date/Author: 2026-08-11 / Codex

## Outcomes & Retrospective

The implementation now creates a stable synthetic class owner for each Scala anonymous template. It publishes direct methods, fields, and type aliases under that owner. Parser-derived supertype facts connect the owner to the normal hierarchy logic.

Forward and inverse member lookup now use all compound self-type bounds. Anonymous overload calls remain inside their synthetic owner. Existing named refinement precedence and replica source-set exactness remain unchanged.

The new issue suite has four passing tests. Issue 1857 has six passing tests. The two existing refinement and replica tests pass. The JVM crate tests and featureless workspace clippy also pass. The semantic census witness now requires the resolved self-type result.

## Context and Orientation

`crates/bifrost-jvm/src/scala/declarations.rs` walks a Scala tree-sitter tree and creates the language-neutral `ParsedFile`. Named classes, traits, objects, enums, methods, fields, and type aliases become `CodeUnit` values. A `CodeUnit` is Bifrost's source declaration identity. Parent-child edges record declaration ownership.

An anonymous class is an `instance_expression` with a `template_body`, for example `new Task { def run = 1 }`. The current declaration walk enters named template bodies only. It does not enter expressions inside method bodies or field initializers. Thus, it cannot publish the anonymous class or its members.

`crates/bifrost-jvm/src/scala/supertypes.rs` extracts structured parent-type paths for named templates. The anonymous owner must use the same fact format. `crates/bifrost-analysis/src/analyzer/usages/get_definition/scala.rs` and `crates/bifrost-jvm/src/scala/graph/` then use stored hierarchy and ownership facts for forward and inverse resolution.

`crates/bifrost-analysis/src/analyzer/store/epoch.rs` salts the persisted Scala cache format. Adding declarations changes the stored unit set, so the Scala epoch must change.

The issue regression belongs in `tests/suite_issues/issue_1860_scala_anonymous_class_members.rs`. Add one `mod issue_1860_scala_anonymous_class_members;` line to `tests/suite_issues/main.rs`, as required by the shared test-harness policy. Use `InlineTestProject` for all small fixtures.

## Plan of Work

First, add a focused issue test. Use two anonymous implementations with the same method names. Assert that the analyzer publishes each method under a different synthetic owner. Assert that a sibling overload call inside the first anonymous class resolves only to declarations in the first class. Add a compound self-type fixture such as `self: Parser with Tokens =>` and prove that bare references reach members from both bounds. The anonymous declaration assertion must fail on the current parent commit.

Next, extend `crates/bifrost-jvm/src/scala/declarations.rs` with an iterative anonymous-instance pass. Collect only `instance_expression` nodes that own a `template_body`. Process outer instances before inner instances. For each instance, select the smallest already-recorded declaration range that contains it. Create a synthetic class identity below that declaration with a `SegmentKind::Nested` source-position segment. Record the instance range and a readable signature based on its structured parent types.

Then, reuse the existing template-body member visitor for the anonymous body's direct methods, fields, and type aliases. Leave nested named templates with the named-template pass. Do not descend through a local block as if it were a template. Do permit a nested anonymous instance to select the outer anonymous member or class as its nearest recorded owner. Keep traversal iterative.

Extend `crates/bifrost-jvm/src/scala/supertypes.rs` with a parser-derived extractor for the direct constructed types of an `instance_expression`. Reuse `scala_type_lookup_segments`. Ignore arguments and the anonymous template body. Set package prefixes and lexical import scopes exactly as named templates do. Store these facts on the synthetic class so existing hierarchy resolution can find inherited members.

Update `crates/bifrost-jvm/src/scala/bare_name_scopes.rs` only if the new declarations change its documented publication boundary or a behavior test proves a mismatch. Do not add a text-search fallback. Update the Scala cache epoch in `crates/bifrost-analysis/src/analyzer/store/epoch.rs` because the persisted declaration set changes.

## Concrete Steps

Run all commands from `/Users/dave/.codex/worktrees/b9fc/bifrost`.

Add the issue test and run it before implementation:

    cargo test --test suite_issues issue_1860 -- --nocapture

Expect the anonymous declaration assertion to fail on the parent commit. The self-type control should pass.

Implement the declaration model, then run:

    cargo test --test suite_issues issue_1860 -- --nocapture
    cargo test --test suite_issues issue_1857 -- --nocapture
    cargo test --test suite_usages scala_self_type_is_a_class_member_visibility_tier_not_an_ancestor -- --exact
    cargo test --test suite_usages scala_usage_finder_bridges_anonymous_refinement_type_members -- --exact
    cargo fmt
    cargo clippy --workspace --all-targets -- -D warnings

The issue tests must pass. The issue 1857 diagnostic test will need an intentional expectation update if the new declaration now resolves the previously missing sibling member. Preserve its layout-independence assertion.

## Validation and Acceptance

Acceptance requires these observable behaviors:

An analyzer declaration query for a method inside `new Task { ... }` returns one source-backed method `CodeUnit`. Its owner contains a synthetic source-position segment. A second anonymous class with the same member name has a different owner and a different full identity.

`get_definitions_by_location` on a sibling overload call inside the anonymous body returns declarations only in that body. It does not return the same-name member from another anonymous body or the abstract member on the base trait.

`scan_usages_by_location` for the anonymous member returns its declaration-site and call-site evidence according to the existing same-owner rules. It does not include the second anonymous class.

A trait with `self: Parser with Tokens =>` resolves bare member references from both `Parser` and `Tokens`. The normal enclosing-owner hierarchy keeps its current precedence.

Replica declarations across two source sets remain source-exact. An ambiguous shared consumer still produces no hit under `target_is_physically_unique`.

## Idempotence and Recovery

The tests use temporary projects and can run repeatedly. The declaration pass is deterministic because source positions and traversal order are deterministic. Cache-epoch changes rebuild stale Scala cache rows instead of trying to migrate identities in place.

If the source-position identity collides, keep both the line and column and add a deterministic same-position ordinal only with a failing fixture. Do not use a random identifier. If hierarchy resolution cannot consume the anonymous owner, fix the stored structured parent facts. Do not add source-text matching.

## Artifacts and Notes

Live issue: `https://github.com/BrokkAi/bifrost/issues/1860`.

Current branch at plan start: `1860-fird-escalation-scala-self-type-members-and-anonymous-class-members-are-not-modelled-no-codeunit-minted` at `05ed127c1`, equal to `origin/master`.

The closest prior regression is `tests/suite_issues/issue_1857_scala_scope_boundaries.rs`. It proves the current diagnostic is honest but still missing the declaration.

## Interfaces and Dependencies

No new crate or external dependency is required.

Add one private Scala declaration helper that creates an anonymous owner from an `instance_expression`, its lexical parent `CodeUnit`, and its package context. The returned `CodeUnit` must have `CodeUnitType::Class`, `synthetic = true`, and a structured `SegmentKind::Nested` identity.

Add one parser-backed supertype extractor in `crates/bifrost-jvm/src/scala/supertypes.rs`. It must return the existing `ScalaSupertypeFact` type, so declaration storage and hierarchy resolution need no parallel format.

Revision note: 2026-08-11. Created the plan after live issue verification and current-code research. The design uses the existing Java source-position identity precedent and preserves Scala's fail-closed physical uniqueness rule.

Revision note: 2026-08-11. Completed the implementation and validation. Limited anonymous publication to direct members after a named-refinement regression exposed the required boundary.

Revision note: 2026-08-11. Updated the semantic census witness after PR CI showed that its expected unresolved result described the fixed defect.

Revision note: 2026-08-11. Limited new identifier overload filtering to actual invocation shapes after CI exposed an unapplied generic reference regression.
