# CodeScaleBench grep-hard checkpoint (2026-08-09)

Arm 03 (symbols, runtime r26) and arm 04 (symbols plus NLP, runtime r26) are both complete.
The headings up to "Next actions" record arm 03. The headings that start at "Arm 04" record arm 04
and the four-arm comparison. Read "Semantic use: the census correction" first if you only want the
semantic-use answer.

## Status

Arm 03 is complete. It is the 14-task paired manifest
`.agents/docs/codescale-grep-hard-luna-max-r3-shovel14.tasks` with symbol tools on runtime r26
and per-repository analyzer caches. The arm directory is
`symbols-r26-final/` in the campaign directory. Phase 1 provenance is `PHASE1-r26-record.md`.

Arm 03 replaces the earlier plan to run "symbols plus NLP" next. It answers a different and more
urgent question: does the `bifrost-nlp-ft` fix arc plus the per-repository cache layout remove the
latency that gated the 2026-08-07 conclusion? The answer is mostly yes. The tool-value answer did
not change.

Runtime r26 is Bifrost `74ff5cbd`. It differs from runtime r25 only in the two Bifrost binaries.
Arm 02 and arm 03 carry identical per-task guidance fingerprints, model, reasoning effort and
evaluation mode. Only the runtime and the cache layout differ.

## Validity gate

The arm passed. There were 0 MCP-start errors in 14 tasks, 14 of 14 `mcp server ready` records,
14 of 14 tasks with a successful Bifrost call, and 14 of 14 archives.

Bifrost call success is 100 percent in 11 tasks. Three tasks each have one failed call. All three
are the same agent argument error: `get_summaries` on `/workspace/answer.json`, a path outside the
analysis workspace, rejected in 0 ms. No failure is a Bifrost defect.

## Paired result

The maximal comparable subset for r26 against bare is 12 of 14 tasks. It excludes the two tasks
that arm 03 failed on answer shape. Bare scores 0.5909 mean on those two excluded tasks, against
its 14-task mean of 0.5877, so the exclusion is close to neutral.

| Metric | Bare | r26 symbols |
| --- | ---: | ---: |
| Mean composite | 0.5872 | 0.5768 |
| Median composite | 0.5732 | 0.5821 |
| Mean file_f1 | 0.8111 | 0.8214 |
| Mean symbol_recall | 0.3633 | 0.3321 |
| Solves | 0 | 0 |
| Total wall | 8,135 s | 5,661 s |
| Total tokens | 9,138,215 | 7,056,975 |
| Total cost | $0.5011 | $0.4708 |

Mean of the per-task deltas is -0.0104. Four tasks improved, four regressed, four did not move.

The maximal three-arm subset is 10 of 14 tasks. It also excludes the two tasks that arm 02 left
unscored. Bare 0.6019, r25 symbols 0.6021, r26 symbols 0.5940.

On quality the three arms are indistinguishable at this sample size. What r26 buys is 30 percent
less wall clock and 23 percent fewer tokens than bare, not composite.

## Latency: the fix stack's eval-scale verdict

The census counts each Bifrost `tool_timing` record in the archived traces. One method was applied
to both symbol arms. The recomputed r25 numbers reproduce the published r12 census exactly, which
validates the comparison.

| Metric | r25 (arm 02) | r26 (arm 03) |
| --- | ---: | ---: |
| Bifrost calls | 381 | 375 |
| Calls over 5 s | 148 (38.9%) | 105 (28.0%) |
| Median call | 3.24 s | 0.96 s |
| p90 call | 62.9 s | 19.7 s |
| Worst call | 1200.3 s | 330.3 s |
| Median slow call | 27.3 s | 14.7 s |
| Sum of call durations | 10,431 s | 3,382 s |
| Wall-clock occupancy | 6,794 s | 1,572 s |

Per tool, r25 to r26: `get_symbol_sources` median 0.44 s to 0.15 s and total 5,159 s to 805 s;
`search_symbols` median 7.31 s to 3.56 s and total 1,532 s to 812 s; `get_summaries` total 1,024 s
to 355 s; `scan_usages_by_reference` median 45.9 s to 17.2 s and total 2,715 s to 1,410 s.

