# Runbook: trace the OWASP `getHeaders` taint abstention (find the real TPR lever)

Self-contained instructions to start a fresh session. Goal: find why OWASP
pathtraver real cases that route taint through a collection source
(`getHeaders` -> `Enumeration.nextElement()`) abstain instead of being found,
localize the single break, and fix the smallest thing that raises true positives
(TP) without ever producing a false green.

## Standing context (already done, do not redo)

- The demand-driven budget scaling is landed on `master`. Require-model taint now
  runs the whole pathtraver subset in ONE pass. Commits (in history):
  `36ce00279` (per-region + workspace-sized materialization budgets + non-fatal
  catch-and-skip + work accounting), `7aaea6337` (per-batch witness reset),
  `73824558f` (name the unsupported capability in diagnostics), `22bac575c`
  (env-gated per-case verdict dump). ExecPlans:
  `.agents/plans/demand-driven-taint.md`,
  `.agents/plans/java-exceptional-value-flow.md`.
- Current corpus state (measure by TP; NA count is noisy, see below): pathtraver
  268 cases -> ~6 TP, ~8 FP, **false_greens = 0**, 0 Complete verdicts, ~220
  analyzed, rest abstain on a value-flow capability gap.

## The finding this trace is built on

Found vs abstained is decided by the TRANSFORMATION on the taint path, not the
source getter. Concretely, comparing a found case to an abstained one that share
everything except one step:

- **BenchmarkTest00001 (FOUND)**: `getCookies()` -> for-each -> `theCookie.getValue()`
  -> `URLDecoder.decode(..)` -> `DIR + param` -> `new File(fileName)` ->
  `FileInputStream`. All steps modeled.
- **BenchmarkTest00011 (ABSTAINED)**: `getHeaders("..")` -> `headers.nextElement()`
  -> `URLDecoder.decode(param)` -> `new File(param, "/Test.txt")`.

What is / isn't modeled (checked):
- `java.net.URLDecoder.decode(String,String)` IS in the golden pack
  (`semantic-packs/golden-core/bifrost.jdk-golden-summaries.json`).
- `getHeaders` and `getValue` are in
  `semantic-packs/framework-decls/staged/bifrost.javax.servlet-api-framework-decls.json`.
- `java.util.Enumeration.nextElement()` is modeled NOWHERE. It is the ONLY
  unmodeled step separating 00011 from 00001.

