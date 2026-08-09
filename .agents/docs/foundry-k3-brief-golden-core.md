# k3 brief: golden flow-through reference core (#1871 foundry, content stream 2)

This is a delegation brief for a high-token opus-class agent. It is self-contained. It produces reviewable JSON candidates; it does not ship anything. Read the whole brief before starting.

## What you are building and why

You are hand-authoring a small, high-quality reference set of **procedure summaries** for the highest-frequency Java standard-library propagators: the methods that pass taint from an input to an output. A procedure summary states which inputs of an external method reach which outputs, so a taint engine can propagate through a call whose body it cannot see.

This set has two jobs:

1. A shipped seed of trustworthy summaries for the APIs that dominate real taint verdicts.
2. The **answer key** for a batch audit of machine-derived summaries: a random sample of auto-derived entries is checked against hand-authored truth to measure the derivation's error rate.

## A calibration constraint you must honor

This golden set must NOT become the oracle that grades the pipeline's LLM proposals. The pipeline measures LLM accuracy by grading blind proposals against CodeQL; if your set (authored by an LLM) were also the grading oracle, the trust measurement would be self-referential and meaningless. Therefore: author every entry independently from concrete reasoning over the method's documented behavior, cite that reasoning, and do not consult or transcribe another model's summaries. Where you use CodeQL or Joern models as a sanity check, say so in `citations` so the entry can be excluded from any oracle role.

## The model

A summary states which **inputs** reach which **outputs**.

- Inputs: `receiver` (the object the method is called on), or `parameter` by zero-based `ordinal`.
- Outputs: `normal_return` (the returned value), `receiver` (the method mutates the object it was called on), a named heap or capture cell, or `exceptional_return`.

A method propagates taint when it copies or incorporates an input into an output. A method that reads an input only for control flow (a length check, a comparison) does not propagate and has no transfer for that input. Do not invent a transfer to be safe; an absent transfer is a real claim and the negative fixtures test it.

## The honesty rule

`completeness: "complete"` means: these transfers are the whole truth for this method - every real flow is listed, and any input-to-output pair not listed genuinely does not flow. Claim `complete` only when you are certain the method's behavior is fully captured (small, well-understood methods like `String.concat`). Use `partial` when the method may have flows you are not certain about (anything that calls into machinery you cannot fully reason about). A wrong `complete` is a silent false negative; prefer `partial` when unsure.

## Scope

Java standard library, the high-frequency propagators. Author across these families:

- String assembly: `String.concat`, `String.format`, `String.valueOf`, `String.join`, `String.substring`, `String.replace`, `String.trim`, `String.toLowerCase`/`toUpperCase`, `String.getBytes`, `new String(byte[])`.
- Builders: `StringBuilder`/`StringBuffer` `append`, `insert`, `toString`, and their constructors.
- Boxing and Optional: `Optional.of`/`ofNullable`/`get`/`orElse`, `Integer.toString`, `String.valueOf(Object)`.
- Collections: `List.add`/`get`, `Map.put`/`get`, `Arrays.asList`, `Collections.singletonList`, iterator `next`. For collections, model taint at the container granularity (an element added taints the container; an element read is tainted if the container is) and say so in the rationale; note where element-level precision is lost.
- IO wrappers that carry data: `ByteArrayOutputStream.toByteArray`/`toString`, `StringWriter.toString`, `Reader`/`Writer` copies, `Base64` (flow only; sanitization context belongs in the sanitizer brief, not here).

Do not model sources or sinks; those are policy-level, not flow-through summaries. Do not model sanitization context; that is the separate sanitizer knowledge base.

## Output contract

Emit one JSON file per family under `.agents/foundry/candidates/golden/<family>.json`, each a JSON array sorted by `symbol`. Each object:

    {
      "target": {
        "path": "<jvm source path, e.g. java.base/java/lang/String.java>",
        "symbol": "<fully.qualified.Method, e.g. java.lang.String.concat>",
        "has_receiver": <bool>,
        "parameter_count": <int>
      },
      "completeness": "complete" | "partial",
      "transfers": [
        {
          "input": {"kind": "receiver"} | {"kind": "parameter", "ordinal": <N>},
          "exit_kind": "normal" | "exceptional",
          "output": {"kind": "normal_return"} | {"kind": "receiver"} |
                    {"kind": "capture", "location": "<name>"} |
                    {"kind": "heap", "location": "<name>"} |
                    {"kind": "exceptional_return"}
        }
      ],
      "rationale": "<2 to 4 sentences: which flows exist and why, and for a complete claim, why the unlisted pairs do not flow>",
      "provenance": "hand-authored from javadoc and JDK source reasoning",
      "confidence": "high" | "medium" | "low",
      "citations": "<javadoc, JLS, or source reasoning; note any CodeQL/Joern sanity check used>"
    }

Overloads are distinct targets: `StringBuilder.append(String)` and `StringBuilder.append(char[])` are separate entries with their own `parameter_count`. A mutating builder method has both a `receiver -> receiver` transfer (the builder accumulates the input) and usually a `receiver -> normal_return` transfer (it returns itself); think through both.

## Rules

- Every `complete` entry must justify its absences in the rationale, not just its transfers.
- Deterministic output: sort arrays, no timestamps.
- Breadth of correct high-confidence entries beats volume; label uncertainty honestly.

## What happens to your output

Candidates, not shipped content. Each entry will be proof-gated by a mechanically generated fixture pair: a positive fixture flows tainted data through the method and asserts each claimed transfer fires with the summary active and does not fire with it absent; a negative fixture (for `complete` entries) asserts an unlisted input-to-output pair does not carry taint. Entries that fail their fixtures are rejected. You have no shipping authority; your value is correct judgment with rationale a reviewer can check against the fixture result.

## Delivery (you run on a different machine)

Deliver through the shared git remote, not the filesystem.

1. Work inside a clone of `BrokkAi/bifrost`. Branch from the latest `origin/master`. You need the checkout only to know the target paths and to read the IR types under `crates/bifrost-semantic-packs/src/summary_foundry/` and `crates/bifrost-analysis/src/analyzer/semantic_model/model.rs`; you do not build anything.
2. Write your JSON files at the exact repo-relative paths in the output contract: `.agents/foundry/candidates/golden/<family>.json`.
3. Commit them on a branch named `foundry/k3-golden`. Do not touch any code, do not run `cargo`, do not modify `master`.
4. Push the branch to `origin`. Do not open a pull request and do not push to `master`. Report the pushed branch name back; the candidates are proof-gated and merged from the maintainer side.

If the machine lacks push access to `origin`, fall back to producing the files as a single archive to hand off, but a pushed branch is strongly preferred because it carries provenance and diffs cleanly.
