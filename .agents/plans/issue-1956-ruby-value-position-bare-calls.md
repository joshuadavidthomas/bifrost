# Give Ruby value-position bare calls structural call rows (#1956)

## Purpose

A Ruby method call written without a receiver and without parentheses is a "bare call". When it stands alone as a statement (`dfb_source` on its own line), Bifrost's structural extraction already turns it into a `call` fact, so an RQL selector such as `(language ruby (call :callee (name "dfb_source")))` finds it. When the same bare call sits in a value position — as a call argument in `dfb_sink(dfb_source)`, or on the right side of an assignment in `value = dfb_source` — extraction keeps it an `identifier` fact. The selector then returns zero rows.

After the #1953 binding correction, a taint policy whose source selector selects zero rows compiles as a clean empty selection. For the DataFlowBench Ruby direct positive, the run reports `Complete` with zero findings: a silent false negative, which the #1951 acceptance rules forbid.

After this change, the query above returns one exact call row for the bare call inside `dfb_sink(dfb_source)`, and the production taint policy retains the positive finding for both value-position fixture forms. A user can verify this by running the focused tests listed in the Validation section.

## The confirmed defect and why the fix has this shape

The structural spec hook that decides a fact's kind is `RubyStructuralSpec::refine_kind` in `crates/bifrost-ruby/src/structural.rs`. Its helper `is_bare_call_identifier` upgrades an `identifier` to `NormalizedKind::Call` only when the identifier's parent tree-sitter node is `body_statement` (statement position). Every value-position identifier keeps the `Identifier` kind.

The upgrade cannot be a purely local rule. In Ruby, whether `x` is a local-variable read or a zero-argument method call depends on whether an assignment to `x` (or a parameter named `x`) appears lexically before the read inside the same method, block, or lambda. The Ruby semantic lowering already implements exactly this rule: `collect_local_bindings` in `crates/bifrost-analysis/src/analyzer/ruby/semantic.rs` builds a `LocalBindingTimeline` per procedure (entry bindings from parameters, plus per-name activation byte offsets from assignments), and `ambiguous_identifier` (same file) treats an identifier as a local read when `timeline.is_active_at(name, byte)` holds and as a bare call otherwise.

`refine_kind` is stateless per node, so a correct implementation needs a per-file precomputation. The extraction driver `crates/bifrost-analysis/src/analyzer/structural/extract.rs` already has exactly one such hook: `StructuralSpec::call_site_context(root, source)` runs once per file, before any kind refinement, and produces a `CallSiteContext` (defined in `crates/bifrost-core/src/analyzer/structural/callable.rs`; today it carries only the C/C++ function-like macro names). The fix threads that context into `refine_kind` and lets the Ruby spec precompute, in one iterative pass over the file, the byte offsets of every identifier that is a value-position bare call.

Rejected alternatives: a per-identifier backward scan from `refine_kind` would be quadratic during workspace initialization; the broad "declare Ruby call enumeration typed-incomplete" workaround (option (b) in the issue) makes every Ruby call-selector run inconclusive and is only a fallback if this implementation blocks; duplicating the binding rule in the structural crate would create a second diverging implementation of a subtle rule, so the analysis is moved to the language crate and reused by the semantic lowering.

## Repository orientation

- `crates/bifrost-core` is the bottom of the crate graph. It owns `StructuralSpec` (the per-language extraction trait, `src/analyzer/structural/spec.rs`) and `CallSiteContext` (`src/analyzer/structural/callable.rs`).
- `crates/bifrost-ruby` is the Ruby language crate. It depends only on core, `tree-sitter`, `tree-sitter-ruby`, and `serde_json`. The structural spec `RubyStructuralSpec` lives in `src/structural.rs`.
- `crates/bifrost-analysis` is the large analysis crate. It depends on the language crates. The extraction driver is `src/analyzer/structural/extract.rs`; the Ruby semantic lowering is `src/analyzer/ruby/semantic.rs`.
- Integration tests live under `tests/<suite>/`. The pinned boundary test to replace is `tests/suite_bench_policy/issue_1953_ruby_call_binding.rs::value_position_bare_calls_are_not_selectable_yet`.

