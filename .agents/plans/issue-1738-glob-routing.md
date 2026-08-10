# Route `get_summaries` directory and glob targets off the analyzed-file universe

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

The canonical rules for this document are in `.agents/PLANS.md` (repository root relative).
Maintain this file in accordance with that file.

## Purpose / Big Picture

Today a `get_summaries` call whose target is a glob (`layout/style/**/*.cpp`) or a
directory-shaped path costs time proportional to the whole workspace multiplied by the
number of analyzer languages, even when the target names three files and even when the
call is going to be rejected as too broad. On the Firefox tree (about 250,000 files, 11
analyzer languages) one measured glob pair took 248.6 seconds of client wall time, of
which 123.5 seconds was one server-side routing span with no child spans at all. A second
call, `["layout/style"]`, took 109.1 seconds. Twenty-seven such calls consumed 943.7
seconds in a single agent task, and most of them returned a one-kilobyte "your target was
too broad" rejection.

After this change a caller can issue the same directory or glob target against the same
Firefox workspace and get either the summaries or the too-broad rejection in a time that
scales with the number of files the target actually matches, not with the size of the
workspace. Concretely: the glob leg stops enumerating `analyzer.analyzed_files()` (11
live-filesystem snapshot scans plus 11 whole-workspace SQLite queries, rebuilt for every
request) and instead matches the pattern against the session's already-cached workspace
file listing, then asks the store to confirm only the files that actually matched. And the
per-target fan-out cap is decided from the cheap match count before any store validation
happens, so a rejected call costs a listing scan and nothing else.

You can see it working in three ways: a unit-level call-count pin (a glob request performs
zero whole-workspace `analyzed_files` enumerations and validates only as many files as it
matched), an end-to-end pin (a workspace file that is listed but not analyzed never appears
in a glob result, so the cheaper universe did not change what the tool answers), and a
wall-clock measurement on the Firefox clone recorded in the `Artifacts and Notes` section.

Issue: <https://github.com/BrokkAi/bifrost/issues/1738>.

## Progress

- [x] (2026-08-09 19:05Z) Read the prior read-only attribution report and re-verified every
      code citation at HEAD `bc85ec9f`. Recorded the line-number drift in
      `Context and Orientation`.
- [x] (2026-08-09 19:20Z) Wrote this ExecPlan into `.agents/plans/`.
- [ ] Step 0: add profiling scopes to the glob-resolution legs and to the two previously
      invisible analyzer/store calls.
- [ ] Step 0b: re-measure the baseline at HEAD on the Firefox clone with the new scopes.
      Record the result in `Artifacts and Notes`.
- [ ] Step 1: swap the glob match universe to the session listing and add batched
      per-candidate validation.
- [ ] Step 2: apply the per-target fan-out cap to the cheap match count before any
      validation work.
- [ ] Tests: call-count pins, budget-before-work pin, membership and ordering contract pins.
- [ ] Re-measure after the change; before/after table in `Artifacts and Notes`.
- [ ] Gate: fmt, workspace nextest, doctests, all-features clippy.

## Surprises & Discoveries

- Observation: the baseline this plan was designed against no longer exists at HEAD. The
  attribution measured runtime r26 (`74ff5cbd`), where a 594.5 s background
  `warm_query_indexes` churned the same SQLite database that foreground routing read, and
  it concluded the glob cost was "contention-dominated, not intrinsic" because identical
  globs routed in about 0-1.3 s once the warm finished. The usage-v2 re-land replaced that
  warm. Re-measuring at HEAD is therefore mandatory before claiming any delta, and the
  honest claim is bounded by what the post-v2 baseline actually shows.
  Evidence: pending Step 0b; to be recorded in `Artifacts and Notes`, section
  "Baseline at HEAD".

