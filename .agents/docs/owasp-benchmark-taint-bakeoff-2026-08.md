# OWASP BenchmarkJava taint bakeoff (2026-08)

This note records a measurement. It scores Bifrost's `require-model` taint
analysis against the OWASP BenchmarkJava corpus. The result is honest and
unflattering: the shipped taint policy produces no source-to-sink verdicts on
the Benchmark. It abstains. The note explains what was built, the exact result,
why it happens, and what to fix next.

## What was built

- A reproducible scorer in the facade: `src/owasp_benchmark.rs` (pure scoring
  core plus a live runner) and `src/bin/bifrost_owasp_benchmark.rs` (a
  `release-tooling`-gated driver). The scorer lives in the facade for the same
  reason `summary_foundry_demand` does: it needs both the semantic-model
  machinery from `brokk-bifrost-analysis` and the policy evaluator from
  `brokk-bifrost-policy`, and only the facade sees both.
- The pinned ESAPI sanitizer pack:
  `semantic-packs/sanitizers/bifrost.esapi-sanitizers.json`. The driver digested
  the ESAPI jar the Maven build resolved and wrote the digest into the pack's
  shard activation.
- The committed score artifact:
  `semantic-packs/benchmarks/owasp-benchmark-java.json`.

## The Benchmark and how it was built

- Repo: `https://github.com/OWASP-Benchmark/BenchmarkJava`.
- Pinned commit: `007786f86b965a9ea8e4a7613baa5f90adbbd611` (v1.2, 2740 cases).
- Build: `mvn -B -DskipTests dependency:copy-dependencies
  -DoutputDirectory=target/dependency compile`.
- Toolchain: system OpenJDK 21.0.7 and Maven 3.6.3. The build succeeded with no
  pom changes and no sdkman switch. It compiled 4513 classes and resolved 132
  dependency jars, including `org.owasp.esapi:esapi:2.7.0.0`.
- The clone and build stay in a scratch directory outside the repo.

## Scope

The bakeoff scores only the injection/taint subset: `sqli`, `cmdi`, `ldapi`,
`pathtraver`, `xpathi`, and `xss`. That is 1572 cases (819 real, 753 fake). The
Benchmark's other categories (crypto, hash, weakrand, securecookie, trustbound)
are not source-to-sink taint problems. They are out of scope.

## ESAPI sanitizer pack

- Coordinate: `org.owasp.esapi:esapi:2.7.0.0`.
- Jar SHA-256: `2288e84a6c93a457c5215eb8028c87ebd4326a515e21545d2e02db8356d6ccff`.
- The staged pack under `semantic-packs/sanitizers/staged/` carried the package
  coordinate but no byte-level pin. Promotion wrote the jar digest into each
  shard activation and recorded it in `provenance.source`. The 13 summaries are
  unchanged. The promoted pack compiles through `compile_source`.

## The result

Every category abstained. The headline:

    category     total  real  fake | naive Youden | honest Youden | false-green
    sqli           504   272   232 |     0.00     |   undefined   |     0
    cmdi           251   126   125 |     0.00     |   undefined   |     0
    ldapi           59    27    32 |     0.00     |   undefined   |     0
    pathtraver     268   133   135 |     0.00     |   undefined   |     0
    xpathi          35    15    20 |     0.00     |   undefined   |     0
    xss            455   246   209 |     0.00     |   undefined   |     0
    overall       1572   819   753 |     0.00     |   undefined   |     0

- Findings: 0. False positives: 0. False greens: 0.
- Completion profile: 1572 of 1572 cases did not reach a per-case verdict. The
  taint run abstained at the run level for every category.
- Naive Youden J is 0.00 because no case is flagged; the naive score folds each
  abstention into the negatives, so every real case is a false negative and
  every fake case is a true negative.
- Honest Youden J is undefined because no case was decided; the honest score
  excludes abstentions, and every case abstained.

The "no false greens" claim holds in the strongest form: Bifrost never told the
user a real vulnerability was safe, because it never cleared any case. It also
never found any vulnerability. The shipped require-model taint is not yet
productive on this corpus.

## Why it abstains

The abstention is at endpoint binding and selector execution, before taint
propagation. Four causes compound, confirmed by reading the compiler
(`crates/bifrost-policy/src/taint_policy.rs`) and four representative cases
(BenchmarkTest00001, 00008, 00344, 00428):