The dependency direction forces the sharing direction: `bifrost-ruby` cannot see `bifrost-analysis`, so the local-binding analysis moves from `semantic.rs` into `bifrost-ruby`, and `semantic.rs` imports it back.

## Milestone 1: move the local-binding analysis into brokk-bifrost-ruby

Create `crates/bifrost-ruby/src/local_bindings.rs` (registered in `lib.rs`). Move these items from `crates/bifrost-analysis/src/analyzer/ruby/semantic.rs`, unchanged in logic:

- `LocalBindingTimeline` (entry bindings set + per-name activation byte map, with `is_active_at` and `active_names_at`), now `pub`.
- The collector (`LocalBindingCollector`) and the traversal functions `collect_local_bindings`, `collect_parameters`, `collect_assignment`, `collect_pattern`, and `callable_parameters`. All traversals are already iterative with explicit stacks; keep them that way.

The semantic version charges `SemanticWork` against a `SemanticBudget` and polls a `CancellationToken`; those types stay in `bifrost-analysis`. Parameterize the shared collector with a small trait so both callers keep their exact behavior:

    pub trait LocalBindingBudget {
        type Error;
        /// One traversal entry: poll cancellation and charge one visited node.
        fn enter_node(&mut self) -> Result<(), Self::Error>;
        /// Poll cancellation before a name insertion is attempted.
        fn before_insert(&mut self) -> Result<(), Self::Error>;
        /// Charge the owned bytes of a newly recorded name.
        fn charge_name(&mut self, name: &str) -> Result<(), Self::Error>;
    }

The charging points map one-to-one onto the current code: `visit()` becomes `enter_node`; the cancellation checks at the top of `insert_entry_name`/`insert_activation` become `before_insert`; the `owned_text_bytes` charges (only when a genuinely new name is recorded) become `charge_name`. Provide `UnboundedLocalBindingBudget` (every method `Ok(())`, `Error = std::convert::Infallible`) for the structural caller.

In `semantic.rs`, keep a private wrapper with the current signature (`source, callable, body, inherited, budget, cancellation -> Result<LocalBindingCollection, RubyLoweringError>`) whose `LocalBindingBudget` implementation reproduces today's `SemanticWork` accounting and `RubyLoweringError::{Cancelled, Budget}` values, so the existing unit tests in `semantic.rs` (budget exhaustion, cancellation, timeline behavior, lambda inheritance) keep passing unmodified. The semantic-side `LocalBindingCollection { timeline, has_parameter_defaults, work }` keeps its `work` field by reading the accumulated work out of the budget adapter after the call; the shared collection carries only `timeline` and `has_parameter_defaults`.

Acceptance: `cargo test -p brokk-bifrost-ruby --lib` and the existing `collect_local_bindings` tests inside `cargo test -p brokk-bifrost-analysis --lib ruby::semantic` compile and pass.

## Milestone 2: thread the per-file context into kind refinement

In `crates/bifrost-core/src/analyzer/structural/callable.rs`, extend `CallSiteContext` with a second field `identifier_call_starts: HashSet<usize>` — the start byte of every identifier node the language adapter proved to be a call site. Add a constructor `with_identifier_call_starts` and a query `is_identifier_call_at(start_byte)`. Derive or hand-write `Default` so both constructors can fill one field each.

In `crates/bifrost-core/src/analyzer/structural/spec.rs`, add `context: &CallSiteContext` as the last parameter of `StructuralSpec::refine_kind` (default body ignores it). Update the one call site in `crates/bifrost-analysis/src/analyzer/structural/extract.rs` (the context is already computed before pass 1) and the eight adapter implementations (`bifrost-rust`, `bifrost-python`, `bifrost-js-ts`, `bifrost-cpp`, `bifrost-php`, `bifrost-ruby`, `bifrost-jvm` kotlin and scala) — all except Ruby just gain an ignored `_context` parameter.