- Observation: the workspace listing is a strict superset of the analyzed file set, by
  construction, so swapping the glob universe cannot lose a file that validation would
  then re-admit. `TreeSitterAnalyzer::build_state` enumerates its files from
  `Project::analyzable_files(language)`
  (`crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs:3302-3312`), and
  `FilesystemProject::analyzable_files` is `all_files_shared()` filtered by extension and
  by `.bifrostignore` (`crates/bifrost-core/src/analyzer/project.rs:690-720`). Both sides
  read the same `WorkspaceFileListingCache`.
  Evidence: the code cited above. The membership contract pin
  `glob_target_excludes_listed_but_unanalyzed_file` in
  `crates/bifrost-analysis/src/searchtools/tests.rs` guards it.

- Observation: the attribution's claim that the response-budget degrade path "re-runs
  `resolve_file_patterns` and can re-pay the universe enumeration in the same request" does
  not hold for the too-broad case, and does not hold for the glob universe in any case. A
  too-broad reply has zero summaries, and
  `fit_get_summaries_output_to_budget` (`crates/bifrost-mcp/src/mcp_common.rs:248-258`)
  short-circuits to `mark_listing_budget_degradation` when `summaries_len == 0`, so the
  second `list_symbols` call is never issued. When it is issued, its `file_patterns` are
  literal paths taken from summaries already produced
  (`compact_symbols_paths`, `mcp_common.rs:375`; `summary_paths_for_compaction`,
  `mcp_common.rs:361`), and a literal path resolves through
  `WorkspaceFileResolver::resolve_literal` without touching the glob leg at all.
  Evidence: the code cited, plus the service-level pin
  `too_broad_glob_response_does_not_issue_a_second_list_symbols_call`.

- Observation: `MultiAnalyzer::is_analyzed` asks every language delegate
  (`crates/bifrost-analysis/src/analyzer/multi_analyzer.rs:832-836`), and each delegate's
  `TreeSitterAnalyzer::is_analyzed` issues its own
  `contains_parsed_blob_at_generation` store round trip
  (`tree_sitter_analyzer.rs:9410-9428`). Validating N matched files one call at a time
  would cost N x languages point queries. The batched `retain_analyzed` added by this plan
  keeps the same membership rule but spends one `parsed_blob_keys_at_generations` query per
  language for the whole candidate set, which is the same store call shape the expensive
  path already used -- only over the matched files instead of the whole workspace.

## Decision Log

- Decision: validate matched candidates through a new batched analyzer method
  `CodeUnitIndex::retain_analyzed(&[ProjectFile]) -> Vec<ProjectFile>` rather than calling
  the existing per-file `is_analyzed` in a loop.
  Rationale: `is_analyzed` already exists and already encodes the membership rule, but it
  costs one SQLite round trip per file per language. `retain_analyzed` performs the same
  ownership and liveness checks per candidate and then issues one
  `parsed_blob_keys_at_generations` query for the whole candidate set, which is exactly the
  store call the whole-workspace path already made -- with a candidate-sized key list. The
  default implementation on the trait keeps every other analyzer and every test fake
  working unchanged.
  Date/Author: 2026-08-09, Claude (Opus 5).

- Decision: introduce a second, purely lexical analyzer predicate
  `CodeUnitIndex::may_analyze(&ProjectFile) -> bool` and filter listing candidates through
  it before counting them against the fan-out cap.
  Rationale: the cap must be decided before validation or the rejection is not cheap, and
  counting raw listing matches would count files no analyzer could ever summarize (README,
  test data, images). `may_analyze` answers "could this path belong to my indexed set" from
  the path alone, with no store and no filesystem access; it is the exact lexical half of
  `TreeSitterAnalyzer::adapter_owns_file` (`tree_sitter_analyzer.rs:5414-5421`). Putting it
  on the analyzer contract keeps the C++ "claims included files" rule where it already
  lives instead of duplicating a language special case inside `searchtools`.
  Date/Author: 2026-08-09, Claude (Opus 5).

