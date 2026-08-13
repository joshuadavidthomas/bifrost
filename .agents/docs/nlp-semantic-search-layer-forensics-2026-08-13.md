# Why semantic_search does not help: layer forensics

Date: 2026-08-13

This note answers one question. The CodeScale arms show no end-to-end gain from
`semantic_search`. Where is the value lost?

The analysis reads only stored traces. It ran no new arm.

Source data: the 16 traces of the final arm
`r27-deconfuser-signatures/bare-semantic-compaction`.

Scripts and extracted data:
`r27-deconfuser-signatures/analysis-20260813/`

- `extract.py` pulls each `semantic_search` call and its result from
  `anvil-trace.jsonl`, exactly as the main model received it.
- `oracle.py` rebuilds the gold file and symbol sets with the benchmark scorer.
- `funnel.py` compares gold files against semantic results, other tool output,
  and the final answer.
- `rerank.py` aggregates the `semantic_search_rerank` telemetry.
- `reranker_damage.py` compares Bifrost's candidate symbols against the symbols
  that reached the model.
- `latency.py` measures the time the agent blocked inside `semantic_search`.

## Summary

Three defects account for the result. Only one of them is in Anvil.

1. The semantic index holds functions only. It cannot return a type, class,
   struct, interface, or constant. Most gold symbols are types.
2. Only the vector leg works. BM25 returned 0 results in all 109 queries. The
   co-edit leg returned 1,370 candidates and none of them ever reached the model.
3. Anvil gives the whole result set to a small model, which selects about 20
   percent of it. The main model sees names and signatures, never source.

Defects 1 and 2 are Bifrost. Defect 3 is Anvil.

## The pipeline as measured

For each `semantic_search` call with three queries:

| Stage | Volume |
|---|---|
| Bifrost candidates returned | 7,254 over 109 queries |
| Context that Bifrost fetched for them | 7.08 MB |
| Tokens sent to the reranker | 4,829,668 |
| Reranker output tokens | 24,065 |
| Results the main model received | 1,152 |

The reranker is `deepseek::deepseek-v4-flash`. It read 4.83 million tokens. The
main agent, `bedrock::openai.gpt-5.6-luna` at `max` effort, read 2.13 million
tokens for the whole task. The small selector model reads 2.3 times more than
the agent it serves.

## Defect 1: the index holds functions only

Every result that reached the model in all 16 tasks resolved to a function:

- 1,370 `function` context lines.
- 6 `field` context lines.
- 0 type, class, struct, interface, enum, or constant lines.

This is intended behavior, not a data problem. `crates/bifrost-nlp/src/chunker.rs:59`
declares "Extract every named function from `file`". The walk at lines 79-98
pushes a unit into `functions` only when `unit.is_function()`. It enters a class
or module solely to reach the children. The unit test at line 215 is named
`extracts_only_ordered_functions_with_structured_class_names`.

The benchmark asks for types. Examples of gold symbols that the index can never
return: `ListWatch`, `ListerWatcher`, `DeltaFIFO`, `Store`, `RateLimitingInterface`,
`SharedInformerFactory`, `BaseHandler`, `HardwareRenderer`.

`SharedInformerFactory` occurs 172 times in the result text, always inside
another function's signature. It is never a retrieved unit. The same holds for
`DeltaFIFO`, `ListerWatcher`, `cache.Store`, and `RateLimitingInterface`.

Gold-symbol recall of Bifrost's own candidate set, before Anvil selects anything,
is 49 percent (85 of 172). This defect explains most of that loss.

## Defect 2: two of three retrieval legs are dead

From the 109 `semantic_search_rerank` records:

| Leg | Candidates | Results shown to the model |
|---|---:|---:|
| vector | 5,884 | 1,152 |
| bm25 | 0 | 0 |
| co-edit | 1,370 | 0 |

BM25 realized 0 in every query. The requested leg counts never include it.

The system is therefore pure dense retrieval. This matters because the agent
writes queries that contain the exact identifier. For `ccx-dep-trace-173` the
agent asked "DeltaFIFO queue processes watch add update delete events" and the
candidate set did not contain `DeltaFIFO`. A lexical leg would have returned it
first. Gold-symbol candidate recall for that task was 1 of 8.

The co-edit leg costs retrieval work and context fetching in every query. It has
never yet produced one result that the model saw.

## Defect 3: the Anvil layer discards the context and delegates selection

For one query in `ccx-domain-156` the telemetry reads:

    realized_dedup_candidates: 120
    context_bytes:             99771
    reranker_usage.input_tokens: 32986
    reranker_selected_count:   20
    utility_model:             deepseek::deepseek-v4-flash