Acceptance: the workspace compiles; no behavior change outside Ruby.

## Milestone 3: the Ruby per-file bare-call precomputation

In `crates/bifrost-ruby/src/structural.rs`, implement:

- `fn call_site_context(&self, root, source) -> CallSiteContext` returning `CallSiteContext::with_identifier_call_starts(bare_call_identifier_starts(root, source))`.
- `refine_kind`: replace the `is_bare_call_identifier` statement-position rule with `node.kind() == "identifier" && context.is_identifier_call_at(node.start_byte())`.
- In `extract` for `NormalizedKind::Call`, replace the `is_bare_call_identifier(node)` guard with `node.kind() == "identifier"` (an identifier fact can only carry the Call kind through the refinement above), and delete `is_bare_call_identifier`.

`bare_call_identifier_starts` mirrors the semantic scope model with two iterative walks per scope, no recursion:

1. Scope enumeration. A work queue of `(scope root node, optional inherited timeline index)` starts at the file root (`program`). Scope roots, matching `callable_shape` in the semantic lowering: `program`; `class`/`module`/`singleton_class` (body field, fresh scope); `method`/`singleton_method` (fresh scope with parameters); `lambda` (its `body` field is the `block`/`do_block` wrapper, same scope); `block`/`do_block` not directly under a `lambda`. Only `lambda`/`block`/`do_block` scopes inherit: the parent timeline's `active_names_at(scope start byte)` seed the child's entry bindings, exactly as the semantic lowering does. Each scope's timeline comes from the shared `collect_local_bindings` with the unbounded budget; timelines are stored in a `Vec` and referenced by index, so parents are computed before children and nothing is rescanned per identifier.
2. Classification. For each scope, walk the scope subtree with an explicit stack, skipping nested scope roots (they are queued with their parent timeline index instead). For every `identifier` node in a recognized value-read position whose name is not `is_active_at(name, start_byte)` in the scope's timeline, record `start_byte` in the result set.

"Recognized value-read position" is a closed, grammar-field-based list — no source-text parsing. An identifier is a value read when its parent node kind is one of the statement containers (`program`, `body_statement`, `then`, `else`, `do`, `block_body`, `begin`, `parenthesized_statements`, `interpolation`), an argument context (`argument_list`, or the argument wrappers `splat_argument`, `hash_splat_argument`, `block_argument`), an operand context (`binary`, `range`, `array`, `element_reference`, or `unary` whose `operator` field is not `defined?`), a condition or branch context (`if`, `unless`, `elsif`, `while`, `until`, `conditional`, `case`, `when`, and the `*_modifier` statement forms), a jump argument (`return`, `break`, `next`), or when the identifier is the `value` field of a `pair`, the `right` field of an `assignment`/`operator_assignment`, or the `receiver` field of a `call`. Every position not on the list keeps the `Identifier` kind — the honest status quo for constructs this pass does not understand (for example pattern-match binders and parameter lists), so ambiguous or unsupported states are preserved rather than guessed. Note one deliberate behavior correction inside the list: statement position is now also gated by the timeline, so a trailing local-variable read such as `x` at the end of `def f(x); ...; x; end` stops producing a call row it never deserved; this matches the semantic model, which lowers that read as a lexical input flow, not a call.

Acceptance: new unit tests in `crates/bifrost-ruby` (Milestone 4) pass; the two existing structural expectations for statement-position bare calls (`ruby_bare_call_has_a_structured_call_site` and `exact_ruby_call_spans_use_the_terminal_callee_for_every_ordinary_form` in `crates/bifrost-analysis/src/analyzer/usages/get_definition/call_sites.rs`) still pass because their fixtures use unbound names.

## Milestone 4: tests

Unit tests in `crates/bifrost-ruby` (lib tests beside the new code, driving `bare_call_identifier_starts` and, where useful, `refine_kind` through a parsed tree) covering at minimum:

- `dfb_sink(dfb_source)` — argument-position bare call classified as a call.
- `value = dfb_source` — assignment-value bare call classified; the left-hand `value` is not.
- `dfb_source()` and statement-position `dfb_source` — unchanged call classification (the parenthesized form is a `call` grammar node and never enters the identifier path).
- A local variable named `dfb_source` (assigned before use, read in argument position) — stays an identifier.
- A parameter named `dfb_source` read in argument position — stays an identifier.
- A same-named receiver method `helper.dfb_source` — the `call` node is the call; the method-name identifier is not separately classified.
- Nested calls `dfb_sink(wrap(dfb_source))` — the innermost bare call is classified.
- Block-local and lambda scope inheritance — a local assigned in the enclosing method stays a local inside a block; an unassigned name inside the block is a bare call.
- Timeline ordering — a read lexically before its assignment (`dfb_sink(v); v = 1`) is a bare call at the read.

Integration: in `tests/suite_bench_policy/issue_1953_ruby_call_binding.rs`, delete `value_position_bare_calls_are_not_selectable_yet` and replace it with a positive test (same two fixtures) asserting `assert_reached_propagation`, exact bound endpoint spans via `bound_endpoint_spans` (the source span is the bare `dfb_source` identifier), one retained finding per fixture on the production policy route, and non-empty `taint_analysis_results`. Update the module doc comment that describes the old boundary.

## Milestone 5: validation, commit, PR

Run exactly the focused local checks (workspace root):

    cargo fmt
    cargo test -p brokk-bifrost-ruby --lib
    cargo test --test suite_bench_policy issue_1953_ruby_call_binding
    cargo test --test suite_bench_policy issue_1951_balanced_policy::ruby
    cargo test --test suite_semantic ruby_balanced_source_call
    cargo clippy -p brokk-bifrost-ruby --all-targets -- -D warnings
    git diff --check

Do not run the workspace suite, all-features checks, NLP tests, or DataFlowBench locally. Commit the focused change on the current feature branch, push, and open a ready PR that closes #1956. The PR CI matrix is the full-suite gate; monitor it to completion. `master` currently has a policy-scan warning failure — compare any policy-scan failure against `master` and do not absorb unrelated findings.

## Progress

- [x] ExecPlan written.
- [x] Milestone 1: shared local-binding module in `brokk-bifrost-ruby` (`src/local_bindings.rs`); semantic lowering reuses it through `SemanticLocalBindingBudget`; the pre-existing budget/cancellation/timeline unit tests in `semantic.rs` pass unmodified.
- [x] Milestone 2: `CallSiteContext` gained `identifier_call_starts`; `refine_kind` receives `&CallSiteContext`; all eight adapter impls updated; featureless workspace compiles with no warnings.
- [x] Milestone 3: `bare_call_identifier_starts` in `crates/bifrost-ruby/src/structural.rs` wired through `call_site_context`; `is_bare_call_identifier` deleted.
- [x] Milestone 4: 16 lib unit tests in `crates/bifrost-ruby`; the pinned `value_position_bare_calls_are_not_selectable_yet` replaced by two exact-binding positives that retain the finding. One semantic-lowering correction was required (see Surprises).
- [x] Milestone 5: focused checks green locally (ruby lib tests, issue-1953 suite 8/8, issue-1951 ruby 2/2, semantic ruby_balanced 2/2, plus ruby-filtered analysis lib, cross-language, usages, and semantic suites; fmt, clippy -p ruby, git diff --check). PR #1985 open and its full CI matrix passed (0 non-pass checks).

## Surprises & Discoveries