- Decision: when the lexical candidate count exceeds the per-target cap, report `too_broad`
  with that candidate count and skip validation entirely, accepting that the reported
  `matched` number is an upper bound on the analyzed matches rather than the exact count it
  is today.
  Rationale: this is what makes a rejected call cost milliseconds, which is the point of the
  issue. The divergence requires more than `GET_SUMMARIES_MAX_FILES_PER_TARGET` (20) files
  that match the target, carry a source extension of a language this workspace analyzes,
  and are nevertheless absent from the store. In that case the caller is told to narrow a
  target that really does match more than twenty source files. The `TooBroadScope.matched`
  doc comment states the upper-bound semantics, and the tool description already frames the
  field as "a sample of the match" for narrowing rather than a result count. Below the cap
  nothing changes: those candidates are validated and the answer is exactly today's answer.
  Date/Author: 2026-08-09, Claude (Opus 5).

- Decision: leave container-listing truncation where it is, at MCP response assembly
  (`crates/bifrost-mcp/src/mcp_common.rs:343`), instead of moving it into the work layer.
  Rationale: the design sketch grouped "listing truncation" with the fan-out cap under
  "budget before work", but there is no work after a listing in the work layer to save.
  `route_summary_targets_with_cancellation` pushes the listing and `continue`s
  (`crates/bifrost-analysis/src/searchtools/summaries.rs:264-270`), and
  `summarize_routed_targets_with_cancellation` only clones the listings into the result
  (`summaries.rs:930`). Nothing is summarized, resolved, or validated on account of a large
  listing, so an earlier cutoff would save zero measured time while changing a
  user-visible payload shape. The directory listing scan itself (60-180 ms on Firefox) is
  addressed only by the universe question, which the directory leg already answers
  correctly.
  Date/Author: 2026-08-09, Claude (Opus 5).

- Decision: keep `resolve_directory_target`'s `.` (workspace root) case on
  `analyzer.analyzed_files()`.
  Rationale: that target literally asks for the analyzed universe. Routing it through
  listing-plus-validation would validate every file in the workspace, which is strictly
  more work than asking for the set directly.
  Date/Author: 2026-08-09, Claude (Opus 5).

- Decision: do not apply a fan-out budget to `list_symbols` or `get_symbol_sources` in this
  change; they get the universe swap (Step 1) but keep passing no budget.
  Rationale: `list_symbols` reports `total_files` and a "Showing X of Y" note computed from
  the full match set (`skim_files_for_files`, `summaries.rs:657-693`); capping expansion
  would change that user-visible contract, and the issue is about `get_summaries`. Both
  tools still get strictly less work than before, because their candidate set is the
  matched files rather than the whole analyzed workspace.
  Date/Author: 2026-08-09, Claude (Opus 5).

- Decision: the client-side head-of-line wait (about 491 s of the 943.7 s in the incident)
  is out of scope and no Bifrost change addresses it.
  Rationale: all 81 requests in the measured session executed with zero overlap even though
  `ANALYZER_POOL_CAPACITY` is 4 (`crates/bifrost-mcp/src/analyzer_pool.rs:22`) and waiting
  requests recorded approximately zero `mcp_request.queue_wait`, so they had not reached the
  server's dispatch when they waited. The serialization is upstream of the server, most
  likely one-request-at-a-time-per-connection behavior in the evaluation client. If it
  reproduces after this change, it belongs in an issue against that harness.
  Date/Author: 2026-08-09, Claude (Opus 5).

## Outcomes & Retrospective

Filled in at completion; see the bottom of this document.

## Context and Orientation

You need no prior knowledge of this repository. This section names every file and term
used later.

### The tool and the request path

`get_summaries` is an MCP (Model Context Protocol) tool: an agent sends a JSON request over
stdio to the `bifrost` binary and gets a JSON reply. Its input is a list of strings called
`targets`. A target can be a literal file path, a directory path, a glob pattern
(`src/**/*.rs`), a package or namespace name, or a symbol name. The tool answers with
summaries (a per-file outline of declarations), container listings (an `ls`-like view of a
directory or package), and error-shaped fields (`not_found`, `ambiguous`, `too_broad`).