Anvil fetches 99.7 KB of source context, gives all of it to DeepSeek-flash, and
keeps DeepSeek's 20 picks. The main model then receives four lines per pick:

    1. symbol django.core.handlers.base.BaseHandler.load_middleware [vector]
       django/core/handlers/base.py:27-104
       function django.core.handlers.base.BaseHandler.load_middleware at django/core/handlers/base.py:27-104
          def load_middleware(self, is_async=False): ...

Lines 1 and 3 state the same symbol and the same range twice. Line 4 is a
signature with the body removed. No source reaches the main model.

This is what the arm records as "enriched context". It costs about twice the
bare arm and adds a signature line. The names-only arm scored higher
(0.5277 against 0.4519), which is consistent with paying for a redundant line.

Anvil's selection damage by itself is small. The reranker keeps 86 percent of
the gold symbols that Bifrost found (73 of 85). It dropped 12. Full list:

    crossorg-222  AbstractConfig
    dep-trace-258 Create, CreatePods
    dep-trace-264 BytecodeEmitter
    domain-137    HardwareRenderer, postFrameCallback
    domain-155    ToStatusErr
    domain-156    SharedInformerFactory
    incident-108  EVT
    incident-148  assemble_extension_candidates_for_traits_in_scope
    incident-149  normalize
    platform-241  Add

The selection stage is not the main leak. The candidate set is.

Go signature rendering also truncates at the first brace. A Go channel parameter
destroys the rest of the line:

    func (f *sharedInformerFactory) Start(stopCh <-chan struct { ... }

The return type and the remaining parameters are gone. This affects eight
signatures in the arm.

## Effect on the answer

Gold file coverage across all 16 tasks, out of 150 gold files:

| Where the gold file appeared | Count | Share |
|---|---:|---:|
| Named in grep, shell, or list output | 145 | 97% |
| Opened with `read_file` | 109 | 73% |
| Shown in a semantic result | 98 | 65% |
| Present in the final answer | 86 | 57% |

Gold files that semantic search found and no other tool found: **0**. Not one,
in any task.

Semantic search adds no unique discovery, and its file coverage is below what
the ordinary tools had already shown.

## Latency

The agent blocked inside `semantic_search` for 5,404 seconds of 9,611 seconds of
total agent time, which is 56 percent. Index readiness accounts for 743 seconds
of that, so 86 percent of the wait is retrieval, context fetch, and the
reranker call. This is not the readiness design; it is the work itself.

`ccx-incident-149` timed out at 1,802 seconds. It spent 854 seconds inside
`semantic_search`.

Note: two tasks report block time above their recorded task time, because
batches overlap. Treat the per-task figures for `dep-trace-254` and `domain-140`
as approximate. The aggregate share is sound.

## Correction to the earlier five-whys

The 2026-08-13 handoff attributed the `ccx-domain-156` file expansion to
semantic and symbol evidence. The final arm does not support that.

This arm offers no symbol tools. Of the 245 files in the answer, 234 were not
gold, and semantic search had shown only 17 of those 234. The model produced the
generated-adapter inventory with its own shell command:

    python3 - <<'PY'
    root=Path('staging/src/k8s.io/client-go/informers')
    for p in sorted(root.rglob('*.go')):
        if 'InformerFor(' in p.read_text():
            ...

The breadth error is the model enumerating with `rglob`. It is not retrieval
breadth. A diversity rule on `k` would not have prevented it.

## Two zero scores are a scorer artifact

`ccx-dep-trace-173` and `ccx-platform-241` both scored 0.0. Both answers are
substantially right.

`dep-trace-173` listed all 6 gold files. It wrote repo `kubernetes/kubernetes`
with path `staging/src/k8s.io/client-go/tools/cache/listwatch.go`. The oracle
holds repo `sg-evals/client-go--v0.32.0` with path `tools/cache/listwatch.go`.
The scorer cannot attribute the vendored workspace, so file recall is 0.0.

Its true file F1 is 1.0. The recorded mean understates the arm.

## Recommended order of work

1. Index type-level declarations. Extend `extract_file_chunks` past
   `is_function()`. This is the largest single cause and it is inside Bifrost.
2. Find out why BM25 realizes 0 and turn the lexical leg on. Dense-only
   retrieval fails on exact identifiers, which is what agents actually type.
3. Decide the co-edit leg's future. It costs work in every query and has
   produced no visible result.
4. Give the main model the source that Bifrost already fetched, or stop fetching
   it. Today a 7 MB fetch is compressed to a name list by a small model.
5. Fix the Go signature truncation at the first brace.

Do not run another arm before item 1 and item 2. The end-to-end score cannot
move while the index cannot return the symbols the tasks ask for.
