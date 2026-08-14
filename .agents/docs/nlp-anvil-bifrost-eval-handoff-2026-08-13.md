# NLP Anvil and Bifrost evaluation handoff

Date: 2026-08-13

This note records the completed CodeScale NLP investigation. It is self-contained. Continue from this file in a new session.

## Goal and evaluation design

The original question was why the new Anvil-basic arm outperformed the new NLP arm. "Bare" means standard Anvil tools. It does not mean grep only.

The follow-up experiment separated the optional Bifrost tools:

- `bare-semantic`: standard Anvil tools plus only Bifrost `semantic_search`.
- `bare-symbols`: standard Anvil tools plus only Bifrost symbol tools.

The runs used the same 16 CodeScale tasks as r27. They used AWS Bedrock in `us-east-2`, model `openai.gpt-5.6-luna`, and reasoning effort `max`. Concurrency was 10. The task limit was 1,800 seconds.

## Main findings

The original bare lead was mostly a scorer and answer-shape effect. Bifrost returned qualified symbol names. The exact scorer expected short names. Tail normalization removed approximately 73 percent of the aggregate bare lead. On the corrected valid set, normalized scoring changed the bare lead into a small NLP lead.

Semantic search does provide useful conceptual signal. For example, it linked the Kafka task's nonexistent annotation wording to the real `ConfigDef` metadata and renderer design. However, it did not prove unique file discovery. Standard Anvil searches had already shown the central files.

The clearest semantic-search failure mode is breadth. Broad queries with `k=20` return many valid but indirect candidates. The model can treat these candidates as the requested implementation set. This reduces file precision.

The clearest symbol-tool failure was `ccx-domain-156`. Usage results showed every generated resource adapter calling `InformerFor`. The model then included 124 generated adapters. Structural reachability confused generated consumers with the shared implementation boundary.

## Five-whys result for the 10-to-208 file expansion

The first enriched `domain-156` trace expanded from 10 files to 208 files.

1. Why did the model include 208 files? It treated all generated informer adapters as parts of the requested implementation.
2. Why did it treat them as implementation files? Semantic and symbol evidence showed many adapters with the same factory relationship.
3. Why did that evidence dominate? The query asked for generated informer registration and used a large result count.
4. Why did the final answer become an inventory? Anvil context compaction removed the exact task and answer schema in that run.
5. Why could compaction remove them? Anvil protected only the system prefix. It summarized the active user message with the tool history.

The latest fixed run corrects item 4. It does not correct items 1 through 3. With the exact task retained, `domain-156` still returned 245 files. Of these, 194 were generated informer files. Thus, context loss caused contract damage, but it did not cause the breadth error.

## Bifrost root causes and fixes

### Hidden context enrichment

Anvil's semantic reranker calls Bifrost context tools after `semantic_search`. These tools can remain hidden from the model tool catalog. Bifrost now permits calls to hidden tools from trusted clients.

Commit: `042da899 Allow NLP clients to call hidden context tools`

### Slash-bearing canonical names

Go canonical symbol names include module or import paths with slashes. The summary lookup treated these names incorrectly. It could not map semantic hits back to declarations. Anvil then displayed paths and ranges with `signature unavailable`.

Bifrost now resolves slash-bearing canonical symbols through the structured analyzer lookup.

Commit: `cdb9ad78 Resolve slash-bearing canonical symbols in summaries`

This problem was not a general Go parser limit. In the final arm, 457 of 465 selected Go results had signatures. No semantic rerank request failed.

### Stale analyzer rows

Old analyzer rows did not contain the new signature metadata. Bifrost now invalidates those rows when the metadata format changes.

Commit: `8c698e5d Invalidate analyzer rows after signature metadata change`

### Error propagation

Anvil no longer treats Bifrost context-fetch errors as empty context. A context-enrichment error now fails the `semantic_search` call.

Anvil commit: `b7a5b8a Make semantic-search context enrichment reliable`

### Evidence ordering

Anvil presents semantic evidence from strongest to weakest.

Anvil commit: `14f81b2 Order semantic evidence by strength`