The request enters `crates/bifrost-mcp/src/searchtools_service.rs`, which acquires an
analyzer from a small pool and calls
`brokk_bifrost::searchtools::get_summaries_with_cancellation`
(`crates/bifrost-analysis/src/searchtools/summaries.rs:638-655`). That function does two
things: it *routes* the targets (decides what each target names) and then it *summarizes*
the routed file and symbol targets. Routing is
`route_summary_targets_with_cancellation` (`summaries.rs:199-328`).

### Where the time went (measured, not assumed)

A read-only attribution over a real incident trace (task `ccx-incident-149`, runtime r26 =
commit `74ff5cbd`, the Firefox workspace) established:

- 27 `get_summaries` calls cost 943.7 s of client wall time but only 358.0 s of server
  execution; 353.8 s of that was `searchtools::route_summary_targets`, and
  `searchtools::summarize_files` was about 0.0 s in every single call. Producing summaries
  is not the problem; deciding what the targets mean is.
- About 335 s of the routing time was inside glob resolution and was invisible to the
  profiler: the worst route span, 123.5 s, contained zero child spans.
- The remaining roughly 491 s was client-side head-of-line waiting, not server work.

### The expensive call chain

For a glob target, routing falls through to
`resolve_file_patterns` (`crates/bifrost-analysis/src/searchtools/mod.rs:487-566`). Its
glob leg (`mod.rs:548-560`) matches every compiled pattern against
`analyzer.analyzed_files()`.

- `MultiAnalyzer::analyzed_files` (`crates/bifrost-analysis/src/analyzer/multi_analyzer.rs:783-792`)
  concatenates the answer of all language delegates, then sorts and dedups. Firefox has 11.
- Each delegate is a `TreeSitterAnalyzer`, whose `analyzed_files` is
  `analyzed_live_files` (`crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs:9395-9397`
  and `5455-5596`). That function iterates every path in the live filesystem snapshot,
  rebases it to the project root, checks adapter ownership, takes the snapshot's validated
  git object id, and then issues one `parsed_blob_keys_at_generations` SQLite query over the
  language's entire candidate key set (`tree_sitter_analyzer.rs:5547-5559`).
- The result is memoized only for the lifetime of one request's `AnalyzerQueryScope`
  (`summaries.rs:647`; the cache cell is at `tree_sitter_analyzer.rs:5465-5486`). The next
  request pays it again.

So one glob target costs 11 whole-workspace snapshot scans plus 11 whole-workspace SQLite
queries, regardless of how narrow the pattern is. `dom/*Parser*` and `layout/style/**`
cost the same.

Note on line numbers: the earlier attribution cited `tree_sitter_analyzer.rs:5372-5513` for
`analyzed_live_files`; at HEAD `bc85ec9f` it is `5455-5596`. The drift comes from the
usage-v2 re-land and the deletion of `RustUsageIndex`. Every other citation in this plan was
re-verified at `bc85ec9f`.

### The cheap universe that already exists

The directory leg of routing (`summaries.rs:250-270`) does *not* use the analyzed universe.
It asks `Project::has_directory` (one `stat`,
`crates/bifrost-core/src/analyzer/project.rs:239` and its `FilesystemProject` override at
`project.rs:441`) and then builds the listing from
`analyzer.project().all_files_shared()` (`project.rs:658-664`), which returns an
`Arc<BTreeSet<ProjectFile>>` straight out of a session-lifetime, watcher-invalidated cache
(`WorkspaceFileListingCache`, `project.rs:483-555`). Warm, that costs microseconds to
obtain and 60-180 ms to scan on Firefox. Cold, it costs one ignore-aware tree walk unioned
with the git index (`collect_workspace_files`), which is the same walk every other listing
consumer pays once per session.

