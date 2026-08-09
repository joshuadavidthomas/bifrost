# k3 brief: Java sanitizer knowledge base (#1871 foundry, content stream 1)

This is a delegation brief for a high-token opus-class agent. It is self-contained. It produces reviewable JSON candidates; it does not ship anything. Read the whole brief before starting.

## What you are building and why

You are building a knowledge base of taint **sanitizers** for a static-analysis taint engine (Bifrost, this repository). A sanitizer is a procedure that receives data and returns or stores a value that is safe **for a specific context** - for example `java.net.URLEncoder.encode` makes data safe to place in a URL, but not safe for SQL, shell, or HTML. This context judgment cannot be derived from the method body by any static analysis; a body only shows character shuffling, not which attack grammar it defeats. Supplying that judgment, with reasons, is the entire point of this task.

The deep design reason you exist for this and a deterministic deriver does not: a machine can derive that data flows from a parameter to the return value, but only judgment can state that the returned value is neutralized for HTML and still dangerous for SQL. Sanitizers are the one part of the taint model that is inherently a claim about attack grammars.

## The honesty rule (most important)

Wrong-but-confident is strictly worse than absent. Bifrost's whole product promise is that it never reports a false clean. If you are unsure whether a method fully neutralizes a context, mark it `partial` and explain the gap; never assert `complete`. A missing entry is recovered later; a wrong `complete` sanitizer is a silent false negative that hides a real vulnerability. When in doubt, downgrade confidence and completeness and write the reason.

## Scope

Java only. Cover, in this order:

1. JDK neutralizers and encoders: `java.net.URLEncoder`/`URLDecoder`, `java.util.Base64` encoders, `java.util.regex.Pattern.quote`, numeric coercions that neutralize most injection (`Integer.parseInt`, `Long.parseLong`, `Double.parseDouble`), and any other JDK method whose output is safe for a named context.
2. The canonical library sanitizers, because in practice most sanitizers live in libraries rather than the JDK: OWASP java-encoder (`org.owasp.encoder.Encode.*`), OWASP ESAPI, Spring (`org.springframework.web.util.HtmlUtils`, `UriUtils`), Apache Commons Text (`StringEscapeUtils`), Google Guava (`HtmlEscapers`, `UrlEscapers`). Tag each with its owning artifact coordinate.

Do not include taint sources or sinks. Only neutralizers and encoders.

## Output contract

Emit one JSON file per owning artifact under `.agents/foundry/candidates/sanitizers/<artifact-slug>.json`, each a JSON array of objects sorted by `symbol` for deterministic diffs. Each object:

    {
      "target": {
        "path": "<jvm source path, e.g. java.base/java/net/URLEncoder.java, or artifact-relative path>",
        "symbol": "<fully.qualified.Method, e.g. java.net.URLEncoder.encode>",
        "has_receiver": <bool>,
        "parameter_count": <int>
      },
      "artifact": "<jdk | maven coordinate, e.g. org.owasp.encoder:encoder:1.2.3>",
      "sanitized_input": {"kind": "parameter", "ordinal": <N>} | {"kind": "receiver"},
      "output": {"kind": "normal_return"} | {"kind": "receiver"},
      "neutralizes": [ <one or more context tokens> ],
      "does_not_neutralize": [ <context tokens this method is UNSAFE for> ],
      "completeness": "complete" | "partial",
      "rationale": "<2 to 4 sentences: the mechanism, and why it is safe for the neutralized contexts and unsafe for the others>",
      "confidence": "high" | "medium" | "low",
      "citations": "<javadoc, spec, RFC, or source reasoning you relied on>"
    }

The context vocabulary is fixed. Use exactly these tokens, invent none:

    html  html_attr  js  css  url  sql  shell  path  ldap  xpath  xml  log

## Rules

- `does_not_neutralize` is mandatory and is the highest-value field. Most real-world injection bugs are a sanitizer applied for the wrong context (HTML-escaping data that lands in a SQL string). List every context in the vocabulary that this method does not make safe.
- `completeness: "complete"` only if the method neutralizes the named context for all inputs. If it has documented gaps (an encoder that misses an edge case, an escaper that assumes a specific quoting style), use `partial` and name the gap in the rationale.
- Prefer a smaller set of well-reasoned high-confidence entries over a large set of guesses. A `low` confidence entry is acceptable when labeled and explained; an unlabeled guess is not.
- Deterministic output: sort arrays, do not embed timestamps, so two runs diff cleanly.

## What happens to your output

These are candidates, not shipped content. Each entry will be:

1. Proof-gated: a mechanically generated fixture flows tainted data through the method and asserts the finding is neutralized for the named context, and is not neutralized for a context in `does_not_neutralize`.
2. Cross-checked against CodeQL's sanitizer models where they overlap.

You have no shipping authority. Your value is the context judgment and the rationale, which survive review; the envelope may be re-mapped into the final overlay schema by a downstream mechanical step. Write the reasoning as if the reviewer will read only your rationale and the fixture result, because they will.
