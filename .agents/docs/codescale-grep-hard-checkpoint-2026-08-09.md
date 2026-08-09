# CodeScaleBench grep-hard checkpoint (2026-08-09)

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