The analyzed set is a subset of that listing by construction: each language analyzer
enumerates its files from `Project::analyzable_files(language)`
(`tree_sitter_analyzer.rs:3302-3312`), which is the same cached listing filtered by file
extension and `.bifrostignore` (`project.rs:690-720`).

### The fan-out cap that already exists

`GET_SUMMARIES_MAX_FILES_PER_TARGET = 20` (`mod.rs:278-282`) bounds how many files one
target may expand to. Over the cap the target is *skipped*, not truncated, and reported
through `TooBroadScope { target, matched, cap, sample }` (`mod.rs:300-328`), where `sample`
is the first `FILE_PATTERN_FANOUT_SAMPLE = 10` matched paths (`mod.rs:276`). The check runs
in routing at `summaries.rs:289-297` -- *after* `resolve_file_patterns` has already
enumerated the whole analyzed universe. That is why a call that returns a 1.3 KB rejection
took 248.6 s.

There is also a byte budget, `GET_SUMMARIES_RESPONSE_BUDGET_BYTES = 4096`
(`crates/bifrost-mcp/src/mcp_common.rs:13`), applied at response assembly in
`fit_get_summaries_output_to_budget` (`mcp_common.rs:220-272`). It is not on the critical
path for this issue and this plan does not change it.

### Terms used in this plan

- *Live snapshot*: the analyzer's view of which workspace paths exist right now and what
  git object id each one currently hashes to. Held per analyzer generation.
- *Generation*: a version number for the persisted analysis of one language. A store row is
  only valid at the generation that produced it.
- *Query scope* (`AnalyzerQueryScope`): a request-lifetime boundary that lets the analyzer
  memoize filesystem liveness and a few derived sets for the duration of one tool call, then
  drop them.
- *Candidate*: a workspace file that a target's pattern matched lexically, before anyone
  has confirmed the analyzer actually indexed it.
- *Validation*: confirming that a candidate is in the analyzed set, which means the store
  holds a parsed blob for that file's current content at the current generation.

## Plan of Work

The work is four steps. Step 0 is pure instrumentation and can land alone. Step 0b is a
measurement, not an edit. Steps 1 and 2 are the fix. Each step ends with the workspace test
suite green.

### Step 0 -- make the invisible span decompose

The 123.5 s route span had zero children, so no measurement of this path can be trusted
until it decomposes. Add `profiling::scope` calls (the helper is
`crates/bifrost-core/src/profiling.rs:55`, re-exported as `crate::profiling` inside
`brokk-bifrost-analysis`) at exactly the places the attribution named:

1. In `crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs`, at the top of
   `analyzed_live_files` (line 5455), a scope naming the adapter's language, and a second
   scope around the `parsed_blob_keys_at_generations` call (line 5551) that records how many
   keys it asked about.
2. In `crates/bifrost-analysis/src/analyzer/multi_analyzer.rs`, at the top of
   `analyzed_files` (line 783), so the language fan-out is one visible parent span.
3. In `crates/bifrost-analysis/src/searchtools/mod.rs`, one scope over
   `resolve_file_patterns` and one over its glob leg, so a route span splits into literal
   resolution and glob resolution.

Scope labels must be stable strings; the timing log groups by label. Use the existing
`searchtools::` / `analyzer::` prefix conventions visible elsewhere in those files.

### Step 0b -- re-measure the baseline at HEAD

Before changing behavior, measure the unmodified-except-for-scopes binary against the
Firefox clone at `/mnt/T9/repo-clones/firefox--871325b8`, with `BIFROST_TIMING=1` so the
new spans print. Record cold and warm, wall and CPU. This is mandatory: the attribution's
numbers came from a binary whose background index warm has since been replaced, and the
warm was the multiplier it blamed. If the post-replacement baseline no longer shows
multi-second route spans, say so and scope the claim to what remains.

