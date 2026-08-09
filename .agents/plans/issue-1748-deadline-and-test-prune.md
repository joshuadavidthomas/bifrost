# Make `scan_usages` honour its own deadline through the type hierarchy, and stop building the half of the hierarchy index the request already said it does not want

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

The canonical rules for ExecPlans in this repository are `.agents/PLANS.md`, from the
repository root. This document must be maintained in accordance with that file.

Issue: BrokkAi/bifrost#1748. Implementation branch: `bifrost-nlp-ft`. Base commit for the
citations below: `f68bc2b8` ("Merge remote-tracking branch 'origin/master' into
bifrost-nlp-ft"), which is the tree after the usage-v2 re-land and the deletion of the v1
`RustUsageIndex`. Every file and line reference in this plan was re-verified at that commit.

## Purpose / Big Picture

A user of the Bifrost MCP server can ask "where is this symbol used?" with a time budget:
`scan_usages_by_reference` takes a `max_duration_secs` argument. Today that budget is a
promise the code cannot keep. On a large C++ workspace a call that asked for 30 seconds ran
for about 1,200 seconds and then died on the host's own request budget with a transport
error and zero results. The 30 seconds were never enforced, and the caller got nothing at
all -- not even the partial answer the machinery is capable of reporting.

After this change, that same call stops near its own deadline and returns a structured
partial answer: the usages found so far, plus a completion marker that says the scan ran out
of time rather than that the symbol is unused. The difference a user sees is a 30-second
answer with a "time budget" flag instead of a 20-minute wait ending in an error.

The same change stops the scan from building half of an index it was told to ignore. When a
request passes `include_tests: false`, the code today still walks every test class in the
workspace to build a whole-workspace class-hierarchy index, and only throws the test files
away afterwards. On the workspace that produced the incident, 52.3% of the per-file builds
inside that index were test-side. After this change the excluded half is never built.

Both effects are observable from tests without a large corpus: the C++ analyzer exposes a
build counter (`visible_type_units_build_count_for_test`), and the cancellation token has a
deterministic "stop after N checks" test constructor, so a small inline fixture can prove
both "the loop stopped early" and "the excluded classes were never built".

## Definitions

These terms recur below. They are defined once here and used with exactly these meanings.

A **`CodeUnit`** is Bifrost's identity for one declaration: a class, a function, a field. It
carries a fully qualified name and the `ProjectFile` it was declared in
(`crates/bifrost-core/src/analyzer/model.rs`).

A **`ProjectFile`** is one source file inside the analyzed workspace, identified by its path
relative to the workspace root.

A **`CancellationToken`** (`crates/bifrost-core/src/cancellation.rs`) is a cloneable
cooperative-stop flag. Nothing forcibly interrupts a thread: long loops must call
`token.is_cancelled()` at explicit checkpoints and stop themselves. The token distinguishes
a wall-clock deadline (`is_timed_out()` becomes true) from an explicit caller cancellation,
which is what lets the tool report "time budget" rather than "cancelled".

A **poll point** is one such explicit `is_cancelled()` call inside a loop.

A **`TypeHierarchyProvider`** (`crates/bifrost-core/src/analyzer/capabilities.rs`) is the
capability trait every language analyzer implements to answer "what are this class's
supertypes / subtypes". Its `get_descendants` walks the whole subtype tree.

The **descendant index** (`DirectDescendantIndex`, same file) is a whole-workspace table
mapping each class to its direct subclasses. It is built once per analyzer and memoized,
because inverting the ancestor relation requires visiting every class in the workspace.

A **`PoolSafeMemo`** (`crates/bifrost-core/src/analyzer/pool_memo.rs`) is a build-once cell
with a deadlock-safe claim protocol for rayon workers. `KeyedPoolSafeMemo` is one such cell
per key. Both publish **complete-or-nothing**: a build that stops short stores nothing, so a
later caller never receives a truncated value as if it were the whole answer.

The **include-closure** of a C++ file is that file plus every file reachable from it through
`#include`, transitively. `build_cpp_visible_type_units`
(`crates/bifrost-cpp/src/hierarchy.rs`) walks it to collect every class name visible at that
file, which is what C++ base-specifier resolution needs.

**Test-like** is the path-driven verdict `is_test_like_file`
(`crates/bifrost-analysis/src/searchtools/scan_usages.rs`): a file under a test directory, a
file with a test filename convention, or a file the analyzer's module index says is reachable
only from test-gated code.

## Progress

- [x] (2026-08-09) Re-verified every citation from the read-only attribution at `f68bc2b8`.
- [x] (2026-08-09) ExecPlan written to `.agents/plans/issue-1748-deadline-and-test-prune.md`.
- [x] (2026-08-09) Milestone 1: cancellable, scope-filtered descendant-index build in
      `brokk-bifrost-core`, plus the two new `TypeHierarchyProvider` methods.
- [x] (2026-08-09) Milestone 2: C++ providers poll (`build_cpp_visible_type_units` per BFS
      pop; the analyzer's per-class ancestor asks) and publish complete-or-nothing.
- [x] (2026-08-09) Milestone 3: mechanical fan-out to the other hierarchy providers.
- [x] (2026-08-09) Milestone 4: `candidates.rs` / `finder.rs` / `scan_usages.rs` carry the
      scope, so the request's deadline and its `include_tests` answer both reach the build.
- [x] (2026-08-09) Milestone 5: tests, with fail-before evidence recorded below.
- [x] (2026-08-09) Milestone 6: gate (fmt, nextest workspace, doctests, all-features clippy).
- [x] (2026-08-09) Milestone 7: reproducer measurement, or the recorded reason it could not
      be run on this host.

## Surprises & Discoveries

- Observation: `PoolSafeMemo` already has exactly the complete-or-nothing cancellable form
  this change needs -- `get_or_build_while(keep_going, build_parallel, build_serial)` returns
  `Option<Arc<T>>` and publishes nothing when the build returns `None`
  (`crates/bifrost-core/src/analyzer/pool_memo.rs:421-447`). No new memo primitive was
  required; the descendant-index cells only had to move from `PoolSafeMemo<T>` to
  `KeyedPoolSafeMemo<DescendantIndexVariant, T>` and switch method.

- Observation: changing the *existing* `TypeHierarchyProvider` method signatures was not
  viable. `get_direct_descendants` and `get_ancestors` have roughly 150 call sites, most of
  them in analyzer tests that have nothing to do with deadlines. The change is therefore
  additive: two new trait methods carrying the scope, with defaults that delegate to the
  uncancellable forms, and overrides only where a build actually walks the workspace.
  Evidence: `grep -rn "get_direct_descendants\|get_ancestors" --include=*.rs` at `f68bc2b8`.

- Observation: the exclusion predicate cannot be the finder's existing `file_filter`. That
  filter is the conjunction of the test exclusion *and* the request's optional path filter
  (`scoped_usage_finder`, `crates/bifrost-analysis/src/searchtools/scan_usages.rs`). A path
  filter is per-request and arbitrary, so keying a cross-request shared index by "some filter
  was present" would let one request's path scope poison another's index. Only the test
  exclusion -- a pure function of the analyzer and the file -- is safe to key by a bool, so
  it travels down its own channel.

- Observation (fail-before, Fix A): with the scope plumbed all the way to
  `build_cpp_visible_type_units` but its per-pop poll deleted, the build counter goes from
  8 to 38 include-closure builds under the same 40-check budget, and
  `issue_1748_a_cpp_descendant_index_build_stops_at_the_scan_deadline` fails its build-count
  assertion. The 38 is the per-candidate poll in the core builder working alone: it stops
  after roughly one check per class, which is 63% of the fixture's classes rather than 13%.
  Transcript in `Artifacts and Notes`.

- Observation (fail-before, Fix B): with the variant-keyed memo in place but
  `DescendantIndexScope::admits` hardwired to `true`, the production-only build charges
  85 include-closure builds instead of 65 -- exactly the 20 excluded test headers -- and
  `issue_1748_b_excluding_tests_never_builds_the_test_classes` fails.

- Observation: Rust and Go still build their whole-workspace hierarchy index behind a plain
  `OnceLock` (`RustHierarchyIndex::build`, `GoHierarchyIndex::build`), so they keep the
  trait's bounded-by-construction default and their builds remain unpolled. Neither was
  implicated by the trace and neither has the per-class-times-per-file shape that made the
  C++ build unbounded -- each is a single pass. Making them cancellable means changing their
  memo from `OnceLock` to a `PoolSafeMemo`, which is a separate change with its own
  worker-parking argument to make. Recorded here so it is a known gap rather than an
  oversight.

## Decision Log

- Decision: add two new `TypeHierarchyProvider` methods (`get_direct_descendants_within`,
  `get_descendants_within`) carrying a `DescendantIndexScope`, instead of adding a parameter
  to the existing ones.
  Rationale: the existing methods have ~150 call sites, nearly all in analyzer tests that ask
  a purely structural question with no deadline. A required-parameter change would be a
  mechanical edit of every one of them and would obscure the actual fix in review. The
  additive form still makes every workspace-walking build cancellable, because the default is
  overridden exactly where such a build exists.
  Date/Author: 2026-08-09, implementation agent.

- Decision: carry the deadline and the workspace slice together in one `DescendantIndexScope`
  value rather than as two parameters.
  Rationale: both are request state, both are consumed by the same loop, and both must key
  or gate the same memo. A single value also gives the memo key (`variant()`) one obvious
  home. `CLAUDE.md`'s rule against flag parameters is about Booleans that select between two
  behaviours inside one function; this is a description of what index is being asked for,
  and the two variants are genuinely different indexes with their own memo cells.
  Date/Author: 2026-08-09, implementation agent.

- Decision: `DescendantIndexScope::excluding_sources` takes a caller-supplied
  `&dyn Fn(&ProjectFile) -> bool` rather than moving test classification into
  `brokk-bifrost-core`.
  Rationale: `CLAUDE.md` forbids new crate dependency edges of this shape, and
  `is_test_like_file` needs an `IAnalyzer` (`file_is_test_only`), which core must not know
  about. The closure crossing is the same pattern `RustUsageWalks` already uses for its
  `keep_going` predicate.
  Date/Author: 2026-08-09, implementation agent.

- Decision: the memo key is a two-valued enum (`WholeWorkspace` / `ProductionOnly`), so at
  most two descendant indexes exist per analyzer.
  Rationale: the excluded set is a pure function of the analyzer and the file, so every
  `include_tests: false` request describes the same index. Keying by the predicate's identity
  would be unbounded and pointless. This is asserted in the type's doc comment because it is
  the correctness condition for the key.
  Date/Author: 2026-08-09, implementation agent.

- Decision: a stopped build publishes nothing, and `get_descendants_within` returns `None`
  rather than a partial descendant list.
  Rationale: house precedent. `cancelled_cold_candidate_discovery_does_not_publish_partial_index`
  (`crates/bifrost-analysis/src/analyzer/usages/rust_graph.rs`) and the #1809 rule that every
  walk cache is gated on `keep_going` both say the same thing: a truncated answer memoized as
  complete is served to every later caller as the truth. `PoolSafeMemo::get_or_build_while`
  enforces it structurally.
  Date/Author: 2026-08-09, implementation agent.

- Decision: the finder-layer `candidates.retain(filter)` stays exactly where it is.
  Rationale: it is the correctness backstop. The build-side prune is an optimisation that
  several providers legitimately ignore (their indexes stay whole-workspace supersets), and
  the path filter is only ever applied at the finder. Removing the retain would make
  correctness depend on every provider honouring the scope.
  Date/Author: 2026-08-09, implementation agent.

- Decision: Scala's provider polls its per-candidate descendant loop but does not variant-key
  its index, and does not gain a build-side exclusion.
  Rationale: Scala already uses the #908 *lazy* hierarchy index -- a single cheap global pass
  that records spelled supertype names, with ancestor resolution deferred to the candidates
  that could match. There is no eager per-class ancestor build to prune, so a second keyed
  variant would double a cheap pass to save nothing.
  Date/Author: 2026-08-09, implementation agent.

- Decision: no cache-sizing change, no per-file ask reordering, no C++ facts substrate in
  this change.
  Rationale: the measured 1.9x duplicate-build factor is moka eviction churn against a 32 MiB
  cap, but Fix B halves the pressure, and the moka-migration lesson (share is not the cost)
  says not to resize blind. These are the re-measure gate, recorded under
  `Explicitly Deferred` below.
  Date/Author: 2026-08-09, implementation agent.

## Outcomes & Retrospective

See the entry at the end of this document, written at completion.

## Context and Orientation

### What the code does today, end to end

A `scan_usages_by_reference` MCP request arrives at
`crates/bifrost-mcp/src/searchtools_service.rs`, which forwards a per-request
`CancellationToken` into `scan_usages_by_reference_with_cancellation`. The scan entry point
(`crates/bifrost-analysis/src/searchtools/scan_usages.rs`) combines that token with the
requested `max_duration_secs`, capped at 300 seconds, using
`CancellationToken::with_timeout`. `with_timeout` takes the *minimum* of any deadlines
already on the token, so the host's cancellation and the scan's own budget compose onto one
token. From here on the deadline exists and is correct.

The scan resolves the requested symbols (this phase polls the token per resolution candidate)
and then builds a `UsageFinder` (`crates/bifrost-analysis/src/analyzer/usages/finder.rs`)
carrying the same token. `UsageFinder::query_with_provider_and_source_budget` runs candidate
discovery, then filters, then runs the language graph walk.

Candidate discovery is `find_default_candidates_with_cancellation`
(`crates/bifrost-analysis/src/analyzer/usages/candidates.rs`). Its first step expands the
target by polymorphism: if the target is a method of a class, every subclass of that class
could also be the thing the caller meant, so their files are candidates too. That expansion
is one call:

    for descendant in provider.get_descendants(&parent) {
        if is_cancelled(cancellation) {
            return candidates;
        }
        all_targets.insert(descendant);
    }

The loop polls. The call does not. `TypeHierarchyProvider::get_descendants` takes no token,
and on a cold analyzer it triggers the whole-workspace descendant-index build behind it. On
the incident workspace that build ran for about twenty minutes inside a call that had asked
for thirty seconds, and the poll loop above was never reached even once.

Behind `get_descendants` there are two unpolled loops:

1. `build_direct_descendant_index_from_candidates`
   (`crates/bifrost-core/src/analyzer/capabilities.rs`) iterates every class-like declaration
   in the workspace and calls `direct_ancestors(&candidate)` on each. Natural checkpoint: one
   per class.
2. For C++, each of those ancestor resolutions needs the include-visible class table of the
   declaring file, which is `build_cpp_visible_type_units`
   (`crates/bifrost-cpp/src/hierarchy.rs`) -- a breadth-first walk of that file's transitive
   `#include` closure. Natural checkpoint: one per file popped from the pending stack.

Each individual leaf is fast. The trace behind the issue recorded 44,971 of these builds in
one request, each between a fraction of a millisecond and about 180 ms. Nothing was slow;
there were simply tens of thousands of unbounded iterations. This is why the earlier fix that
bounded a single long SQL statement (polling every 512 rows) did not help here: it bounds one
statement, and this is a loop over cheap statements.

### Where `include_tests` is applied today

`test_file_exclusion(analyzer, include_tests)`
(`crates/bifrost-analysis/src/searchtools/scan_usages.rs`) builds a request-scoped
`TestFileExclusion`, and `scoped_usage_finder` folds it, together with the optional path
filter, into the finder's `file_filter`. That filter runs at
`candidates.retain(...)` in `UsageFinder::query_with_provider_and_source_budget` -- *after*
candidate discovery has completely finished. Everything described in the previous section
happens before that line. Nothing inside discovery has ever seen `include_tests`.

The verdict itself is available much earlier and costs almost nothing: `is_test_like_file` is
two pure path predicates plus one module-index lookup.

### Why this matters beyond latency

Three of six timed repetitions in an earlier measurement on another workspace spent the whole
budget in a scan prologue and returned before symbol resolution ran. The same shape applies
here: when an early phase does not poll, every structured outcome downstream of it becomes
unreachable, and the tool degrades from "partial answer with a reason" to "error, no data".
Enforcing the deadline restores the reporting channel that already exists
(`UsageQueryCompletion::Cancelled`, `ScanUsagesIncompleteReason::TimeBudget`).

## Plan of Work

The work is six milestones. Milestones 1-3 are the deadline (Fix A). Milestone 4 adds the
test push-down (Fix B) on top of the same carrier. Milestone 5 is the tests, Milestone 6 the
gate, Milestone 7 the measurement.

### Milestone 1 -- a cancellable, scope-filtered descendant-index build in core

Scope: after this milestone `brokk-bifrost-core` can build a descendant index that stops on a
deadline and that can be restricted to a subset of the workspace, and the
`TypeHierarchyProvider` trait can express both. Nothing observable changes yet, because no
caller supplies a scope that ever stops.

Edit `crates/bifrost-core/src/analyzer/capabilities.rs`.

Add, next to `DirectDescendantIndex`:

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub enum DescendantIndexVariant {
        WholeWorkspace,
        ProductionOnly,
    }

    #[derive(Clone, Copy)]
    pub struct DescendantIndexScope<'a> {
        cancellation: &'a CancellationToken,
        excluded_source: Option<&'a dyn Fn(&ProjectFile) -> bool>,
    }

with constructors `whole_workspace(&CancellationToken)` and
`excluding_sources(&CancellationToken, &dyn Fn(&ProjectFile) -> bool)`, and accessors
`cancellation()`, `variant()`, `keep_going()` (returning `impl Fn() -> bool + '_`) and
`admits(&CodeUnit) -> bool`. `variant()` returns `ProductionOnly` exactly when a predicate is
present. Document on the type that the predicate must be a pure function of the analyzer and
the file, because that is what makes the two-valued key a complete key.

Change the two builders to be cancellable and filtered:

    pub fn build_direct_descendant_index<A, P>(
        analyzer: &A,
        provider: &P,
        scope: &DescendantIndexScope<'_>,
    ) -> Option<DirectDescendantIndex>

    pub fn build_direct_descendant_index_from_candidates<F>(
        candidates: Vec<CodeUnit>,
        direct_ancestors: F,
        keep_going: &dyn Fn() -> bool,
    ) -> Option<DirectDescendantIndex>

The first filters `analyzer.all_declarations()` with `scope.admits(...)` *before* collecting,
so an excluded class never reaches `get_direct_ancestors` and therefore never triggers its
include-closure walk. The second polls once per candidate in the edge-building loop and
returns `None` the moment the predicate says stop.

Add the two trait methods, with defaults:

    fn get_direct_descendants_within(
        &self,
        code_unit: &CodeUnit,
        scope: &DescendantIndexScope<'_>,
    ) -> Option<HashSet<CodeUnit>>

    fn get_descendants_within(
        &self,
        code_unit: &CodeUnit,
        scope: &DescendantIndexScope<'_>,
    ) -> Option<Vec<CodeUnit>>

The default `get_direct_descendants_within` checks the token once and delegates to
`get_direct_descendants`; that is correct for a provider whose build is bounded by
construction. The default `get_descendants_within` runs the same breadth-first walk as
`get_descendants` through a new `traverse_hierarchy_while` helper that polls per pop and
propagates `None`.

### Milestone 2 -- the C++ path polls and publishes complete-or-nothing

Scope: after this milestone a C++ descendant-index build stops at the deadline, and a stopped
build leaves the memo empty so the next call rebuilds from scratch rather than reading a
truncated index.

In `crates/bifrost-cpp/src/hierarchy.rs`, give `build_cpp_visible_type_units` a
`keep_going: &dyn Fn() -> bool` parameter and an `Option<Vec<CodeUnit>>` return, polling once
per `pending.pop()`. Give `cpp_resolve_direct_ancestors` the same treatment, because it
consumes the visible-type table.

The `CppSource` trait's `visible_type_units` accessor is what the analyzer memoizes, so it
gains a cancellable sibling that returns `Option<Arc<Vec<CodeUnit>>>`.

In `crates/bifrost-analysis/src/analyzer/cpp/mod.rs`, change
`direct_descendant_index: Arc<PoolSafeMemo<DirectDescendantIndex>>` to
`Arc<KeyedPoolSafeMemo<DescendantIndexVariant, DirectDescendantIndex>>`, in both `from_inner`
and `with_updated_inner`.

In `crates/bifrost-analysis/src/analyzer/cpp/hierarchy.rs`, override
`get_direct_descendants_within` to take the cell for `scope.variant()` and call
`get_or_build_while(&scope.keep_going(), ...)` with a builder that returns
`Option<DirectDescendantIndex>`.

### Milestone 3 -- mechanical fan-out

Scope: every other provider whose descendant index walks the workspace honours the deadline.
Behaviour is otherwise unchanged.

The providers that share the core builder -- C#, Ruby, Python, PHP -- follow C++ exactly:
variant-keyed cell, `get_or_build_while`, scope passed into the core builder.

Java, Kotlin and JS/TS have their own builders in `brokk-bifrost-jvm` and
`brokk-bifrost-js-ts`. Each gains a `keep_going` parameter polled in its per-class or
per-batch loop and an `Option` return, plus a candidate filter so the production-only variant
skips test declarations before resolving their ancestors. Their cells become variant-keyed
too.

Scala polls its per-candidate loop in `ScalaLazyHierarchyIndex::direct_descendants` and keeps
one cell; see the Decision Log for why it gains no second variant.

`MultiAnalyzer` forwards `get_direct_descendants_within` to each sub-provider and propagates
a `None` from any of them.

### Milestone 4 -- the request's deadline and its `include_tests` answer reach the build

Scope: after this milestone the user-visible behaviour changes. This is the milestone whose
acceptance is the issue's own reproducer.

`crates/bifrost-analysis/src/analyzer/usages/candidates.rs`: replace the
`Option<&CancellationToken>` parameter threaded through `find_import_graph_candidates` and
its helpers with `Option<&DescendantIndexScope<'_>>`, and call `get_descendants_within`. On
`None`, return the candidates accumulated so far immediately -- the finder already reports
the completion.

`crates/bifrost-analysis/src/analyzer/usages/finder.rs`: add an `excluded_test_files` field
alongside `file_filter`, set through `with_test_file_exclusion`, and build the scope from it
plus the finder's token. Keep the existing `candidates.retain(filter)` untouched.

`crates/bifrost-analysis/src/searchtools/scan_usages.rs`: `scoped_usage_finder` passes the
`TestFileExclusion` through the new channel in addition to folding it into `file_filter`.

### Milestone 5 -- tests

Every test below must fail before its fix and pass after. The fail-before transcripts belong
in `Artifacts and Notes`.

Fix A, in `tests/suite_usages/`: an `InlineTestProject` C++ fixture with dozens of classes
whose headers include each other, queried through `scan_usages_by_reference` with a token
built by `CancellationToken::timeout_after_checks_for_test(k)`. Assert (a) the scan entry
reports the time-budget incomplete reason, (b)
`visible_type_units_build_count_for_test` is far below the class count, and (c) a fresh call
on the same analyzer with an uncancelled token returns the complete, correct descendant set,
proving no truncated index was published.

Fix B, same suite: a fixture with production classes and test-directory classes.
With `include_tests: false`, the build counter must show the test classes were never built.
With `include_tests: true` on the *same* analyzer, the test descendants must still be
returned -- the two variants must not poison each other.

Keep green: the `issue_1228` cancellation pins in `finder.rs`, the four `suite_usages`
regression suites, and the `suite_symbols` too-broad-scope pins.

### Milestone 6 -- the gate

From the repository root:

    cargo fmt
    cargo nextest run --workspace --all-targets --no-fail-fast
    cargo test --workspace --doc
    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

Check `df -h` before the clippy run: an all-features build can use tens of GiB.

### Milestone 7 -- measurement

If `/mnt/T9/repo-clones` still holds the llvm-project clone from the campaign, run one
`scan_usages_by_reference` with `symbols=["llvm.EVT.isSimple"]`, `include_tests=false`,
`max_duration_secs=30` against it under `/usr/bin/time -v` and record wall and CPU time. The
claim to validate is only that the call terminates near 30 seconds instead of near 1,200. If
the clone is absent, record that, and rely on the test counters for the behavioural pin.

## Concrete Steps

Run everything from `/mnt/optane/bifrost-nlp`.

Focused compile check while editing core and the language crates:

    cargo check -p brokk-bifrost-core -p brokk-bifrost-cpp -p brokk-bifrost-jvm -p brokk-bifrost-js-ts

Focused test run for the new suites:

    cargo nextest run -p brokk-bifrost --test suite_usages

Full gate as in Milestone 6.

## Validation and Acceptance

Acceptance is behavioural, in three parts.

First, a scan that runs out of time says so and still answers. Run the new test
`issue_1748_a_cpp_descendant_index_build_stops_at_the_scan_deadline`. It must pass, and with
the poll inside `build_cpp_visible_type_units` removed it must fail on the build-count
assertion. That is the whole claim of Fix A reduced to a counter.

Second, an excluded class is never built. Run
`issue_1748_b_excluding_tests_never_builds_the_test_classes`. It must pass, and must fail if
`DescendantIndexScope::admits` is made unconditionally true.

Third, the two index variants are independent. Run
`issue_1748_b_including_tests_still_sees_test_descendants_on_the_same_analyzer`. Asking the
same analyzer first without and then with test files must return the test subclasses on the
second ask.

Beyond the tests, the acceptance a user can see is the reproducer in Milestone 7: a request
with `max_duration_secs: 30` returns in roughly 30 seconds with an incomplete-by-time-budget
marker, rather than running for about 1,200 seconds and failing with a transport error.

## Idempotence and Recovery

Every step is an ordinary source edit under version control; re-running the plan on an
already-patched tree is a no-op. The build helper
`scripts/with-isolated-cargo-target.sh` removes its own target directory on success, failure
or interruption, so an aborted clippy run leaves nothing behind. No migration, no persisted
state, no schema change: the descendant index is an in-memory memo rebuilt on demand, so a
partially applied change cannot corrupt anything on disk.

## Explicitly Deferred, with a re-measure gate

None of the following is in scope, and none should be attempted before the reproducer is
re-run after Fix A and Fix B.

Cache sizing. `visible_type_units_by_file` is a moka weighted cache capped at
`memo_budget / 8`, which is 32 MiB by default, against a retained demand in the multi-GB
range on the incident workspace. That cap is the mechanism behind the measured 1.9x duplicate
builds (44,971 builds over 23,216 distinct files; one file was built 91 times). Fix B halves
the pressure on it, and the moka-migration lesson is that sharing is not the cost, so
resizing before re-measuring would be guessing.

Ask ordering. The index asks per class, and `candidates.sort()` orders by declaration, not by
file, so one file's classes are scattered across the walk and its cache entry is often
evicted between the first and the last. Reordering the asks per file is the obvious follow-up
and is entirely a question for the post-fix histogram.

A C++ facts substrate -- per-blob hierarchy facts with query-time composition, the shape the
Rust usage-v2 work established -- is the real long-term answer for this index. It is a
separate designed effort. Do not smuggle it in here.

## Interfaces and Dependencies

In `crates/bifrost-core/src/analyzer/capabilities.rs`, at the end of this work these must
exist:

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub enum DescendantIndexVariant { WholeWorkspace, ProductionOnly }

    pub struct DescendantIndexScope<'a> { /* private */ }

    impl<'a> DescendantIndexScope<'a> {
        pub fn whole_workspace(cancellation: &'a CancellationToken) -> Self;
        pub fn excluding_sources(
            cancellation: &'a CancellationToken,
            excluded: &'a dyn Fn(&ProjectFile) -> bool,
        ) -> Self;
        pub fn cancellation(&self) -> &CancellationToken;
        pub fn variant(&self) -> DescendantIndexVariant;
        pub fn keep_going(&self) -> impl Fn() -> bool + '_;
        pub fn admits(&self, declaration: &CodeUnit) -> bool;
    }

    pub trait TypeHierarchyProvider: CapabilityProvider + Send + Sync {
        fn get_direct_descendants_within(
            &self,
            code_unit: &CodeUnit,
            scope: &DescendantIndexScope<'_>,
        ) -> Option<HashSet<CodeUnit>>;

        fn get_descendants_within(
            &self,
            code_unit: &CodeUnit,
            scope: &DescendantIndexScope<'_>,
        ) -> Option<Vec<CodeUnit>>;
    }

    pub fn build_direct_descendant_index<A, P>(
        analyzer: &A, provider: &P, scope: &DescendantIndexScope<'_>,
    ) -> Option<DirectDescendantIndex>;

    pub fn build_direct_descendant_index_from_candidates<F>(
        candidates: Vec<CodeUnit>, direct_ancestors: F, keep_going: &dyn Fn() -> bool,
    ) -> Option<DirectDescendantIndex>;

In `crates/bifrost-cpp/src/hierarchy.rs`:

    pub fn build_cpp_visible_type_units(
        cpp: &dyn CppSource, file: &ProjectFile, keep_going: &dyn Fn() -> bool,
    ) -> Option<Vec<CodeUnit>>;

    pub fn cpp_resolve_direct_ancestors(
        cpp: &dyn CppSource, code_unit: &CodeUnit, keep_going: &dyn Fn() -> bool,
    ) -> Option<Vec<CodeUnit>>;

No new crate is added and no new dependency edge is introduced. `brokk-bifrost-core` gains no
dependency: `CancellationToken`, `CodeUnit` and `ProjectFile` all already live there.

## Artifacts and Notes

The five new tests live in `tests/suite_usages/issue_1748_hierarchy_deadline.rs`, registered
from `tests/suite_usages/main.rs`. The fixture is 60 production subclasses of one base class,
each in its own header over a four-link `#include` chain, plus 20 subclasses under `tests/`
for the Fix B half.

Passing, from the repository root:

    cargo nextest run --test suite_usages issue_1748 --no-fail-fast

    PASS issue_1748_hierarchy_deadline::issue_1748_a_cpp_descendant_index_build_stops_at_the_scan_deadline
    PASS issue_1748_hierarchy_deadline::issue_1748_a_scan_reports_time_budget_when_the_hierarchy_build_runs_out
    PASS issue_1748_hierarchy_deadline::issue_1748_b_excluding_tests_never_builds_the_test_classes
    PASS issue_1748_hierarchy_deadline::issue_1748_b_including_tests_still_sees_test_descendants_on_the_same_analyzer
    PASS issue_1748_hierarchy_deadline::issue_1748_b_test_exclusion_still_decides_which_files_a_scan_answers_from
    Summary 12 tests run: 12 passed

Fix A fail-before. The poll inside `build_cpp_visible_type_units` was deleted, leaving the
scope plumbed everywhere else, and the same test re-run:

    FAIL issue_1748_a_cpp_descendant_index_build_stops_at_the_scan_deadline
    the build must stop well short of walking every class: 38 include-closure builds for 60 subclasses

With the poll restored the same counter reads 8. Both numbers are from an instrumented run
of the identical fixture and the identical 40-check budget; only the poll differed.

Fix B fail-before. `DescendantIndexScope::admits` was replaced by an unconditional `true`:

    FAIL issue_1748_b_excluding_tests_never_builds_the_test_classes
    production_builds = 85   (65 with the prune)

65 is exactly one build per production class header: 60 subclass headers, `base.h`, and the
four chain headers. 85 adds the 20 excluded test headers, one each.