This is a large win. Total Bifrost tool time fell 68 percent and wall-clock occupancy fell 77
percent on an almost identical call mix. The 1,200 s `get_symbol_sources` calls that lost two
Firefox tasks in arm 02 are gone; both Firefox tasks completed in arm 03.

Attribute the win honestly. The traces carry per-call wall time only. There is no span-level
attribution. The change is the joint effect of the per-repository analyzer caches, the
analyzer-only host prewarm, and the whole `bifrost-nlp-ft` fix arc between r25 and r26. Do not
credit a single commit.

`scan_usages_by_reference` is the remaining slow path: 30 of 34 calls exceed 5 s, its median is
17.2 s and it is 1,410 s of the 3,382 s total from 9 percent of the calls. `search_symbols` is
second at 38 of 95 calls over 5 s. Both are the correct next targets. Issue #1748 is the open
record for `scan_usages_by_reference`.

## The symbol scoring artifact

Phase 1 recorded a risk. `score_answer` matches symbols by exact `(repo, path, symbol)` tuple
equality with no name normalization, and the r26 smoke wrote Bifrost-style fully qualified names
that matched nothing. The risk was that the arm's `symbol_recall` column would measure spelling,
not retrieval.

The risk is real but symmetric. Each task's oracle was rebuilt with the scorer's own
`canonical_oracle`, and each arm's `answer.json` was parsed with the scorer's own `_symbol_entry`.
The recomputed exact `symbol_recall` equals the stored value for every scored task in every arm,
so the reconstruction is faithful. A tail-normalized counterfactual then matched the last name
component within the same `(repo, path)`.

On the 12 comparable tasks, normalization adds 14 matched symbols to r26 and 15 to bare. Micro
symbol recall rises from 0.4123 to 0.5351 for r26 and from 0.4211 to 0.5526 for bare. Mean
composite rises from 0.5768 to 0.6580 for r26 and from 0.5872 to 0.6799 for bare. It creates two
solves in each arm.

So the artifact costs each arm about 12 recall points and about 0.08 composite, and it hides two
near-solves per arm. It does not favour either arm: the share of written symbols that carry a
qualifier is 0.39 in both arms, and the bare-minus-r26 gap widens slightly under normalization.
The single smoke observation did not generalize.

Not all lost recall is naming. On `ccx-incident-110` and `ccx-onboard-103` the path-agnostic upper
bound is 1.0 while the normalized recall stays at 0.5 and 0.2. There the agent named the right
symbol against the wrong file. That is localization, and normalization must not recover it.

Recommendation, not implemented: propose a versioned scorer change `canonical_grep_hard_v2` that
adds tail normalization, with a rescore of all arms from their stored answers in one pass. This is
an owner decision. The case for it is measurement quality, because the current scorer understates
every arm and suppresses the solve count. There is no urgency for arm comparison, because the
artifact is symmetric. Do not change symbol matching inside an arm comparison.

## The two INVALID_OUTPUT tasks

`ccx-dep-trace-273` and `ccx-dep-trace-264` are `invalid_output` with failure
`invalid_answer_contract`, message "files is not a list". Both wrote an answer artifact. Neither
artifact has a `files` key. Each agent invented its own report schema instead of the
`{files, symbols}` contract in `instruction.md`.

The failure is answer shape only. Bifrost was healthy in both traces: 273 made 43 calls with 42
successes, 264 made 56 calls with 55 successes, and both had `mcp server ready`. The one failure
in each is the `answer.json` argument error described above.

`ccx-dep-trace-273` also shows smoke-versus-arm variance. Hours earlier, the r26 smoke ran the same
task on the same runtime, cache and guidance and scored it: composite 0.4545, file_f1 0.9091,
symbol_recall 0.0. Nothing in the harness changed. This is agent output-format variance. A
single-task smoke cannot certify the answer contract for an arm.

The rate is not new. Arm 02 also lost 2 of 14 tasks to `invalid_output`, and two more to a missing
reward record. `ccx-dep-trace-273` failed the contract in both symbol arms. No task was rerun.

## Anomalies

1. Bifrost calls are heavily concurrent and the over-5 s metric does not know it. In r26, 358 of
   375 calls overlap another call, with up to 6 in flight. On `ccx-platform-240`, three calls
   started together and ran 286 s, 330 s and 330 s, which the census reads as three slow calls and
   947 s of tool time against about 330 s of real elapsed time. The concurrency ratio rose from
   1.54 in r25 to 2.15 in r26, so part of the modest fall in the over-5 s count is agents batching
   more calls per turn, each of which then charges itself for its siblings' latency. The union
   figure is the honest wall-clock one. Both censuses use the same per-call definition, so the
   comparison holds.