This ordering is correct for general use. It does not prevent a repeated generated role from occupying many result positions.

## Anvil context compaction defect and fix

Trace inspection found a more important Anvil defect. During long active turns, Anvil compacted all messages after the canonical prefix. This set included the current user task. The generated state snapshot sometimes omitted the exact task and output contract.

The failed traces contained direct model statements such as:

- `retained context has investigation results but not original JSON schema`
- `No authoritative JSON schema was present`

The fix pins the active user message outside the generated state snapshot. Normal and Asgard tool loops use the same retention rule. A behavior test proves that the exact user contract remains in history and never enters the compactor prompt.

Anvil commit: `25281b6 Preserve active user requests during compaction`

This commit is pushed to `origin/bifrost-multi-workspace`.

Validation:

- All 23 context-manager tests passed.
- `cargo fmt --check` passed.
- `cargo clippy --all-targets -- -D warnings` passed.
- The complete suite passed 1,235 tests.
- Eight unrelated tests failed. Six could not fork helper processes. One found a newer Bifrost cache schema. One had an old model-effort expectation.

The final live run confirmed the fix. Every final model request in all 16 traces still contained `TASK_OUTPUT=/workspace/answer.json` and the published contract.

## Answer validation

The benchmark now validates `/workspace/answer.json` after writes. It can request a repair turn for invalid JSON or a contract error. The Python hook supports Python 3.10 task images.

Brokkbench commits:

- `365995569d5 Validate CodeScale answer artifacts during agent runs`
- `caa7f8db930 Run answer validator on Python 3.10 task images`

The latest full arm had 15 scorable answers and one timeout. It had no malformed completed answer.

## Duplicate semantic results

There were no exact duplicate symbols inside any one query result in the inspected arms. Cross-query repeats are intentional because separate queries can express different intents.

The phrase "equivalent generated results" means unique generated files with the same adapter role. It does not mean duplicate result records. This distinction matters for `domain-156`.

## Removed recommendation

One earlier recommendation proposed an eval-specific tool-description warning that semantic results are only candidates. That recommendation was removed. No code was removed.

The warning would tailor stock Anvil and Bifrost to this closed-world evaluation. A general production improvement is acceptable. An eval-specific instruction is not.

## Arm results

The completed fixed-signature arm before the compaction fix produced:

| Arm | Scorable | Timeouts | Mean over all 16 | Cost |
|---|---:|---:|---:|---:|
| Historical r27 bare | 14/16 | 0 | 0.3438 | $0.95 |
| Names-only semantic | 13/15 | 1 | 0.5277 | $1.02 |
| Old enriched semantic | 12/16 | 1 | 0.3515 | $1.87 |
| Fixed-signature enriched semantic | 16/16 | 0 | 0.4519 | $1.92 |

The new arm with exact active-task retention produced:

- 16 scheduled tasks.
- 15 scorable answers.
- One timeout: `ccx-incident-149`.
- Mean over all 16: 0.47725625.
- Mean over 15 scorable tasks: 0.50907333.
- Total cost: $2.0296433432.
- Two scores at or above 0.8.

Per-task scores:

| Task | Previous fixed-signature | New compaction fix |
|---|---:|---:|
| `ccx-crossorg-217` | 0.5833 | 0.5333 |
| `ccx-crossorg-222` | 0.2456 | 0.5714 |
| `ccx-dep-trace-173` | 0.0000 | 0.0000 |
| `ccx-dep-trace-254` | 0.2632 | 0.5504 |
| `ccx-dep-trace-258` | 0.8144 | 0.8516 |
| `ccx-dep-trace-264` | 0.6554 | 0.5312 |
| `ccx-domain-137` | 0.9388 | 0.8706 |
| `ccx-domain-140` | 0.4615 | 0.4615 |
| `ccx-domain-155` | 0.4825 | 0.5833 |
| `ccx-domain-156` | 0.0536 | 0.3428 |
| `ccx-incident-032` | 0.6458 | 0.6111 |
| `ccx-incident-108` | 0.6944 | 0.6414 |
| `ccx-incident-144` | 0.5000 | 0.5000 |
| `ccx-incident-148` | 0.4625 | 0.5875 |
| `ccx-incident-149` | 0.4286 | timeout |
| `ccx-platform-241` | 0.0000 | 0.0000 |

