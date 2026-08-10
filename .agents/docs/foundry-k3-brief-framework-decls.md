# k3 brief: framework declaration content to unblock the demand sweep (#1871, decision #1)

This is a delegation brief for a high-token opus-class agent (k3). It is self-contained. It produces reviewable JSON candidates delivered on a pushed branch; it does not ship anything active. Read the whole brief first.

## Why this exists

Bifrost's foundry demand sweep runs a real taint policy over a corpus of Java repos and measures which unmodeled calls block real taint verdicts. The realistic run (Milestone 4.6) found that all blocked verdicts abstain at `capability_incomplete` **upstream of any taint verdict**: the framework types that anchor the taint sources and sinks — `javax.servlet.http.HttpServletRequest`, the `java.sql` statement/connection types, `java.lang.Runtime`/`ProcessBuilder` — are **unresolved external types** under source-only corpus analysis, because the corpus repos' dependency jars are not present in the analysis environment. Without those types resolved, the taint compiler cannot form a call region, so no verdict forms and nothing can be measured.

Your job: hand-author **declaration facts** for exactly those anchor types, so the analyzer can resolve them as external-indexed. The hypothesis (which the Bifrost maintainer will test by re-running the sweep with your content activated) is that resolving these types lets taint verdicts form, which unblocks the real "summaries vs engine" demand number. This is the same authoring shape as a prior foundry content task (the golden flow-through core), but the payload is *declaration facts* (a type's API surface), not flow summaries.

**Honest framing:** this is an experiment. Making the types external-indexed may be enough to form verdicts, or the taint compiler may still abstain for a deeper reason. Either outcome is a real finding — do not tune the content to force a particular result; author the true API surface.

## What a declaration fact is

A declaration fact records that an external type exists and what members it declares — its name, kind, visibility, and its methods with their signatures and return types. It does **not** record behavior/flow (that is a procedure summary, a different payload). Read the shipped IR before authoring: `crates/bifrost-analysis/src/analyzer/semantic_model/model.rs` — the `TypeFact` and `MemberFact` shapes and the `declaration_facts` payload. Read one real example end to end: the JDK fixture pack constant `JDK_21_FIXTURE_PACK` in `tests/suite_bench_policy/bifrost_policy_cli.rs` shows a `declaration_facts` payload with a `types` array (id, name, type_kind, visibility, type_parameters, hierarchy, aliases, extension_surfaces, locator) and a `members` array. Match that structure.

## Exactly which types and members to author

Author the API surface the taint source/sink policy anchors on. These are the types whose non-resolution blocks the sweep. For each, author the type plus the listed members (methods); include obvious sibling overloads and closely-related getters, but do not pad with the whole class.

Sources (return attacker-controlled data):
- `javax.servlet.http.HttpServletRequest` (interface): `getParameter(String)`, `getParameterValues(String)`, `getParameterMap()`, `getParameterNames()`, `getHeader(String)`, `getHeaders(String)`, `getHeaderNames()`, `getQueryString()`, `getCookies()`, `getRequestURI()`, `getRequestURL()`, `getPathInfo()`, `getInputStream()`, `getReader()`.
- `javax.servlet.http.Cookie` (class): `getValue()`, `getName()`.
- `java.lang.System` (class): `getenv()`, `getenv(String)`, `getProperty(String)`, `getProperty(String,String)`.

Sinks (execute / consume sensitive input):
- `java.sql.Statement` (interface): `executeQuery(String)`, `execute(String)`, `executeUpdate(String)`, `addBatch(String)`.
- `java.sql.Connection` (interface): `prepareStatement(String)`, `prepareStatement(String,int)`, `prepareCall(String)`, `createStatement()`.
- `java.sql.PreparedStatement` (interface): `execute()`, `executeQuery()`, `executeUpdate()`, `setString(int,String)`.
- `java.lang.Runtime` (class): `getRuntime()`, `exec(String)`, `exec(String[])`, `exec(String,String[])`.
- `java.lang.ProcessBuilder` (class): the `ProcessBuilder(String...)` and `ProcessBuilder(List)` constructors, `command(String...)`, `command(List)`, `start()`.

Authoring source of truth: the official Javadoc for the exact library/JDK versions. Servlet types are `javax.servlet` (Java EE / Jakarta `javax` namespace, servlet-api 4.0 era). The `java.sql`/`java.lang` types are JDK 21. Record the version you authored against in a provenance field on each pack/file.

## Output contract

Emit candidate JSON under `.agents/foundry/candidates/framework-decls/`, one file per artifact:
- `javax.servlet.http.json` — the servlet source types.
- `java.sql.json` — the JDBC sink types.
- `java.lang.json` — Runtime/ProcessBuilder/System.

Each file is a JSON object: `{ "artifact": "<jdk | jakarta.servlet:javax.servlet-api:4.0.1 | ...>", "provenance": "javadoc, <exact version>", "types": [ <TypeFact-shaped objects> ] }`, `types` sorted by name for deterministic diffs. Each type carries its `members`. Use the field names and value vocabulary of the real `TypeFact`/`MemberFact` shapes (type_kind: `class`/`interface`; visibility: `public`; method members with parameter types and return type). Where the real shape has a field you cannot fill meaningfully (e.g. `locator`), follow the JDK fixture pack's convention rather than inventing one. Plain ASCII. No timestamps.

## Rules

- Author the TRUE surface from Javadoc; do not invent members or guess signatures. A method you are unsure of, omit rather than guess.
- Interfaces vs classes matters (`HttpServletRequest`, `Statement`, `Connection`, `PreparedStatement` are interfaces; `Cookie`, `System`, `Runtime`, `ProcessBuilder` are classes) — set `type_kind` correctly.
- Deterministic: sort types and members, no timestamps.
- These are declaration facts (surface), never summaries (flow) and never sanitizers — do not add flow/transfer/sanitize content.

## Delivery (you run on a different machine)

Deliver through the shared git remote, not the filesystem.
1. Work inside a clone of `BrokkAi/bifrost`. Branch from the latest `origin/master`. You need the checkout only to read the IR types and the JDK fixture-pack example named above; you do not build anything.
2. Write your JSON files at the paths in the output contract.
3. Commit on a branch named `foundry/k3-framework-decls`. Do not touch code, do not run `cargo`, do not modify `master`.
4. Push the branch to `origin`; do not open a PR. Report the pushed branch name.

## What happens next (not your job)

The Bifrost maintainer converts your candidates into a `declaration_facts` pack, activates it over the demand-sweep corpus slice, and re-runs the sweep to test whether the framework types now resolve and taint verdicts form. Your value is the accurate, complete API surface for exactly these anchor types; the experiment and the number are downstream.