2. The same agent argument error appears in 3 of 14 tasks. Two of the three are the two
   INVALID_OUTPUT tasks. The sample is too small to call.
3. `ccx-platform-242` scores composite 0.5750 with file_f1 0.4000 and symbol_recall 0.7500 in all
   three arms. The agent finds the right symbols and files them under the wrong paths. File
   localization, not symbol retrieval, is the binding constraint there.
4. `ccx-incident-034` wrote no symbols at all in both symbol arms and finished in 114 s, the
   fastest task in the arm. The agent stopped early rather than failing.

## Artifacts

All in `/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/`:
`paired14-r26-vs-bare-v1.json` and `.md`, `build-paired14-r26-report-v1.py`,
`PHASE1-r26-record.md`, `symbols-r26-final/`, `symbols-r26-smoke/`, `host-prewarm-r26/`,
`runtime-r26-source/`.

## Next actions

1. Owner decision on the scorer. Version it to `canonical_grep_hard_v2` with tail normalization and
   rescore all arms, or record that the artifact stands and every published composite is a
   spelling-sensitive lower bound.
2. Target `scan_usages_by_reference` next. It is now the single dominant slow path at eval scale.
   Add the r26 timing evidence to #1748.
3. Decide whether the arm needs more statistical power. Three arms inside 0.01 mean composite on
   10 to 12 tasks cannot separate the tools. A larger manifest or repeated runs is the only way to
   turn "no measured gain" into a supported claim.
4. Arm 04, symbols plus NLP, is still unrun. It needs a GPU assignment for the embedding sidecar.

## Arm 04: symbols plus NLP on runtime r26

Arm 04 is the same 14-task manifest with `--codescale-mode symbols-nlp`. The arm directory is
`symbols-nlp-r26-final/`. Phase 2 provenance is `PHASE2-NLP-r26-record.md`. The arm added
`semantic_search` to the tool set. It used the shared voyage-4-nano DW10 sidecar on GPU 1 and the
vectors that the host prewarm wrote into the same six per-repository caches.

Arm 03 and arm 04 share the runtime bundle, the cache root, the manifest, the model, the reasoning
effort and the 14 guidance fingerprints. Only NLP enablement differs.

### Validity gate

The arm passed. There were 14 of 14 `mcp server ready` records with `tool_count=8` and the toolset
expression `symbol|nlp`, 0 MCP-start errors, 14 of 14 tasks with a successful Bifrost call, and
14 of 14 archives.

Outcomes are a separate matter. 9 tasks scored, 2 failed the answer contract
(`ccx-incident-032`, `ccx-onboard-103`), and 3 hit the 1,800 s agent limit
(`ccx-domain-112`, `ccx-incident-125`, `ccx-dep-trace-264`). The two contract failures are
different tasks from the two that failed in arm 03.

### Semantic use: the census correction

The first pass over this arm reported zero semantic calls. That report was wrong. It read
`toolCalls.byTool`, and `semantic_search` never appears there.

`semantic_search` is an ordinary tool call in `llm_response.tool_calls`, but it emits no
`tool_timing` record. Its execution is recorded as `semantic_search_batch`,
`semantic_search_phase` and `semantic_search_rerank` events. Any census that counts `byTool` will
report zero semantic use on every task. `result.json.semanticSearch` is the correct per-task
counter, and it agrees with the traces.

The corrected census:

| Metric | Arm 04 |
| --- | ---: |
| Tasks that called `semantic_search` at least once | 13 of 14 |
| Agent-issued calls in the traces | 46 |
| `semanticSearch.agentCalls` | 42 |
| `semanticSearch.queryRuns` | 112 |
| `semanticSearch.syntheticCalls` | 0 |
| Batches started / completed | 42 / 40 |
| Tasks where `byTool` shows `semantic_search` | 0 of 14 |

Only `ccx-onboard-103` never called it. Calls per task range from 1 to 9. The NLP smoke's 3 calls
on `ccx-dep-trace-273` were not an outlier; they were typical.