1. Shared-region coverage. The require-model compiler requires every selected
   source and every selected sink to bind into a shared, completely-discovered
   call region. A workspace-wide name selector matches sources in files that
   hold no matching sink, so coverage fails and the whole policy compile abstains
   (`capability_incomplete`).
2. Selector result limit. At corpus scale the source selector exceeds the RQL
   100-result limit before binding (`result_limit_reached` after scanning 467
   files), which truncates the match set to a partial discovery.
3. Arity. Binding hard-fails when a matched sink call lacks the selected argument
   index. Arity-overloaded names collide: `Statement.execute(String)` is a sink,
   but `PreparedStatement.execute()` has no argument, and one no-argument match
   aborts the whole compile. There is no RQL predicate to constrain a call's
   arity.
4. Unmodeled transforms. Where binding does succeed (observed per-file for
   pathtraver and xss), the flow still crosses an unmodeled JDK transform on
   essentially every case: `java.net.URLDecoder.decode`, `java.util.List`
   `add`/`get`, `Enumeration.nextElement`, `StringBuilder`. require-model fails
   closed on an unmodeled boundary, so propagation reaches `PartialDiscovery` and
   abstains rather than passing taint through.

## What this directs

This is the same summaries-vs-engine question the foundry work feeds:

1. Make endpoint binding arity-tolerant. Skip a matched call that lacks the
   selected operand instead of aborting the whole policy.
2. Bind endpoints per completely-discovered region, or add a file/region scope to
   selectors, so workspace-wide selection does not require global co-occurrence
   and does not hit the selector result limit.
3. Ship taint-preserving procedure summaries for the common JDK string, codec,
   and collection transforms (URLDecoder.decode, StringBuilder, List, Map,
   Enumeration) so require-model can close the boundaries the Benchmark routes
   every flow through.
4. Mount external declaration packs (servlet, java.sql, ESAPI) so sanitizer
   summaries bind by exact symbol, letting ESAPI-sanitized fake cases clear
   rather than abstain.

## How to reproduce

Build the Benchmark in a scratch directory:

    cd $SCRATCH
    git clone https://github.com/OWASP-Benchmark/BenchmarkJava.git
    cd BenchmarkJava
    git checkout 007786f86b965a9ea8e4a7613baa5f90adbbd611
    mvn -B -DskipTests dependency:copy-dependencies \
        -DoutputDirectory=target/dependency compile

Promote the ESAPI pack (idempotent; already committed):

    cargo run -p brokk-bifrost --features release-tooling \
        --bin bifrost_owasp_benchmark -- promote-esapi \
        --staged semantic-packs/sanitizers/staged/bifrost.esapi-sanitizers.json \
        --jar $SCRATCH/BenchmarkJava/target/dependency/esapi-2.7.0.0.jar \
        --out semantic-packs/sanitizers/bifrost.esapi-sanitizers.json

Run the bakeoff:

    cargo run -p brokk-bifrost --features release-tooling \
        --bin bifrost_owasp_benchmark -- run \
        --benchmark $SCRATCH/BenchmarkJava \
        --packs-dir semantic-packs/sanitizers \
        --deps $SCRATCH/BenchmarkJava/target/dependency \
        --esapi-digest 2288e84a6c93a457c5215eb8028c87ebd4326a515e21545d2e02db8356d6ccff \
        --out semantic-packs/benchmarks/owasp-benchmark-java.json

Set `BIFROST_OWASP_DEBUG=1` to print each category's run-level completion and its
binding diagnostics.

## Validation

- `cargo fmt`.
- `cargo test -p brokk-bifrost --lib owasp_benchmark`: 7 hermetic scoring-core
  tests pass. They prove the naive-vs-honest math, the empty-class rates, the CSV
  parse, and the ESAPI promotion over fabricated inputs; they never run an
  analyzer.
- `cargo clippy -p brokk-bifrost --all-targets --features release-tooling
  -- -D warnings`: clean.

## Provenance

- Bifrost commit at run time: recorded in the artifact's `bifrost.commit`.
- The artifact carries no timestamp, so the same inputs write byte-identical
  bytes.