Negative experiment already run: I added receiver->return summaries for
`Enumeration.nextElement()`, `Collection.iterator()`, `Iterable.iterator()`,
`Map.values()` directly to the golden JSON. The pack ACTIVATED (confirmed in the
artifact's `packs.activated`), but TP stayed at 6. So adding the summary data was
not sufficient. `Iterator.next()`, `List.get(int)`, `Map.get(Object)` were
already modeled before the experiment. The pack edit was reverted; master is
clean.

## The hypothesis to test first

The added `nextElement()` summary did not change behavior, so the prime suspect is
that it never BOUND to the call `headers.nextElement()`. Likely causes, in order:
1. Symbol / receiver-type mismatch. The summary target symbol was
   `java.util.Enumeration.nextElement()` with `has_receiver: true,
   parameter_count: 0`. The call's receiver `headers` is declared
   `java.util.Enumeration<String>`; binding may need the concrete resolved
   receiver type, generic erasure handling, or a different symbol spelling. See
   the "unmaterialized summary binding" memory (has_receiver, fully-qualified
   symbol format, #1978) for how JDK-call summaries bind without a body.
2. The source taint is not actually on `headers`. `getHeaders` is a framework
   DECL (so the call resolves), and the policy name-selector taints its
   return-value. Confirm the returned `Enumeration` carries the label before
   `nextElement()` is reached.
3. Container-element taint is not propagated: a tainted `Enumeration` receiver
   may not flow to `nextElement()`'s return even with a receiver->return
   transfer, if the model treats the container and its elements as distinct.

The trace decides which of 1/2/3 it is. Do NOT author more summaries until the
break is localized.

## Ready-to-use single-case corpus

`/home/jonathan/scratch-owasp/trace-11/` is already built: bench-one cloned,
BenchmarkTest00001 swapped for BenchmarkTest00011, CSV trimmed to the one
`pathtraver,true` row, all helpers retained. Source of the case:
`trace-11/src/main/java/org/owasp/benchmark/testcode/BenchmarkTest00011.java`.
(If it is missing, rebuild it: `cp -r /home/jonathan/scratch-owasp/bench-one
/home/jonathan/scratch-owasp/trace-11`, replace the testcode file with 00011 from
`/home/jonathan/scratch-owasp/pt-all/.../BenchmarkTest00011.java`, and trim the
CSV to the header line plus `BenchmarkTest00011,pathtraver,true,22`.)

## Commands

Build the release-tooling runner (needed for the OWASP scoreboard bin):

    cargo build --release --features release-tooling --bin bifrost_owasp_benchmark

Run the single case with the per-run debug hook (prints each policy run's
completion and every diagnostic):

    BIFROST_OWASP_DEBUG=1 \
    target/release/bifrost_owasp_benchmark run \
      --benchmark /home/jonathan/scratch-owasp/trace-11 \
      --packs-dir semantic-packs \
      --deps /home/jonathan/scratch-owasp/BenchmarkJava/target/dependency \
      --esapi-digest 2288e84a6c93a457c5215eb8028c87ebd4326a515e21545d2e02db8356d6ccff \
      --out /home/jonathan/scratch-owasp/trace-11.json

Useful env vars added this session (see `src/owasp_benchmark.rs`):
- `BIFROST_OWASP_PER_CASE=<path>` writes one TSV line per case
  (index, name, category, real, flagged, completion).
- `BIFROST_OWASP_SAMPLE_DIAGS=<n>` raises the per-category diagnostic sample cap
  (default 6) so a run surfaces the full status distribution.
- `BIFROST_OWASP_DEBUG=1` prints per-run completion + diagnostics to stderr.

The abstention diagnostic now names the missing capability, e.g.
`... is unsupported (exceptional_control_flow)` or `... is unknown` /
`unproven`. For a summary-binding break expect an `unknown`/`unproven` snapshot
(not `exceptional_control_flow`).

## Where to trace in code

- Source/sink selection + region discovery: `crates/bifrost-policy/src/taint_policy.rs`
  (`compile_inner` roots ~592-630, `discover_value_flow` ~859).
- The value-flow snapshot status (`Complete`/`Unknown`/`Unsupported`/`Unproven`)
  comes from `SemanticInputStatus::from_outcome`
  (`crates/bifrost-analysis/src/analyzer/dataflow/input.rs:31`) off the
  `SemanticOutcome` produced when materializing the procedure
  (`crates/bifrost-analysis/src/analyzer/semantic/service.rs`).
- Summary binding for bodiless/JDK calls: search the summary-binding path
  (`has_receiver`, symbol resolution) used by the golden/framework packs; the
  "unmaterialized summary binding" memory names the anchors.
- Golden pack authoring (do NOT hand-edit the shipped JSON as the fix; go through
  the generator): `crates/bifrost-semantic-packs/src/summary_foundry/golden_pack.rs`.
  Summary shape is `{"input":{"kind":"receiver"},"exit_kind":"normal",
  "output":{"kind":"normal_return"}}` with target
  `{"path":..,"symbol":..,"has_receiver":true,"parameter_count":N}`.

## Guardrail (keep these green after any change)

    cargo test --test suite_bench_policy taint_regression_fixtures
    cargo test --test suite_bench_policy taint_policy_adapter

The four `taint_regression_fixtures` are the known-answer honesty net. Any fix
that lets a real case reach Complete-with-no-finding is a false-green bug, not a
win -- stop and treat it as a correctness defect. Verify false_greens = 0 on the
corpus after every change.

## Measurement discipline

- Measure TPR deltas by TP count (stable), NOT by not_analyzed. NA is
  NON-DETERMINISTIC run to run (observed 21 vs 48 across builds with no logic
  change). Confirm any NA change reproduces before trusting it.
- The scoreboard artifact is category-aggregated; use `BIFROST_OWASP_PER_CASE`
  for per-case verdicts.

## Loose ends deliberately left alone (not this trace's scope)

1. **Java exceptional control flow (ECF) in value-flow.** 83/221 analyzed cases'
   first cause is `unsupported (exceptional_control_flow)`. Java lowers try/catch
   structurally but marks implicit exceptional edges from throwing operations to
   catch/finally as gaps, so the catch is CFG-unreachable. Full design in
   `.agents/plans/java-exceptional-value-flow.md`. DECISION: deprioritized because
   OWASP taint flows on the NORMAL path, so ECF mostly converts
   Inconclusive->Complete rather than adding TP, AND cases carry ECF PLUS
   JDK-call gaps, so ECF alone likely will not move the corpus. Honesty-critical:
   removing ECF gaps without soundly tracing exception-path flow would create
   false greens. Do NOT start this without the de-risk experiment in the plan.

2. **`not_analyzed` cases.** After the rebase they are all `real=false` (fake) and
   cluster in a few erroring batches (e.g. indices 76-81, 161-169, 262-267 in one
   run). Closing them raises coverage cosmetically but yields ZERO TPR (no missed
   real vulnerability hides in NA). Also the count is non-deterministic. Low
   priority; the value is entirely in the Inconclusive real cases.

3. **`ReportRetentionBudget` / witness_steps=64 projection cap.** Even found cases
   carry a secondary diagnostic `projected witness_steps exceeds the effective
   report limit of 64` (`crates/bifrost-policy/src/projection.rs`, limit is
   `min(authored max_steps, 1024)` at line ~222). It degrades the verdict/evidence
   but did NOT drop the finding for BenchmarkTest00001. Investigate only if a
   found flow is later shown to be dropped by it.

4. **6-vs-9 TP "gap".** The one-pass number (6) is below an older batched-eager
   run (9). Not attributed per case; may be modeling/capability or a
   per-region-vs-shared flow difference. Only meaningful after the trace above
   moves TP.

5. **Retire the eager all-procedures roots (plan Stage D).** Cannot be done: full
   demand (lazy ValueFlowProvider) is blocked by the plan-global require-model
   fallback, and endpoint seeding sacrifices cross-procedure completeness. The
   eager path stays. See the Outcome section of
   `.agents/plans/demand-driven-taint.md`.

## Environment notes

- Work in the primary checkout `/home/jonathan/Projects/bifrost` on `master`
  (not a worktree). There are UNRELATED uncommitted `mcp_property_fuzzer` changes
  in the tree from another session -- do NOT stage or commit them. Only ever
  `git add` your own specific files; never `git add -A`.
- The OWASP corpus lives at `/home/jonathan/scratch-owasp/` (`pt-all` = 268 cases
  / 289 files; `bench-one` = single case; `trace-11` = the getHeaders trace case).
  `--packs-dir` must be the `semantic-packs` repo root. The esapi digest above is
  the sha256 of `esapi-2.7.0.0.jar` under the deps dir.
- Do NOT enable the `nlp` feature for this work.