The compaction fix recovered three major losses:

- `crossorg-222`: 0.2456 to 0.5714.
- `dep-trace-254`: 0.2632 to 0.5504.
- `domain-156`: 0.0536 to 0.3428.

The result still trails the names-only semantic mean by approximately 0.0504. Richer context improves actionability, but it also increases breadth, latency, and answer-selection risk.

## Artifact locations

Campaign root:

`/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/r27-deconfuser-signatures`

Fixed-signature arm before the compaction fix:

`/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/r27-deconfuser-signatures/bare-semantic`

Final arm with the Anvil compaction fix:

`/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/r27-deconfuser-signatures/bare-semantic-compaction`

Extracted traces from the prior arm:

`/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/r27-deconfuser-signatures/inspect`

Runtime archive used for the final arm:

`/mnt/containers/code_isnt_memory/codescale-grep-hard-cleanup-20260805/r27-deconfuser-signatures/runtime/runtime.tgz`

Runtime SHA-256 after the Anvil compaction rebuild:

`4f896df5c933840943f05e745a3e04cef8916d42c5c6abbbc6f87ab9d8380351`

The embedding sidecar used logical GPU 2:

`GPU-13db0817-4937-36dc-3061-d51b47799ce9`

The sidecar was stopped after the run.

## Repository state and push status

Anvil worktree:

`/mnt/optane/anvil-bifrost-multi-workspace`

Branch `bifrost-multi-workspace` is clean and pushed through commit `25281b6`.

Bifrost worktree:

`/mnt/optane/bifrost-nlp`

Current branch: `bifrost-nlp-ft`.

The branch contains these local commits above `origin/master`:

- `042da899 Allow NLP clients to call hidden context tools`
- `cdb9ad78 Resolve slash-bearing canonical symbols in summaries`
- `8c698e5d Invalidate analyzer rows after signature metadata change`
- `9ebbf44e Record paired r27 NLP trace forensics`
- `f70d12dd Update r27 forensics after scorer repairs`
- Upstream merge commits `2d1698ad` and `d0d4bb9d`

The untracked files `nlp-claude-handoff.md` and `nlp-eval-claude-session.md` belong to the user. Do not delete or stage them without review.

Brokkbench worktree:

`/home/jonathan/Projects/brokkbench`

Commit `caa7f8db930` is local on `master`. The worktree contains many unrelated user changes. Stage only task-owned files.

Direct pushes to default branches were blocked by the safety system. Do not retry without exact informed approval. The required approval scope is:

- Push the current Bifrost history to `origin/master`, including three fixes, two trace documents, and upstream merge commits.
- Push Brokkbench commit `caa7f8db930` to `origin/master`.

## Recommended next work

First, analyze breadth without changing tool descriptions. Use the final `domain-156` trace. Separate these classes:

- Shared implementation files.
- Generated adapters.
- Production callers.
- Tests and examples.

Measure which result ranks and which query terms introduced each class. The final trace is useful because it retained the exact task throughout.

Second, test a general retrieval diversity rule. Do not add an eval-specific warning. A suitable production change would prevent one repeated structural role from filling most of `k`. Preserve strong results, but limit near-identical generated adapters by role or declaration family. This needs a general behavior test across at least two repositories.

Third, profile `incident-149`. Both old semantic modes timed out, and the final arm also timed out. Determine whether semantic rerank latency, repeated searches, or model-loop behavior dominates. Report index readiness wait separately from retrieval execution, as required by the Bifrost design.

Fourth, decide whether to keep enriched source context by default. The evidence now shows this tradeoff:

- It makes semantic results actionable.
- It fixes the names-only payload defect.
- It increases token volume and can encourage scope expansion.
- It costs approximately twice the bare arm.

Do not interpret the current mean as proof that semantic search lacks value. The exact scorer, stochastic answer selection, closed-world precision, and one timeout still have large effects.