### Step 1 -- swap the glob match universe, validate only the matches

Add two methods to the analyzer contract in
`crates/bifrost-core/src/analyzer/code_unit_index.rs`, next to the existing
`analyzed_files` / `is_analyzed` (lines 29-44):

    /// Whether `file` could belong to this analyzer's indexed set, judged from
    /// the path alone: no store query, no filesystem access.
    fn may_analyze(&self, file: &ProjectFile) -> bool;

    /// The subset of `candidates` this analyzer has actually analyzed, decided
    /// with the same rule as `is_analyzed` but batched.
    fn retain_analyzed(&self, candidates: &[ProjectFile]) -> Vec<ProjectFile>;

Default implementations keep every existing analyzer and test fake correct:
`may_analyze` answers `language_for_file(file)` is in `self.languages()`; `retain_analyzed`
materializes `get_analyzed_files()` once and filters.

Override both where it matters:

- `TreeSitterAnalyzer` (`tree_sitter_analyzer.rs`): `may_analyze` is the lexical half of
  `adapter_owns_file` (`5414-5421`) -- the adapter's own language, or an unclaimed
  extension when the adapter claims included files. `retain_analyzed` walks the candidates
  applying ownership, live-oid resolution and the dirty-state retry exactly as `is_analyzed`
  does (`9410-9428`), collects the persisted ones, and issues a single
  `parsed_blob_keys_at_generations` query for them.
- `MultiAnalyzer` (`multi_analyzer.rs`): both fan out to the delegates and union.
- Each language wrapper that already forwards `is_analyzed` (`java/mod.rs:456`,
  `rust/mod.rs:823`, and the rest of the list found by
  `grep -rn "fn is_analyzed" crates/`) forwards the two new methods the same way.

Then, in `crates/bifrost-analysis/src/searchtools/mod.rs`:

- `resolve_file_patterns`'s glob leg (548-560) matches against
  `analyzer.project().all_files_shared()` filtered by `may_analyze`, and passes the result
  through `analyzer.retain_analyzed`.
- `resolve_directory_target` (606-617) does the same for its prefix case, keeping the `.`
  case on `analyzed_files()`.

Nothing about ordering or truncation changes: matches still accumulate into a
`BTreeSet<ProjectFile>` and are still emitted in path order.

### Step 2 -- decide the fan-out cap before doing the work

Give `resolve_file_patterns` a `max_glob_matches: Option<usize>` parameter and a new
`glob_overflow: Option<TooBroadScope>` field on its `ResolvedFilePatterns` return value.
When a budget is supplied and the lexical candidate count exceeds it, build the
`TooBroadScope` from the candidates and return without validating anything.
`route_summary_targets_with_cancellation` passes `Some(max_files_per_target)` and, when
`glob_overflow` is set, pushes it straight into `too_broad`. `list_symbols`
(`summaries.rs:948-953`), `get_symbol_sources` (`sources.rs:530`) and the unit tests pass
`None`.

## Concrete Steps

Working directory for everything below is the repository root, `/mnt/optane/bifrost-nlp`.

Build the binary used for measurement:

    cargo build --release --bin bifrost

Run the focused test suites while iterating:

    cargo test -p brokk-bifrost-analysis --lib searchtools
    cargo test --test suite_symbols

Measure against Firefox (the clone and its per-repo cache both exist on this box):

    BIFROST_TIMING=1 MCP_REPLAY_STDERR_FILE=/tmp/.../spans.txt \
      scripts/mcp-replay.py --binary target/release/bifrost \
      --workspace /mnt/T9/repo-clones/firefox--871325b8 --scenario ...

A purpose-built driver that issues exactly the incident's two targets
(`["layout/style"]` and `["layout/style/**/*.cpp","layout/style/**/*.h"]`) is easier to read
than a scenario; it reuses `McpClient` from `scripts/mcp-replay.py`. Keep it in the
scratchpad, not in the repository -- it is measurement scaffolding, not a checked-in tool.