**Verdict on the plan's synthetic step zero.** The plan adds a synthetic step zero only "if natural
semantic use is too sparse for comparison". Natural use at eval scale is 3.3 agent calls and 8.0
query runs per task, on 13 of 14 tasks. The trigger condition is not met. The synthetic step zero
is therefore an optional design choice, not a required repair. Nothing was implemented.

### Four-arm paired result

The maximal three-arm subset is 8 of 14 tasks. It keeps every task that bare, arm 03 and arm 04 all
scored. It drops `ccx-dep-trace-264`, `ccx-dep-trace-273`, `ccx-domain-112`, `ccx-incident-032`,
`ccx-incident-125` and `ccx-onboard-103`. Bare scores 0.5594 mean on the six dropped tasks against
its 14-task mean of 0.5877, so the exclusion is close to neutral for bare. The same 8 tasks also
carry an arm 02 score, so the four-arm subset is the same 8 tasks.

| Metric | Bare | r25 symbols | r26 symbols | r26 symbols+NLP |
| --- | ---: | ---: | ---: | ---: |
| Mean composite | 0.6090 | 0.6093 | 0.6117 | 0.6042 |
| Median composite | 0.5732 | 0.5930 | 0.5875 | 0.5718 |
| Mean file_f1 | 0.8392 | 0.8392 | 0.8506 | 0.8506 |
| Mean symbol_recall | 0.3788 | 0.3795 | 0.3728 | 0.3578 |
| Solves | 0 | 0 | 0 | 0 |
| Total wall | 5,160 s | 5,713 s | 3,359 s | 7,030 s |
| Total tokens | 5,986,310 | 4,220,588 | 3,781,426 | 6,581,536 |
| Total cost | $0.3267 | $0.2919 | $0.2666 | $0.5942 |

Mean paired delta, arm 04 minus bare: -0.0048. Arm 04 minus arm 03: -0.0075, with 1 task improved,
3 regressed and 4 unchanged. All four arms sit inside 0.008 mean composite. At this sample size the
arms are indistinguishable on quality.

The pairwise arm 04 against bare subset is 9 of 14 tasks, because arm 04 scored
`ccx-dep-trace-273` and arm 03 did not. On those 9 tasks bare is 0.6230 and arm 04 is 0.5815. That
subset is **not** neutral: bare scores only 0.5243 on the 5 dropped tasks against 0.5877 over all
14, so the 9-task comparison flatters bare. Use the 8-task subset for any claim.

### Cost, tokens and the utility split

Over all 14 tasks arm 04 spent $1.1771 against arm 03's $0.6137, and 13,583,588 tokens against
8,968,471. The extra spend is the semantic reranker. 5,907,615 tokens, 43.5 percent of the arm's
total, went to the utility model `deepseek::deepseek-v4-flash`. Per-task utility share reaches 67
percent on `ccx-incident-034` and 66 percent on `ccx-incident-032`. `ccx-onboard-103`, the one task
with no semantic call, has a 0 percent utility share.

So NLP roughly doubles cost and tokens and returns no measured composite gain on this manifest.

### Latency census, arm 03 against arm 04

Bifrost MCP tool calls are counted by the same method as arm 03: per-call `tool_timing` records,
with occupancy taken as the per-task union and then summed across tasks. `semantic_search` is
counted separately, because it emits no `tool_timing` record; its interval is the
`semantic_search_batch` start-to-complete span.

| Metric | arm 03 MCP | arm 04 MCP | arm 04 semantic | arm 04 combined |
| --- | ---: | ---: | ---: | ---: |
| Calls | 375 | 329 | 40 | 369 |
| Calls over 5 s | 105 | 100 | 40 | 140 |
| Share over 5 s | 28.0% | 30.4% | 100.0% | 37.9% |
| Median call | 0.96 s | 1.40 s | 71.1 s | 1.77 s |
| p90 call | 19.7 s | 37.9 s | 560.7 s | 72.6 s |
| Worst call | 330.3 s | 459.0 s | 1,000.6 s | 1,000.6 s |
| Median slow call | 14.7 s | 20.2 s | 71.1 s | 26.0 s |
| Sum of call durations | 3,382 s | 6,730 s | 6,442 s | 13,172 s |
| Wall-clock occupancy (union) | 1,572 s | 4,225 s | 6,212 s | 8,606 s |
| Concurrency ratio | 2.15 | 1.59 | 1.04 | 1.53 |

