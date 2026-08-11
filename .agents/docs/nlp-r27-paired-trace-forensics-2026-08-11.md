# NLP r27 paired trace forensics

Date: 2026-08-11

This note compares each r27 bare win with its same-task NLP loss.

The source report is `r27/report-v1.json` in the CodeScale grep-hard campaign.
The source traces are the archived `anvil-trace.jsonl` files for each arm.

## Result

The official bare lead is mainly a scorer artifact.

The official 11-task paired means are 0.3634 bare and 0.3139 NLP.
Bare leads by 0.0495.

NLP has a 0.0163 advantage from file scores.
Exact symbol scoring gives bare a 0.0658 advantage.

Bifrost returns qualified names such as `DeploymentController.syncDeployment`.
The oracle often stores short names such as `syncDeployment`.
The old scorer required exact symbol text.

Same-file tail normalization changes the means to 0.4559 bare and 0.4753 NLP.
NLP then leads by 0.0194.

The fixed scorer keeps the repository and path strict.
It normalizes only the final symbol component.
Brokkbench commit `384754071f9` contains this correction and its tests.

## Same-task pairs

| Task | Official bare | Official NLP | Fixed bare | Fixed NLP | Main cause |
| --- | ---: | ---: | ---: | ---: | --- |
| `ccx-incident-144` | 0.5625 | 0.3250 | 0.5625 | 0.4500 | Name shape, then broad localization |
| `ccx-domain-155` | 0.6491 | 0.4833 | 0.6491 | 0.6500 | Name shape |
| `ccx-dep-trace-258` | 0.8516 | 0.4595 | 0.8516 | 0.8441 | Name shape, then early stop |
| `ccx-domain-137` | 0.5297 | 0.4286 | 0.9388 | 0.8149 | Missing symbols and one extra file |

### `ccx-dep-trace-258`

Both arms found almost the same files and symbols.
Bare matched 11 of 13 oracle symbols.
NLP matched one symbol under exact scoring.
NLP matched 11 symbols after name normalization.

The semantic trace found the Deployment and ReplicaSet controller chain.
It also found the Pod storage entry point.
The final answer copied Bifrost receiver-qualified names.

Bare continued into generic REST creation and Pod strategy.
NLP stopped before those two downstream files.
This leaves a fixed score difference of only 0.0075.

### `ccx-domain-155`

NLP found seven correct files.
Bare found six correct files.
NLP added the correct generic interface file.

NLP copied receiver-qualified dispatcher names.
Bare copied short source names.
The fixed score gives NLP a 0.0009 lead.

### `ccx-incident-144`

The first semantic results found the correct eviction symbols.
Later calls expanded into two platform observation helpers.
Those helpers are false-positive files for this oracle.

NLP also missed `signalToNodeCondition`.
The fixed bare lead is 0.1125.
This is a small real localization loss.

### `ccx-domain-137`

NLP found the six core Android rendering files.
It also added the adjacent `RenderNode.java` file.

NLP omitted five oracle symbols that bare found.
These include `View.onMeasure`, `View.onLayout`, and `View.onDraw`.
This is the clearest real localization loss.

## Other confounds

The NLP arm does not add semantic search to the bare tool set.
It replaces grep, file reads, directory lists, and shell commands.

This difference affects exact text search and answer validation.
Bare can reopen and parse `answer.json`.
The NLP arm cannot read that artifact through analyzer tools.

Bare produced 14 scorable answers from 16 tasks.
NLP produced 11 scorable answers.

Repository inference recovers one NLP answer.
Two additional NLP answers use the wrong object keys.
Those failures are not retrieval failures.

Latency is also not isolated to semantic search.
Across seven bare-win pairs, semantic calls used 624 seconds.
Usage scans used 1,582 seconds.
No pair timed out.

Three usage scans each took approximately 250 seconds.
These calls delayed good and bad answers alike.

## Decisions

Use qualified-name normalization in the canonical scorer.
Do not use the old official symbol scores for product conclusions.

Keep `source_roots` correction separate.
It fixes repository attribution, not symbol identity.

Treat bare versus NLP as a product-toolset comparison.
Treat symbols versus symbols-plus-NLP as the semantic-search ablation.

## Next work

1. Add source-root repository inference to the official scorer.
2. Give each arm the same answer validation capability.
3. Investigate the 250-second usage scans separately.
4. Review semantic over-expansion on `domain-137` and `incident-144`.
5. Select future tasks with the current bare runtime and fixed scorer.