Full gate before finishing:

    cargo fmt
    cargo nextest run --workspace --all-targets --no-fail-fast
    cargo test --workspace --doc
    df -h .
    scripts/with-isolated-cargo-target.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

## Validation and Acceptance

Behavioral acceptance, in order of how directly it proves the issue is fixed:

1. On the Firefox clone, with the analyzer warm, `get_summaries` with
   `targets = ["layout/style/**/*.cpp","layout/style/**/*.h"]` returns its `too_broad`
   rejection in well under a second, and the `[bifrost-timing]` log shows no
   `analyzer::analyzed_live_files` span inside the route. Before the change the same call
   shows 11 such spans (one per language) and their store queries.
2. `cargo test -p brokk-bifrost-analysis --lib searchtools` passes, including these new
   pins:
   - `glob_file_pattern_validates_only_matched_files`: a glob matching 3 of 200 workspace
     files performs zero `analyzed_files` enumerations and offers exactly 3 candidates for
     validation. Fails before Step 1 (the old code makes exactly one `analyzed_files` call
     and never validates).
   - `too_broad_glob_reports_before_validating_any_file`: a glob matching 25 files with a
     cap of 20 returns `too_broad { matched: 25, cap: 20 }` having validated zero files and
     enumerated zero analyzed sets. Fails before Step 2 (validation runs first).
   - `glob_target_excludes_listed_but_unanalyzed_file`: a workspace file present in the
     listing but absent from the analyzed set does not appear in a glob result. Fails if
     validation is dropped from Step 1.
   - `glob_file_pattern_avoids_analyzed_file_enumeration`: the updated form of the existing
     `glob_file_pattern_scans_analyzed_files`, which asserted the old behavior (exactly one
     whole-universe enumeration).
3. `cargo test --test suite_symbols` passes, including the existing
   `searchtools_too_broad_scope.rs` behavior pins unchanged, and these new end-to-end pins:
   - `glob_summaries_keep_deterministic_path_order`
   - `directory_listing_reports_total_entries_and_truncation_flag`
   - `too_broad_glob_response_does_not_issue_a_second_list_symbols_call`
4. The full gate passes: `cargo fmt`, workspace `nextest`, doctests, and all-features
   clippy with `-D warnings`.

## Idempotence and Recovery

Every step is a normal source edit; re-running the commands is safe. The measurement writes
only into the scratchpad directory and into the Firefox clone's `.bifrost/cache` (which the
tool owns and rebuilds). No step deletes a cache the box needs: the per-repo cache at
`/mnt/T9/repo-clones/.codescale-cache-perrepo-r26` is read-only input for this work.

If Step 1 turns out to lose files on some workspace, the recovery is one line: restore the
glob leg's universe to `analyzer.analyzed_files()`. The new analyzer methods are additive
and harmless if unused.

## Artifacts and Notes

Filled in as measurements are taken. See "Baseline at HEAD" and "Before and after" below.

## Interfaces and Dependencies

In `crates/bifrost-core/src/analyzer/code_unit_index.rs`, `pub trait CodeUnitIndex` gains:

    fn may_analyze(&self, file: &ProjectFile) -> bool { ... }
    fn retain_analyzed(&self, candidates: &[ProjectFile]) -> Vec<ProjectFile> { ... }

In `crates/bifrost-analysis/src/searchtools/mod.rs`:

    struct ResolvedFilePatterns {
        files: Vec<ProjectFile>,
        ambiguous_paths: Vec<AmbiguousPathInput>,
        glob_overflow: Option<TooBroadScope>,
    }

    fn resolve_file_patterns(
        analyzer: &dyn IAnalyzer,
        patterns: &[String],
        max_glob_matches: Option<usize>,
    ) -> ResolvedFilePatterns;

No new crate, no new dependency, no schema change.