Every one of the 40 completed semantic batches exceeded five seconds. The median batch is 71.1 s.
`semantic_search` alone is 6,442 s, which is more than the whole arm 03 MCP total of 3,382 s.

The MCP tools are also slower in arm 04 than in arm 03 on a smaller call count. Read that with
care: the arms ran on different days against the same caches, and in arm 04 the MCP calls compete
with concurrent semantic work inside the same Bifrost process.

Startup lag before the first assistant turn is not an NLP cost. Arm 03 sums 2,303 s over 14 tasks
and arm 04 sums 2,377 s, with the same Firefox and GCC tasks at the top of both.

Per tool, arm 03 to arm 04: `scan_usages_by_reference` 34 calls at 17.17 s median and 1,410 s total,
against 15 calls at 39.53 s median and 2,279 s total, with a 459.0 s worst call. `search_symbols`
95 calls at 3.56 s against 88 calls at 3.74 s, but total 812 s against 1,747 s. `get_summaries`
88 calls at 0.57 s against 71 calls at 1.35 s, total 355 s against 1,652 s.

### Where the semantic time goes

Each `semantic_search_rerank` record carries Bifrost's own `retrieval_timings`. This is the only
span-level attribution the arm gives, and it is decisive.

- The embedding sidecar is not the cost. Over 106 query runs it accounts for **2.5 s in total**,
  with a 21.9 ms median per run and a 0.0 s queue. The GPU is idle.
- The semantic index readiness wait is the cost. `wait_ready_ms` accounts for **1,731 s**, with a
  single worst wait of **451.4 s** on `ccx-dep-trace-264` (Firefox), 323.9 s on `ccx-domain-112`
  (Firefox) and 259.0 s on `ccx-incident-110` (Firefox). Only 14 of 106 runs wait more than a
  second, so this is a one-time hydration per container, charged in full to whichever agent call
  happens to be first.
- Context fetch is the second cost. Its per-query spans total 7,434 s with a 16.6 s median and a
  559.7 s worst. It carries no internal instrumentation, so its I/O cannot be separated from
  queueing behind concurrent MCP work.
- The utility rerank is steady and small by comparison: 106 runs, 11.9 s median, 62.4 s worst,
  1,756 s total.

The one-time readiness wait scales with index size. Firefox holds 2,287,199 chunks, GCC 681,022 and
Kubernetes 137,711, and the waits order the same way.

### Timeout triage

| Task | agent s | startup s | trace span s | tail gap s | MCP call s | MCP union s | semantic batch s | last batch finished |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :--: |
| ccx-domain-112 | 1,802 | 368 | 1,298 | 136 | 1,112 | 441 | 932 | no |
| ccx-incident-125 | 1,802 | 299 | 1,178 | 325 | 813 | 442 | 1,026 | no |
| ccx-dep-trace-264 | 1,802 | 181 | 1,514 | 107 | 810 | 497 | 1,001 | yes |

None of the three is an agent loop. Main-model time is 35.2 s, 32.8 s and 149.4 s. Turn counts are
8, 10 and 22. Each task made a small number of Bifrost calls and spent its budget waiting.

`ccx-domain-112` (Firefox). Two completed semantic batches took 589.1 s and 342.9 s. A third batch
started at trace second 1,225 and never returned; all three of its queries stalled in
`context_fetch`. The task made **no** `scan_usages_by_reference` call at all. Arm 03 scored the same
task 0.6875 in 748 s of agent time with 31 MCP calls and 380 s of call time.

`ccx-incident-125` (GCC). Two completed semantic batches took 761.6 s and 264.8 s. A third stalled
in `context_fetch` for the final 325 s. `scan_usages_by_reference` ran 3 times for 64.4 s total,
which is not the binding cost. Arm 03 scored the same task 0.5893 in 613 s.

`ccx-dep-trace-264` (Firefox). One semantic call took **1,000.6 s**, of which 451.4 s was the
readiness wait. The agent then finished normally: it wrote a contract-shaped answer and the tool
loop exited `stop=Completed { had_text: true }` about 107 s before the limit. The harness still
recorded `TIMEOUT` with no reward record. See the anomalies below.

