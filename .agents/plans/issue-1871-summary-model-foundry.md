# Procedure-summary model foundry (#1871)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must remain current while implementation proceeds.

This plan follows `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

A procedure summary describes the value-flow behavior of one external procedure: which inputs (receiver, parameter N) reach which outputs (return, receiver, named heap or capture cells, exceptional return). The taint engine consumes summaries when a call target's body is unavailable, which is true for every standard-library call. Today the pipeline that consumes summaries is wired end to end (`bind_compiled_procedure_summaries` in the semantic-model runtime, `ExternalSemanticSummarySet`, `ValueFlowPlan::with_external_summaries`, proven by the Java vertical in `.agents/plans/activate-semantic-pack-taint-summaries.md`) but almost no summaries exist. A taint policy over real code must therefore either over-approximate through every stdlib call (`paranoid` mode, noisy) or fail closed (`require-model` mode, cannot conclude). Either way it cannot both terminate cleanly and stay precise.

After this change, a maintainer can run one command per language ecosystem that produces a reviewed, versioned, proof-carrying summary pack for a pinned standard-library edition, and re-run the same command when a new edition ships with human attention required only on the delta. The observable outcome: a `require-model` taint policy over a fixture that flows attacker-controlled data through a summarized stdlib API yields a finding with a witness and `Complete` completion when the pack is active, and the honest typed incomplete outcome when it is not; and a corpus sweep reports the fraction of taint verdicts that conclude cleanly, which must rise as packs land.

The factory is called the foundry in this plan. It is a pipeline of five stages per module (a module is one stdlib namespace, for example `java.lang` or Python's `os`), executed depth-first per module, with every stage artifact persisted and content-addressed so the pipeline is resumable and individual stages can be edited and re-run without redoing the others.

## Design constraints (from the 2026-08-09 design discussion)

Activation cost and retained memory are not constraints: measured JDK-scale activation is ~5.5 s cold and ~14 MB retained for declaration facts, accepted, and SQLite-backed storage work landing separately will reduce residency further. The binding constraints are precision at native and dynamic boundaries, review attention (the human review surface must be roughly a dozen decisions per stdlib edition, not a hundred), trust (every shipped claim must carry machine-checkable evidence), and maintainability (a new stdlib edition is one pin update plus a delta review).

## Architecture

Stage 0, pinned inputs. Each ecosystem has a pinned source artifact (the JVM uses Temurin `src.zip` plus `kotlin-stdlib` and `scala-library` source jars pinned by sha256 in `scripts/build-pinned-jvm-semantic-packs.sh`; Python uses the typeshed revision pinned in `semantic-packs/python/typeshed-stdlib-*.json`), the external model corpora normalized into our summary IR (CodeQL Models-as-Data first, MIT-licensed, deepest for Java and C#; Joern semantics second, Apache-2.0, as a lighter cross-check), and a demand ranking produced by running the repository's taint policies in `require-model` mode over the benchmark corpus (the FIRD clone fleet) and aggregating the typed unmodeled-call blockers by how many verdicts each blocks.

Stage 1, deterministic derivation. Our own analyzer runs over the pinned bodies at generation time and emits candidate summaries in the authored IR (`AuthoredProcedureSummary` in `crates/bifrost-analysis/src/analyzer/semantic_model/model.rs`: exact target with artifact path, symbol, receiver-ness and parameter count; transfers; effects; a completeness claim). The derivation must type its own incompleteness per entry using the boundary-status machinery: when a body bottoms out in a native method, reflection, or an unresolvable call, the entry records that typed reason. No LLM is involved in this stage.

Stage 2, corpus alignment. Translators convert CodeQL Models-as-Data rows and Joern semantics entries into the same IR. A three-way join per target classifies each entry: all sources agree (high confidence), sources disagree (a dispute record), or only one source covers it (a gap record). Where our derivation has no answer and CodeQL does, the CodeQL entry is imported directly with provenance and attribution recorded. Everywhere else the external corpora act as standing verification oracles, re-checked on every foundry run, never a one-time import.

Stage 3, LLM adjudication, minimal by construction. Routing follows stage 1's typed incompleteness: fully derivable entries never see an LLM; underivable entries covered by CodeQL are translate-and-verify, no LLM; only entries that are both underivable and uncovered enter LLM proposal, plus dispute repair. The harness hands the model the pinned source body, documentation from the source artifact, the partial derivation with its typed incompleteness, and any oracle entries for neighboring overloads. The calibration discipline is strict: the model proposes blind first; first-pass agreement against the CodeQL overlap (stratified by difficulty class: native-boundary versus pure code, arity, receiver-ness) is the recorded trust metric; only after grading does the mismatch detail return to the model as a self-correction signal, after which it either concedes and repairs or produces a refutation dossier. Bulk proposals run on a smaller model; disputes and anything touching sanitization escalate to a stronger model. The model has no shipping authority: its output must survive stage 4.

The calibration data comes from a dedicated grading pass, because production routing alone would never produce it: shipped LLM output exists only where CodeQL is absent, so the harness additionally runs the model blind over the underivable-but-CodeQL-covered stratum purely to be graded. Those proposals are never shipped (the CodeQL translation ships for that stratum); they exist to measure first-pass agreement, and they re-run at every edition so the trust metric stays current. A graded proposal that maintains disagreement with CodeQL through the correction loop becomes a refutation dossier that cross-examines the corpus entry we were about to import, so the calibration pass doubles as an audit of the imported corpus.

Stage 4, proof gates. Every entry ships only with machine-run executable evidence generated mechanically from the IR by a per-language fixture template engine, never by the proposing model. A positive fixture proves each claimed transfer carries taint with the pack active and fails with the entry absent (the fail-before control). For entries claiming `complete`, a negative fixture proves taint does not cross where the summary says it does not, which is provable under `require-model` because the summary is then the only model in play. Fixture generation scales with the corpus because it is a deterministic function of the record.

Stage 5, assembly and audit. Packs build through the existing pinned-spec pipeline in `crates/bifrost-semantic-packs` (license, provenance, semantic hashes, rejects reporting, measurements). Two corpus-level audits close the loop: the demand sweep re-runs and reports the percentage of taint verdicts concluding cleanly (the acceptance metric), and a pack-on versus pack-off diff over the fleet lists every finding that disappears with the pack active, mechanically attributed to the suppressing entry in its trace. That diff is the only gate that can see silent false negatives, the failure mode per-entry proofs cannot.

## Trust and the human review surface

Entries default to `completeness: partial`. A partial summary can add flows but can never prove absence, so a wrong one produces at worst a visible false positive, never a silent miss; these auto-ship on ensemble agreement plus proofs. Suppression power exists only in sanitizers and `complete` promotions, so the human queue is exactly the demand-ranked promotion list: the top roughly ten entries per stdlib edition by verdicts unblocked times suppression power, each arriving with its dispute dossier and both-polarity proofs. Beyond that queue the human surface is one pilot-module report and one batch-audit verdict (a random sample of auto-shipped entries whose measured error rate validates or rejects the whole batch). Escalation to a human happens for one reason only: the strong model reviewed a CodeQL discrepancy, maintains CodeQL is wrong, and presents fixture evidence. Each such dossier is either a foundry bug to fix or an upstream CodeQL model bug worth filing.

## Execution model

Every (module, stage) artifact is a file in a foundry work directory, keyed by the hash of its inputs: the pinned source slice, the stage code version, the prompt version, and the upstream artifact hash. A re-run skips any artifact whose key is unchanged, which makes interruption resumable at no cost and makes editing one stage (for example a prompt) invalidate exactly that stage and its downstream. Modules run depth-first, all five stages to a finished module report before the next module begins, in demand order, so the pilot module a human sanity-checks is also the highest-value content. Scaling after the pilot is parallel module workers, each still depth-first. The work directory is git-committable so review and edition-over-edition diff are the same operation.

## Language calculus

CodeQL covers nine of our eleven languages (all but Scala and PHP), so calibration is available almost everywhere, with corpus depth best in Java and C# (translation bootstrap), moderate in JS/TS and Python, thin in Go, Ruby, C++ and Rust (calibration only, sampled audit carries more weight). Scala needs no separate JDK work because JVM summaries target artifact path and symbol, not a language, and the JVM realm already serves Java, Kotlin and Scala together; the residual is `scala-library` content through the same foundry. PHP receives no summary content until demand appears; `require-model` taint over PHP then reports honest typed incompleteness, which is a shipping posture, not a removal.

## Milestones

Milestone 1, the IR seam and translators. Extend or wrap the authored IR as the foundry's interchange form, write the CodeQL Models-as-Data translator and the Joern semantics translator, and the three-way join with dispute and gap records. Acceptance: translating the CodeQL Java corpus produces a count of well-formed entries, a dispute list against the Joern translation, and every translated entry round-trips through `compile_source` in `crates/bifrost-analysis/src/analyzer/semantic_model/compiler.rs` without error.

Milestone 2, deterministic derivation with typed incompleteness. Implement generation-time summary derivation over pinned JVM sources, reusing the analyzer's value-flow machinery, emitting per-entry typed incompleteness. Acceptance: for a chosen pure-Java class the derived transfers match the CodeQL translation exactly; for a native-backed class the entry records the native boundary rather than guessing.

Milestone 3, the fixture engine. A deterministic generator that, from any IR entry, emits a compiling per-language fixture pair (positive with fail-before control; negative for `complete` claims) and a runner that executes them through the production policy path. Acceptance: the generator produces passing fixtures for twenty hand-picked known-good entries and correctly fails a deliberately corrupted entry.

Milestone 4, the foundry driver and adjudication harness. The content-addressed (module, stage) store, depth-first module scheduling, resumability, and the blind-then-graded LLM loop with first-pass calibration recording. Acceptance: interrupting a module run mid-stage and re-running completes without repeating finished stages; editing a prompt version re-runs only stage 3 and later; the calibration report shows first-pass agreement stratified by difficulty class.

Milestone 5, the Java pilot. Run the foundry over the top-demand JDK modules end to end, produce the pilot module report, the promotion queue, and the batch-audit sample, and take the human decisions. Acceptance: the #1871 vertical shape passes against foundry-produced content, and the demand sweep's clean-conclusion percentage rises measurably from its pre-pack baseline.

Milestone 6, audits and scale. The pack-on/off fleet diff with suppression attribution, the measured batch error rate, then parallel module workers across the remaining JDK and the first non-JVM ecosystem (Python via typeshed pins) to prove the foundry is not Java-shaped. Acceptance: the fleet diff report exists and every disappearing finding traces to a reviewed suppressing entry; the Python run reuses the same driver with only ecosystem adapters changed.

## Progress

- [x] (2026-08-09) Design discussion with jbellis: constraints, trust model, routing, CodeQL-as-verifier calculus, and the human-review budget recorded in this plan.
- [ ] Architecture note posted to #1871 for DavidBakerEffendi's review (license and attribution posture for corpus translation, dispute-resolution policy, Joern-experience critique).
- [ ] Milestone 1 not started.

## Surprises & Discoveries

- The 2026-08-09 epic acceptance probe (#1893, closed) proved the boundary-status machinery can type a derivation's own incompleteness, which is what makes no-LLM routing decidable per entry.

## Decision Log

- Activation cost and retained memory are explicitly not design constraints (jbellis, 2026-08-09); the earlier 5.5 s JDK cold-activation flag on #1882 does not shape this plan.
- Human review is budgeted at roughly a dozen decisions per stdlib edition; the design must reach reliability through decoupled proposers and verifiers, ensemble agreement, both-polarity proofs, and the fleet false-negative diff, not through item-by-item human review (jbellis, 2026-08-09).
- CodeQL Models-as-Data is the primary external corpus (deeper than Joern; MIT license); the LLM is graded blind against the overlap first and corrected after, preserving the calibration measurement (jbellis and Fable, 2026-08-09).
- Entries default to `partial`; only demand-justified promotions to `complete` and sanitizers enter the human queue, because only suppression can fail silently (accepted 2026-08-09).
- PHP gets no summary content until demand appears; structural RQL support for PHP is unaffected (jbellis, 2026-08-09).
- Depth-first per module with content-addressed stage artifacts, in demand order (jbellis, 2026-08-09).
- The trust signal requires a dedicated blind grading pass over the underivable-but-CodeQL-covered stratum, with outputs discarded from shipping; production routing alone generates no calibration data. Spotted by jbellis after Milestone 1 dispatch (2026-08-09).

## Outcomes & Retrospective

Not started.