- `defined?(x)` wraps its operand in a `parenthesized_statements` node, so the identifier's parent is the parentheses, not the `unary`. The classification climbs through consecutive `parenthesized_statements` and rejects the read when the eventual ancestor is a `defined?` unary. The bare-operand spelling `defined? x` is the plain `unary`/`operand` case. Both are pinned by `defined_operands_are_not_classified`.
- The first run of the new policy positives failed with `taint plan compilation failed: distinct value-flow carriers share one stable key`. A carrier's stable key for a plain value is `(source anchor, value-kind label, ordinal)` (`value_flow/model.rs::value_key`). The Ruby lowering's `bare_call_expression` created its call-result temporary with `self.value(...)` — uncached — while the enclosing call's `semantic_call_argument` created a second temporary for the same identifier node through the `expression_values` cache. Two distinct temporaries anchored at one identifier collide the moment both become carriers, which is exactly what a bound source (return value) plus a bound sink argument produce. `call_expression` already routed its result through `expression_value`; `bare_call_expression` now does the same, which also makes the source's return carrier and the sink's argument carrier the same value, so the direct flow is found. `ADAPTER_VERSION` bumped to `ruby-value-semantics-v3` because lowering output changed.
- The `span_of` fixture helper requires unique needles; the assignment fixture's `dfb_sink(value)` collided with the definition header `def dfb_sink(value)`, so the definition parameter is named `arg` there.
- CI caught one stale fixture the local ruby-filtered runs missed: the planner test `formerly_unsupported_languages_are_searched_after_adapter_registration` pinned one Ruby call row for `def run; eval(input); end`, where the unbound `input` argument is now honestly a second bare-call row. The fixture declares `input` as a parameter now, preserving the test's intent; the full `suite_cross_language` passes.

## Decision Log

- Reuse direction: move `collect_local_bindings` into `brokk-bifrost-ruby` behind a budget trait, rather than duplicating the rule in the structural spec or moving the structural spec into `bifrost-analysis`. Reason: single source of truth for a subtle lexical rule; the crate graph only permits this direction.
- Context plumbing: extend the existing `CallSiteContext` per-file hook and pass it to `refine_kind`, rather than adding a second parallel context type. Reason: the driver already computes this context once per file before refinement; identifier-call classification is call-site knowledge.
- Classification is a closed whitelist of value-read positions. Unlisted positions keep `Identifier`. Reason: the issue demands that ambiguous or unsupported states be preserved; a whitelist can only move a position from "silently unselectable" to "exactly selectable", never invent a call from a binder.
- Statement position becomes timeline-gated (behavior correction for trailing local reads). Reason: parity with the semantic model; a call row for a local read can never bind to a semantic call site and would stay a permanent capability failure.
- Scope walks run under `UnboundedLocalBindingBudget`. Reason: the structural extraction driver bounds work by source bytes and fact count already, and the only existing per-file context precomputation (C/C++ macro scan) is likewise unbudgeted.
- The Ruby semantic lowering's `bare_call_expression` result value became the node's cached expression value, and `ADAPTER_VERSION` moved to v3. Reason: two temporaries anchored at one identifier violate the value-flow plan's stable-carrier-key invariant as soon as a bound source and a bound sink argument both materialize carriers there; `call_expression` already used the cache, so this is a consistency correction at the root cause, not a workaround in the plan compiler.

## Outcomes & Retrospective

Delivered as PR #1985 (closes #1956); the full CI matrix passed. Ruby value-position bare calls (`dfb_sink(dfb_source)`, `value = dfb_source`, nested arguments, receivers, conditions, operands) now produce structural `call` rows gated by the same local-binding timeline the semantic lowering uses, so RQL call selectors select them exactly and the production taint policy retains the DataFlowBench positive with exact endpoint anchors instead of reporting an optimistic clean `Complete`. The pinned boundary test became two exact-binding positives; 16 lib unit tests pin the classification whitelist, shadowing, capture inheritance, lexical ordering, and the `defined?` exclusion.

Two lessons. First, the fix needed one semantic-lowering correction that only became visible once selection worked: `bare_call_expression` minted an uncached result temporary, and two temporaries at one anchor collide in the value-flow plan's stable carrier keys; routing the result through the expression-value cache (as `call_expression` always did) fixed the collision and made the source and sink carriers meet. Second, name-filtered local test runs (`cargo test ... ruby`) do not cover fixtures that merely contain Ruby files under other test names; the one CI failure was exactly such a fixture, so budget a full suite run for changes that alter cross-language fact extraction.