**Attribution.** The timeout cluster is `semantic_search`, and inside it the one-time semantic
index readiness wait plus context fetch. It is not agent looping. It is not `scan_usages_by_reference`
and therefore not issue #1748, although #1748 remains open and arm 04 adds evidence to it: 12 of 15
calls over five seconds, a 39.53 s median and a 459.0 s worst call, against the single 245.7 s
sample the NLP smoke recorded.

### Anomalies

1. `toolCalls.byTool` cannot see `semantic_search`. It reports 0 on all 14 tasks against 46
   agent-issued calls in the traces. Any dashboard or census built on `byTool` understates NLP use
   to zero.
2. `ccx-dep-trace-264` produced a contract-shaped answer with `files`, `symbols`, `chain` and
   `text`, and its tool loop exited `Completed` about 107 s before the limit, yet the harness wrote
   `stopReason: TIMEOUT`, `timedOut: true` and no reward record, and mjolnir wrote
   `stop_reason: cancelled`. A scorable answer was discarded. This is a harness shutdown-window
   question, not a Bifrost or model question.
3. Six query runs stalled, all in `context_fetch`, all in the final batch of `ccx-domain-112` and
   `ccx-incident-125`. Two of 42 batches never completed.
4. One batch reports `failed: true` while all three of its queries produced reranks
   (`ccx-dep-trace-106`, 175.6 s). The batch flag and the per-query records disagree.
5. `ccx-dep-trace-273` has four r26 runs on one runtime and one cache. It is `invalid_output` in
   arm 03 and in the r26 NLP smoke. It is scored in the r26 symbols smoke (0.4545) and in arm 04
   (0.4000). Arm 02 is also `invalid_output`, and bare scores it 0.7348. The answer contract, not
   the toolset, is what varies. This is also why the arm 04 comparable subset is not a subset of
   the arm 03 one.
6. Bifrost timing logs reach gigabyte scale inside a task container. `anvil-stderr.txt` is 1,169 MB
   and 18.0 million lines on `ccx-incident-125`, 423 MB on `ccx-incident-032` and 326 MB on
   `ccx-dep-trace-264`. Arm 03 shows the same class at 198 MB and 211 MB. This is unmeasured
   overhead inside the timed window.

### Arm 04 artifacts

All in `/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/`:
`fourarm14-r26nlp-v1.json` and `.md`, `build-fourarm-r26nlp-report-v1.py` (it imports
`build-paired14-r26-report-v1.py` and reuses its method unchanged), `PHASE2-NLP-r26-record.md`,
`symbols-nlp-r26-final/`, `symbols-nlp-r26-smoke/`, `host-prewarm-r26-semantic/`,
`nlp-r26-sidecar/`. The v1 three-arm report and its generator are unchanged.

### Decisions now queued for the owner

1. **Scorer v2.** Unchanged from the arm 03 recommendation. Version the scorer to
   `canonical_grep_hard_v2` with tail-normalized symbol matching and rescore every arm from the
   archives in one pass, or record that the artifact stands and every published composite is a
   spelling-sensitive lower bound.
2. **Synthetic step zero.** The plan's trigger condition is not met, so this is now a choice, not a
   repair. Implementing it needs: a `semantic-synthetic` evaluation mode in Brokkbench; a query
   model configured separately from the utility reranker; harness-run queries whose results are
   injected before Luna's first turn, with the query-model turn kept out of Luna's history; query
   selection by necessity with deduplication rather than a fixed count; and the `querygen` record
   populated, which today returns None outside `semantic-synthetic`. Note one measurement side
   effect: a step zero would move the 451 s readiness wait out of the agent's visible loop and
   change what the latency comparison measures.
3. **Semantic index readiness.** Decide whether a container should be able to start with the
   semantic index already hydrated, the way the analyzer cache already is. The current behaviour
   charges up to 451 s of one-time hydration to a single agent tool call and it caused two of the
   three timeouts. This is the highest-value Bifrost follow-up from this arm.
4. **Discarded answer on `ccx-dep-trace-264`.** Decide whether to rescore it from its archived
   `task-output` as a labelled correction, or to leave the timeout record as it stands.
5. **Statistical power.** Unchanged and now stronger. Four arms inside 0.008 mean composite on 8
   tasks cannot separate the tools. A larger manifest or repeated runs is the only way to turn "no
   measured gain" into a supported claim.
